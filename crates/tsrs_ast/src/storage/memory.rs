//! Buffer requests and hash-map payload bounds for the owned prototype. These
//! exclude allocator size classes, resident pages and whole-compiler RSS.

use std::ops::Add;

use rustc_hash::FxHashMap;

use super::AstStore;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AstBufferMemory {
    pub live_bytes: usize,
    pub capacity_bytes: usize,
    pub allocations: usize,
}

impl AstBufferMemory {
    pub(super) const fn object(bytes: usize) -> Self {
        Self { live_bytes: bytes, capacity_bytes: bytes, allocations: 1 }
    }
}

impl Add for AstBufferMemory {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            live_bytes: self.live_bytes + rhs.live_bytes,
            capacity_bytes: self.capacity_bytes + rhs.capacity_bytes,
            allocations: self.allocations + rhs.allocations,
        }
    }
}

pub(super) trait BufferMemory {
    fn buffer_memory(&self) -> AstBufferMemory;
}

impl<T> BufferMemory for Vec<T> {
    fn buffer_memory(&self) -> AstBufferMemory {
        let width = size_of::<T>();
        AstBufferMemory {
            live_bytes: self.len() * width,
            capacity_bytes: self.capacity() * width,
            allocations: usize::from(self.capacity() != 0 && width != 0),
        }
    }
}

impl BufferMemory for String {
    fn buffer_memory(&self) -> AstBufferMemory {
        AstBufferMemory {
            live_bytes: self.len(),
            capacity_bytes: self.capacity(),
            allocations: usize::from(self.capacity() != 0),
        }
    }
}

impl<T> BufferMemory for hashbrown::HashTable<T> {
    fn buffer_memory(&self) -> AstBufferMemory {
        AstBufferMemory {
            live_bytes: self.len() * size_of::<T>(),
            capacity_bytes: self.allocation_size(),
            allocations: usize::from(self.allocation_size() != 0),
        }
    }
}

/// Public `HashMap` APIs expose entry capacity, but not bucket/control bytes.
/// These payload totals are lower bounds, separate from exact buffer requests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AstMapMemory {
    pub live_payload_bytes: usize,
    pub capacity_payload_bytes: usize,
    pub allocated_maps: usize,
}

impl Add for AstMapMemory {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            live_payload_bytes: self.live_payload_bytes + rhs.live_payload_bytes,
            capacity_payload_bytes: self.capacity_payload_bytes + rhs.capacity_payload_bytes,
            allocated_maps: self.allocated_maps + rhs.allocated_maps,
        }
    }
}

impl AstMapMemory {
    pub(super) fn of<K, V>(map: &FxHashMap<K, V>) -> Self {
        Self {
            live_payload_bytes: map.len() * size_of::<(K, V)>(),
            capacity_payload_bytes: map.capacity() * size_of::<(K, V)>(),
            allocated_maps: usize::from(map.capacity() != 0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFileInfoMemory {
    /// Inline immutable info; also the size of its exclusive draft. Excludes
    /// any enclosing Arc/control block, file entry or mapped-output bundle.
    pub inline_bytes: usize,
    pub file_name: AstBufferMemory,
    /// Shared directive header, vectors and all their owned strings. Updates can
    /// retain the same block: deduplicate by the borrowed directives' identity.
    ///
    /// The block's Arc control bytes are excluded.
    pub directives: AstBufferMemory,
    /// A file's optional check-JS value, excluded from `SourceFile` update sharing.
    pub check_js_directive: AstBufferMemory,
    /// Initialized line/position map buffers. Uninitialized maps allocate nothing.
    pub text_maps: AstBufferMemory,
    /// Pragma argument map payload lower bounds, separate from buffer requests.
    pub maps: AstMapMemory,
    /// Shared Arc payloads; count once across all retaining owners, separately
    /// from buffers. Arc control blocks and allocator overhead are excluded.
    pub source_bytes: usize,
    pub path_bytes: usize,
}

impl SourceFileInfoMemory {
    pub fn buffers(self) -> AstBufferMemory {
        self.file_name + self.directives + self.check_js_directive + self.text_maps
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingMemory {
    /// Inline `BindingData`, excluding its owner, lock and Arc control block.
    pub inline_bytes: usize,
    /// Symbol records, optional tails and compact declaration elements.
    pub symbols: AstBufferMemory,
    /// Ordered table records, entries, extras, odd names and hash indexes.
    pub tables: AstBufferMemory,
    /// Annotation directories, boxed column headers and populated columns.
    pub annotations: AstBufferMemory,
    pub flows: AstBufferMemory,
    pub text: AstBufferMemory,
    /// Foreign syntax keys; their reverse map is counted in `maps`.
    pub foreign: AstBufferMemory,
    /// Optional deferred-list header and pending queue; its map is in `maps`.
    pub deferred: AstBufferMemory,
    /// Optional published-write journal, including its boxed header.
    pub validation: AstBufferMemory,
    /// Payload lower bounds for all owned standard hash maps. Do not add these
    /// to buffers and describe the result as an exact allocation/RSS total.
    pub maps: AstMapMemory,
    /// Retained fragment bodies are counted separately, once per store identity.
    pub retained_fragments: usize,
    /// Shared source bytes, excluding their Arc header; do not count per store.
    pub source_bytes: usize,
}

impl BindingMemory {
    /// Requested buffer/box/hash-index bytes, excluding standard hash map backing
    /// allocations, retained syntax, source sharing and allocator overhead.
    pub fn buffers(self) -> AstBufferMemory {
        self.symbols
            + self.tables
            + self.annotations
            + self.flows
            + self.text
            + self.foreign
            + self.deferred
            + self.validation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AstStoreMemory {
    /// Metadata stored inline in `AstStore`; excludes its `Arc` control block.
    pub inline_bytes: usize,
    /// Node table directory, boxed column headers, column buffers and capacity.
    pub nodes: AstBufferMemory,
    /// Node/modifier list records and their shared element buffer.
    pub lists: AstBufferMemory,
    /// Owned cooked text and its span descriptors.
    pub text: AstBufferMemory,
    /// Qualified foreign node/list keys. Construction maps have been discarded.
    pub foreign: AstBufferMemory,
    /// Source bytes may be shared by other stores/owners. Count separately.
    pub source_bytes: usize,
}

impl AstStoreMemory {
    pub fn buffers(self) -> AstBufferMemory {
        self.nodes + self.lists + self.text + self.foreign
    }
}

impl AstStore {
    /// Counts storage requested by this store. Excludes source sharing/control
    /// blocks, allocator overhead and the file-owner/directory registrations.
    pub fn memory(&self) -> AstStoreMemory {
        AstStoreMemory {
            inline_bytes: size_of::<Self>(),
            nodes: self.payloads.buffer_memory(),
            lists: self.lists.buffer_memory(),
            text: self.texts.buffer_memory(),
            foreign: self.foreign.buffer_memory()
                + self.foreign_lists.buffer_memory()
                + self.foreign_modifiers.buffer_memory(),
            source_bytes: self.source.len(),
        }
    }
}

impl super::AstStoreBuilder {
    /// Read-only construction accounting; map bucket overhead is excluded.
    pub fn memory(&self) -> AstStoreMemory {
        AstStoreMemory {
            inline_bytes: size_of::<Self>(),
            nodes: self.payloads.buffer_memory(),
            lists: self.lists.buffer_memory(),
            text: self.texts.buffer_memory(),
            foreign: self.foreign.buffer_memory()
                + self.foreign_lists.buffer_memory()
                + self.foreign_modifiers.buffer_memory(),
            source_bytes: self.source.len(),
        }
    }

    pub fn node_table_lengths(&self) -> Vec<usize> {
        self.payloads.lengths()
    }
}
