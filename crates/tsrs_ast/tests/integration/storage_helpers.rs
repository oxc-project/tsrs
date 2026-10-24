use tsrs_ast::storage::{AstStoreBuilder, NodeKey};

pub const TABLE_CAPACITY: usize = 1 << 20;

// Exercise actual allocation boundaries through public construction; no scoped
// address constructors or test-only capacity fields are exposed.
pub fn fill_table(ast: &mut AstStoreBuilder, seed: NodeKey, remaining: usize) {
    let mut last = seed;
    while last.index().offset() % TABLE_CAPACITY + 1 < TABLE_CAPACITY - remaining {
        last = ast.clone_node(seed).unwrap();
    }
}
