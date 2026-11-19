use super::{
    EncodedRootLength, get_directory_path_cow, get_encoded_root_length,
    get_normalized_absolute_path, get_path_from_path_components, get_root_length,
    has_trailing_directory_separator, is_encoded_dynamic_file_name, path_components,
    path_components_relative_to, path_is_absolute,
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

// relative_path.go RelativePathFromPath: `None` when the paths have different roots.
pub fn relative_path_from_path(
    directory: &str,
    path: &str,
    use_case_sensitive_file_names: bool,
) -> Option<String> {
    let relative =
        relative_path_from_normalized_paths(directory, path, use_case_sensitive_file_names);
    if get_encoded_root_length(&relative) != EncodedRootLength::Relative {
        return None;
    }
    Some(relative)
}

// relative_path.go RelativePathFromFileToPath
pub fn relative_path_from_file_to_path(
    file: &str,
    path: &str,
    use_case_sensitive_file_names: bool,
) -> Option<String> {
    relative_path_from_path(&get_directory_path_cow(file), path, use_case_sensitive_file_names)
}

// rooted_path.go relativePathFromNormalizedPaths
fn relative_path_from_normalized_paths(
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
