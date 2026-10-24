//! Retained metadata for content-mapped output groups, without AST body ownership.

use std::sync::Arc;

use rustc_hash::FxHashSet;
use tsrs_core::TextRange;
use tsrs_spanmap::SpanMap;

use super::memory::BufferMemory;
use super::{AstBufferMemory, SourceFileId, SourceFileInfo, SourceFileInfoError};
use crate::{MappedDiagnosticDirectivePolicy, SourceFileLike, SourceFileParseOptions};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedMappedDiagnosticDirective {
    pub original_range: TextRange,
    pub virtual_range: TextRange,
    pub policy: MappedDiagnosticDirectivePolicy,
    pub unused_code: i32,
    pub unused_message_text: String,
    pub source: String,
}

/// One finalized output's mapping data. The bundle validates mappings against
/// the shared original text before publication. No body owner can enter it.
pub struct MappedSourceOutput {
    source: Arc<SourceFileInfo>,
    virtual_file_name: String,
    span_map: Option<Arc<SpanMap>>,
    diagnostic_directives: Vec<OwnedMappedDiagnosticDirective>,
}

impl MappedSourceOutput {
    pub fn new(
        source: Arc<SourceFileInfo>,
        virtual_file_name: String,
        span_map: Option<Arc<SpanMap>>,
        diagnostic_directives: Vec<OwnedMappedDiagnosticDirective>,
    ) -> Self {
        Self { source, virtual_file_name, span_map, diagnostic_directives }
    }
}

/// Canonical and supplemental metadata share one owner. Cross-output links are
/// indexes into this immutable bundle, so no strong ownership cycle is formed.
pub struct MappedSourceBundle {
    content_mapper: String,
    transform_identity: String,
    parse_options: SourceFileParseOptions,
    original_text: Arc<str>,
    // Element zero is canonical. The others preserve supplemental output order.
    outputs: Vec<MappedSourceOutput>,
}

impl MappedSourceBundle {
    /// Call after the loader/cache has finalized each output's plain metadata.
    /// This publishes only source information; binding/body publication is separate.
    pub fn new(
        content_mapper: String,
        transform_identity: String,
        parse_options: SourceFileParseOptions,
        original_text: Arc<str>,
        canonical: MappedSourceOutput,
        supplemental: Vec<MappedSourceOutput>,
    ) -> Result<Arc<Self>, SourceFileInfoError> {
        let mut outputs = Vec::with_capacity(1 + supplemental.len());
        outputs.push(canonical);
        outputs.extend(supplemental);
        let mut ids = FxHashSet::default();
        for output in &outputs {
            let source = output.source.id();
            if !ids.insert(source) {
                return Err(SourceFileInfoError::DuplicateSource(source));
            }
            if let Some(map) = &output.span_map
                && let Some(error) = map.validate(output.source.text(), &original_text)
            {
                return Err(SourceFileInfoError::InvalidMapping { source, error });
            }
        }
        Ok(Arc::new(Self {
            content_mapper,
            transform_identity,
            parse_options,
            original_text,
            outputs,
        }))
    }

    pub fn file(
        self: &Arc<Self>,
        id: SourceFileId,
    ) -> Result<MappedSourceFile, SourceFileInfoError> {
        let index = self
            .outputs
            .iter()
            .position(|output| output.source.id() == id)
            .ok_or(SourceFileInfoError::UnknownSource(id))?;
        Ok(MappedSourceFile {
            bundle: Arc::clone(self),
            source: Arc::clone(&self.outputs[index].source),
            index,
        })
    }

    pub fn source_infos(&self) -> impl ExactSizeIterator<Item = &SourceFileInfo> {
        self.outputs.iter().map(|output| output.source.as_ref())
    }

    /// Bundle-only requests. Source info objects/buffers, their shared text/path,
    /// all Arc control blocks and allocator overhead must be counted separately.
    pub fn memory(&self) -> MappedSourceBundleMemory {
        let metadata = self.content_mapper.buffer_memory()
            + self.transform_identity.buffer_memory()
            + self.parse_options.file_name.buffer_memory();
        let mut output_data = AstBufferMemory::default();
        for (index, output) in self.outputs.iter().enumerate() {
            output_data = output_data
                + output.virtual_file_name.buffer_memory()
                + output.diagnostic_directives.buffer_memory();
            for directive in &output.diagnostic_directives {
                output_data = output_data
                    + directive.unused_message_text.buffer_memory()
                    + directive.source.buffer_memory();
            }
            if let Some(map) = &output.span_map {
                // Several outputs may share one immutable map; count it once.
                if !self.outputs[..index].iter().any(|prior| {
                    prior.span_map.as_ref().is_some_and(|prior| Arc::ptr_eq(prior, map))
                }) {
                    let memory = map.memory();
                    output_data = output_data
                        + AstBufferMemory::object(memory.inline_bytes)
                        + AstBufferMemory {
                            live_bytes: memory.live_bytes,
                            capacity_bytes: memory.capacity_bytes,
                            allocations: memory.allocations,
                        };
                }
            }
        }
        MappedSourceBundleMemory {
            inline_bytes: size_of::<Self>(),
            outputs: self.outputs.buffer_memory(),
            metadata,
            output_data,
            original_text_bytes: self.original_text.len(),
            parse_options_path_bytes: self.parse_options.path.len(),
            source_infos: self.outputs.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappedSourceBundleMemory {
    pub inline_bytes: usize,
    pub outputs: AstBufferMemory,
    pub metadata: AstBufferMemory,
    pub output_data: AstBufferMemory,
    /// Shared payloads, excluding Arc control blocks. Count separately.
    pub original_text_bytes: usize,
    pub parse_options_path_bytes: usize,
    /// Inspect each info's memory separately, deduplicating shared data.
    pub source_infos: usize,
}

impl MappedSourceBundleMemory {
    pub fn buffers(self) -> AstBufferMemory {
        self.outputs + self.metadata + self.output_data
    }
}

/// Retains one output's current metadata and the original mapping bundle.
///
/// `SourceFile` updates can replace current metadata while keeping the original
/// canonical/supplemental links, as the reference clone does.
#[derive(Clone)]
pub struct MappedSourceFile {
    bundle: Arc<MappedSourceBundle>,
    source: Arc<SourceFileInfo>,
    index: usize,
}

impl MappedSourceFile {
    pub fn view(&self) -> MappedSourceFileView<'_> {
        MappedSourceFileView { bundle: &self.bundle, source: &self.source, index: self.index }
    }

    /// Copy mapper information for a `SourceFile` update without rebasing its links.
    /// The reference clone keeps the same source text; reject a changed text version.
    pub fn copy_for(&self, source: Arc<SourceFileInfo>) -> Result<Self, SourceFileInfoError> {
        if !Arc::ptr_eq(source.source(), self.source.source())
            && source.text() != self.source.text()
        {
            return Err(SourceFileInfoError::DifferentSourceText {
                original: self.source.id(),
                replacement: source.id(),
            });
        }
        Ok(Self { bundle: Arc::clone(&self.bundle), source, index: self.index })
    }
}

#[derive(Clone, Copy)]
pub struct MappedSourceFileView<'a> {
    bundle: &'a Arc<MappedSourceBundle>,
    source: &'a Arc<SourceFileInfo>,
    index: usize,
}

impl<'a> MappedSourceFileView<'a> {
    pub fn source_info(self) -> &'a SourceFileInfo {
        self.source
    }

    pub fn retain(self) -> MappedSourceFile {
        MappedSourceFile {
            bundle: Arc::clone(self.bundle),
            source: Arc::clone(self.source),
            index: self.index,
        }
    }

    pub fn canonical_source_file(self) -> Option<Self> {
        (self.index != 0).then(|| Self {
            bundle: self.bundle,
            source: &self.bundle.outputs[0].source,
            index: 0,
        })
    }

    pub fn supplemental_source_files(self) -> impl ExactSizeIterator<Item = Self> + 'a {
        let outputs = if self.index == 0 { &self.bundle.outputs[1..] } else { &[] };
        outputs.iter().enumerate().map(move |(index, output)| Self {
            bundle: self.bundle,
            source: &output.source,
            index: index + 1,
        })
    }

    pub fn original_file_name(self) -> &'a str {
        self.canonical_source_file().map_or(self.source, |canonical| canonical.source).file_name()
    }

    pub fn original_text(self) -> &'a str {
        if self.bundle.content_mapper.is_empty() {
            self.source.text()
        } else {
            &self.bundle.original_text
        }
    }

    pub fn content_mapper(self) -> &'a str {
        &self.bundle.content_mapper
    }

    pub fn transform_identity(self) -> &'a str {
        &self.bundle.transform_identity
    }

    pub fn content_mapper_parse_options(self) -> &'a SourceFileParseOptions {
        &self.bundle.parse_options
    }

    pub fn virtual_file_name(self) -> &'a str {
        &self.bundle.outputs[self.index].virtual_file_name
    }

    pub fn span_map(self) -> Option<&'a SpanMap> {
        self.bundle.outputs[self.index].span_map.as_deref()
    }

    pub fn diagnostic_directives(self) -> &'a [OwnedMappedDiagnosticDirective] {
        &self.bundle.outputs[self.index].diagnostic_directives
    }

    pub fn is_content_mapper_failure_stub(self) -> bool {
        !self.content_mapper().is_empty() && self.span_map().is_none()
    }
}

impl SourceFileLike for MappedSourceFileView<'_> {
    fn text(&self) -> &str {
        self.source.text()
    }

    fn ecma_line_map(&self) -> &[u32] {
        self.source.ecma_line_map()
    }
}
