// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "tracing")]

#[path = "../../support/diagnostics_capture.rs"]
mod capture;
use capture::{Capture, Value};
use osmocore_rs::{
    time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::{TimerError, TimerId, TimerScheduler},
};

struct TestClock {
    now: i64,
    reads: usize,
}
impl Clock for TestClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        self.reads += 1;
        Ok(TimeVal::new(self.now, 0))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        panic!("unexpected clock")
    }
}
#[derive(Default)]
struct Data {
    calls: usize,
    panic: bool,
    nested_error: Option<TimerError>,
}
type Scheduler = TimerScheduler<TestClock, Data>;
fn callback(s: &mut Scheduler, id: TimerId, data: &mut Data) {
    assert_eq!(s.pending(&id), Ok(false));
    data.calls += 1;
    tracing::debug!(target: "application", operation = "callback_body", call = data.calls);
    data.nested_error = s.update(data).err();
    if data.calls == 1 {
        let span = tracing::info_span!("rearm_application", request = 2u64);
        let _guard = span.enter();
        s.add(&id, TimeVal::new(102, 0)).unwrap();
    }
    assert!(!data.panic, "original callback panic");
}
fn scheduler() -> Scheduler {
    Scheduler::new(TestClock { now: 100, reads: 0 })
}

#[test]
fn timer_arm_cancel_expire_and_rearm_keep_activation_context() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let mut s = scheduler();
        let mut data = Data::default();
        let id = s.create(Some(callback));
        {
            let span = tracing::info_span!("application", request = 1u64);
            let _guard = span.enter();
            s.schedule(&id, 1, 0).unwrap();
        }
        s.clock.now = 101;
        assert_eq!(s.update(&mut data), Ok(1));
        assert_eq!(data.nested_error, Some(TimerError::NestedUpdate));
        assert_eq!(s.deadline(&id), Ok(TimeVal::new(102, 0)));
        s.clock.now = 102;
        assert_eq!(s.update(&mut data), Ok(1));
        assert_eq!(data.calls, 2);
        assert_eq!(s.clock.reads, 3); // schedule plus the two outer updates
        s.delete(&id).unwrap(); // idempotent cancellation is still successful
        s.release(&id).unwrap();
        assert_eq!(
            s.add(&id, TimeVal::new(103, 0)),
            Err(TimerError::InvalidHandle)
        );
    });
    let events = capture.events();
    let arm = events
        .iter()
        .find(|e| e.fields.text("operation") == Some("arm"))
        .unwrap();
    assert_eq!(arm.target, "osmocore_rs::timer");
    assert_eq!(arm.level, tracing::Level::DEBUG);
    assert_eq!(arm.fields.0["timer_slot"], Value::Unsigned(0));
    assert_eq!(arm.fields.0["timer_generation"], Value::Unsigned(0));
    assert_eq!(arm.fields.0["deadline_seconds"], Value::Signed(101));
    let expiries: Vec<_> = events
        .iter()
        .filter(|e| e.fields.text("operation") == Some("expire"))
        .collect();
    assert_eq!(expiries.len(), 2);
    assert_eq!(expiries[0].fields.0["now_seconds"], Value::Signed(101));
    assert!(
        expiries[0]
            .scope
            .iter()
            .any(|s| s.name == "application" && s.fields.0["request"] == Value::Unsigned(1))
    );
    assert!(
        expiries[1]
            .scope
            .iter()
            .any(|s| s.name == "rearm_application" && s.fields.0["request"] == Value::Unsigned(2))
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.fields.text("operation") == Some("callback_body"))
            .count(),
        2
    );
    let cancel = events
        .iter()
        .find(|e| e.fields.text("operation") == Some("cancel"))
        .unwrap();
    assert_eq!(cancel.fields.0["was_pending"], Value::Bool(false));
    assert!(
        events
            .iter()
            .any(|e| e.fields.text("operation") == Some("release"))
    );
    let stale = events
        .iter()
        .find(|e| e.fields.text("error_category") == Some("invalid_handle"))
        .unwrap();
    assert_eq!(stale.level, tracing::Level::WARN);
    assert!(!stale.fields.0.contains_key("timer_slot"));
}

#[test]
fn filtering_keeps_application_context_when_internal_spans_are_disabled() {
    let capture = Capture::default();
    tracing::subscriber::with_default(
        capture.subscriber("off,application=info,osmocore_rs::timer=debug"),
        || {
            let mut s = scheduler();
            let id = s.create(None);
            {
                let span =
                    tracing::info_span!(target: "application", "application", request = 5u64);
                let _guard = span.enter();
                s.add(&id, TimeVal::new(100, 0)).unwrap();
            }
            s.update(&mut Data::default()).unwrap();
        },
    );
    let events = capture.events();
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|e| e.target == "osmocore_rs::timer" && e.level == tracing::Level::DEBUG)
    );
    let expiry = events
        .iter()
        .find(|e| e.fields.text("operation") == Some("expire"))
        .unwrap();
    assert_eq!(expiry.scope.len(), 1);
    assert_eq!(expiry.scope[0].name, "application");
    assert_eq!(expiry.scope[0].fields.0["request"], Value::Unsigned(5));
}

#[test]
fn cancelled_and_released_activations_do_not_leak_scheduling_context() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let mut s = scheduler();
        let id = s.create(None);
        {
            let span = tracing::info_span!("cancelled_application");
            let _guard = span.enter();
            s.add(&id, TimeVal::new(100, 0)).unwrap();
        }
        s.delete(&id).unwrap();
        s.add(&id, TimeVal::new(100, 0)).unwrap();
        s.update(&mut Data::default()).unwrap();
        s.release(&id).unwrap();
        let reused = s.create(None);
        s.add(&reused, TimeVal::new(100, 0)).unwrap();
        s.update(&mut Data::default()).unwrap();
    });
    let expiries: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|e| e.fields.text("operation") == Some("expire"))
        .collect();
    assert_eq!(expiries.len(), 2);
    assert!(
        expiries
            .iter()
            .all(|e| e.scope.iter().all(|s| s.name != "cancelled_application"))
    );
    assert_eq!(expiries[1].fields.0["timer_generation"], Value::Unsigned(1));
}

#[test]
fn panic_boundary_preserves_payload_rearm_and_remaining_batch() {
    let capture = Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let mut s = scheduler();
        let untouched = s.create(None);
        let panicking = s.create(Some(callback));
        s.add(&untouched, TimeVal::new(100, 0)).unwrap();
        s.add(&panicking, TimeVal::new(100, 0)).unwrap();
        let mut data = Data {
            panic: true,
            ..Data::default()
        };
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.update(&mut data)))
            .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>().copied(),
            Some("original callback panic")
        );
        assert_eq!(s.pending(&untouched), Ok(true));
        assert_eq!(s.deadline(&panicking), Ok(TimeVal::new(102, 0)));
        assert_eq!(s.update(&mut data), Ok(1)); // dispatch guard was reset
        assert_eq!(data.calls, 1);
        s.clock.now = 102;
        data.panic = false;
        assert_eq!(s.update(&mut data), Ok(1));
        assert_eq!(data.calls, 2);
    });
    let events = capture.events();
    let panic = events
        .iter()
        .find(|e| e.fields.text("error_category") == Some("callback_panic"))
        .unwrap();
    assert_eq!(panic.target, "osmocore_rs::timer");
    assert_eq!(panic.level, tracing::Level::ERROR);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.fields.text("operation") == Some("expire"))
            .count(),
        3
    );
}
