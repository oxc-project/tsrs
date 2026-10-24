//! List grammar uses copied keys while recursive callbacks mutate construction storage.

#![forbid(unsafe_code)]
use crate::lexical::LexicalScanner;
use crate::parser_1::ParsingContext;
use crate::recognition::RecognitionParser;
use tsrs_ast::Kind;
use tsrs_core::TextRange;
use tsrs_diagnostics as diagnostics;

pub trait ListParser<'src>: RecognitionParser<'src> {
    type List: Copy;
    fn parsing_contexts(&self) -> u32;
    fn set_parsing_contexts(&mut self, contexts: u32);
    fn node_stack(&self) -> &[Self::Node];
    fn node_stack_mut(&mut self) -> &mut Vec<Self::Node>;
    fn reparse_list_mut(&mut self) -> &mut Vec<Self::Node>;
    fn node_is_js_alias_or_import(&self, node: Self::Node) -> bool;
    fn finish_node_list(&mut self, start: usize, range: TextRange) -> Self::List;
    fn new_node_list(&mut self, range: TextRange, nodes: &[Self::Node]) -> Self::List;
    fn create_missing_list(&mut self) -> Self::List;
    fn node_pos(&self) -> u32 {
        self.scanner().token_full_start()
    }
    fn parse_list_index(
        &mut self,
        kind: ParsingContext,
        parse_element: impl FnMut(&mut Self, usize) -> Self::Node,
    ) -> Vec<Self::Node> {
        let start = self.parse_list_index_on_stack(kind, parse_element);
        self.node_stack_mut().split_off(start)
    }

    /// Keeps elements on the shared key stack until the caller allocates the list.
    fn parse_list_index_on_stack(
        &mut self,
        kind: ParsingContext,
        mut parse_element: impl FnMut(&mut Self, usize) -> Self::Node,
    ) -> usize {
        let save_parsing_contexts = self.parsing_contexts();
        self.set_parsing_contexts(self.parsing_contexts() | (1 << kind as u32));
        let mut outer_reparse_list = std::mem::take(self.reparse_list_mut());
        let start = self.node_stack().len();
        while !self.is_list_terminator(kind) {
            if self.is_list_element(kind, false /*inErrorRecovery*/) {
                let index = self.node_stack().len() - start;
                let elt = parse_element(self, index);
                if !self.reparse_list_mut().is_empty() {
                    outer_reparse_list = std::mem::take(self.reparse_list_mut()).into_iter().fold(
                        outer_reparse_list,
                        |mut outer_reparse_list, e| {
                            // Propagate @typedef type alias declarations outwards to a context that permits them.
                            if self.node_is_js_alias_or_import(e)
                                && kind != ParsingContext::SourceElements
                                && kind != ParsingContext::BlockStatements
                            {
                                outer_reparse_list.push(e);
                            } else {
                                self.node_stack_mut().push(e);
                            }

                            outer_reparse_list
                        },
                    );
                }
                self.node_stack_mut().push(elt);
                continue;
            }
            if self.abort_parsing_list_or_move_to_next_token(kind) {
                break;
            }
        }
        *self.reparse_list_mut() = outer_reparse_list;
        self.set_parsing_contexts(save_parsing_contexts);
        start
    }

    fn parse_list(
        &mut self,
        kind: ParsingContext,
        mut parse_element: impl FnMut(&mut Self) -> Self::Node,
    ) -> Self::List {
        let pos = self.node_pos();
        let start = self.parse_list_index_on_stack(kind, |p, _| parse_element(p));
        let end = self.node_pos();
        self.finish_node_list(start, TextRange::new(pos, end))
    }

    fn parse_delimited_list<R: Into<Option<Self::Node>>>(
        &mut self,
        kind: ParsingContext,
        mut parse_element: impl FnMut(&mut Self) -> R,
    ) -> Option<Self::List> {
        let pos = self.node_pos();
        let save_parsing_contexts = self.parsing_contexts();
        self.set_parsing_contexts(self.parsing_contexts() | (1 << kind as u32));
        let start = self.node_stack().len();
        loop {
            if self.is_list_element(kind, false /*inErrorRecovery*/) {
                let start_pos = self.node_pos();
                let element: Option<Self::Node> = parse_element(self).into();
                let Some(element) = element else {
                    self.set_parsing_contexts(save_parsing_contexts);
                    self.node_stack_mut().truncate(start);
                    // Return nil to indicate parseElement failed
                    return None;
                };
                self.node_stack_mut().push(element);
                if self.parse_optional(Kind::CommaToken) {
                    // No need to check for a zero length node since we know we parsed a comma
                    continue;
                }
                if self.is_list_terminator(kind) {
                    break;
                }
                // We didn't get a comma, and the list wasn't terminated, explicitly parse
                // out a comma so we give a good error message.
                if self.token() != Kind::CommaToken && kind == ParsingContext::EnumMembers {
                    self.parse_error_at_current_token(
                        &diagnostics::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR,
                        &[],
                    );
                } else {
                    self.parse_expected(Kind::CommaToken);
                }
                // If the token was a semicolon, and the caller allows that, then skip it and
                // continue.  This ensures we get back on track and don't result in tons of
                // parse errors.  For example, this can happen when people do things like use
                // a semicolon to delimit object literal members.   Note: we'll have already
                // reported an error when we called parseExpected above.
                if (kind == ParsingContext::ObjectLiteralMembers
                    || kind == ParsingContext::ImportAttributes)
                    && self.token() == Kind::SemicolonToken
                    && !self.has_preceding_line_break()
                {
                    self.next_token();
                }
                if start_pos == self.node_pos() {
                    // What we're parsing isn't actually remotely recognizable as a element and we've consumed no tokens whatsoever
                    // Consume a token to advance the parser in some way and avoid an infinite loop
                    // This can happen when we're speculatively parsing parenthesized expressions which we think may be arrow functions,
                    // or when a modifier keyword which is disallowed as a parameter name (ie, `static` in strict mode) is supplied
                    self.next_token();
                }
                continue;
            }
            if self.is_list_terminator(kind) {
                break;
            }
            if self.abort_parsing_list_or_move_to_next_token(kind) {
                break;
            }
        }
        self.set_parsing_contexts(save_parsing_contexts);
        let end = self.node_pos();
        Some(self.finish_node_list(start, TextRange::new(pos, end)))
    }

    fn parse_bracketed_list<R: Into<Option<Self::Node>>>(
        &mut self,
        kind: ParsingContext,
        parse_element: impl FnMut(&mut Self) -> R,
        opening: Kind,
        closing: Kind,
    ) -> Option<Self::List> {
        if self.parse_expected(opening) {
            let result = self.parse_delimited_list(kind, parse_element);
            self.parse_expected(closing);
            return result;
        }
        Some(self.create_missing_list())
    }

    fn parse_empty_node_list(&mut self) -> Self::List {
        let pos = self.node_pos();
        self.new_node_list(TextRange::new(pos, pos), &[])
    }

    fn abort_parsing_list_or_move_to_next_token(&mut self, kind: ParsingContext) -> bool {
        self.parsing_context_errors(kind);
        if self.is_in_some_parsing_context() {
            return true;
        }
        self.next_token();
        false
    }

    fn is_in_some_parsing_context(&mut self) -> bool {
        // We should be in at least one parsing context, be it SourceElements while parsing
        // a SourceFile, or JSDocComment when lazily parsing JSDoc.
        assert!(self.parsing_contexts() != 0, "Missing parsing context");
        if ParsingContext::ALL.into_iter().any(|kind| {
            if self.parsing_contexts() & (1 << kind as u32) != 0
                && (self.is_list_element(kind, true /*inErrorRecovery*/)
                    || self.is_list_terminator(kind))
            {
                return true;
            }

            false
        }) {
            return true;
        }
        false
    }
}

impl ListParser<'static> for crate::Parser {
    type List = tsrs_core::P<tsrs_ast::NodeList>;
    fn parsing_contexts(&self) -> u32 {
        self.parsing_contexts
    }
    fn set_parsing_contexts(&mut self, contexts: u32) {
        self.parsing_contexts = contexts;
    }
    fn node_stack(&self) -> &[Self::Node] {
        &self.node_stack
    }
    fn node_stack_mut(&mut self) -> &mut Vec<Self::Node> {
        &mut self.node_stack
    }
    fn reparse_list_mut(&mut self) -> &mut Vec<Self::Node> {
        &mut self.reparse_list
    }
    fn node_is_js_alias_or_import(&self, node: Self::Node) -> bool {
        matches!(node.kind(), Kind::JSTypeAliasDeclaration | Kind::JSImportDeclaration)
    }
    fn finish_node_list(&mut self, start: usize, range: TextRange) -> Self::List {
        crate::Parser::finish_node_list(self, start, range)
    }
    fn new_node_list(&mut self, range: TextRange, nodes: &[Self::Node]) -> Self::List {
        crate::Parser::new_node_list(self, range, nodes)
    }
    fn create_missing_list(&mut self) -> Self::List {
        crate::Parser::create_missing_list(self)
    }
}
