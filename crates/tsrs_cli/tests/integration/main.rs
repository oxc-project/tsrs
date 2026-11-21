// The binary's allocator (src/main.rs), so the in-process API builds and their RSS checks run as they do in `tsrs`.
#[cfg(not(any(feature = "system-alloc", miri)))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

mod api_contentmapper;
mod api_memory_test;
mod api_test;
mod base_type_cycle_entry;
mod broken_pipe;
mod build_free_programs;
mod build_project_path_trailing_slash;
mod buildinfo_name_config_drive_stem;
mod buildinfo_root_empty_array_omitted;
mod buildinfo_tuple_array_length;
mod common_source_dir_config_dir;
mod default_emit;
mod deferred_type_argument_check;
mod document_identifier_trailing_slash;
mod document_uri_trailing_slash_not_normalized;
mod flow_memo;
mod free_leaf_files;
mod generate_trace;
mod include_rooted_dot_segments;
mod invalid_utf8_argument;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod isolated;
mod lazy_dts_build;
mod lint_mode;
mod lone_surrogate_path_valid_utf8;
mod ls_resolve_file_not_normalized;
mod lsp_edit_resolutions;
mod maproot_not_resolved;
mod merged_interface_check_site;
mod npm_lib_path;
mod overlay_files_missing_from_path_completions;
mod parse_command_line_file_names_not_rooted;
mod process_ending_index_directory;
mod program_path_case_sensitivity;
mod recycle_checkers;
mod reference_path_resolve_file;
mod server_signals;
mod show_config;
mod split_check;
mod symbol_name_literal_clone;
mod trace_resolution_order;
mod union_too_complex_cross_product;
mod unsupported_commands;
mod watch_b_stale_reference_config;
mod watch_macos;
