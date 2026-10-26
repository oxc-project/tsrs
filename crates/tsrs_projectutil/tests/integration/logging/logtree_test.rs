use tsrs_projectutil::logging::logger::Logger;

use tsrs_projectutil::logging::logtree::*;

// logtree_test.go:12 TestLogTreeImplementsLogger
#[test]
fn log_tree_implements_logger() {
    let tree = new_log_tree("test");
    let _: &dyn Logger = &tree;
}

// logtree_test.go:17 TestLogTree (empty in Go); tsrs-only: the rendering of nested forks.
#[test]
fn log_tree() {
    let tree = new_log_tree("root");
    tree.log("a");
    let child = tree.fork("child");
    child.log("b");
    let s = tree.string();
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines[0], "======== root ========");
    assert!(lines[1].ends_with(" a"));
    assert!(lines[2].ends_with(" child"));
    assert!(lines[3].starts_with('\t') && lines[3].ends_with(" b"));
    LogTree::nil().fork("x").log("ignored");
}
