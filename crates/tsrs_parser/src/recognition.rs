//! Shared token-only recognition for lists, declarations, expressions and types.

#![forbid(unsafe_code)]

use crate::lexical::LexicalScanner;
use crate::parser_1::ParsingContext;
use crate::tokens::TokenParser;
use crate::utilities::{
    token_is_identifier_or_keyword, token_is_identifier_or_keyword_or_greater_than,
};
use std::fmt::Display;
use tsrs_ast::{self as ast, Kind, NodeFlags, OperatorPrecedence};
use tsrs_diagnostics::{self as diagnostics, Message};
use tsrs_scanner as scanner;

pub trait RecognitionParser<'src>: TokenParser<'src> {
    fn has_preceding_line_break(&self) -> bool {
        self.scanner().has_preceding_line_break()
    }
    fn parse_error_at_current_token(&mut self, message: &'static Message, args: &[&dyn Display]) {
        self.error_at(self.scanner().token_range(), message, args);
    }
    fn is_list_element(
        &mut self,
        parsing_context: ParsingContext,
        in_error_recovery: bool,
    ) -> bool {
        match parsing_context {
            ParsingContext::SourceElements
            | ParsingContext::BlockStatements
            | ParsingContext::SwitchClauseStatements => {
                // If we're in error recovery, then we don't want to treat ';' as an empty statement.
                // The problem is that ';' can show up in far too many contexts, and if we see one
                // and assume it's a statement, then we may bail out inappropriately from whatever
                // we're parsing.  For example, if we have a semicolon in the middle of a class, then
                // we really don't want to assume the class is over and we're on a statement in the
                // outer module.  We just want to consume and move on.
                !(self.token() == Kind::SemicolonToken && in_error_recovery)
                    && self.is_start_of_statement()
            }
            ParsingContext::SwitchClauses => {
                self.token() == Kind::CaseKeyword || self.token() == Kind::DefaultKeyword
            }
            ParsingContext::TypeMembers => {
                self.look_ahead(|parser| parser.scan_type_member_start())
            }
            ParsingContext::ClassMembers => {
                // We allow semicolons as class elements (as specified by ES6) as long as we're
                // not in error recovery.  If we're in error recovery, we don't want an errant
                // semicolon to be treated as a class member (since they're almost always used
                // for statements.
                self.look_ahead(|parser| parser.scan_class_member_start())
                    || self.token() == Kind::SemicolonToken && !in_error_recovery
            }
            ParsingContext::EnumMembers => {
                // Include open bracket computed properties. This technically also lets in indexers,
                // which would be a candidate for improved error reporting.
                self.token() == Kind::OpenBracketToken || self.is_literal_property_name()
            }
            ParsingContext::ObjectLiteralMembers => match self.token() {
                // Not an object literal member, but don't want to close the object (see `tests/cases/fourslash/completionsDotInObjectLiteral.ts`)
                Kind::OpenBracketToken
                | Kind::AsteriskToken
                | Kind::DotDotDotToken
                | Kind::DotToken => true,
                _ => self.is_literal_property_name(),
            },
            ParsingContext::RestProperties => self.is_literal_property_name(),
            ParsingContext::ObjectBindingElements => {
                self.token() == Kind::OpenBracketToken
                    || self.token() == Kind::DotDotDotToken
                    || self.is_literal_property_name()
            }
            ParsingContext::ImportAttributes => self.is_import_attribute_name(),
            ParsingContext::HeritageClauseElement => {
                // If we see `{ ... }` then only consume it as an expression if it is followed by `,` or `{`
                // That way we won't consume the body of a class in its heritage clause.
                if self.token() == Kind::OpenBraceToken {
                    return self.is_valid_heritage_clause_object_literal();
                }
                if !in_error_recovery {
                    return self.is_start_of_left_hand_side_expression()
                        && !self.is_heritage_clause_extends_or_implements_keyword();
                }
                // If we're in error recovery we tighten up what we're willing to match.
                // That way we don't treat something like "this" as a valid heritage clause
                // element during recovery.
                self.is_identifier() && !self.is_heritage_clause_extends_or_implements_keyword()
            }
            ParsingContext::VariableDeclarations => {
                self.is_binding_identifier_or_private_identifier_or_pattern()
            }
            ParsingContext::ArrayBindingElements => {
                self.token() == Kind::CommaToken
                    || self.token() == Kind::DotDotDotToken
                    || self.is_binding_identifier_or_private_identifier_or_pattern()
            }
            ParsingContext::TypeParameters => {
                self.token() == Kind::InKeyword
                    || self.token() == Kind::ConstKeyword
                    || self.is_identifier()
            }
            ParsingContext::ArrayLiteralMembers | ParsingContext::ArgumentExpressions => {
                if parsing_context == ParsingContext::ArrayLiteralMembers {
                    // Not an array literal member, but don't want to close the array (see `tests/cases/fourslash/completionsDotInArrayLiteralInObjectLiteral.ts`)
                    if self.token() == Kind::CommaToken || self.token() == Kind::DotToken {
                        return true;
                    }
                }
                self.token() == Kind::DotDotDotToken || self.is_start_of_expression()
            }
            ParsingContext::Parameters => {
                self.is_start_of_parameter(false /*isJSDocParameter*/)
            }
            ParsingContext::JSDocParameters => {
                self.is_start_of_parameter(true /*isJSDocParameter*/)
            }
            ParsingContext::TypeArguments | ParsingContext::TupleElementTypes => {
                self.token() == Kind::CommaToken
                    || self.is_start_of_type(false /*inStartOfParameter*/)
            }
            ParsingContext::HeritageClauses => self.is_heritage_clause(),
            ParsingContext::ImportOrExportSpecifiers => {
                // bail out if the next token is [FromKeyword StringLiteral].
                // That means we're in something like `import { from "mod"`. Stop here can give better error message.
                if self.token() == Kind::FromKeyword
                    && self.look_ahead(|parser| parser.next_token_is_token_string_literal())
                {
                    return false;
                }
                if self.token() == Kind::StringLiteral {
                    return true; // For "arbitrary module namespace identifiers"
                }
                token_is_identifier_or_keyword(self.token())
            }
            ParsingContext::JsxAttributes => {
                token_is_identifier_or_keyword(self.token()) || self.token() == Kind::OpenBraceToken
            }
            ParsingContext::JsxChildren | ParsingContext::JSDocComment => true,
            ParsingContext::Count => panic!("Unhandled case in isListElement"),
        }
    }

    fn is_start_of_statement(&mut self) -> bool {
        match self.token() {
            // 'catch' and 'finally' do not actually indicate that the code is part of a statement,
            // however, we say they are here so that we may gracefully parse them and error later.
            Kind::AtToken
            | Kind::SemicolonToken
            | Kind::OpenBraceToken
            | Kind::VarKeyword
            | Kind::LetKeyword
            | Kind::UsingKeyword
            | Kind::FunctionKeyword
            | Kind::ClassKeyword
            | Kind::EnumKeyword
            | Kind::IfKeyword
            | Kind::DoKeyword
            | Kind::WhileKeyword
            | Kind::ForKeyword
            | Kind::ContinueKeyword
            | Kind::BreakKeyword
            | Kind::ReturnKeyword
            | Kind::WithKeyword
            | Kind::SwitchKeyword
            | Kind::ThrowKeyword
            | Kind::TryKeyword
            | Kind::DebuggerKeyword
            | Kind::CatchKeyword
            | Kind::FinallyKeyword
            // When these don't start a declaration, they're an identifier in an expression statement.
            | Kind::AsyncKeyword
            | Kind::DeclareKeyword
            | Kind::InterfaceKeyword
            | Kind::ModuleKeyword
            | Kind::NamespaceKeyword
            | Kind::TypeKeyword
            | Kind::GlobalKeyword
            | Kind::DeferKeyword
            | Kind::SourceKeyword => true,
            Kind::ImportKeyword => {
                self.is_start_of_declaration()
                    || self.is_next_token_open_paren_or_less_than_or_dot()
            }
            Kind::ConstKeyword | Kind::ExportKeyword => self.is_start_of_declaration(),
            Kind::AccessorKeyword
            | Kind::PublicKeyword
            | Kind::PrivateKeyword
            | Kind::ProtectedKeyword
            | Kind::StaticKeyword
            | Kind::ReadonlyKeyword => {
                // When these don't start a declaration, they may be the start of a class member if an identifier
                // immediately follows. Otherwise they're an identifier in an expression statement.
                self.is_start_of_declaration()
                    || !self.look_ahead(|parser| {
                        parser.next_token_is_identifier_or_keyword_on_same_line()
                    })
            }
            _ => self.is_start_of_expression(),
        }
    }

    fn is_start_of_declaration(&mut self) -> bool {
        self.look_ahead(|parser| parser.scan_start_of_declaration())
    }

    fn scan_start_of_declaration(&mut self) -> bool {
        loop {
            match self.token() {
                Kind::VarKeyword
                | Kind::LetKeyword
                | Kind::ConstKeyword
                | Kind::FunctionKeyword
                | Kind::ClassKeyword
                | Kind::EnumKeyword => {
                    return true;
                }
                Kind::UsingKeyword => return self.is_using_declaration(),
                Kind::AwaitKeyword => return self.is_await_using_declaration(),
                // 'declare', 'module', 'namespace', 'interface'* and 'type' are all legal JavaScript identifiers;
                // however, an identifier cannot be followed by another identifier on the same line. This is what we
                // count on to parse out the respective declarations. For instance, we exploit this to say that
                //
                //    namespace n
                //
                // can be none other than the beginning of a namespace declaration, but need to respect that JavaScript sees
                //
                //    namespace
                //    n
                //
                // as the identifier 'namespace' on one line followed by the identifier 'n' on another.
                // We need to look one token ahead to see if it permissible to try parsing a declaration.
                //
                // *Note*: 'interface' is actually a strict mode reserved word. So while
                //
                //   "use strict"
                //   interface
                //   I {}
                //
                // could be legal, it would add complexity for very little gain.
                Kind::InterfaceKeyword
                | Kind::TypeKeyword
                | Kind::DeferKeyword
                | Kind::SourceKeyword => {
                    return self.next_token_is_identifier_on_same_line();
                }
                Kind::ModuleKeyword | Kind::NamespaceKeyword => {
                    return self.next_token_is_identifier_or_string_literal_on_same_line();
                }
                Kind::AbstractKeyword
                | Kind::AccessorKeyword
                | Kind::AsyncKeyword
                | Kind::DeclareKeyword
                | Kind::PrivateKeyword
                | Kind::ProtectedKeyword
                | Kind::PublicKeyword
                | Kind::ReadonlyKeyword => {
                    let previous_token = self.token();
                    self.next_token();
                    // ASI takes effect for this modifier.
                    if self.has_preceding_line_break() {
                        return false;
                    }
                    if previous_token == Kind::DeclareKeyword && self.token() == Kind::TypeKeyword {
                        // If we see 'declare type', then commit to parsing a type alias. parseTypeAliasDeclaration will
                        // report Line_break_not_permitted_here if needed.
                        return true;
                    }
                    continue;
                }
                Kind::GlobalKeyword => {
                    self.next_token();
                    return self.token() == Kind::OpenBraceToken
                        || self.token() == Kind::Identifier
                        || self.token() == Kind::ExportKeyword;
                }
                Kind::ImportKeyword => {
                    self.next_token();
                    return self.token() == Kind::DeferKeyword
                        || self.token() == Kind::SourceKeyword
                        || self.token() == Kind::StringLiteral
                        || self.token() == Kind::AsteriskToken
                        || self.token() == Kind::OpenBraceToken
                        || token_is_identifier_or_keyword(self.token());
                }
                Kind::ExportKeyword => {
                    self.next_token();
                    if self.token() == Kind::EqualsToken
                        || self.token() == Kind::AsteriskToken
                        || self.token() == Kind::OpenBraceToken
                        || self.token() == Kind::DefaultKeyword
                        || self.token() == Kind::AsKeyword
                        || self.token() == Kind::AtToken
                    {
                        return true;
                    }
                    if self.token() == Kind::TypeKeyword {
                        self.next_token();
                        return self.token() == Kind::AsteriskToken
                            || self.token() == Kind::OpenBraceToken
                            || self.is_identifier() && !self.has_preceding_line_break();
                    }
                    continue;
                }
                Kind::StaticKeyword => {
                    self.next_token();
                    continue;
                }
                _ => {}
            }
            return false;
        }
    }

    fn is_using_declaration(&mut self) -> bool {
        // 'using' always starts a lexical declaration if followed by an identifier. We also eagerly parse
        // |ObjectBindingPattern| so that we can report a grammar error during check. We don't parse out
        // |ArrayBindingPattern| since it potentially conflicts with element access (i.e., `using[x]`).
        self.look_ahead(|p| {
            p.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(
                false, /*disallowOf*/
            )
        })
    }

    fn is_await_using_declaration(&mut self) -> bool {
        self.look_ahead(|parser| parser.next_is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line())
    }

    fn next_is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line(
        &mut self,
    ) -> bool {
        self.next_token() == Kind::UsingKeyword
            && self.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(
                false, /*disallowOf*/
            )
    }

    fn next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(
        &mut self,
        disallow_of: bool,
    ) -> bool {
        self.next_token();
        if disallow_of && self.token() == Kind::OfKeyword {
            return self
                .look_ahead(|parser| parser.next_token_is_equals_or_semicolon_or_colon_token());
        }
        (self.is_binding_identifier() || self.token() == Kind::OpenBraceToken)
            && !self.has_preceding_line_break()
    }

    fn next_token_is_equals_or_semicolon_or_colon_token(&mut self) -> bool {
        self.next_token();
        self.token() == Kind::EqualsToken
            || self.token() == Kind::SemicolonToken
            || self.token() == Kind::ColonToken
    }

    fn next_token_is_identifier_on_same_line(&mut self) -> bool {
        self.next_token();
        self.is_identifier() && !self.has_preceding_line_break()
    }

    fn next_token_is_identifier_or_string_literal_on_same_line(&mut self) -> bool {
        self.next_token();
        (self.is_identifier() || self.token() == Kind::StringLiteral)
            && !self.has_preceding_line_break()
    }

    fn is_next_token_open_paren_or_less_than_or_dot(&mut self) -> bool {
        self.look_ahead(|parser| parser.next_token_is_open_paren_or_less_than_or_dot())
    }

    fn next_token_is_open_paren_or_less_than_or_dot(&mut self) -> bool {
        matches!(self.next_token(), Kind::OpenParenToken | Kind::LessThanToken | Kind::DotToken)
    }

    fn next_token_is_identifier_or_keyword_on_same_line(&mut self) -> bool {
        self.next_token_is_identifier_or_keyword() && !self.has_preceding_line_break()
    }

    fn next_token_is_identifier_or_keyword(&mut self) -> bool {
        token_is_identifier_or_keyword(self.next_token())
    }

    fn is_start_of_expression(&mut self) -> bool {
        if self.is_start_of_left_hand_side_expression() {
            return true;
        }
        match self.token() {
            Kind::PlusToken
            | Kind::MinusToken
            | Kind::TildeToken
            | Kind::ExclamationToken
            | Kind::DeleteKeyword
            | Kind::TypeOfKeyword
            | Kind::VoidKeyword
            | Kind::PlusPlusToken
            | Kind::MinusMinusToken
            | Kind::LessThanToken
            | Kind::AwaitKeyword
            | Kind::YieldKeyword
            | Kind::PrivateIdentifier
            | Kind::AtToken => {
                // Yield/await always starts an expression.  Either it is an identifier (in which case
                // it is definitely an expression).  Or it's a keyword (either because we're in
                // a generator or async function, or in strict mode (or both)) and it started a yield or await expression.
                return true;
            }
            _ => {}
        }
        // Error tolerance.  If we see the start of some binary operator, we consider
        // that the start of an expression.  That way we'll parse out a missing identifier,
        // give a good message about an identifier being missing, and then consume the
        // rest of the binary expression.
        if self.is_binary_operator() {
            return true;
        }
        self.is_identifier()
    }

    fn is_start_of_left_hand_side_expression(&mut self) -> bool {
        match self.token() {
            Kind::ThisKeyword
            | Kind::SuperKeyword
            | Kind::NullKeyword
            | Kind::TrueKeyword
            | Kind::FalseKeyword
            | Kind::NumericLiteral
            | Kind::BigIntLiteral
            | Kind::StringLiteral
            | Kind::NoSubstitutionTemplateLiteral
            | Kind::TemplateHead
            | Kind::OpenParenToken
            | Kind::OpenBracketToken
            | Kind::OpenBraceToken
            | Kind::FunctionKeyword
            | Kind::ClassKeyword
            | Kind::NewKeyword
            | Kind::SlashToken
            | Kind::SlashEqualsToken
            | Kind::Identifier => return true,
            Kind::ImportKeyword => return self.is_next_token_open_paren_or_less_than_or_dot(),
            _ => {}
        }
        self.is_identifier()
    }

    fn is_binary_operator(&self) -> bool {
        if self.in_disallow_in_context() && self.token() == Kind::InKeyword {
            return false;
        }
        ast::get_binary_operator_precedence(self.token()) != OperatorPrecedence::Invalid
    }

    fn in_disallow_in_context(&self) -> bool {
        self.context_flags().intersects(NodeFlags::DisallowInContext)
    }

    fn scan_type_member_start(&mut self) -> bool {
        // Return true if we have the start of a signature member
        if self.token() == Kind::OpenParenToken
            || self.token() == Kind::LessThanToken
            || self.token() == Kind::GetKeyword
            || self.token() == Kind::SetKeyword
        {
            return true;
        }
        let mut id_token = false;
        // Eat up all modifiers, but hold on to the last one in case it is actually an identifier
        while ast::is_modifier_kind(self.token()) {
            id_token = true;
            self.next_token();
        }
        // Index signatures and computed property names are type members
        if self.token() == Kind::OpenBracketToken {
            return true;
        }
        // Try to get the first property-like token following all modifiers
        if self.is_literal_property_name() {
            id_token = true;
            self.next_token();
        }
        // If we were able to get any potential identifier, check that it is
        // the start of a member declaration
        if id_token {
            return self.token() == Kind::OpenParenToken
                || self.token() == Kind::LessThanToken
                || self.token() == Kind::QuestionToken
                || self.token() == Kind::ColonToken
                || self.token() == Kind::CommaToken
                || self.can_parse_semicolon();
        }
        false
    }

    fn is_literal_property_name(&self) -> bool {
        token_is_identifier_or_keyword(self.token())
            || self.token() == Kind::StringLiteral
            || self.token() == Kind::NumericLiteral
            || self.token() == Kind::BigIntLiteral
    }

    fn can_parse_semicolon(&self) -> bool {
        // If there's a real semicolon, then we can always parse it out.
        // We can parse out an optional semicolon in ASI cases in the following cases.
        self.token() == Kind::SemicolonToken
            || self.token() == Kind::CloseBraceToken
            || self.token() == Kind::EndOfFile
            || self.has_preceding_line_break()
    }

    fn scan_class_member_start(&mut self) -> bool {
        let mut id_token = Kind::Unknown;
        if self.token() == Kind::AtToken {
            return true;
        }
        // Eat up all modifiers, but hold on to the last one in case it is actually an identifier.
        while ast::is_modifier_kind(self.token()) {
            id_token = self.token();
            // If the idToken is a class modifier (protected, private, public, and static), it is
            // certain that we are starting to parse class member. This allows better error recovery
            // Example:
            //      public foo() ...     // true
            //      public @dec blah ... // true; we will then report an error later
            //      export public ...    // true; we will then report an error later
            if ast::is_class_member_modifier(id_token) {
                return true;
            }
            self.next_token();
        }
        if self.token() == Kind::AsteriskToken {
            return true;
        }
        // Try to get the first property-like token following all modifiers.
        // This can either be an identifier or the 'get' or 'set' keywords.
        if self.is_literal_property_name() {
            id_token = self.token();
            self.next_token();
        }
        // Index signatures and computed properties are class members; we can parse.
        if self.token() == Kind::OpenBracketToken {
            return true;
        }
        // If we were able to get any potential identifier...
        if id_token != Kind::Unknown {
            // If we have a non-keyword identifier, or if we have an accessor, then it's safe to parse.
            if !ast::is_keyword(id_token)
                || id_token == Kind::SetKeyword
                || id_token == Kind::GetKeyword
            {
                return true;
            }
            // If it *is* a keyword, but not an accessor, check a little farther along
            // to see if it should actually be parsed as a class member.
            match self.token() {
                Kind::OpenParenToken // Method declaration
                | Kind::LessThanToken // Generic Method declaration
                | Kind::ExclamationToken // Non-null assertion on property name
                | Kind::ColonToken // Type Annotation for declaration
                | Kind::EqualsToken // Initializer for declaration
                | Kind::QuestionToken => {
                    // Not valid, but permitted so that it gets caught later on.
                    return true;
                }
                _ => {}
            }
            // Covers
            //  - Semicolons     (declaration termination)
            //  - Closing braces (end-of-class, must be declaration)
            //  - End-of-files   (not valid, but permitted so that it gets caught later on)
            //  - Line-breaks    (enabling *automatic semicolon insertion*)
            return self.can_parse_semicolon();
        }
        false
    }

    fn is_import_attribute_name(&self) -> bool {
        token_is_identifier_or_keyword(self.token()) || self.token() == Kind::StringLiteral
    }

    fn is_valid_heritage_clause_object_literal(&mut self) -> bool {
        self.look_ahead(|parser| parser.next_is_valid_heritage_clause_object_literal())
    }

    fn next_is_valid_heritage_clause_object_literal(&mut self) -> bool {
        if self.next_token() == Kind::CloseBraceToken {
            // if we see "extends {}" then only treat the {} as what we're extending (and not
            // the class body) if we have:
            //
            //      extends {} {
            //      extends {},
            //      extends {} extends
            //      extends {} implements
            let next = self.next_token();
            return next == Kind::CommaToken
                || next == Kind::OpenBraceToken
                || next == Kind::ExtendsKeyword
                || next == Kind::ImplementsKeyword;
        }
        true
    }

    fn is_heritage_clause_extends_or_implements_keyword(&mut self) -> bool {
        self.is_heritage_clause() && self.look_ahead(|parser| parser.next_is_start_of_expression())
    }

    fn is_heritage_clause(&self) -> bool {
        self.token() == Kind::ExtendsKeyword || self.token() == Kind::ImplementsKeyword
    }

    fn next_is_start_of_expression(&mut self) -> bool {
        self.next_token();
        self.is_start_of_expression()
    }

    fn is_binding_identifier_or_private_identifier_or_pattern(&self) -> bool {
        self.token() == Kind::OpenBraceToken
            || self.token() == Kind::OpenBracketToken
            || self.token() == Kind::PrivateIdentifier
            || self.is_binding_identifier()
    }

    fn is_start_of_parameter(&mut self, is_jsdoc_parameter: bool) -> bool {
        self.token() == Kind::DotDotDotToken
            || self.is_binding_identifier_or_private_identifier_or_pattern()
            || ast::is_modifier_kind(self.token())
            || self.token() == Kind::AtToken
            || self.is_start_of_type(!is_jsdoc_parameter /*inStartOfParameter*/)
    }

    fn is_start_of_type(&mut self, in_start_of_parameter: bool) -> bool {
        match self.token() {
            Kind::AnyKeyword
            | Kind::UnknownKeyword
            | Kind::StringKeyword
            | Kind::NumberKeyword
            | Kind::BigIntKeyword
            | Kind::BooleanKeyword
            | Kind::ReadonlyKeyword
            | Kind::SymbolKeyword
            | Kind::UniqueKeyword
            | Kind::VoidKeyword
            | Kind::UndefinedKeyword
            | Kind::NullKeyword
            | Kind::ThisKeyword
            | Kind::TypeOfKeyword
            | Kind::NeverKeyword
            | Kind::OpenBraceToken
            | Kind::OpenBracketToken
            | Kind::LessThanToken
            | Kind::BarToken
            | Kind::AmpersandToken
            | Kind::NewKeyword
            | Kind::StringLiteral
            | Kind::NumericLiteral
            | Kind::BigIntLiteral
            | Kind::TrueKeyword
            | Kind::FalseKeyword
            | Kind::ObjectKeyword
            | Kind::AsteriskToken
            | Kind::QuestionToken
            | Kind::ExclamationToken
            | Kind::DotDotDotToken
            | Kind::InferKeyword
            | Kind::ImportKeyword
            | Kind::AssertsKeyword
            | Kind::NoSubstitutionTemplateLiteral
            | Kind::TemplateHead => return true,
            Kind::FunctionKeyword => return !in_start_of_parameter,
            Kind::MinusToken => {
                return !in_start_of_parameter
                    && self.look_ahead(|parser| parser.next_token_is_numeric_or_big_int_literal());
            }
            Kind::OpenParenToken => {
                // Only consider '(' the start of a type if followed by ')', '...', an identifier, a modifier,
                // or something that starts a type. We don't want to consider things like '(1)' a type.
                return !in_start_of_parameter
                    && self.look_ahead(|parser| parser.next_is_parenthesized_or_function_type());
            }
            _ => {}
        }
        self.is_identifier()
    }

    fn next_token_is_numeric_or_big_int_literal(&mut self) -> bool {
        self.next_token();
        self.token() == Kind::NumericLiteral || self.token() == Kind::BigIntLiteral
    }

    fn next_is_parenthesized_or_function_type(&mut self) -> bool {
        self.next_token();
        self.token() == Kind::CloseParenToken
            || self.is_start_of_parameter(false /*isJSDocParameter*/)
            || self.is_start_of_type(false /*inStartOfParameter*/)
    }

    fn next_token_is_token_string_literal(&mut self) -> bool {
        self.next_token() == Kind::StringLiteral
    }

    fn is_list_terminator(&mut self, kind: ParsingContext) -> bool {
        if self.token() == Kind::EndOfFile {
            return true;
        }
        match kind {
            ParsingContext::BlockStatements
            | ParsingContext::SwitchClauses
            | ParsingContext::TypeMembers
            | ParsingContext::ClassMembers
            | ParsingContext::EnumMembers
            | ParsingContext::ObjectLiteralMembers
            | ParsingContext::ObjectBindingElements
            | ParsingContext::ImportOrExportSpecifiers
            | ParsingContext::ImportAttributes => self.token() == Kind::CloseBraceToken,
            ParsingContext::SwitchClauseStatements => {
                self.token() == Kind::CloseBraceToken
                    || self.token() == Kind::CaseKeyword
                    || self.token() == Kind::DefaultKeyword
            }
            ParsingContext::HeritageClauseElement => {
                self.token() == Kind::OpenBraceToken
                    || self.token() == Kind::ExtendsKeyword
                    || self.token() == Kind::ImplementsKeyword
            }
            ParsingContext::VariableDeclarations => {
                // If we can consume a semicolon (either explicitly, or with ASI), then consider us done
                // with parsing the list of variable declarators.
                // In the case where we're parsing the variable declarator of a 'for-in' statement, we
                // are done if we see an 'in' keyword in front of us. Same with for-of
                // ERROR RECOVERY TWEAK:
                // For better error recovery, if we see an '=>' then we just stop immediately.  We've got an
                // arrow function here and it's going to be very unlikely that we'll resynchronize and get
                // another variable declaration.
                self.can_parse_semicolon()
                    || self.token() == Kind::InKeyword
                    || self.token() == Kind::OfKeyword
                    || self.token() == Kind::EqualsGreaterThanToken
            }
            ParsingContext::TypeParameters => {
                // Tokens other than '>' are here for better error recovery
                self.token() == Kind::GreaterThanToken
                    || self.token() == Kind::OpenParenToken
                    || self.token() == Kind::OpenBraceToken
                    || self.token() == Kind::ExtendsKeyword
                    || self.token() == Kind::ImplementsKeyword
            }
            ParsingContext::ArgumentExpressions => {
                // Tokens other than ')' are here for better error recovery
                self.token() == Kind::CloseParenToken || self.token() == Kind::SemicolonToken
            }
            ParsingContext::ArrayLiteralMembers
            | ParsingContext::TupleElementTypes
            | ParsingContext::ArrayBindingElements => self.token() == Kind::CloseBracketToken,
            ParsingContext::JSDocParameters
            | ParsingContext::Parameters
            | ParsingContext::RestProperties => {
                // Tokens other than ')' and ']' (the latter for index signatures) are here for better error recovery
                self.token() == Kind::CloseParenToken || self.token() == Kind::CloseBracketToken /*|| token == ast.KindOpenBraceToken*/
            }
            ParsingContext::TypeArguments => {
                // All other tokens should cause the type-argument to terminate except comma token
                self.token() != Kind::CommaToken
            }
            ParsingContext::HeritageClauses => {
                self.token() == Kind::OpenBraceToken || self.token() == Kind::CloseBraceToken
            }
            ParsingContext::JsxAttributes => {
                self.token() == Kind::GreaterThanToken || self.token() == Kind::SlashToken
            }
            ParsingContext::JsxChildren => {
                self.token() == Kind::LessThanToken
                    && self.look_ahead(|parser| parser.next_token_is_slash())
            }
            _ => false,
        }
    }

    fn next_token_is_slash(&mut self) -> bool {
        self.next_token() == Kind::SlashToken
    }

    fn parsing_context_errors(&mut self, context: ParsingContext) {
        match context {
            ParsingContext::SourceElements => {
                if self.token() == Kind::DefaultKeyword {
                    self.parse_error_at_current_token(&diagnostics::X_0_EXPECTED, &[&"export"]);
                } else {
                    self.parse_error_at_current_token(
                        &diagnostics::DECLARATION_OR_STATEMENT_EXPECTED,
                        &[],
                    );
                }
            }
            ParsingContext::BlockStatements => {
                self.parse_error_at_current_token(
                    &diagnostics::DECLARATION_OR_STATEMENT_EXPECTED,
                    &[],
                );
            }
            ParsingContext::SwitchClauses => {
                self.parse_error_at_current_token(&diagnostics::X_CASE_OR_DEFAULT_EXPECTED, &[]);
            }
            ParsingContext::SwitchClauseStatements => {
                self.parse_error_at_current_token(&diagnostics::STATEMENT_EXPECTED, &[]);
            }
            ParsingContext::RestProperties | ParsingContext::TypeMembers => {
                self.parse_error_at_current_token(
                    &diagnostics::PROPERTY_OR_SIGNATURE_EXPECTED,
                    &[],
                );
            }
            ParsingContext::ClassMembers => {
                self.parse_error_at_current_token(&diagnostics::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED, &[]);
            }
            ParsingContext::EnumMembers => {
                self.parse_error_at_current_token(&diagnostics::ENUM_MEMBER_EXPECTED, &[]);
            }
            ParsingContext::HeritageClauseElement => {
                self.parse_error_at_current_token(&diagnostics::EXPRESSION_EXPECTED, &[]);
            }
            ParsingContext::VariableDeclarations => {
                if ast::is_keyword(self.token()) {
                    let text = scanner::token_to_string(self.token());
                    self.parse_error_at_current_token(
                        &diagnostics::X_0_IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
                        &[&text],
                    );
                } else {
                    self.parse_error_at_current_token(
                        &diagnostics::VARIABLE_DECLARATION_EXPECTED,
                        &[],
                    );
                }
            }
            ParsingContext::ObjectBindingElements => {
                self.parse_error_at_current_token(
                    &diagnostics::PROPERTY_DESTRUCTURING_PATTERN_EXPECTED,
                    &[],
                );
            }
            ParsingContext::ArrayBindingElements => {
                self.parse_error_at_current_token(
                    &diagnostics::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED,
                    &[],
                );
            }
            ParsingContext::ArgumentExpressions => {
                self.parse_error_at_current_token(&diagnostics::ARGUMENT_EXPRESSION_EXPECTED, &[]);
            }
            ParsingContext::ObjectLiteralMembers => {
                self.parse_error_at_current_token(&diagnostics::PROPERTY_ASSIGNMENT_EXPECTED, &[]);
            }
            ParsingContext::ArrayLiteralMembers => {
                self.parse_error_at_current_token(&diagnostics::EXPRESSION_OR_COMMA_EXPECTED, &[]);
            }
            ParsingContext::JSDocParameters => {
                self.parse_error_at_current_token(
                    &diagnostics::PARAMETER_DECLARATION_EXPECTED,
                    &[],
                );
            }
            ParsingContext::Parameters => {
                if ast::is_keyword(self.token()) {
                    let text = scanner::token_to_string(self.token());
                    self.parse_error_at_current_token(
                        &diagnostics::X_0_IS_NOT_ALLOWED_AS_A_PARAMETER_NAME,
                        &[&text],
                    );
                } else {
                    self.parse_error_at_current_token(
                        &diagnostics::PARAMETER_DECLARATION_EXPECTED,
                        &[],
                    );
                }
            }
            ParsingContext::TypeParameters => {
                self.parse_error_at_current_token(
                    &diagnostics::TYPE_PARAMETER_DECLARATION_EXPECTED,
                    &[],
                );
            }
            ParsingContext::TypeArguments => {
                self.parse_error_at_current_token(&diagnostics::TYPE_ARGUMENT_EXPECTED, &[]);
            }
            ParsingContext::TupleElementTypes => {
                self.parse_error_at_current_token(&diagnostics::TYPE_EXPECTED, &[]);
            }
            ParsingContext::HeritageClauses => {
                self.parse_error_at_current_token(&diagnostics::UNEXPECTED_TOKEN_EXPECTED, &[]);
            }
            ParsingContext::ImportOrExportSpecifiers => {
                if self.token() == Kind::FromKeyword {
                    self.parse_error_at_current_token(&diagnostics::X_0_EXPECTED, &[&"}"]);
                } else {
                    self.parse_error_at_current_token(&diagnostics::IDENTIFIER_EXPECTED, &[]);
                }
            }
            ParsingContext::JsxAttributes
            | ParsingContext::JsxChildren
            | ParsingContext::JSDocComment => {
                self.parse_error_at_current_token(&diagnostics::IDENTIFIER_EXPECTED, &[]);
            }
            ParsingContext::ImportAttributes => {
                self.parse_error_at_current_token(
                    &diagnostics::IDENTIFIER_OR_STRING_LITERAL_EXPECTED,
                    &[],
                );
            }
            ParsingContext::Count => panic!("Unhandled case in parsingContextErrors"),
        }
    }

    fn is_let_declaration(&mut self) -> bool {
        // In ES6 'let' always starts a lexical declaration if followed by an identifier or {
        // or [.
        self.look_ahead(|parser| {
            parser.next_token_is_binding_identifier_or_start_of_destructuring()
        })
    }

    fn next_token_is_binding_identifier_or_start_of_destructuring(&mut self) -> bool {
        self.next_token();
        self.is_binding_identifier()
            || self.token() == Kind::OpenBraceToken
            || self.token() == Kind::OpenBracketToken
    }

    fn next_is_identifier_and_close_paren(&mut self) -> bool {
        self.next_token_is_identifier() && self.next_token() == Kind::CloseParenToken
    }

    fn next_token_is_identifier(&mut self) -> bool {
        self.next_token();
        self.is_identifier()
    }

    fn is_implements_clause(&mut self) -> bool {
        self.token() == Kind::ImplementsKeyword
            && self.look_ahead(|parser| parser.next_token_is_identifier_or_keyword())
    }

    fn next_token_is_open_paren(&mut self) -> bool {
        self.next_token() == Kind::OpenParenToken
    }

    fn next_is_not_dot(&mut self) -> bool {
        self.next_token() != Kind::DotToken
    }

    fn next_token_is_from_keyword_or_equals_token(&mut self) -> bool {
        self.next_token();
        self.token() == Kind::FromKeyword || self.token() == Kind::EqualsToken
    }

    fn should_parse_import_phase_modifier(&mut self) -> bool {
        match self.token() {
            Kind::CommaToken | Kind::EqualsToken => false,
            Kind::FromKeyword => {
                !self.look_ahead(|parser| parser.next_token_is_token_string_literal())
            }
            _ => true,
        }
    }

    fn token_after_import_definitely_produces_import_declaration(&self) -> bool {
        self.token() == Kind::AsteriskToken || self.token() == Kind::OpenBraceToken
    }

    fn token_after_imported_identifier_allows_import_equals_declaration(&self) -> bool {
        !matches!(self.token(), Kind::CommaToken | Kind::FromKeyword)
    }

    fn can_parse_module_export_name(&self) -> bool {
        token_is_identifier_or_keyword(self.token()) || self.token() == Kind::StringLiteral
    }

    fn next_is_start_of_type(&mut self) -> bool {
        self.next_token();
        self.is_start_of_type(false /*inStartOfParameter*/)
    }

    fn next_is_start_of_type_of_import_type(&mut self) -> bool {
        self.next_token();
        self.token() == Kind::ImportKeyword
    }

    fn next_is_start_of_mapped_type(&mut self) -> bool {
        self.next_token();
        if self.token() == Kind::PlusToken || self.token() == Kind::MinusToken {
            return self.next_token() == Kind::ReadonlyKeyword;
        }
        if self.token() == Kind::ReadonlyKeyword {
            self.next_token();
        }
        self.token() == Kind::OpenBracketToken
            && self.next_token_is_identifier()
            && self.next_token() == Kind::InKeyword
    }

    fn next_token_is_open_paren_or_less_than(&mut self) -> bool {
        self.next_token();
        self.token() == Kind::OpenParenToken || self.token() == Kind::LessThanToken
    }

    fn is_parameter_name_start(&self) -> bool {
        // Be permissive about await and yield by calling isBindingIdentifier instead of isIdentifier; disallowing
        // them during a speculative parse leads to many more follow-on errors than allowing the function to parse then later
        // complaining about the use of the keywords.
        self.is_binding_identifier()
            || self.token() == Kind::OpenBracketToken
            || self.token() == Kind::OpenBraceToken
    }

    fn should_parse_return_type(&mut self, return_token: Kind, is_type: bool) -> bool {
        if return_token == Kind::EqualsGreaterThanToken {
            self.parse_expected(return_token);
            return true;
        } else if self.parse_optional(Kind::ColonToken) {
            return true;
        } else if is_type && self.token() == Kind::EqualsGreaterThanToken {
            // This is easy to get backward, especially in type contexts, so parse the type anyway
            self.parse_error_at_current_token(
                &diagnostics::X_0_EXPECTED,
                &[&scanner::token_to_string(Kind::ColonToken)],
            );
            self.next_token();
            return true;
        }
        false
    }

    fn is_index_signature(&mut self) -> bool {
        self.token() == Kind::OpenBracketToken
            && self.look_ahead(|parser| parser.next_is_unambiguously_index_signature())
    }

    fn next_is_unambiguously_index_signature(&mut self) -> bool {
        // The only allowed sequence is:
        //
        //   [id:
        //
        // However, for error recovery, we also check the following cases:
        //
        //   [...
        //   [id,
        //   [id?,
        //   [id?:
        //   [id?]
        //   [public id
        //   [private id
        //   [protected id
        //   []
        //
        self.next_token();
        if self.token() == Kind::DotDotDotToken || self.token() == Kind::CloseBracketToken {
            return true;
        }
        if ast::is_modifier_kind(self.token()) {
            self.next_token();
            if self.is_identifier() {
                return true;
            }
        } else if !self.is_identifier() {
            return false;
        } else {
            // Skip the identifier
            self.next_token();
        }
        // A colon signifies a well formed indexer
        // A comma should be a badly formed indexer because comma expressions are not allowed
        // in computed properties.
        if self.token() == Kind::ColonToken || self.token() == Kind::CommaToken {
            return true;
        }
        // Question mark could be an indexer with an optional property,
        // or it could be a conditional expression in a computed property.
        if self.token() != Kind::QuestionToken {
            return false;
        }
        // If any of the following tokens are after the question mark, it cannot
        // be a conditional expression, so treat it as an indexer.
        self.next_token();
        self.token() == Kind::ColonToken
            || self.token() == Kind::CommaToken
            || self.token() == Kind::CloseBracketToken
    }

    fn scan_start_of_named_tuple_element(&mut self) -> bool {
        if self.token() == Kind::DotDotDotToken {
            return token_is_identifier_or_keyword(self.next_token())
                && self.next_token_is_colon_or_question_colon();
        }
        token_is_identifier_or_keyword(self.token()) && self.next_token_is_colon_or_question_colon()
    }

    fn next_token_is_colon_or_question_colon(&mut self) -> bool {
        self.next_token() == Kind::ColonToken
            || self.token() == Kind::QuestionToken && self.next_token() == Kind::ColonToken
    }

    fn next_token_is_new_keyword(&mut self) -> bool {
        self.next_token() == Kind::NewKeyword
    }

    fn next_token_can_follow_modifier(&mut self) -> bool {
        match self.token() {
            Kind::ConstKeyword => {
                // 'const' is only a modifier if followed by 'enum'.
                self.next_token() == Kind::EnumKeyword
            }
            Kind::ExportKeyword => {
                self.next_token();
                if self.token() == Kind::DefaultKeyword {
                    return self
                        .look_ahead(|parser| parser.next_token_can_follow_default_keyword());
                }
                if self.token() == Kind::TypeKeyword {
                    return self
                        .look_ahead(|parser| parser.next_token_can_follow_export_modifier());
                }
                self.can_follow_export_modifier()
            }
            Kind::DefaultKeyword => self.next_token_can_follow_default_keyword(),
            Kind::StaticKeyword => {
                self.next_token();
                self.can_follow_modifier()
            }
            Kind::GetKeyword | Kind::SetKeyword => {
                self.next_token();
                self.can_follow_get_or_set_keyword()
            }
            _ => self.next_token_is_on_same_line_and_can_follow_modifier(),
        }
    }

    fn next_token_can_follow_default_keyword(&mut self) -> bool {
        match self.next_token() {
            Kind::ClassKeyword | Kind::FunctionKeyword | Kind::InterfaceKeyword | Kind::AtToken => {
                true
            }
            Kind::AbstractKeyword => {
                self.look_ahead(|parser| parser.next_token_is_class_keyword_on_same_line())
            }
            Kind::AsyncKeyword => {
                self.look_ahead(|parser| parser.next_token_is_function_keyword_on_same_line())
            }
            _ => false,
        }
    }

    fn next_token_is_identifier_or_keyword_or_greater_than(&mut self) -> bool {
        token_is_identifier_or_keyword_or_greater_than(self.next_token())
    }

    fn next_token_is_identifier_or_keyword_or_literal_on_same_line(&mut self) -> bool {
        (self.next_token_is_identifier_or_keyword()
            || self.token() == Kind::NumericLiteral
            || self.token() == Kind::BigIntLiteral
            || self.token() == Kind::StringLiteral)
            && !self.has_preceding_line_break()
    }

    fn next_token_is_class_keyword_on_same_line(&mut self) -> bool {
        self.next_token() == Kind::ClassKeyword && !self.has_preceding_line_break()
    }

    fn next_token_is_function_keyword_on_same_line(&mut self) -> bool {
        self.next_token() == Kind::FunctionKeyword && !self.has_preceding_line_break()
    }

    fn next_token_can_follow_export_modifier(&mut self) -> bool {
        self.next_token();
        self.can_follow_export_modifier()
    }

    fn can_follow_export_modifier(&self) -> bool {
        self.token() == Kind::AtToken
            || self.token() != Kind::AsteriskToken
                && self.token() != Kind::AsKeyword
                && self.token() != Kind::OpenBraceToken
                && self.can_follow_modifier()
    }

    fn can_follow_modifier(&self) -> bool {
        self.token() == Kind::OpenBracketToken
            || self.token() == Kind::OpenBraceToken
            || self.token() == Kind::AsteriskToken
            || self.token() == Kind::DotDotDotToken
            || self.is_literal_property_name()
    }

    fn can_follow_get_or_set_keyword(&self) -> bool {
        self.token() == Kind::OpenBracketToken || self.is_literal_property_name()
    }

    fn next_token_is_on_same_line_and_can_follow_modifier(&mut self) -> bool {
        self.next_token();
        if self.has_preceding_line_break() {
            return false;
        }
        self.can_follow_modifier()
    }

    fn next_token_is_open_brace(&mut self) -> bool {
        self.next_token() == Kind::OpenBraceToken
    }

    fn is_yield_expression(&mut self) -> bool {
        if self.token() == Kind::YieldKeyword {
            // If we have a 'yield' keyword, and this is a context where yield expressions are
            // allowed, then definitely parse out a yield expression.
            if self.in_yield_context() {
                return true;
            }

            // We're in a context where 'yield expr' is not allowed.  However, if we can
            // definitely tell that the user was trying to parse a 'yield expr' and not
            // just a normal expr that start with a 'yield' identifier, then parse out
            // a 'yield expr'.  We can then report an error later that they are only
            // allowed in generator expressions.
            //
            // for example, if we see 'yield(foo)', then we'll have to treat that as an
            // invocation expression of something called 'yield'.  However, if we have
            // 'yield foo' then that is not legal as a normal expression, so we can
            // definitely recognize this as a yield expression.
            //
            // for now we just check if the next token is an identifier.  More heuristics
            // can be added here later as necessary.  We just need to make sure that we
            // don't accidentally consume something legal.
            return self.look_ahead(|parser| {
                parser.next_token_is_identifier_or_keyword_or_literal_on_same_line()
            });
        }
        false
    }

    fn is_start_of_expression_statement(&mut self) -> bool {
        // As per the grammar, none of '{' or 'function' or 'class' can start an expression statement.
        self.token() != Kind::OpenBraceToken
            && self.token() != Kind::FunctionKeyword
            && self.token() != Kind::ClassKeyword
            && self.token() != Kind::AtToken
            && self.is_start_of_expression()
    }

    fn is_await_expression(&mut self) -> bool {
        if self.token() == Kind::AwaitKeyword {
            if self.in_await_context() {
                return true;
            }
            // here we are using similar heuristics as 'isYieldExpression'
            return self.look_ahead(|parser| {
                parser.next_token_is_identifier_or_keyword_or_literal_on_same_line()
            });
        }
        false
    }

    fn next_token_is_dot(&mut self) -> bool {
        self.next_token() == Kind::DotToken
    }

    fn is_template_start_of_tagged_template(&self) -> bool {
        self.token() == Kind::NoSubstitutionTemplateLiteral || self.token() == Kind::TemplateHead
    }

    fn can_follow_type_arguments_in_expression(&mut self) -> bool {
        match self.token() {
            // These tokens can follow a type argument list in a call expression:
            // foo<x>(
            // foo<T> `...`
            // foo<T> `...${100}...`
            Kind::OpenParenToken | Kind::NoSubstitutionTemplateLiteral | Kind::TemplateHead => {
                return true;
            }
            // A type argument list followed by `<` never makes sense, and a type argument list followed
            // by `>` is ambiguous with a (re-scanned) `>>` operator, so we disqualify both. Also, in
            // this context, `+` and `-` are unary operators, not binary operators.
            Kind::LessThanToken | Kind::GreaterThanToken | Kind::PlusToken | Kind::MinusToken => {
                return false;
            }
            _ => {}
        }
        // We favor the type argument list interpretation when it is immediately followed by
        // a line break, a binary operator, or something that can't start an expression.
        self.has_preceding_line_break()
            || self.is_binary_operator()
            || !self.is_start_of_expression()
    }

    fn is_start_of_member_expression_rest(&self) -> bool {
        matches!(
            self.token(),
            Kind::DotToken
                | Kind::QuestionDotToken
                | Kind::OpenBracketToken
                | Kind::NoSubstitutionTemplateLiteral
                | Kind::TemplateHead
                | Kind::ExclamationToken
                | Kind::LessThanToken
                | Kind::LessThanLessThanToken
        )
    }

    fn is_start_of_optional_property_or_element_access_chain(&mut self) -> bool {
        self.token() == Kind::QuestionDotToken
            && self.look_ahead(|parser| {
                parser.next_token_is_identifier_or_keyword_or_open_bracket_or_template()
            })
    }

    fn next_token_is_identifier_or_keyword_or_open_bracket_or_template(&mut self) -> bool {
        self.next_token();
        token_is_identifier_or_keyword(self.token())
            || self.token() == Kind::OpenBracketToken
            || self.is_template_start_of_tagged_template()
    }

    fn try_parse_semicolon(&mut self) -> bool {
        if !self.can_parse_semicolon() {
            return false;
        }
        if self.token() == Kind::SemicolonToken {
            // consume the semicolon if it was explicitly provided.
            self.next_token();
        }
        true
    }

    fn parse_semicolon(&mut self) -> bool {
        self.try_parse_semicolon() || self.parse_expected(Kind::SemicolonToken)
    }

    fn next_token_is_binding_identifier_or_start_of_destructuring_on_same_line_disallow_of(
        &mut self,
    ) -> bool {
        self.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(
            true, /*disallowOf*/
        )
    }

    fn in_yield_context(&self) -> bool {
        self.context_flags().intersects(NodeFlags::YieldContext)
    }

    fn in_disallow_conditional_types_context(&self) -> bool {
        self.context_flags().intersects(NodeFlags::DisallowConditionalTypesContext)
    }

    fn in_decorator_context(&self) -> bool {
        self.context_flags().intersects(NodeFlags::DecoratorContext)
    }

    fn in_await_context(&self) -> bool {
        self.context_flags().intersects(NodeFlags::AwaitContext)
    }
    fn next_is_un_parenthesized_async_arrow_function(&mut self) -> bool {
        // AsyncArrowFunctionExpression:
        //      1) async[no LineTerminator here]AsyncArrowBindingIdentifier[?Yield][no LineTerminator here]=>AsyncConciseBody[?In]
        //      2) CoverCallExpressionAndAsyncArrowHead[?Yield, ?Await][no LineTerminator here]=>AsyncConciseBody[?In]
        if self.token() == Kind::AsyncKeyword {
            self.next_token();
            // If the "async" is followed by "=>" token then it is not a beginning of an async arrow-function
            // but instead a simple arrow-function which will be parsed inside "parseAssignmentExpressionOrHigher"
            if self.has_preceding_line_break() || self.token() == Kind::EqualsGreaterThanToken {
                return false;
            }
            // Check for un-parenthesized AsyncArrowFunction
            if !self.is_identifier() {
                return false;
            }
            self.scan_token();
            return !self.has_preceding_line_break()
                && self.token() == Kind::EqualsGreaterThanToken;
        }
        false
    }
}

impl RecognitionParser<'static> for crate::Parser {}
impl<'src> RecognitionParser<'src> for crate::owned::OwnedParser<'src> {}

impl<'src> RecognitionParser<'src> for crate::owned::OwnedParserScope<'src, '_, '_, '_> {}
