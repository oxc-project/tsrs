use std::sync::Arc;

use tsrs_ast::Kind;
use tsrs_compiler::{Context, ProgramOptions, new_compiler_host, new_program};
use tsrs_core::{
    CompilerOptions, P, ScriptTarget, Tristate,
    json::{self, Value},
    tspath::ComparePathsOptions,
};
use tsrs_lint::engine::Options;
use tsrs_lint::payload::ConfiguredRule;
use tsrs_lint::protocol::Diagnostic;
use tsrs_lint::rule::{Listener, RuleContext, visit_lint_nodes};
use tsrs_tsoptions::new_parsed_command_line;
use tsrs_vfs::{bundled, vfstest};

const DECLARATIONS: &str = include_str!("../fixtures/no_misused_promises/generic_overloads.ts");

#[test]
fn generic_callback_constraints_do_not_expand_union_signatures() {
    std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(|| {
            let Value::Array(expected) =
                json::unmarshal(include_str!("../fixtures/no_misused_promises/diagnostics.json"))
                    .unwrap()
            else {
                panic!()
            };
            for (index, call) in [
                "on('start', () => {});",
                "on('start', () => {}, () => {});",
                "on();",
                "asyncOn('start', async () => {});",
                "new newOn('start', () => {});",
                "on('start', async () => {});",
                "optionalOn('start', async () => {});",
                "new newOn('start', async () => {});",
            ]
            .iter()
            .enumerate()
            {
                check(&format!("{DECLARATIONS}\n{call}"), &expected[index]);
            }
            check(
                include_str!("../fixtures/no_misused_promises/never_intersection.ts"),
                &expected[8],
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

fn check(source: &str, expected: &Value) {
    let region = tsrs_core::arena::Region::new(1 << 20);
    let _scope = region.enter();
    let fs = Arc::new(bundled::wrap_fs(vfstest::from_map([("/project/file.ts", source)], true)));
    let config = new_parsed_command_line(
        P::new(CompilerOptions {
            strict: Tristate::True,
            target: ScriptTarget::ES_NEXT,
            types: Some(Vec::new()),
            single_threaded: Tristate::True,
            ..Default::default()
        }),
        vec!["/project/file.ts".into()],
        Vec::new(),
        ComparePathsOptions::default(),
    );
    let host = new_compiler_host("/project", fs, &bundled::default_lib_path(), None, None, None);
    let program = new_program(ProgramOptions::new(P::new(config), host));
    let file = program.get_source_file("/project/file.ts").unwrap();
    let mut checker = program.get_type_checker_for_file(&Context::background(), file);
    let mut diagnostics = Vec::new();
    let mut context = RuleContext {
        source_file: file,
        program,
        checker: &mut checker,
        rule_name: "no-misused-promises",
        options: Options::default(),
        diagnostics: &mut diagnostics,
    };
    let mut rule = tsrs_lint::registry::create_rule(&ConfiguredRule {
        name: "no-misused-promises".into(),
        options: Value::Null,
    })
    .unwrap();
    rule.initialize(&mut context);
    let mut calls = 0;
    visit_lint_nodes(file, &mut |listener, node| {
        if rule.listens_to(listener) {
            let before = context.checker.type_count;
            rule.visit(listener, node, &mut context);
            if matches!(listener, Listener::Enter(Kind::CallExpression | Kind::NewExpression)) {
                calls += 1;
                let created = context.checker.type_count - before;
                assert!(created <= 4096, "created {created} types: {source}");
            }
        }
    });
    assert!(calls > 0);
    let actual = Value::Array(diagnostics.iter().map(Diagnostic::to_json).collect());
    let actual = json::unmarshal(&json::marshal(&actual).unwrap()).unwrap();
    assert_eq!(actual, *expected, "{source}");
}
