//! Compile real clients to verify that owned AST views cannot outlive storage or
//! remain in use while a builder mutates. A successful client is the control.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Client {
    root: PathBuf,
    target: PathBuf,
}

impl Client {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("tsrs-storage-borrows-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        let ast = Path::new(env!("CARGO_MANIFEST_DIR"));
        let core = ast.join("../tsrs_core");
        let scanner = ast.join("../tsrs_scanner");
        let parser = ast.join("../tsrs_parser");
        let spanmap = ast.join("../tsrs_spanmap");
        let target = ast.join("../../target/ast-soa-borrow-checks");
        #[expect(
            clippy::unnecessary_debug_formatting,
            reason = "Debug formatting supplies quoting and escaping for the TOML path value."
        )]
        let manifest = format!(
            "[package]\nname = \"storage-borrow-client\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n\n[dependencies]\ntsrs_ast = {{ path = {ast:?} }}\ntsrs_core = {{ path = {core:?} }}\ntsrs_scanner = {{ path = {scanner:?} }}\ntsrs_parser = {{ path = {parser:?} }}\ntsrs_spanmap = {{ path = {spanmap:?} }}\n"
        );
        std::fs::write(root.join("Cargo.toml"), manifest).unwrap();
        Self { root, target }
    }

    fn compile(&self, body: &str) -> Output {
        let mut source = String::from(
            "use tsrs_ast::{Kind, NodeFlags};\nuse tsrs_ast::storage::{AstStoreBuilder, AstStoreLease, NodeView};\nuse tsrs_core::TextRange;\n",
        );
        writeln!(source, "{body}").unwrap();
        std::fs::write(self.root.join("src/lib.rs"), source).unwrap();
        Command::new(env!("CARGO"))
            .args(["check", "--offline", "--color", "never", "--manifest-path"])
            .arg(self.root.join("Cargo.toml"))
            // Separate from the invoking cargo's build lock, including in CI.
            .arg("--target-dir")
            .arg(&self.target)
            .output()
            .unwrap()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn storage_borrows_are_enforced_for_real_clients() {
    let client = Client::new();
    let control = client.compile(
        r#"

pub fn scoped_control() {
    use tsrs_ast::storage::{RollbackBuffers, TextInput};
    let mut ast = AstStoreBuilder::new("source").unwrap();
    let mut escaped = None;
    let key = ast.try_parse_node(RollbackBuffers::Reuse, |scope| {
        let child = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), TextInput::Owned("cooked"))?;
        let nested = scope.try_parse_list(RollbackBuffers::Reuse, |inner| {
            let child = inner.import_scoped_node(child)?;
            Ok(Some(inner.new_node_list(TextRange::synthetic(), &[child])?))
        })?.unwrap();
        assert_eq!(scope.list(nested)?.nodes().count(), 1);
        escaped = Some(scope.retain_list(nested));
        Ok(Some(child))
    }).unwrap().unwrap();
    assert_eq!(ast.node(key).unwrap().as_identifier().unwrap().text(), "cooked");
    assert_eq!(ast.list(escaped.unwrap()).unwrap().nodes().count(), 1);
    let retained = ast.try_parse_modifiers(RollbackBuffers::Reuse, |scope| {
        let key = scope.import_node(key);
        Ok(Some(scope.new_modifier_list(TextRange::synthetic(), &[key], tsrs_ast::ModifierFlags::None)?))
    }).unwrap().unwrap();
    assert_eq!(ast.modifier_list(retained).unwrap().list().len(), 1);
}

pub fn parser_scope_control(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| {
        let node = parser.parse_identifier();
        assert_eq!(parser.ast().node(node).unwrap().kind(), Kind::Identifier);
        parser.look_ahead(|inner| { inner.parse_identifier(); true });
        true
    });
    let node = parser.try_parse_node(|parser| {
        let nested = parser.try_parse_node(|inner| Some(inner.parse_identifier()))?;
        Some(nested)
    }).unwrap();
    assert_eq!(parser.ast().node(node).unwrap().kind(), Kind::Identifier);
    let mut retained = None;
    parser.look_ahead(|parser| {
        let node = parser.parse_identifier();
        retained = Some(parser.retain_node(node));
        true
    });
    let _ = parser.ast().node(retained.unwrap()).unwrap();
}

pub fn template_parser_scope_control(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| {
        let key = parser.parse_template_head(true);
        let node = parser.ast().node(key).unwrap().as_template_head().unwrap();
        let _ = (node.text(), node.raw_text());
        parser.parse_identifier();
        parser.parse_literal_of_template_span(true);
        true
    });
    let key = parser.try_parse_node(|parser| Some(parser.parse_template_head(true))).unwrap();
    let _ = parser.ast().node(key).unwrap().as_template_head().unwrap().raw_text();
}

pub fn control() {
    let ast = AstStoreBuilder::new("source").unwrap();
    let lease = AstStoreLease::from(ast.finish());
    let retained = lease.clone();
    drop(lease);
    drop(retained);
}

pub fn borrowed(lease: &AstStoreLease, key: tsrs_ast::storage::NodeKey) -> NodeView<'_> {
    let node = lease.node(key).unwrap();
    let _: Kind = node.kind();
    let _: NodeFlags = node.flags();
    node
}

pub fn borrowed_text(lease: &AstStoreLease, key: tsrs_ast::storage::NodeKey) -> &str {
    lease.node(key).unwrap().as_string_literal().unwrap().text()
}

pub fn borrowed_list(lease: &AstStoreLease, key: tsrs_ast::storage::ListKey) -> impl Iterator<Item = tsrs_ast::storage::NodeKey> + '_ {
    lease.list(key).unwrap().nodes()
}

pub fn borrowed_symbol<'a>(read: &'a tsrs_ast::storage::BindingRead<'_>, key: tsrs_ast::storage::SymbolKey) -> tsrs_ast::storage::SymbolView<'a> {
    read.data().symbol(key).unwrap()
}

pub fn snapshot_before_force(context: &mut tsrs_ast::storage::FileBindingContext, symbol: tsrs_ast::storage::SymbolKey, list: tsrs_ast::storage::ListKey, hooks: &mut impl tsrs_ast::storage::LazyListHooks) {
    let snapshot = context.read().unwrap().data().symbol(symbol).unwrap().declaration_snapshot();
    context.force_list(list, hooks).unwrap();
    let read = context.read().unwrap();
    let _ = read.data().read_declarations(snapshot).unwrap().count();
}

pub fn table_snapshot_before_force(context: &mut tsrs_ast::storage::FileBindingContext, table: tsrs_ast::storage::SymbolTableKey, list: tsrs_ast::storage::ListKey, hooks: &mut impl tsrs_ast::storage::LazyListHooks) {
    let entries = context.read().unwrap().data().symbol_table(table).unwrap().snapshot_entries();
    for entry in entries {
        context.force_list(list, hooks).unwrap();
        let read = context.read().unwrap();
        let _ = read.data().read_symbol_table_entry(entry).unwrap();
    }
}

pub fn mutate_with_saved_entry(data: &mut tsrs_ast::storage::BindingData, table: tsrs_ast::storage::SymbolTableKey, entry: tsrs_ast::storage::SymbolTableEntrySnapshot, replacement: tsrs_ast::storage::SymbolKey) {
    data.set_symbol_table_entry_from_snapshot(table, entry, replacement).unwrap();
    data.delete_symbol_table_entry_from_snapshot(table, entry).unwrap();
}

pub fn borrowed_annotation<'a>(read: &'a tsrs_ast::storage::BindingRead<'_>, node: NodeView<'_>) -> tsrs_ast::storage::IdentifierBindingsView<'a> {
    read.data().identifier(&node).unwrap()
}

pub fn borrowed_entries<'a>(read: &'a tsrs_ast::storage::BindingRead<'_>, table: tsrs_ast::storage::SymbolTableKey) -> impl Iterator<Item = (&'a str, tsrs_ast::storage::SymbolKey)> + 'a {
    read.data().symbol_table(table).unwrap().entries()
}

pub fn borrowed_raw_children(lease: &AstStoreLease, key: tsrs_ast::storage::NodeKey) -> impl Iterator<Item = tsrs_ast::storage::NodeKey> + '_ {
    lease.node(key).unwrap().as_syntax_list().unwrap().children().nodes()
}

pub fn borrowed_text_list(lease: &AstStoreLease, key: tsrs_ast::storage::NodeKey) -> impl Iterator<Item = &str> + '_ {
    lease.node(key).unwrap().as_jsdoc_text().unwrap().text().texts()
}

pub fn borrowed_deferred_list(lease: &tsrs_ast::storage::DeferredListLease) -> impl Iterator<Item = tsrs_ast::storage::NodeKey> + '_ {
    lease.nodes()
}

pub fn force_after_read(context: &mut tsrs_ast::storage::FileBindingContext, list: tsrs_ast::storage::ListKey, hooks: &mut impl tsrs_ast::storage::LazyListHooks) {
    let _ = context.read().unwrap().data().lazy_list_state(list);
    context.force_list(list, hooks).unwrap();
}

pub fn force_after_session_read(session: &mut tsrs_ast::storage::LazyBindSession<'_>, list: tsrs_ast::storage::ListKey, hooks: &mut impl tsrs_ast::storage::LazyListHooks) {
    let _ = session.list(list).unwrap().len();
    session.force_list(list, hooks).unwrap();
}

pub fn borrowed_scanner_value<'a>(scanner: &'a tsrs_scanner::OwnedScanner<'_>) -> &'a str {
    scanner.token_value_ref()
}

pub fn borrowed_scanner_input<'src>(scanner: &tsrs_scanner::OwnedScanner<'src>) -> &'src str {
    scanner.text()
}

pub fn retained_owned_snapshot<'src>(scanner: tsrs_scanner::OwnedScanner<'src>) -> tsrs_scanner::OwnedScannerState<'src> {
    scanner.mark()
}

pub fn moved_owned_value<'src>(mut scanner: tsrs_scanner::OwnedScanner<'src>) -> std::borrow::Cow<'src, str> {
    scanner.take_token_value()
}

pub fn borrowed_comments(source: &str) -> tsrs_scanner::CommentRangeIter<'_> {
    tsrs_scanner::get_leading_comment_ranges(source, 0)
}

pub fn owned_parse_handoff() -> tsrs_ast::storage::AstStoreBuilder {
    let source: std::sync::Arc<str> = std::sync::Arc::from("name.value");
    let mut parser = tsrs_parser::owned::OwnedParser::new(&source, tsrs_core::ScriptKind::TS).unwrap();
    let _ = parser.parse_entity_name(true, false, None);
    let (ast, _) = parser.into_parts();
    drop(source);
    ast
}

pub fn freeze_source_info(draft: tsrs_ast::storage::SourceFileInfoDraft) -> tsrs_ast::storage::SourceFileInfo {
    draft.freeze()
}

pub fn source_text(info: &tsrs_ast::storage::SourceFileInfo) -> &str {
    info.text()
}

pub fn source_maps(info: &tsrs_ast::storage::SourceFileInfo) -> (&[u32], &tsrs_ast::PositionMap) {
    (info.ecma_line_map(), info.get_position_map())
}

pub fn mutate_source_draft(draft: &mut tsrs_ast::storage::SourceFileInfoDraft) {
    let count = draft.info().directives().referenced_files.len();
    draft.parse_results_mut().identifier_count = count;
}

pub fn borrowed_mapped_source(file: &tsrs_ast::storage::MappedSourceFile) -> tsrs_ast::storage::MappedSourceFileView<'_> {
    file.view()
}

pub fn borrowed_mapped_spanmap(file: &tsrs_ast::storage::MappedSourceFile) -> Option<&tsrs_spanmap::SpanMap> {
    file.view().span_map()
}

pub fn retain_canonical(file: &tsrs_ast::storage::MappedSourceFile) -> tsrs_ast::storage::MappedSourceFile {
    file.view().canonical_source_file().unwrap().retain()
}
"#,
    );
    assert!(control.status.success(), "{}", String::from_utf8_lossy(&control.stderr));

    let cases = [
        (
            "E0521",
            r"pub fn escaped_parser_key(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    let mut escaped = None;
    parser.look_ahead(|parser| { escaped = Some(parser.parse_identifier()); true });
    let _ = escaped.unwrap();
}",
        ),
        (
            "E0521",
            r"pub fn escaped_parser_collection(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    let mut escaped = Vec::new();
    parser.look_ahead(|parser| { escaped.push(parser.parse_identifier()); true });
    let _ = escaped.len();
}",
        ),
        (
            "E0521",
            r"pub fn escaped_conditional_key(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    let mut escaped = None;
    parser.try_parse_node(|parser| { escaped = Some(parser.parse_identifier()); None });
    let _ = escaped.unwrap();
}",
        ),
        (
            "E0521",
            r"pub fn escaped_nested_parser_key(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| {
        let mut escaped = None;
        parser.look_ahead(|inner| { escaped = Some(inner.parse_identifier()); true });
        let _ = escaped.unwrap();
        true
    });
}",
        ),
        (
            "E0521",
            r"pub fn escaped_parser_text(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    let mut escaped = None;
    parser.look_ahead(|parser| {
        let key = parser.parse_identifier();
        escaped = Some(parser.ast().node(key).unwrap().as_identifier().unwrap().text());
        true
    });
    let _ = escaped.unwrap();
}",
        ),
        (
            "E0502",
            r"pub fn mutate_with_parser_view(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| {
        let key = parser.parse_identifier();
        let node = parser.ast().node(key).unwrap();
        parser.parse_identifier();
        let _ = node.kind();
        true
    });
}",
        ),
        (
            "E0616",
            r"pub fn access_private_parser_writer(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| { let _ = &mut parser.ast; true });
}",
        ),
        (
            "E0502",
            r"pub fn template_raw_text_cannot_cross_scanner_and_ast_mutation(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| {
        let key = parser.parse_template_head(true);
        let raw = parser.ast().node(key).unwrap().as_template_head().unwrap().raw_text();
        parser.parse_literal_of_template_span(true);
        let _ = raw.len();
        true
    });
}",
        ),
        (
            "E0521",
            r"pub fn template_raw_text_cannot_escape_speculative_owner(source: &std::sync::Arc<str>) {
    let mut escaped = None;
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| {
        let key = parser.parse_template_head(true);
        escaped = Some(parser.ast().node(key).unwrap().as_template_head().unwrap().raw_text());
        true
    });
    let _ = escaped.unwrap().len();
}",
        ),
        (
            "E0308",
            r"pub fn scoped_parser_key_is_not_stable(source: &std::sync::Arc<str>) {
    let mut parser = tsrs_parser::owned::OwnedParser::new(source, tsrs_core::ScriptKind::TS).unwrap();
    parser.look_ahead(|parser| {
        let _: tsrs_ast::storage::NodeKey = parser.parse_identifier();
        true
    });
}",
        ),
        (
            "E0521",
            r#"pub fn escaped_key() { let mut escaped = None; let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); escaped = Some(key); }); let _ = escaped.unwrap(); }"#,
        ),
        (
            "E0521",
            r#"pub fn escaped_keys() { let mut escaped = Vec::new(); let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); let node = scope.new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[key]).unwrap(); escaped.extend(scope.node(node).unwrap().as_syntax_list().unwrap().children().nodes()); }); let _ = escaped.len(); }"#,
        ),
        (
            "E0521",
            r#"pub fn nested_escape() { let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); let mut escaped = None; scope.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |inner| { escaped = Some(inner.import_scoped_node(key).unwrap()); }); let _ = escaped.unwrap(); }); }"#,
        ),
        (
            "lifetime may not live long enough",
            r#"pub fn conditional_escape() { let mut escaped = None; let mut ast = AstStoreBuilder::new("").unwrap(); ast.try_parse_node(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| { let key = scope.new_node(Kind::Identifier, NodeFlags::None, TextRange::synthetic())?; escaped = Some(key); Ok(Some(key)) }).unwrap(); let _ = escaped; }"#,
        ),
        (
            "E0502",
            r#"pub fn mutate_with_view() { let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); let view = scope.node(key).unwrap(); let text = view.as_identifier().unwrap().text(); scope.new_node(Kind::Unknown, NodeFlags::None, TextRange::synthetic()).unwrap(); let _ = text.len(); }); }"#,
        ),
        (
            "E0616",
            r#"pub fn private_writer() { let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); let _ = scope.writer.node_count(); }); }"#,
        ),
        (
            "E0624",
            r#"pub fn private_raw() { let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); let _ = key.raw(); }); }"#,
        ),
        (
            "E0616",
            r#"pub fn private_error() { let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); let error = scope.new_token(Kind::FunctionDeclaration, NodeFlags::None, TextRange::synthetic()).unwrap_err(); let _ = error.0; }); }"#,
        ),
        (
            "E0624",
            r"pub fn forge_key(key: tsrs_ast::storage::NodeKey) { let _ = tsrs_ast::storage::NodeKey::new(key.store(), key.index()); }",
        ),
        (
            "E0624",
            r"pub fn forge_index() { let _ = tsrs_ast::storage::NodeIndex::from_offset(0); }",
        ),
        (
            "E0308",
            r#"pub fn leak_plain() { let mut ast = AstStoreBuilder::new("").unwrap();
    ast.look_ahead(tsrs_ast::storage::RollbackBuffers::Reuse, |scope| {
        let key = scope.new_identifier(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked")).unwrap(); let _: tsrs_ast::storage::NodeKey = scope.node(key).unwrap().as_identifier().unwrap().key(); }); }"#,
        ),
        (
            "E0515",
            r"
pub fn escape_mapped_text(file: tsrs_ast::storage::MappedSourceFile) -> &'static str {
    file.view().original_text()
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_canonical_text(file: tsrs_ast::storage::MappedSourceFile) -> &'static str {
    file.view().canonical_source_file().unwrap().source_info().text()
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_mapped_spanmap(file: tsrs_ast::storage::MappedSourceFile) -> Option<&'static tsrs_spanmap::SpanMap> {
    file.view().span_map()
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_mapped_directive(file: tsrs_ast::storage::MappedSourceFile) -> &'static str {
    &file.view().diagnostic_directives()[0].source
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_mapped_outputs(file: tsrs_ast::storage::MappedSourceFile) -> impl Iterator<Item = tsrs_ast::storage::MappedSourceFileView<'static>> {
    file.view().supplemental_source_files()
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_source_text(info: std::sync::Arc<tsrs_ast::storage::SourceFileInfo>) -> &'static str {
    info.text()
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_source_map(info: std::sync::Arc<tsrs_ast::storage::SourceFileInfo>) -> &'static [u32] {
    info.ecma_line_map()
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_source_directive(info: std::sync::Arc<tsrs_ast::storage::SourceFileInfo>) -> &'static str {
    &info.directives().referenced_files[0].file_name
}
",
        ),
        (
            "E0502",
            r"
pub fn mutate_live_source_directives(draft: &mut tsrs_ast::storage::SourceFileInfoDraft) {
    let references = &draft.info().directives().referenced_files;
    draft.directives_mut().referenced_files.clear();
    let _ = references.len();
}
",
        ),
        (
            "E0505",
            r"
pub fn freeze_with_live_source_view(draft: tsrs_ast::storage::SourceFileInfoDraft) {
    let lines = draft.info().ecma_line_map();
    let _published = draft.freeze();
    let _ = lines.len();
}
",
        ),
        (
            "E0599",
            r"
pub fn mutate_frozen_source_info(info: &mut tsrs_ast::storage::SourceFileInfo) {
    info.parse_results_mut().identifier_count = 10;
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_snapshot_name(bindings: &tsrs_ast::storage::BindingLease, entry: tsrs_ast::storage::SymbolTableEntrySnapshot) -> &str {
    bindings.read().unwrap().data().read_symbol_table_entry(entry).unwrap().0
}
",
        ),
        (
            "E0502",
            r"
pub fn mutate_with_snapshot_name(data: &mut tsrs_ast::storage::BindingData, table: tsrs_ast::storage::SymbolTableKey, entry: tsrs_ast::storage::SymbolTableEntrySnapshot) {
    let (name, _) = data.read_symbol_table_entry(entry).unwrap();
    data.delete_symbol_table_entry(table, name).unwrap();
}
",
        ),
        (
            "E0515",
            r#"
pub fn escape_taken_scanner_value() -> std::borrow::Cow<'static, str> {
    let source = String::from("name");
    let mut scanner = tsrs_scanner::OwnedScanner::new();
    scanner.set_text(&source);
    scanner.scan();
    scanner.take_token_value()
}
"#,
        ),
        (
            "E0515",
            r"
pub fn escape_declaration_snapshot(bindings: &tsrs_ast::storage::BindingLease, snapshot: tsrs_ast::storage::DeclarationSnapshot) -> impl Iterator<Item = tsrs_ast::storage::NodeKey> + '_ {
    let read = bindings.read().unwrap();
    read.data().read_declarations(snapshot).unwrap()
}
",
        ),
        (
            "E0502",
            r"
pub fn mutate_snapshot_with_live_iterator(data: &mut tsrs_ast::storage::BindingData, symbol: tsrs_ast::storage::SymbolKey, snapshot: tsrs_ast::storage::DeclarationSnapshot) {
    let nodes = data.read_declarations(snapshot).unwrap();
    data.set_symbol_flags(symbol, tsrs_ast::SymbolFlags::None).unwrap();
    let _ = nodes.count();
}
",
        ),
        (
            "E0515",
            r#"
pub fn escape_parser() -> tsrs_parser::owned::OwnedParser<'static> {
    let source: std::sync::Arc<str> = std::sync::Arc::from("name.value");
    tsrs_parser::owned::OwnedParser::new(&source, tsrs_core::ScriptKind::TS).unwrap()
}
"#,
        ),
        (
            "E0505",
            r#"
pub fn drop_parser_input() {
    let source: std::sync::Arc<str> = std::sync::Arc::from("name.value");
    let mut parser = tsrs_parser::owned::OwnedParser::new(&source, tsrs_core::ScriptKind::TS).unwrap();
    drop(source);
    parser.parse_entity_name(true, false, None);
}
"#,
        ),
        (
            "E0502",
            r#"
pub fn parse_with_live_node() {
    let source: std::sync::Arc<str> = std::sync::Arc::from("name value");
    let mut parser = tsrs_parser::owned::OwnedParser::new(&source, tsrs_core::ScriptKind::TS).unwrap();
    let key = parser.parse_entity_name(true, false, None);
    let view = parser.ast().node(key).unwrap();
    parser.parse_entity_name(true, false, None);
    let _ = view.flags();
}
"#,
        ),
        (
            "E0505",
            r#"
pub fn drop_parser_with_live_node() {
    let source: std::sync::Arc<str> = std::sync::Arc::from("name");
    let mut parser = tsrs_parser::owned::OwnedParser::new(&source, tsrs_core::ScriptKind::TS).unwrap();
    let key = parser.parse_entity_name(true, false, None);
    let view = parser.ast().node(key).unwrap();
    drop(parser);
    let _ = view.flags();
}
"#,
        ),
        (
            "E0515",
            r#"
pub fn escape_comments() -> tsrs_scanner::CommentRangeIter<'static> {
    let source = String::from("/** doc */ name");
    tsrs_scanner::get_leading_comment_ranges(&source, 0)
}
"#,
        ),
        (
            "E0515",
            r#"
pub fn escape_scanner() -> tsrs_scanner::OwnedScanner<'static> {
    let source = String::from("name");
    let mut scanner = tsrs_scanner::OwnedScanner::new();
    scanner.set_text(&source);
    scanner
}
"#,
        ),
        (
            "E0505",
            r#"
pub fn drop_scanner_input() {
    let source = String::from("name");
    let mut scanner = tsrs_scanner::OwnedScanner::new();
    scanner.set_text(&source);
    drop(source);
    scanner.scan();
}
"#,
        ),
        (
            "E0515",
            r#"
pub fn escape_scanner_value() -> &'static str {
    let source = String::from("'cooked\\n'");
    let mut scanner = tsrs_scanner::OwnedScanner::new();
    scanner.set_text(&source);
    scanner.scan();
    scanner.token_value_ref()
}
"#,
        ),
        (
            "E0502",
            r#"
pub fn scan_with_live_value() {
    let source = String::from("'cooked\\n' next");
    let mut scanner = tsrs_scanner::OwnedScanner::new();
    scanner.set_text(&source);
    scanner.scan();
    let value = scanner.token_value_ref();
    scanner.scan();
    let _ = value.len();
}
"#,
        ),
        (
            "E0515",
            r#"
pub fn escape_scanner_snapshot() -> tsrs_scanner::OwnedScannerState<'static> {
    let source = String::from("name");
    let mut scanner = tsrs_scanner::OwnedScanner::new();
    scanner.set_text(&source);
    scanner.scan();
    scanner.mark()
}
"#,
        ),
        (
            "E0502",
            r"
pub fn force_with_live_binding_read(context: &mut tsrs_ast::storage::FileBindingContext, list: tsrs_ast::storage::ListKey, key: tsrs_ast::storage::SymbolKey, hooks: &mut impl tsrs_ast::storage::LazyListHooks) {
    let read = context.read().unwrap();
    let symbol = read.data().symbol(key).unwrap();
    context.force_list(list, hooks).unwrap();
    let _ = symbol.flags();
}
",
        ),
        (
            "E0505",
            r"
pub fn drop_deferred_lease(lease: tsrs_ast::storage::DeferredListLease) {
    let mut nodes = lease.nodes();
    drop(lease);
    let _ = nodes.next();
}
",
        ),
        (
            "E0597",
            r"
pub fn escape_deferred_lease(lease: tsrs_ast::storage::DeferredListLease) -> impl Iterator<Item = tsrs_ast::storage::NodeKey> {
    lease.nodes()
}
",
        ),
        (
            "E0502",
            r"
pub fn force_with_live_session_node(session: &mut tsrs_ast::storage::LazyBindSession<'_>, node: tsrs_ast::storage::NodeKey, list: tsrs_ast::storage::ListKey, hooks: &mut impl tsrs_ast::storage::LazyListHooks) {
    let view = session.node(node).unwrap();
    session.force_list(list, hooks).unwrap();
    let _ = view.kind();
}
",
        ),
        (
            "E0502",
            r"
pub fn force_with_live_session_list(session: &mut tsrs_ast::storage::LazyBindSession<'_>, list: tsrs_ast::storage::ListKey, hooks: &mut impl tsrs_ast::storage::LazyListHooks) {
    let mut nodes = session.list(list).unwrap().nodes();
    session.force_list(list, hooks).unwrap();
    let _ = nodes.next();
}
",
        ),
        (
            "E0502",
            r"
pub fn grow_with_live_session_view(session: &mut tsrs_ast::storage::LazyBindSession<'_>, node: tsrs_ast::storage::NodeKey) {
    let view = session.node(node).unwrap();
    session.parts_mut(node.store()).unwrap().0.new_token(Kind::SemicolonToken, NodeFlags::None, TextRange::synthetic()).unwrap();
    let _ = view.kind();
}
",
        ),
        (
            "E0502",
            r#"
pub fn mutate_with_raw_children() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let key = ast.new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[]).unwrap();
    let mut children = ast.node(key).unwrap().as_syntax_list().unwrap().children().nodes();
    ast.new_syntax_list(NodeFlags::None, TextRange::synthetic(), &[]).unwrap();
    let _ = children.next();
}
"#,
        ),
        (
            "E0515",
            r#"
pub fn escape_text_list() -> impl Iterator<Item = &'static str> {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let key = ast.new_jsdoc_text(NodeFlags::None, TextRange::synthetic(), &[tsrs_ast::storage::TextInput::Owned("text")]).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    lease.node(key).unwrap().as_jsdoc_text().unwrap().text().texts()
}
"#,
        ),
        (
            "E0515",
            r#"
pub fn escape() -> NodeView<'static> {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let key = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let store = ast.finish();
    store.node(key).unwrap()
}
"#,
        ),
        (
            "E0502",
            r#"
pub fn mutate() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let key = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let node = ast.node(key).unwrap();
    ast.set_flags(key, NodeFlags::None).unwrap();
    let _ = node.kind();
}
"#,
        ),
        (
            "E0505",
            r#"
pub fn drop_owner() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let key = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    let node = lease.node(key).unwrap();
    drop(lease);
    let _ = node.kind();
}
"#,
        ),
        (
            "E0515",
            r"
pub fn escape_guard(owner: &tsrs_ast::storage::FileAstOwner, key: tsrs_ast::storage::NodeKey) -> NodeView<'_> {
    let guard = owner.parsed_guard().unwrap();
    guard.ast().node(key).unwrap()
}
",
        ),
        (
            "E0502",
            r"
pub fn mutate_session(session: &mut tsrs_ast::storage::BindSession<'_>, key: tsrs_ast::storage::NodeKey) {
    let view = session.ast().node(key).unwrap();
    session.ast_mut().set_flags(key, NodeFlags::None).unwrap();
    let _ = view.kind();
}
",
        ),
        (
            "E0505",
            r#"
pub fn drop_list_owner() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let list = ast.new_node_list(TextRange::synthetic(), &[]).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    let view = lease.list(list).unwrap();
    drop(lease);
    let _ = view.len();
}
"#,
        ),
        (
            "E0502",
            r#"
pub fn mutate_with_list_iterator() {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let node = ast.new_node(Kind::Identifier, NodeFlags::None, TextRange::synthetic()).unwrap();
    let key = ast.new_node_list(TextRange::synthetic(), &[node]).unwrap();
    let list = ast.list(key).unwrap();
    let mut nodes = list.nodes();
    ast.new_node_list(TextRange::synthetic(), &[]).unwrap();
    let _ = nodes.next();
}
"#,
        ),
        (
            "E0515",
            r#"
pub fn escape_text() -> &'static str {
    let mut ast = AstStoreBuilder::new("").unwrap();
    let key = ast.new_string_literal(NodeFlags::None, TextRange::synthetic(), tsrs_ast::storage::TextInput::Owned("cooked"), tsrs_ast::TokenFlags::None).unwrap();
    let lease = AstStoreLease::from(ast.finish());
    lease.node(key).unwrap().as_string_literal().unwrap().text()
}
"#,
        ),
        (
            "E0515",
            r"
pub fn escape_binding(lease: &tsrs_ast::storage::BindingLease, key: tsrs_ast::storage::SymbolKey) -> tsrs_ast::storage::SymbolView<'_> {
    let read = lease.read().unwrap();
    read.data().symbol(key).unwrap()
}
",
        ),
        (
            "E0505",
            r"
pub fn drop_binding_guard(lease: &tsrs_ast::storage::BindingLease, key: tsrs_ast::storage::SymbolKey) {
    let read = lease.read().unwrap();
    let symbol = read.data().symbol(key).unwrap();
    drop(read);
    let _ = symbol.flags();
}
",
        ),
        (
            "E0502",
            r"
pub fn mutate_with_binding_view(write: &mut tsrs_ast::storage::BindingWrite<'_>, key: tsrs_ast::storage::SymbolKey) {
    let symbol = write.data().symbol(key).unwrap();
    write.data_mut().set_symbol_flags(key, tsrs_ast::SymbolFlags::None).unwrap();
    let _ = symbol.flags();
}
",
        ),
        (
            "E0515",
            r"
pub fn escape_annotation(lease: &tsrs_ast::storage::BindingLease, node: NodeView<'_>) -> tsrs_ast::storage::IdentifierBindingsView<'static> {
    let read = lease.read().unwrap();
    read.data().identifier(&node).unwrap()
}
",
        ),
        (
            "E0502",
            r#"
pub fn mutate_with_table_iterator(data: &mut tsrs_ast::storage::BindingData, table: tsrs_ast::storage::SymbolTableKey) {
    let mut entries = data.symbol_table(table).unwrap().entries();
    data.delete_symbol_table_entry(table, "x").unwrap();
    let _ = entries.next();
}
"#,
        ),
        (
            "E0502",
            r"
pub fn mutate_with_annotation(data: &mut tsrs_ast::storage::BindingData, syntax: &AstStoreLease, key: tsrs_ast::storage::NodeKey) {
    let view = data.identifier(&syntax.node(key).unwrap()).unwrap();
    data.set_identifier_flow_node(&syntax.node(key).unwrap(), None).unwrap();
    let _ = view.flow_node();
}
",
        ),
    ];
    for (expected, source) in cases {
        let output = client.compile(source);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "client compiled despite {expected}");
        assert!(errors.contains(expected), "expected {expected}: {errors}");
    }
}
