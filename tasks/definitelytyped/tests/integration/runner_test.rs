use tsrs_definitelytyped::runner::*;

#[test]
fn versions_and_shards_stay_together() {
    assert!(is_version_directory("v2"));
    assert!(is_version_directory("v2.1"));
    assert!(!is_version_directory("v2.x"));
    let manifest = Manifest {
        dt_commit: String::new(),
        typescript_commit: String::new(),
        packages: vec!["a".into(), "a/v1".into(), "b".into()],
    };
    assert_eq!(shard_packages(&manifest, 0, 2).len(), 2);
    assert_eq!(shard_packages(&manifest, 1, 2).len(), 1);
}
