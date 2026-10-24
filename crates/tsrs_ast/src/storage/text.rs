use tsrs_core::TextRange;

use super::AstStoreError;
use super::memory::{AstBufferMemory, BufferMemory};

/// Text supplied by a scanner/factory, without extending its input's lifetime.
#[derive(Clone, Copy)]
pub enum TextInput<'a> {
    Source(TextRange),
    Owned(&'a str),
}

/// Source byte range, or an index into owned text spans when `start` is MAX.
/// No source position can use that sentinel (`SourcePos::MAX` is MAX - 1).
#[derive(Clone, Copy)]
pub(super) struct StoredText {
    start: u32,
    len: u32,
}

#[derive(Clone, Copy)]
pub(super) struct StoredTextList {
    start: u32,
    len: u32,
}

#[derive(Default)]
pub(super) struct TextColumns {
    owned: String,
    // Owned string ranges and text-list descriptors share this buffer. Only
    // checked encoders construct indexes into the corresponding record domain.
    spans: Vec<StoredText>,
}

#[derive(Clone, Copy)]
pub(super) struct TextCheckpoint {
    owned: usize,
    spans: usize,
}

impl BufferMemory for TextColumns {
    fn buffer_memory(&self) -> AstBufferMemory {
        self.owned.buffer_memory() + self.spans.buffer_memory()
    }
}

impl TextColumns {
    pub(super) fn checkpoint(&self) -> TextCheckpoint {
        TextCheckpoint { owned: self.owned.len(), spans: self.spans.len() }
    }

    pub(super) fn restore(&mut self, checkpoint: TextCheckpoint, release: bool) {
        self.owned.truncate(checkpoint.owned);
        self.spans.truncate(checkpoint.spans);
        if release {
            self.owned.shrink_to_fit();
            self.spans.shrink_to_fit();
        }
    }

    pub(super) fn encode(
        &mut self,
        source: &str,
        input: TextInput<'_>,
    ) -> Result<StoredText, AstStoreError> {
        match input {
            TextInput::Source(range) => {
                let (Some(start), Some(end)) = (range.source_pos(), range.source_end()) else {
                    return Err(AstStoreError::InvalidTextRange(range));
                };
                source
                    .get(start as usize..end as usize)
                    .ok_or(AstStoreError::InvalidTextRange(range))?;
                Ok(StoredText { start, len: end - start })
            }
            TextInput::Owned(text) => {
                let index = u32::try_from(self.spans.len())
                    .map_err(|_| AstStoreError::OwnedTextCapacity)?;
                let start = u32::try_from(self.owned.len())
                    .map_err(|_| AstStoreError::OwnedTextCapacity)?;
                let len =
                    u32::try_from(text.len()).map_err(|_| AstStoreError::OwnedTextCapacity)?;
                start.checked_add(len).ok_or(AstStoreError::OwnedTextCapacity)?;
                self.owned.reserve(text.len());
                self.spans.reserve(1);
                self.owned.push_str(text);
                self.spans.push(StoredText { start, len });
                Ok(StoredText { start: u32::MAX, len: index })
            }
        }
    }

    pub(super) fn encode_list(
        &mut self,
        source: &str,
        inputs: &[TextInput<'_>],
    ) -> Result<StoredTextList, AstStoreError> {
        if inputs.is_empty() {
            return Ok(StoredTextList { start: 0, len: 0 });
        }
        let start =
            u32::try_from(self.spans.len()).map_err(|_| AstStoreError::OwnedTextCapacity)?;
        let len = u32::try_from(inputs.len()).map_err(|_| AstStoreError::OwnedTextCapacity)?;
        let end = start.checked_add(len).ok_or(AstStoreError::OwnedTextCapacity)?;
        let old_text_len = self.owned.len();
        // Reserve the contiguous list first. Encoding cooked strings can append
        // their own range descriptors after it without a temporary descriptor Vec.
        self.spans.resize(end as usize, StoredText::EMPTY);
        for (offset, &input) in inputs.iter().enumerate() {
            match self.encode(source, input) {
                Ok(text) => self.spans[start as usize + offset] = text,
                Err(error) => {
                    // These descriptors are still private; failed input must not
                    // retain newly copied text or expose a partial list.
                    self.spans.truncate(start as usize);
                    self.owned.truncate(old_text_len);
                    return Err(error);
                }
            }
        }
        Ok(StoredTextList { start, len })
    }

    pub(super) const fn list<'a>(
        &'a self,
        source: &'a str,
        span: StoredTextList,
    ) -> TextListView<'a> {
        TextListView { source, texts: self, span }
    }
}

pub struct TextListView<'a> {
    source: &'a str,
    texts: &'a TextColumns,
    span: StoredTextList,
}

impl<'a> TextListView<'a> {
    pub const fn len(&self) -> usize {
        self.span.len as usize
    }
    pub const fn is_empty(&self) -> bool {
        self.span.len == 0
    }
    pub fn get(&self, index: usize) -> Option<&'a str> {
        if index >= self.len() {
            return None;
        }
        Some(self.texts.spans[self.span.start as usize + index].resolve(self.source, self.texts))
    }
    pub fn texts(
        &self,
    ) -> impl ExactSizeIterator<Item = &'a str> + DoubleEndedIterator + Clone + use<'a> {
        let source = self.source;
        let texts = self.texts;
        let start = self.span.start as usize;
        texts.spans[start..start + self.span.len as usize]
            .iter()
            .map(move |&text| text.resolve(source, texts))
    }
}

impl StoredText {
    pub(super) const EMPTY: Self = Self { start: 0, len: 0 };

    pub(super) fn resolve<'a>(self, source: &'a str, texts: &'a TextColumns) -> &'a str {
        if self.start == u32::MAX {
            let span = texts.spans[self.len as usize];
            &texts.owned[span.start as usize..span.start as usize + span.len as usize]
        } else {
            &source[self.start as usize..self.start as usize + self.len as usize]
        }
    }
}

const _: () = assert!(size_of::<StoredText>() == 8);
