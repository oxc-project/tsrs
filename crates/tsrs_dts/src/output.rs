use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use rustc_hash::FxHashSet;

use tsrs_ast::{self as ast, Kind, Node, NodeVisitor, NodeVisitorHooks, TokenFlags};
use tsrs_core::{LanguageVariant, ModuleKind, NewLineKind, P, alloc_str, tspath};
use tsrs_printer::{EmitContext, PrintHandlers, Printer, PrinterOptions};

use crate::{
    Error, Files, Result,
    graph::{Graph, Live},
};

/// Shared bundle of a multi-entry build; entry facades re-export its bindings.
pub const SHARED_BUNDLE: &str = "__shared.d.cts";
const SHARED_SPECIFIER: &str = "./__shared.cjs";

fn identifier(context: P<EmitContext>, text: &str) -> P<Node> {
    context.factory.new_identifier(alloc_str(text))
}

fn string_literal(context: P<EmitContext>, text: &str) -> P<Node> {
    context.factory.new_string_literal(alloc_str(text), TokenFlags::None)
}

fn side_effect_import(context: P<EmitContext>, from: &str) -> P<Node> {
    context.factory.new_import_declaration(None, None, string_literal(context, from), None)
}

/// `declare <keyword> <name> { statements }`
fn declare_module(
    context: P<EmitContext>,
    keyword: Kind,
    name: P<Node>,
    statements: Vec<P<Node>>,
) -> P<Node> {
    let f = &context.factory;
    f.new_module_declaration(
        Some(f.new_modifier_list(vec![f.new_modifier(Kind::DeclareKeyword)])),
        keyword,
        name,
        None,
        Some(f.new_module_block(f.new_node_list(statements))),
    )
}

fn is_identifier_name(text: &str) -> bool {
    !text.is_empty() && tsrs_scanner::is_identifier_text(text, LanguageVariant::Standard)
}

fn module_export_name(context: P<EmitContext>, text: &str) -> P<Node> {
    if is_identifier_name(text) { identifier(context, text) } else { string_literal(context, text) }
}

fn export_statement(
    context: P<EmitContext>,
    local: P<Node>,
    public: &str,
    type_only: bool,
    from: Option<&str>,
) -> P<Node> {
    let f = &context.factory;
    let specifier =
        f.new_export_specifier(type_only, Some(local), module_export_name(context, public));
    f.new_export_declaration(
        None,
        false,
        Some(f.new_named_exports(f.new_node_list(vec![specifier]))),
        from.map(|from| string_literal(context, from)),
        None,
    )
}

fn resolution_attributes(context: P<EmitContext>, mode: ModuleKind) -> Option<P<Node>> {
    (mode != ModuleKind::NONE).then(|| {
        let f = &context.factory;
        f.new_import_attributes(
            Kind::WithKeyword,
            f.new_node_list(vec![f.new_import_attribute(
                Some(f.new_string_literal("resolution-mode", TokenFlags::None)),
                f.new_string_literal(
                    if mode == ModuleKind::ES_NEXT { "import" } else { "require" },
                    TokenFlags::None,
                ),
            )]),
            false,
        )
    })
}

fn companion_extension(mode: ModuleKind, file_name: &str) -> Option<&'static str> {
    let extension = match mode {
        ModuleKind::ES_NEXT => "mts",
        ModuleKind::COMMON_JS => "cts",
        _ => return None,
    };
    (!file_name.ends_with(&format!(".d.{extension}"))).then_some(extension)
}

fn emit_companion(
    printer: &mut crate::maps::OutputPrinter<'_>,
    files: &mut Files,
    companions: &mut usize,
    file_name: &str,
    extension: &str,
    node: P<Node>,
    source: P<ast::SourceFile>,
) -> String {
    // Count companions separately: `files` also receives their maps.
    let name = format!("{file_name}.external{companions}.d.{extension}");
    *companions += 1;
    let runtime_extension = tsrs_modulespecifiers::get_js_extension_for_declaration_file_extension(
        &format!(".d.{extension}"),
    );
    let specifier = format!("./{}", tspath::change_extension(&name, &runtime_extension));
    let mut code = printer.buffer();
    printer.append(&mut code, node, source);
    let code = printer.finish(code, &name, files, 0);
    files.insert(name, code);
    specifier
}

fn import_statement(
    context: P<EmitContext>,
    imported: &str,
    local: P<Node>,
    from: &str,
    type_only: bool,
    namespace: bool,
    require: bool,
    mode: ModuleKind,
) -> P<Node> {
    let f = &context.factory;
    if require {
        return f.new_import_equals_declaration(
            None,
            type_only,
            local,
            f.new_external_module_reference(string_literal(context, from)),
        );
    }
    let bindings = if namespace {
        f.new_namespace_import(local)
    } else {
        let specifier =
            f.new_import_specifier(false, Some(module_export_name(context, imported)), local);
        f.new_named_imports(f.new_node_list(vec![specifier]))
    };
    let clause = f.new_import_clause(
        if type_only { Kind::TypeKeyword } else { Kind::Unknown },
        None,
        Some(bindings),
    );
    f.new_import_declaration(
        None,
        Some(clause),
        string_literal(context, from),
        if type_only { resolution_attributes(context, mode) } else { None },
    )
}

fn name_anonymous(context: P<EmitContext>, node: P<Node>, name: P<Node>) -> P<Node> {
    let f = &context.factory;
    match node.kind() {
        Kind::ClassDeclaration => f.update_class_declaration(
            node,
            node.modifiers(),
            Some(name),
            node.type_parameter_list(),
            node.as_class_declaration().heritage_clauses(),
            node.member_list().unwrap(),
        ),
        Kind::FunctionDeclaration => {
            let data = node.as_function_declaration();
            f.update_function_declaration(
                node,
                node.modifiers(),
                data.asterisk_token(),
                Some(name),
                node.type_parameter_list(),
                node.parameter_list(),
                node.type_node(),
                data.full_signature(),
                node.body(),
            )
        }
        _ => node,
    }
}

fn unexport(context: P<EmitContext>, node: P<Node>) -> P<Node> {
    let f = context.factory.as_node_factory();
    let mut modifiers: Vec<_> = node
        .modifier_nodes()
        .iter()
        .copied()
        .filter(|m| ![Kind::ExportKeyword, Kind::DefaultKeyword].contains(&m.kind()))
        .collect();
    if [
        Kind::VariableStatement,
        Kind::ClassDeclaration,
        Kind::FunctionDeclaration,
        Kind::EnumDeclaration,
        Kind::ModuleDeclaration,
    ]
    .contains(&node.kind())
        && !modifiers.iter().any(|m| m.kind() == Kind::DeclareKeyword)
    {
        modifiers.insert(0, f.new_modifier(Kind::DeclareKeyword));
    }
    ast::replace_modifiers(f, node, (!modifiers.is_empty()).then(|| f.new_modifier_list(modifiers)))
}

fn printer(context: P<EmitContext>, reserved: FxHashSet<String>) -> Printer {
    tsrs_printer::new_printer(
        PrinterOptions { new_line: NewLineKind::LF, source_map: true, ..PrinterOptions::default() },
        PrintHandlers {
            has_global_name: Some(Rc::new(move |name: &str| reserved.contains(name))),
            ..PrintHandlers::default()
        },
        Some(context),
    )
}

/// How public entries map to output files.
pub enum Layout<'a> {
    /// One bundle, named by the caller, that exports `Graph::entry`.
    Bundle(&'a str),
    /// `SHARED_BUNDLE` plus one facade per entry (output name to module).
    /// `source` attributes shared-bundle statements to a module.
    Facades { entries: &'a BTreeMap<String, usize>, source: usize },
}

pub fn generate(
    graph: &mut Graph,
    live: &Live,
    layout: &Layout<'_>,
    directory: &str,
    captured: &crate::CapturedDeclarations,
    maps: bool,
) -> Result<Files> {
    // Transformed nodes and printer side tables belong to this output-only arena.
    // All returned artifacts are owned strings; no node escapes this scope.
    let region = tsrs_core::arena::Region::new_scratch(1 << 20);
    let _scope = region.enter_scratch();
    let (file_name, entries, source) = match *layout {
        Layout::Bundle(file_name) => (file_name, None, graph.entry),
        Layout::Facades { entries, source } => (SHARED_BUNDLE, Some(entries), source),
    };
    let context = tsrs_printer::new_scratch_emit_context();
    let source = graph.modules[source].file;
    let mut reserved = std::mem::take(&mut graph.reserved_names);
    reserved.extend(graph.imports.values().map(|i| i.local.clone()));
    let mut printer = crate::maps::OutputPrinter::new(
        printer(context, reserved),
        &captured.declaration_maps,
        maps,
        directory.to_string(),
    );
    let mut names = BTreeMap::new();
    let global_symbols: BTreeSet<_> = graph
        .statements
        .iter()
        .filter(|statement| graph.modules[statement.module].script)
        .flat_map(|statement| statement.bindings.iter().copied())
        .collect();
    for &symbol in &live.symbols {
        if let Some(name) = graph.binding_names.get(&symbol) {
            names.insert(
                symbol,
                if global_symbols.contains(&symbol) {
                    identifier(context, name)
                } else {
                    context.factory.new_unique_name(name)
                },
            );
        }
    }
    // Module namespaces expose original export names backed by generated locals.
    let mut namespace_nodes = Vec::new();
    for (&symbol, &module) in &graph.module_symbols {
        let Some(&name) = names.get(&symbol) else {
            continue;
        };
        let mut exports = Vec::new();
        for export in &graph.modules[module].exports {
            let local = names.get(&export.symbol).ok_or_else(|| {
                Error(format!("missing namespace export binding for {:?}", export.name))
            })?;
            exports.push(export_statement(context, *local, &export.name, export.type_only, None));
        }
        if !graph.modules[module].has_values {
            let f = &context.factory;
            // Give even a type-only module a value identity without merging a
            // variable with a namespace. Explicit exports keep this marker private.
            let declaration = f.new_variable_declaration(
                f.new_unique_name("moduleValue"),
                None,
                Some(f.new_type_literal_node(f.new_node_list(vec![]))),
                None,
            );
            exports.insert(
                0,
                f.new_variable_statement(
                    None,
                    f.new_variable_declaration_list(
                        f.new_node_list(vec![declaration]),
                        ast::NodeFlags::Const,
                    ),
                ),
            );
            if graph.modules[module].exports.is_empty() {
                exports.push(tsrs_declarations::create_empty_exports(
                    context.factory.as_node_factory(),
                ));
            }
        }
        namespace_nodes.push(declare_module(context, Kind::NamespaceKeyword, name, exports));
    }
    let names = Rc::new(names);
    // These lookup tables are only used by the visitor from this point onward.
    let references = std::mem::take(&mut graph.references);
    let renamed_names = Rc::clone(&names);
    let inline_imports = std::mem::take(&mut graph.inline_imports);
    let external_specifiers = std::mem::take(&mut graph.external_specifiers);
    let external_import_modes = std::mem::take(&mut graph.external_import_modes);
    let namespace_blocks: BTreeSet<_> = graph
        .namespace_children
        .keys()
        .filter_map(|&index| graph.statements[index].node.body())
        .collect();
    let live_nodes: BTreeSet<_> =
        live.statements.iter().map(|&index| graph.statements[index].node).collect();
    let mut visitor = ast::new_node_visitor(
        Some(Rc::new(move |visitor: &mut NodeVisitor, node: P<Node>| {
            if namespace_blocks.contains(&node) {
                let mut statements = Vec::new();
                for &child in node.statements() {
                    if child.kind() == Kind::VariableStatement {
                        let list = child.as_variable_statement().declaration_list();
                        let declarations: Vec<_> = list
                            .as_variable_declaration_list()
                            .declarations()
                            .nodes()
                            .iter()
                            .filter(|node| live_nodes.contains(node))
                            .map(|&node| visitor.visit_node(Some(node)).unwrap())
                            .collect();
                        if !declarations.is_empty() {
                            statements.push(context.factory.new_variable_statement(
                                child.modifiers(),
                                context.factory.new_variable_declaration_list(
                                    context.factory.new_node_list(declarations),
                                    list.flags(),
                                ),
                            ));
                        }
                    } else if (live_nodes.contains(&child)
                        || matches!(
                            child.kind(),
                            Kind::ExportDeclaration | Kind::ImportEqualsDeclaration
                        ))
                        && let Some(child) = visitor.visit_node(Some(child))
                    {
                        statements.push(child);
                    }
                }
                return Some(
                    context
                        .factory
                        .update_module_block(node, context.factory.new_node_list(statements)),
                );
            }
            if let Some(specifier) = external_specifiers.get(&node) {
                return Some(
                    context.factory.new_string_literal(alloc_str(specifier), TokenFlags::None),
                );
            }
            if let Some(import) = inline_imports.get(&node) {
                let data = node.as_import_type_node();
                let name = renamed_names[&import.symbol];
                let qualifier = if let Some(root) = import.root {
                    replace_qualifier_root(context, data.qualifier().unwrap(), root, name)
                } else {
                    name
                };
                let arguments = visitor.visit_nodes(data.type_arguments());
                return Some(if data.is_type_of() {
                    context.factory.new_type_query_node(qualifier, arguments)
                } else {
                    context.factory.new_type_reference_node(qualifier, arguments)
                });
            }
            if let Some(&mode) = external_import_modes.get(&node) {
                let data = node.as_import_type_node();
                return Some(context.factory.update_import_type_node(
                    node,
                    data.is_type_of(),
                    visitor.visit_node(Some(data.argument())).unwrap(),
                    data.attributes().or_else(|| resolution_attributes(context, mode)),
                    visitor.visit_node(data.qualifier()),
                    visitor.visit_nodes(data.type_arguments()),
                ));
            }
            // Member names and public export names belong to their namespace;
            // only their qualifier or local binding participates in renaming.
            if node.kind() == Kind::BindingElement {
                let data = node.as_binding_element();
                let name = data.name();
                let property = data.property_name().or_else(|| {
                    (node.parent().is_some_and(|p| p.kind() == Kind::ObjectBindingPattern)
                        && data.dot_dot_dot_token().is_none()
                        && name.is_some_and(|n| n.kind() == Kind::Identifier))
                    .then_some(name)
                    .flatten()
                });
                return Some(context.factory.update_binding_element(
                    node,
                    data.dot_dot_dot_token(),
                    property,
                    visitor.visit_node(name),
                    visitor.visit_node(data.initializer()),
                ));
            }
            if node.kind() == Kind::QualifiedName {
                let data = node.as_qualified_name();
                return Some(context.factory.update_qualified_name(
                    node,
                    visitor.visit_node(Some(data.left())).unwrap(),
                    data.right(),
                ));
            }
            if node.kind() == Kind::PropertyAccessExpression {
                let data = node.as_property_access_expression();
                return Some(context.factory.update_property_access_expression(
                    node,
                    visitor.visit_node(Some(data.expression())).unwrap(),
                    data.question_dot_token(),
                    data.name(),
                    node.flags(),
                ));
            }
            if node.kind() == Kind::ExportSpecifier {
                let data = node.as_export_specifier();
                let local = visitor.visit_node(Some(data.property_name().unwrap_or(data.name())));
                return Some(context.factory.update_export_specifier(
                    node,
                    data.is_type_only(),
                    local,
                    data.name(),
                ));
            }
            if node.kind() == Kind::Identifier
                && let Some(name) =
                    references.get(&node).and_then(|symbol| renamed_names.get(symbol))
            {
                let replacement = name.clone_node(context.factory.as_node_factory());
                context.assign_source_map_range(replacement, node);
                context.set_original_ex(replacement, node, true);
                return Some(replacement);
            }
            visitor.visit_each_child(Some(node))
        })),
        Some(context.factory.as_node_factory().clone()),
        NodeVisitorHooks::default(),
    );
    // Declaration inputs can require ambient packages without an import. Retain
    // parsed directives from live modules and let tsrs print their attributes.
    let directives = context
        .factory
        .new_source_file(
            source.parse_options().clone(),
            "",
            context.factory.new_node_list(vec![]),
            context.factory.new_token(Kind::EndOfFile),
        )
        .as_source_file_p();
    directives.is_declaration_file.set(true);
    for (types, target) in [
        (true, &directives.type_reference_directives),
        (false, &directives.lib_reference_directives),
    ] {
        let mut references = BTreeMap::new();
        for &module in &live.modules {
            let file = graph.modules[module].file;
            for &reference in if types {
                file.type_reference_directives()
            } else {
                file.lib_reference_directives()
            } {
                let key = (reference.get().file_name.as_str(), reference.resolution_mode);
                let previous = references.entry(key).or_insert(reference);
                if reference.preserve {
                    *previous = reference;
                }
            }
        }
        let references: Vec<_> = references.into_values().collect();
        target.set(tsrs_core::alloc_slice(&references));
    }
    let mut code = printer.buffer();
    if !directives.type_reference_directives().is_empty()
        || !directives.lib_reference_directives().is_empty()
    {
        printer.append(&mut code, directives.as_node(), directives);
    }
    let ambient_name = format!("{file_name}.ambient.d.ts");
    let mut ambient_code = printer.buffer();
    let mut external_files = BTreeMap::new();
    let mut companions = 0;
    let mut side_effects = BTreeSet::new();
    for &module in &live.modules {
        for (specifier, mode) in &graph.modules[module].side_effects {
            if side_effects.insert((specifier, *mode)) {
                let companion = companion_extension(*mode, file_name).map(|extension| {
                    let node = side_effect_import(context, specifier);
                    emit_companion(
                        &mut printer,
                        &mut external_files,
                        &mut companions,
                        file_name,
                        extension,
                        node,
                        graph.modules[module].file,
                    )
                });
                let node = side_effect_import(context, companion.as_deref().unwrap_or(specifier));
                printer.append(&mut code, node, graph.modules[module].file);
            }
        }
    }
    for binding in graph.imports.values().filter(|i| i.external && live.symbols.contains(&i.symbol))
    {
        let name = names
            .get(&binding.symbol)
            .ok_or_else(|| Error("missing external import name".into()))?;
        // Value imports cannot carry resolution-mode attributes. A companion
        // with the source format preserves package conditions after relocation.
        let companion = companion_extension(binding.mode, file_name)
            .filter(|_| !binding.type_only && !binding.require);
        let from = companion.map(|extension| {
            let node = if binding.namespace {
                context.factory.new_export_declaration(
                    None,
                    false,
                    Some(context.factory.new_namespace_export(identifier(context, "binding"))),
                    Some(string_literal(context, &binding.specifier)),
                    None,
                )
            } else {
                export_statement(
                    context,
                    module_export_name(context, &binding.imported),
                    "binding",
                    false,
                    Some(&binding.specifier),
                )
            };
            emit_companion(
                &mut printer,
                &mut external_files,
                &mut companions,
                file_name,
                extension,
                node,
                graph.modules[binding.module].file,
            )
        });
        let node = import_statement(
            context,
            if companion.is_some() { "binding" } else { &binding.imported },
            *name,
            from.as_deref().unwrap_or(&binding.specifier),
            binding.type_only,
            companion.is_none() && binding.namespace,
            binding.require,
            binding.mode,
        );
        printer.append(&mut code, node, graph.modules[binding.module].file);
    }
    for &index in &live.statements {
        let statement = &graph.statements[index];
        if statement.namespace_parent.is_some() {
            continue;
        }
        if graph.modules[statement.module].script && ast::is_ambient_module(statement.node) {
            // Keep creation of ambient modules in a script. Modules whose bodies
            // reference bundled declarations are extended from the bundle scope.
            let file = graph.modules[statement.module].file;
            let transformed = visitor.visit_node(Some(statement.node)).unwrap();
            if transformed == statement.node {
                printer.append(&mut ambient_code, statement.node, file);
            } else {
                let data = statement.node.as_module_declaration();
                let empty = context.factory.update_module_declaration(
                    statement.node,
                    data.modifiers(),
                    data.keyword(),
                    data.name(),
                    data.attributes(),
                    Some(context.factory.new_module_block(context.factory.new_node_list(vec![]))),
                );
                printer.append(&mut ambient_code, empty, file);
                printer.append(&mut code, transformed, file);
            }
            continue;
        }
        let renamed = visitor
            .visit_node(Some(statement.node))
            .ok_or_else(|| Error("declaration transform removed a live statement".into()))?;
        let renamed = if let Some(symbol) = graph.anonymous.get(&statement.node) {
            name_anonymous(context, renamed, names[symbol])
        } else if statement.node.kind() == Kind::VariableDeclaration {
            let list = statement.node.parent().unwrap();
            let owner = list.parent().unwrap();
            context.factory.new_variable_statement(
                owner.modifiers(),
                context.factory.new_variable_declaration_list(
                    context.factory.new_node_list(vec![renamed]),
                    list.flags(),
                ),
            )
        } else {
            renamed
        };
        let renamed = if graph.modules[statement.module].script && !ast::is_ambient_module(renamed)
        {
            let modifiers: Vec<_> = renamed
                .modifier_nodes()
                .iter()
                .copied()
                .filter(|modifier| modifier.kind() != Kind::DeclareKeyword)
                .collect();
            let renamed = ast::replace_modifiers(
                context.factory.as_node_factory(),
                renamed,
                (!modifiers.is_empty()).then(|| context.factory.new_modifier_list(modifiers)),
            );
            declare_module(
                context,
                Kind::GlobalKeyword,
                identifier(context, "global"),
                vec![renamed],
            )
        } else {
            unexport(context, renamed)
        };
        printer.append(&mut code, renamed, graph.modules[statement.module].file);
    }
    for node in namespace_nodes {
        printer.append(&mut code, node, source);
    }
    if entries.is_none() {
        for &node in &graph.modules[graph.entry].global_exports {
            printer.append(&mut code, node, source);
        }
        if let Some(node) = graph.modules[graph.entry].assignment_node {
            let expression = node.as_export_assignment().expression();
            let root = ast::get_first_identifier(expression);
            let symbol = graph.modules[graph.entry].assignment.unwrap();
            let expression = replace_qualifier_root(context, expression, root, names[&symbol]);
            let node = context.factory.new_export_assignment(None, true, None, expression);
            printer.append(&mut code, node, source);
        }
        // Graph::analyze clears the exports of a module with `export =`.
        for export in &graph.modules[graph.entry].exports {
            let name = names
                .get(&export.symbol)
                .ok_or_else(|| Error(format!("missing output binding for {:?}", export.name)))?;
            printer.append(
                &mut code,
                export_statement(context, *name, &export.name, export.type_only, None),
                source,
            );
        }
        if graph.modules[graph.entry].exports.is_empty()
            && graph.modules[graph.entry].assignment.is_none()
        {
            let node = tsrs_declarations::create_empty_exports(context.factory.as_node_factory());
            printer.append(&mut code, node, source);
        }
    }
    let mut files = external_files;
    if let Some(entries) = entries {
        let mut shared_exports = BTreeMap::new();
        let mut ordered: Vec<_> = names.iter().collect();
        ordered.sort_by_cached_key(|(symbol, _)| {
            (
                graph.binding_names.get(symbol).map_or("", String::as_str),
                graph.owners.get(symbol).and_then(|owners| owners.first()).copied(),
                graph.imports.get(symbol).map(|import| {
                    (import.module, import.local.as_str(), import.specifier.as_str())
                }),
                graph.module_symbols.get(symbol).copied(),
            )
        });
        for (&symbol, &name) in ordered {
            let public = format!("binding{}", shared_exports.len());
            printer.append(
                &mut code,
                export_statement(context, name, &public, false, None),
                source,
            );
            shared_exports.insert(symbol, public);
        }
        if shared_exports.is_empty() {
            let node = tsrs_declarations::create_empty_exports(context.factory.as_node_factory());
            printer.append(&mut code, node, source);
        }
        for (file, &module) in entries {
            let mut wrapper = printer.buffer();
            for &node in &graph.modules[module].global_exports {
                printer.append(&mut wrapper, node, graph.modules[module].file);
            }
            if let Some(assignment) = graph.modules[module].assignment_node {
                let symbol = graph.modules[module].assignment.unwrap();
                let local = identifier(context, "entry");
                let binding = &shared_exports[&symbol];
                let node = import_statement(
                    context,
                    binding,
                    local,
                    SHARED_SPECIFIER,
                    false,
                    false,
                    false,
                    ModuleKind::NONE,
                );
                printer.append(&mut wrapper, node, graph.modules[module].file);
                let expression = assignment.as_export_assignment().expression();
                let root = ast::get_first_identifier(expression);
                let expression = replace_qualifier_root(context, expression, root, local);
                printer.append(
                    &mut wrapper,
                    context.factory.new_export_assignment(None, true, None, expression),
                    graph.modules[module].file,
                );
            } else {
                for export in &graph.modules[module].exports {
                    let binding = &shared_exports[&export.symbol];
                    let node = export_statement(
                        context,
                        identifier(context, binding),
                        &export.name,
                        export.type_only,
                        Some(SHARED_SPECIFIER),
                    );
                    printer.append(&mut wrapper, node, graph.modules[module].file);
                }
                if graph.modules[module].exports.is_empty() {
                    printer.append(
                        &mut wrapper,
                        side_effect_import(context, SHARED_SPECIFIER),
                        source,
                    );
                    printer.append(
                        &mut wrapper,
                        tsrs_declarations::create_empty_exports(context.factory.as_node_factory()),
                        source,
                    );
                }
            }
            let wrapper = printer.finish(wrapper, file, &mut files, 0);
            files.insert(file.clone(), wrapper);
        }
    }
    let prefix_lines = u32::from(!ambient_code.text.is_empty());
    if !ambient_code.text.is_empty() {
        code.text.insert_str(0, &format!("/// <reference path=\"./{ambient_name}\" />\n"));
        let ambient_code = printer.finish(ambient_code, &ambient_name, &mut files, 0);
        files.insert(ambient_name, ambient_code);
    }
    let code = printer.finish(code, file_name, &mut files, prefix_lines);
    files.insert(file_name.to_string(), code);
    Ok(files)
}

fn replace_qualifier_root(
    context: P<EmitContext>,
    qualifier: P<Node>,
    root: P<Node>,
    replacement: P<Node>,
) -> P<Node> {
    if qualifier == root {
        replacement
    } else if qualifier.kind() == Kind::PropertyAccessExpression {
        let data = qualifier.as_property_access_expression();
        context.factory.update_property_access_expression(
            qualifier,
            replace_qualifier_root(context, data.expression(), root, replacement),
            data.question_dot_token(),
            data.name(),
            qualifier.flags(),
        )
    } else {
        let name = qualifier.as_qualified_name();
        context.factory.new_qualified_name(
            replace_qualifier_root(context, name.left(), root, replacement),
            name.right(),
        )
    }
}
