// The API accepts path-valued compiler options as strings. Go's
// RawCompilerOptions.Finalize resolves them against the server cwd before
// constructing CompilerOptions; the JSON representation alone does not.

use tsrs_core::CompilerOptions;
use tsrs_core::json::Value;
use tsrs_core::tspath;
use tsrs_tsoptions::gojson;

use crate::handler::{ApiError, ApiResult};

fn rooted_option(path: &str, cwd: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let path = if path
        .get(.."${configDir}".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("${configDir}"))
    {
        format!("./{}", &path["${configDir}".len()..])
    } else {
        path.to_string()
    };
    tspath::get_normalized_absolute_path(&path, cwd)
}

/// Go `RawCompilerOptions.Finalize`, for the path-valued fields accepted by
/// the API. This runs only on API input; parsed tsconfig options already have
/// their own base directory and must not be resolved against the server cwd.
pub fn compiler_options_from_raw(value: &Value, cwd: &str) -> ApiResult<CompilerOptions> {
    let mut options =
        gojson::compiler_options_from_go_json(value).map_err(ApiError::invalid_request)?;
    crate::predecode::exact_compiler_options_ints(value, &mut options);

    macro_rules! rooted_fields {
        ($($field:ident),* $(,)?) => {
            $(options.$field = rooted_option(&options.$field, cwd);)*
        };
    }
    rooted_fields!(
        declaration_dir,
        out_dir,
        project,
        root_dir,
        ts_build_info_file,
        base_url,
        out_file,
        config_file_path,
        paths_base_path,
        generate_cpu_profile,
        generate_trace,
        pprof_dir,
    );
    for paths in [&mut options.root_dirs, &mut options.type_roots].into_iter().flatten() {
        for path in paths {
            *path = rooted_option(path, cwd);
        }
    }
    // Source-map locations are URI-like text, not rooted filesystem paths.
    options.map_root = tspath::normalize_slashes(&options.map_root);
    options.source_root = tspath::normalize_slashes(&options.source_root);
    Ok(options)
}

#[cfg(test)]
mod tests {
    use tsrs_core::json;

    use super::compiler_options_from_raw;

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
}
