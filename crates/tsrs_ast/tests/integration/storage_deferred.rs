use std::sync::{Arc, mpsc};
use std::time::Duration;
use tsrs_ast::{Kind, NodeFlags};
use tsrs_core::TextRange;

use tsrs_ast::storage::*;
use tsrs_ast::storage::{AstRead, BindSession, BindingAttempt, SymbolKey, TextInput};
use tsrs_ast::{FlowFlags, SymbolFlags};

fn draft() -> (Arc<FileAstOwner>, [LazyListSpec; 2]) {
    let mut ast =
        AstStoreBuilder::new("interface I { x: number } interface J { y: number }").unwrap();
    let mut specs = Vec::new();
    for (name, range) in [("I", TextRange::new(13, 23)), ("J", TextRange::new(39, 49))] {
        let members = ast.new_node_list(range, &[]).unwrap();
        let name = ast.new_identifier(NodeFlags::None, range, TextInput::Owned(name)).unwrap();
        let owner = ast
            .new_interface_declaration(NodeFlags::Ambient, range, name, None, None, members, None)
            .unwrap();
        ast.set_parent(name, Some(owner)).unwrap();
        specs.push(LazyListSpec {
            list: members,
            owner,
            range,
            kind: MemberListKind::Type,
            context_flags: NodeFlags::Ambient,
            parsing_contexts: 0,
        });
    }
    (FileAstOwner::parsed(ast, Arc::default()).unwrap(), specs.try_into().unwrap())
}

fn configure(
    binding: &mut BindSession<'_>,
    specs: [LazyListSpec; 2],
    defer: [bool; 2],
) -> [SymbolKey; 2] {
    let (ast, data) = binding.parts_mut();
    let flow = data.new_flow(FlowFlags::Unreachable, None, None).unwrap();
    std::array::from_fn(|index| {
        let spec = specs[index];
        data.register_lazy_list(ast, spec).unwrap();
        let symbol = data.new_symbol(SymbolFlags::Interface, TextInput::Owned("owner")).unwrap();
        let members = data.new_symbol_table().unwrap();
        data.set_symbol_members(symbol, Some(members)).unwrap();
        data.set_symbol_declarations(symbol, &[spec.owner]).unwrap();
        data.set_interface_declaration_symbol(&ast.node(spec.owner).unwrap(), Some(symbol))
            .unwrap();
        if defer[index] {
            data.defer_lazy_list(
                spec.list,
                LazyBindingContext {
                    container: Some(spec.owner),
                    this_container: Some(spec.owner),
                    block_scope_container: Some(spec.owner),
                    last_container: Some(spec.owner),
                    current_flow: Some(flow),
                    unreachable_flow: flow,
                },
            )
            .unwrap();
        }
        symbol
    })
}

fn bound(defer: bool) -> (Arc<FileAstOwner>, Hooks) {
    let (owner, specs) = draft();
    let BindingAttempt::Started(mut binding) = owner.try_begin_binding().unwrap() else {
        panic!("parsed file");
    };
    let symbols = configure(&mut binding, specs, [defer; 2]);
    binding.publish().unwrap();
    (
        owner,
        Hooks { specs, symbols, parses: [0; 2], binds: [0; 2], recurse: false, phases: Vec::new() },
    )
}

struct Hooks {
    specs: [LazyListSpec; 2],
    symbols: [SymbolKey; 2],
    parses: [usize; 2],
    binds: [usize; 2],
    recurse: bool,
    phases: Vec<LazyBindPhase>,
}

impl Hooks {
    fn index(&self, list: ListKey) -> usize {
        self.specs.iter().position(|spec| spec.list == list).unwrap()
    }
}

impl LazyListHooks for Hooks {
    fn parse(
        &mut self,
        ast: &mut AstStoreBuilder,
        spec: LazyListSpec,
    ) -> Result<ListKey, AstAccessError> {
        let index = self.index(spec.list);
        self.parses[index] += 1;
        let name = ast.new_identifier(
            NodeFlags::Ambient,
            spec.range,
            TextInput::Owned(if index == 0 { "x" } else { "y" }),
        )?;
        let node = ast.new_property_signature_declaration(
            NodeFlags::Ambient,
            spec.range,
            None,
            None,
            name,
            None,
            None,
        )?;
        ast.set_parent(name, Some(node))?;
        ast.set_parent(node, Some(spec.owner))?;
        Ok(ast.new_node_list(spec.range, &[node])?)
    }

    fn bind(
        &mut self,
        session: &mut LazyBindSession<'_>,
        list: ListKey,
        context: LazyBindingContext,
    ) -> Result<(), AstAccessError> {
        let index = self.index(list);
        self.binds[index] += 1;
        self.phases.push(session.phase());
        let member = session.list(list)?.get(0).unwrap();
        let name = session.node(member)?.as_property_signature_declaration()?.name();
        let text = if index == 0 { "x" } else { "y" };
        let table = session.bindings().symbol(self.symbols[index])?.members().unwrap();
        let symbol =
            session.bindings_mut().new_symbol(SymbolFlags::Property, TextInput::Owned(text))?;
        session.bindings_mut().set_symbol_table_entry(table, text, symbol)?;
        session.bindings_mut().append_symbol_declarations(symbol, &[member])?;
        session.with_node_bindings(member, |node, data| {
            data.set_property_signature_declaration_symbol(node, Some(symbol))?;
            Ok(())
        })?;
        session.with_node_bindings(name, |node, data| {
            data.set_identifier_flow_node(node, context.current_flow)?;
            Ok(())
        })?;
        assert!(matches!(
            session.parts_mut(self.specs[index].owner.store()),
            Err(AstAccessError::NotMutableStore(_))
        ));
        if self.recurse {
            // Models a parameter property's lookup of its own owner table and
            // another deferred list which recursively reads this first list.
            session.force_list(list, self)?;
            assert_eq!(session.list(list)?.get(0), Some(member));
            session.force_list(self.specs[1 - index].list, self)?;
        }
        Ok(())
    }
}

#[test]
fn recursive_forcing_publishes_both_fragments_once_and_preserves_list_identity() {
    let (owner, mut hooks) = bound(true);
    let mut first = AstRead::new(Arc::default());
    let mut second = AstRead::new(Arc::default());
    first.include(Arc::clone(&owner));
    second.include(Arc::clone(&owner));
    hooks.recurse = true;
    let mut access = owner.binding_context().unwrap();
    let list = access.force_list(hooks.specs[0].list, &mut hooks).unwrap();
    let again = access.force_list(hooks.specs[0].list, &mut hooks).unwrap();
    assert_eq!(list.key(), hooks.specs[0].list);
    assert_eq!(list.range(), hooks.specs[0].range);
    assert_eq!(list.nodes().collect::<Vec<_>>(), again.nodes().collect::<Vec<_>>());
    assert_eq!(hooks.parses, [1, 1]);
    assert_eq!(hooks.binds, [1, 1]);
    assert_eq!(hooks.phases, [LazyBindPhase::Late, LazyBindPhase::Late]);
    let node = list.nodes().next().unwrap();
    for reads in [&first, &second] {
        assert_eq!(
            reads.lease(node.store()).unwrap().node(node).unwrap().parent(),
            Some(hooks.specs[0].owner)
        );
    }
    let read = access.read().unwrap();
    for (spec, symbol) in hooks.specs.iter().zip(hooks.symbols) {
        assert_eq!(read.data().lazy_list_state(spec.list).unwrap(), LazyListState::Ready);
        assert_eq!(
            read.data()
                .symbol_table(read.data().symbol(symbol).unwrap().members().unwrap())
                .unwrap()
                .len(),
            1
        );
    }
}

struct RetainingHooks {
    inner: Hooks,
    retained_node: Option<NodeKey>,
}

impl LazyListHooks for RetainingHooks {
    fn parse(
        &mut self,
        ast: &mut AstStoreBuilder,
        spec: LazyListSpec,
    ) -> Result<ListKey, AstAccessError> {
        self.inner.parse(ast, spec)
    }

    fn bind(
        &mut self,
        session: &mut LazyBindSession<'_>,
        list: ListKey,
        context: LazyBindingContext,
    ) -> Result<(), AstAccessError> {
        self.inner.bind(session, list, context)?;
        let member = session.list(list)?.get(0).unwrap();
        let mut ast = session.bindings().fragment_builder()?;
        let synthetic = ast.new_identifier(
            NodeFlags::None,
            TextRange::synthetic(),
            TextInput::Owned("synthetic"),
        )?;
        ast.set_parent(synthetic, Some(member))?;
        session.bindings_mut().retain_fragment(ast.finish())?;
        let symbol = session
            .bindings_mut()
            .new_symbol(SymbolFlags::Property, TextInput::Owned("synthetic"))?;
        session.bindings_mut().set_symbol_declarations(symbol, &[synthetic])?;
        self.retained_node = Some(synthetic);
        Ok(())
    }
}

#[test]
fn binder_retained_syntax_is_validated_and_registered_with_parsed_fragments() {
    let (owner, hooks) = bound(true);
    let mut reads = AstRead::new(Arc::default());
    reads.include(Arc::clone(&owner));
    let list = hooks.specs[0].list;
    let mut hooks = RetainingHooks { inner: hooks, retained_node: None };
    let parsed = owner.binding_context().unwrap().force_list(list, &mut hooks).unwrap();
    let synthetic = hooks.retained_node.unwrap();
    let syntax = reads.lease(synthetic.store()).unwrap();
    assert_eq!(syntax.node(synthetic).unwrap().parent(), parsed.nodes().next());
}

#[test]
fn unreached_lists_parse_without_binding_and_results_retain_the_file_data() {
    let (owner, mut hooks) = bound(false);
    let weak_owner = Arc::downgrade(&owner);
    let mut access = owner.binding_context().unwrap();
    assert_eq!(
        access.read().unwrap().data().lazy_list_state(hooks.specs[0].list).unwrap(),
        LazyListState::ParseOnly
    );
    let list = access.force_list(hooks.specs[0].list, &mut hooks).unwrap();
    assert_eq!(hooks.parses, [1, 0]);
    assert_eq!(hooks.binds, [0, 0]);
    let base = list.bindings().root_ast();
    let weak_base = base.downgrade();
    let weak_fragment = list.ast().downgrade();
    drop(base);
    drop(access);
    drop(owner);
    assert!(weak_owner.upgrade().is_none());
    assert!(weak_base.upgrade().is_some());
    assert!(weak_fragment.upgrade().is_some());
    assert_eq!(
        list.ast().node(list.nodes().next().unwrap()).unwrap().kind(),
        Kind::PropertySignature
    );
    drop(list);
    assert!(weak_base.upgrade().is_none());
    assert!(weak_fragment.upgrade().is_none());
}

#[test]
fn initial_forcing_preserves_pending_lists_and_uses_the_current_binding_phase() {
    let (owner, specs) = draft();
    let BindingAttempt::Started(mut binding) = owner.try_begin_binding().unwrap() else {
        panic!("parsed file");
    };
    let symbols = configure(&mut binding, specs, [true, false]);
    let mut hooks =
        Hooks { specs, symbols, parses: [0; 2], binds: [0; 2], recurse: false, phases: Vec::new() };
    // A list that has not been visited by the binder is still Pending.
    binding
        .with_lazy_lists(|session| {
            session.force_list(specs[1].list, &mut hooks)?;
            assert!(session.list(specs[1].list)?.is_empty());
            session.force_list(specs[0].list, &mut hooks)
        })
        .unwrap();
    assert_eq!(binding.bindings().lazy_list_state(specs[1].list).unwrap(), LazyListState::Pending);
    assert_eq!(hooks.parses, [1, 0]);
    assert_eq!(hooks.phases, [LazyBindPhase::Initial]);
    binding.with_lazy_lists(|session| session.parse_list_now(specs[1].list, &mut hooks)).unwrap();
    let parsed = binding.publish().unwrap();
    assert_eq!(parsed.id(), owner.id());
    assert_eq!(hooks.parses, [1, 1]);
    assert_eq!(hooks.binds, [1, 0]);
    let mut reads = AstRead::new(Arc::default());
    reads.include(Arc::clone(&owner));
    let mut access = owner.binding_context().unwrap();
    let list = access.force_list(specs[0].list, &mut hooks).unwrap();
    let node = list.nodes().next().unwrap();
    assert_eq!(reads.lease(node.store()).unwrap().node(node).unwrap().key(), node);
}

struct BlockingHooks {
    inner: Hooks,
    entered: mpsc::Sender<()>,
    resume: mpsc::Receiver<()>,
    outcome: BindOutcome,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BindOutcome {
    Complete,
    Fail,
    Panic,
}

impl LazyListHooks for BlockingHooks {
    fn parse(
        &mut self,
        ast: &mut AstStoreBuilder,
        spec: LazyListSpec,
    ) -> Result<ListKey, AstAccessError> {
        self.inner.parse(ast, spec)
    }
    fn bind(
        &mut self,
        session: &mut LazyBindSession<'_>,
        list: ListKey,
        context: LazyBindingContext,
    ) -> Result<(), AstAccessError> {
        self.inner.bind(session, list, context)?;
        self.entered.send(()).unwrap();
        tsrs_core::runslots::blocking(|| self.resume.recv_timeout(Duration::from_secs(10)))
            .unwrap();
        match self.outcome {
            BindOutcome::Complete => Ok(()),
            BindOutcome::Fail => Err(AstAccessError::Failed),
            BindOutcome::Panic => panic!("binding canceled after annotation writes"),
        }
    }
}

#[test]
fn concurrent_lists_and_cached_symbol_readers_wait_for_complete_binding_or_failure() {
    for outcome in [BindOutcome::Complete, BindOutcome::Fail, BindOutcome::Panic] {
        let (owner, hooks) = bound(true);
        let specs = hooks.specs;
        let symbols = hooks.symbols;
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let mut blocker =
            BlockingHooks { inner: hooks, entered: entered_tx, resume: resume_rx, outcome };
        std::thread::scope(|scope| {
            let mut first = owner.binding_context().unwrap();
            let mut second = owner.binding_context().unwrap();
            let cached = owner.bindings().unwrap();
            let (read_tx, read_rx) = mpsc::channel();
            let (second_tx, second_rx) = mpsc::channel();
            let worker = scope.spawn(move || first.force_list(specs[0].list, &mut blocker));
            entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let reader = scope.spawn(move || {
                let result = cached.read().map(|read| {
                    let table = read.data().symbol(symbols[0]).unwrap().members().unwrap();
                    read.data().symbol_table(table).unwrap().len()
                });
                read_tx.send(result).unwrap();
            });
            let other = scope.spawn(move || {
                let mut hooks = Hooks {
                    specs,
                    symbols,
                    parses: [0; 2],
                    binds: [0; 2],
                    recurse: false,
                    phases: Vec::new(),
                };
                second_tx
                    .send(
                        second.force_list(specs[1].list, &mut hooks).map(|list| list.nodes().len()),
                    )
                    .unwrap();
            });
            assert!(read_rx.try_recv().is_err());
            assert!(second_rx.try_recv().is_err());
            resume_tx.send(()).unwrap();
            let first = worker.join();
            reader.join().unwrap();
            other.join().unwrap();
            if outcome == BindOutcome::Complete {
                assert_eq!(first.unwrap().unwrap().nodes().len(), 1);
                assert_eq!(read_rx.recv().unwrap(), Ok(1));
                assert_eq!(second_rx.recv().unwrap(), Ok(1));
            } else {
                if outcome == BindOutcome::Panic {
                    assert!(first.is_err());
                } else {
                    assert!(matches!(first.unwrap(), Err(AstAccessError::Failed)));
                }
                assert_eq!(read_rx.recv().unwrap(), Err(AstAccessError::Failed));
                assert_eq!(second_rx.recv().unwrap(), Err(AstAccessError::Failed));
                assert_eq!(owner.phase(), Ok(FileAstPhase::Failed));
            }
        });
    }
}

struct CaughtPanicHooks {
    inner: Hooks,
    incomplete_node: Option<NodeKey>,
}

impl LazyListHooks for CaughtPanicHooks {
    fn parse(
        &mut self,
        ast: &mut AstStoreBuilder,
        spec: LazyListSpec,
    ) -> Result<ListKey, AstAccessError> {
        self.inner.parse(ast, spec)
    }

    fn bind(
        &mut self,
        session: &mut LazyBindSession<'_>,
        list: ListKey,
        context: LazyBindingContext,
    ) -> Result<(), AstAccessError> {
        self.inner.bind(session, list, context)?;
        if list == self.inner.specs[0].list {
            let other = self.inner.specs[1].list;
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                session.force_list(other, self)
            }));
            assert!(panic.is_err());
            Ok(())
        } else {
            self.incomplete_node = session.list(list)?.get(0);
            panic!("nested binder panicked after writing symbols");
        }
    }
}

#[test]
fn caught_nested_panics_cannot_publish_initial_or_late_bindings() {
    for initial in [false, true] {
        let (owner, specs) = draft();
        let BindingAttempt::Started(mut binding) = owner.try_begin_binding().unwrap() else {
            panic!("parsed file");
        };
        let symbols = configure(&mut binding, specs, [true; 2]);
        let mut hooks = CaughtPanicHooks {
            inner: Hooks {
                specs,
                symbols,
                parses: [0; 2],
                binds: [0; 2],
                recurse: false,
                phases: Vec::new(),
            },
            incomplete_node: None,
        };
        if initial {
            assert_eq!(
                binding.with_lazy_lists(|session| session.force_list(specs[0].list, &mut hooks)),
                Err(AstAccessError::Failed)
            );
            assert!(matches!(binding.publish(), Err(AstAccessError::Failed)));
        } else {
            binding.publish().unwrap();
            let cached = owner.bindings().unwrap();
            assert!(matches!(
                owner.binding_context().unwrap().force_list(specs[0].list, &mut hooks),
                Err(AstAccessError::Failed)
            ));
            assert!(matches!(cached.read(), Err(AstAccessError::Failed)));
        }
        assert_eq!(owner.phase(), Ok(FileAstPhase::Failed));
        let mut reads = AstRead::new(Arc::default());
        reads.include(owner);
        assert!(matches!(
            reads.lease(hooks.incomplete_node.unwrap().store()),
            Err(AstAccessError::UnknownStore(_))
        ));
    }
}

#[test]
fn invalid_caller_keys_leave_healthy_binding_and_ready_results_usable() {
    let (owner, mut hooks) = bound(true);
    let (other, other_hooks) = bound(true);
    let mut access = owner.binding_context().unwrap();
    assert!(matches!(
        access.force_list(other_hooks.specs[0].list, &mut hooks),
        Err(AstAccessError::UnknownLazyList(_))
    ));
    assert_eq!(owner.phase(), Ok(FileAstPhase::Bound));
    let list = access.force_list(hooks.specs[0].list, &mut hooks).unwrap();
    owner.retire().unwrap();
    assert!(matches!(
        access.force_list(hooks.specs[0].list, &mut hooks),
        Err(AstAccessError::Retired)
    ));
    assert_eq!(list.nodes().len(), 1);
    assert_eq!(other.phase(), Ok(FileAstPhase::Bound));
}

#[test]
fn saved_contexts_reject_other_file_nodes_and_same_slot_flows() {
    let (owner, specs) = draft();
    let (other, other_specs) = draft();
    let BindingAttempt::Started(mut binding) = owner.try_begin_binding().unwrap() else {
        panic!("parsed file");
    };
    let BindingAttempt::Started(mut other_binding) = other.try_begin_binding().unwrap() else {
        panic!("parsed file");
    };
    configure(&mut binding, specs, [false; 2]);
    configure(&mut other_binding, other_specs, [false; 2]);
    let foreign_flow =
        other_binding.parts_mut().1.new_flow(FlowFlags::Unreachable, None, None).unwrap();
    let data = binding.parts_mut().1;
    let flow = data.new_flow(FlowFlags::Unreachable, None, None).unwrap();
    assert_eq!(flow.index(), foreign_flow.index());
    let context = LazyBindingContext {
        container: Some(specs[0].owner),
        this_container: None,
        block_scope_container: None,
        last_container: None,
        current_flow: None,
        unreachable_flow: foreign_flow,
    };
    assert!(matches!(
        data.defer_lazy_list(specs[0].list, context),
        Err(AstAccessError::Storage(_))
    ));
    assert_eq!(data.lazy_list_state(specs[0].list), Ok(LazyListState::Pending));
    assert_eq!(specs[0].owner.index(), other_specs[0].owner.index());
    data.defer_lazy_list(
        specs[0].list,
        LazyBindingContext {
            container: Some(other_specs[0].owner),
            unreachable_flow: flow,
            ..context
        },
    )
    .unwrap();
    assert!(matches!(binding.publish(), Err(AstAccessError::UnpermittedOwner(_))));
    assert_eq!(owner.phase(), Ok(FileAstPhase::Failed));
    other_binding.publish().unwrap();
}

#[test]
fn initial_binding_cannot_publish_after_a_caught_callback_panic() {
    let (owner, specs) = draft();
    let BindingAttempt::Started(mut binding) = owner.try_begin_binding().unwrap() else {
        panic!("parsed file");
    };
    configure(&mut binding, specs, [true; 2]);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        binding.with_lazy_lists::<()>(|session| {
            session
                .bindings_mut()
                .new_symbol(SymbolFlags::Property, TextInput::Owned("partial"))?;
            panic!("caller catches this outside the session");
        })
    }));
    assert!(panic.is_err());
    assert!(matches!(binding.publish(), Err(AstAccessError::Failed)));
    assert_eq!(owner.phase(), Ok(FileAstPhase::Failed));
}

#[test]
fn deferred_binding_waiters_release_run_slots_for_the_winning_binder() {
    if !crate::isolated::in_child(
        "storage_deferred::deferred_binding_waiters_release_run_slots_for_the_winning_binder",
    ) {
        return;
    }
    let (owner, hooks) = bound(true);
    let list = hooks.specs[0].list;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let mut hooks = BlockingHooks {
        inner: hooks,
        entered: entered_tx,
        resume: resume_rx,
        outcome: BindOutcome::Complete,
    };
    let count = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    std::thread::scope(|scope| {
        let mut context = owner.binding_context().unwrap();
        let winner =
            scope.spawn(move || tsrs_core::runslots::run(|| context.force_list(list, &mut hooks)));
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut readers = Vec::new();
        for _ in 0..count {
            let bindings = owner.bindings().unwrap();
            let ready_tx = ready_tx.clone();
            readers.push(scope.spawn(move || {
                tsrs_core::runslots::run(|| {
                    ready_tx.send(()).unwrap();
                    bindings.read().map(|read| read.data().lazy_list_state(list).unwrap())
                })
            }));
        }
        for _ in 0..count {
            ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        let (done_tx, done_rx) = mpsc::channel();
        let resume = &resume_tx;
        let resumer = scope.spawn(move || {
            tsrs_core::runslots::run(|| {
                resume.send(()).unwrap();
                done_tx.send(()).unwrap();
            });
        });
        let progressed = done_rx.recv_timeout(Duration::from_secs(10));
        if progressed.is_err() {
            // Release the gate before reporting starvation, so no thread leaks.
            resume_tx.send(()).unwrap();
        }
        resumer.join().unwrap();
        winner.join().unwrap().unwrap();
        for reader in readers {
            assert_eq!(reader.join().unwrap(), Ok(LazyListState::Ready));
        }
        progressed.unwrap();
    });
}

#[test]
fn retirement_waits_for_fragment_installation_and_preserves_existing_leases() {
    let (owner, hooks) = bound(true);
    let list = hooks.specs[0].list;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let mut hooks = BlockingHooks {
        inner: hooks,
        entered: entered_tx,
        resume: resume_rx,
        outcome: BindOutcome::Complete,
    };
    std::thread::scope(|scope| {
        let mut context = owner.binding_context().unwrap();
        let forcing = scope.spawn(move || context.force_list(list, &mut hooks));
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (retired_tx, retired_rx) = mpsc::channel();
        let file = &owner;
        let retiring = scope.spawn(move || {
            let result = file.retire();
            retired_tx.send(result).unwrap();
        });
        assert!(retired_rx.try_recv().is_err());
        resume_tx.send(()).unwrap();
        let retained = forcing.join().unwrap().unwrap();
        retiring.join().unwrap();
        assert_eq!(retired_rx.recv().unwrap(), Ok(()));
        assert_eq!(owner.phase(), Ok(FileAstPhase::Retired));
        assert_eq!(retained.nodes().len(), 1);
        assert_eq!(
            retained.bindings().read().unwrap().data().lazy_list_state(list),
            Ok(LazyListState::Ready)
        );
    });
}
