use super::{
    EncodedRootLength, get_directory_path_cow, get_encoded_root_length,
    relative_path_from_normalized_paths,
};

// RelativePathFromPath: `None` when the paths have different roots. Both are normalized rooted paths (Go's
// RootedDirectoryPath and RootedPath types); `.` and `..` components are compared as names, not resolved.
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

// RelativePathFromFileToPath: as relative_path_from_path, from the directory of `file`.
pub fn relative_path_from_file_to_path(
    file: &str,
    path: &str,
    use_case_sensitive_file_names: bool,
) -> Option<String> {
    relative_path_from_path(&get_directory_path_cow(file), path, use_case_sensitive_file_names)
}
