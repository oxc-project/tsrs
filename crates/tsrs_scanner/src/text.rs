//! Text ownership policies for the shared scanner algorithm.

#![forbid(unsafe_code)]

use std::borrow::{Borrow, Cow};
use std::fmt::{Debug, Display, Write};
use std::hash::Hash;
use std::ops::Deref;

use tsrs_core::ArenaStringBuilder;

use crate::scanner::ScannerStateImpl;

/// The legacy policy preserves arena allocation while AST consumers migrate.
#[derive(Default)]
pub struct ArenaScannerText;

/// Source slices borrow the input; cooked token values own ordinary strings.
#[derive(Default)]
pub struct OwnedScannerText;

pub trait ScannerText<'src>: Default {
    type Text: Clone
        + Default
        + Debug
        + Display
        + Eq
        + Hash
        + Deref<Target = str>
        + AsRef<str>
        + Borrow<str>
        + From<&'src str>;
    type Builder: ScannerTextBuilder<Self::Text>;

    fn copy_text(text: &str) -> Self::Text;
    fn pin_unless_source(source: &str, key: &str, value: &str);

    fn snapshot(state: &ScannerStateImpl<Self::Text>) -> ScannerStateImpl<Self::Text> {
        state.clone()
    }
}

pub trait ScannerTextBuilder<T>: Write {
    fn with_capacity(capacity: usize) -> Self;
    fn push_str(&mut self, text: &str);
    fn push(&mut self, ch: char);
    fn finish(self) -> T;
}

impl ScannerText<'static> for ArenaScannerText {
    type Text = &'static str;
    type Builder = ArenaStringBuilder;

    #[inline]
    fn copy_text(text: &str) -> Self::Text {
        tsrs_core::alloc_str(text)
    }

    #[inline]
    fn snapshot(state: &ScannerStateImpl<Self::Text>) -> ScannerStateImpl<Self::Text> {
        // Preserve the legacy whole-state copy during parser speculation.
        // Fieldwise Clone introduces extra loads/stores in this hot path.
        *state
    }

    fn pin_unless_source(source: &str, key: &str, value: &str) {
        let in_text = |text: &str| {
            let (p, start) = (text.as_ptr() as usize, source.as_ptr() as usize);
            text.is_empty() || (p >= start && p + text.len() <= start + source.len())
        };
        if !in_text(key) || !in_text(value) {
            tsrs_core::arena_pin();
        }
    }
}

impl ScannerTextBuilder<&'static str> for ArenaStringBuilder {
    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }
    #[inline]
    fn push_str(&mut self, text: &str) {
        self.push_str(text);
    }
    #[inline]
    fn push(&mut self, ch: char) {
        self.push(ch);
    }
    #[inline]
    fn finish(self) -> &'static str {
        self.into_str()
    }
}

impl<'src> ScannerText<'src> for OwnedScannerText {
    type Text = Cow<'src, str>;
    type Builder = String;

    fn copy_text(text: &str) -> Self::Text {
        Cow::Owned(text.to_owned())
    }

    fn pin_unless_source(_source: &str, _key: &str, _value: &str) {}
}

impl<'src> ScannerTextBuilder<Cow<'src, str>> for String {
    fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }
    fn push_str(&mut self, text: &str) {
        self.push_str(text);
    }
    fn push(&mut self, ch: char) {
        self.push(ch);
    }
    fn finish(self) -> Cow<'src, str> {
        Cow::Owned(self)
    }
}
