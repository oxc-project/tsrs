use tsrs_testrunner::test_case_parser::*;

#[test]
fn test_make_units_from_test() {
    let code = "// @strict: true
// @noEmit: true
// @filename: firstFile.ts
function foo() { return \"a\"; }
// normal comment
// @filename: secondFile.ts
// some other comment
function bar() { return \"b\"; }";
    let result = make_units_from_test(code, "simpleTest.ts");
    assert_eq!(
        result.test_unit_data,
        vec![
            TestUnit {
                content: "function foo() { return \"a\"; }\n// normal comment".to_string(),
                name: "firstFile.ts".to_string()
            },
            TestUnit {
                content: "// some other comment\nfunction bar() { return \"b\"; }".to_string(),
                name: "secondFile.ts".to_string()
            },
        ]
    );
    assert_eq!(result.ts_config_file_unit_data, None);
    assert!(result.symlinks.is_empty());
}

#[test]
fn test_extract_settings() {
    let s = extract_compiler_settings(
        "// @Target: ES5, ES2015;\r\n//@strict:true\n// @filename: a.ts\nfoo",
    );
    assert_eq!(s.get("target").map(String::as_str), Some("ES5, ES2015"));
    assert_eq!(s.get("strict").map(String::as_str), Some("true"));
    assert_eq!(s.get("filename").map(String::as_str), Some("a.ts"));
}

#[test]
fn test_symlinks() {
    let code = "// @filename: /a/b.ts\nexport {}\n// @link: /a -> /c\n// @filename: /x.ts\n// @symlink: /y.ts, /z.ts\nlet x";
    let result = make_units_from_test(code, "t.ts");
    assert_eq!(result.symlinks.get("/c").map(String::as_str), Some("/a"));
    assert_eq!(result.symlinks.get("/y.ts").map(String::as_str), Some("/x.ts"));
    assert_eq!(result.symlinks.get("/z.ts").map(String::as_str), Some("/x.ts"));
}
