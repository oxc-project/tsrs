//! Isolated name grammar parity for the legacy and owned contexts. The pinned
//! Go oracle exposes the same entity-dump/entity-hash format.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::sync::Arc;

use tsrs_ast::storage::{AstStoreBuilder, NodeKey};
use tsrs_ast::{Kind, Node};
use tsrs_core::{P, ScriptKind};
use tsrs_parser::owned::OwnedParser;

use super::{escape, kind_name, read};

fn legacy_node(sb: &mut String, node: P<Node>, depth: usize, parent: Option<P<Node>>) {
    let _ = write!(
        sb,
        "N{depth} {} {} {} {:x} ",
        kind_name(node.kind()),
        node.pos(),
        node.end(),
        node.flags().bits()
    );
    escape(
        sb,
        if matches!(node.kind(), Kind::Identifier | Kind::PrivateIdentifier) {
            node.text()
        } else {
            ""
        },
    );
    if node.parent() != parent {
        sb.push_str(" P!");
    }
    sb.push('\n');
    node.for_each_child_static(&mut |child| {
        legacy_node(sb, child, depth + 1, Some(node));
        false
    });
}

fn owned_node(
    sb: &mut String,
    ast: &AstStoreBuilder,
    key: NodeKey,
    depth: usize,
    parent: Option<NodeKey>,
) {
    let node = ast.node(key).unwrap();
    let _ = write!(
        sb,
        "N{depth} {} {} {} {:x} ",
        kind_name(node.kind()),
        node.range().pos(),
        node.range().end(),
        node.flags().bits()
    );
    escape(
        sb,
        match node.kind() {
            Kind::Identifier => node.as_identifier().unwrap().text(),
            Kind::PrivateIdentifier => node.as_private_identifier().unwrap().text(),
            _ => "",
        },
    );
    if node.parent() != parent {
        sb.push_str(" P!");
    }
    sb.push('\n');
    if node.kind() == Kind::QualifiedName {
        let name = node.as_qualified_name().unwrap();
        owned_node(sb, ast, name.left(), depth + 1, Some(key));
        owned_node(sb, ast, name.right(), depth + 1, Some(key));
    }
}

pub fn dump(path: &str, owned: bool) -> Result<String, String> {
    let source = read(path).ok_or_else(|| format!("cannot read {path}"))?;
    let mut sb = String::new();
    if owned {
        let source: Arc<str> = source.into();
        let mut parser = OwnedParser::new(&source, ScriptKind::JS).unwrap();
        let root = parser.parse_entity_name(true, false, None);
        if parser.token() != Kind::EndOfFile || !parser.diagnostics().is_empty() {
            return Ok("INVALID\n".to_owned());
        }
        owned_node(&mut sb, parser.ast(), root, 0, None);
    } else if let Some(root) = tsrs_parser::parse_isolated_entity_name(&source) {
        legacy_node(&mut sb, root, 0, None);
    } else {
        sb.push_str("INVALID\n");
    }
    Ok(sb)
}
