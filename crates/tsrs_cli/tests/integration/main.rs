// The binary's allocator (src/main.rs), so the in-process API builds and their RSS checks run as they do in `tsrs`.
#[cfg(not(any(feature = "system-alloc", miri)))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

mod api_memory_test;
mod api_test;
mod base_type_cycle_entry;
mod broken_pipe;
mod build_free_programs;
mod default_emit;
mod deferred_type_argument_check;
mod flow_memo;
mod free_leaf_files;
mod generate_trace;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod isolated;
mod lazy_dts_build;
mod lsp_edit_resolutions;
mod merged_interface_check_site;
mod npm_lib_path;
mod recycle_checkers;
mod server_signals;
mod split_check;
mod symbol_name_literal_clone;
mod trace_resolution_order;
mod union_too_complex_cross_product;
mod watch_macos;
