//! Non-function declarations of `inference.go`.

use crate::*;

/// Go's `InferenceKey` in two words.
///
/// Source and target ids, the walk's priority, the contravariant and bivariant bits. Injective: the ids keep all
/// 32 bits, the priority all its 15 (a circularity, never a walk priority, maps to bit 15), the two bits sit
/// above it.
#[derive(Clone, Copy)]
pub struct VisitedKey {
    ids: u64,
    rest: u32,
}

impl VisitedKey {
    #[inline]
    pub(crate) fn new(
        source: TypeId,
        target: TypeId,
        priority: InferencePriority,
        contravariant: bool,
        bivariant: bool,
    ) -> VisitedKey {
        VisitedKey {
            ids: u64::from(source.0) | (u64::from(target.0) << 32),
            rest: priority.memo_bits().min(1 << 15)
                | (u32::from(contravariant) << 16)
                | (u32::from(bivariant) << 17),
        }
    }

    /// The slot index for `len` slots (a power of two).
    #[inline]
    fn index(self, len: usize) -> usize {
        let h = (self.ids ^ u64::from(self.rest).rotate_left(27))
            .wrapping_mul(0x9e37_79b9_7f4a_7c15);
        (h >> 32) as usize & (len - 1)
    }
}

#[derive(Clone, Copy, Default)]
struct VisitedSlot {
    ids: u64,
    rest: u32,
    /// The table's epoch when the slot was filled; a slot from another epoch is empty.
    epoch: u16,
    status: InferencePriority,
}

const _: () = assert!(std::mem::size_of::<VisitedSlot>() == 16);

/// `InferenceState.visited`: Go's `map[InferenceKey]InferencePriority` of `invokeOnce` as a flat table.
///
/// Open addressing with linear probing. Returning the state to the pool bumps the epoch instead of clearing the
/// slots, so a table that grew once costs nothing to reuse; the slots are cleared only when the epoch wraps.
pub struct VisitedTable {
    slots: Vec<VisitedSlot>,
    count: usize,
    epoch: u16,
}

impl Default for VisitedTable {
    fn default() -> Self {
        VisitedTable { slots: vec![VisitedSlot::default(); 16], count: 0, epoch: 1 }
    }
}

impl VisitedTable {
    /// Go `status, ok := n.visited[key]` followed by `n.visited[key] = Circularity` when absent: the stored status
    /// when the key was visited; None after marking it visited.
    #[inline]
    pub(crate) fn lookup_or_insert(&mut self, key: VisitedKey) -> Option<InferencePriority> {
        if (self.count + 1) * 4 >= self.slots.len() * 3 {
            self.grow();
        }
        let len = self.slots.len();
        let mut i = key.index(len);
        loop {
            let slot = &mut self.slots[i];
            if slot.epoch != self.epoch {
                *slot = VisitedSlot {
                    ids: key.ids,
                    rest: key.rest,
                    epoch: self.epoch,
                    status: InferencePriority::CIRCULARITY,
                };
                self.count += 1;
                return None;
            }
            if slot.ids == key.ids && slot.rest == key.rest {
                return Some(slot.status);
            }
            i = (i + 1) & (len - 1);
        }
    }

    /// Go `n.visited[key] = status` for a key `lookup_or_insert` marked in this epoch.
    #[inline]
    pub(crate) fn update(&mut self, key: VisitedKey, status: InferencePriority) {
        let len = self.slots.len();
        let mut i = key.index(len);
        loop {
            let slot = &mut self.slots[i];
            assert!(slot.epoch == self.epoch, "inference visit remains present during its action");
            if slot.ids == key.ids && slot.rest == key.rest {
                slot.status = status;
                return;
            }
            i = (i + 1) & (len - 1);
        }
    }

    /// Forgets every entry (the state goes back to the pool).
    pub(crate) fn reset(&mut self) {
        if self.count == 0 {
            return;
        }
        self.count = 0;
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.slots.fill(VisitedSlot::default());
            self.epoch = 1;
        }
    }

    #[cold]
    #[inline(never)]
    fn grow(&mut self) {
        let len = self.slots.len() * 2;
        let old = std::mem::replace(&mut self.slots, vec![VisitedSlot::default(); len]);
        for slot in old {
            if slot.epoch == self.epoch {
                let mut i = VisitedKey { ids: slot.ids, rest: slot.rest }.index(len);
                while self.slots[i].epoch == 1 {
                    i = (i + 1) & (len - 1);
                }
                self.slots[i] = VisitedSlot { epoch: 1, ..slot };
            }
        }
        self.epoch = 1;
    }
}

/// Arena handle (`P<InferenceState>`), pooled through `Checker::freeinference_state` like Go.
#[derive(Default)]
pub struct InferenceState {
    pub(crate) inferences: RefCell<Vec<P<InferenceInfo>>>,
    pub(crate) original_source: Cell<Option<P<Type>>>,
    pub(crate) original_target: Cell<Option<P<Type>>>,
    pub(crate) priority: Cell<InferencePriority>,
    pub(crate) inference_priority: Cell<InferencePriority>,
    pub(crate) contravariant: Cell<bool>,
    pub(crate) bivariant: Cell<bool>,
    pub(crate) expanding_flags: Cell<ExpandingFlags>,
    pub(crate) propagation_type: Cell<Option<P<Type>>>,
    // Scratch owned by this checker's inference-state pool. Forks start with an empty pool, so a
    // frozen seed's unused states are never reused and this table needs no shared-graph overlay.
    pub(crate) visited: RefCell<VisitedTable>,
    pub(crate) source_stack: RefCell<Vec<RelaterStackEntry>>,
    pub(crate) target_stack: RefCell<Vec<RelaterStackEntry>>,
    pub(crate) next: Cell<Option<P<InferenceState>>>,
    /// The walk called `clear_cached_inferences` (the inference memo, infermemo.rs).
    pub(crate) cleared_inferences: Cell<bool>,
}
