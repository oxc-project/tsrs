use std::num::{NonZeroU16, NonZeroU32};
use std::sync::atomic::{AtomicU32, Ordering};

use super::AstStoreError;

pub(super) const FOREIGN_EDGE: u32 = 1 << 31;
pub(super) const MAX_INDEX: u32 = FOREIGN_EDGE - 1;
const ELEMENT_BITS: u32 = 20;
pub(super) const NODE_TABLE_CAPACITY: usize = 1 << ELEMENT_BITS;
const NODE_TABLE_LIMIT: usize = 1 << (31 - ELEMENT_BITS);

/// A present node table, encoded one-based so the builder's optional lookup
/// entries remain two bytes. It never appears in a public key on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct NodeTableIndex(NonZeroU16);

impl NodeTableIndex {
    pub(super) const fn from_offset(offset: usize) -> Result<Self, AstStoreError> {
        if offset >= NODE_TABLE_LIMIT {
            return Err(AstStoreError::NodeCapacity);
        }
        let encoded = (offset + 1) as u16;
        Ok(Self(NonZeroU16::new(encoded).expect("table indexes start at one")))
    }

    pub(super) const fn offset(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

static NEXT_STORE_ID: AtomicU32 = AtomicU32::new(1);

/// Identity of one owned AST store. Identities are never reused within a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct AstStoreId(NonZeroU32);

impl AstStoreId {
    pub fn allocate() -> Result<Self, AstStoreError> {
        // Relaxed: uniqueness is required; this counter publishes no AST storage.
        let id = NEXT_STORE_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map_err(|_| AstStoreError::StoreIdExhausted)?;
        Ok(Self(NonZeroU32::new(id).expect("store identities start at one")))
    }
}

/// An opaque address within one store; its high bit is reserved for foreign edges.
/// Node tables and their elements share the remaining 31 bits. This is not a
/// semantic id or allocation-order counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NodeIndex(NonZeroU32);

impl NodeIndex {
    /// Only storage constructs addresses; scoped slots must not be forged.
    pub(super) const fn from_offset(offset: usize) -> Result<Self, AstStoreError> {
        if offset >= MAX_INDEX as usize {
            return Err(AstStoreError::NodeCapacity);
        }
        let encoded = offset as u32 + 1;
        Ok(Self(NonZeroU32::new(encoded).expect("encoded indexes start at one")))
    }

    pub const fn offset(self) -> usize {
        (self.0.get() - 1) as usize
    }

    pub(super) const fn in_table(
        table: NodeTableIndex,
        element: usize,
    ) -> Result<Self, AstStoreError> {
        if element >= NODE_TABLE_CAPACITY {
            return Err(AstStoreError::NodeCapacity);
        }
        Self::from_offset(table.offset() * NODE_TABLE_CAPACITY + element)
    }

    pub(super) const fn table(self) -> NodeTableIndex {
        let encoded = ((self.offset() >> ELEMENT_BITS) + 1) as u16;
        NodeTableIndex(NonZeroU16::new(encoded).expect("table indexes start at one"))
    }

    pub(super) const fn element(self) -> usize {
        self.offset() & (NODE_TABLE_CAPACITY - 1)
    }
}

/// An owner-qualified node identity. A key grants neither retention nor mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct NodeKey {
    store: AstStoreId,
    index: NodeIndex,
}

impl NodeKey {
    pub(super) const fn new(store: AstStoreId, index: NodeIndex) -> Self {
        Self { store, index }
    }

    pub const fn store(self) -> AstStoreId {
        self.store
    }

    pub const fn index(self) -> NodeIndex {
        self.index
    }
}

macro_rules! qualified_key {
    ($index:ident, $key:ident, $capacity:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        pub struct $index(NonZeroU32);

        impl $index {
            pub(super) fn from_offset(offset: usize) -> Result<Self, AstStoreError> {
                let encoded = u32::try_from(offset)
                    .ok()
                    .and_then(|offset| offset.checked_add(1))
                    .filter(|&index| index <= MAX_INDEX)
                    .ok_or(AstStoreError::$capacity)?;
                Ok(Self(NonZeroU32::new(encoded).expect("encoded indexes start at one")))
            }

            pub const fn offset(self) -> usize {
                (self.0.get() - 1) as usize
            }
        }

        /// An owner-qualified identity; it grants no access or retention.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(C)]
        pub struct $key {
            store: AstStoreId,
            index: $index,
        }

        impl $key {
            pub(super) const fn new(store: AstStoreId, index: $index) -> Self {
                Self { store, index }
            }

            pub const fn store(self) -> AstStoreId {
                self.store
            }

            pub const fn index(self) -> $index {
                self.index
            }
        }

        const _: () = {
            assert!(size_of::<$index>() == 4);
            assert!(size_of::<$key>() == 8);
            assert!(size_of::<Option<$key>>() == 8);
        };
    };
}

qualified_key!(ListIndex, ListKey, ListCapacity);
qualified_key!(ModifierListIndex, ModifierListKey, ModifierListCapacity);
qualified_key!(SymbolIndex, SymbolKey, SymbolCapacity);
qualified_key!(FlowIndex, FlowKey, FlowCapacity);
qualified_key!(FlowListIndex, FlowListKey, FlowListCapacity);
qualified_key!(SymbolTableIndex, SymbolTableKey, SymbolTableCapacity);

macro_rules! stored_key {
    ($stored:ident, $key:ident, $index:ident) => {
        /// Store-relative encoding; it never crosses the owning store's public boundary.
        #[derive(Clone, Copy)]
        #[repr(transparent)]
        pub(super) struct $stored(NonZeroU32);

        impl $stored {
            pub(super) const fn local(index: $index) -> Self {
                Self(index.0)
            }

            pub(super) fn foreign(offset: usize) -> Result<Self, AstStoreError> {
                let index = u32::try_from(offset)
                    .ok()
                    .and_then(|offset| offset.checked_add(1))
                    .filter(|&index| index <= MAX_INDEX)
                    .ok_or(AstStoreError::ForeignCapacity)?;
                Ok(Self(NonZeroU32::new(index | FOREIGN_EDGE).expect("foreign tag is nonzero")))
            }

            pub(super) const fn resolve(self, store: AstStoreId, foreign: &[$key]) -> $key {
                let encoded = self.0.get();
                if encoded & FOREIGN_EDGE == 0 {
                    $key::new(store, $index(self.0))
                } else {
                    foreign[((encoded & MAX_INDEX) - 1) as usize]
                }
            }
        }

        const _: () = {
            assert!(size_of::<$stored>() == 4);
            assert!(size_of::<Option<$stored>>() == 4);
        };
    };
}

stored_key!(StoredNodeKey, NodeKey, NodeIndex);
stored_key!(StoredListKey, ListKey, ListIndex);
stored_key!(StoredModifierListKey, ModifierListKey, ModifierListIndex);

const _: () = {
    assert!(size_of::<AstStoreId>() == 4);
    assert!(size_of::<NodeTableIndex>() == 2);
    assert!(size_of::<Option<NodeTableIndex>>() == 2);
    assert!(size_of::<NodeIndex>() == 4);
    assert!(size_of::<NodeKey>() == 8);
    assert!(size_of::<Option<NodeKey>>() == 8);
};

// Encoding boundaries are checked without exposing constructors for scoped keys.
const _: () = {
    assert!(NodeIndex::from_offset(0x7fff_fffe).is_ok());
    assert!(NodeIndex::from_offset(0x7fff_ffff).is_err());
    assert!(NodeIndex::from_offset(usize::MAX).is_err());
    assert!(NodeTableIndex::from_offset(NODE_TABLE_LIMIT).is_err());
    let Ok(first) = NodeTableIndex::from_offset(0) else { panic!("first table must be valid") };
    let Ok(next) = NodeTableIndex::from_offset(1) else { panic!("second table must be valid") };
    let Ok(last) = NodeTableIndex::from_offset(NODE_TABLE_LIMIT - 1) else {
        panic!("last table must be valid")
    };
    assert!(NodeIndex::in_table(first, NODE_TABLE_CAPACITY).is_err());
    assert!(NodeIndex::in_table(last, NODE_TABLE_CAPACITY - 1).is_err());
    assert_node_address_round_trip(first, NODE_TABLE_CAPACITY - 1);
    assert_node_address_round_trip(next, 0);
    assert_node_address_round_trip(last, NODE_TABLE_CAPACITY - 2);
};

const fn assert_node_address_round_trip(table: NodeTableIndex, element: usize) {
    let Ok(index) = NodeIndex::in_table(table, element) else {
        panic!("table address must be valid")
    };
    assert!(index.table().offset() == table.offset());
    assert!(index.element() == element);
}
