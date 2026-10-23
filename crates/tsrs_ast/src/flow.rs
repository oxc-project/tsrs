use bitflags::bitflags;
use tsrs_core::{OwnedCell, P, PKey};

use crate::ast::{Node, NodeFactoryHooks, new_node};
use crate::kind::Kind;

// FlowFlags

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
    pub struct FlowFlags: u32 {
        const NONE = 0;
        const UNREACHABLE = 1 << 0; // Unreachable code
        const START = 1 << 1; // Start of flow graph
        const BRANCH_LABEL = 1 << 2; // Non-looping junction
        const LOOP_LABEL = 1 << 3; // Looping junction
        const ASSIGNMENT = 1 << 4; // Assignment
        const TRUE_CONDITION = 1 << 5; // Condition known to be true
        const FALSE_CONDITION = 1 << 6; // Condition known to be false
        const SWITCH_CLAUSE = 1 << 7; // Switch statement clause
        const ARRAY_MUTATION = 1 << 8; // Potential array mutation
        const CALL = 1 << 9; // Potential assertion call
        const REDUCE_LABEL = 1 << 10; // Temporarily reduce antecedents of label
        const REFERENCED = 1 << 11; // Referenced as antecedent once
        const SHARED = 1 << 12; // Referenced as antecedent more than once
        const LABEL = Self::BRANCH_LABEL.bits() | Self::LOOP_LABEL.bits();
        const CONDITION = Self::TRUE_CONDITION.bits() | Self::FALSE_CONDITION.bits();
    }
}

// FlowNode

/// Shares storage between mutually exclusive flow antecedent forms.
///
/// Go's `Antecedent` (all but labels; set at creation) and `Antecedents` (labels; Go's binder creates labels
/// without an antecedent and gives antecedents only to labels) are never both set, so they share one field
/// (`link`, the `P::key` of either): a node whose flags have `Label` holds the antecedents, any other node
/// the antecedent.
///
/// Label bits never change after creation (only `Referenced` / `Shared` are added). 24 bytes, 16 with
/// compressed pointers; 1.9M flow nodes on the private monorepo.
pub struct FlowNode {
    pub flags: OwnedCell<FlowFlags>,
    // Not in Go: the text index of the file whose binder made this flow node (`NO_SOURCE_TEXT` for the checker's);
    // compact identifiers read their text index from their flow node (identifier.rs).
    pub text_index: u32,
    pub node: OwnedCell<Option<P<Node>>>, // Associated AST node
    link: OwnedCell<PKey>,
}

#[cfg(target_pointer_width = "64")]
const _: () =
    assert!(std::mem::size_of::<FlowNode>() == if tsrs_core::COMPRESSED_PTRS { 16 } else { 24 });
#[cfg(target_pointer_width = "32")]
const _: () = assert!(std::mem::size_of::<FlowNode>() == 16);

impl FlowNode {
    pub fn new(
        flags: FlowFlags,
        node: Option<P<Node>>,
        antecedent: Option<P<FlowNode>>,
        text_index: u32,
    ) -> FlowNode {
        assert!(
            antecedent.is_none() || !flags.intersects(FlowFlags::LABEL),
            "flow label created with an antecedent"
        );
        FlowNode {
            flags: OwnedCell::new(flags),
            text_index,
            node: OwnedCell::new(node),
            link: OwnedCell::new(P::key_opt(antecedent)),
        }
    }
    pub fn flags(&self) -> FlowFlags {
        self.flags.get()
    }
    pub fn node(&self) -> Option<P<Node>> {
        self.node.get()
    }
    #[inline]
    pub fn text_index(&self) -> u32 {
        self.text_index
    }
    /// Go `Antecedent` (antecedent for all but `FlowLabel`).
    #[inline]
    pub fn antecedent(&self) -> Option<P<FlowNode>> {
        // SAFETY: a non-label's link is 0 or the antecedent stored by `new`.
        (!self.flags.get().intersects(FlowFlags::LABEL))
            .then(|| unsafe { P::from_key_opt(self.link.get()) })
            .flatten()
    }
    /// Go `Antecedents` (linked list of antecedents for `FlowLabel`).
    #[inline]
    pub fn antecedents(&self) -> Option<P<FlowList>> {
        // SAFETY: a label's link is 0 or the list stored by `set_antecedents`.
        self.flags
            .get()
            .intersects(FlowFlags::LABEL)
            .then(|| unsafe { P::from_key_opt(self.link.get()) })
            .flatten()
    }
    /// Go `Antecedents = list`. Panics on a flow node that is not a label (Go gives antecedents only to labels).
    pub fn set_antecedents(&self, list: Option<P<FlowList>>) {
        assert!(
            self.flags.get().intersects(FlowFlags::LABEL),
            "antecedents on a flow node that is not a label"
        );
        self.link.set(P::key_opt(list));
    }
}

pub struct FlowList {
    pub flow: P<FlowNode>,
    pub next: OwnedCell<Option<P<FlowList>>>,
}

pub type FlowLabel = FlowNode;

// FlowSwitchClauseData (synthetic AST node for FlowFlagsSwitchClause)

pub struct FlowSwitchClauseData {
    pub switch_statement: P<Node>,
    pub clause_start: usize, // Start index of case/default clause range
    pub clause_end: usize,   // End index of case/default clause range
}

pub fn new_flow_switch_clause_data(
    switch_statement: P<Node>,
    clause_start: usize,
    clause_end: usize,
) -> P<Node> {
    let data = FlowSwitchClauseData { switch_statement, clause_start, clause_end };
    new_node(Kind::Unknown, data, &NodeFactoryHooks::default())
}

impl FlowSwitchClauseData {
    pub fn is_empty(&self) -> bool {
        self.clause_start == self.clause_end
    }
}

// FlowReduceLabelData (synthetic AST node for FlowFlagsReduceLabel)

pub struct FlowReduceLabelData {
    pub target: P<FlowLabel>,             // Target label
    pub antecedents: Option<P<FlowList>>, // Temporary antecedent list
}

pub fn new_flow_reduce_label_data(
    target: P<FlowLabel>,
    antecedents: Option<P<FlowList>>,
) -> P<Node> {
    new_node(
        Kind::Unknown,
        FlowReduceLabelData { target, antecedents },
        &NodeFactoryHooks::default(),
    )
}
