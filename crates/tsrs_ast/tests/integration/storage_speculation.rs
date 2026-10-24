use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use tsrs_ast::{Kind, ModifierFlags, NodeFlags};
use tsrs_core::TextRange;

use tsrs_ast::storage::TextInput;
use tsrs_ast::storage::*;

const RANGE: TextRange = TextRange::synthetic();

#[test]
fn borrowed_writers_leave_rollback_and_retention_with_the_owning_scope() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(RollbackBuffers::Reuse, |scope| {
        {
            let mut writer = scope.borrow_writer();
            let child = leaf(&mut writer, "temporary");
            writer.look_ahead(RollbackBuffers::Reuse, |inner| {
                leaf(inner, "nested");
            });
            assert_eq!(writer.node(child).unwrap().as_identifier().unwrap().text(), "temporary");
            assert!(!writer.has_checkpoint());
        }
        assert!(scope.has_checkpoint());
        assert_eq!(scope.node_table_lengths(), [1]);
    });
    assert_eq!(ast.node_table_lengths(), [] as [usize; 0]);
    let mut retained = None;
    ast.look_ahead(RollbackBuffers::Reuse, |scope| {
        let mut writer = scope.borrow_writer();
        let child = leaf(&mut writer, "retained");
        retained = Some(writer.retain_node(child));
    });
    assert_eq!(ast.node(retained.unwrap()).unwrap().as_identifier().unwrap().text(), "retained");
    assert_eq!(ast.node_count(), 3);
}

fn leaf<'brand>(scope: &mut AstScope<'brand, '_>, text: &str) -> ScopedNode<'brand> {
    scope.new_identifier(NodeFlags::None, RANGE, TextInput::Owned(text)).unwrap()
}

#[test]
fn nested_commit_promotes_only_to_its_parent_and_counters_survive_rejection() {
    let source: Arc<str> = Arc::from("source");
    let mut ast = AstStoreBuilder::new(Arc::clone(&source)).unwrap();
    let retained =
        ast.new_identifier(NodeFlags::None, RANGE, TextInput::Owned("retained")).unwrap();
    let before = ast.node_table_lengths();
    ast.look_ahead(RollbackBuffers::Reuse, |scope| {
        let first = leaf(scope, "outer");
        let nested = scope
            .try_parse_node(RollbackBuffers::Reuse, |inner| {
                let first = inner.import_scoped_node(first)?;
                let second = leaf(inner, "inner");
                Ok(Some(inner.new_syntax_list(NodeFlags::None, RANGE, &[first, second])?))
            })
            .unwrap()
            .unwrap();
        assert_eq!(scope.node(nested).unwrap().as_syntax_list().unwrap().children().len(), 2);
        let mut cursor = ScopedChildCursor::new(nested);
        assert_eq!(cursor.next(scope).unwrap(), Some(ScopedChildEdge::Node(first)));
        let ScopedChildEdge::Node(second) = cursor.next(scope).unwrap().unwrap() else {
            panic!("raw syntax span")
        };
        scope.set_flags(second, NodeFlags::ContainsThis).unwrap();
        assert!(cursor.next(scope).unwrap().is_none());
        assert_eq!(scope.node(second).unwrap().as_identifier().unwrap().text(), "inner");
    });
    assert_eq!(ast.node_table_lengths(), before);
    assert_eq!(ast.node_count(), 4);
    assert_eq!(ast.text_count(), 3);
    let committed = ast
        .try_parse_node(RollbackBuffers::Reuse, |scope| Ok(Some(leaf(scope, "committed"))))
        .unwrap()
        .unwrap();
    assert_ne!(committed, retained);
    drop(source);
    let store = ast.finish();
    assert_eq!(store.node(retained).unwrap().as_identifier().unwrap().text(), "retained");
    assert_eq!(store.node(committed).unwrap().as_identifier().unwrap().text(), "committed");
}

#[test]
fn retention_pins_all_ancestors_even_when_the_nested_parse_rejects() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let mut escaped = None;
    ast.look_ahead(RollbackBuffers::Release, |scope| {
        leaf(scope, "before nested");
        scope.look_ahead(RollbackBuffers::Release, |inner| {
            let node = leaf(inner, "escaped");
            escaped = Some(inner.retain_node(node));
        });
        leaf(scope, "after nested");
    });
    assert_eq!(ast.node_count(), 3);
    assert_eq!(ast.node_table_lengths(), [3]);
    assert_eq!(ast.node(escaped.unwrap()).unwrap().as_identifier().unwrap().text(), "escaped");
}

#[test]
fn writing_a_temporary_edge_to_a_stable_node_cancels_reclamation() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let seed = ast.new_identifier(NodeFlags::None, RANGE, TextInput::Owned("initial")).unwrap();
    let stable = ast.new_parenthesized_expression(NodeFlags::None, RANGE, seed).unwrap();
    ast.look_ahead(RollbackBuffers::Release, |scope| {
        let stable = scope.import_node(stable);
        scope.look_ahead(RollbackBuffers::Release, |inner| {
            let stable = inner.import_scoped_node(stable).unwrap();
            let child = leaf(inner, "edge escape");
            inner.set_parenthesized_expression_expression(stable, child).unwrap();
        });
    });
    let child = ast.node(stable).unwrap().as_parenthesized_expression().unwrap().expression();
    assert_eq!(ast.node(child).unwrap().as_identifier().unwrap().text(), "edge escape");
}

#[test]
fn inner_edge_escape_pins_only_frames_whose_target_predates_the_frame() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(RollbackBuffers::Release, |scope| {
        let parent = leaf(scope, "outer temporary");
        scope.look_ahead(RollbackBuffers::Release, |inner| {
            let parent = inner.import_scoped_node(parent).unwrap();
            let child = leaf(inner, "inner temporary");
            inner.set_parent(parent, Some(child)).unwrap();
        });
        assert!(scope.node(parent).unwrap().parent().is_some());
    });
    assert_eq!(ast.node_table_lengths(), [] as [usize; 0]);
    assert_eq!(ast.node_count(), 2);
}

#[test]
fn foreign_nodes_lists_and_modifiers_restore_their_deduplication_indexes() {
    let mut foreign = AstStoreBuilder::new("").unwrap();
    let node = foreign.new_identifier(NodeFlags::None, RANGE, TextInput::Owned("foreign")).unwrap();
    let list = foreign.new_node_list(RANGE, &[node]).unwrap();
    let modifiers = foreign.new_modifier_list(RANGE, &[node], ModifierFlags::None).unwrap();
    let mut ast = AstStoreBuilder::new("").unwrap();
    for _ in 0..3 {
        ast.look_ahead(RollbackBuffers::Reuse, |scope| {
            let node = scope.import_node(node);
            let list = scope.import_list(list);
            let modifiers = scope.import_modifiers(modifiers);
            scope
                .new_call_expression(NodeFlags::None, RANGE, node, Some(node), Some(list), list)
                .unwrap();
            let declaration = scope
                .new_type_alias_declaration(
                    NodeFlags::None,
                    RANGE,
                    Kind::TypeAliasDeclaration,
                    node,
                    Some(list),
                    Some(node),
                    Some(modifiers),
                )
                .unwrap();
            assert_eq!(
                scope.node(declaration).unwrap().as_type_alias_declaration().unwrap().name(),
                node
            );
        });
        assert_eq!(ast.memory().foreign.live_bytes, 0);
    }
    let result =
        ast.new_call_expression(NodeFlags::None, RANGE, node, None, Some(list), list).unwrap();
    assert_eq!(ast.node(result).unwrap().as_call_expression().unwrap().arguments(), list);
    assert_eq!(ast.memory().foreign.live_bytes, size_of::<NodeKey>() + size_of::<ListKey>());
}

#[test]
fn sparse_columns_rollover_and_comma_bits_can_be_reused_without_stale_values() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let node = ast.new_token(Kind::QuestionDotToken, NodeFlags::None, RANGE).unwrap();
    let stable =
        ast.new_property_access_expression(NodeFlags::None, RANGE, node, None, node).unwrap();
    super::storage_helpers::fill_table(&mut ast, stable, 1);
    let lengths = ast.node_table_lengths();
    ast.look_ahead(RollbackBuffers::Reuse, |scope| {
        let node = scope.import_node(node);
        for _ in 0..8 {
            let key = scope
                .new_property_access_expression(
                    NodeFlags::OptionalChain,
                    RANGE,
                    node,
                    Some(node),
                    node,
                )
                .unwrap();
            scope.mark_trailing_comma(key).unwrap();
        }
    });
    assert_eq!(ast.node_table_lengths(), lengths);
    let key = ast.new_property_access_expression(NodeFlags::None, RANGE, node, None, node).unwrap();
    assert_eq!(
        ast.node(key).unwrap().as_property_access_expression().unwrap().question_dot_token(),
        None
    );
    assert!(!ast.node(key).unwrap().has_trailing_comma_marker());
    assert!(!ast.node(stable).unwrap().has_trailing_comma_marker());
}

#[test]
fn panic_and_error_paths_restore_text_list_modifiers_and_partial_factory_writes() {
    let mut ast = AstStoreBuilder::new("é").unwrap();
    let stable = ast.new_identifier(NodeFlags::None, RANGE, TextInput::Owned("keep")).unwrap();
    let before = ast.node_table_lengths();
    let lists_before = ast.memory().lists;
    let text_before = ast.memory().text;
    assert!(
        catch_unwind(AssertUnwindSafe(|| ast.look_ahead(RollbackBuffers::Release, |scope| {
            let node = leaf(scope, "cooked discarded");
            scope.new_node_list(RANGE, &[node]).unwrap();
            scope.new_missing_node_list(RANGE).unwrap();
            scope.new_modifier_list(RANGE, &[node], ModifierFlags::None).unwrap();
            panic!("abandon scope");
        })))
        .is_err()
    );
    let error = ast.try_parse_node(RollbackBuffers::Release, |scope| {
        leaf(scope, "before failed field");
        scope.new_jsdoc_text(
            NodeFlags::None,
            RANGE,
            &[TextInput::Owned("partial"), TextInput::Source(TextRange::new(0, 1))],
        )?;
        Ok(None)
    });
    assert!(error.is_err());
    assert_eq!(ast.node_table_lengths(), before);
    assert_eq!(ast.memory().lists.live_bytes, lists_before.live_bytes);
    assert_eq!(ast.memory().text.live_bytes, text_before.live_bytes);
    assert_eq!(ast.node(stable).unwrap().as_identifier().unwrap().text(), "keep");
}

#[test]
fn release_policy_removes_rejected_capacity_and_token_only_scopes_have_no_checkpoint() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    ast.new_identifier(NodeFlags::None, RANGE, TextInput::Owned("stable")).unwrap();
    ast.look_ahead(RollbackBuffers::Reuse, |scope| {
        scope.look_ahead(RollbackBuffers::Reuse, |inner| assert!(!inner.has_checkpoint()));
        assert!(!scope.has_checkpoint());
        for _ in 0..4096 {
            leaf(scope, "large discarded body");
        }
    });
    let retained_capacity = ast.memory().nodes.capacity_bytes + ast.memory().text.capacity_bytes;
    ast.look_ahead(RollbackBuffers::Release, |scope| {
        leaf(scope, "force checkpoint");
    });
    let after = ast.memory().nodes + ast.memory().text;
    assert!(after.capacity_bytes < retained_capacity / 100);
    assert_eq!(ast.node_table_lengths(), [1]);
}

#[test]
fn foreign_temporary_handles_cannot_be_rebranded_into_an_independent_writer() {
    let mut first = AstStoreBuilder::new("").unwrap();
    let mut second = AstStoreBuilder::new("").unwrap();
    first.look_ahead(RollbackBuffers::Reuse, |outer| {
        let node = leaf(outer, "first");
        let list = outer.new_node_list(RANGE, &[node]).unwrap();
        let modifiers = outer.new_modifier_list(RANGE, &[node], ModifierFlags::None).unwrap();
        second.look_ahead(RollbackBuffers::Reuse, |inner| {
            assert!(inner.import_scoped_node(node).is_err());
            assert!(inner.import_scoped_list(list).is_err());
            assert!(inner.import_scoped_modifiers(modifiers).is_err());
        });
    });
    assert_eq!(first.node_table_lengths(), [] as [usize; 0]);
    assert_eq!(second.node_table_lengths(), [] as [usize; 0]);
}

#[test]
fn list_and_modifier_commits_keep_their_graphs_and_clones_after_later_rejection() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let list = ast
        .try_parse_list(RollbackBuffers::Reuse, |scope| {
            let node = leaf(scope, "list child");
            let list = scope.new_node_list(RANGE, &[node])?;
            Ok(Some(scope.clone_node_list(list)?))
        })
        .unwrap()
        .unwrap();
    let modifiers = ast
        .try_parse_modifiers(RollbackBuffers::Reuse, |scope| {
            let node = leaf(scope, "modifier child");
            let list = scope.new_modifier_list(RANGE, &[node], ModifierFlags::Export)?;
            Ok(Some(scope.clone_modifier_list(list)?))
        })
        .unwrap()
        .unwrap();
    ast.look_ahead(RollbackBuffers::Release, |scope| {
        for _ in 0..128 {
            leaf(scope, "discarded");
        }
    });
    let list_child = ast.list(list).unwrap().get(0).unwrap();
    let modifier_child = ast.modifier_list(modifiers).unwrap().list().get(0).unwrap();
    assert_eq!(ast.node(list_child).unwrap().as_identifier().unwrap().text(), "list child");
    assert_eq!(ast.node(modifier_child).unwrap().as_identifier().unwrap().text(), "modifier child");
    assert_eq!(ast.modifier_list(modifiers).unwrap().modifier_flags(), ModifierFlags::Export);
    assert_eq!(size_of::<ScopedNode<'_>>(), size_of::<NodeKey>());
    assert_eq!(size_of::<Option<ScopedList<'_>>>(), size_of::<Option<ListKey>>());
    assert!(size_of::<AstScope<'_, '_>>() <= 64);
}
