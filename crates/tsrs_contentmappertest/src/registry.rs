use std::sync::Arc;

use crate::component::ComponentHandler;
use crate::diagnostic_code_collision::DiagnosticCodeCollisionHandler;
use crate::duplicate::DuplicateHandler;
use crate::duplicate_projection::DuplicateProjectionHandler;
use crate::dynamic_verbatim::{DynamicVerbatimHandler, ProjectLifecycle};
use crate::editing::{PrefixedSupplementalHandler, UnmappedFoldingHandler};
use crate::failing::FailingHandler;
use crate::hoisting::HoistingHandler;
use crate::lisp::LispHandler;
use crate::protocol::TestHandler;
use crate::supplemental::SupplementalHandler;
use crate::supplemental_diagnostics::SupplementalDiagnosticsHandler;
use crate::supplemental_globals::SupplementalGlobalsHandler;
use crate::supplemental_module::SupplementalModuleHandler;
use crate::synthesizing::SynthesizingHandler;
use crate::transforming::Handler;
use crate::verbatim::{ModuleVerbatimHandler, VerbatimHandler};

// registry.go:10
pub const TRANSFORMING_MAPPER: &str = "compiler-test-mapper";
pub const VERBATIM_MAPPER: &str = "verbatim-mapper";
pub const MODULE_VERBATIM_MAPPER: &str = "module-verbatim-mapper";
pub const DYNAMIC_VERBATIM_MAPPER: &str = "dynamic-verbatim-mapper";
pub const DIAGNOSTIC_CODE_COLLISION_MAPPER: &str = "diagnostic-code-collision-mapper";
pub const FAILING_MAPPER: &str = "failing-mapper";
pub const SYNTHESIZING_MAPPER: &str = "synthesizing-mapper";
pub const COMPONENT_MAPPER: &str = "component-mapper";
pub const DUPLICATE_MAPPER: &str = "duplicate-mapper";
pub const LISP_MAPPER: &str = "lisp-mapper";
pub const SUPPLEMENTAL_MAPPER: &str = "supplemental-mapper";
pub const SUPPLEMENTAL_DIAGNOSTICS_MAPPER: &str = "supplemental-diagnostics-mapper";
pub const SUPPLEMENTAL_GLOBALS_MAPPER: &str = "supplemental-globals-mapper";
pub const SUPPLEMENTAL_MODULE_MAPPER: &str = "supplemental-module-mapper";
pub const PREFIXED_SUPPLEMENTAL_MAPPER: &str = "prefixed-supplemental-mapper";
pub const UNMAPPED_FOLDING_MAPPER: &str = "unmapped-folding-mapper";
pub const HOISTING_MAPPER: &str = "hoisting-mapper";
pub const DUPLICATE_PROJECTION_MAPPER: &str = "duplicate-projection-mapper";

// registry.go:31
type HandlerConstructor = fn(Option<Arc<ProjectLifecycle>>) -> Arc<dyn TestHandler>;

// registry.go:33 (Go's map; looked up by name.)
static MAPPER_HANDLERS: [(&str, HandlerConstructor); 18] = [
    (TRANSFORMING_MAPPER, |_| Arc::new(Handler::default())),
    (VERBATIM_MAPPER, |_| Arc::new(VerbatimHandler)),
    (MODULE_VERBATIM_MAPPER, |_| Arc::new(ModuleVerbatimHandler)),
    (DYNAMIC_VERBATIM_MAPPER, |lifecycle| Arc::new(DynamicVerbatimHandler { lifecycle })),
    (DIAGNOSTIC_CODE_COLLISION_MAPPER, |_| Arc::new(DiagnosticCodeCollisionHandler)),
    (FAILING_MAPPER, |_| Arc::new(FailingHandler)),
    (SYNTHESIZING_MAPPER, |_| Arc::new(SynthesizingHandler)),
    (COMPONENT_MAPPER, |_| Arc::new(ComponentHandler)),
    (DUPLICATE_MAPPER, |_| Arc::new(DuplicateHandler)),
    (LISP_MAPPER, |_| Arc::new(LispHandler)),
    (SUPPLEMENTAL_MAPPER, |_| Arc::new(SupplementalHandler)),
    (SUPPLEMENTAL_DIAGNOSTICS_MAPPER, |_| Arc::new(SupplementalDiagnosticsHandler)),
    (SUPPLEMENTAL_GLOBALS_MAPPER, |_| Arc::new(SupplementalGlobalsHandler)),
    (SUPPLEMENTAL_MODULE_MAPPER, |_| Arc::new(SupplementalModuleHandler)),
    (PREFIXED_SUPPLEMENTAL_MAPPER, |_| Arc::new(PrefixedSupplementalHandler)),
    (UNMAPPED_FOLDING_MAPPER, |_| Arc::new(UnmappedFoldingHandler)),
    (HOISTING_MAPPER, |_| Arc::new(HoistingHandler)),
    (DUPLICATE_PROJECTION_MAPPER, |_| Arc::new(DuplicateProjectionHandler)),
];

// registry.go:54
pub fn handler_for_mapper(
    command: &[String],
    lifecycle: Option<Arc<ProjectLifecycle>>,
) -> Result<Arc<dyn TestHandler>, String> {
    if command.is_empty() {
        return Err("contentmappertest: empty mapper command".to_string());
    }
    let Some((_, constructor)) = MAPPER_HANDLERS.iter().find(|(name, _)| *name == command[0])
    else {
        // fmt's %v of a []string.
        return Err(format!("contentmappertest: unknown mapper command [{}]", command.join(" ")));
    };
    Ok(constructor(lifecycle))
}
