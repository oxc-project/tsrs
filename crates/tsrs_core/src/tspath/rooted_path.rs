use super::{
    EncodedRootLength, get_encoded_root_length, get_normalized_absolute_path,
    get_path_from_path_components, get_root_length, has_trailing_directory_separator,
    is_encoded_dynamic_file_name, path_components, path_components_relative_to, path_is_absolute,
    trim_file_path_prefix,
};
use crate::stringutil::equate_string_case_insensitive;

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

// rooted_path.go:746 `RootedDirectoryPath.ResolveRelativeFile`. Unlike `resolve_file`, it appends `path` as text, so a
// first segment such as `c:` or `^` stays below `directory`. Go also panics for an empty directory and for a URL query
// or fragment; those assertions are not ported.
pub fn resolve_relative_file(directory: &str, path: &str) -> String {
    if path.is_empty() {
        return directory.to_string();
    }
    // relative_path.go:70 `requiresResolution`
    if path == ".." || path.starts_with("../") || has_trailing_directory_separator(path) {
        return to_rooted_path(path, directory);
    }
    append_path_to_directory(directory, path)
}

// rooted_path.go:853
fn append_path_to_directory(directory: &str, path: &str) -> String {
    if has_trailing_directory_separator(directory) {
        format!("{directory}{path}")
    } else {
        format!("{directory}/{path}")
    }
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

// rooted_path.go:563 `RelativeFilePathFromDirectory`: the part of `child` below `parent`, without the separator between
// them, or `None` if `child` is not `parent` and not below it. Go calls `RelativePathWithinDirectory` and
// `trimContainedPath` (rooted_path.go:575), which differ from this function only in the Go type.
pub fn relative_file_path_from_directory<'a>(
    parent: &str,
    child: &'a str,
    use_case_sensitive_file_names: bool,
) -> Option<&'a str> {
    if parent.is_empty() || child.is_empty() {
        return None;
    }
    let parent_root_length = get_root_length(parent);
    let child_root_length = get_root_length(child);
    let parent_root = &parent[..parent_root_length];
    let child_root = &child[..child_root_length];
    let dynamic = is_encoded_dynamic_file_name(parent) || is_encoded_dynamic_file_name(child);
    let roots_equal = if dynamic {
        parent_root.strip_suffix('/').unwrap_or(parent_root)
            == child_root.strip_suffix('/').unwrap_or(child_root)
    } else {
        equate_string_case_insensitive(parent_root, child_root)
    };
    if !roots_equal {
        return None;
    }
    let relative = trim_file_path_prefix(
        &child[child_root_length..],
        &parent[parent_root_length..],
        dynamic || use_case_sensitive_file_names,
    )?;
    if !relative.is_empty()
        && !has_trailing_directory_separator(parent)
        && parent.len() != parent_root_length
        && !relative.starts_with('/')
    {
        return None;
    }
    Some(relative.strip_prefix('/').unwrap_or(relative))
}

// rooted_path.go:481 `RootedFilePath.SplitAtLastComponent`: the path before and through the last whole component
// `component`, or `None` if no component is equal to it. A component that only starts with it (`node_modules_x`) does
// not match. Go also panics for a component that is empty, `.`, `..`, or has a separator; those assertions are not
// ported.
pub fn split_at_last_component<'a>(path: &'a str, component: &str) -> Option<(&'a str, &'a str)> {
    let needle = format!("/{component}");
    let root_length = get_root_length(path);
    let mut end = path.len();
    while end > 0 {
        let index = path[..end].rfind(&needle)?;
        let component_end = index + needle.len();
        if index + 1 >= root_length
            && (component_end == path.len() || path.as_bytes()[component_end] == b'/')
        {
            return Some((&path[..index.max(root_length)], &path[..component_end]));
        }
        end = index;
    }
    None
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
