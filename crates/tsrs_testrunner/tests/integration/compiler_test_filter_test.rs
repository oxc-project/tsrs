use rustc_hash::FxHashSet;

use tsrs_testrunner::compiler_test_filter::CompilerTestFileFilter;

#[test]
fn filters_plain_uppercase_names_before_expansion() {
    let filter = CompilerTestFileFilter::new("compiler", Some("compiler/FlakyDiagnostic"), None);
    assert!(filter.includes("FlakyDiagnostic1038.ts"));
    assert!(!filter.includes("other.ts"));
    assert!(filter.includes("FlakyDiagnostic1038.tsx"));
    assert!(
        !CompilerTestFileFilter::new("conformance", Some("compiler/FlakyDiagnostic"), None)
            .includes("FlakyDiagnostic1038.ts")
    );
}

#[test]
fn ambiguous_patterns_fall_back_to_reading_the_file() {
    for pattern in [
        "target",
        "^compiler/lowercase",
        "compiler/Example\\.ts",
        "compiler/(Example|Other)",
        "compiler/Example(",
        "compiler/Example Name",
    ] {
        assert!(
            CompilerTestFileFilter::new("compiler", Some(pattern), None).includes("other.ts"),
            "{pattern}"
        );
    }
}

#[test]
fn list_keeps_file_and_configuration_names() {
    for entry in [
        "Example.ts",
        "Example",
        "compiler/Example",
        "Example(target=es2020)",
        "compiler/Example(target=es2020)",
    ] {
        let list = FxHashSet::from_iter([entry.to_string()]);
        let filter = CompilerTestFileFilter::new("compiler", None, Some(&list));
        assert!(filter.includes("Example.ts"), "{entry}");
        assert!(!filter.includes("other.ts"), "{entry}");
    }
}
