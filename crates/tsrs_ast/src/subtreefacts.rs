use bitflags::bitflags;
use tsrs_core::P;

use crate::*;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
    pub struct SubtreeFacts: u32 {
        // Facts
        // - Flags used to indicate that a node or subtree contains syntax relevant to a specific transform

        const CONTAINS_TYPE_SCRIPT = 1 << 0;
        const CONTAINS_JSX = 1 << 1;
        const CONTAINS_ES_DECORATORS = 1 << 2;
        const CONTAINS_USING = 1 << 3;
        const CONTAINS_CLASS_STATIC_BLOCKS = 1 << 4;
        const CONTAINS_ES_CLASS_FIELDS = 1 << 5;
        const CONTAINS_LOGICAL_ASSIGNMENTS = 1 << 6;
        const CONTAINS_NULLISH_COALESCING = 1 << 7;
        const CONTAINS_OPTIONAL_CHAINING = 1 << 8;
        const CONTAINS_MISSING_CATCH_CLAUSE_VARIABLE = 1 << 9;
        const CONTAINS_ES_OBJECT_REST_OR_SPREAD = 1 << 10; // subtree has a `...` somewhere inside it, never cleared
        const CONTAINS_FOR_AWAIT_OR_ASYNC_GENERATOR = 1 << 11;
        const CONTAINS_ANY_AWAIT = 1 << 12;
        const CONTAINS_EXPONENTIATION_OPERATOR = 1 << 13;

        // Markers
        // - Flags used to indicate that a node or subtree contains a particular kind of syntax.

        const CONTAINS_LEXICAL_THIS = 1 << 14;
        const CONTAINS_LEXICAL_SUPER = 1 << 15;
        const CONTAINS_REST_OR_SPREAD = 1 << 16; // marker on any `...` - cleared on binding pattern exit
        const CONTAINS_OBJECT_REST_OR_SPREAD = 1 << 17; // marker on any `{...x}` - cleared on most scope exits
        const CONTAINS_AWAIT = 1 << 18;
        const CONTAINS_DYNAMIC_IMPORT = 1 << 19;
        const CONTAINS_CLASS_FIELDS = 1 << 20;
        const CONTAINS_DECORATORS = 1 << 21;
        const CONTAINS_IDENTIFIER = 1 << 22;
        const CONTAINS_PRIVATE_IDENTIFIER_IN_EXPRESSION = 1 << 23;
        const CONTAINS_INVALID_TEMPLATE_ESCAPE = 1 << 24;

        const COMPUTED = 1 << 25; // NOTE: This should always be last
        const NONE = 0;

        // Scope Exclusions
        // - Bitmasks that exclude flags from propagating out of a specific context
        //   into the subtree flags of their container.

        const EXCLUSIONS_NODE = Self::COMPUTED.bits();
        const EXCLUSIONS_ERASEABLE = !Self::CONTAINS_TYPE_SCRIPT.bits();
        const EXCLUSIONS_OUTER_EXPRESSION = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_PROPERTY_ACCESS = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_ELEMENT_ACCESS = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_ARROW_FUNCTION = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_AWAIT.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_FUNCTION = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_LEXICAL_THIS.bits() | Self::CONTAINS_LEXICAL_SUPER.bits() | Self::CONTAINS_AWAIT.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_CONSTRUCTOR = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_LEXICAL_THIS.bits() | Self::CONTAINS_LEXICAL_SUPER.bits() | Self::CONTAINS_AWAIT.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_METHOD = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_LEXICAL_THIS.bits() | Self::CONTAINS_LEXICAL_SUPER.bits() | Self::CONTAINS_AWAIT.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_ACCESSOR = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_LEXICAL_THIS.bits() | Self::CONTAINS_LEXICAL_SUPER.bits() | Self::CONTAINS_AWAIT.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_PROPERTY = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_LEXICAL_THIS.bits() | Self::CONTAINS_LEXICAL_SUPER.bits();
        const EXCLUSIONS_CLASS = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_MODULE = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_LEXICAL_THIS.bits() | Self::CONTAINS_LEXICAL_SUPER.bits();
        const EXCLUSIONS_OBJECT_LITERAL = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_ARRAY_LITERAL = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_CALL = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_NEW = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_VARIABLE_DECLARATION_LIST = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_PARAMETER = Self::EXCLUSIONS_NODE.bits();
        const EXCLUSIONS_CATCH_CLAUSE = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_OBJECT_REST_OR_SPREAD.bits();
        const EXCLUSIONS_BINDING_PATTERN = Self::EXCLUSIONS_NODE.bits() | Self::CONTAINS_REST_OR_SPREAD.bits();

        // Masks
        // - Additional bitmasks

        const CONTAINS_LEXICAL_THIS_OR_SUPER = Self::CONTAINS_LEXICAL_THIS.bits() | Self::CONTAINS_LEXICAL_SUPER.bits();
    }
}

const NONE: SubtreeFacts = SubtreeFacts::NONE;

fn if_else(cond: bool, a: SubtreeFacts, b: SubtreeFacts) -> SubtreeFacts {
    if cond { a } else { b }
}

fn propagate_eraseable_syntax_list_subtree_facts(
    children: impl Into<Option<P<NodeList>>>,
) -> SubtreeFacts {
    if_else(children.into().is_some(), SubtreeFacts::CONTAINS_TYPE_SCRIPT, NONE)
}

fn propagate_eraseable_syntax_subtree_facts(child: impl Into<Option<P<Node>>>) -> SubtreeFacts {
    if_else(child.into().is_some(), SubtreeFacts::CONTAINS_TYPE_SCRIPT, NONE)
}

fn propagate_object_binding_element_subtree_facts(child: P<Node>) -> SubtreeFacts {
    let mut facts = propagate_subtree_facts(child);
    if facts.intersects(SubtreeFacts::CONTAINS_REST_OR_SPREAD) {
        facts &= !SubtreeFacts::CONTAINS_REST_OR_SPREAD;
        facts |= SubtreeFacts::CONTAINS_OBJECT_REST_OR_SPREAD
            | SubtreeFacts::CONTAINS_ES_OBJECT_REST_OR_SPREAD;
    }
    facts
}

fn propagate_binding_element_subtree_facts(child: P<Node>) -> SubtreeFacts {
    propagate_subtree_facts(child) & !SubtreeFacts::CONTAINS_REST_OR_SPREAD
}

fn propagate_subtree_facts(child: impl Into<Option<P<Node>>>) -> SubtreeFacts {
    match child.into() {
        None => NONE,
        Some(child) => child.propagate_subtree_facts(),
    }
}

fn propagate_node_list_subtree_facts(
    children: impl Into<Option<P<NodeList>>>,
    propagate: fn(P<Node>) -> SubtreeFacts,
) -> SubtreeFacts {
    let Some(children) = children.into() else {
        return NONE;
    };
    let mut facts = NONE;
    facts = children.nodes().iter().fold(facts, |mut facts, &child| {
        facts |= propagate(child);

        facts
    });
    facts
}

fn propagate_modifier_list_subtree_facts(children: Option<P<ModifierList>>) -> SubtreeFacts {
    match children {
        None => NONE,
        Some(children) => {
            let mut facts = NONE;
            facts = children.nodes().iter().fold(facts, |mut facts, &child| {
                facts |= propagate_subtree_facts(child);

                facts
            });
            facts
        }
    }
}

fn has_ambient_modifier(modifiers: Option<P<ModifierList>>) -> bool {
    modifiers.is_some_and(|m| m.modifier_flags.intersects(ModifierFlags::AMBIENT))
}

fn prop(child: P<Node>) -> SubtreeFacts {
    propagate_subtree_facts(child)
}

fn template_facts(flags: TokenFlags) -> SubtreeFacts {
    if flags.intersects(TokenFlags::CONTAINS_INVALID_ESCAPE) {
        return SubtreeFacts::CONTAINS_INVALID_TEMPLATE_ESCAPE;
    }
    NONE
}

fn async_generator_facts(is_async: bool, is_generator: bool) -> SubtreeFacts {
    if_else(is_async && is_generator, SubtreeFacts::CONTAINS_FOR_AWAIT_OR_ASYNC_GENERATOR, NONE)
        | if_else(is_async && !is_generator, SubtreeFacts::CONTAINS_ANY_AWAIT, NONE)
}

// Go caches the facts of composite nodes in CompositeBase; computeSubtreeFacts is idempotent, so the port
// recomputes them instead of storing a field in every node (callers only walk trees that are finished). The
// transformers ask at every node they visit, which makes recomputing quadratic in the tree depth, so emit runs them
// under `with_subtree_facts_cache`, a side table on the transforming thread (Go's cache, scoped to one file).
struct SubtreeFactsCache {
    enabled: std::cell::Cell<bool>,
    facts: std::cell::RefCell<rustc_hash::FxHashMap<usize, SubtreeFacts>>, // kept allocated between scopes
}

thread_local! {
    static SUBTREE_FACTS_CACHE: SubtreeFactsCache =
        SubtreeFactsCache { enabled: std::cell::Cell::new(false), facts: std::cell::RefCell::new(rustc_hash::FxHashMap::default()) };
}

/// Runs `f` with `Node::subtree_facts` cached on this thread (nested calls share the outer cache).
pub fn with_subtree_facts_cache<T>(f: impl FnOnce() -> T) -> T {
    if SUBTREE_FACTS_CACHE.with(|c| c.enabled.replace(true)) {
        return f();
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SUBTREE_FACTS_CACHE.with(|c| {
                c.enabled.set(false);
                c.facts.borrow_mut().clear();
            });
        }
    }
    let _reset = Reset;
    f()
}

impl Node {
    pub fn subtree_facts(&self) -> SubtreeFacts {
        SUBTREE_FACTS_CACHE.with(|c| {
            if !c.enabled.get() {
                return self.compute_subtree_facts() & !SubtreeFacts::EXCLUSIONS_NODE;
            }
            let key = std::ptr::from_ref::<Node>(self) as usize;
            if let Some(&facts) = c.facts.borrow().get(&key) {
                return facts;
            }
            // no borrow is held here: computing asks for the children's facts
            let facts = self.compute_subtree_facts() & !SubtreeFacts::EXCLUSIONS_NODE;
            c.facts.borrow_mut().insert(key, facts);
            facts
        })
    }

    fn compute_subtree_facts(&self) -> SubtreeFacts {
        use SubtreeFacts as F;
        match self.data() {
            // TypeSyntaxBase
            NodeData::InterfaceDeclaration(_)
            | NodeData::TypeAliasDeclaration(_)
            | NodeData::NamespaceExportDeclaration(_)
            | NodeData::CallSignatureDeclaration(_)
            | NodeData::ConstructSignatureDeclaration(_)
            | NodeData::IndexSignatureDeclaration(_)
            | NodeData::MethodSignatureDeclaration(_)
            | NodeData::PropertySignatureDeclaration(_)
            | NodeData::KeywordTypeNode
            | NodeData::UnionTypeNode(_)
            | NodeData::IntersectionTypeNode(_)
            | NodeData::ConditionalTypeNode(_)
            | NodeData::TypeOperatorNode(_)
            | NodeData::InferTypeNode(_)
            | NodeData::ArrayTypeNode(_)
            | NodeData::IndexedAccessTypeNode(_)
            | NodeData::TypeReferenceNode(_)
            | NodeData::LiteralTypeNode(_)
            | NodeData::ThisTypeNode
            | NodeData::TypePredicateNode(_)
            | NodeData::TypeQueryNode(_)
            | NodeData::MappedTypeNode(_)
            | NodeData::TypeLiteralNode(_)
            | NodeData::TupleTypeNode(_)
            | NodeData::NamedTupleMember(_)
            | NodeData::OptionalTypeNode(_)
            | NodeData::RestTypeNode(_)
            | NodeData::ParenthesizedTypeNode(_)
            | NodeData::FunctionTypeNode(_)
            | NodeData::ConstructorTypeNode(_)
            | NodeData::TemplateLiteralTypeNode(_)
            | NodeData::TemplateLiteralTypeSpan(_)
            | NodeData::JSDocTypeExpression(_)
            | NodeData::JSDocNonNullableType(_)
            | NodeData::JSDocNullableType(_)
            | NodeData::JSDocAllType
            | NodeData::JSDocVariadicType(_)
            | NodeData::JSDocOptionalType(_)
            | NodeData::JSDocSignature(_)
            | NodeData::JSDocNameReference(_)
            | NodeData::ImportTypeNode(_)
            | NodeData::TypeParameterDeclaration(_)
            | NodeData::JSDocTypeLiteral(_) => F::CONTAINS_TYPE_SCRIPT,

            NodeData::Token => match self.kind() {
                Kind::UsingKeyword => F::CONTAINS_USING,
                Kind::PublicKeyword
                | Kind::PrivateKeyword
                | Kind::ProtectedKeyword
                | Kind::ReadonlyKeyword
                | Kind::AbstractKeyword
                | Kind::DeclareKeyword
                | Kind::ConstKeyword
                | Kind::AnyKeyword
                | Kind::NumberKeyword
                | Kind::BigIntKeyword
                | Kind::NeverKeyword
                | Kind::ObjectKeyword
                | Kind::InKeyword
                | Kind::OutKeyword
                | Kind::OverrideKeyword
                | Kind::StringKeyword
                | Kind::BooleanKeyword
                | Kind::SymbolKeyword
                | Kind::VoidKeyword
                | Kind::UnknownKeyword
                | Kind::UndefinedKeyword
                | Kind::ExportKeyword => F::CONTAINS_TYPE_SCRIPT,
                Kind::AccessorKeyword => F::CONTAINS_CLASS_FIELDS,
                Kind::AsyncKeyword => F::CONTAINS_ANY_AWAIT,
                Kind::SuperKeyword => F::CONTAINS_LEXICAL_SUPER,
                Kind::ThisKeyword => F::CONTAINS_LEXICAL_THIS,
                Kind::AsteriskAsteriskToken | Kind::AsteriskAsteriskEqualsToken => {
                    F::CONTAINS_EXPONENTIATION_OPERATOR
                }
                Kind::QuestionQuestionToken => F::CONTAINS_NULLISH_COALESCING,
                Kind::QuestionDotToken => F::CONTAINS_OPTIONAL_CHAINING,
                Kind::QuestionQuestionEqualsToken
                | Kind::BarBarEqualsToken
                | Kind::AmpersandAmpersandEqualsToken => F::CONTAINS_LOGICAL_ASSIGNMENTS,
                _ => NONE,
            },
            NodeData::PrivateIdentifier(_) => F::CONTAINS_CLASS_FIELDS,
            NodeData::Decorator(d) => {
                prop(d.expression()) | F::CONTAINS_TYPE_SCRIPT | F::CONTAINS_DECORATORS
            }
            NodeData::ForInOrOfStatement(d) => {
                prop(d.initializer())
                    | prop(d.expression())
                    | prop(d.statement())
                    | if_else(
                        d.await_modifier().is_some(),
                        F::CONTAINS_FOR_AWAIT_OR_ASYNC_GENERATOR,
                        NONE,
                    )
            }
            // return in an ES2018 async generator must be awaited
            NodeData::ReturnStatement(d) => {
                propagate_subtree_facts(d.expression()) | F::CONTAINS_FOR_AWAIT_OR_ASYNC_GENERATOR
            }
            NodeData::CatchClause(d) => {
                let mut res = propagate_subtree_facts(d.variable_declaration()) | prop(d.block());
                if d.variable_declaration().is_none() {
                    res |= F::CONTAINS_MISSING_CATCH_CLAUSE_VARIABLE;
                }
                res
            }
            NodeData::VariableStatement(d) => {
                if has_ambient_modifier(d.modifiers()) {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | prop(d.declaration_list())
                }
            }
            NodeData::VariableDeclaration(d) => {
                propagate_subtree_facts(d.name())
                    | propagate_eraseable_syntax_subtree_facts(d.exclamation_token())
                    | propagate_eraseable_syntax_subtree_facts(d.type_())
                    | propagate_subtree_facts(d.initializer())
            }
            NodeData::VariableDeclarationList(d) => {
                propagate_node_list_subtree_facts(d.declarations(), propagate_subtree_facts)
                    | if_else(self.flags().intersects(NodeFlags::USING), F::CONTAINS_USING, NONE)
            }
            NodeData::BindingPattern(d) => match self.kind() {
                Kind::ObjectBindingPattern => propagate_node_list_subtree_facts(
                    d.elements(),
                    propagate_object_binding_element_subtree_facts,
                ),
                Kind::ArrayBindingPattern => propagate_node_list_subtree_facts(
                    d.elements(),
                    propagate_binding_element_subtree_facts,
                ),
                _ => NONE,
            },
            NodeData::ParameterDeclaration(d) => {
                if is_this_identifier(d.name()) {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | propagate_subtree_facts(d.name())
                        | propagate_eraseable_syntax_subtree_facts(d.question_token())
                        | propagate_eraseable_syntax_subtree_facts(d.type_())
                        | propagate_subtree_facts(d.initializer())
                }
            }
            NodeData::BindingElement(d) => {
                propagate_subtree_facts(d.property_name())
                    | propagate_subtree_facts(d.name())
                    | propagate_subtree_facts(d.initializer())
                    | if_else(d.dot_dot_dot_token().is_some(), F::CONTAINS_REST_OR_SPREAD, NONE)
            }
            NodeData::FunctionDeclaration(d) => {
                if d.body().is_none() || self.modifier_flags().intersects(ModifierFlags::AMBIENT) {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    let is_async = self.modifier_flags().intersects(ModifierFlags::ASYNC);
                    let is_generator = d.asterisk_token().is_some();
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | propagate_subtree_facts(d.asterisk_token())
                        | propagate_subtree_facts(d.name())
                        | propagate_eraseable_syntax_list_subtree_facts(d.type_parameters())
                        | propagate_node_list_subtree_facts(d.parameters(), propagate_subtree_facts)
                        | propagate_eraseable_syntax_subtree_facts(d.type_())
                        | propagate_eraseable_syntax_subtree_facts(d.full_signature())
                        | propagate_subtree_facts(d.body())
                        | async_generator_facts(is_async, is_generator)
                }
            }
            NodeData::ClassDeclaration(_) | NodeData::ClassExpression(_) => {
                let class = self.class_like_data().unwrap();
                if has_ambient_modifier(class.modifiers()) {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_modifier_list_subtree_facts(class.modifiers())
                        | propagate_subtree_facts(class.name())
                        | propagate_eraseable_syntax_list_subtree_facts(class.type_parameters())
                        | propagate_node_list_subtree_facts(
                            class.heritage_clauses(),
                            propagate_subtree_facts,
                        )
                        | propagate_node_list_subtree_facts(
                            class.members(),
                            propagate_subtree_facts,
                        )
                }
            }
            NodeData::HeritageClause(d) => match d.token {
                Kind::ExtendsKeyword => {
                    propagate_node_list_subtree_facts(d.types(), propagate_subtree_facts)
                }
                Kind::ImplementsKeyword => F::CONTAINS_TYPE_SCRIPT,
                _ => NONE,
            },
            NodeData::EnumMember(d) => {
                prop(d.name()) | propagate_subtree_facts(d.initializer()) | F::CONTAINS_TYPE_SCRIPT
            }
            NodeData::EnumDeclaration(d) => {
                if has_ambient_modifier(d.modifiers()) {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | prop(d.name())
                        | propagate_node_list_subtree_facts(d.members(), propagate_subtree_facts)
                        | F::CONTAINS_TYPE_SCRIPT
                }
            }
            NodeData::ModuleDeclaration(d) => {
                if self.modifier_flags().intersects(ModifierFlags::AMBIENT) {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | prop(d.name())
                        | propagate_subtree_facts(d.body())
                        | F::CONTAINS_TYPE_SCRIPT
                }
            }
            NodeData::ImportEqualsDeclaration(d) => {
                if d.is_type_only || !is_external_module_reference(d.module_reference()) {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | prop(d.name())
                        | prop(d.module_reference())
                }
            }
            NodeData::ImportSpecifier(d) => {
                if d.is_type_only {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_subtree_facts(d.property_name()) | prop(d.name())
                }
            }
            NodeData::ImportClause(d) => {
                if d.phase_modifier() == Kind::TypeKeyword {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_subtree_facts(d.name()) | propagate_subtree_facts(d.named_bindings())
                }
            }
            NodeData::ExportAssignment(d) => {
                propagate_modifier_list_subtree_facts(d.modifiers())
                    | propagate_subtree_facts(d.type_())
                    | prop(d.expression())
                    | if_else(d.is_export_equals, F::CONTAINS_TYPE_SCRIPT, NONE)
            }
            NodeData::ExportDeclaration(d) => {
                propagate_modifier_list_subtree_facts(d.modifiers())
                    | propagate_subtree_facts(d.export_clause())
                    | propagate_subtree_facts(d.module_specifier())
                    | propagate_subtree_facts(d.attributes())
                    | if_else(d.is_type_only, F::CONTAINS_TYPE_SCRIPT, NONE)
            }
            NodeData::ExportSpecifier(d) => {
                if d.is_type_only {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_subtree_facts(d.property_name()) | prop(d.name())
                }
            }
            NodeData::ConstructorDeclaration(d) => {
                if d.body().is_none() {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | propagate_eraseable_syntax_list_subtree_facts(d.type_parameters())
                        | propagate_node_list_subtree_facts(d.parameters(), propagate_subtree_facts)
                        | propagate_eraseable_syntax_subtree_facts(d.type_())
                        | propagate_eraseable_syntax_subtree_facts(d.full_signature())
                        | propagate_subtree_facts(d.body())
                }
            }
            NodeData::GetAccessorDeclaration(_) | NodeData::SetAccessorDeclaration(_) => {
                if self.body().is_none() {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    let f = self.function_like_data().unwrap();
                    propagate_modifier_list_subtree_facts(self.modifiers())
                        | propagate_subtree_facts(self.name())
                        | propagate_eraseable_syntax_list_subtree_facts(f.type_parameters())
                        | propagate_node_list_subtree_facts(f.parameters(), propagate_subtree_facts)
                        | propagate_eraseable_syntax_subtree_facts(f.type_())
                        | propagate_eraseable_syntax_subtree_facts(f.full_signature())
                        | propagate_subtree_facts(self.body())
                }
            }
            NodeData::MethodDeclaration(d) => {
                if d.body().is_none() {
                    F::CONTAINS_TYPE_SCRIPT
                } else {
                    let is_async = d
                        .modifiers()
                        .is_some_and(|m| m.modifier_flags.intersects(ModifierFlags::ASYNC));
                    let is_generator = d.asterisk_token().is_some();
                    propagate_modifier_list_subtree_facts(d.modifiers())
                        | propagate_subtree_facts(d.asterisk_token())
                        | prop(d.name())
                        | propagate_eraseable_syntax_subtree_facts(d.postfix_token())
                        | propagate_eraseable_syntax_list_subtree_facts(d.type_parameters())
                        | propagate_node_list_subtree_facts(d.parameters(), propagate_subtree_facts)
                        | propagate_subtree_facts(d.body())
                        | propagate_eraseable_syntax_subtree_facts(d.type_())
                        | propagate_eraseable_syntax_subtree_facts(d.full_signature())
                        | async_generator_facts(is_async, is_generator)
                }
            }
            NodeData::PropertyDeclaration(d) => {
                propagate_modifier_list_subtree_facts(d.modifiers())
                    | prop(d.name())
                    | propagate_eraseable_syntax_subtree_facts(d.postfix_token())
                    | propagate_eraseable_syntax_subtree_facts(d.type_())
                    | propagate_subtree_facts(d.initializer())
                    | F::CONTAINS_CLASS_FIELDS
            }
            NodeData::ClassStaticBlockDeclaration(d) => {
                propagate_modifier_list_subtree_facts(d.modifiers())
                    | prop(d.body())
                    | F::CONTAINS_CLASS_FIELDS
            }
            NodeData::KeywordExpression(_) => match self.kind() {
                Kind::ThisKeyword => F::CONTAINS_LEXICAL_THIS,
                Kind::SuperKeyword => F::CONTAINS_LEXICAL_SUPER,
                _ => NONE,
            },
            NodeData::BigIntLiteral(_) => NONE, // `bigint` is not downleveled in any way
            NodeData::Identifier(_) => F::CONTAINS_IDENTIFIER,
            NodeData::NoSubstitutionTemplateLiteral(d) => template_facts(d.template_flags()),
            NodeData::BinaryExpression(d) => {
                let mut facts = propagate_modifier_list_subtree_facts(d.modifiers())
                    | prop(d.left())
                    | propagate_subtree_facts(d.type_())
                    | prop(d.operator_token())
                    | prop(d.right())
                    | if_else(
                        d.operator_token().kind() == Kind::InKeyword
                            && is_private_identifier(d.left()),
                        F::CONTAINS_CLASS_FIELDS | F::CONTAINS_PRIVATE_IDENTIFIER_IN_EXPRESSION,
                        NONE,
                    );
                if d.operator_token().kind() == Kind::EqualsToken
                    && (is_object_literal_expression(d.left())
                        || is_array_literal_expression(d.left()))
                    && contains_object_rest_or_spread(d.left())
                {
                    facts |= F::CONTAINS_OBJECT_REST_OR_SPREAD;
                }
                facts
            }
            NodeData::YieldExpression(d) => {
                propagate_subtree_facts(d.expression()) | F::CONTAINS_FOR_AWAIT_OR_ASYNC_GENERATOR
            }
            NodeData::ArrowFunction(d) => {
                propagate_modifier_list_subtree_facts(d.modifiers())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_parameters())
                    | propagate_node_list_subtree_facts(d.parameters(), propagate_subtree_facts)
                    | propagate_eraseable_syntax_subtree_facts(d.type_())
                    | propagate_eraseable_syntax_subtree_facts(d.full_signature())
                    | propagate_subtree_facts(d.body())
                    | if_else(
                        self.modifier_flags().intersects(ModifierFlags::ASYNC),
                        F::CONTAINS_ANY_AWAIT,
                        NONE,
                    )
            }
            NodeData::FunctionExpression(d) => {
                let is_async = d
                    .modifiers()
                    .is_some_and(|m| m.modifier_flags.intersects(ModifierFlags::ASYNC));
                let is_generator = d.asterisk_token().is_some();
                propagate_modifier_list_subtree_facts(d.modifiers())
                    | propagate_subtree_facts(d.asterisk_token())
                    | propagate_subtree_facts(d.name())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_parameters())
                    | propagate_node_list_subtree_facts(d.parameters(), propagate_subtree_facts)
                    | propagate_eraseable_syntax_subtree_facts(d.type_())
                    | propagate_eraseable_syntax_subtree_facts(d.full_signature())
                    | propagate_subtree_facts(d.body())
                    | async_generator_facts(is_async, is_generator)
            }
            NodeData::AsExpression(d) => prop(d.expression()) | F::CONTAINS_TYPE_SCRIPT,
            NodeData::SatisfiesExpression(d) => prop(d.expression()) | F::CONTAINS_TYPE_SCRIPT,
            NodeData::PropertyAccessExpression(d) => {
                let private_name = if_else(
                    !is_identifier(d.name()),
                    F::CONTAINS_PRIVATE_IDENTIFIER_IN_EXPRESSION,
                    NONE,
                );
                prop(d.expression())
                    | propagate_subtree_facts(d.question_dot_token())
                    | prop(d.name())
                    | private_name
            }
            NodeData::CallExpression(d) => {
                prop(d.expression())
                    | propagate_subtree_facts(d.question_dot_token())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_arguments())
                    | propagate_node_list_subtree_facts(d.arguments(), propagate_subtree_facts)
                    | if_else(
                        d.expression().kind() == Kind::ImportKeyword
                            || is_import_phase_meta_property(d.expression()),
                        F::CONTAINS_DYNAMIC_IMPORT,
                        NONE,
                    )
            }
            NodeData::NewExpression(d) => {
                prop(d.expression())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_arguments())
                    | propagate_node_list_subtree_facts(d.arguments(), propagate_subtree_facts)
            }
            NodeData::MetaProperty(d) => prop(d.name()) & !F::CONTAINS_IDENTIFIER,
            NodeData::NonNullExpression(d) => prop(d.expression()) | F::CONTAINS_TYPE_SCRIPT,
            NodeData::SpreadElement(d) => prop(d.expression()) | F::CONTAINS_REST_OR_SPREAD,
            NodeData::TaggedTemplateExpression(d) => {
                prop(d.tag())
                    | propagate_subtree_facts(d.question_dot_token())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_arguments())
                    | prop(d.template())
            }
            NodeData::SpreadAssignment(d) => {
                prop(d.expression())
                    | F::CONTAINS_ES_OBJECT_REST_OR_SPREAD
                    | F::CONTAINS_OBJECT_REST_OR_SPREAD
            }
            NodeData::PropertyAssignment(d) => {
                prop(d.name()) | propagate_subtree_facts(d.type_()) | prop(d.initializer())
            }
            NodeData::ShorthandPropertyAssignment(d) => {
                prop(d.name())
                    | propagate_subtree_facts(d.type_())
                    | propagate_subtree_facts(d.object_assignment_initializer())
                    | F::CONTAINS_TYPE_SCRIPT
            }
            // await in an ES2018 async generator must use `yield __await(expr)`
            NodeData::AwaitExpression(d) => {
                prop(d.expression())
                    | F::CONTAINS_AWAIT
                    | F::CONTAINS_ANY_AWAIT
                    | F::CONTAINS_FOR_AWAIT_OR_ASYNC_GENERATOR
            }
            NodeData::TypeAssertion(d) => prop(d.expression()) | F::CONTAINS_TYPE_SCRIPT,
            NodeData::ExpressionWithTypeArguments(d) => {
                prop(d.expression())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_arguments())
            }
            NodeData::TemplateHead(d) => template_facts(d.template_flags()),
            NodeData::TemplateMiddle(d) => template_facts(d.template_flags()),
            NodeData::TemplateTail(d) => template_facts(d.template_flags()),
            NodeData::JsxElement(d) => {
                prop(d.opening_element())
                    | propagate_node_list_subtree_facts(d.children(), propagate_subtree_facts)
                    | prop(d.closing_element())
                    | F::CONTAINS_JSX
            }
            NodeData::JsxAttributes(d) => {
                propagate_node_list_subtree_facts(d.properties(), propagate_subtree_facts)
                    | F::CONTAINS_JSX
            }
            NodeData::JsxNamespacedName(d) => {
                prop(d.namespace()) | prop(d.name()) | F::CONTAINS_JSX
            }
            NodeData::JsxOpeningElement(d) => {
                prop(d.tag_name())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_arguments())
                    | prop(d.attributes())
                    | F::CONTAINS_JSX
            }
            NodeData::JsxSelfClosingElement(d) => {
                prop(d.tag_name())
                    | propagate_eraseable_syntax_list_subtree_facts(d.type_arguments())
                    | prop(d.attributes())
                    | F::CONTAINS_JSX
            }
            NodeData::JsxFragment(d) => {
                propagate_node_list_subtree_facts(d.children(), propagate_subtree_facts)
                    | F::CONTAINS_JSX
            }
            NodeData::JsxOpeningFragment => F::CONTAINS_JSX,
            NodeData::JsxClosingFragment => F::CONTAINS_JSX,
            NodeData::JsxAttribute(d) => {
                prop(d.name()) | propagate_subtree_facts(d.initializer()) | F::CONTAINS_JSX
            }
            NodeData::JsxSpreadAttribute(d) => prop(d.expression()) | F::CONTAINS_JSX,
            NodeData::JsxClosingElement(d) => prop(d.tag_name()) | F::CONTAINS_JSX,
            NodeData::JsxExpression(d) => propagate_subtree_facts(d.expression()) | F::CONTAINS_JSX,
            NodeData::JsxText(_) => F::CONTAINS_JSX,
            NodeData::SourceFile(d) => {
                propagate_node_list_subtree_facts(d.statements, propagate_subtree_facts)
            }

            // Generated (ast_generated.go)
            NodeData::QualifiedName(d) => prop(d.left()) | prop(d.right()),
            NodeData::ComputedPropertyName(d) => prop(d.expression()),
            NodeData::IfStatement(d) => {
                prop(d.expression())
                    | prop(d.then_statement())
                    | propagate_subtree_facts(d.else_statement())
            }
            NodeData::DoStatement(d) => prop(d.statement()) | prop(d.expression()),
            NodeData::WhileStatement(d) => prop(d.expression()) | prop(d.statement()),
            NodeData::ForStatement(d) => {
                propagate_subtree_facts(d.initializer())
                    | propagate_subtree_facts(d.condition())
                    | propagate_subtree_facts(d.incrementor())
                    | prop(d.statement())
            }
            NodeData::WithStatement(d) => prop(d.expression()) | prop(d.statement()),
            NodeData::SwitchStatement(d) => prop(d.expression()) | prop(d.case_block()),
            NodeData::CaseBlock(d) => {
                propagate_node_list_subtree_facts(d.clauses(), propagate_subtree_facts)
            }
            NodeData::CaseOrDefaultClause(d) => {
                propagate_subtree_facts(d.expression())
                    | propagate_node_list_subtree_facts(d.statements(), propagate_subtree_facts)
            }
            NodeData::ThrowStatement(d) => prop(d.expression()),
            NodeData::TryStatement(d) => {
                prop(d.try_block())
                    | propagate_subtree_facts(d.catch_clause())
                    | propagate_subtree_facts(d.finally_block())
            }
            NodeData::LabeledStatement(d) => prop(d.label()) | prop(d.statement()),
            NodeData::ExpressionStatement(d) => prop(d.expression()),
            NodeData::Block(d) => {
                propagate_node_list_subtree_facts(d.statements(), propagate_subtree_facts)
            }
            NodeData::ModuleBlock(d) => {
                propagate_node_list_subtree_facts(d.statements(), propagate_subtree_facts)
            }
            NodeData::ImportDeclaration(d) => {
                propagate_modifier_list_subtree_facts(d.modifiers())
                    | propagate_subtree_facts(d.import_clause())
                    | prop(d.module_specifier())
                    | propagate_subtree_facts(d.attributes())
            }
            NodeData::ExternalModuleReference(d) => prop(d.expression()),
            NodeData::NamespaceImport(d) => prop(d.name()),
            NodeData::NamedImports(d) => {
                propagate_node_list_subtree_facts(d.elements(), propagate_subtree_facts)
            }
            NodeData::NamespaceExport(d) => prop(d.name()),
            NodeData::NamedExports(d) => {
                propagate_node_list_subtree_facts(d.elements(), propagate_subtree_facts)
            }
            NodeData::PrefixUnaryExpression(d) => prop(d.operand()),
            NodeData::PostfixUnaryExpression(d) => prop(d.operand()),
            NodeData::ConditionalExpression(d) => {
                prop(d.condition())
                    | prop(d.question_token())
                    | prop(d.when_true())
                    | prop(d.colon_token())
                    | prop(d.when_false())
            }
            NodeData::ElementAccessExpression(d) => {
                prop(d.expression())
                    | propagate_subtree_facts(d.question_dot_token())
                    | prop(d.argument_expression())
            }
            NodeData::TemplateExpression(d) => {
                prop(d.head())
                    | propagate_node_list_subtree_facts(d.template_spans(), propagate_subtree_facts)
            }
            NodeData::TemplateSpan(d) => prop(d.expression()) | prop(d.literal()),
            NodeData::ParenthesizedExpression(d) => prop(d.expression()),
            NodeData::ArrayLiteralExpression(d) => {
                propagate_node_list_subtree_facts(d.elements(), propagate_subtree_facts)
            }
            NodeData::ObjectLiteralExpression(d) => {
                propagate_node_list_subtree_facts(d.properties(), propagate_subtree_facts)
            }
            NodeData::DeleteExpression(d) => prop(d.expression()),
            NodeData::TypeOfExpression(d) => prop(d.expression()),
            NodeData::VoidExpression(d) => prop(d.expression()),
            NodeData::ImportAttribute(d) => propagate_subtree_facts(d.name()) | prop(d.value()),
            NodeData::ImportAttributes(d) => {
                propagate_node_list_subtree_facts(d.attributes(), propagate_subtree_facts)
            }
            NodeData::PartiallyEmittedExpression(d) => prop(d.expression()),
            NodeData::SyntheticReferenceExpression(d) => prop(d.expression()) | prop(d.this_arg()),

            _ => NONE,
        }
    }

    fn propagate_subtree_facts(&self) -> SubtreeFacts {
        use SubtreeFacts as F;
        match self.data() {
            // TypeSyntaxBase.propagateSubtreeFacts
            NodeData::InterfaceDeclaration(_)
            | NodeData::TypeAliasDeclaration(_)
            | NodeData::NamespaceExportDeclaration(_)
            | NodeData::CallSignatureDeclaration(_)
            | NodeData::ConstructSignatureDeclaration(_)
            | NodeData::IndexSignatureDeclaration(_)
            | NodeData::MethodSignatureDeclaration(_)
            | NodeData::PropertySignatureDeclaration(_)
            | NodeData::KeywordTypeNode
            | NodeData::UnionTypeNode(_)
            | NodeData::IntersectionTypeNode(_)
            | NodeData::ConditionalTypeNode(_)
            | NodeData::TypeOperatorNode(_)
            | NodeData::InferTypeNode(_)
            | NodeData::ArrayTypeNode(_)
            | NodeData::IndexedAccessTypeNode(_)
            | NodeData::TypeReferenceNode(_)
            | NodeData::LiteralTypeNode(_)
            | NodeData::ThisTypeNode
            | NodeData::TypePredicateNode(_)
            | NodeData::TypeQueryNode(_)
            | NodeData::MappedTypeNode(_)
            | NodeData::TypeLiteralNode(_)
            | NodeData::TupleTypeNode(_)
            | NodeData::NamedTupleMember(_)
            | NodeData::OptionalTypeNode(_)
            | NodeData::RestTypeNode(_)
            | NodeData::ParenthesizedTypeNode(_)
            | NodeData::FunctionTypeNode(_)
            | NodeData::ConstructorTypeNode(_)
            | NodeData::TemplateLiteralTypeNode(_)
            | NodeData::TemplateLiteralTypeSpan(_)
            | NodeData::JSDocTypeExpression(_)
            | NodeData::JSDocNonNullableType(_)
            | NodeData::JSDocNullableType(_)
            | NodeData::JSDocAllType
            | NodeData::JSDocVariadicType(_)
            | NodeData::JSDocOptionalType(_)
            | NodeData::JSDocSignature(_)
            | NodeData::JSDocNameReference(_)
            | NodeData::ImportTypeNode(_)
            | NodeData::TypeParameterDeclaration(_)
            | NodeData::JSDocTypeLiteral(_) => F::CONTAINS_TYPE_SCRIPT,
            NodeData::CatchClause(_) => self.subtree_facts() & !F::EXCLUSIONS_CATCH_CLAUSE,
            NodeData::VariableDeclarationList(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_VARIABLE_DECLARATION_LIST
            }
            NodeData::BindingPattern(_) => self.subtree_facts() & !F::EXCLUSIONS_BINDING_PATTERN,
            NodeData::ParameterDeclaration(_) => self.subtree_facts() & !F::EXCLUSIONS_PARAMETER,
            NodeData::FunctionDeclaration(_) => self.subtree_facts() & !F::EXCLUSIONS_FUNCTION,
            NodeData::ClassDeclaration(_) | NodeData::ClassExpression(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_CLASS
            }
            NodeData::ModuleDeclaration(_) => self.subtree_facts() & !F::EXCLUSIONS_MODULE,
            NodeData::ConstructorDeclaration(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_CONSTRUCTOR
            }
            NodeData::GetAccessorDeclaration(_) | NodeData::SetAccessorDeclaration(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_ACCESSOR
                    | propagate_subtree_facts(self.name())
            }
            NodeData::MethodDeclaration(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_METHOD | propagate_subtree_facts(self.name())
            }
            NodeData::PropertyDeclaration(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_PROPERTY
                    | propagate_subtree_facts(self.name())
            }
            NodeData::ArrowFunction(_) => self.subtree_facts() & !F::EXCLUSIONS_ARROW_FUNCTION,
            NodeData::FunctionExpression(_) => self.subtree_facts() & !F::EXCLUSIONS_FUNCTION,
            NodeData::AsExpression(_)
            | NodeData::SatisfiesExpression(_)
            | NodeData::TypeAssertion(_) => self.subtree_facts() & !F::EXCLUSIONS_OUTER_EXPRESSION,
            NodeData::PropertyAccessExpression(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_PROPERTY_ACCESS
            }
            NodeData::ElementAccessExpression(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_ELEMENT_ACCESS
            }
            NodeData::CallExpression(_) => self.subtree_facts() & !F::EXCLUSIONS_CALL,
            NodeData::NewExpression(_) => self.subtree_facts() & !F::EXCLUSIONS_NEW,
            NodeData::ArrayLiteralExpression(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_ARRAY_LITERAL
            }
            NodeData::ObjectLiteralExpression(_) => {
                self.subtree_facts() & !F::EXCLUSIONS_OBJECT_LITERAL
            }
            _ => self.subtree_facts() & !F::EXCLUSIONS_NODE,
        }
    }
}

pub fn contains_object_rest_or_spread(node: P<Node>) -> bool {
    if node.subtree_facts().intersects(SubtreeFacts::CONTAINS_OBJECT_REST_OR_SPREAD) {
        return true;
    }
    if node.subtree_facts().intersects(SubtreeFacts::CONTAINS_ES_OBJECT_REST_OR_SPREAD) {
        // check for nested spread assignments, otherwise '{ x: { a, ...b } = foo } = c'
        // will not be correctly interpreted by the rest/spread transformer
        if get_elements_of_binding_or_assignment_pattern(node).iter().any(|&element| {
            let target = get_target_of_binding_or_assignment_element(element);
            if let Some(target) = target {
                if is_assignment_pattern(target) {
                    if target
                        .subtree_facts()
                        .intersects(SubtreeFacts::CONTAINS_OBJECT_REST_OR_SPREAD)
                    {
                        return true;
                    }
                    if target
                        .subtree_facts()
                        .intersects(SubtreeFacts::CONTAINS_ES_OBJECT_REST_OR_SPREAD)
                        && contains_object_rest_or_spread(target)
                    {
                        return true;
                    }
                }
            }

            false
        }) {
            return true;
        }
    }
    false
}
