//! Context scopes and token speculation preserve the parser's existing state boundaries.

#![forbid(unsafe_code)]

use crate::recognition::RecognitionParser;
use tsrs_ast::{self as ast, Kind, NodeFlags};

pub trait ContextParser<'src>: RecognitionParser<'src> {
    type State;
    fn mark(&self) -> Self::State;
    fn rewind(&mut self, state: Self::State);
    fn replace_context_flags(&mut self, flags: NodeFlags);

    fn set_context_flags(&mut self, flags: NodeFlags, value: bool) {
        let context_flags =
            if value { self.context_flags() | flags } else { self.context_flags() & !flags };
        self.replace_context_flags(context_flags);
    }

    fn do_in_context<T>(
        &mut self,
        flags: NodeFlags,
        value: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let save_context_flags = self.context_flags();
        self.set_context_flags(flags, value);
        let result = f(self);
        self.replace_context_flags(save_context_flags);
        result
    }

    #[inline]
    fn parse_contextual_modifier(&mut self, t: Kind) -> bool {
        // A nonmatching token would only mark and rewind unchanged parser state.
        if self.token() != t {
            return false;
        }
        self.parse_contextual_modifier_worker()
    }

    #[inline(never)]
    fn parse_contextual_modifier_worker(&mut self) -> bool {
        let state = self.mark();
        if self.next_token_can_follow_modifier() {
            return true;
        }
        self.rewind(state);
        false
    }

    fn parse_any_contextual_modifier(&mut self) -> bool {
        // Not a modifier: Go marks and rewinds an unchanged state (a no-op) before returning false.
        if !ast::is_modifier_kind(self.token()) {
            return false;
        }
        let state = self.mark();
        if self.next_token_can_follow_modifier() {
            return true;
        }
        self.rewind(state);
        false
    }
}

impl ContextParser<'static> for crate::Parser {
    type State = crate::parser_1::ParserState;
    fn mark(&self) -> Self::State {
        crate::Parser::mark(self)
    }
    fn rewind(&mut self, state: Self::State) {
        crate::Parser::rewind(self, state);
    }
    fn replace_context_flags(&mut self, flags: NodeFlags) {
        self.context_flags = flags;
    }
}
