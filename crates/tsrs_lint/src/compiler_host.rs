//! Compiler host that selects the pinned tsgolint string escape semantics.

use std::fmt::Display;
use std::sync::Arc;

use tsrs_ast::{SourceFile, SourceFileParseOptions};
use tsrs_compiler::CompilerHost;
use tsrs_contentmapper::{self as contentmapper, Mapper, Project};
use tsrs_core::{P, tspath::Path};
use tsrs_tsoptions::ParsedCommandLine;
use tsrs_vfs::FS;

pub struct LintCompilerHost(pub Arc<dyn CompilerHost>);

impl CompilerHost for LintCompilerHost {
    fn fs(&self) -> &dyn FS {
        self.0.fs()
    }
    fn default_library_path(&self) -> &str {
        self.0.default_library_path()
    }
    fn get_current_directory(&self) -> &str {
        self.0.get_current_directory()
    }
    fn trace(&self, message: &'static tsrs_diagnostics::Message, args: &[&dyn Display]) {
        self.0.trace(message, args);
    }
    fn get_source_file(&self, mut options: SourceFileParseOptions) -> Option<P<SourceFile>> {
        options.legacy_string_escapes = true;
        self.0.get_source_file(options)
    }
    fn get_content_mapped_source_files(
        &self,
        mut options: SourceFileParseOptions,
        mapper: &Mapper,
    ) -> Result<contentmapper::SourceFiles, contentmapper::Error> {
        options.legacy_string_escapes = true;
        self.0.get_content_mapped_source_files(options, mapper)
    }
    fn content_mapper_project(&self) -> Option<Arc<dyn Project>> {
        self.0.content_mapper_project()
    }
    fn get_resolved_project_reference(
        &self,
        file_name: &str,
        path: Path,
    ) -> Option<P<ParsedCommandLine>> {
        self.0.get_resolved_project_reference(file_name, path)
    }
}
