use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};

use rustc_hash::FxHashMap;

use super::{
    AstStore, AstStoreBuilder, AstStoreError, AstStoreId, AstStoreLease, BindingData, BindingLease,
    FileBindingContext, LazyBindPhase, LazyBindSession, ListKey,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAstPhase {
    Parsed,
    Binding,
    Bound,
    Failed,
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AstAccessError {
    UnknownStore(AstStoreId),
    UnpermittedOwner(AstStoreId),
    DuplicateStore(AstStoreId),
    NotParsed(FileAstPhase),
    NotBound(FileAstPhase),
    Failed,
    Retired,
    Poisoned,
    Storage(AstStoreError),
    UnknownLazyList(ListKey),
    InvalidLazyList(ListKey),
    LazyListNotParsed(ListKey),
    NotMutableStore(AstStoreId),
}

impl From<AstStoreError> for AstAccessError {
    fn from(error: AstStoreError) -> Self {
        Self::Storage(error)
    }
}

struct Registration {
    root: AstStoreId,
    owner: Weak<FileAstOwner>,
}

/// A discovery index, never a strong owner of a file or its syntax body.
#[derive(Default)]
pub struct AstDirectory {
    stores: Mutex<FxHashMap<AstStoreId, Registration>>,
}

impl AstDirectory {
    fn register(&self, store: AstStoreId, owner: &Arc<FileAstOwner>) -> Result<(), AstAccessError> {
        let mut stores = tsrs_core::runslots::blocking(|| self.stores.lock())
            .map_err(|_| AstAccessError::Poisoned)?;
        if stores.contains_key(&store) {
            return Err(AstAccessError::DuplicateStore(store));
        }
        stores.insert(store, Registration { root: owner.id, owner: Arc::downgrade(owner) });
        Ok(())
    }

    fn resolve(
        &self,
        store: AstStoreId,
    ) -> Result<Option<(AstStoreId, Weak<FileAstOwner>)>, AstAccessError> {
        let stores = tsrs_core::runslots::blocking(|| self.stores.lock())
            .map_err(|_| AstAccessError::Poisoned)?;
        Ok(stores
            .get(&store)
            .map(|registration| (registration.root, Weak::clone(&registration.owner))))
    }

    /// Call at cache-eviction boundaries; this does not traverse live syntax.
    pub fn prune_expired(&self) -> Result<(), AstAccessError> {
        let mut stores = tsrs_core::runslots::blocking(|| self.stores.lock())
            .map_err(|_| AstAccessError::Poisoned)?;
        stores.retain(|_, registration| registration.owner.strong_count() != 0);
        Ok(())
    }
}

struct FileDraft {
    ast: AstStoreBuilder,
    bindings: BindingData,
}

enum FileBody {
    Parsed(Box<FileDraft>),
    Binding,
    Bound {
        base: Arc<AstStore>,
        extensions: FxHashMap<AstStoreId, Arc<AstStore>>,
        bindings: BindingLease,
    },
    Failed,
    Retired,
}

impl FileBody {
    fn phase(&self) -> FileAstPhase {
        match self {
            Self::Parsed(_) => FileAstPhase::Parsed,
            Self::Binding => FileAstPhase::Binding,
            Self::Bound { bindings, .. } => {
                if bindings.is_failed() {
                    FileAstPhase::Failed
                } else {
                    FileAstPhase::Bound
                }
            }
            Self::Failed => FileAstPhase::Failed,
            Self::Retired => FileAstPhase::Retired,
        }
    }
}

/// One file version. Program/project caches retain this owner; text survives
/// retirement of its syntax body. The directory holds only weak registrations.
pub struct FileAstOwner {
    id: AstStoreId,
    source: Arc<str>,
    directory: Arc<AstDirectory>,
    body: Mutex<FileBody>,
    changed: Condvar,
}

impl FileAstOwner {
    pub fn parsed(
        mut builder: AstStoreBuilder,
        directory: Arc<AstDirectory>,
    ) -> Result<Arc<Self>, AstAccessError> {
        let bindings = BindingData::new(&mut builder).map_err(AstAccessError::Storage)?;
        let owner = Arc::new(Self {
            id: builder.id,
            source: Arc::clone(&builder.source),
            directory,
            body: Mutex::new(FileBody::Parsed(Box::new(FileDraft { ast: builder, bindings }))),
            changed: Condvar::new(),
        });
        owner.directory.register(owner.id, &owner)?;
        Ok(owner)
    }

    pub const fn id(&self) -> AstStoreId {
        self.id
    }

    pub fn source_text(&self) -> &str {
        &self.source
    }

    pub fn source(&self) -> &Arc<str> {
        &self.source
    }

    fn lock(&self) -> Result<MutexGuard<'_, FileBody>, AstAccessError> {
        tsrs_core::runslots::blocking(|| self.body.lock()).map_err(|_| AstAccessError::Poisoned)
    }

    pub fn phase(&self) -> Result<FileAstPhase, AstAccessError> {
        Ok(self.lock()?.phase())
    }

    /// Loading reads/finalizes an unbound file under this guard. No reference can
    /// escape the guard, and binding cannot take the builder until it is dropped.
    pub fn parsed_guard(&self) -> Result<ParsedAstGuard<'_>, AstAccessError> {
        let body = self.lock()?;
        if !matches!(*body, FileBody::Parsed(_)) {
            return Err(AstAccessError::NotParsed(body.phase()));
        }
        Ok(ParsedAstGuard { body })
    }

    /// Does not run binding or wait for another binder. Recursive binder helpers
    /// receive the returned session's builder instead of reacquiring this owner.
    pub fn try_begin_binding(self: &Arc<Self>) -> Result<BindingAttempt<'_>, AstAccessError> {
        let mut body = self.lock()?;
        match &*body {
            FileBody::Parsed(_) => {
                let FileBody::Parsed(builder) = std::mem::replace(&mut *body, FileBody::Binding)
                else {
                    unreachable!("parsed state was checked while holding the mutex");
                };
                Ok(BindingAttempt::Started(BindSession {
                    owner: self,
                    builder: Some(builder),
                    published: false,
                    failed: false,
                }))
            }
            FileBody::Binding => Ok(BindingAttempt::InProgress),
            FileBody::Bound { base, bindings, .. } => {
                if bindings.is_failed() {
                    return Err(AstAccessError::Failed);
                }
                Ok(BindingAttempt::AlreadyBound(AstStoreLease::from(Arc::clone(base))))
            }
            FileBody::Failed => Err(AstAccessError::Failed),
            FileBody::Retired => Err(AstAccessError::Retired),
        }
    }

    /// Waits only for an already-started binding session. Waiting releases the
    /// worker's run slot so the initializer can execute in a bounded worker pool.
    pub fn wait_bound(&self) -> Result<AstStoreLease, AstAccessError> {
        let mut body = self.lock()?;
        while matches!(*body, FileBody::Binding) {
            body = tsrs_core::runslots::blocking(|| self.changed.wait(body))
                .map_err(|_| AstAccessError::Poisoned)?;
        }
        bound_lease(&body, self.id)
    }

    pub fn publish_extension(
        self: &Arc<Self>,
        store: AstStore,
    ) -> Result<AstStoreLease, AstAccessError> {
        // Gate -> body is the only nested lock order. Acquire a lease under the
        // body lock, release it, then take exclusive binding authority.
        let bindings = self.bindings()?;
        let mut writer = bindings.write()?;
        let mut body = self.lock()?;
        let FileBody::Bound { extensions, .. } = &mut *body else {
            writer.complete()?;
            return Err(state_error(&body));
        };
        extensions.reserve(1);
        let lease = match writer.publish_fragment(store) {
            Ok(lease) => lease,
            Err(error) => {
                writer.complete()?;
                return Err(error);
            }
        };
        // Keep the body lock through both insertions: readers can discover the
        // registration only after the strong extension owner has been installed.
        self.directory.register(lease.0.id(), self)?;
        extensions.insert(lease.0.id(), Arc::clone(&lease.0));
        writer.complete()?;
        Ok(lease)
    }

    pub fn bindings(&self) -> Result<BindingLease, AstAccessError> {
        let body = self.lock()?;
        let FileBody::Bound { bindings, .. } = &*body else {
            return Err(state_error(&body));
        };
        if bindings.is_failed() {
            return Err(AstAccessError::Failed);
        }
        Ok(bindings.clone())
    }

    pub fn binding_context(self: &Arc<Self>) -> Result<FileBindingContext, AstAccessError> {
        Ok(FileBindingContext::new(Arc::clone(self), self.bindings()?))
    }

    /// The caller holds the file binding write gate. Keep registration and
    /// ownership installation inside one body-lock interval.
    pub(super) fn install_fragments(
        self: &Arc<Self>,
        fragments: &[AstStoreLease],
    ) -> Result<(), AstAccessError> {
        let mut body = self.lock()?;
        let FileBody::Bound { extensions, .. } = &mut *body else {
            return Err(state_error(&body));
        };
        extensions.reserve(fragments.len());
        for fragment in fragments {
            self.directory.register(fragment.0.id(), self)?;
            extensions.insert(fragment.0.id(), Arc::clone(&fragment.0));
        }
        Ok(())
    }

    /// Existing leases remain valid; later acquisitions through the entry fail.
    pub fn retire(&self) -> Result<(), AstAccessError> {
        // Use the same gate -> body order as fragment publication. Retirement
        // cannot remove the entry midway through a deferred binding commit.
        let bindings = match self.bindings() {
            Ok(bindings) => bindings,
            Err(AstAccessError::Retired) => return Ok(()),
            Err(error) => return Err(error),
        };
        let writer = bindings.write()?;
        let mut body = self.lock()?;
        match &*body {
            FileBody::Bound { .. } => {
                *body = FileBody::Retired;
                self.changed.notify_all();
                writer.complete()?;
                Ok(())
            }
            FileBody::Retired => {
                writer.complete()?;
                Ok(())
            }
            _ => {
                writer.complete()?;
                Err(state_error(&body))
            }
        }
    }

    fn lease(&self, store: AstStoreId) -> Result<AstStoreLease, AstAccessError> {
        let body = self.lock()?;
        bound_lease(&body, store)
    }
}

fn state_error(body: &FileBody) -> AstAccessError {
    match body {
        FileBody::Failed => AstAccessError::Failed,
        FileBody::Retired => AstAccessError::Retired,
        _ => AstAccessError::NotBound(body.phase()),
    }
}

fn bound_lease(body: &FileBody, store: AstStoreId) -> Result<AstStoreLease, AstAccessError> {
    let FileBody::Bound { base, extensions, bindings } = body else {
        return Err(state_error(body));
    };
    if bindings.is_failed() {
        return Err(AstAccessError::Failed);
    }
    let store = if store == base.id() {
        base
    } else {
        extensions.get(&store).ok_or(AstAccessError::UnknownStore(store))?
    };
    Ok(AstStoreLease::from(Arc::clone(store)))
}

pub struct ParsedAstGuard<'a> {
    body: MutexGuard<'a, FileBody>,
}

impl ParsedAstGuard<'_> {
    pub fn ast(&self) -> &AstStoreBuilder {
        let FileBody::Parsed(ast) = &*self.body else {
            unreachable!("parsed guard keeps the file entry locked in Parsed");
        };
        &ast.ast
    }

    pub fn ast_mut(&mut self) -> &mut AstStoreBuilder {
        let FileBody::Parsed(ast) = &mut *self.body else {
            unreachable!("parsed guard keeps the file entry locked in Parsed");
        };
        &mut ast.ast
    }
}

pub enum BindingAttempt<'a> {
    Started(BindSession<'a>),
    AlreadyBound(AstStoreLease),
    InProgress,
}

/// Initial binding has sole access to the builder, outside the file-entry lock.
/// Dropping an unfinished session marks the version Failed and wakes waiters.
#[must_use]
pub struct BindSession<'a> {
    owner: &'a Arc<FileAstOwner>,
    builder: Option<Box<FileDraft>>,
    published: bool,
    failed: bool,
}

impl BindSession<'_> {
    pub fn ast(&self) -> &AstStoreBuilder {
        &self.builder.as_deref().expect("only publish consumes a binding session's builder").ast
    }

    pub fn ast_mut(&mut self) -> &mut AstStoreBuilder {
        &mut self
            .builder
            .as_deref_mut()
            .expect("only publish consumes a binding session's builder")
            .ast
    }

    pub fn bindings(&self) -> &BindingData {
        &self.builder.as_deref().expect("binding session is live").bindings
    }

    pub fn parts_mut(&mut self) -> (&mut AstStoreBuilder, &mut BindingData) {
        let draft = self.builder.as_deref_mut().expect("binding session is live");
        (&mut draft.ast, &mut draft.bindings)
    }

    pub fn with_lazy_lists<R>(
        &mut self,
        bind: impl FnOnce(&mut LazyBindSession<'_>) -> Result<R, AstAccessError>,
    ) -> Result<R, AstAccessError> {
        if self.failed {
            return Err(AstAccessError::Failed);
        }
        // Set first so an unwind caught by the caller cannot publish partial
        // initial binding. Only a fully committed session clears this marker.
        self.failed = true;
        let draft = self.builder.as_deref_mut().expect("binding session is live");
        let mut session =
            LazyBindSession::new(&draft.ast, &mut draft.bindings, LazyBindPhase::Initial);
        let result = bind(&mut session)?;
        session.commit()?;
        self.failed = false;
        Ok(result)
    }

    pub fn publish(mut self) -> Result<AstStoreLease, AstAccessError> {
        if self.failed {
            return Err(AstAccessError::Failed);
        }
        let builder = self.builder.take().expect("binding builder is consumed once");
        let FileDraft { ast, mut bindings } = *builder;
        let session = LazyBindSession::new(&ast, &mut bindings, LazyBindPhase::Initial);
        session.validate_dependencies(&ast.foreign, &ast.foreign_lists, &ast.foreign_modifiers)?;
        session.validate_binding_data()?;
        session.commit()?;
        let base = Arc::new(ast.finish());
        let bindings = bindings.publish_shared(Arc::clone(&base));
        let mut body = self.owner.lock()?;
        let mut extensions = FxHashMap::default();
        {
            let read = bindings.read()?;
            for fragment in read.fragments() {
                self.owner.directory.register(fragment.0.id(), self.owner)?;
                extensions.insert(fragment.0.id(), Arc::clone(&fragment.0));
            }
        }
        let lease = AstStoreLease::from(Arc::clone(&base));
        *body = FileBody::Bound { base, extensions, bindings };
        self.published = true;
        self.owner.changed.notify_all();
        Ok(lease)
    }
}

impl Drop for BindSession<'_> {
    fn drop(&mut self) {
        if !self.published {
            // A poisoned entry is already terminal. Never expose or retry its
            // partially mutated builder, even while unwinding a binder panic.
            if let Ok(mut body) = self.owner.lock() {
                *body = FileBody::Failed;
            }
            self.owner.changed.notify_all();
        }
    }
}

/// An operation's permitted file versions and their original directories.
///
/// Importing the directory, rather than copying its current registrations, makes
/// later JSDoc/token publication visible to every Program retaining that file.
pub struct AstRead {
    directories: Vec<Arc<AstDirectory>>,
    owners: FxHashMap<AstStoreId, Arc<FileAstOwner>>,
}

impl AstRead {
    pub fn new(directory: Arc<AstDirectory>) -> Self {
        Self { directories: vec![directory], owners: FxHashMap::default() }
    }

    pub fn include(&mut self, owner: Arc<FileAstOwner>) {
        if !self.directories.iter().any(|directory| Arc::ptr_eq(directory, &owner.directory)) {
            self.directories.push(Arc::clone(&owner.directory));
        }
        self.owners.insert(owner.id, owner);
    }

    /// Acquire once for a traversal; each node/list view then borrows the lease
    /// directly, without directory locks or reference-count work per field.
    pub fn lease(&self, store: AstStoreId) -> Result<AstStoreLease, AstAccessError> {
        for directory in &self.directories {
            if let Some((root, registered)) = directory.resolve(store)? {
                let permitted =
                    self.owners.get(&root).ok_or(AstAccessError::UnpermittedOwner(root))?;
                if !Weak::ptr_eq(&registered, &Arc::downgrade(permitted)) {
                    return Err(AstAccessError::UnpermittedOwner(root));
                }
                return permitted.lease(store);
            }
        }
        Err(AstAccessError::UnknownStore(store))
    }
}

// An unpublished builder is boxed; a bound/retired entry must not reserve inline
// space for every construction-only column header throughout the file's life.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<FileBody>() <= 64);
