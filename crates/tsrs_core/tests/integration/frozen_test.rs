use tsrs_core::frozen::*;

/// A value with a word to spare for the flag (bit 0 of `flag`).
#[derive(Default)]
struct Words {
    items: Vec<u32>,
    flag: u32,
}

// SAFETY: `flag` is a `u32` field nothing but the cell writes.
unsafe impl WriteFlag for Words {
    const FLAG_OFFSET: usize = std::mem::offset_of!(Words, flag);
    const FLAG: u32 = 1;
}

fn words(items: &[u32]) -> FrozenCell<Words> {
    FrozenCell::new(Words { items: items.to_vec(), flag: 0 })
}

#[test]
fn borrow_and_mutate() {
    let c = FrozenCell::new(Words { items: vec![1], flag: 1 });
    c.borrow_mut().items.push(2);
    c.borrow_mut().items.push(3);
    assert_eq!(&c.borrow().items, &[1, 2, 3]);
    let a = c.borrow();
    let b = c.borrow();
    assert_eq!(a.items.len() + b.items.len(), 6);
    assert_eq!(a.flag, 0, "the flag is clear outside a borrow_mut (also at construction)");
}

#[cfg(any(debug_assertions, feature = "checked-cells"))]
#[test]
#[should_panic(expected = "FrozenCell already borrowed")]
fn aliasing_is_detected() {
    let c = words(&[1]);
    let _r = c.borrow();
    c.borrow_mut().items.push(2);
}

/// Issue 646's reproduction: two live `borrow_mut` guards, caught in every build.
#[test]
#[should_panic(expected = "FrozenCell already borrowed")]
fn second_borrow_mut_panics() {
    let c = words(&[0]);
    let _first = c.borrow_mut();
    let _second = c.borrow_mut();
}
