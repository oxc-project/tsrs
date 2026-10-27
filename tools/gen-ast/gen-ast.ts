// Rust AST code generator: reads ts-ref/tools/scripts/tsc/ast.json (via a copy of the Go generator's
// schema.ts) and writes crates/tsrs_ast/src/generated.rs, the Rust counterpart of ast_generated.go.
//
// Usage: node tools/gen-ast/gen-ast.ts
//
// Emits node data structs, base structs, the NodeData enum and its NodeDataTag (the data struct of a node is
// co-allocated right after its header, see `NodeAlloc` in ast.rs), NodeFactory::new_*/update_* constructors,
// Node::as_* casts, is_* predicates, for_each_child/visit_each_child/clone_node, per-struct field getters,
// the Node-level base accessors (declaration_data(), name(), modifiers(), ...) and the kind alias guards.
// Subtree facts, the encoder and TS output are intentionally not generated.

import * as fs from "node:fs";
import * as path from "node:path";
import type { MemberInfo, NodeType } from "./schema.ts";
import { api, kindGuardName } from "./schema.ts";
import { screamingSnakeCase, rustSurface } from "../lib/rust-names.mjs";

const root = path.resolve(import.meta.dirname, "../..");
const outPath = path.join(root, "crates/tsrs_ast/src/generated.rs");

// Rust implementation surface; schema fields and numeric values remain unchanged.
const SURFACE = [
    ["","field","ArrowFunction","body_base declaration_base flow_node_base modifiers_base"],
    ["","field","BigIntLiteral","literal_like_node_base"],
    ["","field","BinaryExpression","declaration_base modifiers_base"],
    ["","field","BindingElement","declaration_base exportable_base flow_node_base"],
    ["","field","BindingElementRare","dot_dot_dot_token initializer property_name"],
    ["","field","Block","flow_node_base locals_container_base"],
    ["","field","BreakStatement","flow_node_base"],
    ["","field","CallExpression","declaration_base"],
    ["","field","CallExpressionRare","question_dot_token type_arguments"],
    ["","field","CallSignatureDeclaration","declaration_base function_like_base"],
    ["","field","CaseBlock","locals_container_base"],
    ["","field","CatchClause","locals_container_base"],
    ["","field","ClassDeclaration","declaration_base flow_node_base"],
    ["","field","ClassExpression","declaration_base"],
    ["","field","ClassLikeBase","exportable_base locals_container_base modifiers_base name"],
    ["","field","ClassStaticBlockDeclaration","declaration_base locals_container_base modifiers_base"],
    ["","field","ConditionalExpression","colon_token"],
    ["","field","ConditionalTypeNode","locals_container_base"],
    ["","field","ConstructorDeclaration","body_base declaration_base modifiers_base"],
    ["","field","ConstructorTypeNode","declaration_base function_like_base modifiers_base"],
    ["","field","ConstructSignatureDeclaration","declaration_base function_like_base"],
    ["","field","ContinueStatement","flow_node_base"],
    ["","field","DebuggerStatement","flow_node_base"],
    ["","field","DoStatement","iteration_statement_base"],
    ["","field","ElementAccessExpression","flow_node_base"],
    ["","field","ElementAccessExpressionRare","question_dot_token"],
    ["","field","EmptyStatement","flow_node_base"],
    ["","field","EnumDeclaration","declaration_base exportable_base flow_node_base modifiers_base"],
    ["","field","EnumMember","declaration_base named_member_base"],
    ["","field","ExportAssignment","declaration_base flow_node_base modifiers_base"],
    ["","field","ExportDeclaration","declaration_base flow_node_base modifiers_base"],
    ["","field","ExportSpecifier","declaration_base exportable_base"],
    ["","field","ExpressionStatement","flow_node_base"],
    ["","field","ForInOrOfStatement","flow_node_base locals_container_base"],
    ["","field","ForStatement","iteration_statement_base locals_container_base"],
    ["","field","FunctionDeclaration","declaration_base exportable_base flow_node_base function_like_base modifiers_base name return_flow_node"],
    ["","field","FunctionExpression","declaration_base flow_node_base function_like_base modifiers_base name return_flow_node"],
    ["","field","FunctionLikeBase","locals_container_base"],
    ["","field","FunctionTypeNode","declaration_base function_like_base modifiers_base"],
    ["","field","GetAccessorDeclaration","body_base declaration_base flow_node_base function_like_base named_member_base"],
    ["","field","IfStatement","flow_node_base"],
    ["","field","ImportAttributes","multi_line"],
    ["","field","ImportClause","declaration_base exportable_base"],
    ["","field","ImportDeclaration","declaration_base flow_node_base modifiers_base"],
    ["","field","ImportDeclarationRare","attributes"],
    ["","field","ImportEqualsDeclaration","declaration_base exportable_base flow_node_base modifiers_base"],
    ["","field","ImportSpecifier","declaration_base exportable_base name"],
    ["","field","ImportSpecifierRare","property_name"],
    ["","field","ImportTypeNode","node_with_type_arguments_base"],
    ["","field","IndexSignatureDeclaration","declaration_base modifiers_base"],
    ["","field","InterfaceDeclaration","declaration_base exportable_base flow_node_base modifiers_base"],
    ["","field","IterationStatementBase","flow_node_base statement"],
    ["","field","JSDocCallbackTag","name"],
    ["","field","JSDocCommentBase","text"],
    ["","field","JSDocLink","jsdoc_comment_base name"],
    ["","field","JSDocLinkCode","jsdoc_comment_base name"],
    ["","field","JSDocLinkPlain","jsdoc_comment_base name"],
    ["","field","JSDocNameReference","name"],
    ["","field","JSDocSignature","declaration_base"],
    ["","field","JSDocText","jsdoc_comment_base"],
    ["","field","JSDocTypedefTag","name"],
    ["","field","JSDocTypeLiteral","declaration_base"],
    ["","field","JsxAttribute","declaration_base"],
    ["","field","JsxAttributes","declaration_base"],
    ["","field","JsxSpreadAttribute","declaration_base"],
    ["","field","JsxText","literal_like_node_base"],
    ["","field","KeywordExpression","flow_node_base"],
    ["","field","LabeledStatement","flow_node_base"],
    ["","field","MappedTypeNode","declaration_base locals_container_base"],
    ["","field","MetaProperty","flow_node_base"],
    ["","field","MethodDeclaration","declaration_base flow_node_base function_like_base"],
    ["","field","MethodSignatureDeclaration","declaration_base function_like_base named_member_base"],
    ["","field","MissingDeclaration","declaration_base flow_node_base modifiers_base"],
    ["","field","ModifiersBase","modifiers"],
    ["","field","ModuleBlock","flow_node_base"],
    ["","field","ModuleDeclaration","body_base declaration_base exportable_base flow_node_base locals_container_base modifiers_base name"],
    ["","field","NamedMemberBase","modifiers_base name"],
    ["","field","NamedTupleMember","declaration_base name"],
    ["","field","NamespaceExport","declaration_base name"],
    ["","field","NamespaceExportDeclaration","declaration_base flow_node_base modifiers_base name"],
    ["","field","NamespaceImport","declaration_base exportable_base name"],
    ["","field","NewExpressionRare","type_arguments"],
    ["","field","NodeWithTypeArgumentsBase","type_arguments"],
    ["","field","NoSubstitutionTemplateLiteral","declaration_base template_literal_like_node_base"],
    ["","field","NotEmittedStatement","flow_node_base"],
    ["","field","NotEmittedTypeElement","declaration_base"],
    ["","field","ObjectLiteralExpression","declaration_base"],
    ["","field","ParameterDeclaration","declaration_base modifiers_base"],
    ["","field","ParameterDeclarationRare","dot_dot_dot_token initializer"],
    ["","field","PrivateIdentifier","text"],
    ["","field","PropertyAccessExpression","flow_node_base"],
    ["","field","PropertyAccessExpressionRare","question_dot_token"],
    ["","field","PropertyAssignment","declaration_base"],
    ["","field","PropertyDeclaration","declaration_base"],
    ["","field","PropertySignatureDeclaration","named_member_base"],
    ["","field","PropertySignatureDeclarationRare","initializer"],
    ["","field","QualifiedName","flow_node_base"],
    ["","field","RegularExpressionLiteral","literal_like_node_base"],
    ["","field","ReturnStatement","flow_node_base"],
    ["","field","SemicolonClassElement","declaration_base"],
    ["","field","SetAccessorDeclaration","body_base declaration_base flow_node_base function_like_base named_member_base"],
    ["","field","ShorthandPropertyAssignment","declaration_base"],
    ["","field","SpreadAssignment","declaration_base"],
    ["","field","SwitchStatement","flow_node_base"],
    ["","field","TemplateHead","template_literal_like_node_base"],
    ["","field","TemplateMiddle","template_literal_like_node_base"],
    ["","field","TemplateTail","template_literal_like_node_base"],
    ["","field","ThrowStatement","flow_node_base"],
    ["","field","TryStatement","flow_node_base"],
    ["","field","TypeAliasDeclaration","declaration_base exportable_base flow_node_base locals_container_base modifiers_base name"],
    ["","field","TypeLiteralNode","declaration_base"],
    ["","field","TypeParameterDeclaration","declaration_base modifiers_base"],
    ["","field","TypeQueryNode","node_with_type_arguments_base"],
    ["","field","TypeReferenceNode","node_with_type_arguments_base"],
    ["","field","VariableDeclaration","declaration_base exportable_base"],
    ["","field","VariableDeclarationRare","exclamation_token"],
    ["","field","VariableStatement","flow_node_base modifiers_base"],
    ["","field","WhileStatement","iteration_statement_base"],
    ["","field","WithStatement","flow_node_base"],
    ["","fn","ArrayLiteralExpression","clone_node for_each_child visit_each_child"],
    ["","fn","ArrayTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","ArrowFunction","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","AsExpression","clone_node for_each_child visit_each_child"],
    ["","fn","AwaitExpression","clone_node for_each_child visit_each_child"],
    ["","fn","BigIntLiteral","clone_node"],
    ["","fn","BinaryExpression","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","BindingElement","clone_node for_each_child visit_each_child"],
    ["","fn","BindingPattern","clone_node for_each_child visit_each_child"],
    ["","fn","Block","clone_node for_each_child visit_each_child"],
    ["","fn","BreakStatement","clone_node for_each_child visit_each_child"],
    ["","fn","CallExpression","clone_node for_each_child visit_each_child"],
    ["","fn","CallSignatureDeclaration","clone_node for_each_child visit_each_child"],
    ["","fn","CaseBlock","clone_node for_each_child visit_each_child"],
    ["","fn","CaseOrDefaultClause","clone_node for_each_child visit_each_child"],
    ["","fn","CatchClause","clone_node for_each_child visit_each_child"],
    ["","fn","ClassDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ClassExpression","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ClassStaticBlockDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ComputedPropertyName","clone_node for_each_child visit_each_child"],
    ["","fn","ConditionalExpression","clone_node for_each_child visit_each_child"],
    ["","fn","ConditionalTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","ConstructorDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ConstructorTypeNode","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ConstructSignatureDeclaration","clone_node for_each_child visit_each_child"],
    ["","fn","ContinueStatement","clone_node for_each_child visit_each_child"],
    ["","fn","DebuggerStatement","clone_node"],
    ["","fn","Decorator","clone_node for_each_child visit_each_child"],
    ["","fn","DeleteExpression","clone_node for_each_child visit_each_child"],
    ["","fn","DoStatement","clone_node for_each_child visit_each_child"],
    ["","fn","ElementAccessExpression","clone_node for_each_child visit_each_child"],
    ["","fn","EmptyStatement","clone_node"],
    ["","fn","EnumDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","EnumMember","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ExportAssignment","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ExportDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ExportSpecifier","clone_node for_each_child visit_each_child"],
    ["","fn","ExpressionStatement","clone_node for_each_child visit_each_child"],
    ["","fn","ExpressionWithTypeArguments","clone_node for_each_child visit_each_child"],
    ["","fn","ExternalModuleReference","clone_node for_each_child visit_each_child"],
    ["","fn","ForInOrOfStatement","clone_node for_each_child visit_each_child"],
    ["","fn","ForStatement","clone_node for_each_child visit_each_child"],
    ["","fn","FunctionDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","FunctionExpression","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","FunctionTypeNode","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","GetAccessorDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","HeritageClause","clone_node for_each_child visit_each_child"],
    ["","fn","IfStatement","clone_node for_each_child visit_each_child"],
    ["","fn","ImportAttribute","clone_node for_each_child visit_each_child"],
    ["","fn","ImportAttributes","clone_node for_each_child visit_each_child"],
    ["","fn","ImportClause","clone_node for_each_child visit_each_child"],
    ["","fn","ImportDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ImportEqualsDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ImportSpecifier","clone_node for_each_child visit_each_child"],
    ["","fn","ImportTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","IndexedAccessTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","IndexSignatureDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","InferTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","InterfaceDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","IntersectionTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","JSDoc","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocAllType","clone_node"],
    ["","fn","JSDocAugmentsTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocCallbackTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocDeprecatedTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocImplementsTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocImportTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocLink","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocLinkCode","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocLinkPlain","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocNameReference","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocNonNullableType","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocNullableType","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocOptionalType","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocOverloadTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocOverrideTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocParameterOrPropertyTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocPrivateTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocProtectedTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocPublicTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocReadonlyTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocReturnTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocSatisfiesTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocSeeTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocSignature","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocTemplateTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocText","clone_node"],
    ["","fn","JSDocThisTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocThrowsTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocTypedefTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocTypeExpression","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocTypeLiteral","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocTypeTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocUnknownTag","clone_node for_each_child visit_each_child"],
    ["","fn","JSDocVariadicType","clone_node for_each_child visit_each_child"],
    ["","fn","JsxAttribute","clone_node for_each_child visit_each_child"],
    ["","fn","JsxAttributes","clone_node for_each_child visit_each_child"],
    ["","fn","JsxClosingElement","clone_node for_each_child visit_each_child"],
    ["","fn","JsxClosingFragment","clone_node"],
    ["","fn","JsxElement","clone_node for_each_child visit_each_child"],
    ["","fn","JsxExpression","clone_node for_each_child visit_each_child"],
    ["","fn","JsxFragment","clone_node for_each_child visit_each_child"],
    ["","fn","JsxNamespacedName","clone_node for_each_child visit_each_child"],
    ["","fn","JsxOpeningElement","clone_node for_each_child visit_each_child"],
    ["","fn","JsxOpeningFragment","clone_node"],
    ["","fn","JsxSelfClosingElement","clone_node for_each_child visit_each_child"],
    ["","fn","JsxSpreadAttribute","clone_node for_each_child visit_each_child"],
    ["","fn","JsxText","clone_node"],
    ["","fn","KeywordExpression","clone_node"],
    ["","fn","KeywordTypeNode","clone_node"],
    ["","fn","LabeledStatement","clone_node for_each_child visit_each_child"],
    ["","fn","LiteralTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","MappedTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","MetaProperty","clone_node for_each_child visit_each_child"],
    ["","fn","MethodDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","MethodSignatureDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","MissingDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ModuleBlock","clone_node for_each_child visit_each_child"],
    ["","fn","ModuleDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","NamedExports","clone_node for_each_child visit_each_child"],
    ["","fn","NamedImports","clone_node for_each_child visit_each_child"],
    ["","fn","NamedTupleMember","clone_node for_each_child visit_each_child"],
    ["","fn","NamespaceExport","clone_node for_each_child visit_each_child"],
    ["","fn","NamespaceExportDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","NamespaceImport","clone_node for_each_child visit_each_child"],
    ["","fn","NewExpression","clone_node for_each_child visit_each_child"],
    ["","fn","NodeFactory","update_array_type_node update_await_expression update_binding_pattern update_break_statement update_case_block update_class_static_block_declaration update_continue_statement update_decorator update_delete_expression update_export_specifier update_import_attribute update_import_attributes update_infer_type_node update_intersection_type_node update_jsdoc update_jsdoc_augments_tag update_jsdoc_callback_tag update_jsdoc_deprecated_tag update_jsdoc_implements_tag update_jsdoc_import_tag update_jsdoc_link update_jsdoc_link_code update_jsdoc_link_plain update_jsdoc_name_reference update_jsdoc_non_nullable_type update_jsdoc_nullable_type update_jsdoc_optional_type update_jsdoc_overload_tag update_jsdoc_override_tag update_jsdoc_private_tag update_jsdoc_protected_tag update_jsdoc_public_tag update_jsdoc_readonly_tag update_jsdoc_return_tag update_jsdoc_satisfies_tag update_jsdoc_see_tag update_jsdoc_template_tag update_jsdoc_this_tag update_jsdoc_throws_tag update_jsdoc_type_expression update_jsdoc_type_literal update_jsdoc_type_tag update_jsdoc_typedef_tag update_jsdoc_unknown_tag update_jsdoc_variadic_type update_jsx_attribute update_jsx_attributes update_jsx_closing_element update_jsx_element update_jsx_expression update_jsx_fragment update_jsx_namespaced_name update_jsx_spread_attribute update_meta_property update_missing_declaration update_named_tuple_member update_namespace_export update_namespace_export_declaration update_namespace_import update_optional_type_node update_parenthesized_type_node update_qualified_name update_rest_type_node update_syntax_list update_synthetic_expression update_synthetic_reference_expression update_template_expression update_template_literal_type_node update_template_literal_type_span update_template_span update_throw_statement update_tuple_type_node update_type_of_expression update_void_expression"],
    ["","fn","NonNullExpression","clone_node for_each_child visit_each_child"],
    ["","fn","NoSubstitutionTemplateLiteral","clone_node"],
    ["","fn","NotEmittedStatement","clone_node"],
    ["","fn","NotEmittedTypeElement","clone_node"],
    ["","fn","NumericLiteral","clone_node"],
    ["","fn","ObjectLiteralExpression","clone_node for_each_child visit_each_child"],
    ["","fn","OmittedExpression","clone_node"],
    ["","fn","OptionalTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","ParameterDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ParenthesizedExpression","clone_node for_each_child visit_each_child"],
    ["","fn","ParenthesizedTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","PartiallyEmittedExpression","clone_node for_each_child visit_each_child"],
    ["","fn","PostfixUnaryExpression","clone_node for_each_child visit_each_child"],
    ["","fn","PrefixUnaryExpression","clone_node for_each_child visit_each_child"],
    ["","fn","PrivateIdentifier","clone_node"],
    ["","fn","PropertyAccessExpression","clone_node for_each_child visit_each_child"],
    ["","fn","PropertyAssignment","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","PropertyDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","PropertySignatureDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","QualifiedName","clone_node for_each_child visit_each_child"],
    ["","fn","RegularExpressionLiteral","clone_node"],
    ["","fn","RestTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","ReturnStatement","clone_node for_each_child visit_each_child"],
    ["","fn","SatisfiesExpression","clone_node for_each_child visit_each_child"],
    ["","fn","SemicolonClassElement","clone_node"],
    ["","fn","SetAccessorDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","ShorthandPropertyAssignment","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","SpreadAssignment","clone_node for_each_child visit_each_child"],
    ["","fn","SpreadElement","clone_node for_each_child visit_each_child"],
    ["","fn","StringLiteral","clone_node"],
    ["","fn","SwitchStatement","clone_node for_each_child visit_each_child"],
    ["","fn","SyntaxList","clone_node for_each_child visit_each_child"],
    ["","fn","SyntheticExpression","clone_node for_each_child type_ visit_each_child"],
    ["","fn","SyntheticReferenceExpression","clone_node for_each_child visit_each_child"],
    ["","fn","TaggedTemplateExpression","clone_node for_each_child visit_each_child"],
    ["","fn","TemplateExpression","clone_node for_each_child visit_each_child"],
    ["","fn","TemplateHead","clone_node"],
    ["","fn","TemplateLiteralTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","TemplateLiteralTypeSpan","clone_node for_each_child visit_each_child"],
    ["","fn","TemplateMiddle","clone_node"],
    ["","fn","TemplateSpan","clone_node for_each_child visit_each_child"],
    ["","fn","TemplateTail","clone_node"],
    ["","fn","ThisTypeNode","clone_node"],
    ["","fn","ThrowStatement","clone_node for_each_child visit_each_child"],
    ["","fn","Token","clone_node"],
    ["","fn","TryStatement","clone_node for_each_child visit_each_child"],
    ["","fn","TupleTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","TypeAliasDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","TypeAssertion","clone_node for_each_child visit_each_child"],
    ["","fn","TypeLiteralNode","clone_node for_each_child visit_each_child"],
    ["","fn","TypeOfExpression","clone_node for_each_child visit_each_child"],
    ["","fn","TypeOperatorNode","clone_node for_each_child visit_each_child"],
    ["","fn","TypeParameterDeclaration","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","TypePredicateNode","clone_node for_each_child visit_each_child"],
    ["","fn","TypeQueryNode","clone_node for_each_child visit_each_child"],
    ["","fn","TypeReferenceNode","clone_node for_each_child visit_each_child"],
    ["","fn","UnionTypeNode","clone_node for_each_child visit_each_child"],
    ["","fn","VariableDeclaration","clone_node for_each_child visit_each_child"],
    ["","fn","VariableDeclarationList","clone_node for_each_child visit_each_child"],
    ["","fn","VariableStatement","clone_node for_each_child set_modifiers visit_each_child"],
    ["","fn","VoidExpression","clone_node for_each_child visit_each_child"],
    ["","fn","WhileStatement","clone_node for_each_child visit_each_child"],
    ["","fn","WithStatement","clone_node for_each_child visit_each_child"],
    ["","fn","YieldExpression","clone_node for_each_child visit_each_child"],
    ["","struct","","JSDocAllType JsxClosingFragment JsxOpeningFragment OmittedExpression ThisTypeNode"],
    ["omit","fn","","is_additive_operator is_additive_operator_or_higher is_array_type_node is_assignment_operator_or_higher is_binary_operator is_bitwise_operator is_bitwise_operator_or_higher is_break_statement is_case_block is_compound_assignment_operator is_continue_statement is_debugger_statement is_delete_expression is_do_statement is_equality_operator is_equality_operator_or_higher is_exponentiation_operator is_import_attribute is_import_phase_modifier_kind is_jsdoc_all_type is_jsdoc_link is_jsdoc_link_code is_jsdoc_link_plain is_jsdoc_node_kind is_jsdoc_optional_type is_jsdoc_overload_tag is_jsdoc_override_tag is_jsdoc_private_tag is_jsdoc_property_tag is_jsdoc_protected_tag is_jsdoc_public_tag is_jsdoc_readonly_tag is_jsdoc_see_tag is_jsdoc_text is_jsdoc_this_tag is_jsdoc_throws_tag is_jsdoc_unknown_tag is_jsdoc_variadic_type is_jsx_closing_fragment is_jsx_token_kind is_keyword_expression is_keyword_expression_kind is_keyword_type_kind is_keyword_type_node is_logical_operator is_logical_operator_or_higher is_missing_declaration is_multiplicative_operator is_multiplicative_operator_or_higher is_not_emitted_type_element is_postfix_unary_operator is_prefix_unary_operator is_pseudo_literal_kind is_relational_operator is_relational_operator_or_higher is_semicolon_class_element is_shift_operator is_shift_operator_or_higher is_template_literal_type_node is_template_middle is_token is_trivia_kind is_type_assertion is_while_statement is_with_statement"],
    ["omit","fn","ArrowFunction","end_flow_node flow_node locals next_container set_end_flow_node set_flow_node set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","BigIntLiteral","set_token_flags"],
    ["omit","fn","BinaryExpression","set_right set_symbol set_type symbol"],
    ["omit","fn","BindingElement","flow_node local_symbol set_flow_node set_local_symbol set_symbol symbol"],
    ["omit","fn","Block","flow_node locals next_container set_flow_node set_locals set_next_container"],
    ["omit","fn","BodyBase","asterisk_token body set_end_flow_node"],
    ["omit","fn","BreakStatement","flow_node set_flow_node"],
    ["omit","fn","CallExpression","set_symbol symbol"],
    ["omit","fn","CallSignatureDeclaration","full_signature locals next_container set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","CaseBlock","locals next_container set_locals set_next_container"],
    ["omit","fn","CatchClause","locals next_container set_locals set_next_container"],
    ["omit","fn","ClassDeclaration","flow_node local_symbol locals next_container set_flow_node set_heritage_clauses set_local_symbol set_locals set_next_container set_symbol set_type_parameters symbol"],
    ["omit","fn","ClassExpression","local_symbol locals next_container set_heritage_clauses set_local_symbol set_locals set_next_container set_symbol set_type_parameters symbol"],
    ["omit","fn","ClassLikeBase","local_symbol locals next_container set_heritage_clauses set_local_symbol set_locals set_modifiers set_next_container set_type_parameters"],
    ["omit","fn","ClassStaticBlockDeclaration","locals next_container set_locals set_next_container set_symbol symbol"],
    ["omit","fn","ConditionalTypeNode","locals next_container set_locals set_next_container"],
    ["omit","fn","ConstructorDeclaration","asterisk_token end_flow_node locals next_container set_end_flow_node set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","ConstructorTypeNode","full_signature locals next_container set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","ConstructSignatureDeclaration","full_signature locals next_container set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","ContinueStatement","flow_node set_flow_node"],
    ["omit","fn","DebuggerStatement","flow_node set_flow_node"],
    ["omit","fn","DoStatement","flow_node set_flow_node"],
    ["omit","fn","ElementAccessExpression","flow_node set_flow_node"],
    ["omit","fn","EmptyStatement","flow_node set_flow_node"],
    ["omit","fn","EnumDeclaration","flow_node local_symbol set_flow_node set_local_symbol set_symbol symbol"],
    ["omit","fn","EnumMember","modifiers set_symbol symbol"],
    ["omit","fn","ExportableBase","local_symbol set_local_symbol"],
    ["omit","fn","ExportAssignment","flow_node set_expression set_flow_node set_symbol set_type symbol"],
    ["omit","fn","ExportDeclaration","flow_node set_flow_node set_symbol symbol"],
    ["omit","fn","ExportSpecifier","local_symbol set_local_symbol set_symbol symbol"],
    ["omit","fn","ExpressionStatement","flow_node set_flow_node"],
    ["omit","fn","ExpressionWithTypeArguments","set_type_arguments"],
    ["omit","fn","FlowNodeBase","flow_node set_flow_node"],
    ["omit","fn","ForInOrOfStatement","flow_node locals next_container set_flow_node set_locals set_next_container"],
    ["omit","fn","ForStatement","flow_node locals next_container set_flow_node set_locals set_next_container"],
    ["omit","fn","FunctionDeclaration","end_flow_node flow_node local_symbol locals next_container set_end_flow_node set_flow_node set_full_signature set_local_symbol set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","FunctionExpression","end_flow_node flow_node locals next_container set_end_flow_node set_flow_node set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","FunctionLikeBase","locals next_container set_full_signature set_locals set_next_container set_parameters set_type set_type_parameters"],
    ["omit","fn","FunctionTypeNode","full_signature locals modifiers next_container set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","GetAccessorDeclaration","asterisk_token end_flow_node flow_node locals next_container set_end_flow_node set_flow_node set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","IfStatement","flow_node set_flow_node"],
    ["omit","fn","ImportClause","local_symbol set_local_symbol set_phase_modifier set_symbol symbol"],
    ["omit","fn","ImportDeclaration","flow_node set_flow_node set_symbol symbol"],
    ["omit","fn","ImportEqualsDeclaration","flow_node local_symbol set_flow_node set_local_symbol set_symbol symbol"],
    ["omit","fn","ImportSpecifier","local_symbol set_local_symbol set_symbol symbol"],
    ["omit","fn","IndexSignatureDeclaration","full_signature locals next_container set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol type_parameters"],
    ["omit","fn","InterfaceDeclaration","flow_node local_symbol set_flow_node set_local_symbol set_symbol symbol"],
    ["omit","fn","IterationStatementBase","flow_node set_flow_node statement"],
    ["omit","fn","JSDocCommentBase","text"],
    ["omit","fn","JSDocSignature","full_signature locals next_container set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","JSDocTagBase","comment tag_name"],
    ["omit","fn","JSDocTypeLiteral","set_symbol symbol"],
    ["omit","fn","JsxAttribute","set_symbol symbol"],
    ["omit","fn","JsxAttributes","set_symbol symbol"],
    ["omit","fn","JsxSpreadAttribute","set_symbol symbol"],
    ["omit","fn","JsxText","set_token_flags token_flags"],
    ["omit","fn","KeywordExpression","flow_node set_flow_node"],
    ["omit","fn","LabeledStatement","flow_node set_flow_node"],
    ["omit","fn","LiteralLikeNodeBase","set_token_flags"],
    ["omit","fn","LocalsContainerBase","locals next_container set_next_container"],
    ["omit","fn","MappedTypeNode","locals next_container set_locals set_next_container set_symbol symbol"],
    ["omit","fn","MetaProperty","flow_node set_flow_node"],
    ["omit","fn","MethodDeclaration","end_flow_node flow_node locals next_container set_end_flow_node set_flow_node set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","MethodSignatureDeclaration","full_signature locals next_container set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","MissingDeclaration","flow_node set_flow_node set_symbol symbol"],
    ["omit","fn","ModifiersBase","modifiers set_modifiers"],
    ["omit","fn","ModuleBlock","flow_node set_flow_node"],
    ["omit","fn","ModuleDeclaration","asterisk_token end_flow_node flow_node local_symbol locals next_container set_end_flow_node set_flow_node set_local_symbol set_locals set_next_container set_symbol symbol"],
    ["omit","fn","NamedMemberBase","modifiers name postfix_token set_modifiers"],
    ["omit","fn","NamedTupleMember","set_symbol symbol"],
    ["omit","fn","NamespaceExport","set_symbol symbol"],
    ["omit","fn","NamespaceExportDeclaration","flow_node set_flow_node set_symbol symbol"],
    ["omit","fn","NamespaceImport","local_symbol set_local_symbol set_symbol symbol"],
    ["omit","fn","Node","as_debugger_statement as_empty_statement as_jsdoc_all_type as_jsx_closing_fragment as_jsx_opening_fragment as_not_emitted_statement as_not_emitted_type_element as_omitted_expression as_semicolon_class_element as_this_type_node"],
    ["omit","fn","NodeWithTypeArgumentsBase","type_arguments"],
    ["omit","fn","NoSubstitutionTemplateLiteral","raw_text set_symbol set_token_flags symbol token_flags"],
    ["omit","fn","NotEmittedStatement","flow_node set_flow_node"],
    ["omit","fn","NotEmittedTypeElement","set_symbol symbol"],
    ["omit","fn","NumericLiteral","set_token_flags"],
    ["omit","fn","ObjectLiteralExpression","set_symbol symbol"],
    ["omit","fn","ParameterDeclaration","set_question_token set_symbol set_type symbol"],
    ["omit","fn","ParenthesizedExpression","set_expression"],
    ["omit","fn","PropertyAccessExpression","flow_node set_flow_node"],
    ["omit","fn","PropertyAssignment","set_initializer set_symbol set_type symbol"],
    ["omit","fn","PropertyDeclaration","set_initializer set_symbol set_type symbol"],
    ["omit","fn","PropertySignatureDeclaration","set_symbol set_type symbol"],
    ["omit","fn","QualifiedName","flow_node set_flow_node"],
    ["omit","fn","RegularExpressionLiteral","set_token_flags"],
    ["omit","fn","ReturnStatement","flow_node set_expression set_flow_node"],
    ["omit","fn","SemicolonClassElement","set_symbol symbol"],
    ["omit","fn","SetAccessorDeclaration","asterisk_token end_flow_node flow_node locals next_container set_end_flow_node set_flow_node set_full_signature set_locals set_next_container set_parameters set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","ShorthandPropertyAssignment","set_object_assignment_initializer set_symbol set_type symbol"],
    ["omit","fn","SpreadAssignment","set_symbol symbol"],
    ["omit","fn","StringLiteral","set_token_flags"],
    ["omit","fn","SwitchStatement","flow_node set_flow_node"],
    ["omit","fn","TemplateHead","set_token_flags token_flags"],
    ["omit","fn","TemplateLiteralLikeNodeBase","raw_text set_token_flags template_flags text token_flags"],
    ["omit","fn","TemplateMiddle","set_token_flags token_flags"],
    ["omit","fn","TemplateTail","set_token_flags token_flags"],
    ["omit","fn","ThrowStatement","flow_node set_flow_node"],
    ["omit","fn","TryStatement","flow_node set_flow_node"],
    ["omit","fn","TypeAliasDeclaration","flow_node local_symbol locals next_container set_flow_node set_local_symbol set_locals set_next_container set_symbol set_type set_type_parameters symbol"],
    ["omit","fn","TypeLiteralNode","set_symbol symbol"],
    ["omit","fn","TypeParameterDeclaration","set_symbol symbol"],
    ["omit","fn","UnionOrIntersectionTypeNodeBase","types"],
    ["omit","fn","VariableDeclaration","local_symbol set_initializer set_local_symbol set_symbol set_type symbol"],
    ["omit","fn","VariableStatement","flow_node set_flow_node"],
    ["omit","fn","WhileStatement","flow_node set_flow_node"],
    ["omit","fn","WithStatement","flow_node set_flow_node"],
    ["pub(crate)","field","BreakStatement","label"],
    ["pub(crate)","field","ConditionalExpression","question_token"],
    ["pub(crate)","field","ContinueStatement","label"],
    ["pub(crate)","field","DeleteExpression","expression"],
    ["pub(crate)","field","ExportAssignment","expression type_"],
    ["pub(crate)","field","ExternalModuleReference","expression"],
    ["pub(crate)","field","FlowNodeBase","flow_node"],
    ["pub(crate)","field","JSDocAugmentsTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocCallbackTag","jsdoc_tag_base type_expression"],
    ["pub(crate)","field","JSDocDeprecatedTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocImplementsTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocImportTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocOverloadTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocOverrideTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocParameterOrPropertyTag","jsdoc_tag_base name"],
    ["pub(crate)","field","JSDocPrivateTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocProtectedTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocPublicTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocReadonlyTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocReturnTag","jsdoc_tag_base type_expression"],
    ["pub(crate)","field","JSDocSatisfiesTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocSeeTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocTagBase","comment tag_name"],
    ["pub(crate)","field","JSDocTemplateTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocThisTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocThrowsTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocTypedefTag","jsdoc_tag_base type_expression"],
    ["pub(crate)","field","JSDocTypeTag","jsdoc_tag_base"],
    ["pub(crate)","field","JSDocUnknownTag","jsdoc_tag_base"],
    ["pub(crate)","field","JsxAttributes","properties"],
    ["pub(crate)","field","JsxClosingElement","tag_name"],
    ["pub(crate)","field","JsxOpeningElement","type_arguments"],
    ["pub(crate)","field","JsxSelfClosingElement","type_arguments"],
    ["pub(crate)","field","NonNullExpression","expression"],
    ["pub(crate)","field","OptionalTypeNode","type_"],
    ["pub(crate)","field","PropertyAssignment","type_"],
    ["pub(crate)","field","PropertyDeclaration","initializer type_"],
    ["pub(crate)","field","RestTypeNode","type_"],
    ["pub(crate)","field","ReturnStatement","expression"],
    ["pub(crate)","field","ShorthandPropertyAssignment","type_"],
    ["pub(crate)","field","TaggedTemplateExpression","type_arguments"],
    ["pub(crate)","field","TemplateLiteralTypeSpan","type_"],
    ["pub(crate)","field","ThrowStatement","expression"],
    ["pub(crate)","field","TupleTypeNode","elements"],
    ["pub(crate)","field","VariableDeclaration","initializer type_"],
    ["pub(crate)","field","VoidExpression","expression"],
    ["pub(crate)","field","WithStatement","statement"],
    ["pub(crate)","fn","","is_as_expression is_jsdoc_deprecated_tag is_jsdoc_name_reference"],
    ["pub(crate)","fn","ClassLikeBase","modifiers name type_parameters"],
    ["pub(crate)","fn","ConstructorDeclaration","full_signature"],
    ["pub(crate)","fn","EnumMember","postfix_token"],
    ["pub(crate)","fn","GetAccessorDeclaration","postfix_token"],
    ["pub(crate)","fn","LiteralLikeNodeBase","token_flags"],
    ["pub(crate)","fn","Node","as_jsdoc_text"],
    ["pub(crate)","fn","NodeFactory","update_jsdoc_parameter_or_property_tag"],
    ["pub(crate)","fn","SetAccessorDeclaration","postfix_token"],
    ["pub(crate)","struct","","BindingElementRare CallExpressionRare ElementAccessExpressionRare ImportDeclarationRare ImportSpecifierRare NewExpressionRare ParameterDeclarationRare PropertyAccessExpressionRare PropertySignatureDeclarationRare VariableDeclarationRare"],
];

// ── naming ───────────────────────────────────────────────────────────────────

const RUST_KEYWORDS = new Set(["type", "in", "as", "ref", "mod", "match", "loop", "override", "static", "const", "struct", "enum", "impl", "fn", "let", "yield", "abstract", "final", "use", "where", "true", "false", "self", "super", "crate", "move", "return", "if", "else", "for", "while", "break", "continue", "pub", "trait", "unsafe", "extern", "dyn", "async", "await", "box", "do", "macro", "priv", "typeof", "unsized", "virtual", "try"]);

function snake(s: string): string {
    s = s.replace(/JSDoc/g, "Jsdoc");
    return s.replace(/([a-z0-9])([A-Z])/g, "$1_$2").replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2").toLowerCase();
}

function ident(s: string): string {
    const n = snake(s);
    return RUST_KEYWORDS.has(n) ? n + "_" : n;
}

function paramName(m: MemberInfo): string {
    return ident(m.goParamName());
}

// ── Cell policy: fields Go mutates after construction ─────────────────────────
// goOnly fields (symbol, locals, flow nodes, ...) are always Cells. The list below is exactly the set of
// fields the parser's reparser writes after construction (MutableNode.SetType/SetExpression/
// SetInitializer/SetModifiers call sites and direct writes in reparser.go). Mirrored in docs/AST.md.
// They are `OwnedCell`s: after binding, only nodes a checker created itself are written (docs/PORTING.md,
// "Threading").

const CELL_EXPLICIT = new Set([
    "ModifiersBase.modifiers",
    "FunctionLikeBase.TypeParameters",
    "FunctionLikeBase.Parameters",
    "FunctionLikeBase.Type",
    "FunctionLikeBase.FullSignature",
    "ClassLikeBase.TypeParameters",
    "ClassLikeBase.HeritageClauses",
    "VariableDeclaration.Type",
    "VariableDeclaration.Initializer",
    "ParameterDeclaration.Type",
    "ParameterDeclaration.QuestionToken",
    "PropertyDeclaration.Type",
    "PropertyDeclaration.Initializer",
    "PropertySignatureDeclaration.Type",
    "PropertyAssignment.Type",
    "PropertyAssignment.Initializer",
    "ShorthandPropertyAssignment.Type",
    "ShorthandPropertyAssignment.ObjectAssignmentInitializer",
    "ExportAssignment.Type",
    "ExportAssignment.Expression",
    "ReturnStatement.Expression",
    "ParenthesizedExpression.Expression",
    "BinaryExpression.Type",
    "BinaryExpression.Right",
    "TypeAliasDeclaration.TypeParameters",
    "TypeAliasDeclaration.Type",
    "ImportClause.PhaseModifier",
    "ExpressionWithTypeArguments.TypeArguments",
    "HeritageClause.Types",
    // Written by the checker's node builder (nodecopy.go: `c.AsStringLiteral().TokenFlags ^= ast.TokenFlagsSingleQuote` on a clone).
    "LiteralLikeNodeBase.TokenFlags",
]);

// Fields that exist in Go but carry no data in a type-check-only port.
const SKIPPED_FIELDS = new Set(["NodeBase.Flags", "CompositeBase.facts"]);

// ── layout model ──────────────────────────────────────────────────────────────

interface Field {
    owner: string; // struct or base that declares it
    name: string; // Go name
    rust: string; // Rust field name
    ty: string; // Rust value type (without Cell)
    cell: boolean;
    rare: boolean; // stored in the struct's rare tail (RARE_FIELDS)
    member: MemberInfo;
}

// Own fields that are almost never set, stored in a tail allocated after the data struct only when one of them is set
// at construction (`NodeAllocRare` in ast.rs; the getter reads None otherwise). Only nil-able fields that nothing
// writes after construction (not Cells, not Go-only bookkeeping). Occupancy on the private monorepo (notes/mem-frontend.md):
// CallExpression ?. 0.1% and type arguments 0.5% of 1.24M, PropertyAccessExpression ?. 2.7% of 1.27M, Parameter
// `...` 1.8% and initializer 1.4% of 374K, PropertySignature initializer 0% of 401K, VariableDeclaration `!` 0.02%
// of 344K, ImportSpecifier property name 3.2% of 184K, BindingElement `...` / property name / initializer
// 1.3 / 5.9 / 2.2% of 76K, ImportDeclaration attributes 0.02% of 148K, NewExpression type arguments 5.3% of 60K.
const RARE_FIELDS = new Map<string, string[]>([
    ["CallExpression", ["QuestionDotToken", "TypeArguments"]],
    ["PropertyAccessExpression", ["QuestionDotToken"]],
    ["ElementAccessExpression", ["QuestionDotToken"]],
    ["ParameterDeclaration", ["DotDotDotToken", "Initializer"]],
    ["PropertySignatureDeclaration", ["Initializer"]],
    ["VariableDeclaration", ["ExclamationToken"]],
    ["ImportSpecifier", ["PropertyName"]],
    ["BindingElement", ["DotDotDotToken", "PropertyName", "Initializer"]],
    ["ImportDeclaration", ["Attributes"]],
    ["NewExpression", ["TypeArguments"]],
]);

interface Layout {
    name: string;
    embeds: string[]; // kept base structs embedded as fields
    fields: Field[]; // own fields
}

function rustType(m: MemberInfo): string {
    if (m.goOnly) {
        switch (m.rawType) {
            case "*Symbol":
                return "Option<P<Symbol>>";
            case "*FlowNode":
                return "Option<P<FlowNode>>";
            case "SymbolTable":
                return "Option<P<SymbolTable>>";
            case "*Node":
                return "Option<P<Node>>";
        }
        throw new Error(`unknown goOnly type ${m.rawType} for ${m.name}`);
    }
    const t = m.type;
    if (t.kind === "list") {
        if (t.listKind === "raw") {
            if (t.elementType.baseKind() === "node") return "&'static [P<Node>]";
            if (t.elementType.kind === "primitive" && t.elementType.name === "string") return "&'static [&'static str]";
            throw new Error(`unsupported raw list for ${m.name}`);
        }
        if (t.listKind === "ModifierList") return "Option<P<ModifierList>>";
        return m.optional ? "Option<P<NodeList>>" : "P<NodeList>";
    }
    switch (t.baseKind()) {
        case "list":
            return m.optional ? "Option<P<NodeList>>" : "P<NodeList>";
        case "node":
            return m.optional ? "Option<P<Node>>" : "P<Node>";
        case "kind":
            return "Kind";
        case "primitive": {
            const name = (t as any).name;
            switch (name) {
                case "bool":
                    return "bool";
                case "string":
                    return "&'static str";
                case "int":
                    return "i32";
                case "TokenFlags":
                case "NodeFlags":
                case "ModifierFlags":
                    return name;
                case "any":
                    return "&'static dyn Any";
            }
            throw new Error(`unknown primitive ${name} for ${m.name}`);
        }
    }
    throw new Error(`unknown type for ${m.name}`);
}

function isCell(owner: string, m: MemberInfo): boolean {
    return m.goOnly || CELL_EXPLICIT.has(`${owner}.${m.name}`);
}

// Fields the Go parser was observed to leave nil although ast.json does not mark them optional
// (tools/oracle/nilfields run over the test corpus + lib files, parsed as .ts/.tsx/.js).
const OBSERVED_NIL = new Set<string>(JSON.parse(fs.readFileSync(path.join(import.meta.dirname, "nilable-fields.json"), "utf8")));
const widenedToOption: string[] = [];
// Constructed with nil by the reparser or checker (then assigned, or left nil on synthetic nodes).
const FORCE_OPTION = new Set(["FunctionLikeBase.Parameters", "TypeAliasDeclaration.Type"]);

function makeField(owner: string, m: MemberInfo): Field {
    let ty = rustType(m);
    if ((ty === "P<Node>" || ty === "P<NodeList>") && (OBSERVED_NIL.has(`${owner}.${m.name}`) || FORCE_OPTION.has(`${owner}.${m.name}`))) {
        ty = `Option<${ty}>`;
        widenedToOption.push(`${owner}.${m.name}`);
    }
    const rare = RARE_FIELDS.get(owner)?.includes(m.name) ?? false;
    const cell = isCell(owner, m);
    if (rare && (cell || !ty.startsWith("Option<"))) throw new Error(`rare field ${owner}.${m.name} must be a nil-able non-Cell field`);
    return { owner, name: m.name, rust: ident(m.name), ty, cell, rare, member: m };
}

function baseOwnFields(key: string): Field[] {
    const base = api.getBase(key)!;
    return base.fields.filter(f => !f.noGo && !SKIPPED_FIELDS.has(`${key}.${f.name}`)).map(f => makeField(key, f));
}

function isKeptBase(key: string): boolean {
    return baseOwnFields(key).length > 0;
}

// Bases without own Rust fields are transparent: their own bases are inlined into the embedding struct.
function expandEmbeds(keys: string[], seen: Set<string>, owner: string): string[] {
    const result: string[] = [];
    for (const key of keys) {
        if (seen.has(key)) continue;
        seen.add(key);
        if (isKeptBase(key)) {
            for (const d of allBasesBelow(key)) {
                if (seen.has(d) && isKeptBase(d)) throw new Error(`${owner}: base ${d} embedded twice`);
                seen.add(d);
            }
            result.push(key);
        }
        else {
            result.push(...expandEmbeds(api.getBase(key)!.extendsKeys, seen, owner));
        }
    }
    return result;
}

function allBasesBelow(key: string): string[] {
    const out: string[] = [];
    for (const k of api.getBase(key)!.extendsKeys) {
        out.push(k, ...allBasesBelow(k));
    }
    return out;
}

const baseLayouts = new Map<string, Layout>();
function baseLayout(key: string): Layout {
    let l = baseLayouts.get(key);
    if (!l) {
        l = { name: key, embeds: expandEmbeds(api.getBase(key)!.extendsKeys, new Set(), key), fields: baseOwnFields(key) };
        baseLayouts.set(key, l);
    }
    return l;
}

function nodeOwnMembers(node: NodeType): MemberInfo[] {
    return node.members.filter(m => !m.inherited && !m.isKindParam() && !m.noGo);
}

function nodeLayout(node: NodeType): Layout {
    return {
        name: node.name,
        embeds: expandEmbeds(node.extendsKeys, new Set(), node.name),
        fields: nodeOwnMembers(node).map(m => makeField(node.name, m)),
    };
}

interface FlatField extends Field {
    path: string; // e.g. "function_like_base.locals_container_base.locals"
    depth: number;
}

function flatten(l: Layout, prefix = "", depth = 0): FlatField[] {
    const out: FlatField[] = [];
    for (const f of l.fields) out.push({ ...f, path: prefix + f.rust, depth });
    for (const e of l.embeds) out.push(...flatten(baseLayout(e), `${prefix}${snake(e)}.`, depth + 1));
    return out;
}

// Promoted field set, Go-style: shallowest wins.
function promoted(l: Layout): FlatField[] {
    const all = flatten(l);
    const byName = new Map<string, FlatField>();
    for (const f of all) {
        const prev = byName.get(f.rust);
        if (!prev || f.depth < prev.depth) byName.set(f.rust, f);
    }
    return all.filter(f => byName.get(f.rust) === f);
}

// Path to an embedded base anywhere inside a layout.
function basePath(l: Layout, key: string, prefix = ""): string | undefined {
    for (const e of l.embeds) {
        const p = `${prefix}${snake(e)}`;
        if (e === key) return p;
        const inner = basePath(baseLayout(e), key, p + ".");
        if (inner) return inner;
    }
    return undefined;
}

function isEmptyLayout(l: Layout): boolean {
    return l.embeds.length === 0 && l.fields.length === 0;
}

// ── writer ────────────────────────────────────────────────────────────────────

const out: string[] = [];
function w(s = "") {
    out.push(s);
}

// Node data structs written by hand although ast.json generates them (struct, getters, factory, clone): `Identifier`
// packs its flow node and its text into one word (crates/tsrs_ast/src/identifier.rs).
const HAND_WRITTEN_DATA = new Set(["Identifier"]);

// Identifier and literal text is stored as a `PackedStr` (8 bytes: pointer and length in one word, tsrs_core::ptr); the getter
// and the factory parameter stay `&'static str`.
const PACKED_STR_FIELDS = new Set(["Identifier.Text", "PrivateIdentifier.Text", "LiteralLikeNodeBase.Text"]);

function isPackedStr(f: { owner: string; name: string; }): boolean {
    return PACKED_STR_FIELDS.has(`${f.owner}.${f.name}`);
}

function fieldDecl(f: Field): string {
    if (isPackedStr(f)) return "PackedStr";
    return f.cell ? `OwnedCell<${f.ty}>` : f.ty;
}

function defaultValue(ty: string): string {
    if (ty.startsWith("Option<")) return "None";
    if (ty === "Kind") return "Kind::Unknown";
    if (ty === "bool") return "false";
    if (ty === "&'static str") return '""';
    if (ty.startsWith("&'static [")) return "&[]";
    if (ty === "TokenFlags" || ty === "NodeFlags" || ty === "ModifierFlags") return `${ty}::None`;
    throw new Error(`no default for ${ty}`);
}

function flagConst(mask: string): string {
    const m = /^(TokenFlags|NodeFlags|ModifierFlags)(\w+)$/.exec(mask);
    if (!m) throw new Error(`bad mask ${mask}`);
    return `${m[1]}::${m[2]}`;
}

function isStringType(ty: string) {
    return ty === "&'static str" || ty === "&'static [&'static str]";
}

// ── generation ────────────────────────────────────────────────────────────────

const nodes = api.nodes();
const layouts = new Map<string, Layout>();
for (const n of nodes) layouts.set(n.name, nodeLayout(n));

function schemaMembers(node: NodeType): MemberInfo[] {
    return node.members.filter(m => !m.noFactory);
}

function isNodeFlagsMember(m: MemberInfo) {
    return m.type.kind === "primitive" && m.type.name === "NodeFlags";
}

function findFlat(l: Layout, name: string): FlatField {
    const f = promoted(l).find(f => f.name === name);
    if (!f) throw new Error(`${l.name}: no field ${name}`);
    return f;
}

function header() {
    w("// Code generated by tools/gen-ast/gen-ast.ts from ts-ref/tools/scripts/tsc/ast.json. DO NOT EDIT.");
    w();
    w("use std::any::Any;");
    w();
    w("use tsrs_core::{OwnedCell, PackedStr, P};");
    w();
    w("use crate::ast::*;");
    w("use crate::flow::*;");
    w("use crate::identifier::Identifier;");
    w("use crate::kind::Kind;");
    w("use crate::nodeflags::NodeFlags;");
    w("use crate::symbol::{Symbol, SymbolTable};");
    w("use crate::tokenflags::TokenFlags;");
    w("use crate::visitor::{required_child, same_map_nodes, NodeVisitor};");
    w();
}

function genBaseStructs() {
    w("// ── Base structs ──────────────────────────────────────────────────────────");
    w();
    for (const b of api.bases()) {
        if (!isKeptBase(b.key)) continue;
        const l = baseLayout(b.key);
        w(`pub struct ${b.key} {`);
        for (const e of l.embeds) w(`    pub ${snake(e)}: ${e},`);
        for (const f of l.fields) w(`    pub ${f.rust}: ${fieldDecl(f)},`);
        w("}");
        w();
        genGetters(l);
    }
}

function genGetters(l: Layout) {
    const fields = promoted(l);
    if (fields.length === 0) return;
    w(`impl ${l.name} {`);
    for (const f of fields) {
        const fnName = f.rust;
        w(`    #[inline]`);
        w(`    pub fn ${fnName}(&self) -> ${f.ty} {`);
        if (f.rare) w(`        rare_tail(self)?.${f.rust}`);
        else w(`        self.${f.path}${f.cell ? ".get()" : isPackedStr(f) ? ".as_str()" : ""}`);
        w(`    }`);
        if (f.cell) {
            w(`    #[inline]`);
            w(`    pub fn set_${f.rust.replace(/_$/, "")}(&self, value: ${f.ty}) {`);
            w(`        self.${f.path}.set(value);`);
            w(`    }`);
        }
    }
    w("}");
    w();
}

function genStruct(node: NodeType) {
    if (HAND_WRITTEN_DATA.has(node.name)) return;
    const l = layouts.get(node.name)!;
    if (isEmptyLayout(l)) {
        w(`pub struct ${node.name};`);
        w();
        return;
    }
    w(`pub struct ${node.name} {`);
    for (const e of l.embeds) w(`    pub ${snake(e)}: ${e},`);
    for (const f of l.fields) if (!f.rare) w(`    pub ${f.rust}: ${fieldDecl(f)},`);
    w("}");
    w();
    const rare = l.fields.filter(f => f.rare);
    if (rare.length > 0) {
        w(`/// The rare tail of \`${node.name}\` (\`NodeAllocRare\`): allocated only when one of these is set.`);
        w(`pub struct ${node.name}Rare {`);
        for (const f of rare) w(`    pub ${f.rust}: ${fieldDecl(f)},`);
        w("}");
        w();
        w(`impl NodeRareTail for ${node.name} {`);
        w(`    type Rare = ${node.name}Rare;`);
        w("}");
        w();
    }
    genGetters(l);
}

// Builds the struct literal for a layout, pulling values from factory params by Go field name.
function structLiteral(l: Layout, values: Map<string, string>, indent: string): string {
    if (isEmptyLayout(l)) return l.name;
    const lines: string[] = [`${l.name} {`];
    for (const e of l.embeds) {
        lines.push(`${indent}    ${snake(e)}: ${structLiteral(baseLayout(e), values, indent + "    ")},`);
    }
    for (const f of l.fields) {
        if (f.rare) continue;
        const v = values.get(f.name) ?? defaultValue(f.ty);
        const value = f.cell ? `OwnedCell::new(${v})` : isPackedStr(f) ? `PackedStr::new(${v})` : v;
        lines.push(value === f.rust ? `${indent}    ${f.rust},` : `${indent}    ${f.rust}: ${value},`);
    }
    lines.push(`${indent}}`);
    return lines.join("\n");
}

// The rare tail literal of a node layout and the condition under which it is allocated.
function rareLiteral(l: Layout, values: Map<string, string>, indent: string): { cond: string; lit: string; } | undefined {
    const rare = l.fields.filter(f => f.rare);
    if (rare.length === 0) return undefined;
    const value = (f: Field) => values.get(f.name) ?? defaultValue(f.ty);
    const lines = [`${l.name}Rare {`, ...rare.map(f => value(f) === f.rust ? `${indent}    ${f.rust},` : `${indent}    ${f.rust}: ${value(f)},`), `${indent}}`];
    return { cond: rare.map(f => `${value(f)}.is_some()`).join(" || "), lit: lines.join("\n") };
}

// Factory parameter types are the storage field types (an inherited member may narrow the Go type, but
// the storage in the base is what Rust stores and returns).
function factoryParams(node: NodeType): { m: MemberInfo; name: string; ty: string; }[] {
    const l = layouts.get(node.name)!;
    return schemaMembers(node).map(m => {
        if (m.isKindParam()) return { m, name: "kind", ty: "Kind" };
        if (isNodeFlagsMember(m)) return { m, name: paramName(m), ty: "NodeFlags" };
        return { m, name: paramName(m), ty: findFlat(l, m.name).ty };
    });
}

function hasTextContent(node: NodeType): boolean {
    return schemaMembers(node).some(m => !m.isKindParam() && !m.goOnly && isStringType(rustType(m)));
}

// Later concrete node definitions override Token and other shared kind ranges.
// This is the same canonical kind mapping used by Go's generated casts and factories.
function canonicalKindsByNode(): Map<NodeType, string[]> {
    const nodeByKind = new Map<string, NodeType>();
    for (const node of nodes) {
        for (const kind of node.allKinds()) nodeByKind.set(kind.name, node);
    }
    const kindsByNode = new Map<NodeType, string[]>();
    for (const [kind, node] of nodeByKind) {
        const kinds = kindsByNode.get(node) ?? [];
        kinds.push(kind);
        kindsByNode.set(node, kinds);
    }
    return kindsByNode;
}

// NewToken must use the canonical payload for kinds also owned by a concrete node.
// The generated accessors dispatch through NodeDataTag, so a generic Token payload
// would make, for example, a NullKeyword lack KeywordExpression's flow data.
function genNewTokenFactory(token: NodeType) {
    const tokenKinds = new Set(token.allKinds().map(kind => kind.name));

    w("    pub fn new_token(&self, kind: Kind) -> P<Node> {");
    w('        assert!(is_token_kind(kind), "Invalid token kind");');
    w("        match kind {");
    for (const [node, canonicalKinds] of canonicalKindsByNode()) {
        const kinds = canonicalKinds.filter(kind => tokenKinds.has(kind));
        if (kinds.length === 0 || node === token) continue;
        const payload = node.name === "Identifier"
            ? "self.new_identifier_token()"
            : isEmptyLayout(layouts.get(node.name)!)
            ? `self.new_empty_node(kind, NodeDataTag::${node.name})`
            : `self.new_node(kind, ${structLiteral(layouts.get(node.name)!, new Map(), "                ")})`;
        w(`            ${kinds.map(kind => `Kind::${kind}`).join(" | ")} => ${payload},`);
    }
    w("            _ => self.new_empty_node(kind, NodeDataTag::Token),");
    w("        }");
    w("    }");
    w();
}

function genNewFactory(node: NodeType) {
    if (node.name === "Token") {
        genNewTokenFactory(node);
        return;
    }
    if (HAND_WRITTEN_DATA.has(node.name)) return;
    const l = layouts.get(node.name)!;
    const params = factoryParams(node);
    const kindMember = params.find(p => p.m.isKindParam());
    const flagsMembers = params.filter(p => isNodeFlagsMember(p.m));
    const emit = (fnName: string, kindName: string) => {
        w(`    pub fn ${fnName}(&self${params.map(p => `, ${p.name}: ${p.ty}`).join("")}) -> P<Node> {`);
        if (kindMember) {
            const kinds = canonicalKindsByNode().get(node) ?? [];
            if (kinds.length === 0) throw new Error(`${node.name}: no canonical kinds`);
            w(`        assert!(matches!(kind, ${kinds.map(kind => `Kind::${kind}`).join(" | ")}), "Invalid node kind for ${node.name}");`);
        }
        const values = new Map<string, string>();
        for (const p of params) {
            if (p.m.isKindParam() || isNodeFlagsMember(p.m)) continue;
            values.set(p.m.name, p.m.bitmask ? `${p.name} & ${flagConst(p.m.bitmask)}` : p.name);
        }
        if (hasTextContent(node)) w(`        self.text_count.set(self.text_count.get() + 1);`);
        const kindArg = kindMember ? "kind" : `Kind::${kindName}`;
        const rare = rareLiteral(l, values, "            ");
        const newNode = isEmptyLayout(l)
            ? `self.new_empty_node(${kindArg}, NodeDataTag::${node.name})`
            : rare
            ? `if ${rare.cond} {\n            self.new_node_with_rare(${kindArg}, ${structLiteral(l, values, "            ")}, ${rare.lit})\n        } else {\n            self.new_node(${kindArg}, ${structLiteral(l, values, "            ")})\n        }`
            : `self.new_node(${kindArg}, ${structLiteral(l, values, "        ")})`;
        if (flagsMembers.length > 0) {
            w(`        let node = ${newNode};`);
            for (const f of flagsMembers) {
                if (f.m.bitmask) w(`        node.flags.set(node.flags.get() | (${f.name} & ${flagConst(f.m.bitmask)}));`);
                else w(`        node.flags.set(${f.name});`);
            }
            w(`        node`);
        }
        else {
            w(`        ${newNode}`);
        }
        w(`    }`);
        w();
    };
    emit(`new_${snake(node.name)}`, node.syntaxKindName);
    for (const alias of node.kindAliases) emit(`new_${snake(alias)}`, alias);
}

function diffExpr(ty: string, a: string, b: string): string {
    if (ty.startsWith("&'static [")) return `!same_slice(${a}, ${b})`;
    if (ty === "&'static dyn Any") return `!std::ptr::addr_eq(std::ptr::from_ref::<dyn Any>(${a}), std::ptr::from_ref::<dyn Any>(${b}))`;
    return `${a} != ${b}`;
}

// Value of a factory member read back from existing data (`recv` = data struct, `nodeVar` = the Node).
function memberValue(l: Layout, m: MemberInfo, recv: string, nodeVar: string): string {
    if (m.isKindParam()) return `${nodeVar}.kind()`;
    if (isNodeFlagsMember(m)) return `${nodeVar}.flags.get()`;
    return `${recv}.${findFlat(l, m.name).rust}()`;
}

function genUpdateFactory(node: NodeType) {
    const l = layouts.get(node.name)!;
    const params = factoryParams(node);
    const updateParams = params.filter(p => !p.m.isKindParam());
    if (updateParams.length === 0) return;
    if (!updateParams.some(p => p.m.isChild())) return;
    w(`    pub fn update_${snake(node.name)}(&self, node: P<Node>${updateParams.map(p => `, ${p.name}: ${p.ty}`).join("")}) -> P<Node> {`);
    w(`        let data = node.as_${snake(node.name)}();`);
    const cmps = updateParams.map(p => diffExpr(p.ty, p.name, memberValue(l, p.m, "data", "node")));
    w(`        if ${cmps.join(" || ")} {`);
    const newArgs = params.map(p => p.m.isKindParam() ? "node.kind()" : p.name).join(", ");
    if (node.kindAliases.length > 0) {
        w(`            let updated = match node.kind() {`);
        w(`                Kind::${node.syntaxKindName} => self.new_${snake(node.name)}(${newArgs}),`);
        for (const a of node.kindAliases) w(`                Kind::${a} => self.new_${snake(a)}(${newArgs}),`);
        w(`                _ => panic!("unexpected kind in update_${snake(node.name)}: {:?}", node.kind()),`);
        w(`            };`);
        w(`            return update_node(updated, node, &self.hooks);`);
    }
    else {
        w(`            return update_node(self.new_${snake(node.name)}(${newArgs}), node, &self.hooks);`);
    }
    w(`        }`);
    w(`        node`);
    w(`    }`);
    w();
}

function childMembers(node: NodeType) {
    return schemaMembers(node).filter(m => m.isChild());
}

function hasForEachChild(node: NodeType) {
    if (node.handWritten) return true;
    return childMembers(node).length > 0;
}

function genForEachChild(node: NodeType) {
    const l = layouts.get(node.name)!;
    const children = childMembers(node);
    if (children.length === 0) return;
    w(`impl ${node.name} {`);
    w(`    pub fn for_each_child<V: FnMut(P<Node>) -> bool + ?Sized>(&self, v: &mut V) -> bool {`);
    if (node.handWrittenVisitor) {
        w(`        for_each_child_${snake(node.name)}(self, v)`);
    }
    else {
        const parts = children.map(m => {
            const f = findFlat(l, m.name);
            const access = `self.${f.rust}()`;
            const ty = f.ty;
            if (m.listKind === "raw") return `visit_nodes(v, ${access})`;
            if (m.listKind === "ModifierList") return `visit_modifiers(v, ${access})`;
            if (m.listKind === "NodeList" || m.type.baseKind() === "list") return ty.startsWith("Option") ? `visit_node_list(v, ${access})` : `visit_node_list(v, Some(${access}))`;
            return ty.startsWith("Option") ? `visit(v, ${access})` : `v(${access})`;
        });
        w(`        ${parts.join("\n            || ")}`);
    }
    w(`    }`);
    w(`}`);
    w();
}

function genVisitEachChild(node: NodeType) {
    const l = layouts.get(node.name)!;
    const members = schemaMembers(node);
    const children = members.filter(m => m.isChild());
    if (children.length === 0) return;
    w(`impl ${node.name} {`);
    w(`    pub fn visit_each_child(&self, node: P<Node>, v: &mut NodeVisitor) -> P<Node> {`);
    if (node.handWrittenVisitor) {
        w(`        visit_each_child_${snake(node.name)}(self, node, v)`);
        w(`    }`);
        w(`}`);
        w();
        return;
    }
    const updateMembers = members.filter(m => !m.isKindParam());
    const args: string[] = [];
    for (const m of updateMembers) {
        if (!m.isChild()) {
            args.push(memberValue(l, m, "self", "node"));
            continue;
        }
        const f = findFlat(l, m.name);
        const access = `self.${f.rust}()`;
        const opt = f.ty.startsWith("Option");
        if (m.type.kind === "list" && m.type.listKind === "raw") {
            w(`        let ${paramName(m)} = same_map_nodes(${access}, |n| v.visit_node_hooked(Some(n)).expect("visitor removed a raw list element"));`);
            args.push(paramName(m));
            continue;
        }
        let call: string;
        if (m.visit) call = `v.visit_${snake(m.visit)}_hooked`;
        else if (m.listKind === "ModifierList") call = "v.visit_modifiers_hooked";
        else if (m.listKind === "NodeList" || m.type.baseKind() === "list") call = "v.visit_nodes_hooked";
        else call = "v.visit_node_hooked";
        if (m.visit === "modifiers") call = "v.visit_modifiers_hooked";
        const local = `${paramName(m)}_`;
        if (opt) w(`        let ${local} = ${call}(${access});`);
        else w(`        let ${local} = required_child(${call}(Some(${access})), ${access});`);
        args.push(local);
    }
    w(`        v.factory.update_${snake(node.name)}(node, ${args.join(", ")})`);
    w(`    }`);
    w(`}`);
    w();
}

function genClone(node: NodeType) {
    if (HAND_WRITTEN_DATA.has(node.name)) return;
    const l = layouts.get(node.name)!;
    const params = factoryParams(node);
    const args = params.map(p => memberValue(l, p.m, "self", "node")).join(", ");
    w(`impl ${node.name} {`);
    if (!args.includes("self.")) {
        w(`    #[expect(clippy::unused_self, reason = "Empty node data uses the same cloning interface as nodes with stored fields.")]`);
    }
    w(`    pub fn clone_node(&self, node: P<Node>, f: &NodeFactory) -> P<Node> {`);
    if (node.kindAliases.length > 0) {
        w(`        let updated = match node.kind() {`);
        w(`            Kind::${node.syntaxKindName} => f.new_${snake(node.name)}(${args}),`);
        for (const a of node.kindAliases) w(`            Kind::${a} => f.new_${snake(a)}(${args}),`);
        w(`            _ => panic!("unexpected kind in ${node.name}.clone_node: {:?}", node.kind()),`);
        w(`        };`);
        w(`        clone_node(updated, node, &f.hooks)`);
    }
    else {
        w(`        clone_node(f.new_${snake(node.name)}(${args}), node, &f.hooks)`);
    }
    w(`    }`);
    w(`}`);
    w();
}

function genIsFunctions(node: NodeType) {
    const kindTypes = node.kindTypes();
    if (node.kindType.kind === "typeParameter") {
        w(`pub fn is_${snake(node.name)}(node: P<Node>) -> bool {`);
        w(`    matches!(node.kind(), ${kindTypes.map(k => `Kind::${k.name}`).join(" | ")})`);
        w(`}`);
        w();
        return;
    }
    if (node.isMultiKind()) {
        for (const k of kindTypes) {
            w(`pub fn is_${snake(k.name)}(node: P<Node>) -> bool {`);
            w(`    node.kind() == Kind::${k.name}`);
            w(`}`);
            w();
        }
        return;
    }
    w(`pub fn is_${snake(node.name)}(node: P<Node>) -> bool {`);
    w(`    node.kind() == Kind::${node.syntaxKindName}`);
    w(`}`);
    w();
    for (const a of node.kindAliases) {
        w(`pub fn is_${snake(a)}(node: P<Node>) -> bool {`);
        w(`    node.kind() == Kind::${a}`);
        w(`}`);
        w();
    }
}

// Hand-written node data (not in ast.json, or handWritten there) that still participates in NodeData.
const EXTRA_DATA = ["FlowSwitchClauseData", "FlowReduceLabelData"];

function genNodeDataEnum() {
    w("// ── NodeData ──────────────────────────────────────────────────────────────");
    w();
    w("/// The node's data struct, as returned by `Node::data()` (the data itself lives right after the header).");
    w("#[derive(Clone, Copy)]");
    w("pub enum NodeData {");
    for (const n of nodes) {
        if (isEmptyLayout(layouts.get(n.name)!)) w(`    ${n.name},`);
        else w(`    ${n.name}(&'static ${n.name}),`);
    }
    for (const e of EXTRA_DATA) w(`    ${e}(&'static ${e}),`);
    w("}");
    w();
    w("/// Which data struct follows a node's header (one variant per `NodeData` variant).");
    w("#[repr(u8)]");
    w("#[derive(Clone, Copy, PartialEq, Eq, Debug)]");
    w("pub enum NodeDataTag {");
    for (const n of nodes) w(`    ${n.name},`);
    for (const e of EXTRA_DATA) w(`    ${e},`);
    w("}");
    w();
    w("impl NodeDataTag {");
    w("    /// The number of variants: every `u8` below it is a tag (`NodeHeaderWord::data_tag` checks this in debug builds).");
    w(`    pub(crate) const COUNT: u8 = ${nodes.length + EXTRA_DATA.length};`);
    w("}");
    w();
    for (const n of nodes) {
        if (isEmptyLayout(layouts.get(n.name)!)) continue;
        w(`impl NodePayload for ${n.name} {`);
        w(`    const TAG: NodeDataTag = NodeDataTag::${n.name};`);
        w(`}`);
    }
    for (const e of EXTRA_DATA) {
        w(`impl NodePayload for ${e} {`);
        w(`    const TAG: NodeDataTag = NodeDataTag::${e};`);
        w(`}`);
    }
    w();
}

// One match arm over `self.data_tag()`; `d.` in `expr` stands for the node's data struct.
function arm(name: string, expr: string): string {
    if (isEmptyLayout(layouts.get(name)!)) return `            NodeDataTag::${name} => ${expr},`;
    return `            NodeDataTag::${name} => ${expr.replace(/\bd\./g, `self.payload::<${name}>().`)},`;
}

function genNodeImpl() {
    w("// ── Node casts and generic dispatch ──────────────────────────────────────");
    w();
    w("impl Node {");
    w(`    /// The node's data struct.`);
    w(`    pub fn data(&self) -> NodeData {`);
    w(`        match self.data_tag() {`);
    for (const n of nodes) {
        if (isEmptyLayout(layouts.get(n.name)!)) w(`            NodeDataTag::${n.name} => NodeData::${n.name},`);
        else w(`            NodeDataTag::${n.name} => NodeData::${n.name}(self.payload()),`);
    }
    for (const e of EXTRA_DATA) w(`            NodeDataTag::${e} => NodeData::${e}(self.payload()),`);
    w(`        }`);
    w(`    }`);
    w();
    for (const n of nodes) {
        const fn = `as_${snake(n.name)}`;
        const empty = isEmptyLayout(layouts.get(n.name)!);
        w(`    #[inline]`);
        w(`    pub fn ${fn}(&self) -> &'static ${n.name} {`);
        w(`        assert!(self.data_tag() == NodeDataTag::${n.name}, "${fn} called on {:?}", self.kind());`);
        w(empty ? `        &${n.name}` : `        self.payload()`);
        w(`    }`);
    }
    for (const e of EXTRA_DATA) {
        const fn = `as_${snake(e)}`;
        w(`    #[inline]`);
        w(`    pub fn ${fn}(&self) -> &'static ${e} {`);
        w(`        assert!(self.data_tag() == NodeDataTag::${e}, "${fn} called on {:?}", self.kind());`);
        w(`        self.payload()`);
        w(`    }`);
    }
    w();

    // for_each_child dispatch
    w(`    pub fn for_each_child(&self, v: &mut dyn FnMut(P<Node>) -> bool) -> bool {`);
    w(`        self.for_each_child_static(v)`);
    w(`    }`);
    w();
    w(`    /// \`for_each_child\` compiled for one visitor type: with a closure, no dynamic call per child (the parser`);
    w(`    /// visits the children of every node it finishes).`);
    w(`    pub fn for_each_child_static<V: FnMut(P<Node>) -> bool + ?Sized>(&self, v: &mut V) -> bool {`);
    w(`        match self.data_tag() {`);
    for (const n of nodes) {
        if (!hasForEachChild(n)) continue;
        w(arm(n.name, `d.for_each_child(v)`));
    }
    w(`            _ => false,`);
    w(`        }`);
    w(`    }`);
    w();

    // visit_each_child dispatch
    w(`    pub fn visit_each_child(&self, v: &mut NodeVisitor) -> P<Node> {`);
    w(`        let node = self.as_p();`);
    w(`        match self.data_tag() {`);
    for (const n of nodes) {
        if (!hasForEachChild(n)) continue;
        w(arm(n.name, `d.visit_each_child(node, v)`));
    }
    w(`            _ => node,`);
    w(`        }`);
    w(`    }`);
    w();

    // clone_node dispatch
    w(`    pub fn clone_node(&self, f: &NodeFactory) -> P<Node> {`);
    w(`        let node = self.as_p();`);
    w(`        match self.data_tag() {`);
    for (const n of nodes) {
        if (isEmptyLayout(layouts.get(n.name)!)) w(arm(n.name, `${n.name}.clone_node(node, f)`));
        else w(arm(n.name, `d.clone_node(node, f)`));
    }
    w(`            _ => panic!("clone_node: unsupported node data for {:?}", self.kind()),`);
    w(`        }`);
    w(`    }`);
    w();

    // Name() / Modifiers()
    const withField = (fieldName: string) =>
        nodes.filter(n => n.name !== "SourceFile" && promoted(layouts.get(n.name)!).some(f => f.name === fieldName));
    // A match of constant offsets is lowered back to a jump table by LLVM. An explicit constant array
    // keeps dispatch a data lookup; Rust computes each offset for the target's actual field layout.
    const offsetTable = (entries: [string, string][], tuple = false) => {
        w(`        const OFFSETS: [${tuple ? "(usize, bool)" : "usize"}; ${nodes.length + EXTRA_DATA.length}] = {`);
        w(`            let mut offsets = [${tuple ? "(0, false)" : "0"}; ${nodes.length + EXTRA_DATA.length}];`);
        for (const [name, value] of entries) w(`            offsets[NodeDataTag::${name} as usize] = ${value};`);
        w(`            offsets`);
        w(`        };`);
        w(`        let ${tuple ? "(offset, optional)" : "offset"} = OFFSETS[self.data_tag() as usize];`);
    };
    w(`    pub fn name(&self) -> Option<P<Node>> {`);
    offsetTable(withField("name").map(n => {
        const f = findFlat(layouts.get(n.name)!, "name");
        if (f.cell || f.rare || !["P<Node>", "Option<P<Node>>"].includes(f.ty)) {
            throw new Error(`name offset needs an immutable inline node field: ${n.name}`);
        }
        return [n.name, `(std::mem::offset_of!(NodeAlloc<${n.name}>, data.${f.path}), ${f.ty.startsWith("Option")})`];
    }), true);
    w(`        if offset == 0 { return None; }`);
    w(`        if optional {`);
    w(`            // SAFETY: the tag selects this allocation's optional name field. Both allocation forms`);
    w(`            // have the same data offset, and the arena keeps the field alive.`);
    w(`            unsafe { *tsrs_core::ptr::arena_neighbor::<_, Option<P<Node>>>(self, offset as isize) }`);
    w(`        } else {`);
    w(`            // SAFETY: as above, but the selected field is a required name, stored as P<Node>.`);
    w(`            Some(unsafe { *tsrs_core::ptr::arena_neighbor::<_, P<Node>>(self, offset as isize) })`);
    w(`        }`);
    w(`    }`);
    w();
    w(`    pub fn modifiers(&self) -> Option<P<ModifierList>> {`);
    offsetTable(withField("modifiers").map(n => {
        const f = findFlat(layouts.get(n.name)!, "modifiers");
        if (!f.cell || f.rare || f.ty !== "Option<P<ModifierList>>") throw new Error(`unexpected modifiers field: ${n.name}`);
        return [n.name, `std::mem::offset_of!(NodeAlloc<${n.name}>, data.${f.path})`];
    }));
    w(`        if offset == 0 { return None; }`);
    w(`        // SAFETY: the tag selects the concrete allocation and its modifiers cell. Both allocation`);
    w(`        // forms have the same data offset; read through OwnedCell to preserve its access checks.`);
    w(`        unsafe { tsrs_core::ptr::arena_neighbor::<_, OwnedCell<Option<P<ModifierList>>>>(self, offset as isize) }.get()`);
    w(`    }`);
    w();
    w(`    pub(crate) fn set_modifiers_data(&self, modifiers: Option<P<ModifierList>>) {`);
    w(`        match self.data_tag() {`);
    for (const n of withField("modifiers")) w(arm(n.name, `{ d.set_modifiers(modifiers); }`));
    w(`            _ => {}`);
    w(`        }`);
    w(`    }`);
    w();

    // The flow node of every kind is read and written through `Node::flow_node()` / `set_flow_node()`
    // (identifiers keep theirs packed, without a `FlowNodeBase`), so this one is crate-private.
    const baseAccessors: [string, string][] = [
        ["flow_node_base", "FlowNodeBase"],
        ["declaration_data", "DeclarationBase"],
        ["exportable_data", "ExportableBase"],
        ["locals_container_data", "LocalsContainerBase"],
        ["function_like_data", "FunctionLikeBase"],
        ["class_like_data", "ClassLikeBase"],
        ["body_data", "BodyBase"],
        ["literal_like_data", "LiteralLikeNodeBase"],
        ["template_literal_like_data", "TemplateLiteralLikeNodeBase"],
    ];
    for (const [fn, base] of baseAccessors) {
        w(`    ${fn === "flow_node_base" ? "pub(crate)" : "pub"} fn ${fn}(&self) -> Option<&'static ${base}> {`);
        const entries: [string, string][] = [];
        for (const n of nodes) {
            if (HAND_WRITTEN_DATA.has(n.name)) continue;
            const p = basePath(layouts.get(n.name)!, base);
            if (p) entries.push([n.name, `std::mem::offset_of!(NodeAlloc<${n.name}>, data.${p})`]);
        }
        offsetTable(entries);
        w(`        if offset == 0 { return None; }`);
        w(`        // SAFETY: the tag selects the concrete allocation and its embedded ${base}. Both`);
        w(`        // allocation forms have the same data offset, and the arena keeps the field alive.`);
        w(`        Some(unsafe { tsrs_core::ptr::arena_neighbor(self, offset as isize) })`);
        w(`    }`);
        w();
    }
    w("}");
    w();
}

function genFactory() {
    w("// ── NodeFactory constructors ─────────────────────────────────────────────");
    w();
    w("impl NodeFactory {");
    for (const n of nodes) {
        if (n.handWritten) continue;
        genNewFactory(n);
        genUpdateFactory(n);
    }
    w("}");
    w();
}

function genKindAliasGuards() {
    w("// ── Kind alias guards ────────────────────────────────────────────────────");
    w();
    const skip = new Set(["isJSDocKind"]);
    for (const g of api.kindGuards()) {
        const tsName = kindGuardName(g.aliasName);
        if (skip.has(tsName)) continue;
        const fn = snake(tsName);
        w(`pub fn ${fn}(kind: Kind) -> bool {`);
        if (g.type === "range") {
            w(`    kind >= Kind::${screamingSnakeCase(g.first)} && kind <= Kind::${screamingSnakeCase(g.last)}`);
        }
        else {
            const expanded = api.expandKindAliasMembers(g.aliasName);
            w(`    matches!(kind, ${expanded.map(k => `Kind::${k.name}`).join(" | ")})`);
        }
        w(`}`);
        w();
    }
}

header();
genBaseStructs();
w("// ── Node data structs ─────────────────────────────────────────────────────");
w();
for (const n of nodes) {
    if (n.handWritten) continue;
    genStruct(n);
}
genNodeDataEnum();
genNodeImpl();
genFactory();
for (const n of nodes) {
    if (n.handWritten) continue;
    genForEachChild(n);
    genVisitEachChild(n);
    genClone(n);
}
w("// ── is_* predicates ──────────────────────────────────────────────────────");
w();
for (const n of nodes) genIsFunctions(n);
genKindAliasGuards();

fs.writeFileSync(outPath, rustSurface(out.join("\n"), SURFACE));
console.log(`Widened to Option (observed nil): ${[...new Set(widenedToOption)].sort().join(", ")}`);
console.log(`Wrote ${outPath}`);
