use tsrs_core::json;

use tsrs_api::raw_options::compiler_options_from_raw;

#[test]
fn resolves_api_path_options_against_server_cwd() {
    let input = json::unmarshal(
        r#"{"outDir":"dist","rootDirs":["src","${configDir}/generated"],"tsBuildInfoFile":"cache/build.tsbuildinfo","sourceRoot":"sources\\mapped","configFilePath":"","pathsBasePath":"config","NoImplicitAny":true}"#,
    )
    .unwrap();
    let options = compiler_options_from_raw(&input, "/project").unwrap();
    assert_eq!(options.out_dir, "/project/dist");
    assert_eq!(options.root_dirs.unwrap(), ["/project/src", "/project/generated"]);
    assert_eq!(options.ts_build_info_file, "/project/cache/build.tsbuildinfo");
    assert_eq!(options.source_root, "sources/mapped");
    assert_eq!(options.config_file_path, "");
    assert_eq!(options.paths_base_path, "/project/config");
    assert_eq!(options.no_implicit_any, tsrs_core::Tristate::Unknown);
}
