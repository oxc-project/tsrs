use std::fmt;

use bitflags::bitflags;
use tsrs_ast::Diagnostic;
use tsrs_core::tspath;
use tsrs_core::{CompilerOptions, P, ResolutionMode};
use tsrs_vfs::FS;

use crate::cache::ResolutionData;
use crate::resolver::DiagAndArgs;

pub trait ResolutionHost: Send + Sync {
    fn fs(&self) -> &dyn FS;
    fn get_current_directory(&self) -> &str;
}

pub trait Resolver: Send + Sync {
    fn resolve_module_name(
        &self,
        module_name: &str,
        containing_file: &str,
        resolution_mode: ResolutionMode,
        redirected_reference: Option<&dyn ResolvedProjectReference>,
    ) -> Result<(P<ResolvedModule>, Vec<DiagAndArgs>), String>;
    fn resolve_module_name_from_directory(
        &self,
        module_name: &str,
        containing_directory: &str,
        resolution_mode: ResolutionMode,
    ) -> Result<(P<ResolvedModule>, Vec<DiagAndArgs>), String>;
    fn resolve_type_reference_directive(
        &self,
        type_reference_directive_name: &str,
        containing_file: &str,
        resolution_mode: ResolutionMode,
        redirected_reference: Option<&dyn ResolvedProjectReference>,
    ) -> (P<ResolvedTypeReferenceDirective>, Vec<DiagAndArgs>);
    fn get_resolution_data(&self) -> P<ResolutionData>;
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ModeAwareCacheKey {
    pub name: &'static str,
    pub mode: ResolutionMode,
}

pub trait ResolvedProjectReference: Send + Sync {
    fn config_name(&self) -> &str;
    fn compiler_options(&self) -> Option<P<CompilerOptions>>;
}

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct NodeResolutionFeatures: u32 {
        const IMPORTS = 1 << 0;
        const SELF_NAME = 1 << 1;
        const EXPORTS = 1 << 2;
        const EXPORTS_PATTERN_TRAILERS = 1 << 3;
        // allowing `#/` root imports in package.json imports field
        // not supported until mass adoption - https://github.com/nodejs/node/pull/60864
        const IMPORTS_PATTERN_ROOT = 1 << 4;

        const NONE = 0;
        const ALL = Self::IMPORTS.bits() | Self::SELF_NAME.bits() | Self::EXPORTS.bits() | Self::EXPORTS_PATTERN_TRAILERS.bits() | Self::IMPORTS_PATTERN_ROOT.bits();
        const NODE16_DEFAULT = Self::IMPORTS.bits() | Self::SELF_NAME.bits() | Self::EXPORTS.bits() | Self::EXPORTS_PATTERN_TRAILERS.bits();
        const NODE_NEXT_DEFAULT = Self::ALL.bits();
        const BUNDLER_DEFAULT = Self::IMPORTS.bits() | Self::SELF_NAME.bits() | Self::EXPORTS.bits() | Self::EXPORTS_PATTERN_TRAILERS.bits() | Self::IMPORTS_PATTERN_ROOT.bits();
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct PackageId {
    pub name: &'static str,
    pub sub_module_name: &'static str,
    pub version: &'static str,
    pub peer_dependencies: &'static str,
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}{}", self.package_name(), self.version, self.peer_dependencies)
    }
}

impl PackageId {
    pub fn package_name(&self) -> String {
        if !self.sub_module_name.is_empty() {
            return format!("{}/{}", self.name, self.sub_module_name);
        }
        self.name.to_string()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedModule {
    pub resolution_diagnostics: &'static [P<Diagnostic>],
    pub resolved_file_name: &'static str,
    pub original_path: &'static str,
    pub extension: &'static str,
    pub resolved_using_ts_extension: bool,
    pub resolved_using_extra_extensions: bool,
    pub package_id: PackageId,
    pub is_external_library_import: bool,
    pub is_custom_resolution: bool,
    pub alternate_result: &'static str,
}

impl ResolvedModule {
    pub fn is_resolved(&self) -> bool {
        !self.resolved_file_name.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedTypeReferenceDirective {
    pub resolution_diagnostics: &'static [P<Diagnostic>],
    pub primary: bool,
    pub resolved_file_name: &'static str,
    pub original_path: &'static str,
    pub package_id: PackageId,
    pub is_external_library_import: bool,
}

impl ResolvedTypeReferenceDirective {
    pub fn is_resolved(&self) -> bool {
        !self.resolved_file_name.is_empty()
    }
}

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub(crate) struct Extensions: u32 {
        const TYPE_SCRIPT = 1 << 0;
        const JAVA_SCRIPT = 1 << 1;
        const DECLARATION = 1 << 2;
        const JSON = 1 << 3;

        const IMPLEMENTATION_FILES = Self::TYPE_SCRIPT.bits() | Self::JAVA_SCRIPT.bits();
    }
}

impl fmt::Display for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result: Vec<&str> = Vec::with_capacity(self.bits().count_ones() as usize);
        if self.intersects(Extensions::TYPE_SCRIPT) {
            result.push("TypeScript");
        }
        if self.intersects(Extensions::JAVA_SCRIPT) {
            result.push("JavaScript");
        }
        if self.intersects(Extensions::DECLARATION) {
            result.push("Declaration");
        }
        if self.intersects(Extensions::JSON) {
            result.push("JSON");
        }
        f.write_str(&result.join(", "))
    }
}

impl Extensions {
    pub(crate) fn array(self) -> Vec<&'static str> {
        let mut result = Vec::new();
        if self.intersects(Extensions::TYPE_SCRIPT) {
            result.extend_from_slice(tspath::SUPPORTED_TS_IMPLEMENTATION_EXTENSIONS);
        }
        if self.intersects(Extensions::JAVA_SCRIPT) {
            result.extend_from_slice(tspath::SUPPORTED_JS_EXTENSIONS_FLAT);
        }
        if self.intersects(Extensions::DECLARATION) {
            result.extend_from_slice(tspath::SUPPORTED_DECLARATION_EXTENSIONS);
        }
        if self.intersects(Extensions::JSON) {
            result.push(tspath::EXTENSION_JSON);
        }
        result
    }
}
