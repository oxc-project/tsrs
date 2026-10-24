use std::num::NonZeroU32;
use std::sync::Arc;

use tsrs_core::TextRange;

use tsrs_ast::storage::{
    AstStoreBuilder, AstStoreError, AstStoreLease, ChildCursor, ChildEdge, NodeKey, PayloadTag,
    TextInput,
};
use tsrs_ast::{Kind, ModifierFlags, NodeFlags, TokenFlags};

#[test]
fn mutable_columns_survive_growth_and_freeze_without_changing_identity() {
    let mut ast = AstStoreBuilder::new("left + right").unwrap();
    let left = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 4)).unwrap();
    let right = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(7, 12)).unwrap();
    let parent =
        ast.new_node(Kind::BinaryExpression, NodeFlags::None, TextRange::new(0, 12)).unwrap();
    ast.set_parent(left, Some(parent)).unwrap();
    ast.set_parent(right, Some(parent)).unwrap();
    let copied_parent = ast.node(left).unwrap().parent().unwrap();
    for _ in 0..8192 {
        ast.new_node(Kind::Unknown, NodeFlags::None, TextRange::synthetic()).unwrap();
    }
    ast.set_flags(copied_parent, NodeFlags::ContainsThis).unwrap();
    let store = ast.finish();
    assert_eq!(store.node(left).unwrap().source_text(), Some("left"));
    assert_eq!(store.node(right).unwrap().parent(), Some(parent));
    assert_eq!(store.node(parent).unwrap().flags(), NodeFlags::ContainsThis);
    assert_eq!(store.node(parent).unwrap().key(), parent);
}

#[test]
fn foreign_edges_are_qualified_and_deduplicated() {
    let mut source = AstStoreBuilder::new("x").unwrap();
    let original =
        source.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 1)).unwrap();
    let mut synthetic = AstStoreBuilder::new("").unwrap();
    for _ in 0..100 {
        let node = synthetic
            .new_node(Kind::Identifier, NodeFlags::Synthesized, TextRange::synthetic())
            .unwrap();
        synthetic.set_parent(node, Some(original)).unwrap();
        assert_eq!(synthetic.node(node).unwrap().parent(), Some(original));
    }
    assert_eq!(synthetic.memory().foreign.live_bytes / size_of::<NodeKey>(), 1);
    let frozen = synthetic.finish();
    assert_eq!(frozen.memory().foreign.live_bytes / size_of::<NodeKey>(), 1);
    assert!(matches!(frozen.node(original), Err(AstStoreError::WrongStore { .. })));
}

#[test]
fn stale_or_wrong_store_keys_cannot_alias_another_node() {
    let mut first = AstStoreBuilder::new("a").unwrap();
    let old = first.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 1)).unwrap();
    drop(first);
    let mut second = AstStoreBuilder::new("b").unwrap();
    let current = second.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 1)).unwrap();
    assert_eq!(old.index(), current.index());
    assert_ne!(old.store(), current.store());
    assert!(matches!(second.node(old), Err(AstStoreError::WrongStore { .. })));
    assert!(matches!(
        second.set_flags(old, NodeFlags::None),
        Err(AstStoreError::WrongStore { .. })
    ));
}

#[test]
fn published_views_retain_storage_through_their_lease() {
    let mut builder = AstStoreBuilder::new("value").unwrap();
    let key = builder.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 5)).unwrap();
    let store = Arc::new(builder.finish());
    let weak = Arc::downgrade(&store);
    let lease = AstStoreLease::from(Arc::clone(&store));
    drop(store);
    assert_eq!(lease.node(key).unwrap().source_text(), Some("value"));
    assert!(weak.upgrade().is_some());
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn source_views_validate_utf8_boundaries_and_synthetic_ranges() {
    let mut ast = AstStoreBuilder::new("é").unwrap();
    let key = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 2)).unwrap();
    assert_eq!(ast.node(key).unwrap().source_text(), Some("é"));
    ast.set_range(key, TextRange::new(0, 1)).unwrap();
    assert_eq!(ast.node(key).unwrap().source_text(), None);
    ast.set_range(key, TextRange::synthetic()).unwrap();
    assert_eq!(ast.node(key).unwrap().source_text(), None);
}

#[test]
fn semantic_id_assignment_is_lazy_and_has_one_winner() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let key = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    std::thread::scope(|scope| {
        let mut threads = Vec::new();
        for proposed in 1..=8 {
            let lease = &lease;
            threads.push(scope.spawn(move || {
                lease.node(key).unwrap().get_or_assign_id(|| NonZeroU32::new(proposed).unwrap())
            }));
        }
        let assigned: Vec<_> = threads.into_iter().map(|thread| thread.join().unwrap()).collect();
        assert!(assigned.iter().all(|&id| id == assigned[0]));
        assert_eq!(
            lease.node(key).unwrap().get_or_assign_id(|| panic!("id allocator must not run twice")),
            assigned[0]
        );
    });
}

#[test]
fn lists_preserve_identity_order_and_spans_during_recursive_mutation() {
    let mut ast = AstStoreBuilder::new("a,b,c,").unwrap();
    let mut nodes = Vec::new();
    for offset in [0, 2, 4] {
        nodes.push(
            ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(offset, offset + 1))
                .unwrap(),
        );
    }
    let list = ast.new_node_list(TextRange::new(0, 6), &nodes).unwrap();
    let clone = ast.clone_node_list(list).unwrap();
    assert_ne!(clone, list);
    ast.set_list_range(clone, TextRange::new(0, 5)).unwrap();

    // The parser/binder traversal pattern keeps only copied keys across writes;
    // growth cannot invalidate an element or require copying the whole list.
    let count = ast.list(list).unwrap().len();
    for index in 0..count {
        let key = ast.list(list).unwrap().get(index).unwrap();
        ast.set_flags(key, NodeFlags::ContainsThis).unwrap();
        ast.new_node_list(TextRange::synthetic(), &[key]).unwrap();
    }
    assert_eq!(ast.list(list).unwrap().nodes().collect::<Vec<_>>(), nodes);
    assert!(
        ast.list(list).unwrap().has_trailing_comma(Some(&ast.node(nodes[2]).unwrap())).unwrap()
    );
    assert!(
        !ast.list(clone).unwrap().has_trailing_comma(Some(&ast.node(nodes[2]).unwrap())).unwrap()
    );
    ast.mark_trailing_comma(nodes[2]).unwrap();
    assert!(
        ast.list(clone).unwrap().has_trailing_comma(Some(&ast.node(nodes[2]).unwrap())).unwrap()
    );
    let ast = ast.finish();
    assert_eq!(
        ast.list(list).unwrap().nodes().rev().collect::<Vec<_>>(),
        [nodes[2], nodes[1], nodes[0]]
    );
    for key in nodes {
        assert_eq!(ast.node(key).unwrap().flags(), NodeFlags::ContainsThis);
    }
}

#[test]
fn empty_modifier_and_foreign_lists_keep_their_distinct_contracts() {
    let mut source = AstStoreBuilder::new("export").unwrap();
    let export =
        source.new_node(Kind::ExportKeyword, NodeFlags::None, TextRange::new(0, 6)).unwrap();
    let mut target = AstStoreBuilder::new("").unwrap();
    let empty = target.new_node_list(TextRange::synthetic(), &[]).unwrap();
    assert!(target.list(empty).unwrap().is_empty());
    assert!(!target.list(empty).unwrap().is_missing());
    assert!(!target.list(empty).unwrap().has_trailing_comma(None).unwrap());
    let modifiers =
        target.new_modifier_list(TextRange::new(0, 6), &[export], ModifierFlags::Export).unwrap();
    let clone = target.clone_modifier_list(modifiers).unwrap();
    assert_ne!(clone, modifiers);
    target.set_modifier_list_range(clone, TextRange::synthetic()).unwrap();
    let frozen = AstStoreLease::from(target.finish());
    let view = frozen.modifier_list(modifiers).unwrap();
    assert_eq!(view.modifier_flags(), ModifierFlags::Export);
    assert_eq!(view.list().get(0), Some(export));
    assert_eq!(frozen.modifier_list(clone).unwrap().list().get(0), Some(export));
    assert_eq!(frozen.modifier_list(clone).unwrap().list().range(), TextRange::synthetic());
    assert!(!view.list().has_trailing_comma(Some(&source.node(export).unwrap())).unwrap());
    assert!(matches!(
        view.list().has_trailing_comma(None),
        Err(AstStoreError::ListLastNodeMismatch { .. })
    ));
    assert!(matches!(source.list(empty), Err(AstStoreError::WrongStore { .. })));
}

#[test]
fn parser_missing_lists_keep_their_sentinel_through_clone_and_freeze() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let missing = ast.new_missing_node_list(TextRange::synthetic()).unwrap();
    let empty = ast.new_node_list(TextRange::synthetic(), &[]).unwrap();
    let clone = ast.clone_node_list(missing).unwrap();
    let ast = ast.finish();
    for key in [missing, clone] {
        let list = ast.list(key).unwrap();
        assert!(list.is_empty());
        assert!(list.is_missing());
        assert_eq!(list.nodes().next(), None);
        assert!(!list.has_trailing_comma(None).unwrap());
    }
    assert!(!ast.list(empty).unwrap().is_missing());
}

#[test]
fn list_owner_checks_preserve_local_identity_and_elements() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let node = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 1)).unwrap();
    let mut other = AstStoreBuilder::new("").unwrap();
    let foreign = other.new_node_list(TextRange::synthetic(), &[]).unwrap();
    assert!(matches!(ast.list(foreign), Err(AstStoreError::WrongStore { .. })));
    assert!(matches!(
        ast.set_list_range(foreign, TextRange::synthetic()),
        Err(AstStoreError::WrongStore { .. })
    ));
    let list = ast.new_node_list(TextRange::synthetic(), &[node]).unwrap();
    assert_eq!(list.index().offset(), 0);
    assert_eq!(ast.list(list).unwrap().nodes().collect::<Vec<_>>(), [node]);
}

#[test]
fn generated_payloads_keep_cooked_text_and_validate_casts_and_factory_masks() {
    let mut ast = AstStoreBuilder::new("('é', 0xff)").unwrap();
    let text = ast
        .new_string_literal(
            NodeFlags::None,
            TextRange::new(1, 5),
            TextInput::Source(TextRange::new(2, 4)),
            TokenFlags::SingleQuote | TokenFlags::PrecedingLineBreak | TokenFlags::HexSpecifier,
        )
        .unwrap();
    let number = ast
        .new_numeric_literal(
            NodeFlags::None,
            TextRange::new(7, 11),
            TextInput::Owned("255"),
            TokenFlags::HexSpecifier | TokenFlags::SingleQuote,
        )
        .unwrap();
    let paren =
        ast.new_parenthesized_expression(NodeFlags::None, TextRange::new(0, 12), text).unwrap();
    assert_eq!(ast.node(text).unwrap().as_string_literal().unwrap().text(), "é");
    assert_eq!(
        ast.node(text).unwrap().as_string_literal().unwrap().token_flags(),
        TokenFlags::SingleQuote
    );
    assert_eq!(ast.node(number).unwrap().as_numeric_literal().unwrap().text(), "255");
    assert_eq!(
        ast.node(number).unwrap().as_numeric_literal().unwrap().token_flags(),
        TokenFlags::HexSpecifier
    );
    assert_eq!(ast.text_count(), 2);
    ast.set_parenthesized_expression_expression(paren, number).unwrap();
    assert_eq!(
        ast.node(paren).unwrap().as_parenthesized_expression().unwrap().expression(),
        number
    );
    assert!(matches!(
        ast.node(number).unwrap().as_string_literal(),
        Err(AstStoreError::WrongPayload { .. })
    ));
    assert!(matches!(
        ast.set_parenthesized_expression_expression(text, number),
        Err(AstStoreError::WrongPayload { .. })
    ));
    ast.set_string_literal_token_flags(text, TokenFlags::None).unwrap();
    // Text is an independent scanner value; changing the node range cannot change it.
    ast.set_range(text, TextRange::synthetic()).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    assert_eq!(lease.node(text).unwrap().as_string_literal().unwrap().text(), "é");
    assert_eq!(
        lease.node(text).unwrap().as_string_literal().unwrap().token_flags(),
        TokenFlags::None
    );
    assert_eq!(lease.node(number).unwrap().payload_tag(), PayloadTag::NumericLiteral);
}

#[test]
fn generated_factories_reject_bad_text_without_exposing_nodes() {
    let mut ast = AstStoreBuilder::new("é").unwrap();
    for range in [TextRange::new(0, 1), TextRange::synthetic(), TextRange::new(2, 1)] {
        assert_eq!(
            ast.new_string_literal(
                NodeFlags::None,
                TextRange::synthetic(),
                TextInput::Source(range),
                TokenFlags::None
            ),
            Err(AstStoreError::InvalidTextRange(range))
        );
    }
    assert_eq!(ast.text_count(), 0);
    let token = ast.new_token(Kind::PlusToken, NodeFlags::None, TextRange::synthetic()).unwrap();
    assert_eq!(token.index().offset(), 0);
    assert_eq!(ast.node(token).unwrap().payload_tag(), PayloadTag::Token);
    assert_eq!(
        ast.new_token(Kind::FunctionDeclaration, NodeFlags::None, TextRange::synthetic()),
        Err(AstStoreError::UnsupportedPayload(Kind::FunctionDeclaration))
    );
    let mut other = AstStoreBuilder::new("").unwrap();
    let foreign =
        other.new_parenthesized_expression(NodeFlags::None, TextRange::synthetic(), token).unwrap();
    assert_eq!(
        other.node(foreign).unwrap().as_parenthesized_expression().unwrap().expression(),
        token
    );
}

#[test]
fn owned_text_views_survive_growth_and_release_all_backing_storage() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let input = String::from("cooked\nvalue");
    let first = ast
        .new_string_literal(
            NodeFlags::None,
            TextRange::synthetic(),
            TextInput::Owned(&input),
            TokenFlags::None,
        )
        .unwrap();
    drop(input);
    for _ in 0..128 {
        ast.new_string_literal(
            NodeFlags::None,
            TextRange::synthetic(),
            TextInput::Owned("more text"),
            TokenFlags::None,
        )
        .unwrap();
    }
    let store = Arc::new(ast.finish());
    let weak = Arc::downgrade(&store);
    let lease = AstStoreLease::from(store);
    assert_eq!(lease.node(first).unwrap().as_string_literal().unwrap().text(), "cooked\nvalue");
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn grouped_node_tables_roll_over_without_changing_keys_or_child_edges() {
    let mut ast = AstStoreBuilder::new("1").unwrap();
    let token = ast.new_token(Kind::PlusToken, NodeFlags::None, TextRange::synthetic()).unwrap();
    let seed = ast
        .new_numeric_literal(
            NodeFlags::None,
            TextRange::new(0, 1),
            TextInput::Source(TextRange::new(0, 1)),
            TokenFlags::None,
        )
        .unwrap();
    let paren_seed =
        ast.new_parenthesized_expression(NodeFlags::None, TextRange::synthetic(), seed).unwrap();
    super::storage_helpers::fill_table(&mut ast, seed, 7);
    super::storage_helpers::fill_table(&mut ast, paren_seed, 7);
    let before_count = ast.node_count();
    let mut nodes = Vec::new();
    for _ in 0..35 {
        let number = ast
            .new_numeric_literal(
                NodeFlags::None,
                TextRange::new(0, 1),
                TextInput::Source(TextRange::new(0, 1)),
                TokenFlags::None,
            )
            .unwrap();
        let paren = ast
            .new_parenthesized_expression(NodeFlags::None, TextRange::synthetic(), number)
            .unwrap();
        ast.set_parent(number, Some(paren)).unwrap();
        nodes.push((number, paren));
    }
    assert_eq!(ast.node_count(), before_count + 70);
    assert_ne!(
        nodes[0].0.index().offset() / super::storage_helpers::TABLE_CAPACITY,
        nodes[7].0.index().offset() / super::storage_helpers::TABLE_CAPACITY
    );
    assert_ne!(
        nodes[0].1.index().offset() / super::storage_helpers::TABLE_CAPACITY,
        nodes[7].1.index().offset() / super::storage_helpers::TABLE_CAPACITY
    );
    assert_eq!(nodes[7].0.index().offset() % super::storage_helpers::TABLE_CAPACITY, 0);
    for &(number, paren) in &nodes {
        // A typed table supplies its constant kind; no per-node kind/tag/index
        // columns are required for these concrete payloads.
        assert_eq!(ast.node(number).unwrap().kind(), Kind::NumericLiteral);
        ast.set_flags(number, NodeFlags::ContainsThis).unwrap();
        ast.set_parenthesized_expression_expression(paren, token).unwrap();
    }
    let lease = AstStoreLease::from(ast.finish());
    for (number, paren) in nodes {
        let node = lease.node(number).unwrap();
        assert_eq!(node.kind(), Kind::NumericLiteral);
        assert_eq!(node.as_numeric_literal().unwrap().text(), "1");
        assert_eq!(node.flags(), NodeFlags::ContainsThis);
        assert_eq!(node.parent(), Some(paren));
        assert_eq!(
            lease.node(paren).unwrap().as_parenthesized_expression().unwrap().expression(),
            token
        );
    }
}

#[test]
fn sparse_payloads_preserve_absent_empty_and_missing_lists_across_tables() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let expression = ast
        .new_numeric_literal(
            NodeFlags::None,
            TextRange::synthetic(),
            TextInput::Owned("1"),
            TokenFlags::None,
        )
        .unwrap();
    let empty = ast.new_node_list(TextRange::new(1, 1), &[]).unwrap();
    let missing = ast.new_missing_node_list(TextRange::new(2, 2)).unwrap();
    let types = ast.new_node_list(TextRange::new(3, 4), &[expression]).unwrap();
    let seed = ast
        .new_new_expression(NodeFlags::None, TextRange::synthetic(), expression, None, None)
        .unwrap();
    super::storage_helpers::fill_table(&mut ast, seed, 5);
    let mut nodes = Vec::new();
    for index in 0..48 {
        let type_arguments = (index % 7 == 0).then_some(types);
        let arguments = match index % 3 {
            0 => None,
            1 => Some(empty),
            _ => Some(missing),
        };
        let node = ast
            .new_new_expression(
                NodeFlags::None,
                TextRange::synthetic(),
                expression,
                type_arguments,
                arguments,
            )
            .unwrap();
        nodes.push((node, type_arguments, arguments));
    }
    let lease = AstStoreLease::from(ast.finish());
    for (key, type_arguments, arguments) in nodes {
        let node = lease.node(key).unwrap();
        assert_eq!(node.kind(), Kind::NewExpression);
        let new = node.as_new_expression().unwrap();
        assert_eq!(new.expression(), expression);
        assert_eq!(new.type_arguments(), type_arguments);
        assert_eq!(new.arguments(), arguments);
        if let Some(arguments) = new.arguments() {
            assert_eq!(lease.list(arguments).unwrap().is_missing(), arguments == missing);
        }
        if let Some(types) = new.type_arguments() {
            assert_eq!(lease.list(types).unwrap().nodes().collect::<Vec<_>>(), [expression]);
        }
    }
}

#[test]
fn typed_factories_preserve_an_allocated_empty_list() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let expression = ast
        .new_numeric_literal(
            NodeFlags::None,
            TextRange::synthetic(),
            TextInput::Owned("1"),
            TokenFlags::None,
        )
        .unwrap();
    let valid = ast.new_node_list(TextRange::synthetic(), &[]).unwrap();
    let key = ast
        .new_new_expression(NodeFlags::None, TextRange::synthetic(), expression, Some(valid), None)
        .unwrap();
    assert_eq!(ast.node(key).unwrap().as_new_expression().unwrap().type_arguments(), Some(valid));
    assert_eq!(ast.node_count(), 2);
}

#[test]
fn foreign_payload_lists_use_compact_deduplicated_edges() {
    let mut source = AstStoreBuilder::new("").unwrap();
    let expression = source
        .new_numeric_literal(
            NodeFlags::None,
            TextRange::synthetic(),
            TextInput::Owned("1"),
            TokenFlags::None,
        )
        .unwrap();
    let arguments = source.new_node_list(TextRange::synthetic(), &[expression]).unwrap();
    let source = AstStoreLease::from(source.finish());
    let mut ast = AstStoreBuilder::new("").unwrap();
    let mut keys = Vec::new();
    for _ in 0..80 {
        keys.push(
            ast.new_new_expression(
                NodeFlags::Synthesized,
                TextRange::synthetic(),
                expression,
                Some(arguments),
                Some(arguments),
            )
            .unwrap(),
        );
    }
    assert_eq!(ast.memory().foreign.live_bytes, 2 * size_of::<NodeKey>());
    let lease = AstStoreLease::from(ast.finish());
    for key in keys {
        let new = lease.node(key).unwrap().as_new_expression().unwrap();
        assert_eq!(new.expression(), expression);
        assert_eq!(new.type_arguments(), Some(arguments));
        let list = new.arguments().unwrap();
        assert_eq!(list, arguments);
        assert_eq!(source.list(list).unwrap().nodes().collect::<Vec<_>>(), [expression]);
        assert!(matches!(lease.list(list), Err(AstStoreError::WrongStore { .. })));
    }
}

#[test]
fn traversal_copies_edges_before_recursive_mutation_and_preserves_schema_order() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let mut make_name =
        || ast.new_token(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let (tag_name, name, type_) = (make_name(), make_name(), make_name());
    let comment = ast.new_node_list(TextRange::synthetic(), &[name]).unwrap();
    for is_name_first in [true, false] {
        let tag = ast
            .new_jsdoc_parameter_or_property_tag(
                NodeFlags::None,
                TextRange::synthetic(),
                Kind::JSDocParameterTag,
                name,
                false,
                Some(type_),
                is_name_first,
                tag_name,
                Some(comment),
            )
            .unwrap();
        let mut cursor = ChildCursor::new(tag);
        let mut edges = Vec::new();
        while let Some(edge) = cursor.next(&ast).unwrap() {
            edges.push(edge);
            if let ChildEdge::Node(child) = edge {
                // The traversal state has no borrowed view across writes/growth.
                ast.set_parent(child, Some(tag)).unwrap();
                ast.clone_node(child).unwrap();
            }
        }
        let (first, second) = if is_name_first { (name, type_) } else { (type_, name) };
        assert_eq!(
            edges,
            [
                ChildEdge::Node(tag_name),
                ChildEdge::Node(first),
                ChildEdge::Node(second),
                ChildEdge::List(comment),
            ]
        );
        assert_eq!(cursor.next(&ast).unwrap(), None);
    }
    let raw =
        ast.new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[name, type_, name]).unwrap();
    let mut cursor = ChildCursor::new(raw);
    assert_eq!(cursor.next(&ast).unwrap(), Some(ChildEdge::Node(name)));
    ast.new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[tag_name]).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    assert_eq!(cursor.next(&lease).unwrap(), Some(ChildEdge::Node(type_)));
    assert_eq!(cursor.next(&lease).unwrap(), Some(ChildEdge::Node(name)));
    assert_eq!(cursor.next(&lease).unwrap(), None);
}

#[test]
fn shallow_clones_reset_identity_and_binding_but_share_immutable_spans() {
    let mut ast = AstStoreBuilder::new("x").unwrap();
    let mut bindings = tsrs_ast::storage::BindingData::new(&mut ast).unwrap();
    let name = ast.new_token(Kind::Identifier, NodeFlags::None, TextRange::new(0, 1)).unwrap();
    let parent = ast
        .new_new_expression(NodeFlags::ContainsThis, TextRange::new(0, 1), name, None, None)
        .unwrap();
    ast.set_parent(parent, Some(name)).unwrap();
    ast.mark_trailing_comma(parent).unwrap();
    ast.node(parent).unwrap().get_or_assign_id(|| NonZeroU32::new(7).unwrap());
    let symbol = bindings
        .new_symbol(tsrs_ast::SymbolFlags::FunctionScopedVariable, TextInput::Owned("x"))
        .unwrap();
    let declaration = ast
        .new_variable_declaration(NodeFlags::None, TextRange::new(0, 1), name, None, None, None)
        .unwrap();
    bindings
        .set_variable_declaration_symbol(&ast.node(declaration).unwrap(), Some(symbol))
        .unwrap();
    let declaration_clone = ast.clone_node(declaration).unwrap();
    assert_eq!(
        bindings.variable_declaration(&ast.node(declaration_clone).unwrap()).unwrap().symbol(),
        None
    );
    super::storage_helpers::fill_table(&mut ast, parent, 1);
    let first_clone = ast.clone_node(parent).unwrap();
    let second_clone = ast.clone_node(parent).unwrap();
    assert_ne!(first_clone, second_clone);
    assert_ne!(
        parent.index().offset() / super::storage_helpers::TABLE_CAPACITY,
        second_clone.index().offset() / super::storage_helpers::TABLE_CAPACITY
    );
    let cloned = ast.node(second_clone).unwrap();
    assert_eq!(cloned.range(), TextRange::new(0, 1));
    assert_eq!(cloned.flags(), NodeFlags::ContainsThis);
    assert_eq!(cloned.parent(), None);
    assert!(!cloned.has_trailing_comma_marker());
    assert_eq!(cloned.get_or_assign_id(|| NonZeroU32::new(9).unwrap()).get(), 9);
    assert_eq!(cloned.as_new_expression().unwrap().expression(), name);
    let raw = ast
        .new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[parent, declaration])
        .unwrap();
    let elements = ast.memory().lists;
    let clone = ast.clone_node(raw).unwrap();
    assert_eq!(elements, ast.memory().lists);
    let lease = AstStoreLease::from(ast.finish());
    assert_eq!(
        lease.node(clone).unwrap().as_syntax_list().unwrap().children().nodes().collect::<Vec<_>>(),
        [parent, declaration]
    );
}

#[test]
fn token_factories_keep_canonical_payloads_without_counting_text() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    for value in 0..Kind::Count as i16 {
        let kind = Kind::from_i16(value);
        let before = ast.node_count();
        let result = ast.new_token(kind, NodeFlags::None, TextRange::synthetic());
        if tsrs_ast::is_token_kind(kind) {
            let key = result.unwrap();
            let view = ast.node(key).unwrap();
            assert_eq!(view.kind(), kind);
            assert_eq!(Some(view.payload_tag()), PayloadTag::for_kind(kind));
            assert_eq!(ast.node_count(), before + 1);
        } else {
            assert_eq!(result, Err(AstStoreError::UnsupportedPayload(kind)));
            assert_eq!(ast.node_count(), before);
        }
    }
    assert_eq!(ast.text_count(), 0);
    assert_eq!(ast.memory().text.allocations, 0);
    let name = ast.new_token(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let template =
        ast.new_token(Kind::TemplateHead, NodeFlags::None, TextRange::synthetic()).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    assert_eq!(lease.node(name).unwrap().as_identifier().unwrap().text(), "");
    let view = lease.node(template).unwrap().as_template_head().unwrap();
    assert_eq!(view.text(), "");
    assert_eq!(view.raw_text(), "");
    assert_eq!(view.token_flags(), TokenFlags::None);
}

#[test]
fn checked_factories_keep_alias_kinds_and_schema_flags() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let name = ast.new_token(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let access = ast
        .new_property_access_expression(
            NodeFlags::OptionalChain | NodeFlags::ContainsThis,
            TextRange::synthetic(),
            name,
            None,
            name,
        )
        .unwrap();
    assert_eq!(ast.node(access).unwrap().flags(), NodeFlags::OptionalChain);
    for kind in [Kind::TypeAliasDeclaration, Kind::JSTypeAliasDeclaration] {
        let key = ast
            .new_type_alias_declaration(
                NodeFlags::None,
                TextRange::synthetic(),
                kind,
                name,
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(ast.node(key).unwrap().kind(), kind);
        ast.node(key).unwrap().as_type_alias_declaration().unwrap();
    }
    let before = ast.node_count();
    assert_eq!(
        ast.new_keyword_type_node(NodeFlags::None, TextRange::synthetic(), Kind::NullKeyword),
        Err(AstStoreError::UnsupportedPayload(Kind::NullKeyword))
    );
    assert_eq!(ast.node_count(), before);
    let number = ast
        .new_keyword_type_node(NodeFlags::None, TextRange::synthetic(), Kind::NumberKeyword)
        .unwrap();
    ast.node(number).unwrap().as_keyword_type_node().unwrap();
}

#[test]
fn raw_children_and_mixed_jsdoc_text_keep_owned_spans_through_growth() {
    let mut ast = AstStoreBuilder::new("é source").unwrap();
    let mut other = AstStoreBuilder::new("").unwrap();
    let foreign =
        other.new_token(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let local = ast.new_token(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let children = ast
        .new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[local, foreign, local])
        .unwrap();
    let cooked = String::from("cooked\n文");
    let texts = [
        TextInput::Source(TextRange::new(0, 2)),
        TextInput::Owned(&cooked),
        TextInput::Source(TextRange::new(3, 9)),
    ];
    let comment = ast.new_jsdoc_text(NodeFlags::None, TextRange::synthetic(), &texts).unwrap();
    let link =
        ast.new_jsdoc_link(NodeFlags::None, TextRange::synthetic(), Some(local), &texts).unwrap();
    let empty = ast.new_jsdoc_text(NodeFlags::None, TextRange::synthetic(), &[]).unwrap();
    drop(cooked);
    for _ in 0..128 {
        ast.new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[foreign, local]).unwrap();
        ast.new_jsdoc_text(
            NodeFlags::None,
            TextRange::synthetic(),
            &[TextInput::Owned("more\ntext"), TextInput::Source(TextRange::new(0, 2))],
        )
        .unwrap();
    }
    let before = ast.node_count();
    let text_bytes = ast.memory().text.live_bytes;
    assert_eq!(
        ast.new_jsdoc_text(
            NodeFlags::None,
            TextRange::synthetic(),
            &[TextInput::Owned("valid"), TextInput::Source(TextRange::new(0, 1))],
        ),
        Err(AstStoreError::InvalidTextRange(TextRange::new(0, 1)))
    );
    assert_eq!(ast.memory().text.live_bytes, text_bytes);
    assert_eq!(ast.node_count(), before);
    let lease = AstStoreLease::from(ast.finish());
    let nodes = lease.node(children).unwrap().as_syntax_list().unwrap().children();
    assert_eq!(nodes.nodes().collect::<Vec<_>>(), [local, foreign, local]);
    assert_eq!(nodes.get(1), Some(foreign));
    assert_eq!(nodes.get(3), None);
    assert_eq!(nodes.nodes().rev().collect::<Vec<_>>(), [local, foreign, local]);
    let text = lease.node(comment).unwrap().as_jsdoc_text().unwrap().text();
    assert_eq!(text.texts().collect::<Vec<_>>(), ["é", "cooked\n文", "source"]);
    assert_eq!(text.texts().rev().collect::<Vec<_>>(), ["source", "cooked\n文", "é"]);
    assert_eq!(text.get(3), None);
    let text = lease.node(link).unwrap().as_jsdoc_link().unwrap().text();
    assert_eq!(text.texts().collect::<Vec<_>>(), ["é", "cooked\n文", "source"]);
    assert!(lease.node(empty).unwrap().as_jsdoc_text().unwrap().text().is_empty());
}

#[test]
fn memory_accounting_includes_table_metadata_and_only_present_rare_values() {
    let empty = AstStoreBuilder::new("source").unwrap().finish();
    let memory = empty.memory();
    assert_eq!(memory.buffers(), tsrs_ast::storage::AstBufferMemory::default());
    assert_eq!(memory.inline_bytes, size_of::<tsrs_ast::storage::AstStore>());
    assert_eq!(memory.source_bytes, 6);

    let build = |rare: bool| {
        let mut ast = AstStoreBuilder::new("").unwrap();
        let expression =
            ast.new_token(Kind::PlusToken, NodeFlags::None, TextRange::synthetic()).unwrap();
        let list = ast.new_node_list(TextRange::synthetic(), &[expression]).unwrap();
        for index in 0..100 {
            ast.new_new_expression(
                NodeFlags::None,
                TextRange::synthetic(),
                expression,
                (rare && index % 20 == 0).then_some(list),
                Some(list),
            )
            .unwrap();
        }
        ast.finish().memory()
    };
    let absent = build(false);
    let present = build(true);
    // Five sparse entries, each a checked element address plus a compact edge.
    assert_eq!(present.nodes.live_bytes - absent.nodes.live_bytes, 5 * 8);
    assert_eq!(present.nodes.allocations - absent.nodes.allocations, 1);
    assert_eq!(absent.lists, present.lists);
    assert_eq!(absent.foreign, present.foreign);
    assert!(present.nodes.capacity_bytes >= present.nodes.live_bytes);
    assert!(absent.nodes.capacity_bytes >= absent.nodes.live_bytes);
}
