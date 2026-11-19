// Ports of the typed relative-path assertions of Go's tspath/typed_paths_test.go (CaseSensitivity.RelativePathFromPath,
// RelativePathFromDirectory and RelativePathFromFile).

use tsrs_core::tspath::{relative_path_from_file_to_path, relative_path_from_path};

const CASE_SENSITIVE: bool = true;
const CASE_INSENSITIVE: bool = false;

#[test]
fn encoded_dynamic_paths_preserve_opaque_identity() {
    assert_eq!(
        relative_path_from_path(
            "^/~ts-uri~/custom/Authority/src",
            "^/~ts-uri~/custom/authority/lib/x.ts",
            CASE_INSENSITIVE,
        ),
        None
    );
}

// Under a common encoded dynamic root, the root matches with or without its trailing separator and the other
// components compare case-sensitively. The expected values are Go's on the pinned commit.
#[test]
fn encoded_dynamic_paths_with_a_common_root() {
    assert_eq!(
        relative_path_from_path(
            "^/~ts-uri~/custom/Authority/SRC",
            "^/~ts-uri~/custom/Authority/src/x.ts",
            CASE_INSENSITIVE,
        )
        .as_deref(),
        Some("../src/x.ts")
    );
    assert_eq!(
        relative_path_from_path(
            "^/~ts-uri~/custom/Authority",
            "^/~ts-uri~/custom/Authority/x.ts",
            CASE_SENSITIVE,
        )
        .as_deref(),
        Some("x.ts")
    );
    assert_eq!(
        relative_path_from_path(
            "^/~ts-uri~/custom/Authority/",
            "^/~ts-uri~/custom/Authority/x.ts",
            CASE_SENSITIVE,
        )
        .as_deref(),
        Some("x.ts")
    );
    assert_eq!(
        relative_path_from_file_to_path(
            "^/~ts-uri~/custom/Authority/src/index.ts",
            "^/~ts-uri~/custom/Authority/lib/x.ts",
            CASE_INSENSITIVE,
        )
        .as_deref(),
        Some("../lib/x.ts")
    );
}

#[test]
fn try_relative_path_between_file_paths() {
    assert_eq!(
        relative_path_from_path("/project/src", "/project/lib/file.ts", CASE_SENSITIVE).as_deref(),
        Some("../lib/file.ts")
    );
    assert_eq!(
        relative_path_from_path("c:/project/src", "d:/project/lib/file.ts", CASE_SENSITIVE),
        None
    );
}

#[test]
fn relative_paths_from_typed_paths() {
    assert_eq!(
        relative_path_from_path("/project/src", "/project/lib/util.ts", CASE_SENSITIVE).as_deref(),
        Some("../lib/util.ts")
    );
    assert_eq!(
        relative_path_from_file_to_path(
            "/project/src/index.ts",
            "/project/lib/util.ts",
            CASE_SENSITIVE
        )
        .as_deref(),
        Some("../lib/util.ts")
    );
    assert_eq!(
        relative_path_from_path("/PROJECT/src", "/project/lib/util.ts", CASE_INSENSITIVE)
            .as_deref(),
        Some("../lib/util.ts")
    );
}
