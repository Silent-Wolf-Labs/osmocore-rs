//! Rust checks on FSM definitions, instance names and optional hooks. C doesn't
//! record these, either because it asserts or because Rust strings differ.

use osmocore_rs::fsm::EventData;
use osmocore_rs::fsm::{Fsm, FsmError, FsmRuntime, FsmState, TermCause};
use osmocore_rs::time::SystemClock;
use osmocore_rs::timer::TimerScheduler;

type Runtime = FsmRuntime<SystemClock, ()>;
type Scheduler = TimerScheduler<SystemClock, Runtime>;

fn fsm(name: &str, states: usize) -> Fsm<SystemClock, (), ()> {
    Fsm {
        name: name.into(),
        states: (0..states)
            .map(|_| FsmState {
                name: None,
                in_event_mask: 0,
                out_state_mask: 0,
                action: None,
                onenter: None,
                onleave: None,
            })
            .collect(),
        allstate_event_mask: 0,
        allstate_action: None,
        pre_term: None,
        cleanup: None,
        timer_cb: None,
        event_names: None,
    }
}

fn setup() -> (Scheduler, Runtime) {
    (TimerScheduler::new(SystemClock), Runtime::new(()))
}

#[test]
fn state_tables_must_hold_one_to_32_states() {
    let (_, mut runtime) = setup();
    for states in [0, 33] {
        assert_eq!(
            runtime.register(fsm("rejected", states)).err(),
            Some(FsmError::InvalidDescriptor),
            "{states} states"
        );
        assert_eq!(runtime.find_fsm_by_name("rejected"), None);
    }
    for (name, states) in [("one", 1), ("most", 32)] {
        let id = runtime.register(fsm(name, states)).unwrap();
        assert_eq!(runtime.find_fsm_by_name(name), Some(id));
    }
}

#[test]
fn unregistering_twice_is_an_error_and_the_name_becomes_free() {
    let (_, mut runtime) = setup();
    let first = runtime.register(fsm("machine", 1)).unwrap();
    runtime.unregister(&first).unwrap();
    assert_eq!(runtime.find_fsm_by_name("machine"), None);
    assert_eq!(runtime.unregister(&first), Err(FsmError::NotRegistered));
    let second = runtime.register(fsm("machine", 1)).unwrap();
    assert_eq!(runtime.find_fsm_by_name("machine"), Some(second));
}

#[test]
fn invalid_identifier_allocates_nothing() {
    let (mut scheduler, mut runtime) = setup();
    let definition = runtime.register(fsm("machine", 1)).unwrap();
    for id in ["", "has.dot", "has space", "non\u{e9}ascii"] {
        assert_eq!(
            runtime
                .alloc(&mut scheduler, &definition, Some(id), ())
                .err(),
            Some(FsmError::InvalidIdentifier),
            "{id:?}"
        );
    }
    assert_eq!(runtime.instances(&definition).unwrap().len(), 0);
    assert_eq!(scheduler.check(), 0);
    runtime
        .alloc(&mut scheduler, &definition, Some("valid-id_1"), ())
        .unwrap();
    assert_eq!(runtime.instances(&definition).unwrap().len(), 1);
}

#[test]
fn sanitizing_replaces_illegal_bytes_and_keeps_the_old_name_on_failure() {
    let (mut scheduler, mut runtime) = setup();
    let definition = runtime.register(fsm("machine", 1)).unwrap();
    let id = runtime
        .alloc(&mut scheduler, &definition, Some("old"), ())
        .unwrap();
    let name = |runtime: &Runtime| runtime.instance(&id).unwrap().name.clone();

    runtime
        .update_id_sanitize(&id, b'_', Some("a.b c"))
        .unwrap();
    assert_eq!(name(&runtime), "machine(a_b_c)");

    // Only ASCII identifiers are valid, so these fail after sanitizing.
    for (replacement, text) in [(b'_', "a\0b"), (0xff, "a.b"), (b'.', "a.b")] {
        assert_eq!(
            runtime.update_id_sanitize(&id, replacement, Some(text)),
            Err(FsmError::InvalidIdentifier),
            "{replacement:#x} {text:?}"
        );
        assert_eq!(name(&runtime), "machine(a_b_c)");
    }

    // A NUL replacement ends the name, as it does for C's destination string.
    runtime
        .update_id_sanitize(&id, 0, Some("left.right"))
        .unwrap();
    assert_eq!(name(&runtime), "machine(left)");

    runtime.update_id_sanitize(&id, b'_', None).unwrap();
    assert_eq!(name(&runtime), "machine");
    assert_eq!(runtime.instance(&id).unwrap().id, None);
}

#[test]
fn event_names_stop_at_the_terminator() {
    let (_, mut runtime) = setup();
    let mut named = fsm("named", 1);
    named.event_names = Some(vec![
        (1, Some("ONE".into())),
        (2, Some("TWO".into())),
        (3, None),
        (4, Some("AFTER_END".into())),
    ]);
    let definition = runtime.register(named).unwrap();
    assert_eq!(runtime.event_name(&definition, 1).unwrap(), "ONE");
    assert_eq!(runtime.event_name(&definition, 2).unwrap(), "TWO");
    assert_eq!(runtime.event_name(&definition, 3).unwrap(), "unknown 0x3");
    assert_eq!(runtime.event_name(&definition, 4).unwrap(), "unknown 0x4");

    let plain = runtime.register(fsm("plain", 1)).unwrap();
    assert_eq!(runtime.event_name(&plain, 7).unwrap(), "7");
}

#[test]
fn termination_works_without_hooks_and_children_keep_their_parent_event() {
    let (mut scheduler, mut runtime) = setup();
    let definition = runtime.register(fsm("machine", 1)).unwrap();
    let parent = runtime
        .alloc(&mut scheduler, &definition, None, ())
        .unwrap();
    let child = runtime
        .alloc_child(&mut scheduler, &definition, &parent, 42, ())
        .unwrap();
    assert_eq!(runtime.instance(&child).unwrap().parent_term_event(), 42);
    assert_eq!(runtime.instance(&child).unwrap().parent(), Some(&parent));

    runtime
        .term(
            &mut scheduler,
            &parent,
            TermCause::Regular,
            &EventData::None,
        )
        .unwrap();
    assert!(runtime.instance(&parent).is_err());
    assert!(runtime.instance(&child).is_err());
    assert_eq!(runtime.instances(&definition).unwrap().len(), 0);
}
