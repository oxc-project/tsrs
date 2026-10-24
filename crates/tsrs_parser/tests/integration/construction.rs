use tsrs_ast::storage::TextInput;
use tsrs_ast::{Kind, ModifierFlags};

use tsrs_ast::NodeFlags;
use tsrs_ast::storage::*;
use tsrs_core::TextRange;
use tsrs_parser::construction::*;

#[test]
fn owned_finalization_preserves_flags_error_consumption_and_immediate_parents() {
    let mut writer = AstStoreBuilder::new("f x").unwrap();
    let synthetic = TextRange::synthetic();
    let name = writer
        .new_identifier(NodeFlags::None, synthetic, TextInput::Source(TextRange::new(0, 1)))
        .unwrap();
    let parameter_name = writer
        .new_identifier(NodeFlags::None, synthetic, TextInput::Source(TextRange::new(2, 3)))
        .unwrap();
    let parameter = writer
        .new_parameter_declaration(
            NodeFlags::None,
            synthetic,
            None,
            parameter_name,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let mut has_parse_error = true;
    finish_node(
        &mut writer,
        parameter,
        TextRange::new(2, 3),
        NodeFlags::AwaitContext,
        &mut has_parse_error,
    )
    .unwrap();
    assert!(!has_parse_error);
    let modifier = writer.new_token(Kind::ExportKeyword, NodeFlags::None, synthetic).unwrap();
    let modifiers =
        writer.new_modifier_list(synthetic, &[modifier], ModifierFlags::Export).unwrap();
    let parameters = writer.new_node_list(synthetic, &[parameter]).unwrap();
    let function = writer
        .new_function_declaration(
            NodeFlags::Reparsed,
            synthetic,
            Some(name),
            Some(modifiers),
            None,
            Some(parameters),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        finish_node(
            &mut writer,
            function,
            TextRange::new(0, 3),
            NodeFlags::Ambient,
            &mut has_parse_error
        )
        .unwrap(),
        function
    );
    assert_eq!(writer.node(function).unwrap().flags(), NodeFlags::Reparsed | NodeFlags::Ambient);
    assert_eq!(writer.node(function).unwrap().range(), TextRange::new(0, 3));
    for child in [modifier, name, parameter] {
        assert_eq!(writer.node(child).unwrap().parent(), Some(function));
    }
    assert_eq!(writer.node(parameter_name).unwrap().parent(), Some(parameter));
    assert_eq!(
        writer.node(parameter).unwrap().flags(),
        NodeFlags::AwaitContext | NodeFlags::ThisNodeHasError
    );
    let store = writer.finish();
    assert_eq!(store.node(parameter_name).unwrap().as_identifier().unwrap().text(), "x");
    assert_eq!(store.node(function).unwrap().parent(), None);
}

#[test]
fn owned_child_callbacks_can_grow_columns_and_finish_nested_nodes() {
    let mut writer = AstStoreBuilder::new("").unwrap();
    let range = TextRange::synthetic();
    let first = writer.new_token(Kind::BreakKeyword, NodeFlags::None, range).unwrap();
    let second = writer.new_token(Kind::ContinueKeyword, NodeFlags::None, range).unwrap();
    let list = writer.new_node_list(range, &[first, second]).unwrap();
    let root = writer.new_block(NodeFlags::None, range, list, false).unwrap();
    let mut visited = Vec::new();
    writer
        .for_each_child(root, |writer, child| {
            for _ in 0..256 {
                writer.new_token(Kind::BreakKeyword, NodeFlags::None, range)?;
                writer.new_node_list(range, &[child])?;
            }
            let mut has_parse_error = false;
            finish_node(
                writer,
                child,
                TextRange::new(1, 2),
                NodeFlags::Ambient,
                &mut has_parse_error,
            )?;
            writer.set_parent(child, Some(root))?;
            visited.push(child);
            Ok(())
        })
        .unwrap();
    assert_eq!(visited, [first, second]);
    for child in visited {
        assert_eq!(writer.node(child).unwrap().range(), TextRange::new(1, 2));
        assert_eq!(writer.node(child).unwrap().parent(), Some(root));
    }
}
