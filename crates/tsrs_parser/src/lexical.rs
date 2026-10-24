//! Shared lexical grammar. The context owns construction and diagnostics; only
//! copied node keys and independently retained token text cross scanner writes.

#![forbid(unsafe_code)]

use std::fmt::Display;

use tsrs_ast::{Kind, NodeFlags, TokenFlags};
use tsrs_core::TextRange;
use tsrs_diagnostics::{self as diagnostics, Message};
use tsrs_scanner::{Scanner, ScannerImpl, ScannerText};

use crate::utilities::token_is_identifier_or_keyword;

pub struct ScannedText<T> {
    pub value: T,
    pub source_range: Option<TextRange>,
}

/// Only token observations cross the lexical grammar boundary. Both scanner
/// implementations expose these directly, without constructing a snapshot.
pub trait LexicalScanner<'src> {
    fn has_unicode_escape(&self) -> bool;
    fn has_extended_unicode_escape(&self) -> bool;
    fn has_preceding_jsdoc_leading_asterisks(&self) -> bool;
    fn has_preceding_line_break(&self) -> bool;
    fn token_range(&self) -> TextRange;
    fn token_full_start(&self) -> u32;
    fn token_start(&self) -> u32;
    fn token_end(&self) -> u32;
    fn token_flags(&self) -> TokenFlags;
    fn token_text(&self) -> &'src str;
}

macro_rules! lexical_scanner_getters {
    ($scanner:ty; $($name:ident -> $result:ty),+ $(,)?) => {
        $(
            #[inline]
            fn $name(&self) -> $result {
                <$scanner>::$name(self)
            }
        )+
    };
}

impl LexicalScanner<'static> for Scanner {
    lexical_scanner_getters!(Scanner;
        has_unicode_escape -> bool,
        has_extended_unicode_escape -> bool,
        has_preceding_jsdoc_leading_asterisks -> bool,
        has_preceding_line_break -> bool,
        token_range -> TextRange,
        token_full_start -> u32,
        token_start -> u32,
        token_end -> u32,
        token_flags -> TokenFlags,
        token_text -> &'static str,
    );
}

impl<'src, S: ScannerText<'src>> LexicalScanner<'src> for ScannerImpl<'src, S> {
    lexical_scanner_getters!(ScannerImpl<'src, S>;
        has_unicode_escape -> bool,
        has_extended_unicode_escape -> bool,
        has_preceding_jsdoc_leading_asterisks -> bool,
        has_preceding_line_break -> bool,
        token_range -> TextRange,
        token_full_start -> u32,
        token_start -> u32,
        token_end -> u32,
        token_flags -> TokenFlags,
        token_text -> &'src str,
    );
}

/// Backend operations carry construction authority explicitly. Implementations
/// cannot return a node view detached from their writer or scanner.
pub trait LexicalParser<'src>: Sized {
    type TextPolicy: ScannerText<'src>;
    type Scanner: LexicalScanner<'src>;
    type Node: Copy;
    type Speculative<'brand, 'writer, 'parser>: crate::recognition::RecognitionParser<'src>
    where
        Self: 'parser;

    fn scanner(&self) -> &Self::Scanner;
    fn token(&self) -> Kind;
    fn context_flags(&self) -> NodeFlags;
    fn statement_has_await_identifier(&self) -> bool;
    fn set_statement_has_await_identifier(&mut self, value: bool);
    fn take_current_text(&mut self) -> ScannedText<<Self::TextPolicy as ScannerText<'src>>::Text>;
    fn scan_token(&mut self) -> Kind;
    fn look_ahead<'parser>(
        &'parser mut self,
        callback: impl for<'brand, 'writer> FnOnce(
            &mut Self::Speculative<'brand, 'writer, 'parser>,
        ) -> bool,
    ) -> bool;
    fn error_at(&mut self, range: TextRange, message: &'static Message, args: &[&dyn Display]);
    fn finish_node(&mut self, node: Self::Node, pos: u32) -> Self::Node;
    fn new_token(&mut self, kind: Kind) -> Self::Node;
    fn new_keyword_expression(&mut self, kind: Kind) -> Self::Node;
    fn new_identifier(&mut self, text: &'static str) -> Self::Node;
    fn new_source_identifier(
        &mut self,
        text: ScannedText<<Self::TextPolicy as ScannerText<'src>>::Text>,
        range: TextRange,
    ) -> Self::Node;
    fn new_private_identifier(
        &mut self,
        text: ScannedText<<Self::TextPolicy as ScannerText<'src>>::Text>,
    ) -> Self::Node;
    fn new_literal(
        &mut self,
        kind: Kind,
        text: ScannedText<<Self::TextPolicy as ScannerText<'src>>::Text>,
        token_flags: TokenFlags,
    ) -> Self::Node;
    fn new_qualified_name(&mut self, left: Self::Node, right: Self::Node) -> Self::Node;

    #[inline]
    fn next_token(&mut self) -> Kind {
        if tsrs_ast::is_keyword(self.token())
            && (self.scanner().has_unicode_escape() || self.scanner().has_extended_unicode_escape())
        {
            self.report_keyword_with_escape();
        }
        self.scan_token()
    }

    #[cold]
    fn report_keyword_with_escape(&mut self) {
        self.error_at(
            self.scanner().token_range(),
            &diagnostics::KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS,
            &[],
        );
    }

    fn parse_token_node(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        let kind = self.token();
        self.next_token();
        let token = self.new_token(kind);
        self.finish_node(token, pos)
    }

    fn parse_keyword_expression(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        let result = self.new_keyword_expression(self.token());
        self.next_token();
        self.finish_node(result, pos)
    }

    fn parse_literal_expression(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        let text = self.take_current_text();
        let token_flags = self.scanner().token_flags();
        let result = self.new_literal(self.token(), text, token_flags);
        self.next_token();
        self.finish_node(result, pos)
    }

    fn is_identifier(&self) -> bool {
        if self.token() == Kind::Identifier {
            return true;
        }
        // Yield and await remain keywords in their respective contexts.
        if self.token() == Kind::YieldKeyword
            && self.context_flags().intersects(NodeFlags::YieldContext)
            || self.token() == Kind::AwaitKeyword
                && self.context_flags().intersects(NodeFlags::AwaitContext)
        {
            return false;
        }
        self.token() > Kind::LAST_RESERVED_WORD
    }

    fn is_binding_identifier(&self) -> bool {
        // Binding accepts await/yield here; the binder reports invalid contexts.
        self.token() == Kind::Identifier || self.token() > Kind::LAST_RESERVED_WORD
    }

    fn parse_identifier_name_error_on_unicode_escape_sequence(&mut self) -> Self::Node {
        if self.scanner().has_unicode_escape() || self.scanner().has_extended_unicode_escape() {
            self.error_at(
                self.scanner().token_range(),
                &diagnostics::UNICODE_ESCAPE_SEQUENCE_CANNOT_APPEAR_HERE,
                &[],
            );
        }
        self.create_identifier_with_diagnostic(
            token_is_identifier_or_keyword(self.token()),
            None,
            None,
        )
    }

    fn parse_binding_identifier(&mut self) -> Self::Node {
        self.parse_binding_identifier_with_diagnostic(None)
    }

    fn parse_binding_identifier_with_diagnostic(
        &mut self,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        let save_has_await_identifier = self.statement_has_await_identifier();
        let is_binding_identifier = self.is_binding_identifier();
        let id = self.create_identifier_with_diagnostic(
            is_binding_identifier,
            None,
            private_identifier_diagnostic_message,
        );
        self.set_statement_has_await_identifier(save_has_await_identifier);
        id
    }

    fn parse_optional_binding_identifier(&mut self) -> Option<Self::Node> {
        if self.is_binding_identifier() {
            return Some(self.parse_binding_identifier());
        }
        None
    }

    fn parse_identifier_name(&mut self) -> Self::Node {
        self.parse_identifier_name_with_diagnostic(None)
    }

    fn parse_identifier_name_with_diagnostic(
        &mut self,
        diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        self.create_identifier_with_diagnostic(
            token_is_identifier_or_keyword(self.token()),
            diagnostic_message,
            None,
        )
    }

    fn parse_identifier(&mut self) -> Self::Node {
        self.parse_identifier_with_diagnostic(None, None)
    }

    fn parse_identifier_with_diagnostic(
        &mut self,
        diagnostic_message: Option<&'static Message>,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        let is_identifier = self.is_identifier();
        self.create_identifier_with_diagnostic(
            is_identifier,
            diagnostic_message,
            private_identifier_diagnostic_message,
        )
    }

    fn create_identifier_with_diagnostic(
        &mut self,
        is_identifier: bool,
        diagnostic_message: Option<&'static Message>,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        if is_identifier {
            let pos = if self.scanner().has_preceding_jsdoc_leading_asterisks() {
                self.scanner().token_start()
            } else {
                self.scanner().token_full_start()
            };
            let text = self.take_current_text();
            let end = self.scanner().token_end();
            self.scan_token();
            let id = self.new_source_identifier(text, TextRange::new(pos, end));
            return self.finish_node(id, pos);
        }
        self.create_missing_identifier_with_diagnostic(
            diagnostic_message,
            private_identifier_diagnostic_message,
        )
    }

    #[cold]
    fn create_missing_identifier_with_diagnostic(
        &mut self,
        diagnostic_message: Option<&'static Message>,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        if self.token() == Kind::PrivateIdentifier {
            self.error_at(
                self.scanner().token_range(),
                private_identifier_diagnostic_message.unwrap_or(
                    &diagnostics::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                ),
                &[],
            );
            return self.create_identifier_with_diagnostic(true, None, None);
        }
        // Only EOF reports at the full start, preserving embedded-script recovery.
        let range = if self.token() == Kind::EndOfFile {
            let pos = self.scanner().token_full_start();
            TextRange::new(pos, pos)
        } else {
            self.scanner().token_range()
        };
        if let Some(message) = diagnostic_message {
            self.error_at(range, message, &[]);
        } else if Kind::FIRST_RESERVED_WORD <= self.token()
            && self.token() <= Kind::LAST_RESERVED_WORD
        {
            let text = self.scanner().token_text();
            self.error_at(
                range,
                &diagnostics::IDENTIFIER_EXPECTED_0_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                &[&text],
            );
        } else {
            self.error_at(range, &diagnostics::IDENTIFIER_EXPECTED, &[]);
        }
        self.create_missing_identifier()
    }

    fn create_missing_identifier(&mut self) -> Self::Node {
        let node = self.new_identifier("");
        let pos = self.scanner().token_full_start();
        self.finish_node(node, pos)
    }

    fn parse_private_identifier(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        let text = self.take_current_text();
        self.next_token();
        let node = self.new_private_identifier(text);
        self.finish_node(node, pos)
    }

    fn parse_entity_name(
        &mut self,
        allow_reserved_words: bool,
        allow_private_name: bool,
        diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        let pos = self.scanner().token_full_start();
        let is_identifier = if allow_reserved_words {
            token_is_identifier_or_keyword(self.token())
        } else {
            self.is_identifier()
        };
        let mut entity =
            self.create_identifier_with_diagnostic(is_identifier, diagnostic_message, None);
        while self.token() == Kind::DotToken {
            self.next_token();
            if self.token() == Kind::LessThanToken {
                // JSDoc generics report the name/type-argument gap in the checker.
                break;
            }
            let right =
                self.parse_right_side_of_dot(allow_reserved_words, allow_private_name, true);
            let node = self.new_qualified_name(entity, right);
            entity = self.finish_node(node, pos);
        }
        entity
    }

    fn parse_right_side_of_dot(
        &mut self,
        allow_identifier_names: bool,
        allow_private_identifiers: bool,
        allow_unicode_escape_sequence_in_identifier_name: bool,
    ) -> Self::Node {
        // `name.\nidentifier keyword` recovers before the identifier. Without
        // that newline the name can continue and ASI applies after it instead.
        if self.scanner().has_preceding_line_break()
            && token_is_identifier_or_keyword(self.token())
            && self.look_ahead(|parser| {
                token_is_identifier_or_keyword(parser.next_token())
                    && !parser.scanner().has_preceding_line_break()
            })
        {
            let pos = self.scanner().token_full_start();
            self.error_at(TextRange::new(pos, pos), &diagnostics::IDENTIFIER_EXPECTED, &[]);
            return self.create_missing_identifier();
        }
        if self.token() == Kind::PrivateIdentifier {
            let node = self.parse_private_identifier();
            if allow_private_identifiers {
                return node;
            }
            let pos = self.scanner().token_full_start();
            self.error_at(TextRange::new(pos, pos), &diagnostics::IDENTIFIER_EXPECTED, &[]);
            return self.create_missing_identifier();
        }
        if allow_identifier_names {
            if !allow_unicode_escape_sequence_in_identifier_name
                && (self.scanner().has_unicode_escape()
                    || self.scanner().has_extended_unicode_escape())
            {
                self.error_at(
                    self.scanner().token_range(),
                    &diagnostics::UNICODE_ESCAPE_SEQUENCE_CANNOT_APPEAR_HERE,
                    &[],
                );
            }
            return self.create_identifier_with_diagnostic(
                token_is_identifier_or_keyword(self.token()),
                None,
                None,
            );
        }
        let save_has_await_identifier = self.statement_has_await_identifier();
        let is_identifier = self.is_identifier();
        let id = self.create_identifier_with_diagnostic(is_identifier, None, None);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        id
    }
}
