use rustc_hash::FxHashSet;
use tsrs_ast::{
    self as ast, CheckFlags, FindAncestorResult, Kind, ModifierFlags, Node, NodeFlags, Symbol,
    SymbolFlags,
};
use tsrs_checker::Checker;
use tsrs_core::P;

// symbol_display.go:10
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub enum ScriptElementKind {
    #[default]
    Unknown,
    Warning,
    // predefined type (void) or keyword (class)
    Keyword,
    // top level script node
    ScriptElement,
    // module foo {}
    ModuleElement,
    // class X {}
    ClassElement,
    // var x = class X {}
    LocalClassElement,
    // interface Y {}
    InterfaceElement,
    // type T = ...
    TypeElement,
    // enum E {}
    EnumElement,
    EnumMemberElement,
    // Inside module and script only.
    // const v = ...
    VariableElement,
    // Inside function.
    LocalVariableElement,
    // using foo = ...
    VariableUsingElement,
    // await using foo = ...
    VariableAwaitUsingElement,
    // Inside module and script only.
    // function f() {}
    FunctionElement,
    // Inside function.
    LocalFunctionElement,
    // class X { [public|private]* foo() {} }
    MemberFunctionElement,
    // class X { [public|private]* [get|set] foo:number; }
    MemberGetAccessorElement,
    MemberSetAccessorElement,
    // class X { [public|private]* foo:number; }
    // interface Y { foo:number; }
    MemberVariableElement,
    // class X { [public|private]* accessor foo: number; }
    MemberAccessorVariableElement,
    // class X { constructor() { } }
    // class X { static { } }
    ConstructorImplementationElement,
    // interface Y { ():number; }
    CallSignatureElement,
    // interface Y { []:number; }
    IndexSignatureElement,
    // interface Y { new():Y; }
    ConstructSignatureElement,
    // function foo(*Y*: string)
    ParameterElement,
    TypeParameterElement,
    PrimitiveType,
    Label,
    Alias,
    ConstElement,
    LetElement,
    Directory,
    ExternalModuleName,
    // String literal
    String,
    // Jsdoc @link: in `{@link C link text}`, the before and after text "{@link " and "}"
    Link,
    // Jsdoc @link: in `{@link C link text}`, the entity name "C"
    LinkName,
    // Jsdoc @link: in `{@link C link text}`, the link text "link text"
    LinkText,
}

bitflags::bitflags! {
    // symbol_display.go:83 (Public is `1 << iota` with iota == 1)
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct ScriptElementKindModifier: u32 {
        const PUBLIC = 1 << 1;
        const PRIVATE = 1 << 2;
        const PROTECTED = 1 << 3;
        const EXPORTED = 1 << 4;
        const AMBIENT = 1 << 5;
        const STATIC = 1 << 6;
        const ABSTRACT = 1 << 7;
        const OPTIONAL = 1 << 8;
        const DEPRECATED = 1 << 9;
        const DTS = 1 << 10;
        const TS = 1 << 11;
        const TSX = 1 << 12;
        const JS = 1 << 13;
        const JSX = 1 << 14;
        const JSON = 1 << 15;
        const DMTS = 1 << 16;
        const MTS = 1 << 17;
        const MJS = 1 << 18;
        const DCTS = 1 << 19;
        const CTS = 1 << 20;
        const CJS = 1 << 21;
    }
}

impl ScriptElementKindModifier {
    pub const NONE: ScriptElementKindModifier = ScriptElementKindModifier::empty();
}

// symbol_display.go:108
const SCRIPT_ELEMENT_KIND_MODIFIER_NAMES: &[(ScriptElementKindModifier, &str)] = &[
    (ScriptElementKindModifier::PUBLIC, "public"),
    (ScriptElementKindModifier::PRIVATE, "private"),
    (ScriptElementKindModifier::PROTECTED, "protected"),
    (ScriptElementKindModifier::EXPORTED, "export"),
    (ScriptElementKindModifier::AMBIENT, "declare"),
    (ScriptElementKindModifier::STATIC, "static"),
    (ScriptElementKindModifier::ABSTRACT, "abstract"),
    (ScriptElementKindModifier::OPTIONAL, "optional"),
    (ScriptElementKindModifier::DEPRECATED, "deprecated"),
    (ScriptElementKindModifier::DTS, ".d.ts"),
    (ScriptElementKindModifier::TS, ".ts"),
    (ScriptElementKindModifier::TSX, ".tsx"),
    (ScriptElementKindModifier::JS, ".js"),
    (ScriptElementKindModifier::JSX, ".jsx"),
    (ScriptElementKindModifier::JSON, ".json"),
    (ScriptElementKindModifier::DMTS, ".d.mts"),
    (ScriptElementKindModifier::MTS, ".mts"),
    (ScriptElementKindModifier::MJS, ".mjs"),
    (ScriptElementKindModifier::DCTS, ".d.cts"),
    (ScriptElementKindModifier::CTS, ".cts"),
    (ScriptElementKindModifier::CJS, ".cjs"),
];

impl ScriptElementKindModifier {
    // symbol_display.go:135
    pub fn strings(self) -> FxHashSet<&'static str> {
        let mut result = FxHashSet::default();
        result.extend(
            SCRIPT_ELEMENT_KIND_MODIFIER_NAMES
                .iter()
                .filter_map(|&(flag, name)| if self.intersects(flag) { Some(name) } else { None }),
        );
        result
    }
}

// symbol_display.go:145
pub const FILE_EXTENSION_KIND_MODIFIERS: ScriptElementKindModifier = ScriptElementKindModifier::DTS
    .union(ScriptElementKindModifier::TS)
    .union(ScriptElementKindModifier::TSX)
    .union(ScriptElementKindModifier::JS)
    .union(ScriptElementKindModifier::JSX)
    .union(ScriptElementKindModifier::JSON)
    .union(ScriptElementKindModifier::DMTS)
    .union(ScriptElementKindModifier::MTS)
    .union(ScriptElementKindModifier::MJS)
    .union(ScriptElementKindModifier::DCTS)
    .union(ScriptElementKindModifier::CTS)
    .union(ScriptElementKindModifier::CJS);

// symbol_display.go:159
pub fn get_symbol_kind(
    type_checker: Option<&mut Checker>,
    symbol: P<Symbol>,
    location: P<Node>,
) -> ScriptElementKind {
    let result = get_symbol_kind_of_constructor_property_method_accessor_function_or_var(
        type_checker,
        symbol,
        location,
    );
    if result != ScriptElementKind::Unknown {
        return result;
    }
    let flags = symbol.combined_local_and_export_symbol_flags();
    if flags.intersects(SymbolFlags::CLASS) {
        let decl = ast::get_declaration_of_kind(symbol, Kind::ClassExpression);
        if decl.is_some() {
            return ScriptElementKind::LocalClassElement;
        }
        return ScriptElementKind::ClassElement;
    }
    if flags.intersects(SymbolFlags::ENUM) {
        return ScriptElementKind::EnumElement;
    }
    if flags.intersects(SymbolFlags::TYPE_ALIAS) {
        return ScriptElementKind::TypeElement;
    }
    if flags.intersects(SymbolFlags::INTERFACE) {
        return ScriptElementKind::InterfaceElement;
    }
    if flags.intersects(SymbolFlags::TYPE_PARAMETER) {
        return ScriptElementKind::TypeParameterElement;
    }
    if flags.intersects(SymbolFlags::ENUM_MEMBER) {
        return ScriptElementKind::EnumMemberElement;
    }
    if flags.intersects(SymbolFlags::ALIAS) {
        return ScriptElementKind::Alias;
    }
    if flags.intersects(SymbolFlags::MODULE) {
        return ScriptElementKind::ModuleElement;
    }

    ScriptElementKind::Unknown
}

// symbol_display.go:198
fn get_symbol_kind_of_constructor_property_method_accessor_function_or_var(
    mut type_checker: Option<&mut Checker>,
    symbol: P<Symbol>,
    location: P<Node>,
) -> ScriptElementKind {
    let roots: Vec<P<Symbol>> = match type_checker.as_deref_mut() {
        Some(c) => c.get_root_symbols(symbol),
        None => vec![symbol],
    };

    // If this is a method from a mapped type, leave as a method so long as it still has a call signature, as opposed to e.g.
    // `{ [K in keyof I]: number }`.
    if roots.len() == 1 && roots[0].flags().intersects(SymbolFlags::METHOD) {
        let has_call_signatures = match type_checker.as_deref_mut() {
            None => true,
            Some(c) => {
                let t = c.get_type_of_symbol_at_location(symbol, Some(location)).unwrap();
                let t = c.get_non_nullable_type(t);
                !c.get_call_signatures(t).is_empty()
            }
        };
        if has_call_signatures {
            return ScriptElementKind::MemberFunctionElement;
        }
    }

    if let Some(c) = type_checker.as_deref_mut() {
        if c.is_undefined_symbol(symbol) {
            return ScriptElementKind::VariableElement;
        }
        if c.is_arguments_symbol(symbol) {
            return ScriptElementKind::LocalVariableElement;
        }
        if location.kind() == Kind::ThisKeyword && ast::is_expression(location)
            || ast::is_this_in_type_query(location)
        {
            return ScriptElementKind::ParameterElement;
        }
    }

    let flags = symbol.combined_local_and_export_symbol_flags();
    if flags.intersects(SymbolFlags::VARIABLE) {
        if is_first_declaration_of_symbol_parameter(symbol) {
            return ScriptElementKind::ParameterElement;
        } else if symbol.value_declaration().is_some_and(ast::is_var_const) {
            return ScriptElementKind::ConstElement;
        } else if symbol.value_declaration().is_some_and(ast::is_var_using) {
            return ScriptElementKind::VariableUsingElement;
        } else if symbol.value_declaration().is_some_and(ast::is_var_await_using) {
            return ScriptElementKind::VariableAwaitUsingElement;
        } else if symbol.declarations().iter().any(|&d| ast::is_let(d)) {
            return ScriptElementKind::LetElement;
        }
        if is_local_variable_or_function(symbol) {
            return ScriptElementKind::LocalVariableElement;
        }
        return ScriptElementKind::VariableElement;
    }
    if flags.intersects(SymbolFlags::FUNCTION) {
        if is_local_variable_or_function(symbol) {
            return ScriptElementKind::LocalFunctionElement;
        }
        return ScriptElementKind::FunctionElement;
    }
    // FIXME: getter and setter use the same symbol. And it is rare to use only setter without getter, so in most cases the symbol always has getter flag.
    // So, even when the location is just on the declaration of setter, this function returns getter.
    if flags.intersects(SymbolFlags::GET_ACCESSOR) {
        return ScriptElementKind::MemberGetAccessorElement;
    }
    if flags.intersects(SymbolFlags::SET_ACCESSOR) {
        return ScriptElementKind::MemberSetAccessorElement;
    }
    if flags.intersects(SymbolFlags::METHOD) {
        return ScriptElementKind::MemberFunctionElement;
    }
    if flags.intersects(SymbolFlags::CONSTRUCTOR) {
        return ScriptElementKind::ConstructorImplementationElement;
    }
    if flags.intersects(SymbolFlags::SIGNATURE) {
        return ScriptElementKind::IndexSignatureElement;
    }

    if flags.intersects(SymbolFlags::PROPERTY) {
        if let Some(c) = type_checker {
            if flags.intersects(SymbolFlags::TRANSIENT)
                && symbol.check_flags().intersects(CheckFlags::SYNTHETIC)
            {
                // If union property is result of union of non method (property/accessors/variables), it is labeled as property
                let mut union_property_kind = ScriptElementKind::Unknown;
                if roots.iter().any(|root_symbol| {
                    root_symbol
                        .flags()
                        .intersects(SymbolFlags::PROPERTY_OR_ACCESSOR | SymbolFlags::VARIABLE)
                }) {
                    union_property_kind = ScriptElementKind::MemberVariableElement;
                }
                if union_property_kind == ScriptElementKind::Unknown {
                    // If this was union of all methods,
                    // make sure it has call signatures before we can label it as method.
                    let type_of_union_property =
                        c.get_type_of_symbol_at_location(symbol, Some(location)).unwrap();
                    if !c.get_call_signatures(type_of_union_property).is_empty() {
                        return ScriptElementKind::MemberFunctionElement;
                    }
                    return ScriptElementKind::MemberVariableElement;
                }
                return union_property_kind;
            }
        }

        return ScriptElementKind::MemberVariableElement;
    }

    ScriptElementKind::Unknown
}

// symbol_display.go:300
fn is_first_declaration_of_symbol_parameter(symbol: P<Symbol>) -> bool {
    let declaration = symbol.declarations().first().copied();
    let result = ast::find_ancestor_or_quit(declaration, |n| {
        if ast::is_parameter_declaration(n) {
            return FindAncestorResult::True;
        }
        if ast::is_binding_element(n)
            || ast::is_object_binding_pattern(n)
            || ast::is_array_binding_pattern(n)
        {
            return FindAncestorResult::False;
        }
        FindAncestorResult::Quit
    });

    result.is_some()
}

// symbol_display.go:318
fn is_local_variable_or_function(symbol: P<Symbol>) -> bool {
    if symbol.parent().is_some() {
        return false; // This is exported symbol
    }

    for &decl in symbol.declarations() {
        // Function expressions are local
        if decl.kind() == Kind::FunctionExpression {
            return true;
        }

        if decl.kind() != Kind::VariableDeclaration && decl.kind() != Kind::FunctionDeclaration {
            continue;
        }

        // If the parent is not source file or module block, it is a local variable.
        let mut parent = decl.parent().unwrap();
        while !ast::is_function_block(parent) {
            // Reached source file or module block
            if parent.kind() == Kind::SourceFile || parent.kind() == Kind::ModuleBlock {
                break;
            }
            parent = parent.parent().unwrap();
        }

        if ast::is_function_block(parent) {
            // Parent is in function block.
            return true;
        }
    }
    false
}

// symbol_display.go:350
pub fn get_symbol_modifiers(
    mut type_checker: Option<&mut Checker>,
    symbol: Option<P<Symbol>>,
) -> ScriptElementKindModifier {
    let Some(symbol) = symbol else {
        return ScriptElementKindModifier::NONE;
    };

    let mut modifiers = get_normalized_symbol_modifiers(type_checker.as_deref_mut(), symbol);
    if symbol.flags().intersects(SymbolFlags::ALIAS) {
        if let Some(c) = type_checker {
            let resolved_symbol = c.get_aliased_symbol(symbol);
            if resolved_symbol != symbol {
                modifiers |= get_normalized_symbol_modifiers(Some(c), resolved_symbol);
            }
        }
    }
    if symbol.flags().intersects(SymbolFlags::OPTIONAL) {
        modifiers |= ScriptElementKindModifier::OPTIONAL;
    }

    modifiers
}

// symbol_display.go:369
fn get_normalized_symbol_modifiers(
    mut type_checker: Option<&mut Checker>,
    symbol: P<Symbol>,
) -> ScriptElementKindModifier {
    let mut modifier_set = ScriptElementKindModifier::NONE;
    if !symbol.declarations().is_empty() {
        let declaration = symbol.declarations()[0];
        let declarations = &symbol.declarations()[1..];
        // omit deprecated flag if some declarations are not deprecated
        let exclude_flags = if !declarations.is_empty()
            && is_deprecated_declaration(type_checker.as_deref_mut(), declaration) // !!! include jsdoc node flags
            && declarations.iter().any(|&d| !is_deprecated_declaration(type_checker.as_deref_mut(), d))
        {
            ModifierFlags::DEPRECATED
        } else {
            ModifierFlags::NONE
        };
        modifier_set = get_node_modifiers(type_checker, declaration, exclude_flags);
    }

    modifier_set
}

// symbol_display.go:390
fn is_deprecated_declaration(type_checker: Option<&mut Checker>, declaration: P<Node>) -> bool {
    if let Some(c) = type_checker {
        return c.is_deprecated_declaration(declaration);
    }
    ast::is_deprecated_declaration(declaration)
}

// symbol_display.go:397
fn get_node_modifiers(
    type_checker: Option<&mut Checker>,
    node: P<Node>,
    exclude_flags: ModifierFlags,
) -> ScriptElementKindModifier {
    let mut result = ScriptElementKindModifier::NONE;
    let mut flags = ModifierFlags::NONE;
    if ast::is_declaration(node) {
        flags = ast::get_combined_modifier_flags(node);
        if is_deprecated_declaration(type_checker, node) {
            flags |= ModifierFlags::DEPRECATED;
        }
        flags &= !exclude_flags;
    }

    if flags.intersects(ModifierFlags::PRIVATE) {
        result |= ScriptElementKindModifier::PRIVATE;
    }
    if flags.intersects(ModifierFlags::PROTECTED) {
        result |= ScriptElementKindModifier::PROTECTED;
    }
    if flags.intersects(ModifierFlags::PUBLIC) {
        result |= ScriptElementKindModifier::PUBLIC;
    }
    if flags.intersects(ModifierFlags::STATIC) {
        result |= ScriptElementKindModifier::STATIC;
    }
    if flags.intersects(ModifierFlags::ABSTRACT) {
        result |= ScriptElementKindModifier::ABSTRACT;
    }
    if flags.intersects(ModifierFlags::EXPORT) {
        result |= ScriptElementKindModifier::EXPORTED;
    }
    if flags.intersects(ModifierFlags::DEPRECATED) {
        result |= ScriptElementKindModifier::DEPRECATED;
    }
    if flags.intersects(ModifierFlags::AMBIENT) {
        result |= ScriptElementKindModifier::AMBIENT;
    }
    if node.flags().intersects(NodeFlags::AMBIENT) {
        result |= ScriptElementKindModifier::AMBIENT;
    }
    if node.kind() == Kind::ExportAssignment {
        result |= ScriptElementKindModifier::EXPORTED;
    }

    result
}
