// Port of Go's execute/tsctests/showconfig_test.go TestShowConfig and the `configDir template showConfig` case of
// tsc_test.go TestTscExtends, compared with their baselines. These scenarios need no Go dump: they build no program,
// so none of them reads a default library.

use rustc_hash::FxHashMap;

use super::{Scenario, ScenarioFile, diff_text, repo_root, run_scenario};

const PROJECT: &str = "/home/src/workspaces/project";
const INDEX_TS: (&str, &str) = ("src/index.ts", "export const a = 1;");

// stringtestutil.Dedent for space-indented text: drops the blank lines around the text and the common indentation.
fn dedent(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let is_blank = |line: &&str| line.trim().is_empty();
    let start = lines.iter().position(|line| !is_blank(line)).unwrap();
    let end = lines.iter().rposition(|line| !is_blank(line)).unwrap();
    let lines = &lines[start..=end];
    let indentation = lines
        .iter()
        .filter(|line| !is_blank(line))
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap();
    lines
        .iter()
        .map(|line| if line.len() > indentation { &line[indentation..] } else { "" })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(baseline: String, cwd: &str, args: &[&str], files: Vec<(String, String)>) -> Scenario {
    Scenario {
        baseline,
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        cwd: cwd.to_string(),
        env: FxHashMap::default(),
        output_is_tty: None,
        ignore_case: false,
        windows_style_root: String::new(),
        files: files
            .into_iter()
            .map(|(path, content)| ScenarioFile { path, content, symlink: String::new() })
            .collect(),
        default_libs: Vec::new(),
        edits: Vec::new(),
        signature_job: false,
    }
}

fn show_config(sub_scenario: &str, args: &[&str], files: &[(&str, &str)]) -> Scenario {
    scenario(
        format!("tsc/showConfig/{}.js", sub_scenario.replace(' ', "-")),
        PROJECT,
        args,
        files
            .iter()
            .map(|(path, content)| (format!("{PROJECT}/{path}"), (*content).to_string()))
            .collect(),
    )
}

fn scenarios() -> Vec<Scenario> {
    let config_args = ["-p", "tsconfig.json", "--showConfig"];
    let compile_on_save_config = dedent(
        r#"
        {
            "compilerOptions": {
                "esModuleInterop": true,
                "target": "es5",
                "module": "commonjs",
                "strict": true
            },
            "compileOnSave": true,
            "exclude": [
                "dist"
            ],
            "files": [],
            "include": [
                "src/*"
            ],
            "references": [
                { "path": "./test" }
            ]
        }"#,
    );
    let paths_config = dedent(
        r#"
        {
            "compilerOptions": {
                "allowJs": true,
                "outDir": "./lib",
                "esModuleInterop": true,
                "module": "commonjs",
                "moduleResolution": "node",
                "target": "ES2017",
                "sourceMap": true,
                "baseUrl": ".",
                "paths": {
                    "@root/*": ["./*"],
                    "@configs/*": ["src/configs/*"],
                    "@common/*": ["src/common/*"],
                    "*": [
                        "node_modules/*",
                        "src/types/*"
                    ]
                },
                "experimentalDecorators": true,
                "emitDecoratorMetadata": true,
                "resolveJsonModule": true
            },
            "include": [
                "./src/**/*"
            ]
        }"#,
    );
    let include_config = dedent(
        r#"
        {
            "compilerOptions": {
                "strict": true
            },
            "include": [
                "src/**/*"
            ]
        }"#,
    );
    let references_config = dedent(
        r#"
        {
            "compilerOptions": {
                "composite": true,
                "strict": true
            },
            "references": [
                { "path": "./packages/a" },
                { "path": "./packages/b" }
            ]
        }"#,
    );
    let exclude_config = dedent(
        r#"
        {
            "compilerOptions": {
                "strict": true
            },
            "exclude": [
                "test"
            ]
        }"#,
    );
    let files_and_include_config = dedent(
        r#"
        {
            "compilerOptions": {
                "strict": true
            },
            "files": [
                "extra.ts"
            ],
            "include": [
                "src/**/*"
            ]
        }"#,
    );
    let implied_config = dedent(
        r#"
        {
            "compilerOptions": {
                "module": "nodenext"
            }
        }"#,
    );
    let exclude_and_out_dir_config = dedent(
        r#"
        {
            "compilerOptions": {
                "strict": true,
                "outDir": "./build"
            },
            "exclude": [
                "build"
            ]
        }"#,
    );

    let mut scenarios = vec![
        show_config("Default initialized TSConfig", &["--showConfig"], &[]),
        show_config(
            "Show TSConfig with files options",
            &["--showConfig", "file0.ts", "file1.ts", "file2.ts"],
            &[],
        ),
        show_config(
            "Show TSConfig with boolean value compiler options",
            &["--showConfig", "--noUnusedLocals"],
            &[],
        ),
        show_config(
            "Show TSConfig with enum value compiler options",
            &["--showConfig", "--target", "es5", "--jsx", "react"],
            &[],
        ),
        show_config(
            "Show TSConfig with list compiler options",
            &["--showConfig", "--types", "jquery,mocha"],
            &[],
        ),
        show_config(
            "Show TSConfig with list compiler options with enum value",
            &["--showConfig", "--lib", "es5,es2015.core"],
            &[],
        ),
        show_config(
            "Show TSConfig with incorrect compiler option",
            &["--showConfig", "--someNonExistOption"],
            &[],
        ),
        show_config(
            "Show TSConfig with incorrect compiler option value",
            &["--showConfig", "--lib", "nonExistLib,es5,es2015.promise"],
            &[],
        ),
        show_config(
            "Show TSConfig with advanced options",
            &[
                "--showConfig",
                "--declaration",
                "--declarationDir",
                "lib",
                "--skipLibCheck",
                "--noErrorTruncation",
            ],
            &[],
        ),
        show_config(
            "Show TSConfig with compileOnSave and more",
            &config_args,
            &[INDEX_TS, ("tsconfig.json", &compile_on_save_config)],
        ),
        show_config(
            "Show TSConfig with paths and more",
            &config_args,
            &[INDEX_TS, ("tsconfig.json", &paths_config)],
        ),
        show_config(
            "Show TSConfig with include filtering files",
            &config_args,
            &[
                ("src/main.ts", "export const a = 1;"),
                ("src/util.ts", "export const b = 2;"),
                ("extra.ts", "export const c = 3;"),
                ("tsconfig.json", &include_config),
            ],
        ),
        show_config(
            "Show TSConfig with references",
            &config_args,
            &[INDEX_TS, ("tsconfig.json", &references_config)],
        ),
        show_config(
            "Show TSConfig with exclude",
            &config_args,
            &[
                INDEX_TS,
                ("test/test1.ts", r#"import { a } from "../src";"#),
                ("tsconfig.json", &exclude_config),
            ],
        ),
        show_config(
            "Show TSConfig with files and include",
            &config_args,
            &[
                ("src/main.ts", "export const a = 1;"),
                ("extra.ts", "export const c = 3;"),
                ("tsconfig.json", &files_and_include_config),
            ],
        ),
        show_config(
            "Show TSConfig with transitively implied options",
            &config_args,
            &[INDEX_TS, ("tsconfig.json", &implied_config)],
        ),
        show_config(
            "Show TSConfig with exclude and outDir",
            &config_args,
            &[
                INDEX_TS,
                ("src/bin/tool.ts", "export const b = 2;"),
                ("tsconfig.json", &exclude_and_out_dir_config),
            ],
        ),
    ];

    for value in ["true", "false"] {
        scenarios.push(show_config(
            &format!("Show TSConfig with command line file listing flags {value}"),
            &[
                "--showConfig",
                "--listFiles",
                value,
                "--listEmittedFiles",
                value,
                "--listFilesOnly",
                value,
                "--explainFiles",
                value,
            ],
            &[INDEX_TS, ("tsconfig.json", r#"{"files": ["src/index.ts"]}"#)],
        ));
        let config = dedent(&format!(
            r#"
            {{
                "compilerOptions": {{
                    "listFiles": {value},
                    "listEmittedFiles": {value},
                    "explainFiles": {value}
                }},
                "files": ["src/index.ts"]
            }}"#
        ));
        scenarios.push(show_config(
            &format!("Show TSConfig with configured file listing flags {value}"),
            &["--showConfig"],
            &[INDEX_TS, ("tsconfig.json", &config)],
        ));
    }

    let config_dir_files = [
        (
            "/home/src/projects/configs/first/tsconfig.json",
            r#"
            {
                "extends": "../second/tsconfig.json",
                "include": ["${configDir}/src"],
                "compilerOptions": {
                    "typeRoots": ["root1", "${configDir}/root2", "root3"],
                    "types": [],
                },
            }"#,
        ),
        (
            "/home/src/projects/configs/second/tsconfig.json",
            r#"
            {
                "files": ["${configDir}/main.ts"],
                "compilerOptions": {
                    "declarationDir": "${configDir}/decls",
                    "paths": {
                        "@myscope/*": ["${configDir}/types/*"],
                    },
                },
            }"#,
        ),
        (
            "/home/src/projects/myproject/tsconfig.json",
            r#"
            {
                "extends": "../configs/first/tsconfig.json",
                "compilerOptions": {
                    "declaration": true,
                    "outDir": "outDir",
                    "traceResolution": true,
                },
            }"#,
        ),
        (
            "/home/src/projects/myproject/main.ts",
            r#"
                // some comment
                export const y = 10;
                import { x } from "@myscope/sometype";
            "#,
        ),
        (
            "/home/src/projects/myproject/types/sometype.ts",
            r"
                export const x = 10;
            ",
        ),
    ];
    scenarios.push(scenario(
        "tsc/extends/configDir-template-showConfig.js".to_string(),
        "/home/src/projects/myproject",
        &["--showConfig"],
        config_dir_files
            .iter()
            .map(|(path, content)| ((*path).to_string(), dedent(content)))
            .collect(),
    ));

    scenarios
}

#[test]
fn show_config_baselines() {
    let reference = repo_root().join("ts-ref/tsc/testdata/baselines/reference");
    let scenarios = scenarios();
    assert_eq!(scenarios.len(), 22);
    let mut failures = Vec::new();
    for test in &scenarios {
        let expected = tsrs_core::utf8::read_to_string(reference.join(&test.baseline))
            .unwrap_or_else(|error| panic!("{}: {error}", test.baseline));
        let actual = run_scenario(test);
        if actual != expected {
            failures.push(diff_text(
                &format!("reference/{}", test.baseline),
                &format!("local/{}", test.baseline),
                &expected,
                &actual,
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
