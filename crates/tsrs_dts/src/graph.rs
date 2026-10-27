use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tsrs_ast::{self as ast, Kind, Node, SourceFile, Symbol, SymbolFlags, SymbolId};
use tsrs_checker::Checker;
use tsrs_compiler::{Context, Program};
use tsrs_core::{ModuleKind, P, tspath};

use crate::{Error, Files, Result};

#[derive(Clone)]
pub struct PublicExport {
    pub name: String,
    pub symbol: SymbolId,
    pub type_only: bool,
    pub meaning: ast::SemanticMeaning,
}

pub struct Module {
    pub file: P<SourceFile>,
    pub exports: Vec<PublicExport>,
    pub has_values: bool,
    pub assignment: Option<SymbolId>,
    pub assignment_node: Option<P<Node>>,
    pub global_exports: Vec<P<Node>>,
    pub dependencies: BTreeSet<usize>,
    pub effects: Vec<usize>,
    pub side_effects: Vec<(String, ModuleKind)>,
    /// First reason this module cannot be linked; reported once it is live.
    pub error: Option<String>,
    pub script: bool,
}

impl Module {
    fn new(file: P<SourceFile>) -> Self {
        Self {
            file,
            exports: Vec::new(),
            has_values: false,
            assignment: None,
            assignment_node: None,
            global_exports: Vec::new(),
            dependencies: BTreeSet::new(),
            effects: Vec::new(),
            side_effects: Vec::new(),
            error: None,
            script: !ast::is_external_module(file),
        }
    }
}

pub struct Statement {
    pub module: usize,
    pub node: P<Node>,
    pub bindings: BTreeSet<SymbolId>,
    pub augmentation: bool,
    pub namespace_parent: Option<usize>,
    pub dependency_meanings: BTreeMap<SymbolId, ast::SemanticMeaning>,
    /// First unsupported import in this statement; reported once it is live.
    pub unsupported_import: Option<String>,
}

impl Statement {
    fn new(
        module: usize,
        node: P<Node>,
        bindings: BTreeSet<SymbolId>,
        augmentation: bool,
        namespace_parent: Option<usize>,
    ) -> Self {
        Self {
            module,
            node,
            bindings,
            augmentation,
            namespace_parent,
            dependency_meanings: BTreeMap::new(),
            unsupported_import: None,
        }
    }
}

#[derive(Clone)]
pub struct InlineImport {
    pub symbol: SymbolId,
    pub root: Option<P<Node>>,
}

struct ExternalOrigin {
    specifier: String,
    imported: String,
    namespace: bool,
    mode: ModuleKind,
}

pub struct ImportBinding {
    pub module: usize,
    pub symbol: SymbolId,
    pub target: SymbolId,
    pub local: String,
    pub imported: String,
    pub specifier: String,
    pub type_only: bool,
    pub external: bool,
    pub namespace: bool,
    pub require: bool,
    pub mode: ModuleKind,
    pub import_module: Option<usize>,
}

#[derive(Default)]
pub struct Live {
    pub statements: BTreeSet<usize>,
    pub symbols: BTreeSet<SymbolId>,
    pub modules: BTreeSet<usize>,
}

impl Live {
    pub fn extend(&mut self, other: Self) {
        self.statements.extend(other.statements);
        self.symbols.extend(other.symbols);
        self.modules.extend(other.modules);
    }
}

pub struct Graph {
    pub modules: Vec<Module>,
    pub statements: Vec<Statement>,
    pub imports: BTreeMap<SymbolId, ImportBinding>,
    pub inline_imports: FxHashMap<P<Node>, InlineImport>,
    pub external_specifiers: FxHashMap<P<Node>, String>,
    pub external_import_modes: FxHashMap<P<Node>, ModuleKind>,
    pub anonymous: BTreeMap<P<Node>, SymbolId>,
    pub namespace_children: BTreeMap<usize, Vec<usize>>,
    pub module_symbols: BTreeMap<SymbolId, usize>,
    pub owners: FxHashMap<SymbolId, Vec<usize>>,
    pub binding_names: FxHashMap<SymbolId, String>,
    /// Original AST identifier -> canonical symbol. Only original nodes are
    /// queried; newly synthesized nodes never go through the original checker.
    pub references: FxHashMap<P<Node>, SymbolId>,
    /// Compiler source names and linker names reserved across flattened module scopes.
    pub reserved_names: FxHashSet<String>,
    pub entry: usize,
}

/// Normalized paths of the project's declaration modules.
type Internal = BTreeSet<String>;

fn is_internal_file(file: P<SourceFile>, internal: &Internal) -> bool {
    internal.contains(tspath::normalize_path_cow(file.file_name()).as_ref())
}

fn declared_internally(symbol: P<Symbol>, internal: &Internal) -> bool {
    symbol.declarations().iter().any(|&declaration| {
        ast::get_source_file_of_node(declaration)
            .is_some_and(|file| is_internal_file(file, internal))
    })
}

fn canonical_id(checker: &mut Checker, symbol: P<Symbol>, internal: &Internal) -> SymbolId {
    ast::get_symbol_id(canonical(checker, symbol, internal))
}

fn canonical(checker: &mut Checker, symbol: P<Symbol>, internal: &Internal) -> P<Symbol> {
    let symbol = checker.get_export_symbol_of_symbol(symbol);
    if symbol.flags().contains(SymbolFlags::Alias) {
        let mut immediate = symbol;
        let mut visited = BTreeSet::new();
        while visited.insert(ast::get_symbol_id(immediate)) {
            if immediate.declarations().iter().any(|decl| {
                decl.kind() == Kind::ImportEqualsDeclaration
                    && decl.as_import_equals_declaration().module_reference().kind()
                        != Kind::ExternalModuleReference
            }) {
                return checker.get_export_symbol_of_symbol(immediate);
            }
            let Some(next) = checker.get_immediate_aliased_symbol_exported(immediate) else {
                break;
            };
            immediate = next;
            if !immediate.flags().contains(SymbolFlags::Alias) {
                break;
            }
        }
        let target = checker.get_aliased_symbol(symbol);
        if declared_internally(target, internal) {
            return checker.get_export_symbol_of_symbol(target);
        }
    }
    symbol
}

// Inspect every meaning: a local value on a merged alias must not stop the
// type-only query. Differential coverage lives in tests/integration/type_only_metadata.rs.
fn is_type_only_alias(checker: &mut Checker, symbol: P<Symbol>) -> bool {
    checker.get_type_only_alias_declaration_ex(symbol, SymbolFlags::None).is_some()
}

// Match SourceFile::has_identifier's literal-inclusive reservation set without
// exposing its private cache. The nested-binding regression covers scope capture.
//
// TODO: Use a shared reservation-set API when available; see docs/DTS.md.
fn reserve_names(node: P<Node>, names: &mut FxHashSet<String>) -> bool {
    if matches!(
        node.kind(),
        Kind::Identifier
            | Kind::PrivateIdentifier
            | Kind::StringLiteral
            | Kind::NumericLiteral
            | Kind::BigIntLiteral
            | Kind::NoSubstitutionTemplateLiteral
    ) {
        let text = node.text();
        if !names.contains(text) {
            names.insert(text.to_string());
        }
    }
    node.for_each_child(&mut |child| reserve_names(child, names));
    false
}

/// Follow resolved export paths to determine a name's type-only restriction.
/// Direct exports override stars; ordinary star paths take precedence over type-only paths.
fn export_is_type_only(
    checker: &mut Checker,
    module: P<Symbol>,
    name: &str,
    visited: &mut BTreeSet<SymbolId>,
) -> Option<bool> {
    // Star exports return target symbols. Aggregate star metadata plus the selected
    // alias can retain a named type-only restriction from a competing path.

    // TODO: Add a compiler query with equivalent path-sensitive restrictions, then
    // replace this traversal. See tests/integration/type_only_metadata.rs for both star orders
    // and docs/DTS.md.
    let id = ast::get_symbol_id(module);
    if !visited.insert(id) {
        return None;
    }
    let result = (|| {
        if let Some(symbol) = module.exports().and_then(|exports| exports.lookup(name)) {
            return Some(is_type_only_alias(checker, symbol));
        }
        let file = ast::get_source_file_of_module(module)?;
        let mut restriction = None;
        for &node in file.statements().nodes() {
            if node.kind() != Kind::ExportDeclaration {
                continue;
            }
            let data = node.as_export_declaration();
            if data.export_clause().is_some() || name == "default" {
                continue;
            }
            let Some(target) = data.module_specifier().and_then(|specifier| {
                checker.resolve_external_module_name_exported(specifier, None)
            }) else {
                continue;
            };
            // Augmentation can keep an export key different from symbol.name().
            // Search the filtered symbols by name, rather than looking up a key.
            if !checker
                .get_exports_of_module_exported(target)
                .iter()
                .any(|symbol| symbol.name() == name)
            {
                continue;
            }
            if let Some(type_only) = export_is_type_only(checker, target, name, visited) {
                if !data.is_type_only() && !type_only {
                    return Some(false);
                }
                restriction = Some(true);
            }
        }
        restriction
    })();
    visited.remove(&id);
    result
}

/// Resolve an import to a portable package specifier.
fn portable_specifier(
    program: &'static Program,
    checker: &mut Checker,
    source: P<SourceFile>,
    specifier: P<Node>,
) -> Option<String> {
    let text = specifier.text();
    if !tsrs_core::tspath::path_is_relative(text)
        && !tsrs_core::tspath::is_rooted_disk_path(text)
        && !text.starts_with('#')
    {
        return Some(text.to_string());
    }
    let module = checker.resolve_external_module_name_exported(specifier, None)?;
    let target = ast::get_source_file_of_module(module)?;
    // get_module_specifiers in tsrs_modulespecifiers/src/specifiers.rs may choose
    // project aliases or relative paths.

    // TODO: Add a shared query that guarantees portable package specifiers, then
    // replace this package-path selection. See docs/DTS.md.
    let preferences = tsrs_modulespecifiers::UserPreferences::default();
    let info = tsrs_modulespecifiers::get_info(source.file_name(), program);
    let paths = tsrs_modulespecifiers::get_all_module_paths(
        &info,
        target.file_name(),
        program,
        program.options().get(),
        &preferences,
        tsrs_modulespecifiers::ModuleSpecifierOptions::default(),
    );
    paths
        .iter()
        .map(|path| {
            tsrs_modulespecifiers::try_get_module_name_as_node_module(
                path,
                &info,
                source,
                program,
                program.options().get(),
                &preferences,
                false,
                tsrs_core::ModuleKind::default(),
            )
        })
        .find(|s| {
            !s.is_empty()
                && !s.starts_with('#')
                && !tsrs_core::tspath::path_is_relative(s)
                && !tsrs_core::tspath::is_rooted_disk_path(s)
        })
}

fn export_origin(
    program: &'static Program,
    checker: &mut Checker,
    module: P<Symbol>,
    name: &str,
    internal: &Internal,
    visited: &mut BTreeSet<(SymbolId, String)>,
) -> Option<ExternalOrigin> {
    if !visited.insert((ast::get_symbol_id(module), name.to_string())) {
        return None;
    }
    let file = ast::get_source_file_of_module(module)?;
    let selected =
        checker.get_exports_of_module_exported(module).into_iter().find(|s| s.name() == name)?;
    let selected = checker.skip_alias(selected);
    for &node in file.statements().nodes() {
        if node.kind() != Kind::ExportDeclaration {
            continue;
        }
        let export = node.as_export_declaration();
        let Some(specifier) = export.module_specifier() else {
            continue;
        };
        let imported = if let Some(clause) = export.export_clause() {
            if clause.kind() == Kind::NamespaceExport {
                if clause.name()?.text() != name {
                    continue;
                }
                String::new()
            } else {
                let element = clause.elements().iter().find(|e| e.name().unwrap().text() == name);
                let Some(element) = element else { continue };
                let data = element.as_export_specifier();
                data.property_name().unwrap_or(data.name()).text().to_string()
            }
        } else {
            if name == "default" {
                continue;
            }
            name.to_string()
        };
        let target = checker.resolve_external_module_name_exported(specifier, None)?;
        if !imported.is_empty() {
            let candidate = checker
                .get_exports_of_module_exported(target)
                .into_iter()
                .find(|s| s.name() == imported);
            let Some(candidate) = candidate else { continue };
            let candidate = checker.skip_alias(candidate);
            if checker.get_export_symbol_of_symbol(candidate)
                != checker.get_export_symbol_of_symbol(selected)
            {
                continue;
            }
        }
        if ast::get_source_file_of_module(target).is_some_and(|f| is_internal_file(f, internal)) {
            if let Some(origin) =
                export_origin(program, checker, target, &imported, internal, visited)
            {
                return Some(origin);
            }
        } else {
            let namespace = imported.is_empty();
            return Some(ExternalOrigin {
                specifier: portable_specifier(program, checker, file, specifier)?,
                imported,
                namespace,
                mode: program.get_mode_for_usage_location(file, specifier),
            });
        }
    }
    None
}

// Follow bound aliases to their public package boundary. Keep the imported name
// from that boundary rather than the name assigned by an intermediate barrel.
fn external_origin(
    program: &'static Program,
    checker: &mut Checker,
    mut symbol: P<Symbol>,
    internal: &Internal,
) -> Option<ExternalOrigin> {
    let mut visited = BTreeSet::new();
    while visited.insert(ast::get_symbol_id(symbol)) {
        for &declaration in symbol.declarations() {
            let mut ancestor = Some(declaration);
            while let Some(node) = ancestor {
                if matches!(node.kind(), Kind::ImportDeclaration | Kind::ExportDeclaration)
                    && let Some(specifier) = ast::get_external_module_name(node)
                {
                    let module = checker.resolve_external_module_name_exported(specifier, None)?;
                    if !declared_internally(module, internal) {
                        let (name, namespace) = match declaration.kind() {
                            Kind::ImportSpecifier => {
                                let data = declaration.as_import_specifier();
                                (
                                    data.property_name().unwrap_or(data.name()).text().to_string(),
                                    false,
                                )
                            }
                            Kind::ExportSpecifier => {
                                let data = declaration.as_export_specifier();
                                (
                                    data.property_name().unwrap_or(data.name()).text().to_string(),
                                    false,
                                )
                            }
                            Kind::NamespaceImport | Kind::NamespaceExport => (String::new(), true),
                            Kind::ImportClause => ("default".into(), false),
                            _ => (symbol.name().to_string(), false),
                        };
                        let file = ast::get_source_file_of_node(node)?;
                        return Some(ExternalOrigin {
                            specifier: portable_specifier(program, checker, file, specifier)?,
                            imported: name,
                            namespace,
                            mode: program.get_mode_for_usage_location(file, specifier),
                        });
                    }
                }
                ancestor = node.parent();
            }
        }
        if !symbol.flags().contains(SymbolFlags::Alias) {
            break;
        }
        symbol = checker.get_immediate_aliased_symbol_exported(symbol)?;
    }
    None
}

fn binding_names(name: P<Node>, names: &mut Vec<P<Node>>) {
    if name.kind() == Kind::Identifier {
        names.push(name);
    } else {
        for &element in ast::get_elements_of_binding_or_assignment_pattern(name) {
            if let Some(target) = ast::get_target_of_binding_or_assignment_element(element) {
                binding_names(target, names);
            }
        }
    }
}

/// Classify the type, value and namespace declarations a reference needs.
fn reference_meaning(mut node: P<Node>) -> ast::SemanticMeaning {
    use ast::SemanticMeaning as M;
    while let Some(parent) = node.parent() {
        if ast::is_property_access_or_qualified_name(parent) {
            if !ast::is_right_side_of_qualified_name_or_property_access(node) {
                // get_meaning_from_location in tsrs_ls/src/utilities.rs can return
                // Namespace alone, but qualifier roots also need their value declarations.

                // TODO: Expose a shared meaning helper that retains Namespace | Value
                // here. See the namespace_pruning_review_aliases_and_static_value_roots
                // fixture and docs/DTS.md.
                return M::Namespace | M::Value;
            }
            node = parent;
            continue;
        }
        if matches!(parent.kind(), Kind::ExportSpecifier | Kind::ImportEqualsDeclaration) {
            return M::All;
        }
        if parent.kind() == Kind::TypeQuery || parent.kind() == Kind::ComputedPropertyName {
            return M::Value;
        }
        return if ast::is_part_of_type_node(node) { M::Type } else { M::Value };
    }
    M::All
}

fn walk_statement(node: P<Node>, visitor: &mut impl FnMut(P<Node>) -> Result<()>) -> Result<()> {
    visitor(node)?;
    for child in node.iter_children() {
        if node.kind() == Kind::ModuleDeclaration && child.kind() == Kind::ModuleBlock {
            for &statement in child.statements() {
                if statement.kind() == Kind::ExportDeclaration {
                    walk(statement, visitor)?;
                }
            }
            continue;
        }
        walk(child, visitor)?;
    }
    Ok(())
}

fn walk(node: P<Node>, visitor: &mut impl FnMut(P<Node>) -> Result<()>) -> Result<()> {
    visitor(node)?;
    for child in node.iter_children() {
        walk(child, visitor)?;
    }
    Ok(())
}

/// The module whose file has the normalized path `path`.
fn find_module(modules: &[Module], path: &str) -> Option<usize> {
    modules.iter().position(|module| tspath::normalize_path_cow(module.file.file_name()) == path)
}

/// Queue a statement for analysis, splitting a variable statement into its declarators.
fn enqueue(
    nodes: &mut VecDeque<(P<Node>, bool, Option<usize>)>,
    node: P<Node>,
    effect: bool,
    namespace_parent: Option<usize>,
) {
    if node.kind() == Kind::VariableStatement {
        let list = node.as_variable_statement().declaration_list();
        nodes.extend(
            list.as_variable_declaration_list()
                .declarations()
                .nodes()
                .iter()
                .map(|&declaration| (declaration, effect, namespace_parent)),
        );
    } else {
        nodes.push_back((node, effect, namespace_parent));
    }
}

impl Graph {
    /// The module whose file has the normalized path `path`.
    pub fn find_module(&self, path: &str) -> Option<usize> {
        find_module(&self.modules, path)
    }

    /// Whether a symbol has a declaration, an import binding or a module in this graph.
    fn is_linked(&self, symbol: SymbolId) -> bool {
        self.owners.contains_key(&symbol)
            || self.imports.contains_key(&symbol)
            || self.module_symbols.contains_key(&symbol)
    }

    /// The graph module of a resolved module symbol.
    fn module_index(&self, checker: &mut Checker, module: P<Symbol>) -> Option<usize> {
        self.module_symbols
            .get(&ast::get_symbol_id(checker.get_export_symbol_of_symbol(module)))
            .copied()
    }

    /// The graph module a module specifier resolves to.
    fn resolved_module(&self, checker: &mut Checker, specifier: P<Node>) -> Option<usize> {
        let module = checker.resolve_external_module_name_exported(specifier, None)?;
        self.module_index(checker, module)
    }

    fn register_external_origin(
        &mut self,
        module: usize,
        symbol: SymbolId,
        type_only: bool,
        origin: ExternalOrigin,
    ) {
        let local = format!("external{}", self.imports.len());
        self.binding_names.insert(symbol, local.clone());
        self.imports.insert(
            symbol,
            ImportBinding {
                module,
                symbol,
                target: symbol,
                local,
                imported: origin.imported,
                specifier: origin.specifier,
                type_only,
                external: true,
                namespace: origin.namespace,
                require: false,
                mode: origin.mode,
                import_module: None,
            },
        );
    }

    pub fn analyze(program: &'static Program, files: &Files, entry: &str) -> Result<Self> {
        // CapturedDeclarations::validate has checked that every path is a declaration file.
        let modules = files
            .keys()
            .map(|path| {
                program
                    .get_source_file(path)
                    .map(Module::new)
                    .ok_or_else(|| Error(format!("declaration not loaded: {path}")))
            })
            .collect::<Result<Vec<_>>>()?;
        let internal: Internal = modules
            .iter()
            .filter(|module| !module.script)
            .map(|module| tspath::normalize_path(module.file.file_name()))
            .collect();
        let entry = find_module(&modules, &tspath::normalize_path(entry))
            .ok_or_else(|| Error("entry missing from declaration graph".into()))?;
        let mut checker = program
            .get_type_checker_for_file_exclusive(&Context::background(), modules[entry].file);
        // Union once: scanning every source file on each candidate name scales poorly
        // with module count and repeated names. Only membership affects emitted output.
        let mut reserved_names = FxHashSet::default();
        for module in &modules {
            reserve_names(module.file.as_node(), &mut reserved_names);
        }
        let mut graph = Self {
            modules,
            statements: Vec::new(),
            imports: BTreeMap::new(),
            inline_imports: FxHashMap::default(),
            external_specifiers: FxHashMap::default(),
            external_import_modes: FxHashMap::default(),
            anonymous: BTreeMap::new(),
            namespace_children: BTreeMap::new(),
            module_symbols: BTreeMap::new(),
            owners: FxHashMap::default(),
            binding_names: FxHashMap::default(),
            references: FxHashMap::default(),
            reserved_names,
            entry,
        };
        for (index, module) in graph.modules.iter().enumerate() {
            let Some(symbol) = module.file.symbol() else {
                continue;
            };
            let symbol = ast::get_symbol_id(checker.get_export_symbol_of_symbol(symbol));
            graph.module_symbols.insert(symbol, index);
            graph.binding_names.insert(symbol, "module".into());
        }
        // First establish all top-level declaration owners, including every
        // overload/merged declaration for a symbol, before collecting any edges.
        for module in 0..graph.modules.len() {
            let file = graph.modules[module].file;
            for &reference in file.referenced_files() {
                if let Some(target) = program.get_source_file_from_reference(file, reference)
                    && let Some(target) =
                        graph.modules.iter().position(|module| module.file == target)
                {
                    graph.modules[module].dependencies.insert(target);
                }
            }
            let mut nodes = VecDeque::new();
            for &node in file.statements().nodes() {
                if ast::is_external_module_augmentation(node)
                    && !ast::is_global_scope_augmentation(node)
                    && let Some(target) = graph.resolved_module(&mut checker, node.name().unwrap())
                {
                    graph.modules[module].dependencies.insert(target);
                    if let Some(body) = node.body() {
                        for &child in body.statements() {
                            enqueue(&mut nodes, child, true, None);
                        }
                    }
                } else {
                    enqueue(&mut nodes, node, false, None);
                }
            }
            while let Some((node, effect, namespace_parent)) = nodes.pop_front() {
                if matches!(node.kind(), Kind::ImportDeclaration | Kind::ExportDeclaration)
                    && let Some(specifier) = ast::get_external_module_name(node)
                    && let Some(target) = graph.resolved_module(&mut checker, specifier)
                {
                    graph.modules[module].dependencies.insert(target);
                }
                if node.kind() == Kind::ModuleDeclaration && ast::is_ambient_module(node) {
                    let index = graph.statements.len();
                    graph.modules[module].effects.push(index);
                    graph.statements.push(Statement::new(
                        module,
                        node,
                        BTreeSet::new(),
                        true,
                        namespace_parent,
                    ));
                    continue;
                }
                match node.kind() {
                    Kind::ImportDeclaration => {
                        graph.register_imports(program, module, node, &mut checker, &internal)?;
                    }
                    Kind::ImportEqualsDeclaration => {
                        let data = node.as_import_equals_declaration();
                        let name = data.name();
                        let raw =
                            checker.get_symbol_at_location_exported(name).ok_or_else(|| {
                                Error("import-equals binding has no tsrs symbol".into())
                            })?;
                        let symbol = ast::get_symbol_id(raw);
                        let target = canonical_id(&mut checker, raw, &internal);
                        let reference = data.module_reference();
                        if reference.kind() == Kind::ExternalModuleReference {
                            let specifier = reference.expression().unwrap();
                            // An untyped JavaScript module resolves to no symbol without an
                            // error unless noImplicitAny is set; keep it as an external require.
                            let import_module = graph.resolved_module(&mut checker, specifier);
                            let external = import_module.is_none();
                            if let Some(target) = import_module {
                                graph.modules[module].dependencies.insert(target);
                            }
                            if external {
                                graph.binding_names.insert(symbol, name.text().into());
                            }
                            graph.imports.insert(
                                symbol,
                                ImportBinding {
                                    module,
                                    symbol,
                                    target,
                                    local: name.text().into(),
                                    imported: String::new(),
                                    specifier: if external {
                                        portable_specifier(program, &mut checker, file, specifier)
                                            .ok_or_else(|| Error("external require import has no public package path".into()))?
                                    } else {
                                        specifier.text().into()
                                    },
                                    type_only: data.is_type_only(),
                                    external,
                                    namespace: true,
                                    require: true,
                                    mode: program.get_mode_for_usage_location(file, specifier),
                                    import_module,
                                },
                            );
                        } else {
                            // A qualified alias remains a declaration; its root reference
                            // is collected by the same checker-based dependency walk.
                            if let Some(parent) = namespace_parent {
                                graph
                                    .namespace_children
                                    .entry(parent)
                                    .or_default()
                                    .push(graph.statements.len());
                            } else {
                                graph.binding_names.insert(symbol, name.text().into());
                            }
                            graph.owners.entry(symbol).or_default().push(graph.statements.len());
                            graph.references.insert(name, symbol);
                            graph.statements.push(Statement::new(
                                module,
                                node,
                                BTreeSet::from([symbol]),
                                effect,
                                namespace_parent,
                            ));
                        }
                    }
                    Kind::NamespaceExportDeclaration => {
                        graph.modules[module].global_exports.push(node);
                    }
                    Kind::ExportAssignment => {
                        let data = node.as_export_assignment();
                        if data.is_export_equals() {
                            let root = ast::get_first_identifier(data.expression());
                            let raw =
                                checker.get_symbol_at_location_exported(root).ok_or_else(|| {
                                    Error("export assignment has no bound target".into())
                                })?;
                            graph.modules[module].assignment =
                                Some(canonical_id(&mut checker, raw, &internal));
                            graph.modules[module].assignment_node = Some(node);
                        }
                    }
                    Kind::ExportDeclaration | Kind::EmptyStatement => (),
                    Kind::InterfaceDeclaration
                    | Kind::TypeAliasDeclaration
                    | Kind::ClassDeclaration
                    | Kind::FunctionDeclaration
                    | Kind::EnumDeclaration
                    | Kind::ModuleDeclaration
                    | Kind::VariableDeclaration => {
                        let index = graph.statements.len();
                        let mut bindings = BTreeSet::new();
                        if let Some(name) = node.name() {
                            let mut names = Vec::new();
                            binding_names(name, &mut names);
                            for name in names {
                                let symbol = checker
                                    .get_symbol_at_location_exported(name)
                                    .ok_or_else(|| {
                                        Error(format!(
                                            "tsrs returned no declaration symbol for {}",
                                            name.text()
                                        ))
                                    })?;
                                let symbol = canonical_id(&mut checker, symbol, &internal);
                                bindings.insert(symbol);
                                if namespace_parent.is_none() {
                                    graph
                                        .binding_names
                                        .entry(symbol)
                                        .or_insert_with(|| name.text().to_string());
                                }
                                graph.owners.entry(symbol).or_default().push(index);
                            }
                        } else {
                            let symbol = node.symbol().ok_or_else(|| {
                                Error("anonymous declaration has no tsrs symbol".into())
                            })?;
                            let symbol = canonical_id(&mut checker, symbol, &internal);
                            bindings.insert(symbol);
                            graph.binding_names.insert(symbol, format!("default{module}"));
                            graph.anonymous.insert(node, symbol);
                            graph.owners.entry(symbol).or_default().push(index);
                        }
                        if let Some(parent) = namespace_parent {
                            graph.namespace_children.entry(parent).or_default().push(index);
                        }
                        if node.kind() == Kind::ModuleDeclaration
                            && !graph.modules[module].script
                            && let Some(body) = node.body()
                            && body.kind() == Kind::ModuleBlock
                        {
                            for &child in body.statements() {
                                enqueue(&mut nodes, child, false, Some(index));
                            }
                        }
                        if effect || graph.modules[module].script {
                            graph.modules[module].effects.push(index);
                        }
                        graph.statements.push(Statement::new(
                            module,
                            node,
                            bindings,
                            effect,
                            namespace_parent,
                        ));
                    }
                    other => {
                        return Err(Error(format!("unsupported top-level declaration: {other:?}")));
                    }
                }
            }
            let Some(module_symbol) = file.symbol() else {
                continue;
            };
            for symbol in checker.get_exports_of_module_exported(module_symbol) {
                let type_only = export_is_type_only(
                    &mut checker,
                    module_symbol,
                    symbol.name(),
                    &mut BTreeSet::new(),
                )
                .unwrap_or_else(|| is_type_only_alias(&mut checker, symbol));
                let target = checker.skip_alias(symbol);
                graph.modules[module].has_values |=
                    !type_only && target.flags().intersects(SymbolFlags::Value);
                let name = symbol.name().to_string();
                let resolved = canonical_id(&mut checker, symbol, &internal);
                if !graph.is_linked(resolved)
                    && let Some(origin) = external_origin(program, &mut checker, symbol, &internal)
                        .or_else(|| {
                            export_origin(
                                program,
                                &mut checker,
                                module_symbol,
                                &name,
                                &internal,
                                &mut BTreeSet::new(),
                            )
                        })
                {
                    graph.register_external_origin(module, resolved, type_only, origin);
                }
                graph.modules[module].exports.push(PublicExport {
                    name,
                    symbol: resolved,
                    type_only,
                    // A type-only export can still expose a value-only symbol to
                    // `typeof` queries. Only merged symbols with a type identity
                    // may discard their value declaration at this boundary.
                    meaning: if type_only
                        && target.flags().intersects(SymbolFlags::Type | SymbolFlags::Namespace)
                    {
                        ast::SemanticMeaning::Type | ast::SemanticMeaning::Namespace
                    } else {
                        ast::SemanticMeaning::All
                    },
                });
            }
            if graph.modules[module].assignment.is_some() {
                graph.modules[module].exports.clear();
            }
            graph.modules[module].exports.sort_by(|a, b| a.name.cmp(&b.name));
        }
        // Then bind identifier references, including computed keys, heritage,
        // type queries and nested generic scopes through the tsrs checker.
        for index in 0..graph.statements.len() {
            let node = graph.statements[index].node;
            let owner = graph.statements[index].module;
            let mut dependencies = BTreeSet::new();
            let mut dependency_meanings = BTreeMap::new();
            let mut unsupported_import = None;
            let namespace = graph.namespace_children.contains_key(&index);
            let mut collect = |child: P<Node>| {
                if child.kind() == Kind::ImportType {
                    let import = child.as_import_type_node();
                    if !ast::is_literal_import_type_node(child) {
                        unsupported_import.get_or_insert_with(|| {
                            "non-literal import() types are not supported".into()
                        });
                        return Ok(());
                    }
                    let argument = import.argument().as_literal_type_node().literal();
                    let module = checker
                        .resolve_external_module_name_exported(argument, None)
                        .ok_or_else(|| Error("import() type has no resolved module".into()))?;
                    let module = ast::get_symbol_id(checker.get_export_symbol_of_symbol(module));
                    let target = graph.module_symbols.get(&module).copied();
                    if let Some(target) = target {
                        graph.modules[owner].dependencies.insert(target);
                        if let Some(qualifier) = import.qualifier() {
                            let root = ast::get_first_identifier(qualifier);
                            let raw =
                                checker.get_symbol_at_location_exported(root).ok_or_else(|| {
                                    Error("import() qualifier has no tsrs symbol".into())
                                })?;
                            let symbol = canonical_id(&mut checker, raw, &internal);
                            if !graph.is_linked(symbol)
                                && let Some(origin) =
                                    external_origin(program, &mut checker, raw, &internal)
                            {
                                let type_only = is_type_only_alias(&mut checker, raw);
                                graph.register_external_origin(owner, symbol, type_only, origin);
                            }
                            if graph.is_linked(symbol) {
                                dependencies.insert(symbol);
                                *dependency_meanings
                                    .entry(symbol)
                                    .or_insert(ast::SemanticMeaning::None) |= if import.is_type_of()
                                {
                                    ast::SemanticMeaning::Value
                                } else {
                                    ast::SemanticMeaning::Type | ast::SemanticMeaning::Namespace
                                };
                                graph
                                    .inline_imports
                                    .insert(child, InlineImport { symbol, root: Some(root) });
                            } else {
                                unsupported_import.get_or_insert_with(|| "external types forwarded through internal import() barrels require external provenance tracking".into());
                            }
                        } else {
                            dependencies.insert(module);
                            graph
                                .inline_imports
                                .insert(child, InlineImport { symbol: module, root: None });
                        }
                    } else {
                        let source = graph.modules[owner].file;
                        graph
                            .external_import_modes
                            .insert(child, program.get_mode_for_usage_location(source, argument));
                        if tspath::path_is_relative(argument.text())
                            || tspath::is_rooted_disk_path(argument.text())
                            || argument.text().starts_with('#')
                        {
                            if let Some(specifier) =
                                portable_specifier(program, &mut checker, source, argument)
                            {
                                graph.external_specifiers.insert(argument, specifier);
                            } else {
                                unsupported_import.get_or_insert_with(|| {
                                    "external import() has no portable public package specifier"
                                        .into()
                                });
                            }
                        }
                    }
                }
                if child.kind() == Kind::ExportSpecifier {
                    let data = child.as_export_specifier();
                    if let Some(raw) = checker.get_export_specifier_local_target_symbol(child) {
                        let symbol = canonical_id(&mut checker, raw, &internal);
                        graph
                            .references
                            .insert(data.property_name().unwrap_or(data.name()), symbol);
                        dependencies.insert(symbol);
                        *dependency_meanings.entry(symbol).or_insert(ast::SemanticMeaning::None) |=
                            ast::SemanticMeaning::All;
                    }
                    return Ok(());
                }
                if child.kind() != Kind::Identifier {
                    return Ok(());
                }
                if ast::is_this_in_type_query(child) {
                    return Ok(());
                }
                if child.parent().is_some_and(|parent| parent.kind() == Kind::ExportSpecifier) {
                    return Ok(());
                }
                if let Some(raw) = checker.get_symbol_at_location_exported(child) {
                    let symbol = canonical_id(&mut checker, raw, &internal);
                    graph.references.insert(child, symbol);
                    if graph.is_linked(symbol) && !ast::is_declaration_name(child) {
                        dependencies.insert(symbol);
                        let meaning = reference_meaning(child);
                        *dependency_meanings.entry(symbol).or_insert(ast::SemanticMeaning::None) |=
                            meaning;
                    }
                }
                Ok(())
            };
            if namespace {
                walk_statement(node, &mut collect)?;
            } else {
                walk(node, &mut collect)?;
            }
            for symbol in &dependencies {
                dependency_meanings.entry(*symbol).or_insert(ast::SemanticMeaning::All);
            }
            graph.statements[index].dependency_meanings = dependency_meanings;
            graph.statements[index].unsupported_import = unsupported_import;
        }
        for statement in &graph.statements {
            if ast::is_ambient_module(statement.node) {
                let mut members = BTreeSet::new();
                if let Some(body) = statement.node.body() {
                    for &member in body.statements() {
                        if let Some(name) = member.name()
                            && let Some(&symbol) = graph.references.get(&name)
                        {
                            members.insert(symbol);
                        }
                    }
                }
                walk(statement.node, &mut |node| {
                    if graph.references.get(&node).is_some_and(|symbol| members.contains(symbol)) {
                        graph.references.remove(&node);
                    }
                    Ok(())
                })?;
                if !ast::is_global_scope_augmentation(statement.node)
                    && !graph.modules[statement.module].script
                {
                    let name = statement.node.name().unwrap();
                    if let Some(specifier) = portable_specifier(
                        program,
                        &mut checker,
                        graph.modules[statement.module].file,
                        name,
                    ) {
                        graph.external_specifiers.insert(name, specifier);
                    } else {
                        graph.modules[statement.module].error.get_or_insert_with(|| {
                            "module augmentation has no public package path".into()
                        });
                    }
                }
            }
        }
        for &symbol in graph.anonymous.values() {
            let mut name = graph.binding_names[&symbol].clone();
            while graph.reserved_names.contains(&name) {
                name.push('_');
            }
            graph.reserved_names.insert(name.clone());
            graph.binding_names.insert(symbol, name);
        }
        for export in &graph.modules[graph.entry].exports {
            if !graph.is_linked(export.symbol) {
                return Err(Error(format!(
                    "entry export {:?} requires an unsupported external/namespace re-export",
                    export.name
                )));
            }
        }
        Ok(graph)
    }

    fn register_imports(
        &mut self,
        program: &'static Program,
        module: usize,
        node: P<Node>,
        checker: &mut Checker,
        internal: &Internal,
    ) -> Result<()> {
        let declaration = node.as_import_declaration();
        let Some(clause) = declaration.import_clause() else {
            let specifier = declaration.module_specifier();
            if self.resolved_module(checker, specifier).is_none() {
                let mode =
                    program.get_mode_for_usage_location(self.modules[module].file, specifier);
                if let Some(specifier) =
                    portable_specifier(program, checker, self.modules[module].file, specifier)
                {
                    self.modules[module].side_effects.push((specifier, mode));
                } else {
                    self.modules[module].error.get_or_insert_with(|| {
                        "side-effect import has no public package path".into()
                    });
                }
            }
            return Ok(());
        };
        let clause = clause.as_import_clause();
        let specifier = declaration.module_specifier().text();
        let specifier_is_internal = checker
            .resolve_external_module_name_exported(declaration.module_specifier(), None)
            .is_some_and(|symbol| declared_internally(symbol, internal));
        let type_only = clause.phase_modifier() == Kind::TypeKeyword;
        let mut pending = Vec::new();
        if let Some(name) = clause.name() {
            pending.push((name, "default".to_string(), type_only, false));
        }
        if let Some(bindings) = clause.named_bindings() {
            if bindings.kind() == Kind::NamespaceImport {
                pending.push((bindings.name().unwrap(), String::new(), type_only, true));
            } else {
                for &element in bindings.elements() {
                    let spec = element.as_import_specifier();
                    pending.push((
                        spec.name(),
                        spec.property_name().unwrap_or(spec.name()).text().to_string(),
                        type_only || spec.is_type_only(),
                        false,
                    ));
                }
            }
        }
        for (name, imported, type_only, namespace) in pending {
            let raw = checker
                .get_symbol_at_location_exported(name)
                .ok_or_else(|| Error("import binding has no tsrs symbol".into()))?;
            let type_only = type_only || is_type_only_alias(checker, raw);
            let symbol = ast::get_symbol_id(raw);
            let target = canonical_id(checker, raw, internal);
            let external = symbol == target;
            let origin = if external && specifier_is_internal {
                external_origin(program, checker, raw, internal)
                    .or_else(|| {
                        let target_module = checker.resolve_external_module_name_exported(
                            declaration.module_specifier(),
                            None,
                        )?;
                        export_origin(
                            program,
                            checker,
                            target_module,
                            &imported,
                            internal,
                            &mut BTreeSet::new(),
                        )
                    })
                    .ok_or_else(|| {
                        Error("external alias has no portable package provenance".into())
                    })?
            } else {
                ExternalOrigin {
                    specifier: if external {
                        portable_specifier(
                            program,
                            checker,
                            self.modules[module].file,
                            declaration.module_specifier(),
                        )
                        .ok_or_else(|| Error("external import has no public package path".into()))?
                    } else {
                        specifier.to_owned()
                    },
                    imported,
                    namespace,
                    mode: program.get_mode_for_usage_location(
                        self.modules[module].file,
                        declaration.module_specifier(),
                    ),
                }
            };
            if external {
                self.binding_names.insert(symbol, name.text().to_string());
                if !type_only {
                    self.modules[module].side_effects.push((origin.specifier.clone(), origin.mode));
                }
            }
            self.imports.insert(
                symbol,
                ImportBinding {
                    module,
                    symbol,
                    target,
                    local: name.text().to_string(),
                    imported: origin.imported,
                    specifier: origin.specifier,
                    type_only,
                    external,
                    namespace: origin.namespace,
                    require: false,
                    mode: origin.mode,
                    import_module: None,
                },
            );
        }
        Ok(())
    }

    /// Find declarations reachable from `entry`, tracking type and value usage.
    pub fn reachable(&self, entry: usize) -> Result<Live> {
        // Note: is_declaration_visible in tsrs_checker/src/emitresolver.rs retains
        // module exports. Replacing this traversal needs entry-based pruning too;
        // see docs/DTS.md.
        use ast::SemanticMeaning as M;
        let mut meanings = BTreeMap::<SymbolId, M>::new();
        let mut live = Live::default();
        let mut modules = vec![entry];
        modules.extend(self.modules.iter().enumerate().filter_map(|(i, m)| m.script.then_some(i)));
        let mut pending: Vec<(SymbolId, M)> =
            self.modules[entry].exports.iter().map(|e| (e.symbol, e.meaning)).collect();
        pending.extend(self.modules[entry].assignment.map(|symbol| (symbol, M::All)));
        let mut processed_modules = BTreeSet::new();
        for import in self.imports.values() {
            if import.module == entry && self.modules[entry].assignment == Some(import.target) {
                pending.push((import.symbol, M::All));
            }
        }
        loop {
            while let Some(module) = modules.pop() {
                if !live.modules.insert(module) {
                    continue;
                }
                modules.extend(self.modules[module].dependencies.iter().copied());
            }
            for &module in &live.modules {
                if !processed_modules.insert(module) {
                    continue;
                }
                if let Some(reason) = &self.modules[module].error {
                    return Err(Error(reason.clone()));
                }
                for &index in &self.modules[module].effects {
                    if let Some(reason) = &self.statements[index].unsupported_import {
                        return Err(Error(reason.clone()));
                    }
                    live.statements.insert(index);
                    pending.extend(
                        self.statements[index].bindings.iter().map(|&symbol| (symbol, M::All)),
                    );
                    pending.extend(
                        self.statements[index]
                            .dependency_meanings
                            .iter()
                            .map(|(&symbol, &meaning)| (symbol, meaning)),
                    );
                }
            }
            while let Some((symbol, meaning)) = pending.pop() {
                let seen = meanings.entry(symbol).or_insert(M::None);
                if seen.contains(meaning) {
                    continue;
                }
                *seen |= meaning;
                let meaning = *seen;
                live.symbols.insert(symbol);
                if let Some(&module) = self.module_symbols.get(&symbol) {
                    modules.push(module);
                    pending.extend(self.modules[module].assignment.map(|symbol| (symbol, M::All)));
                    for export in &self.modules[module].exports {
                        if !self.is_linked(export.symbol) {
                            return Err(Error(format!(
                                "module export {:?} requires external provenance tracking",
                                export.name
                            )));
                        }
                        pending.push((export.symbol, export.meaning));
                    }
                }
                if let Some(import) = self.imports.get(&symbol) {
                    if let Some(module) = import.import_module {
                        modules.push(module);
                        pending
                            .extend(self.modules[module].assignment.map(|symbol| (symbol, M::All)));
                        for alias in self.imports.values().filter(|alias| {
                            alias.module == module
                                && self.modules[module].assignment == Some(alias.target)
                        }) {
                            pending.push((alias.symbol, M::All));
                        }
                    }
                    if import.target != symbol {
                        pending.push((import.target, meaning));
                    }
                }
                if let Some(owners) = self.owners.get(&symbol) {
                    for &index in owners {
                        let statement = &self.statements[index];
                        if statement.augmentation && !live.modules.contains(&statement.module) {
                            continue;
                        }
                        let declaration_meaning = ast::get_meaning_from_declaration(statement.node);
                        if !declaration_meaning.intersects(meaning) {
                            continue;
                        }
                        modules.push(statement.module);
                        if let Some(children) = self.namespace_children.get(&index)
                            && meaning != M::Namespace
                            && meaning != (M::Namespace | M::Value)
                        {
                            for &child in children {
                                pending.extend(
                                    self.statements[child]
                                        .bindings
                                        .iter()
                                        .map(|&symbol| (symbol, M::All)),
                                );
                            }
                        }
                        let mut parent = statement.namespace_parent;
                        while let Some(index) = parent {
                            pending.extend(
                                self.statements[index]
                                    .bindings
                                    .iter()
                                    .map(|&symbol| (symbol, M::Namespace)),
                            );
                            parent = self.statements[index].namespace_parent;
                        }
                        if !live.statements.insert(index) {
                            continue;
                        }
                        let statement = &self.statements[index];
                        if let Some(reason) = &statement.unsupported_import {
                            return Err(Error(reason.clone()));
                        }
                        // Multiple variable bindings in a single statement remain atomic.
                        pending.extend(
                            statement
                                .bindings
                                .iter()
                                .filter(|&&binding| binding != symbol)
                                .map(|&binding| (binding, declaration_meaning)),
                        );
                        pending.extend(
                            statement
                                .dependency_meanings
                                .iter()
                                .map(|(&symbol, &meaning)| (symbol, meaning)),
                        );
                    }
                }
            }
            if modules.is_empty() {
                break;
            }
        }
        Ok(live)
    }
}
