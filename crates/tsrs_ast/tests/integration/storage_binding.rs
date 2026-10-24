use std::num::NonZeroU32;
use std::sync::mpsc;
use std::time::Duration;
use tsrs_ast::{FlowFlags, SymbolFlags};

use tsrs_core::TextRange;

use tsrs_ast::storage::*;
use tsrs_ast::{Kind, NodeFlags};

fn identifier(ast: &mut AstStoreBuilder) -> NodeKey {
    ast.new_identifier(
        NodeFlags::None,
        TextRange::new(0, 1),
        TextInput::Source(TextRange::new(0, 1)),
    )
    .unwrap()
}

#[test]
fn declaration_snapshots_release_guards_and_preserve_order_across_published_writes() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let first = identifier(&mut ast);
    let second = identifier(&mut ast);
    let mut data = BindingData::new(&mut ast).unwrap();
    let symbol = data.new_symbol(SymbolFlags::Property, TextInput::Owned("x")).unwrap();
    let empty = data.symbol(symbol).unwrap().declaration_snapshot();
    assert!(empty.is_empty());
    data.set_symbol_declarations(symbol, &[second, first]).unwrap();
    let original = data.symbol(symbol).unwrap().declaration_snapshot();
    let bindings = data.publish(ast.finish()).unwrap();
    let weak = bindings.downgrade();
    let saved = bindings.read().unwrap().data().symbol(symbol).unwrap().declaration_snapshot();

    // No borrowed declaration slice or read guard crosses a callback's writer.
    // Both the compact declaration buffer and foreign-edge directory can grow.
    let mut write = bindings.write().unwrap();
    let mut fragment = write.data().fragment_builder().unwrap();
    let late = identifier(&mut fragment);
    write.publish_fragment(fragment.finish()).unwrap();
    for _ in 0..32 {
        write.data_mut().append_symbol_declarations(symbol, &[late]).unwrap();
    }
    let prefix = write.data().symbol(symbol).unwrap().declaration_snapshot();
    let other_symbol =
        write.data_mut().new_symbol(SymbolFlags::Property, TextInput::Owned("other")).unwrap();
    write.data_mut().set_symbol_declarations(other_symbol, &[late, second]).unwrap();
    for _ in 0..32 {
        write.data_mut().append_symbol_declarations(symbol, &[late]).unwrap();
    }
    let appended = write.data().symbol(symbol).unwrap().declaration_snapshot();
    write.data_mut().set_symbol_declarations(symbol, &[late]).unwrap();
    write.complete().unwrap();

    let read = bindings.read().unwrap();
    assert_eq!(read.data().read_declarations(empty).unwrap().len(), 0);
    for snapshot in [original, saved] {
        let mut nodes = read.data().read_declarations(snapshot).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes.next(), Some(second));
        assert_eq!(nodes.next_back(), Some(first));
        assert_eq!(nodes.next(), None);
    }
    for (snapshot, count) in [(prefix, 34), (appended, 66)] {
        assert_eq!(snapshot.len(), count);
        let nodes = read.data().read_declarations(snapshot).unwrap().collect::<Vec<_>>();
        assert_eq!(nodes.len(), count);
        assert_eq!(&nodes[..2], &[second, first]);
        assert!(nodes[2..].iter().all(|&node| node == late));
    }
    assert_eq!(
        read.data().symbol(other_symbol).unwrap().declarations().collect::<Vec<_>>(),
        [late, second]
    );
    assert_eq!(read.data().symbol(symbol).unwrap().declarations().collect::<Vec<_>>(), [late]);
    drop(read);
    drop(bindings);
    assert!(weak.upgrade().is_none(), "a snapshot does not retain a binding/body owner");

    let mut other_ast = AstStoreBuilder::new("x").unwrap();
    let other = BindingData::new(&mut other_ast).unwrap();
    assert!(matches!(other.read_declarations(saved), Err(AstStoreError::WrongStore { .. })));
}

#[test]
fn table_entry_snapshots_survive_callback_replacement_deletion_and_growth() {
    let mut ast = AstStoreBuilder::new("first").unwrap();
    let node = identifier(&mut ast);
    let mut data = BindingData::new(&mut ast).unwrap();
    let table = data.new_symbol_table().unwrap();
    assert!(data.symbol_table(table).unwrap().snapshot_entries().is_empty());
    let first =
        data.new_symbol(SymbolFlags::Property, TextInput::Source(TextRange::new(0, 5))).unwrap();
    let second = data.new_symbol(SymbolFlags::Method, TextInput::Owned("different")).unwrap();
    let third_name = "long_owned_名前_abcdefghijklmnopqrstuvwxyz";
    let third = data.new_symbol(SymbolFlags::Property, TextInput::Owned(third_name)).unwrap();
    data.set_symbol_declarations(first, &[node]).unwrap();
    for (name, symbol) in [("first", first), ("別名🌳", second), (third_name, third)] {
        data.set_symbol_table_entry(table, name, symbol).unwrap();
    }
    let bindings = data.publish(ast.finish()).unwrap();
    let weak = bindings.downgrade();
    let entries = bindings.read().unwrap().data().symbol_table(table).unwrap().snapshot_entries();
    let copy = {
        let mut write = bindings.write().unwrap();
        let copy = write.data_mut().clone_symbol_table(table).unwrap();
        // Replacing the first entry installs an odd-key descriptor. Deletion
        // shifts the remaining entries; neither may change the saved sequence.
        write.data_mut().set_symbol_table_entry(table, "first", second).unwrap();
        write.data_mut().delete_symbol_table_entry(table, "別名🌳").unwrap();
        write.data_mut().delete_symbol_table_entry(copy, third_name).unwrap();
        for index in 0..128 {
            let name = format!("growth_{index}_é🌳");
            let symbol = write
                .data_mut()
                .new_symbol(SymbolFlags::Property, TextInput::Owned(&name))
                .unwrap();
            write.data_mut().set_symbol_table_entry(table, &name, symbol).unwrap();
            write.data_mut().new_symbol_table().unwrap();
        }
        let text_before = write.data().memory().text;
        for &entry in &entries {
            write.data_mut().set_symbol_table_entry_from_snapshot(table, entry, third).unwrap();
        }
        assert_eq!(write.data().memory().text, text_before, "saved names need no text copy");
        write.complete().unwrap();
        copy
    };
    let expected = [("first", first), ("別名🌳", second), (third_name, third)];
    for (&entry, expected) in entries.iter().zip(expected) {
        assert_eq!(entry.symbol_key(), expected.1);
        {
            let read = bindings.read().unwrap();
            assert_eq!(read.data().read_symbol_table_entry(entry).unwrap(), expected);
        }
        // Like the legacy table's callback snapshot, no entry/name read guard
        // crosses this callback, even when it empties both original tables.
        let mut write = bindings.write().unwrap();
        write.data_mut().delete_symbol_table_entry_from_snapshot(table, entry).unwrap();
        write.data_mut().delete_symbol_table_entry_from_snapshot(copy, entry).unwrap();
        write.data_mut().delete_symbol_table_entry_from_snapshot(copy, entry).unwrap();
        write.complete().unwrap();
    }
    for (&entry, expected) in entries.iter().zip(expected) {
        assert_eq!(
            bindings.read().unwrap().data().read_symbol_table_entry(entry).unwrap(),
            expected
        );
    }
    {
        let mut write = bindings.write().unwrap();
        let restored = write.data_mut().new_symbol_table().unwrap();
        let text_before = write.data().memory().text;
        for &entry in &entries {
            write
                .data_mut()
                .set_symbol_table_entry_from_snapshot(restored, entry, entry.symbol_key())
                .unwrap();
        }
        assert_eq!(write.data().memory().text, text_before);
        assert_eq!(
            write.data().symbol_table(restored).unwrap().entries().collect::<Vec<_>>(),
            expected
        );
        write.complete().unwrap();
    }
    drop(bindings);
    assert!(weak.upgrade().is_none(), "saved entries retain neither binding nor syntax owners");
    let mut other_ast = AstStoreBuilder::new("first").unwrap();
    let mut other = BindingData::new(&mut other_ast).unwrap();
    let other_table = other.new_symbol_table().unwrap();
    let other_symbol = other.new_symbol(SymbolFlags::Property, TextInput::Owned("first")).unwrap();
    assert!(matches!(
        other.read_symbol_table_entry(entries[0]),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert!(matches!(
        other.set_symbol_table_entry_from_snapshot(other_table, entries[0], other_symbol),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert!(matches!(
        other.delete_symbol_table_entry_from_snapshot(other_table, entries[0]),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert!(other.symbol_table(other_table).unwrap().is_empty());
}

#[test]
fn publication_rejects_unretained_foreign_binding_edges() {
    for initial in [false, true] {
        {
            let mut ast = AstStoreBuilder::new("x").unwrap();
            let mut data = BindingData::new(&mut ast).unwrap();
            let name = identifier(&mut ast);
            let mut other = AstStoreBuilder::new("x").unwrap();
            let other_name = identifier(&mut other);
            assert_eq!(name.index(), other_name.index());
            let invalid = other_name;
            let symbol = data.new_symbol(SymbolFlags::Property, TextInput::Owned("x")).unwrap();
            if initial {
                data.set_symbol_declarations(symbol, &[invalid]).unwrap();
                assert!(data.publish(ast.finish()).is_err());
            } else {
                let bindings = data.publish(ast.finish()).unwrap();
                let mut write = bindings.write().unwrap();
                write.data_mut().set_symbol_declarations(symbol, &[invalid]).unwrap();
                assert!(write.complete().is_err());
                assert!(matches!(bindings.read(), Err(AstAccessError::Failed)));
            }
        }
    }
}

#[test]
fn initial_binding_rejects_annotations_on_unretained_fragment_syntax() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let mut fragment = data.fragment_builder().unwrap();
    let name = identifier(&mut fragment);
    let flow = data.new_flow(FlowFlags::Start, None, None).unwrap();
    data.set_identifier_flow_node(&fragment.node(name).unwrap(), Some(flow)).unwrap();
    assert!(matches!(
        data.publish(ast.finish()),
        Err(AstAccessError::UnpermittedOwner(store)) if store == name.store()
    ));
}

#[test]
fn initial_and_late_publication_validate_retained_fragment_syntax_dependencies() {
    for initial in [false, true] {
        for edge_kind in 0..3 {
            let mut ast = AstStoreBuilder::new("x").unwrap();
            let mut data = BindingData::new(&mut ast).unwrap();
            let mut other = AstStoreBuilder::new("x").unwrap();
            let foreign = identifier(&mut other);
            let mut fragment = data.fragment_builder().unwrap();
            match edge_kind {
                0 => {
                    let name = identifier(&mut fragment);
                    fragment.set_parent(name, Some(foreign)).unwrap();
                }
                1 => {
                    let list = other.new_node_list(TextRange::synthetic(), &[foreign]).unwrap();
                    fragment
                        .new_array_literal_expression(
                            NodeFlags::None,
                            TextRange::synthetic(),
                            list,
                            false,
                        )
                        .unwrap();
                }
                _ => {
                    let modifiers = other
                        .new_modifier_list(
                            TextRange::synthetic(),
                            &[],
                            tsrs_ast::ModifierFlags::None,
                        )
                        .unwrap();
                    let name = identifier(&mut fragment);
                    let members = fragment.new_node_list(TextRange::synthetic(), &[]).unwrap();
                    fragment
                        .new_interface_declaration(
                            NodeFlags::None,
                            TextRange::synthetic(),
                            name,
                            None,
                            None,
                            members,
                            Some(modifiers),
                        )
                        .unwrap();
                }
            }
            let store = fragment.finish();
            if initial {
                data.retain_fragment(store).unwrap();
                assert!(matches!(
                    data.publish(ast.finish()),
                    Err(AstAccessError::UnpermittedOwner(_))
                ));
            } else {
                let bindings = data.publish(ast.finish()).unwrap();
                let mut write = bindings.write().unwrap();
                write.publish_fragment(store).unwrap();
                assert!(matches!(write.complete(), Err(AstAccessError::UnpermittedOwner(_))));
                assert!(matches!(bindings.read(), Err(AstAccessError::Failed)));
            }
        }
    }
}

#[test]
fn file_binding_identity_spans_base_and_fragment_syntax_and_retains_both() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let name = identifier(&mut ast);
    let declaration = ast
        .new_variable_declaration(NodeFlags::None, TextRange::new(0, 1), name, None, None, None)
        .unwrap();
    let symbol =
        data.new_symbol(SymbolFlags::FunctionScopedVariable, TextInput::Owned("x")).unwrap();
    let flow = data.new_flow(FlowFlags::Start, Some(declaration), None).unwrap();
    data.set_symbol_declarations(symbol, &[declaration]).unwrap();
    data.set_symbol_value_declaration(symbol, Some(declaration)).unwrap();
    data.set_variable_declaration_symbol(&ast.node(declaration).unwrap(), Some(symbol)).unwrap();
    data.set_identifier_flow_node(&ast.node(name).unwrap(), Some(flow)).unwrap();

    let mut fragment = data.fragment_builder().unwrap();
    let late_name = identifier(&mut fragment);
    let late_declaration = fragment
        .new_variable_declaration(
            NodeFlags::None,
            TextRange::new(0, 1),
            late_name,
            None,
            None,
            None,
        )
        .unwrap();
    data.set_variable_declaration_symbol(&fragment.node(late_declaration).unwrap(), Some(symbol))
        .unwrap();
    data.set_identifier_flow_node(&fragment.node(late_name).unwrap(), Some(flow)).unwrap();
    assert_ne!(name.store(), late_name.store());
    assert_ne!(name.store(), symbol.store());
    assert_eq!(symbol.store(), flow.store());

    let late = data.retain_fragment(fragment.finish()).unwrap();
    let binding = data.publish(ast.finish()).unwrap();
    let root = binding.root_ast();
    let root_weak = root.downgrade();
    let mut write = binding.write().unwrap();
    let late_weak = late.downgrade();
    write.data_mut().append_symbol_declarations(symbol, &[late_declaration]).unwrap();
    write.complete().unwrap();

    let read = binding.read().unwrap();
    assert_eq!(
        read.data().symbol(symbol).unwrap().declarations().collect::<Vec<_>>(),
        [declaration, late_declaration]
    );
    for (syntax, node, ident) in [(&root, declaration, name), (&late, late_declaration, late_name)]
    {
        assert_eq!(
            read.data().variable_declaration(&syntax.node(node).unwrap()).unwrap().symbol(),
            Some(symbol)
        );
        assert_eq!(
            read.data().identifier(&syntax.node(ident).unwrap()).unwrap().flow_node(),
            Some(flow)
        );
    }
    assert_eq!(
        read.extension(late_declaration.store()).unwrap().node(late_declaration).unwrap().kind(),
        Kind::VariableDeclaration
    );
    drop(read);
    drop(root);
    drop(late);
    assert!(root_weak.upgrade().is_some());
    assert!(late_weak.upgrade().is_some());
    drop(binding);
    assert!(root_weak.upgrade().is_none());
    assert!(late_weak.upgrade().is_none());
}

#[test]
fn file_and_class_annotations_follow_container_links_across_fragments() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let name = identifier(&mut ast);
    let members = ast.new_node_list(TextRange::synthetic(), &[]).unwrap();
    let class = ast
        .new_class_declaration(
            NodeFlags::None,
            TextRange::synthetic(),
            Some(name),
            None,
            None,
            members,
            None,
        )
        .unwrap();
    let statements = ast.new_node_list(TextRange::synthetic(), &[class]).unwrap();
    let eof = ast.new_token(Kind::EndOfFile, NodeFlags::None, TextRange::synthetic()).unwrap();
    let file = ast.new_source_file(NodeFlags::None, TextRange::new(0, 1), statements, eof).unwrap();
    let symbol = data.new_symbol(SymbolFlags::Class, TextInput::Owned("x")).unwrap();
    let locals = data.new_symbol_table().unwrap();
    data.set_symbol_table_entry(locals, "x", symbol).unwrap();
    data.set_class_declaration_symbol(&ast.node(class).unwrap(), Some(symbol)).unwrap();
    data.set_class_declaration_locals(&ast.node(class).unwrap(), Some(locals)).unwrap();
    data.set_source_file_locals(&ast.node(file).unwrap(), Some(locals)).unwrap();
    data.set_source_file_next_container(&ast.node(file).unwrap(), Some(class)).unwrap();
    assert!(matches!(
        data.set_class_declaration_symbol(&ast.node(file).unwrap(), Some(symbol)),
        Err(AstStoreError::WrongPayload { .. })
    ));

    let mut fragment = data.fragment_builder().unwrap();
    let late_class = fragment
        .new_class_declaration(
            NodeFlags::None,
            TextRange::synthetic(),
            Some(name),
            None,
            None,
            members,
            None,
        )
        .unwrap();
    data.set_class_declaration_next_container(&ast.node(class).unwrap(), Some(late_class)).unwrap();
    data.set_class_declaration_next_container(&fragment.node(late_class).unwrap(), Some(class))
        .unwrap();
    data.set_class_declaration_locals(&fragment.node(late_class).unwrap(), Some(locals)).unwrap();
    let late = data.retain_fragment(fragment.finish()).unwrap();
    let binding = data.publish(ast.finish()).unwrap();
    let root = binding.root_ast();
    let read = binding.read().unwrap();
    let data = read.data();
    let file_data = data.source_file(&root.node(file).unwrap()).unwrap();
    assert_eq!(file_data.next_container(), Some(class));
    let class_data = data.class_declaration(&root.node(class).unwrap()).unwrap();
    assert_eq!(class_data.next_container(), Some(late_class));
    assert_eq!(class_data.symbol(), Some(symbol));
    let late_data = data.class_declaration(&late.node(late_class).unwrap()).unwrap();
    assert_eq!(late_data.next_container(), Some(class));
    assert_eq!(late_data.symbol(), None);
    for table in [file_data.locals(), class_data.locals(), late_data.locals()] {
        assert_eq!(data.symbol_table(table.unwrap()).unwrap().lookup("x"), Some(symbol));
    }
}

#[test]
fn declaration_replacement_preserves_an_independent_value_declaration() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let first = identifier(&mut ast);
    let second = identifier(&mut ast);
    let symbol =
        data.new_symbol(SymbolFlags::FunctionScopedVariable, TextInput::Owned("x")).unwrap();
    data.set_symbol_declarations(symbol, &[first]).unwrap();
    data.set_symbol_value_declaration(symbol, Some(first)).unwrap();
    assert!(data.auxiliary_len() == 0, "the first declaration needs no auxiliary allocation");
    data.set_symbol_declarations(symbol, &[second]).unwrap();
    assert_eq!(data.symbol(symbol).unwrap().value_declaration(), Some(first));
    assert_eq!(data.symbol(symbol).unwrap().declarations().collect::<Vec<_>>(), [second]);
    data.set_symbol_declarations(symbol, &[]).unwrap();
    assert_eq!(data.symbol(symbol).unwrap().value_declaration(), Some(first));
    data.set_symbol_value_declaration(symbol, None).unwrap();
    assert_eq!(data.symbol(symbol).unwrap().value_declaration(), None);
    data.set_symbol_declarations(symbol, &[first, second]).unwrap();
    data.set_symbol_value_declaration(symbol, Some(second)).unwrap();
    data.set_symbol_declarations(symbol, &[second, first]).unwrap();
    assert_eq!(data.symbol(symbol).unwrap().value_declaration(), Some(second));
    let assigned = data.symbol(symbol).unwrap().get_or_assign_id(|| NonZeroU32::new(7).unwrap());
    assert_eq!(
        data.symbol(symbol).unwrap().get_or_assign_id(|| panic!("already assigned")),
        assigned
    );
}

#[test]
fn annotations_reject_other_binding_owners_and_invalid_keys_before_writing() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let name = identifier(&mut ast);
    let declaration = ast
        .new_variable_declaration(NodeFlags::None, TextRange::synthetic(), name, None, None, None)
        .unwrap();
    let symbol =
        data.new_symbol(SymbolFlags::FunctionScopedVariable, TextInput::Owned("x")).unwrap();
    let mut other_ast = AstStoreBuilder::new("x").unwrap();
    let mut other = BindingData::new(&mut other_ast).unwrap();
    let other_symbol =
        other.new_symbol(SymbolFlags::FunctionScopedVariable, TextInput::Owned("x")).unwrap();
    assert_eq!(symbol.index(), other_symbol.index());
    assert!(matches!(data.symbol(other_symbol), Err(AstStoreError::WrongStore { .. })));
    assert!(matches!(
        other.set_variable_declaration_symbol(&ast.node(declaration).unwrap(), Some(other_symbol)),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert!(matches!(
        data.set_variable_declaration_symbol(&ast.node(declaration).unwrap(), Some(other_symbol)),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert_eq!(data.variable_declaration(&ast.node(declaration).unwrap()).unwrap().symbol(), None);
    assert!(matches!(
        BindingData::new(&mut ast),
        Err(AstStoreError::BindingOwnerAlreadyAssigned(_))
    ));
}

#[test]
fn flow_links_and_unknown_kind_payloads_keep_their_distinct_domains() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let node = identifier(&mut ast);
    let start = data.new_flow(FlowFlags::Start, Some(node), None).unwrap();
    let label = data.new_flow(FlowFlags::BranchLabel, None, None).unwrap();
    let list = data.new_flow_list(start, None).unwrap();
    data.set_flow_antecedents(label, Some(list)).unwrap();
    data.mark_flow_referenced(start).unwrap();
    data.mark_flow_referenced(start).unwrap();
    assert!(data.flow(start).unwrap().flags().contains(FlowFlags::Referenced | FlowFlags::Shared));
    assert_eq!(data.flow(label).unwrap().antecedents(), Some(list));
    assert_eq!(data.flow(label).unwrap().antecedent(), None);
    assert_eq!(data.flow_list(list).unwrap().flow(), start);
    assert_eq!(
        data.new_flow(FlowFlags::BranchLabel, None, Some(start)),
        Err(AstStoreError::FlowLabelWithAntecedent)
    );
    assert_eq!(data.set_flow_antecedents(start, Some(list)), Err(AstStoreError::NotFlowLabel));
    let switch = ast
        .new_flow_switch_clause_data(NodeFlags::None, TextRange::synthetic(), node, 1, 3)
        .unwrap();
    let reduce = ast
        .new_flow_reduce_label_data(
            NodeFlags::None,
            TextRange::synthetic(),
            &data,
            label,
            Some(list),
        )
        .unwrap();
    let syntax = ast.finish();
    assert_eq!(syntax.node(switch).unwrap().kind(), Kind::Unknown);
    assert_eq!(syntax.node(reduce).unwrap().kind(), Kind::Unknown);
    assert!(matches!(
        syntax.node(switch).unwrap().as_flow_reduce_label_data(),
        Err(AstStoreError::WrongPayload { .. })
    ));
    let reduce = syntax.node(reduce).unwrap().as_flow_reduce_label_data().unwrap();
    assert_eq!(reduce.target(), label);
    assert_eq!(reduce.antecedents(), Some(list));
}

#[test]
fn cached_symbol_readers_wait_for_completion_and_failed_writes_are_terminal() {
    for abandon in [false, true] {
        let mut ast = AstStoreBuilder::new("").unwrap();
        let mut data = BindingData::new(&mut ast).unwrap();
        let symbol = data.new_symbol(SymbolFlags::None, TextInput::Owned("cached")).unwrap();
        let binding = data.publish(ast.finish()).unwrap();
        let mut write = binding.write().unwrap();
        write.data_mut().set_symbol_flags(symbol, SymbolFlags::Property).unwrap();
        std::thread::scope(|scope| {
            let (started_tx, started_rx) = mpsc::channel();
            let binding = &binding;
            let reader = scope.spawn(move || {
                started_tx.send(()).unwrap();
                binding.read().map(|read| read.data().symbol(symbol).unwrap().flags())
            });
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if abandon {
                drop(write);
            } else {
                write.complete().unwrap();
            }
            let expected =
                if abandon { Err(AstAccessError::Failed) } else { Ok(SymbolFlags::Property) };
            assert_eq!(reader.join().unwrap(), expected);
        });
        if abandon {
            assert!(matches!(binding.read(), Err(AstAccessError::Failed)));
            assert!(matches!(binding.write(), Err(AstAccessError::Failed)));
        }
    }
}

#[test]
fn published_node_annotations_change_only_under_binding_authority() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let name = identifier(&mut ast);
    let node = ast
        .new_variable_declaration(NodeFlags::None, TextRange::synthetic(), name, None, None, None)
        .unwrap();
    let first =
        data.new_symbol(SymbolFlags::FunctionScopedVariable, TextInput::Owned("x")).unwrap();
    let replacement =
        data.new_symbol(SymbolFlags::FunctionScopedVariable, TextInput::Owned("merged")).unwrap();
    data.set_variable_declaration_symbol(&ast.node(node).unwrap(), Some(first)).unwrap();
    let bindings = data.publish(ast.finish()).unwrap();
    let syntax = bindings.root_ast();
    let cached_node = syntax.node(node).unwrap();
    {
        let read = bindings.read().unwrap();
        assert_eq!(
            read.data().variable_declaration(&syntax.node(node).unwrap()).unwrap().symbol(),
            Some(first)
        );
    }
    {
        let mut write = bindings.write().unwrap();
        write.data_mut().set_variable_declaration_symbol(&cached_node, Some(replacement)).unwrap();
        write.complete().unwrap();
    }
    let read = bindings.read().unwrap();
    assert_eq!(
        read.data().variable_declaration(&syntax.node(node).unwrap()).unwrap().symbol(),
        Some(replacement)
    );
    assert_eq!(syntax.node(node).unwrap().as_variable_declaration().unwrap().name(), name);
}

#[test]
fn ordered_binding_tables_preserve_stored_keys_through_growth_replacement_delete_and_clone() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let table = data.new_symbol_table().unwrap();
    let mut symbols = Vec::new();
    for index in 0..128 {
        let name = format!("property_{index}");
        let symbol = data.new_symbol(SymbolFlags::Property, TextInput::Owned(&name)).unwrap();
        data.set_symbol_table_entry(table, &name, symbol).unwrap();
        symbols.push(symbol);
    }
    let alias = data.new_symbol(SymbolFlags::Method, TextInput::Owned("different_name")).unwrap();
    data.set_symbol_table_entry(table, "property_17", alias).unwrap();
    assert_eq!(data.symbol_table(table).unwrap().entry(17), Some(("property_17", alias)));
    for &index in &[0, 31, 63, 127] {
        data.delete_symbol_table_entry(table, &format!("property_{index}")).unwrap();
    }
    for (index, &symbol) in symbols.iter().enumerate() {
        let found = data.symbol_table(table).unwrap().lookup(&format!("property_{index}"));
        let expected = if [0, 31, 63, 127].contains(&index) {
            None
        } else if index == 17 {
            Some(alias)
        } else {
            Some(symbol)
        };
        assert_eq!(found, expected);
    }
    let copy = data.clone_symbol_table(table).unwrap();
    assert_ne!(copy, table);
    assert_eq!(
        data.symbol_table(table).unwrap().entries().collect::<Vec<_>>(),
        data.symbol_table(copy).unwrap().entries().collect::<Vec<_>>()
    );
    data.delete_symbol_table_entry(copy, "property_17").unwrap();
    data.set_symbol_table_entry(copy, "property_128", alias).unwrap();
    assert_eq!(data.symbol_table(table).unwrap().lookup("property_17"), Some(alias));
    assert_eq!(data.symbol_table(table).unwrap().lookup("property_128"), None);
    assert_eq!(data.symbol_table(copy).unwrap().entries().last(), Some(("property_128", alias)));
}

#[test]
fn symbol_tables_keep_long_utf8_aliases_and_validate_every_owner() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let mut data = BindingData::new(&mut ast).unwrap();
    let table = data.new_symbol_table().unwrap();
    let symbol = data.new_symbol(SymbolFlags::Property, TextInput::Owned("x")).unwrap();
    let first = format!("{}a", "界".repeat(48));
    let second = format!("{}b", "界".repeat(48));
    data.set_symbol_table_entry(table, &first, symbol).unwrap();
    data.set_symbol_table_entry(table, &second, symbol).unwrap();
    assert_eq!(data.symbol_table(table).unwrap().lookup(&first), Some(symbol));
    assert_eq!(data.symbol_table(table).unwrap().lookup(&second), Some(symbol));
    assert_eq!(data.symbol_table(table).unwrap().lookup("missing"), None);
    let mut other_ast = AstStoreBuilder::new("").unwrap();
    let mut other = BindingData::new(&mut other_ast).unwrap();
    let other_table = other.new_symbol_table().unwrap();
    assert_eq!(other_table.index(), table.index());
    assert!(matches!(data.symbol_table(other_table), Err(AstStoreError::WrongStore { .. })));
    assert!(matches!(
        data.set_symbol_table_entry(other_table, "x", symbol),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert!(matches!(
        other.set_symbol_table_entry(other_table, "x", symbol),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert_eq!(other.symbol_table(other_table).unwrap().len(), 0);
    assert_eq!(data.auxiliary_len(), 0);
    data.set_symbol_members(symbol, None).unwrap();
    data.set_symbol_exports(symbol, None).unwrap();
    data.set_symbol_export_symbol(symbol, None).unwrap();
    assert!(data.auxiliary_len() == 0, "absent optional tables need no tail");
    data.set_symbol_members(symbol, Some(table)).unwrap();
    data.set_symbol_exports(symbol, Some(table)).unwrap();
    data.set_symbol_export_symbol(symbol, Some(symbol)).unwrap();
    assert_eq!(data.symbol(symbol).unwrap().members(), Some(table));
    assert_eq!(data.symbol(symbol).unwrap().exports(), Some(table));
    assert_eq!(data.symbol(symbol).unwrap().export_symbol(), Some(symbol));
    assert!(matches!(
        data.set_symbol_exports(symbol, Some(other_table)),
        Err(AstStoreError::WrongStore { .. })
    ));
    assert_eq!(data.symbol(symbol).unwrap().exports(), Some(table));
}
