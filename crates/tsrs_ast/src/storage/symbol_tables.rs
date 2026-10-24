//! File-local, ordered binding tables. Entry words contain checked indexes,
//! never pointer bits. Readers borrow the binding owner's actual guard.

use hashbrown::HashTable;

use super::{BindingData, SymbolView};
use crate::HashedName;
use crate::storage::memory::{AstBufferMemory, BufferMemory};
use crate::storage::text::StoredText;
use crate::storage::{
    AstStoreError, AstStoreId, SymbolIndex, SymbolKey, SymbolTableIndex, SymbolTableKey,
};

const LINEAR_MAX: usize = 16;
const ODD_KEY: u32 = 1 << 31;

#[derive(Clone, Copy)]
struct Entry {
    symbol: SymbolIndex,
    fingerprint: u32,
}

/// One saved table entry, without a borrow or strong reference to its owner.
///
/// Names are immutable text descriptors, independent of later table replacement
/// or deletion. Retain the binding owner separately and reacquire the name with
/// `BindingData::read_symbol_table_entry` after a forcing callback.
#[derive(Clone, Copy)]
pub struct SymbolTableEntrySnapshot {
    owner: AstStoreId,
    name: StoredText,
    symbol: SymbolIndex,
}

impl SymbolTableEntrySnapshot {
    pub const fn symbol_key(self) -> SymbolKey {
        SymbolKey::new(self.owner, self.symbol)
    }
}

fn fingerprint(key: HashedName<'_>) -> u32 {
    key.name().len().min(63) as u32 | ((key.hash() >> 20) << 6)
}

fn index_hash(hash: u32) -> u64 {
    u64::from(hash) | (u64::from(hash) << 44)
}

fn filter_bits(hash: u32) -> u64 {
    (1 << (hash & 63)) | (1 << ((hash >> 6) & 63))
}

/// An odd word is the small-table filter. An even word is a one-based index
/// into the file's extra-record vector. This integer tag owns no allocation.
pub(super) struct SymbolTableRecord {
    entries: Vec<Entry>,
    filter_or_extra: u64,
}

impl Default for SymbolTableRecord {
    fn default() -> Self {
        Self { entries: Vec::new(), filter_or_extra: 1 }
    }
}

#[derive(Clone, Default)]
pub(super) struct SymbolTableExtra {
    index: Option<HashTable<u32>>,
    odd_keys: Vec<(u32, StoredText)>,
}

impl SymbolTableRecord {
    fn extra_index(&self) -> Option<usize> {
        (self.filter_or_extra & 1 == 0).then(|| (self.filter_or_extra / 2 - 1) as usize)
    }

    fn extra<'a>(&self, extras: &'a [SymbolTableExtra]) -> Option<&'a SymbolTableExtra> {
        self.extra_index().map(|index| &extras[index])
    }

    fn key_text(&self, data: &BindingData, offset: usize) -> StoredText {
        let entry = self.entries[offset];
        if entry.fingerprint & ODD_KEY == 0 {
            data.symbols[entry.symbol.offset()].name
        } else {
            self.extra(&data.table_extras)
                .expect("odd keys have an extra record")
                .odd_keys
                .iter()
                .find(|&&(index, _)| index as usize == offset)
                .expect("odd key has a stored descriptor")
                .1
        }
    }

    fn key<'a>(&self, data: &'a BindingData, offset: usize) -> &'a str {
        self.key_text(data, offset).resolve(&data.source, &data.texts)
    }

    fn position(&self, data: &BindingData, name: HashedName<'_>) -> Option<usize> {
        if self.filter_or_extra & 1 != 0
            && self.filter_or_extra & filter_bits(name.hash()) != filter_bits(name.hash())
        {
            return None;
        }
        let print = fingerprint(name);
        let matches = |offset: usize| {
            self.entries[offset].fingerprint & !ODD_KEY == print
                && self.key(data, offset) == name.name()
        };
        match self.extra(&data.table_extras).and_then(|extra| extra.index.as_ref()) {
            Some(index) => index
                .find(index_hash(name.hash()), |&offset| matches(offset as usize))
                .map(|&offset| offset as usize),
            None => (0..self.entries.len()).find(|&offset| matches(offset)),
        }
    }
}

impl BindingData {
    /// The name borrows this actual data/guard. The opaque saved entry alone
    /// grants no access and cannot extend that borrow across another write.
    pub fn read_symbol_table_entry(
        &self,
        entry: SymbolTableEntrySnapshot,
    ) -> Result<(&str, SymbolKey), AstStoreError> {
        self.check_owner(entry.owner)?;
        Ok((entry.name.resolve(&self.source, &self.texts), entry.symbol_key()))
    }

    pub(super) fn table_memory(&self) -> AstBufferMemory {
        let entries = self.tables.iter().fold(AstBufferMemory::default(), |memory, table| {
            memory + table.entries.buffer_memory()
        });
        let extras = self.table_extras.iter().fold(AstBufferMemory::default(), |memory, extra| {
            memory
                + extra.odd_keys.buffer_memory()
                + extra
                    .index
                    .as_ref()
                    .map_or_else(AstBufferMemory::default, BufferMemory::buffer_memory)
        });
        self.tables.buffer_memory() + self.table_extras.buffer_memory() + entries + extras
    }

    pub(in crate::storage) fn table_index(
        &self,
        key: SymbolTableKey,
    ) -> Result<SymbolTableIndex, AstStoreError> {
        self.check_owner(key.store())?;
        self.tables
            .get(key.index().offset())
            .ok_or(AstStoreError::MissingSymbolTable(key.index()))?;
        Ok(key.index())
    }

    fn table_extra_mut(&mut self, table: usize) -> Result<&mut SymbolTableExtra, AstStoreError> {
        let index = if let Some(index) = self.tables[table].extra_index() {
            index
        } else {
            let index = u32::try_from(self.table_extras.len())
                .map_err(|_| AstStoreError::SymbolTableCapacity)?;
            self.table_extras.push(SymbolTableExtra::default());
            self.tables[table].filter_or_extra = (u64::from(index) + 1) * 2;
            index as usize
        };
        Ok(&mut self.table_extras[index])
    }

    pub fn new_symbol_table(&mut self) -> Result<SymbolTableKey, AstStoreError> {
        let index = SymbolTableIndex::from_offset(self.tables.len())?;
        self.tables.push(SymbolTableRecord::default());
        Ok(SymbolTableKey::new(self.id, index))
    }

    pub fn symbol_table(&self, key: SymbolTableKey) -> Result<SymbolTableView<'_>, AstStoreError> {
        let index = self.table_index(key)?;
        Ok(SymbolTableView { data: self, index })
    }

    /// Replacement preserves the original key and insertion position, including
    /// a table entry whose key differs from the replacement symbol's name.
    pub fn set_symbol_table_entry(
        &mut self,
        table: SymbolTableKey,
        name: &str,
        symbol: SymbolKey,
    ) -> Result<(), AstStoreError> {
        let table = self.table_index(table)?.offset();
        let symbol = self.symbol_index(symbol)?;
        let hashed = HashedName::new(name);
        let old = self.tables[table].position(self, hashed);
        let offset = old.unwrap_or(self.tables[table].entries.len());
        u32::try_from(offset).map_err(|_| AstStoreError::SymbolTableCapacity)?;
        let already_odd =
            old.is_some_and(|offset| self.tables[table].entries[offset].fingerprint & ODD_KEY != 0);
        let needs_odd = !already_odd
            && self.symbols[symbol.offset()].name.resolve(&self.source, &self.texts) != name;
        // Allocate any rare name/extra before exposing a new entry.
        let odd = if needs_odd {
            Some(self.texts.encode(&self.source, super::TextInput::Owned(name))?)
        } else {
            None
        };
        self.insert_symbol_table_entry(table, symbol, hashed.hash(), fingerprint(hashed), old, odd)
    }

    /// Set a saved name after a callback, without borrowing or copying its text.
    ///
    /// The snapshot, target table and replacement symbol must have this binding
    /// owner. The saved name is used even if its original entry was deleted.
    pub fn set_symbol_table_entry_from_snapshot(
        &mut self,
        table: SymbolTableKey,
        entry: SymbolTableEntrySnapshot,
        symbol: SymbolKey,
    ) -> Result<(), AstStoreError> {
        self.check_owner(entry.owner)?;
        let table = self.table_index(table)?.offset();
        let symbol = self.symbol_index(symbol)?;
        let name = entry.name.resolve(&self.source, &self.texts);
        let hashed = HashedName::new(name);
        let old = self.tables[table].position(self, hashed);
        let already_odd =
            old.is_some_and(|offset| self.tables[table].entries[offset].fingerprint & ODD_KEY != 0);
        let odd = (!already_odd
            && self.symbols[symbol.offset()].name.resolve(&self.source, &self.texts) != name)
            .then_some(entry.name);
        self.insert_symbol_table_entry(table, symbol, hashed.hash(), fingerprint(hashed), old, odd)
    }

    fn insert_symbol_table_entry(
        &mut self,
        table: usize,
        symbol: SymbolIndex,
        hash: u32,
        print: u32,
        old: Option<usize>,
        odd: Option<StoredText>,
    ) -> Result<(), AstStoreError> {
        let offset = old.unwrap_or(self.tables[table].entries.len());
        u32::try_from(offset).map_err(|_| AstStoreError::SymbolTableCapacity)?;
        if odd.is_some() {
            self.table_extra_mut(table)?.odd_keys.reserve(1);
        }
        if let Some(offset) = old {
            self.tables[table].entries[offset].symbol = symbol;
        } else {
            let entries = &mut self.tables[table].entries;
            let len = entries.len();
            if len == entries.capacity() && len >= 8 {
                entries.reserve_exact(len / 2);
            } else if len == entries.capacity() && len < 2 {
                entries.reserve_exact(1);
            }
            entries.push(Entry { symbol, fingerprint: print });
            if self.tables[table].extra_index().is_none() {
                self.tables[table].filter_or_extra |= filter_bits(hash);
            }
        }
        if let Some(stored) = odd {
            self.table_extra_mut(table)?.odd_keys.push((offset as u32, stored));
            self.tables[table].entries[offset].fingerprint |= ODD_KEY;
        }
        if old.is_none() {
            let extra_index = self.tables[table].extra_index();
            if extra_index.is_some_and(|index| self.table_extras[index].index.is_some()) {
                let extra_index = extra_index.expect("checked extra record");
                // Rehash only needs the table's entries and text, disjoint from
                // the mutable index. The index contains entry positions.
                let mut index = self.table_extras[extra_index].index.take().expect("checked index");
                index.insert_unique(index_hash(hash), offset as u32, |&offset| {
                    index_hash(
                        HashedName::new(self.tables[table].key(self, offset as usize)).hash(),
                    )
                });
                self.table_extras[extra_index].index = Some(index);
            } else if self.tables[table].entries.len() > LINEAR_MAX {
                let mut index = HashTable::with_capacity(self.tables[table].entries.len());
                for offset in 0..self.tables[table].entries.len() {
                    let hash =
                        index_hash(HashedName::new(self.tables[table].key(self, offset)).hash());
                    index.insert_unique(hash, offset as u32, |&offset| {
                        index_hash(
                            HashedName::new(self.tables[table].key(self, offset as usize)).hash(),
                        )
                    });
                }
                self.table_extra_mut(table)?.index = Some(index);
            }
        }
        Ok(())
    }

    pub fn delete_symbol_table_entry(
        &mut self,
        key: SymbolTableKey,
        name: &str,
    ) -> Result<(), AstStoreError> {
        let table = self.table_index(key)?.offset();
        let hashed = HashedName::new(name);
        let Some(offset) = self.tables[table].position(self, hashed) else {
            return Ok(());
        };
        self.remove_symbol_table_entry(table, offset, hashed.hash());
        Ok(())
    }

    /// Delete by the saved name, including after replacement or index shifts.
    pub fn delete_symbol_table_entry_from_snapshot(
        &mut self,
        key: SymbolTableKey,
        entry: SymbolTableEntrySnapshot,
    ) -> Result<(), AstStoreError> {
        self.check_owner(entry.owner)?;
        let table = self.table_index(key)?.offset();
        let hashed = HashedName::new(entry.name.resolve(&self.source, &self.texts));
        let Some(offset) = self.tables[table].position(self, hashed) else {
            return Ok(());
        };
        self.remove_symbol_table_entry(table, offset, hashed.hash());
        Ok(())
    }

    fn remove_symbol_table_entry(&mut self, table: usize, offset: usize, hash: u32) {
        if let Some(extra) =
            self.tables[table].extra_index().map(|index| &mut self.table_extras[index])
        {
            if let Some(index) = &mut extra.index {
                if let Ok(entry) =
                    index.find_entry(index_hash(hash), |&index| index as usize == offset)
                {
                    entry.remove();
                }
                for index in index.iter_mut() {
                    if *index as usize > offset {
                        *index -= 1;
                    }
                }
            }
            extra.odd_keys.retain(|&(index, _)| index as usize != offset);
            for (index, _) in &mut extra.odd_keys {
                if *index as usize > offset {
                    *index -= 1;
                }
            }
        }
        self.tables[table].entries.remove(offset);
    }

    pub fn clone_symbol_table(
        &mut self,
        key: SymbolTableKey,
    ) -> Result<SymbolTableKey, AstStoreError> {
        let table = self.table_index(key)?.offset();
        let index = SymbolTableIndex::from_offset(self.tables.len())?;
        let entries = self.tables[table].entries.clone();
        let mut filter_or_extra = self.tables[table].filter_or_extra;
        if let Some(extra) = self.tables[table].extra_index() {
            let offset = u32::try_from(self.table_extras.len())
                .map_err(|_| AstStoreError::SymbolTableCapacity)?;
            let extra = self.table_extras[extra].clone();
            self.table_extras.push(extra);
            filter_or_extra = (u64::from(offset) + 1) * 2;
        }
        self.tables.push(SymbolTableRecord { entries, filter_or_extra });
        Ok(SymbolTableKey::new(self.id, index))
    }

    pub fn set_symbol_members(
        &mut self,
        key: SymbolKey,
        table: Option<SymbolTableKey>,
    ) -> Result<(), AstStoreError> {
        let symbol = self.symbol_index(key)?.offset();
        let table = table.map(|key| self.table_index(key)).transpose()?;
        if table.is_some() || self.symbol_auxiliary(symbol).is_some() {
            self.symbol_auxiliary_mut(symbol)?.members = table;
        }
        Ok(())
    }

    pub fn set_symbol_exports(
        &mut self,
        key: SymbolKey,
        table: Option<SymbolTableKey>,
    ) -> Result<(), AstStoreError> {
        let symbol = self.symbol_index(key)?.offset();
        let table = table.map(|key| self.table_index(key)).transpose()?;
        if table.is_some() || self.symbol_auxiliary(symbol).is_some() {
            self.symbol_auxiliary_mut(symbol)?.exports = table;
        }
        Ok(())
    }

    pub fn set_symbol_export_symbol(
        &mut self,
        key: SymbolKey,
        export: Option<SymbolKey>,
    ) -> Result<(), AstStoreError> {
        let symbol = self.symbol_index(key)?.offset();
        let export = export.map(|key| self.symbol_index(key)).transpose()?;
        if export.is_some() || self.symbol_auxiliary(symbol).is_some() {
            self.symbol_auxiliary_mut(symbol)?.export_symbol = export;
        }
        Ok(())
    }
}

impl SymbolView<'_> {
    /// This returns the stored table without forcing. Deferred-member forcing
    /// belongs to the caller's exclusive binding context, never a read view.
    pub fn members(&self) -> Option<SymbolTableKey> {
        self.data
            .symbol_auxiliary(self.index.offset())
            .and_then(|aux| aux.members)
            .map(|index| SymbolTableKey::new(self.data.id, index))
    }
    pub fn exports(&self) -> Option<SymbolTableKey> {
        self.data
            .symbol_auxiliary(self.index.offset())
            .and_then(|aux| aux.exports)
            .map(|index| SymbolTableKey::new(self.data.id, index))
    }
    pub fn export_symbol(&self) -> Option<SymbolKey> {
        self.data
            .symbol_auxiliary(self.index.offset())
            .and_then(|aux| aux.export_symbol)
            .map(|index| SymbolKey::new(self.data.id, index))
    }
}

pub struct SymbolTableView<'a> {
    data: &'a BindingData,
    index: SymbolTableIndex,
}

impl<'a> SymbolTableView<'a> {
    fn record(&self) -> &SymbolTableRecord {
        &self.data.tables[self.index.offset()]
    }
    pub const fn key(&self) -> SymbolTableKey {
        SymbolTableKey::new(self.data.id, self.index)
    }
    pub fn len(&self) -> usize {
        self.record().entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn lookup(&self, name: &str) -> Option<SymbolKey> {
        self.lookup_hashed(HashedName::new(name))
    }
    pub fn lookup_hashed(&self, name: HashedName<'_>) -> Option<SymbolKey> {
        self.record()
            .position(self.data, name)
            .map(|offset| SymbolKey::new(self.data.id, self.record().entries[offset].symbol))
    }
    pub fn entry(&self, offset: usize) -> Option<(&'a str, SymbolKey)> {
        let entry = self.record().entries.get(offset)?;
        Some((self.record().key(self.data, offset), SymbolKey::new(self.data.id, entry.symbol)))
    }
    pub fn entries(&self) -> impl Iterator<Item = (&'a str, SymbolKey)> + use<'a> {
        let data = self.data;
        let table = &data.tables[self.index.offset()];
        (0..table.entries.len()).map(move |offset| {
            (table.key(data, offset), SymbolKey::new(data.id, table.entries[offset].symbol))
        })
    }

    /// Preserve the legacy `SymbolTable::entries()` callback snapshot, including
    /// saved names and symbols when callbacks replace/delete table entries.
    /// Copies descriptors and keys, never strings or binding/body ownership.
    pub fn snapshot_entries(&self) -> Vec<SymbolTableEntrySnapshot> {
        let table = self.record();
        table
            .entries
            .iter()
            .enumerate()
            .map(|(offset, entry)| SymbolTableEntrySnapshot {
                owner: self.data.id,
                name: table.key_text(self.data, offset),
                symbol: entry.symbol,
            })
            .collect()
    }
}

const _: () = assert!(size_of::<Entry>() == 8);
const _: () = assert!(size_of::<SymbolTableEntrySnapshot>() == 16);
