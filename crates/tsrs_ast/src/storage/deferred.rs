//! File-local deferred member binding. A public operation takes the file gate
//! before claiming a list; recursive forces share one explicit mutable session.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::memory::{AstBufferMemory, AstMapMemory, BufferMemory};

use super::{
    AstAccessError, AstReader, AstStoreBuilder, AstStoreId, AstStoreLease, BindingData,
    BindingLease, BindingRead, FileAstOwner, FileAstPhase, FlowKey, ListKey, ListView,
    ModifierListKey, ModifierListView, NodeKey, NodeView,
};
use crate::{Kind, NodeFlags};
use tsrs_core::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberListKind {
    Type,
    Class,
}

#[derive(Clone, Copy, Debug)]
pub struct LazyListSpec {
    pub list: ListKey,
    pub owner: NodeKey,
    pub range: TextRange,
    pub kind: MemberListKind,
    pub context_flags: NodeFlags,
    pub parsing_contexts: u32,
}

/// The binder state recorded where it skipped a list. Storage owners, rather
/// than semantic IDs or ambient thread state, qualify every saved reference.
#[derive(Clone, Copy, Debug)]
pub struct LazyBindingContext {
    pub container: Option<NodeKey>,
    pub this_container: Option<NodeKey>,
    pub block_scope_container: Option<NodeKey>,
    pub last_container: Option<NodeKey>,
    pub current_flow: Option<FlowKey>,
    pub unreachable_flow: FlowKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LazyListState {
    Pending,
    Deferred,
    ParseOnly,
    Building,
    Ready,
    Failed,
}

#[derive(Clone, Copy)]
pub(super) struct LazyListRecord {
    spec: LazyListSpec,
    state: LazyListState,
    context: Option<LazyBindingContext>,
    elements: Option<ListKey>,
}

#[derive(Default)]
pub(super) struct DeferredLists {
    lists: FxHashMap<ListKey, LazyListRecord>,
    pending: Vec<ListKey>,
}

impl DeferredLists {
    pub(super) fn memory(&self) -> (AstBufferMemory, AstMapMemory) {
        (self.pending.buffer_memory(), AstMapMemory::of(&self.lists))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LazyBindPhase {
    Initial,
    Late,
}

/// The parser builds privately; only binding receives the reentrant session.
/// Initial binding must splice container links; late binding must not do so.
pub trait LazyListHooks {
    fn parse(
        &mut self,
        ast: &mut AstStoreBuilder,
        spec: LazyListSpec,
    ) -> Result<ListKey, AstAccessError>;

    fn bind(
        &mut self,
        session: &mut LazyBindSession<'_>,
        list: ListKey,
        context: LazyBindingContext,
    ) -> Result<(), AstAccessError>;
}

impl BindingData {
    pub fn register_lazy_list(
        &mut self,
        ast: &AstStoreBuilder,
        spec: LazyListSpec,
    ) -> Result<(), AstAccessError> {
        ast.check_bindings(self)?;
        let list = ast.list(spec.list)?;
        let owner = ast.node(spec.owner)?;
        let kind_matches = match spec.kind {
            MemberListKind::Type => {
                matches!(owner.kind(), Kind::InterfaceDeclaration | Kind::TypeLiteral)
            }
            MemberListKind::Class => {
                matches!(owner.kind(), Kind::ClassDeclaration | Kind::ClassExpression)
            }
        };
        let valid_range = spec
            .range
            .source_pos()
            .zip(spec.range.end().source_offset())
            .is_some_and(|(start, end)| ast.source.get(start as usize..end as usize).is_some());
        if !list.is_empty()
            || list.is_missing()
            || list.range() != spec.range
            || !kind_matches
            || !valid_range
            || self.lazy_list_record(spec.list).is_some()
        {
            return Err(AstAccessError::InvalidLazyList(spec.list));
        }
        self.track_node(spec.owner);
        let deferred = self.deferred.get_or_insert_with(Box::default);
        deferred.pending.push(spec.list);
        deferred.lists.insert(
            spec.list,
            LazyListRecord { spec, state: LazyListState::Pending, context: None, elements: None },
        );
        Ok(())
    }

    pub fn defer_lazy_list(
        &mut self,
        list: ListKey,
        context: LazyBindingContext,
    ) -> Result<(), AstAccessError> {
        self.flow(context.unreachable_flow)?;
        if let Some(flow) = context.current_flow {
            self.flow(flow)?;
        }
        let record = self.lazy_list_record(list).ok_or(AstAccessError::UnknownLazyList(list))?;
        if record.state != LazyListState::Pending {
            return Err(AstAccessError::InvalidLazyList(list));
        }
        for key in [
            context.container,
            context.this_container,
            context.block_scope_container,
            context.last_container,
        ]
        .into_iter()
        .flatten()
        {
            self.track_node(key);
        }
        let record = self.lazy_list_record_mut(list).expect("registered list");
        record.context = Some(context);
        record.state = LazyListState::Deferred;
        Ok(())
    }

    pub fn lazy_list_state(&self, list: ListKey) -> Result<LazyListState, AstAccessError> {
        self.lazy_list_record(list)
            .map(|record| record.state)
            .ok_or(AstAccessError::UnknownLazyList(list))
    }

    pub(super) fn mark_unbound_lazy_lists(&mut self) {
        let Some(deferred) = &mut self.deferred else { return };
        // Drop the drained allocation rather than retaining the initial file's
        // entire queue for later forces of individual lists.
        for list in std::mem::take(&mut deferred.pending) {
            let record = deferred.lists.get_mut(&list).expect("registered list");
            if record.state == LazyListState::Pending {
                record.state = LazyListState::ParseOnly;
            }
        }
    }

    fn lazy_list_record(&self, list: ListKey) -> Option<&LazyListRecord> {
        self.deferred.as_ref()?.lists.get(&list)
    }

    fn lazy_list_record_mut(&mut self, list: ListKey) -> Option<&mut LazyListRecord> {
        self.deferred.as_mut()?.lists.get_mut(&list)
    }
}

/// Views borrowed through this operation prevent forcing through the same
/// context until those borrows end. Callbacks use the active session directly.
pub struct FileBindingContext {
    owner: Arc<FileAstOwner>,
    bindings: BindingLease,
}

impl FileBindingContext {
    pub(super) fn new(owner: Arc<FileAstOwner>, bindings: BindingLease) -> Self {
        Self { owner, bindings }
    }

    pub fn read(&self) -> Result<BindingRead<'_>, AstAccessError> {
        self.bindings.read()
    }

    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "Exclusive context access prevents live binding views across forcing and reentry."
    )]
    pub fn force_list(
        &mut self,
        list: ListKey,
        hooks: &mut impl LazyListHooks,
    ) -> Result<DeferredListLease, AstAccessError> {
        {
            let read = self.bindings.read()?;
            if self.owner.phase()? != FileAstPhase::Bound {
                return Err(AstAccessError::Retired);
            }
            let record =
                read.data().lazy_list_record(list).ok_or(AstAccessError::UnknownLazyList(list))?;
            if record.state == LazyListState::Ready {
                return self.completed_list(list, *record, read.data());
            }
        }
        // No list can be claimed while another session owns the file gate.
        let mut writer = self.bindings.write()?;
        if self.owner.phase()? != FileAstPhase::Bound {
            writer.complete()?;
            return Err(AstAccessError::Retired);
        }
        // Invalid caller keys do not poison an otherwise healthy file.
        if writer.data().lazy_list_record(list).is_none() {
            writer.complete()?;
            return Err(AstAccessError::UnknownLazyList(list));
        }
        let base = self.bindings.root_ast();
        let mut session = LazyBindSession::new(&base, writer.data_mut(), LazyBindPhase::Late);
        session.force_list(list, hooks)?;
        let fragments = session.commit()?;
        self.owner.install_fragments(&fragments)?;
        let result = self.completed_list(
            list,
            *writer.data().lazy_list_record(list).expect("registered list"),
            writer.data(),
        )?;
        writer.complete()?;
        Ok(result)
    }

    fn completed_list(
        &self,
        list: ListKey,
        record: LazyListRecord,
        data: &BindingData,
    ) -> Result<DeferredListLease, AstAccessError> {
        let elements = record.elements.ok_or(AstAccessError::LazyListNotParsed(list))?;
        let syntax = if elements.store() == data.syntax_id {
            self.bindings.root_ast()
        } else {
            AstStoreLease::from(Arc::clone(&data.extensions[&elements.store()]))
        };
        Ok(DeferredListLease { identity: list, elements, syntax, bindings: self.bindings.clone() })
    }
}

/// Retains both the completed syntax and the binding data it references. List
/// identity is the original placeholder, independent of fragment allocation.
pub struct DeferredListLease {
    identity: ListKey,
    elements: ListKey,
    syntax: AstStoreLease,
    bindings: BindingLease,
}

impl DeferredListLease {
    pub const fn key(&self) -> ListKey {
        self.identity
    }
    pub fn range(&self) -> TextRange {
        self.view().range()
    }
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = NodeKey> + DoubleEndedIterator + '_ {
        self.view().nodes()
    }
    pub fn ast(&self) -> &AstStoreLease {
        &self.syntax
    }
    pub fn bindings(&self) -> &BindingLease {
        &self.bindings
    }
    fn view(&self) -> ListView<'_> {
        self.syntax.list(self.elements).expect("completed list validated before publication")
    }
}

/// A single initial or late binder operation. Private fragments may grow; all
/// callers use copied keys and short views. No public gate is reacquired here.
pub struct LazyBindSession<'a> {
    base: &'a dyn AstReader,
    data: &'a mut BindingData,
    phase: LazyBindPhase,
    fragments: Vec<AstStoreBuilder>,
    started: Vec<ListKey>,
    failed: bool,
    committed: bool,
}

impl<'a> LazyBindSession<'a> {
    pub(super) fn new(
        base: &'a dyn AstReader,
        data: &'a mut BindingData,
        phase: LazyBindPhase,
    ) -> Self {
        Self {
            base,
            data,
            phase,
            fragments: Vec::new(),
            started: Vec::new(),
            failed: false,
            committed: false,
        }
    }

    pub const fn phase(&self) -> LazyBindPhase {
        self.phase
    }
    pub fn bindings(&self) -> &BindingData {
        self.data
    }
    pub fn bindings_mut(&mut self) -> &mut BindingData {
        self.data
    }

    fn reader(&self, store: AstStoreId) -> Result<&dyn AstReader, AstAccessError> {
        if store == self.data.syntax_id {
            return Ok(self.base);
        }
        if let Some(ast) = self.fragments.iter().find(|ast| ast.id() == store) {
            return Ok(ast);
        }
        self.data
            .extensions
            .get(&store)
            .map(|ast| &**ast as &dyn AstReader)
            .ok_or(AstAccessError::UnpermittedOwner(store))
    }

    pub fn node(&self, key: NodeKey) -> Result<NodeView<'_>, AstAccessError> {
        Ok(self.reader(key.store())?.node(key)?)
    }

    pub fn list(&self, key: ListKey) -> Result<ListView<'_>, AstAccessError> {
        let elements = match self.data.lazy_list_record(key) {
            Some(record) => match record.state {
                LazyListState::Pending => key,
                LazyListState::Ready | LazyListState::Building => {
                    record.elements.ok_or(AstAccessError::LazyListNotParsed(key))?
                }
                _ => return Err(AstAccessError::LazyListNotParsed(key)),
            },
            None => key,
        };
        Ok(self.reader(elements.store())?.list(elements)?)
    }

    pub fn modifier_list(
        &self,
        key: ModifierListKey,
    ) -> Result<ModifierListView<'_>, AstAccessError> {
        Ok(self.reader(key.store())?.modifier_list(key)?)
    }

    /// Only unpublished fragments are mutable, including when nested forces
    /// return to a suspended outer binder. Base/published syntax has no writer.
    pub fn parts_mut(
        &mut self,
        store: AstStoreId,
    ) -> Result<(&mut AstStoreBuilder, &mut BindingData), AstAccessError> {
        let ast = self
            .fragments
            .iter_mut()
            .find(|ast| ast.id() == store)
            .ok_or(AstAccessError::NotMutableStore(store))?;
        Ok((ast, self.data))
    }

    /// Existing syntax may receive binding annotations, never syntax writes.
    /// The view borrows a disjoint private/base reader or a retained fragment.
    pub fn with_node_bindings<R>(
        &mut self,
        key: NodeKey,
        update: impl FnOnce(&NodeView<'_>, &mut BindingData) -> Result<R, AstAccessError>,
    ) -> Result<R, AstAccessError> {
        if key.store() == self.data.syntax_id {
            return update(&self.base.node(key)?, self.data);
        }
        if let Some(ast) = self.fragments.iter().find(|ast| ast.id() == key.store()) {
            return update(&ast.node(key)?, self.data);
        }
        let ast = self
            .data
            .extensions
            .get(&key.store())
            .cloned()
            .ok_or(AstAccessError::UnpermittedOwner(key.store()))?;
        update(&ast.node(key)?, self.data)
    }

    fn validate_context(&self, context: LazyBindingContext) -> Result<(), AstAccessError> {
        for key in [
            context.container,
            context.this_container,
            context.block_scope_container,
            context.last_container,
        ]
        .into_iter()
        .flatten()
        {
            self.node(key)?;
        }
        self.data.flow(context.unreachable_flow)?;
        if let Some(flow) = context.current_flow {
            self.data.flow(flow)?;
        }
        Ok(())
    }

    pub fn force_list(
        &mut self,
        list: ListKey,
        hooks: &mut impl LazyListHooks,
    ) -> Result<(), AstAccessError> {
        if self.failed {
            return Err(AstAccessError::Failed);
        }
        let result = self.force_worker(list, hooks);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    /// Initial binding sometimes cannot save a replayable context. Parse now;
    /// its caller binds the returned members using the current binder state.
    pub fn parse_list_now(
        &mut self,
        list: ListKey,
        hooks: &mut impl LazyListHooks,
    ) -> Result<(), AstAccessError> {
        if self.phase != LazyBindPhase::Initial
            || self.data.lazy_list_state(list)? != LazyListState::Pending
        {
            return Err(AstAccessError::InvalidLazyList(list));
        }
        self.data.lazy_list_record_mut(list).expect("registered list").state =
            LazyListState::ParseOnly;
        self.force_list(list, hooks)
    }

    fn force_worker(
        &mut self,
        list: ListKey,
        hooks: &mut impl LazyListHooks,
    ) -> Result<(), AstAccessError> {
        let record =
            *self.data.lazy_list_record(list).ok_or(AstAccessError::UnknownLazyList(list))?;
        match record.state {
            LazyListState::Pending | LazyListState::Ready => return Ok(()),
            LazyListState::Building => {
                return record.elements.map(|_| ()).ok_or(AstAccessError::LazyListNotParsed(list));
            }
            LazyListState::Failed => return Err(AstAccessError::Failed),
            LazyListState::Deferred | LazyListState::ParseOnly => {}
        }
        self.started.push(list);
        self.node(record.spec.owner)?;
        if let Some(context) = record.context {
            self.validate_context(context)?;
        }
        self.data.lazy_list_record_mut(list).expect("registered list").state =
            LazyListState::Building;
        let mut ast = self.data.fragment_builder()?;
        let elements = hooks.parse(&mut ast, record.spec)?;
        let parsed = ast.list(elements)?;
        if parsed.range() != record.spec.range || parsed.is_missing() {
            return Err(AstAccessError::InvalidLazyList(list));
        }
        for child in parsed.nodes() {
            if ast.node(child)?.parent() != Some(record.spec.owner) {
                return Err(AstAccessError::InvalidLazyList(list));
            }
        }
        self.fragments.push(ast);
        self.data.lazy_list_record_mut(list).expect("registered list").elements = Some(elements);
        // Parsed keys are available before binding, so same-list recursion sees
        // the partial list/table without attempting to take the public gate.
        if let Some(context) = record.context {
            hooks.bind(self, list, context)?;
        }
        self.data.lazy_list_record_mut(list).expect("registered list").state = LazyListState::Ready;
        Ok(())
    }

    pub(super) fn commit(mut self) -> Result<Vec<AstStoreLease>, AstAccessError> {
        if self.failed {
            return Err(AstAccessError::Failed);
        }
        // Also reject a nested panic caught inside a callback: that stack may
        // unwind without dropping this outer session or setting `failed`.
        if self
            .started
            .iter()
            .any(|&list| self.data.lazy_list_state(list) != Ok(LazyListState::Ready))
        {
            return Err(AstAccessError::Failed);
        }
        for ast in &self.fragments {
            self.validate_dependencies(&ast.foreign, &ast.foreign_lists, &ast.foreign_modifiers)?;
        }
        if let Some(checks) = &self.data.pending_checks {
            for id in &checks.fragments {
                let ast = &self.data.extensions[id];
                self.validate_dependencies(
                    &ast.foreign,
                    &ast.foreign_lists,
                    &ast.foreign_modifiers,
                )?;
            }
            for &key in &checks.nodes {
                self.node(key)?;
            }
        }
        self.data.extensions.reserve(self.fragments.len());
        let retained = self.data.pending_checks.as_ref().map_or(0, |checks| checks.fragments.len());
        let mut leases = Vec::with_capacity(self.fragments.len() + retained);
        if let Some(checks) = &self.data.pending_checks {
            for id in &checks.fragments {
                leases.push(AstStoreLease::from(Arc::clone(&self.data.extensions[id])));
            }
        }
        for ast in self.fragments.drain(..) {
            let store = Arc::new(ast.finish());
            self.data.extensions.insert(store.id(), Arc::clone(&store));
            leases.push(AstStoreLease::from(store));
        }
        // Nested lists not reached by the binder remain parse-only, as at the
        // end of initial file binding; never invent a deferred bind context.
        if self.phase == LazyBindPhase::Late {
            self.data.mark_unbound_lazy_lists();
        }
        if let Some(checks) = &mut self.data.pending_checks {
            **checks = super::binding::BindingChecks::default();
        }
        self.committed = true;
        Ok(leases)
    }

    /// Initial publication checks the complete namespace once. Deferred writes
    /// subsequently journal the edges they change, including annotation targets.
    #[expect(
        clippy::iter_over_hash_type,
        reason = "Validation accepts every dependency or rejects the private file; hash order cannot affect compiler semantics."
    )]
    pub(super) fn validate_binding_data(&self) -> Result<(), AstAccessError> {
        for ast in self.data.extensions.values() {
            self.validate_dependencies(&ast.foreign, &ast.foreign_lists, &ast.foreign_modifiers)?;
        }
        for key in self.data.node_edges() {
            self.node(key)?;
        }
        for &key in &self.data.foreign {
            self.node(key)?;
        }
        for store in self.data.annotations.fragment_stores() {
            self.reader(store)?;
        }
        self.data.annotations.visit_node_edges(
            |key| self.node(key).map(|_| ()),
            self.data.syntax_id,
            &self.data.foreign,
        )?;
        for record in self.data.deferred.iter().flat_map(|deferred| deferred.lists.values()) {
            self.node(record.spec.owner)?;
            self.reader(record.spec.list.store())?.list(record.spec.list)?;
            if let Some(context) = record.context {
                self.validate_context(context)?;
            }
            if let Some(elements) = record.elements {
                self.reader(elements.store())?.list(elements)?;
            }
        }
        Ok(())
    }

    pub(super) fn validate_dependencies(
        &self,
        nodes: &[NodeKey],
        lists: &[ListKey],
        modifiers: &[ModifierListKey],
    ) -> Result<(), AstAccessError> {
        for &key in nodes {
            self.node(key)?;
        }
        for &key in lists {
            self.reader(key.store())?.list(key)?;
        }
        for &key in modifiers {
            self.modifier_list(key)?;
        }
        Ok(())
    }
}

impl Drop for LazyBindSession<'_> {
    fn drop(&mut self) {
        if !self.committed {
            for list in &self.started {
                self.data
                    .lazy_list_record_mut(*list)
                    .expect("started list remains registered")
                    .state = LazyListState::Failed;
            }
        }
    }
}
