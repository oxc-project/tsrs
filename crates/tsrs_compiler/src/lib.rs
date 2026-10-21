mod affinity;
mod checker_program;
mod checkerpool;
#[cfg(test)]
mod contentmapper_test;
pub mod diagnosticwriter;
mod emithost;
mod emitter;
mod file_include;
mod fileloader;
mod fileregions;
mod filesparser;
mod host;
mod includeprocessor;
#[cfg(test)]
mod modulespecifiers_oracle_test;
mod outputpaths;
mod processing_diagnostic;
mod program;
mod program_emit;
#[cfg(test)]
mod program_test;
mod programlike;
mod programregions;
mod projectreferencedtsfakinghost;
mod projectreferencefilemapper;
mod projectreferenceparser;
mod sharedgraph;
mod splitcheck;

pub use checkerpool::{
    Checker, CheckerHandle, CheckerPool, Context, PooledChecker, checker_count_upper_bound,
    parse_memory_size, set_checker_assignment_from_cli, set_checker_cost_cache_from_cli,
    set_max_memory_from_cli, use_go_default_checker_count,
};
pub use emitter::EmitOnly;
pub use file_include::FileIncludeReason;
pub use fileloader::{
    DuplicateSourceFile, LibFile, content_mapper_initialization_diagnostic,
    content_mapper_project_diagnostic, content_mapper_project_error_diagnostic,
};
pub use fileregions::{
    LeafMode, LeafSettings, bind_in_file_regions, enable as enable_file_regions, enable_lazy_dts,
    lazy_dts_allowed, leaf_settings_from_env, stats_report as leaf_stats_report,
};
pub use host::{
    CompilerHost, TraceFn, get_content_mapped_source_files_with, new_cached_fs_compiler_host,
    new_compiler_host,
};
pub use program::{
    CreateCheckerPool, CreateModuleResolver, Program, ProgramBox, ProgramConfig, ProgramOptions,
    filter_no_emit_semantic_diagnostics, get_diagnostics_of_any_program, new_program,
    sort_and_deduplicate_diagnostics, worker_pool,
};
pub use program_emit::{
    EmitOptions, EmitResult, SourceMapEmitResult, WriteFile, WriteFileData, combine_emit_results,
    handle_no_emit_options,
};
pub use programlike::{ProgramLike, get_diagnostics_of_any_program_like};
pub use programregions::ProgramRegions;
pub use tsrs_checker::sitecount::dump as dump_site_counts;
