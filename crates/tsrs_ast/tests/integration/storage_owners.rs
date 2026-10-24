use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use tsrs_core::TextRange;

use tsrs_ast::storage::*;
use tsrs_ast::{Kind, NodeFlags};

#[test]
fn loading_guard_binding_and_retirement_preserve_live_leases() {
    let directory = Arc::new(AstDirectory::default());
    let mut builder = AstStoreBuilder::new("value").unwrap();
    let key = builder.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 5)).unwrap();
    let owner = FileAstOwner::parsed(builder, Arc::clone(&directory)).unwrap();
    assert_eq!(owner.phase(), Ok(FileAstPhase::Parsed));
    assert!(matches!(owner.wait_bound(), Err(AstAccessError::NotBound(FileAstPhase::Parsed))));
    {
        let mut parsed = owner.parsed_guard().unwrap();
        assert_eq!(parsed.ast().node(key).unwrap().source_text(), Some("value"));
        parsed.ast_mut().set_flags(key, NodeFlags::ContainsThis).unwrap();
    }
    let BindingAttempt::Started(mut binding) = owner.try_begin_binding().unwrap() else {
        panic!("first binder takes the parsed builder");
    };
    assert_eq!(owner.phase(), Ok(FileAstPhase::Binding));
    assert!(matches!(owner.try_begin_binding(), Ok(BindingAttempt::InProgress)));
    // Entry lock is free during binding, but no public bound read is possible.
    assert!(matches!(owner.parsed_guard(), Err(AstAccessError::NotParsed(FileAstPhase::Binding))));
    binding.ast_mut().mark_trailing_comma(key).unwrap();
    let lease = binding.publish().unwrap();
    assert!(matches!(owner.try_begin_binding(), Ok(BindingAttempt::AlreadyBound(_))));
    assert_eq!(lease.node(key).unwrap().flags(), NodeFlags::ContainsThis);

    let weak = lease.downgrade();
    let mut reads = AstRead::new(directory);
    assert!(matches!(reads.lease(key.store()), Err(AstAccessError::UnpermittedOwner(_))));
    reads.include(Arc::clone(&owner));
    assert_eq!(reads.lease(key.store()).unwrap().node(key).unwrap().key(), key);
    owner.retire().unwrap();
    assert_eq!(owner.phase(), Ok(FileAstPhase::Retired));
    assert_eq!(owner.source_text(), "value");
    assert!(matches!(reads.lease(key.store()), Err(AstAccessError::Retired)));
    assert!(lease.node(key).unwrap().has_trailing_comma_marker());
    assert!(weak.upgrade().is_some());
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn imported_file_directory_exposes_future_extensions_without_ownership_cycles() {
    let original = Arc::new(AstDirectory::default());
    let imported = Arc::new(AstDirectory::default());
    let builder = AstStoreBuilder::new("source").unwrap();
    let owner = FileAstOwner::parsed(builder, Arc::clone(&original)).unwrap();
    let weak_owner = Arc::downgrade(&owner);
    let BindingAttempt::Started(binding) = owner.try_begin_binding().unwrap() else {
        panic!("file is unbound");
    };
    binding.publish().unwrap();
    let mut first_program = AstRead::new(Arc::clone(&original));
    let mut second_program = AstRead::new(imported);
    first_program.include(Arc::clone(&owner));
    second_program.include(Arc::clone(&owner));

    let mut late = AstStoreBuilder::new(Arc::clone(owner.source())).unwrap();
    let token = late.new_node(Kind::Identifier, NodeFlags::None, TextRange::new(0, 6)).unwrap();
    let result = owner.publish_extension(late.finish()).unwrap();
    let weak_body = result.downgrade();
    for program in [&first_program, &second_program] {
        assert_eq!(
            program.lease(token.store()).unwrap().node(token).unwrap().source_text(),
            Some("source")
        );
    }
    drop(owner);
    drop(first_program);
    assert!(weak_owner.upgrade().is_some());
    drop(second_program);
    assert!(weak_owner.upgrade().is_none());
    assert_eq!(result.node(token).unwrap().source_text(), Some("source"));
    assert!(weak_body.upgrade().is_some());
    drop(result);
    assert!(weak_body.upgrade().is_none());
    original.prune_expired().unwrap();
    assert!(matches!(
        AstRead::new(original).lease(token.store()),
        Err(AstAccessError::UnknownStore(_))
    ));
}

#[test]
fn abandoned_or_panicked_binding_is_terminal_and_wakes_readers() {
    for panic_in_binder in [false, true] {
        let owner =
            FileAstOwner::parsed(AstStoreBuilder::new("").unwrap(), Arc::default()).unwrap();
        let BindingAttempt::Started(binding) = owner.try_begin_binding().unwrap() else {
            panic!("file is unbound");
        };
        std::thread::scope(|scope| {
            let (started_tx, started_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let owner = &owner;
            let waiter = scope.spawn(move || {
                started_tx.send(()).unwrap();
                done_tx.send(owner.wait_bound().map(|_| ())).unwrap();
            });
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if panic_in_binder {
                let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let _binding = binding;
                    panic!("simulated binder panic");
                }));
                assert!(panicked.is_err());
            } else {
                drop(binding);
            }
            assert_eq!(
                done_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                Err(AstAccessError::Failed)
            );
            waiter.join().unwrap();
        });
        assert_eq!(owner.phase(), Ok(FileAstPhase::Failed));
        assert!(matches!(owner.try_begin_binding(), Err(AstAccessError::Failed)));
    }
}

#[test]
fn binding_waiters_release_all_run_slots_for_the_publishing_job() {
    if !crate::isolated::in_child(
        "storage_owners::binding_waiters_release_all_run_slots_for_the_publishing_job",
    ) {
        return;
    }
    let owner = FileAstOwner::parsed(AstStoreBuilder::new("").unwrap(), Arc::default()).unwrap();
    let BindingAttempt::Started(binding) = owner.try_begin_binding().unwrap() else {
        panic!("file is unbound");
    };
    let count = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    std::thread::scope(|scope| {
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut waiters = Vec::new();
        for _ in 0..count {
            let owner = &owner;
            let ready_tx = ready_tx.clone();
            waiters.push(scope.spawn(move || {
                tsrs_core::runslots::run(|| {
                    ready_tx.send(()).unwrap();
                    owner.wait_bound().map(|_| ())
                })
            }));
        }
        for _ in 0..count {
            ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let (done_tx, done_rx) = mpsc::channel();
        let publisher = scope.spawn(move || {
            tsrs_core::runslots::run(|| {
                done_tx.send(binding.publish().map(|_| ())).unwrap();
            });
        });
        let published = done_rx.recv_timeout(Duration::from_secs(5));
        if published.is_err() {
            eprintln!("binding publisher starved while waiters held run slots");
            std::process::exit(1);
        }
        publisher.join().unwrap();
        for waiter in waiters {
            let result = waiter.join().unwrap();
            if published.is_ok() {
                assert_eq!(result, Ok(()));
            }
        }
        assert_eq!(published.unwrap(), Ok(()));
    });
}
