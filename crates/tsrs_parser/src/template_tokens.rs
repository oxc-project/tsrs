//! Template token grammar shares scanner recovery and retains raw source ranges
//! independently of cooked text while construction mutates the AST columns.

#![forbid(unsafe_code)]

use crate::lexical::{LexicalParser, LexicalScanner, ScannedText};
use tsrs_ast::{Kind, TokenFlags};
use tsrs_diagnostics as diagnostics;
use tsrs_scanner::{self as scanner, ScannerText};

pub trait TemplateTokenParser<'src>: LexicalParser<'src> {
    fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind;
    fn new_template_head(
        &mut self,
        text: ScannedText<<Self::TextPolicy as ScannerText<'src>>::Text>,
        raw_text: &'src str,
        template_flags: TokenFlags,
    ) -> Self::Node;
    fn new_template_middle(
        &mut self,
        text: ScannedText<<Self::TextPolicy as ScannerText<'src>>::Text>,
        raw_text: &'src str,
        template_flags: TokenFlags,
    ) -> Self::Node;
    fn new_template_tail(
        &mut self,
        text: ScannedText<<Self::TextPolicy as ScannerText<'src>>::Text>,
        raw_text: &'src str,
        template_flags: TokenFlags,
    ) -> Self::Node;
    fn new_missing_template_tail(&mut self) -> Self::Node;

    fn parse_template_head(&mut self, is_tagged_template: bool) -> Self::Node {
        if !is_tagged_template && self.scanner().token_flags().intersects(TokenFlags::IsInvalid) {
            self.re_scan_template_token(false /*isTaggedTemplate*/);
        }
        let pos = self.scanner().token_full_start();
        let text = self.take_current_text();
        let raw_text = self.get_template_literal_raw_text(2 /*endLength*/);
        let result = self.new_template_head(text, raw_text, self.scanner().token_flags());
        self.next_token();
        self.finish_node(result, pos)
    }

    fn get_template_literal_raw_text(&self, end_length: usize) -> &'src str {
        let mut end_length = end_length;
        let token_text = self.scanner().token_text();
        if self.scanner().token_flags().intersects(TokenFlags::Unterminated) {
            end_length = 0;
        }
        &token_text[1..token_text.len() - end_length]
    }

    fn parse_literal_of_template_span(&mut self, is_tagged_template: bool) -> Self::Node {
        if self.token() == Kind::CloseBraceToken {
            self.re_scan_template_token(is_tagged_template);
            return self.parse_template_middle_or_tail();
        }
        self.error_at(
            self.scanner().token_range(),
            &diagnostics::X_0_EXPECTED,
            &[&scanner::token_to_string(Kind::CloseBraceToken)],
        );
        let node = self.new_missing_template_tail();
        let pos = self.scanner().token_full_start();
        self.finish_node(node, pos)
    }

    fn parse_template_middle_or_tail(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        let result = if self.token() == Kind::TemplateMiddle {
            let text = self.take_current_text();
            let raw_text = self.get_template_literal_raw_text(2 /*endLength*/);
            self.new_template_middle(text, raw_text, self.scanner().token_flags())
        } else {
            let text = self.take_current_text();
            let raw_text = self.get_template_literal_raw_text(1 /*endLength*/);
            self.new_template_tail(text, raw_text, self.scanner().token_flags())
        };
        self.next_token();
        self.finish_node(result, pos)
    }
}

impl TemplateTokenParser<'static> for crate::Parser {
    fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind {
        Self::re_scan_template_token(self, is_tagged_template)
    }
    fn new_template_head(
        &mut self,
        text: ScannedText<&'static str>,
        raw_text: &'static str,
        template_flags: TokenFlags,
    ) -> Self::Node {
        self.factory.new_template_head(text.value, raw_text, template_flags)
    }
    fn new_template_middle(
        &mut self,
        text: ScannedText<&'static str>,
        raw_text: &'static str,
        template_flags: TokenFlags,
    ) -> Self::Node {
        self.factory.new_template_middle(text.value, raw_text, template_flags)
    }
    fn new_template_tail(
        &mut self,
        text: ScannedText<&'static str>,
        raw_text: &'static str,
        template_flags: TokenFlags,
    ) -> Self::Node {
        self.factory.new_template_tail(text.value, raw_text, template_flags)
    }
    fn new_missing_template_tail(&mut self) -> Self::Node {
        self.factory.new_template_tail("", "", TokenFlags::None)
    }
}
