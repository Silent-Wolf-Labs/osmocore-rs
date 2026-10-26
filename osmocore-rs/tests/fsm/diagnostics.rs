// SPDX-License-Identifier: GPL-3.0-or-later

#[path = "../support/diagnostics_capture.rs"]
mod capture;
use capture::{Capture, Record, Value};
use osmocore_rs::{
    fsm::{EventData, Fsm, FsmError, FsmId, FsmRuntime, FsmState, TermCause},
    time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::TimerScheduler,
};

// Neither the context, event payload, nor private data implements Debug.
struct Payload;
#[derive(Default)]
struct Data {
    calls: Vec<&'static str>,
    nested: Option<FsmId>,
    reenter_children: bool,
    nested_error: Option<FsmError>,
    panic_cleanup: bool,
}
struct TestClock {
    now: i64,
    fail: bool,
    reads: usize,
}
impl Clock for TestClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        self.reads += 1;
        if self.fail {
            Err(TimeError::ArithmeticOverflow)
        } else {
            Ok(TimeVal::new(self.now, 0))
        }
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        panic!("no monotonic reads expected")
    }
}
type Runtime = FsmRuntime<TestClock, Data, Payload>;
type Scheduler = TimerScheduler<TestClock, Runtime>;
fn action(
    s: &mut Scheduler,
    r: &mut Runtime,
    id: FsmId,
    event: u32,
    data: &EventData<'_, Payload>,
) {
    r.data.calls.push("action");
    if event == 0 {
        if let Some(child) = r.data.nested.clone() {
            r.dispatch(s, Some(&child), 1, data).unwrap();
        }
        r.state_change(s, &id, 1, 0, 0).unwrap();
    }
}
fn leave(_: &mut Scheduler, r: &mut Runtime, _: FsmId, _: u32) {
    r.data.calls.push("leave");
}
fn enter(_: &mut Scheduler, r: &mut Runtime, _: FsmId, _: u32) {
    r.data.calls.push("enter");
}
fn pre_term(s: &mut Scheduler, r: &mut Runtime, id: FsmId, _: TermCause) {
    r.data.calls.push("pre_term");
    if r.data.reenter_children {
        let parent = r.instance(&id).unwrap().parent().cloned();
        if let Some(parent) = parent {
            r.data.nested_error = r
                .term_children(s, &parent, TermCause::Parent, &EventData::None)
                .err();
        }
    }
}
fn cleanup(_: &mut Scheduler, r: &mut Runtime, _: FsmId, _: TermCause) {
    r.data.calls.push("cleanup");
    assert!(!r.data.panic_cleanup, "cleanup panic");
}
fn setup() -> (Scheduler, Runtime, FsmId) {
    let mut s = Scheduler::new(TestClock {
        now: 100,
        fail: false,
        reads: 0,
    });
    let mut r = Runtime::new(Data::default());
    let fsm = r
        .register(Fsm {
            name: "diagnostic_fsm".into(),
            states: (0..2)
                .map(|n| FsmState {
                    name: None,
                    in_event_mask: 3,
                    out_state_mask: 3,
                    action: Some(action),
                    onleave: Some(leave),
                    onenter: if n == 1 { Some(enter) } else { None },
                })
                .collect(),
            allstate_event_mask: 0,
            allstate_action: None,
            pre_term: Some(pre_term),
            cleanup: Some(cleanup),
            timer_cb: None,
            event_names: None,
        })
        .unwrap();
    let id = r
        .alloc(&mut s, &fsm, Some("private_application_name"), Payload)
        .unwrap();
    (s, r, id)
}
fn operation<'a>(events: &'a [Record], name: &str) -> &'a Record {
    events
        .iter()
        .find(|e| e.fields.text("operation") == Some(name))
        .unwrap()
}

#[test]
fn structured_dispatch_transition_nested_context_and_payload_privacy() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let outer = tracing::info_span!("application", request = 42u64);
        let _guard = outer.enter();
        let (mut s, mut r, id) = setup();
        let definition = r.instance(&id).unwrap().definition().clone();
        let nested = r.alloc(&mut s, &definition, None, Payload).unwrap();
        r.data.nested = Some(nested);
        r.dispatch(&mut s, Some(&id), 0, &EventData::User(&Payload))
            .unwrap();
        assert_eq!(r.data.calls, ["action", "action", "leave", "enter"]);
        assert_eq!(s.clock.reads, 0);
    });
    let events = capture.events();
    let dispatch = operation(&events, "dispatch");
    assert_eq!(dispatch.target, "osmocore_rs::fsm");
    assert_eq!(dispatch.level, tracing::Level::TRACE);
    assert_eq!(dispatch.fields.text("fsm_name"), Some("diagnostic_fsm"));
    assert_eq!(dispatch.fields.0["event"], Value::Unsigned(0));
    assert_eq!(dispatch.fields.0["current_state"], Value::Unsigned(0));
    assert_eq!(dispatch.scope[0].name, "application");
    assert_eq!(dispatch.scope[0].fields.0["request"], Value::Unsigned(42));
    let nested = events
        .iter()
        .find(|e| {
            e.fields.text("operation") == Some("dispatch")
                && e.fields.0["event"] == Value::Unsigned(1)
        })
        .unwrap();
    assert_eq!(
        nested
            .scope
            .iter()
            .filter(|s| s.name == "fsm_execution")
            .count(),
        2
    );
    let transition = operation(&events, "state_change");
    assert_eq!(transition.level, tracing::Level::DEBUG);
    assert_eq!(transition.fields.0["old_state"], Value::Unsigned(0));
    assert_eq!(transition.fields.0["new_state"], Value::Unsigned(1));
    assert!(
        events
            .iter()
            .all(|e| !e.fields.0.contains_key("payload") && !e.fields.0.contains_key("data"))
    );
    assert!(!format!("{events:?}").contains("private_application_name"));
}

#[test]
fn rejected_operations_have_categories_even_with_execution_spans_filtered() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("off,osmocore_rs::fsm=warn"), || {
        let (mut s, mut r, id) = setup();
        assert_eq!(
            r.dispatch(&mut s, Some(&id), 31, &EventData::None),
            Err(FsmError::EventNotPermitted)
        );
        assert_eq!(
            r.state_change(&mut s, &id, 32, 0, 0),
            Err(FsmError::InvalidState(32))
        );
        assert_eq!(
            r.dispatch(&mut s, None, 0, &EventData::None),
            Err(FsmError::InvalidHandle)
        );
    });
    let events = capture.events();
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.level == tracing::Level::WARN
        && e.target == "osmocore_rs::fsm"
        && e.scope.is_empty()));
    assert_eq!(
        events[0].fields.text("error_category"),
        Some("event_not_permitted")
    );
    assert_eq!(events[0].fields.text("fsm_name"), Some("diagnostic_fsm"));
    assert_eq!(
        events[1].fields.text("error_category"),
        Some("invalid_state")
    );
    assert!(!events[2].fields.0.contains_key("instance_slot"));
}

#[test]
fn committed_state_is_reported_without_success_when_timer_scheduling_fails() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let (mut s, mut r, id) = setup();
        s.clock.fail = true;
        assert!(matches!(
            r.state_change(&mut s, &id, 1, 5, 9),
            Err(FsmError::Timer(_))
        ));
        assert_eq!(r.instance(&id).unwrap().state, 1);
        assert_eq!(r.data.calls, ["leave"]);
        assert_eq!(s.clock.reads, 1);
    });
    let events = capture.events();
    assert_eq!(
        operation(&events, "state_commit").fields.0["new_state"],
        Value::Unsigned(1)
    );
    assert!(
        !events
            .iter()
            .any(|e| e.fields.text("operation") == Some("state_change")
                && e.level == tracing::Level::DEBUG)
    );
    assert!(
        events
            .iter()
            .any(|e| e.fields.text("operation") == Some("state_change")
                && e.fields.text("error_category") == Some("time"))
    );
}

#[test]
fn hierarchical_termination_reports_completed_lifecycle_in_both_modes() {
    for safe in [false, true] {
        let capture = Capture::default();
        tracing::subscriber::with_default(capture.subscriber("trace"), || {
            let (mut s, mut r, parent) = setup();
            r.term_safely(safe);
            let definition = r.instance(&parent).unwrap().definition().clone();
            let child = r
                .alloc_child(&mut s, &definition, &parent, 1, Payload)
                .unwrap();
            r.term(&mut s, &parent, TermCause::Request, &EventData::None)
                .unwrap();
            assert!(!r.observable(&parent));
            assert!(!r.observable(&child));
            assert_eq!(r.data.calls, ["pre_term", "pre_term", "cleanup", "cleanup"]);
        });
        let events = capture.events();
        for op in ["termination_requested", "termination_completed", "retire"] {
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.fields.text("operation") == Some(op))
                    .count(),
                2
            );
        }
        let complete = operation(&events, "termination_completed");
        assert_eq!(complete.fields.text("fsm_name"), Some("diagnostic_fsm"));
        assert!(
            complete
                .scope
                .iter()
                .any(|s| s.fields.text("operation") == Some("term"))
        );
    }
}

#[test]
fn timeout_execution_inherits_scheduling_context_after_dispatch_returns() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let (mut s, mut r, id) = setup();
        {
            let span = tracing::info_span!("application", request = 99u64);
            let _guard = span.enter();
            r.state_change(&mut s, &id, 1, 1, 7).unwrap();
        }
        s.clock.now = 101;
        assert_eq!(r.update(&mut s), Ok(1));
        assert!(!r.observable(&id));
        assert_eq!(s.clock.reads, 2);
        assert_eq!(r.data.calls, ["leave", "enter", "pre_term", "cleanup"]);
    });
    let events = capture.events();
    let term = operation(&events, "termination_requested");
    assert_eq!(
        term.fields.0["cause"],
        Value::Signed(TermCause::Timeout as i64)
    );
    assert!(
        term.scope
            .iter()
            .any(|s| s.name == "application" && s.fields.0["request"] == Value::Unsigned(99))
    );
    assert!(
        term.scope
            .iter()
            .any(|s| s.fields.text("operation") == Some("timeout"))
    );
}

#[test]
fn reentrant_child_stall_is_reported_without_changing_termination_progress() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let (mut s, mut r, parent) = setup();
        let definition = r.instance(&parent).unwrap().definition().clone();
        let child = r
            .alloc_child(&mut s, &definition, &parent, 1, Payload)
            .unwrap();
        r.data.reenter_children = true;
        r.term_children(&mut s, &parent, TermCause::Parent, &EventData::None)
            .unwrap();
        assert_eq!(r.data.nested_error, Some(FsmError::StuckChild));
        assert!(!r.observable(&child));
        assert!(r.instance(&parent).unwrap().children().is_empty());
    });
    let events = capture.events();
    let stall = events
        .iter()
        .find(|e| e.fields.text("error_category") == Some("stuck_child"))
        .unwrap();
    assert_eq!(stall.fields.text("operation"), Some("term_children"));
    assert_eq!(stall.level, tracing::Level::WARN);
}

#[test]
fn termination_panic_is_reported_and_resumes_without_completion() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let (mut s, mut r, id) = setup();
        r.term_safely(true);
        r.data.panic_cleanup = true;
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            r.term(&mut s, &id, TermCause::Request, &EventData::None)
        }))
        .unwrap_err();
        assert_eq!(panic.downcast_ref::<&str>().copied(), Some("cleanup panic"));
        assert!(r.observable(&id));
        assert!(r.instance(&id).unwrap().terminating);
        assert_eq!(r.data.calls, ["pre_term", "cleanup"]);
    });
    let events = capture.events();
    let panic = events
        .iter()
        .find(|e| e.fields.text("error_category") == Some("callback_panic"))
        .unwrap();
    assert_eq!(panic.target, "osmocore_rs::fsm");
    assert_eq!(panic.level, tracing::Level::ERROR);
    assert_eq!(panic.fields.text("fsm_name"), Some("diagnostic_fsm"));
    assert!(
        !events
            .iter()
            .any(|e| e.fields.text("operation") == Some("termination_completed"))
    );
}
