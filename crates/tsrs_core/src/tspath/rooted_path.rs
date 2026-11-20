use super::{
    EncodedRootLength, get_encoded_root_length, get_normalized_absolute_path,
    get_path_from_path_components, get_root_length, has_trailing_directory_separator,
    is_encoded_dynamic_file_name, path_components, path_components_relative_to, path_is_absolute,
};

fn rooted_path_from_absolute(path: &str) -> String {
    try_rooted_path_from_absolute(path).unwrap_or_else(|| panic!("path must be absolute"))
}

// rooted_path.go:58
pub fn try_rooted_path_from_absolute(path: &str) -> Option<String> {
    if has_rooted_url_suffix(path) || !path_is_absolute(path) {
        return None;
    }
    Some(ensure_rooted_path_root_separator(get_normalized_absolute_path(path, "")))
}

// rooted_path.go:65
pub fn ensure_rooted_path_root_separator(mut path: String) -> String {
    if get_root_length(&path) == path.len() && !has_trailing_directory_separator(&path) {
        path.push('/');
    }
    path
}

// rooted_path.go:29 `ToRootedPath`. Unlike `resolve_path`, it removes a trailing separator, and a path that is only
// a root ends with one (`C:` becomes `C:/`). Go also panics for an empty path, a URL query or fragment, and a result
// that is not rooted; those assertions are not ported.
pub fn to_rooted_path(path: &str, current_directory: &str) -> String {
    ensure_rooted_path_root_separator(get_normalized_absolute_path(path, current_directory))
}

// rooted_path.go:727 `RootedDirectoryPath.ResolveFile`. Go first tries two shortcuts for a relative path; with a
// normalized directory they give the same result as `ToRootedFilePath`, so they are not ported. `ToRootedFilePath`,
// `ToRootedDirectoryPath`, and `ResolveDirectory` differ from these functions only in the Go type.
pub fn resolve_file(directory: &str, path: &str) -> String {
    if path.is_empty() {
        return directory.to_string();
    }
    to_rooted_path(path, directory)
}

fn has_rooted_url_suffix(path: &str) -> bool {
    if !has_url_root(path) {
        return false;
    }
    let (_, after_scheme) = path.split_once("://").unwrap();
    after_scheme.contains(['?', '#'])
}

fn has_url_root(path: &str) -> bool {
    matches!(get_encoded_root_length(path), EncodedRootLength::Url(_)) && path.contains("://")
}

pub fn rooted_directory_path_from_absolute(path: &str) -> String {
    rooted_path_from_absolute(path)
}

// relativePathFromNormalizedPaths
pub(super) fn relative_path_from_normalized_paths(
    from: &str,
    to: &str,
    mut use_case_sensitive_file_names: bool,
) -> String {
    let mut from_components = path_components(from, get_root_length(from));
    let mut to_components = path_components(to, get_root_length(to));
    if is_encoded_dynamic_file_name(from) || is_encoded_dynamic_file_name(to) {
        for components in [&mut from_components, &mut to_components] {
            if components[0].ends_with('/') {
                components[0].pop();
            }
        }
        if from_components[0] != to_components[0] {
            return to.to_string();
        }
        use_case_sensitive_file_names = true;
    }
    get_path_from_path_components(&path_components_relative_to(
        &from_components,
        to_components,
        use_case_sensitive_file_names,
    ))
}
