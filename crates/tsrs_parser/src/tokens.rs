//! Token recovery shared by both parser construction contexts.
//! Diagnostic handles identify records; no diagnostic borrow crosses a write.

#![forbid(unsafe_code)]

use tsrs_ast::Kind;
use tsrs_core::TextRange;
use tsrs_diagnostics::{self as diagnostics, Message};
use tsrs_scanner::token_to_string;

use crate::lexical::{LexicalParser, LexicalScanner};

pub trait TokenParser<'src>: LexicalParser<'src> {
    type Diagnostic: Copy;

    fn expected_error(&mut self, kind: Kind, message: Option<&'static Message>);
    fn unmatched_bracket_error(&mut self, close_kind: Kind) -> Option<Self::Diagnostic>;
    fn add_bracket_related_info(
        &mut self,
        diagnostic: Self::Diagnostic,
        range: TextRange,
        message: &'static Message,
        open_text: &str,
        close_text: &str,
    );

    fn parse_optional(&mut self, token: Kind) -> bool {
        if self.token() == token {
            self.next_token();
            return true;
        }
        false
    }

    fn parse_expected(&mut self, kind: Kind) -> bool {
        self.parse_expected_with_diagnostic(kind, None, true)
    }

    fn parse_expected_without_advancing(&mut self, kind: Kind) -> bool {
        self.parse_expected_with_diagnostic(kind, None, false)
    }

    #[inline]
    fn parse_expected_with_diagnostic(
        &mut self,
        kind: Kind,
        message: Option<&'static Message>,
        should_advance: bool,
    ) -> bool {
        if self.token() == kind {
            if should_advance {
                self.next_token();
            }
            return true;
        }
        self.expected_error(kind, message);
        false
    }

    fn parse_expected_token(&mut self, kind: Kind) -> Self::Node {
        if let Some(token) = self.parse_optional_token(kind) {
            token
        } else {
            self.expected_error(kind, None);
            let token = self.new_token(kind);
            let pos = self.scanner().token_full_start();
            self.finish_node(token, pos)
        }
    }

    fn parse_optional_token(&mut self, kind: Kind) -> Option<Self::Node> {
        if self.token() == kind {
            return Some(self.parse_token_node());
        }
        None
    }

    #[inline]
    fn parse_expected_matching_brackets(
        &mut self,
        open_kind: Kind,
        close_kind: Kind,
        open_parsed: bool,
        open_position: u32,
    ) {
        if self.token() == close_kind {
            self.next_token();
            return;
        }
        self.report_unmatched_bracket(open_kind, close_kind, open_parsed, open_position);
    }

    #[cold]
    fn report_unmatched_bracket(
        &mut self,
        open_kind: Kind,
        close_kind: Kind,
        open_parsed: bool,
        open_position: u32,
    ) {
        let last_error = self.unmatched_bracket_error(close_kind);
        if !open_parsed {
            return;
        }
        if let Some(last_error) = last_error {
            self.add_bracket_related_info(
                last_error,
                TextRange::new(open_position, open_position),
                &diagnostics::THE_PARSER_EXPECTED_TO_FIND_A_1_TO_MATCH_THE_0_TOKEN_HERE,
                token_to_string(open_kind),
                token_to_string(close_kind),
            );
        }
    }
}

impl TokenParser<'static> for crate::Parser {
    type Diagnostic = tsrs_core::P<tsrs_ast::Diagnostic>;

    #[cold]
    fn expected_error(&mut self, kind: Kind, message: Option<&'static Message>) {
        if let Some(message) = message {
            self.parse_error_at_current_token(message, &[]);
        } else {
            self.parse_error_at_current_token(
                &diagnostics::X_0_EXPECTED,
                &[&token_to_string(kind)],
            );
        }
    }

    fn unmatched_bracket_error(&mut self, close_kind: Kind) -> Option<Self::Diagnostic> {
        self.parse_error_at_current_token(
            &diagnostics::X_0_EXPECTED,
            &[&token_to_string(close_kind)],
        )
    }

    fn add_bracket_related_info(
        &mut self,
        diagnostic: Self::Diagnostic,
        range: TextRange,
        message: &'static Message,
        open_text: &str,
        close_text: &str,
    ) {
        use tsrs_ast::DiagnosticExt as _;
        diagnostic.add_related_info(tsrs_ast::new_diagnostic(
            None,
            range,
            message,
            &[&open_text, &close_text],
        ));
    }
}
