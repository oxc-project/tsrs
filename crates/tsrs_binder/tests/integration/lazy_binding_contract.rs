//! Real parser/binder cases constraining the owned-storage migration. This
//! exercises the legacy backend; it does not establish owned-backend parity.

use tsrs_ast::lazylist::{DEFERRED, DONE};
use tsrs_ast::{FlowFlags, Kind, SourceFile, SourceFileParseOptions, Symbol};
use tsrs_core::{P, ScriptKind};

fn parse(source: &str) -> P<SourceFile> {
    let file = tsrs_parser::parse_source_file(
        SourceFileParseOptions {
            file_name: "/migration-probe.d.ts".to_owned(),
            ..Default::default()
        },
        source,
        ScriptKind::TS,
    );
    assert_eq!(file.diagnostics(), []);
    tsrs_binder::bind_source_file(file);
    file
}

#[derive(Debug, PartialEq, Eq)]
struct Tables {
    kind: Kind,
    flags: u32,
    members: Vec<(&'static str, u32)>,
    exports: Vec<(&'static str, u32)>,
}

fn tables(file: P<SourceFile>, shared_reader: bool) -> Vec<Tables> {
    file.statements()
        .nodes()
        .iter()
        .map(|&node| {
            let original = node.symbol().unwrap();
            let symbol = if shared_reader { Symbol::new_shared_data(original) } else { original };
            Tables {
                kind: node.kind(),
                flags: node.flags().bits(),
                members: symbol.members().map_or_else(Vec::new, |table| {
                    table
                        .entries()
                        .into_iter()
                        .map(|(name, symbol)| (name, symbol.flags().bits()))
                        .collect()
                }),
                exports: symbol.exports().map_or_else(Vec::new, |table| {
                    table
                        .entries()
                        .into_iter()
                        .map(|(name, symbol)| (name, symbol.flags().bits()))
                        .collect()
                }),
            }
        })
        .collect()
}

#[test]
fn deferred_parser_guards_and_shared_symbol_readers_preserve_binding() {
    if !crate::isolated::in_child(
        "lazy_binding_contract::deferred_parser_guards_and_shared_symbol_readers_preserve_binding",
    ) {
        return;
    }
    // One test controls the process-wide switch: collect eager controls first.
    let cases = [
        ("interface I { x: number; m(y: string): void; nested: { a: number } }", true),
        (
            "declare class C { x: number; static y: string; m(a: number): boolean; constructor(); }",
            true,
        ),
        ("declare class C { [Symbol.iterator](): number; }", true),
        // Binding the parameter property reads C.members while C's own lazy
        // list is being forced; the owned writer must use its active context.
        ("declare class C { constructor(public x: number); }", true),
        ("interface I { x: number } interface I { y: string }", true),
        ("declare class C { x: number = 1; }", false),
        ("declare class C { m() {} }", false),
        ("interface I { x: this }", false),
        ("interface I<T> { x: T extends infer U ? U : never }", false),
        ("interface I { x: import('dep').Value }", false),
    ];
    let eager: Vec<_> = cases.iter().map(|&(source, _)| tables(parse(source), false)).collect();
    tsrs_parser::enable_lazy_dts();
    tsrs_binder::enable_lazy_dts();
    for ((source, deferred), eager) in cases.into_iter().zip(eager) {
        let file = parse(source);
        let records = file.lazy_lists.get();
        assert_eq!(records.iter().any(|record| record.state() == DEFERRED), deferred, "{source}");
        assert_eq!(records.is_empty(), !deferred, "{source}");
        if records.len() > 1 {
            // The merge binds the earlier list inline before deferring the next.
            assert_eq!(
                records.iter().map(|record| record.state()).collect::<Vec<_>>(),
                [DONE, DEFERRED]
            );
        }
        let mut flows: Vec<(P<tsrs_ast::FlowNode>, FlowFlags)> = Vec::new();
        for &record in records {
            if let Some(context) = record.bind_context() {
                for node in [
                    context.container,
                    context.this_container,
                    context.block_scope_container,
                    context.last_container,
                ]
                .into_iter()
                .flatten()
                {
                    assert_eq!(tsrs_ast::get_source_file_of_node(node), Some(file));
                }
                for flow in
                    [context.current_flow, Some(context.unreachable_flow)].into_iter().flatten()
                {
                    if !flows.iter().any(|&(saved, _)| saved == flow) {
                        let before = flow.flags();
                        // Otherwise idempotent flag writes could escape the
                        // before/after observation in these probe cases.
                        assert!(!before.intersects(FlowFlags::Referenced | FlowFlags::Shared));
                        flows.push((flow, before));
                    }
                }
            }
        }
        // An instantiated checker symbol shares binder data and can be the first
        // reader that forces its data owner's member/export tables.
        assert_eq!(tables(file, true), eager, "{source}");
        assert!(records.iter().all(|record| record.state() == DONE), "{source}");
        for (flow, before) in flows {
            assert_eq!(flow.flags(), before, "saved flow mutated by {source}");
        }
    }
}
