use tsrs_core::tspath::Path;

use tsrs_execute::build::buildtask::is_content_mapper_supplemental_build_info_path;

// buildtask_contentmapper_test.go:11. A build info file entry that names a supplemental output of a root file
// (`<root>.<n>.<ext>`) is skipped when the file is gone, instead of making the build out of date; anything else
// must still be checked.
#[test]
fn test_is_content_mapper_supplemental_build_info_path() {
    let roots = [Path::new("/src/app.vue"), Path::new("/src/index.ts")];
    let path = Path::new;

    assert!(is_content_mapper_supplemental_build_info_path(&path("/src/app.vue.0.ts"), &roots));
    assert!(is_content_mapper_supplemental_build_info_path(&path("/src/app.vue.12.mts"), &roots));
    assert!(!is_content_mapper_supplemental_build_info_path(&path("/src/app.vue.ts"), &roots));
    assert!(!is_content_mapper_supplemental_build_info_path(&path("/src/app.vue.0.txt"), &roots));
    assert!(!is_content_mapper_supplemental_build_info_path(&path("/src/other.vue.0.ts"), &roots));
}
