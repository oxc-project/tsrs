//! Report requested bytes/capacity for representative owned AST tables.
//! This is a layout probe, not a compiler RSS or performance benchmark.

use std::fmt::Write;
use std::sync::Arc;

use tsrs_ast::storage::{
    AstStore, AstStoreBuilder, AstStoreLease, BindingData, BindingLease, FileAstOwner,
    LazyListSpec, MappedSourceBundle, MappedSourceFile, MappedSourceOutput, MemberListKind,
    NodeKey, SourceFileInfo, SourceFileInfoDraft, TextInput,
};
use tsrs_ast::{
    CheckJsDirective, CommentRange, FileReference, FlowFlags, Kind, NodeFlags,
    SourceFileParseOptions, SymbolFlags, TokenFlags,
};
use tsrs_core::TextRange;

fn main() {
    println!(
        "inline: builder={} store={} file_owner={} lease={} node_key={} binding_data={} binding_lease={}",
        size_of::<AstStoreBuilder>(),
        size_of::<AstStore>(),
        size_of::<FileAstOwner>(),
        size_of::<AstStoreLease>(),
        size_of::<NodeKey>(),
        size_of::<BindingData>(),
        size_of::<BindingLease>(),
    );
    for count in [0, 1, 32, 1000] {
        for payload in ["token", "number", "new", "new-sparse"] {
            let mut ast = AstStoreBuilder::new("1").unwrap();
            // New-expression cases also retain one numeric child and one list.
            let child = (count > 0 && payload.starts_with("new")).then(|| number(&mut ast));
            let list =
                child.map(|child| ast.new_node_list(TextRange::synthetic(), &[child]).unwrap());
            for index in 0..count {
                match payload {
                    "token" => {
                        ast.new_token(Kind::PlusToken, NodeFlags::None, TextRange::synthetic())
                            .unwrap();
                    }
                    "number" => {
                        number(&mut ast);
                    }
                    _ => {
                        ast.new_new_expression(
                            NodeFlags::None,
                            TextRange::synthetic(),
                            child.unwrap(),
                            (payload == "new-sparse" && index % 20 == 0).then_some(list.unwrap()),
                            list,
                        )
                        .unwrap();
                    }
                }
            }
            let store = ast.finish();
            let memory = store.memory();
            println!(
                "{payload}: count={count} nodes={} buffers={:?} node_tables={:?}",
                store.node_count(),
                memory.buffers(),
                memory.nodes,
            );
        }
    }
    for count in [0, 1, 16, 17, 128, 1000] {
        binding_memory(count);
    }
    deferred_memory();
    source_info_memory();
    mapped_source_memory();
}

fn mapped_source_memory() {
    println!(
        "mapped_source: bundle={} output={} handle={}",
        size_of::<MappedSourceBundle>(),
        size_of::<MappedSourceOutput>(),
        size_of::<MappedSourceFile>()
    );
    for supplemental_count in [0, 1, 8] {
        let source: Arc<str> = Arc::from("a");
        let map = Arc::new(tsrs_spanmap::SpanMap::new_owned(&[tsrs_spanmap::Segment {
            virtual_start: 0,
            virtual_end: 1,
            original_start: 0,
            original_end: 1,
            kind: tsrs_spanmap::Kind::VERBATIM,
            features: tsrs_spanmap::Feature::All,
        }]));
        let options = SourceFileParseOptions {
            file_name: "/source.vue".to_owned(),
            path: tsrs_core::tspath::Path::from("/source.vue"),
            ..SourceFileParseOptions::default()
        };
        let canonical = Arc::new(
            SourceFileInfoDraft::new(options.clone(), Arc::clone(&source)).unwrap().freeze(),
        );
        let supplemental = (0..supplemental_count)
            .map(|index| {
                let name = format!("/source.vue.{index}.ts");
                let info = SourceFileInfoDraft::new(
                    SourceFileParseOptions {
                        file_name: name.clone(),
                        path: tsrs_core::tspath::Path::from(name.as_str()),
                        ..SourceFileParseOptions::default()
                    },
                    Arc::clone(&source),
                )
                .unwrap()
                .freeze();
                MappedSourceOutput::new(Arc::new(info), name, Some(Arc::clone(&map)), Vec::new())
            })
            .collect();
        let bundle = MappedSourceBundle::new(
            "mapper".to_owned(),
            "v1".to_owned(),
            options,
            source,
            MappedSourceOutput::new(
                canonical,
                "/source.vue.ts".to_owned(),
                Some(Arc::clone(&map)),
                Vec::new(),
            ),
            supplemental,
        )
        .unwrap();
        println!(
            "mapped_source: supplements={supplemental_count} uninitialized={:?}",
            bundle.memory()
        );
        map.original_to_virtual_positions(0, tsrs_spanmap::Feature::Hover);
        println!(
            "mapped_source: supplements={supplemental_count} initialized={:?}",
            bundle.memory()
        );
        let source_buffers = bundle
            .source_infos()
            .map(|info| info.memory().buffers())
            .fold(tsrs_ast::storage::AstBufferMemory::default(), |sum, memory| sum + memory);
        println!(
            "mapped_source: supplements={supplemental_count} source_info_count={} source_info_buffers={source_buffers:?}",
            bundle.source_infos().len()
        );
    }
}

fn source_info_memory() {
    println!(
        "source_info: inline={} draft={}",
        size_of::<SourceFileInfo>(),
        size_of::<SourceFileInfoDraft>(),
    );
    for (name, text, references) in [
        ("empty", "", false),
        ("ascii", "one\ntwo\n", false),
        ("unicode", "é\r\n😀\u{2028}end", false),
        ("directives", "é\r\n😀\u{2028}end", true),
        ("check-js", "// @check-js\n", false),
    ] {
        let mut draft = SourceFileInfoDraft::new(
            SourceFileParseOptions {
                file_name: "/source.ts".to_owned(),
                path: tsrs_core::tspath::Path::from("/source.ts"),
                ..SourceFileParseOptions::default()
            },
            std::sync::Arc::from(text),
        )
        .unwrap();
        if references {
            draft.directives_mut().referenced_files.push(FileReference {
                text_range: TextRange::new(0, 1),
                file_name: "dependency.ts".to_owned(),
                resolution_mode: tsrs_core::ModuleKind::COMMON_JS,
                preserve: false,
            });
        }
        if name == "check-js" {
            draft.set_check_js_directive(Some(CheckJsDirective {
                enabled: true,
                range: CommentRange {
                    text_range: TextRange::new(0, 12),
                    kind: Kind::SingleLineCommentTrivia,
                    has_trailing_new_line: true,
                },
            }));
        }
        let info = draft.freeze();
        println!("source_info: {name} uninitialized={:?}", info.memory());
        info.ecma_line_map();
        info.get_position_map();
        println!("source_info: {name} initialized={:?}", info.memory());
    }
}

fn binding_memory(count: usize) {
    let mut source = String::new();
    for index in 0..count {
        write!(&mut source, "property_{index} ").unwrap();
    }
    let mut ast = AstStoreBuilder::new(source.as_str()).unwrap();
    let mut bindings = BindingData::new(&mut ast).unwrap();
    if count == 0 {
        println!("binding: empty={:?}", bindings.memory());
    }
    let table = bindings.new_symbol_table().unwrap();
    let mut first = None;
    let mut offset = 0;
    for name in source.split_whitespace() {
        let range = TextRange::new(offset, offset + name.len() as u32);
        let node = ast.new_identifier(NodeFlags::None, range, TextInput::Source(range)).unwrap();
        let symbol = bindings.new_symbol(SymbolFlags::Property, TextInput::Source(range)).unwrap();
        bindings.set_symbol_declarations(symbol, &[node]).unwrap();
        bindings.set_symbol_value_declaration(symbol, Some(node)).unwrap();
        bindings.set_symbol_table_entry(table, name, symbol).unwrap();
        let flow = bindings.new_flow(FlowFlags::Start, Some(node), None).unwrap();
        bindings.set_identifier_flow_node(&ast.node(node).unwrap(), Some(flow)).unwrap();
        first.get_or_insert((node, symbol));
        offset += name.len() as u32 + 1;
    }
    println!("binding: count={count} initial={:?}", bindings.memory());
    if let Some((_, first_symbol)) = first {
        bindings.set_symbol_members(first_symbol, Some(table)).unwrap();
        bindings.set_symbol_exports(first_symbol, Some(table)).unwrap();
        let clone = bindings.clone_symbol_table(table).unwrap();
        bindings.set_symbol_table_entry(clone, "alias", first_symbol).unwrap();
        println!("binding: count={count} cloned_alias={:?}", bindings.memory());
    }
    let lease = bindings.publish(ast.finish()).unwrap();
    if let Some((_, first_symbol)) = first {
        let mut writer = lease.write().unwrap();
        let mut fragment = writer.data().fragment_builder().unwrap();
        let range = TextRange::new(0, "property_0".len() as u32);
        let late =
            fragment.new_identifier(NodeFlags::None, range, TextInput::Source(range)).unwrap();
        let flow = writer.data_mut().new_flow(FlowFlags::Start, Some(late), None).unwrap();
        writer
            .data_mut()
            .set_identifier_flow_node(&fragment.node(late).unwrap(), Some(flow))
            .unwrap();
        writer.publish_fragment(fragment.finish()).unwrap();
        writer.data_mut().append_symbol_declarations(first_symbol, &[late]).unwrap();
        println!("binding: count={count} writing={:?}", writer.data().memory());
        writer.complete().unwrap();
        println!("binding: count={count} published={:?}", lease.read().unwrap().data().memory());
    }
}

fn deferred_memory() {
    let mut ast = AstStoreBuilder::new("interface I {}").unwrap();
    let name = ast
        .new_identifier(
            NodeFlags::None,
            TextRange::new(10, 11),
            TextInput::Source(TextRange::new(10, 11)),
        )
        .unwrap();
    let range = TextRange::new(12, 14);
    let list = ast.new_node_list(range, &[]).unwrap();
    let owner = ast
        .new_interface_declaration(
            NodeFlags::None,
            TextRange::new(0, 14),
            name,
            None,
            None,
            list,
            None,
        )
        .unwrap();
    let mut bindings = BindingData::new(&mut ast).unwrap();
    bindings
        .register_lazy_list(
            &ast,
            LazyListSpec {
                list,
                owner,
                range,
                kind: MemberListKind::Type,
                context_flags: NodeFlags::None,
                parsing_contexts: 0,
            },
        )
        .unwrap();
    println!("binding: lazy_pending={:?}", bindings.memory());
    let lease = bindings.publish(ast.finish()).unwrap();
    println!("binding: lazy_published={:?}", lease.read().unwrap().data().memory());
}

fn number(ast: &mut AstStoreBuilder) -> NodeKey {
    ast.new_numeric_literal(
        NodeFlags::None,
        TextRange::new(0, 1),
        TextInput::Source(TextRange::new(0, 1)),
        TokenFlags::None,
    )
    .unwrap()
}
