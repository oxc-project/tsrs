mod batch_test;
mod build_test;
mod callbackfs_test;
mod checker {
    pub mod coverage_table;
    pub mod coverage_types;
    mod lease_tests;
    mod lifetime_tests;
    mod session_tests;
}
mod checker_test;
mod common;
mod config_memory_test;
mod config_test;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod isolated;
mod memory_test;
mod merged_interface_test;
mod module_resolution_test;
mod predecode_test;
mod printing_test;
mod program_test;
mod raw_options_test;
mod requestfs_differential;
mod requestfs_test;
mod session_apistate_test;
mod sourcefile_test;
mod transpile_test;
