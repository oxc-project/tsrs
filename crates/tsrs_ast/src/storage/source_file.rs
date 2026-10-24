//! Durable source data, independent of retireable syntax and binding owners.
//! Mapped-output bundles retain this information; diagnostic ownership is separate.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use tsrs_core::tspath::{self, Path};
use tsrs_core::{LanguageVariant, ScriptKind, Tristate};

use super::memory::BufferMemory;
use super::{AstBufferMemory, AstMapMemory, SourceFileInfoMemory};
use crate::{
    CheckJsDirective, CommentDirective, FileReference, HasFileName, PositionMap, Pragma,
    SourceFileLike, SourceFileParseOptions, compute_position_map,
};

static NEXT_SOURCE_FILE_ID: AtomicU32 = AtomicU32::new(1);

/// One source-file version, independent of its body and semantic node IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct SourceFileId(NonZeroU32);

impl SourceFileId {
    fn allocate(counter: &AtomicU32) -> Result<Self, SourceFileInfoError> {
        // Relaxed: uniqueness alone is required; this publishes no source data.
        let id = counter
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map_err(|_| SourceFileInfoError::IdentityExhausted)?;
        Ok(Self(NonZeroU32::new(id).expect("source identities start at one")))
    }
}

const _: () = assert!(size_of::<SourceFileId>() == 4);
const _: () = assert!(size_of::<Option<SourceFileId>>() == 4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFileInfoError {
    IdentityExhausted,
    UnknownSource(SourceFileId),
    DuplicateSource(SourceFileId),
    InvalidMapping { source: SourceFileId, error: tsrs_spanmap::MappingError },
    DifferentSourceText { original: SourceFileId, replacement: SourceFileId },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceFileParseResults {
    pub language_variant: LanguageVariant,
    pub script_kind: ScriptKind,
    pub is_declaration_file: bool,
    pub uses_uri_style_node_core_modules: Tristate,
    pub identifier_count: usize,
    pub node_count: usize,
    pub text_count: usize,
}

/// Plain text/range data only. Mutable access comes from an exclusive draft.
/// Absent and empty collections use the legacy Rust API's empty-slice behavior.
#[derive(Clone, Debug, Default)]
pub struct SourceFileDirectives {
    pub ambient_module_names: Vec<String>,
    pub comment_directives: Vec<CommentDirective>,
    pub pragmas: Vec<Pragma>,
    pub referenced_files: Vec<FileReference>,
    pub type_reference_directives: Vec<FileReference>,
    pub lib_reference_directives: Vec<FileReference>,
}

static EMPTY_DIRECTIVES: SourceFileDirectives = SourceFileDirectives {
    ambient_module_names: Vec::new(),
    comment_directives: Vec::new(),
    pragmas: Vec::new(),
    referenced_files: Vec::new(),
    type_reference_directives: Vec::new(),
    lib_reference_directives: Vec::new(),
};

impl SourceFileDirectives {
    pub fn is_empty(&self) -> bool {
        self.ambient_module_names.is_empty()
            && self.comment_directives.is_empty()
            && self.pragmas.is_empty()
            && self.referenced_files.is_empty()
            && self.type_reference_directives.is_empty()
            && self.lib_reference_directives.is_empty()
    }
}

/// Source metadata/text can be retained after the corresponding AST body retires.
///
/// There are deliberately no node keys, body/binding leases or diagnostic lists.
/// A caller can share this value with Arc without retaining any syntax.
pub struct SourceFileInfo {
    id: SourceFileId,
    parse_options: SourceFileParseOptions,
    source: Arc<str>,
    parse_results: SourceFileParseResults,
    directives: Option<Arc<SourceFileDirectives>>,
    check_js_directive: Option<Box<CheckJsDirective>>,
    hash: u128,
    ecma_line_map: OnceLock<Vec<u32>>,
    position_map: OnceLock<PositionMap>,
}

impl SourceFileInfo {
    pub fn directives_weak(&self) -> Option<std::sync::Weak<SourceFileDirectives>> {
        self.directives.as_ref().map(Arc::downgrade)
    }

    pub const fn id(&self) -> SourceFileId {
        self.id
    }

    pub fn parse_options(&self) -> &SourceFileParseOptions {
        &self.parse_options
    }

    pub fn file_name(&self) -> &str {
        &self.parse_options.file_name
    }

    pub fn path(&self) -> &Path {
        &self.parse_options.path
    }

    pub fn text(&self) -> &str {
        &self.source
    }

    /// Share text with a syntax/fragment builder without retaining a body.
    pub fn source(&self) -> &Arc<str> {
        &self.source
    }

    pub const fn parse_results(&self) -> &SourceFileParseResults {
        &self.parse_results
    }

    pub fn directives(&self) -> &SourceFileDirectives {
        self.directives.as_deref().unwrap_or(&EMPTY_DIRECTIVES)
    }

    pub fn check_js_directive(&self) -> Option<&CheckJsDirective> {
        self.check_js_directive.as_deref()
    }

    pub const fn hash(&self) -> u128 {
        self.hash
    }

    /// Counting on a cache miss must not allocate and retain a line map.
    pub fn ecma_line_count(&self) -> usize {
        self.ecma_line_map
            .get()
            .map_or_else(|| tsrs_core::count_ecma_line_starts(&self.source), Vec::len)
    }

    pub fn ecma_line_map(&self) -> &[u32] {
        self.ecma_line_map.get_or_init(|| tsrs_core::compute_ecma_line_starts(&self.source))
    }

    pub fn get_position_map(&self) -> &PositionMap {
        self.position_map.get_or_init(|| compute_position_map(&self.source))
    }

    /// Requested bytes, not resident memory. Shared text/path payloads and
    /// pragma map lower bounds are reported separately from owned buffers.
    ///
    /// Count each shared directive block once across source-file updates.
    pub fn memory(&self) -> SourceFileInfoMemory {
        let mut directives = AstBufferMemory::default();
        let mut maps = AstMapMemory::default();
        if let Some(data) = self.directives.as_deref() {
            directives = AstBufferMemory::object(size_of::<SourceFileDirectives>())
                + data.ambient_module_names.buffer_memory()
                + data.comment_directives.buffer_memory()
                + data.pragmas.buffer_memory()
                + data.referenced_files.buffer_memory()
                + data.type_reference_directives.buffer_memory()
                + data.lib_reference_directives.buffer_memory();
            for name in &data.ambient_module_names {
                directives = directives + name.buffer_memory();
            }
            for reference in data
                .referenced_files
                .iter()
                .chain(&data.type_reference_directives)
                .chain(&data.lib_reference_directives)
            {
                directives = directives + reference.file_name.buffer_memory();
            }
            for pragma in &data.pragmas {
                directives = directives + pragma.name.buffer_memory();
                maps = maps + AstMapMemory::of(&pragma.args);
                #[expect(
                    clippy::iter_over_hash_type,
                    reason = "Byte and allocation sums are independent of argument iteration order."
                )]
                for (name, argument) in &pragma.args {
                    directives = directives
                        + name.buffer_memory()
                        + argument.name.buffer_memory()
                        + argument.value.buffer_memory();
                }
            }
        }
        let mut text_maps = self
            .ecma_line_map
            .get()
            .map_or_else(AstBufferMemory::default, BufferMemory::buffer_memory);
        if let Some(map) = self.position_map.get() {
            let (live_bytes, capacity_bytes) = map.entry_buffer_sizes();
            text_maps = text_maps
                + AstBufferMemory {
                    live_bytes,
                    capacity_bytes,
                    allocations: usize::from(capacity_bytes != 0),
                };
        }
        SourceFileInfoMemory {
            inline_bytes: size_of::<Self>(),
            file_name: self.parse_options.file_name.buffer_memory(),
            directives,
            check_js_directive: self
                .check_js_directive
                .as_ref()
                .map_or_else(AstBufferMemory::default, |_| {
                    AstBufferMemory::object(size_of::<CheckJsDirective>())
                }),
            text_maps,
            maps,
            source_bytes: self.source.len(),
            path_bytes: self.parse_options.path.len(),
        }
    }
}

impl HasFileName for SourceFileInfo {
    fn file_name(&self) -> &str {
        Self::file_name(self)
    }

    fn path(&self) -> &Path {
        Self::path(self)
    }
}

impl SourceFileLike for SourceFileInfo {
    fn text(&self) -> &str {
        Self::text(self)
    }

    fn ecma_line_map(&self) -> &[u32] {
        Self::ecma_line_map(self)
    }
}

/// One parser/loader's exclusive metadata draft. Freezing consumes its writer;
/// it does not imply that binding or file-owner publication has completed.
pub struct SourceFileInfoDraft {
    info: SourceFileInfo,
}

impl SourceFileInfoDraft {
    /// Same normalized absolute filename precondition as `NodeFactory::new_source_file`.
    pub fn new(
        parse_options: SourceFileParseOptions,
        source: Arc<str>,
    ) -> Result<Self, SourceFileInfoError> {
        assert!(
            tspath::get_encoded_root_length(&parse_options.file_name)
                != tspath::EncodedRootLength::Relative
                && parse_options.file_name == tspath::normalize_path(&parse_options.file_name),
            "fileName should be normalized and absolute: {:?}",
            parse_options.file_name,
        );
        Ok(Self {
            info: SourceFileInfo {
                id: SourceFileId::allocate(&NEXT_SOURCE_FILE_ID)?,
                parse_options,
                source,
                parse_results: SourceFileParseResults::default(),
                directives: None,
                check_js_directive: None,
                hash: 0,
                ecma_line_map: OnceLock::new(),
                position_map: OnceLock::new(),
            },
        })
    }

    pub const fn info(&self) -> &SourceFileInfo {
        &self.info
    }

    pub const fn parse_results_mut(&mut self) -> &mut SourceFileParseResults {
        &mut self.info.parse_results
    }

    pub fn directives_mut(&mut self) -> &mut SourceFileDirectives {
        Arc::make_mut(self.info.directives.get_or_insert_with(Arc::default))
    }

    pub fn set_check_js_directive(&mut self, directive: Option<CheckJsDirective>) {
        self.info.check_js_directive = directive.map(Box::new);
    }

    pub const fn set_hash(&mut self, hash: u128) {
        self.info.hash = hash;
    }

    /// The non-node, non-mapper subset of `SourceFile::copy_from`.
    ///
    /// Preserve the target's options/text, counters/hash, check-JS directive and text maps.
    /// Body indicators, imports and mapper links require their own writer.
    pub fn copy_parse_metadata_from(&mut self, other: &SourceFileInfo) {
        let results = &mut self.info.parse_results;
        results.language_variant = other.parse_results.language_variant;
        results.script_kind = other.parse_results.script_kind;
        results.is_declaration_file = other.parse_results.is_declaration_file;
        results.uses_uri_style_node_core_modules =
            other.parse_results.uses_uri_style_node_core_modules;
        self.info.directives.clone_from(&other.directives);
    }

    pub fn freeze(mut self) -> SourceFileInfo {
        if self.info.directives.as_deref().is_some_and(SourceFileDirectives::is_empty) {
            self.info.directives = None;
        }
        self.info
    }
}
