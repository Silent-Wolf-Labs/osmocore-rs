use super::time_fixture::{self, Record, number};
use osmocore_rs::time::*;
use osmocore_rs::timer::*;

type Scheduler = TimerScheduler<ClockOverrides<Source>, Context>;
#[derive(Default)]
struct Source {
    calls: usize,
    fail: bool,
}
impl Clock for Source {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        self.calls += 1;
        if self.fail {
            Err(TimeError::SystemError(5))
        } else {
            Ok(TimeVal::new(100, 0))
        }
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        panic!("timers use wall time")
    }
}
#[derive(Default)]
struct Context {
    ids: Vec<TimerId>,
    mode: String,
    callbacks: usize,
    trace: Vec<Record>,
}
fn scheduler() -> Scheduler {
    let mut clock = ClockOverrides::new(Source::default());
    clock.gettimeofday_override = true;
    clock.gettimeofday_override_time = TimeVal::new(100, 0);
    TimerScheduler::new(clock)
}
fn trace(s: &Scheduler, d: &mut Context, event: &str, who: i64, ret: i32) {
    let active: i64 = d
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| if s.pending(id).unwrap() { 1 << i } else { 0 })
        .sum();
    let near = s.nearest();
    let time = near.unwrap_or_default();
    let mut row = Record::new();
    for (key, value) in [
        ("type", "timer".to_owned()),
        ("case", d.mode.clone()),
        ("event", event.to_owned()),
        ("who", who.to_string()),
        ("depth", "0".to_owned()),
        ("ret", ret.to_string()),
        ("count", s.check().to_string()),
        ("active", active.to_string()),
        ("near", i32::from(near.is_some()).to_string()),
        ("sec", time.seconds.to_string()),
        ("frac", time.microseconds.to_string()),
    ] {
        row.insert(key.to_owned(), value);
    }
    d.trace.push(row);
}
fn replacement(s: &mut Scheduler, id: TimerId, d: &mut Context) {
    let who = d.ids.iter().position(|x| *x == id).unwrap();
    trace(s, d, "replacement", who as i64, 0);
}
fn callback(s: &mut Scheduler, id: TimerId, d: &mut Context) {
    d.callbacks += 1;
    let who = d.ids.iter().position(|x| *x == id).unwrap();
    trace(s, d, "callback", who as i64, 0);
    if who != 0 {
        return;
    }
    match d.mode.as_str() {
        "prepare_during_dispatch" => {
            s.prepare().unwrap();
            trace(s, d, "prepared", 0, s.nearest_ms().unwrap());
            s.delete(&d.ids[1]).unwrap();
            s.prepare().unwrap();
            trace(s, d, "prepared_future", 0, s.nearest_ms().unwrap());
        }
        "self_reschedule" if d.callbacks == 1 => s.schedule(&id, 0, 0).unwrap(),
        "cancel_peer" => s.delete(&d.ids[1]).unwrap(),
        "reschedule_peer" => s.schedule(&d.ids[1], 0, 0).unwrap(),
        "immediate_deferred" => s.schedule(&d.ids[2], 0, 0).unwrap(),
        "replace_self" => {
            s.setup(&id, Some(replacement)).unwrap();
            s.schedule(&id, 0, 0).unwrap();
        }
        "replace_peer" => s.setup(&d.ids[1], Some(replacement)).unwrap(),
        _ => (),
    }
}
fn rows() -> Vec<Record> {
    time_fixture::validate(
        include_str!("../compatibility/vectors/time/time_vectors.ndjson"),
        include_str!("../compatibility/vectors/time/time_provenance.json"),
        include_str!("../compatibility/vectors/time/time_generator_provenance.json"),
    )
}
#[test]
fn ordinary_batches_match_every_c_trace_field() {
    let rows = rows();
    let mut compared = 0;
    for mode in [
        "batch_order",
        "prepare_during_dispatch",
        "self_reschedule",
        "cancel_peer",
        "reschedule_peer",
        "immediate_deferred",
        "replace_self",
        "replace_peer",
    ] {
        let mut s = scheduler();
        let mut d = Context {
            mode: mode.to_owned(),
            ..Context::default()
        };
        d.ids = (0..4).map(|_| s.create(Some(callback))).collect();
        if mode == "batch_order" {
            for (who, seconds) in [(0, 0), (1, 0), (2, -1)] {
                s.schedule(&d.ids[who], seconds, 0).unwrap();
            }
        } else {
            if matches!(
                mode,
                "prepare_during_dispatch" | "cancel_peer" | "reschedule_peer" | "replace_peer"
            ) {
                s.schedule(&d.ids[1], 0, 0).unwrap();
            }
            s.schedule(&d.ids[0], 0, 0).unwrap();
            if mode == "prepare_during_dispatch" {
                s.schedule(&d.ids[3], 1, 1).unwrap();
            }
        }
        s.prepare().unwrap();
        trace(&s, &mut d, "before", -1, s.nearest_ms().unwrap());
        let ret = s.update(&mut d).unwrap();
        trace(&s, &mut d, "after", -1, ret);
        let ret = s.update(&mut d).unwrap();
        trace(&s, &mut d, "second", -1, ret);
        for id in &d.ids {
            s.delete(id).unwrap();
        }
        s.prepare().unwrap();
        trace(&s, &mut d, "clean", -1, s.nearest_ms().unwrap());
        let expected: Vec<_> = rows
            .iter()
            .filter(|r| r.get("case").map(String::as_str) == Some(mode))
            .cloned()
            .collect();
        compared += expected.len();
        assert_eq!(d.trace, expected, "{mode}");
    }
    assert_eq!(compared, 49);
}
#[test]
fn timer_api_observations_match_c() {
    let mut s = scheduler();
    s.clock.gettimeofday_override_time = TimeVal::new(100, 500000);
    let id = s.create(None);
    let mut compared = 0;
    for row in rows().iter().filter(|r| r["type"] == "timer_api") {
        let a = TimeVal::new(number(row, "as"), number(row, "af"));
        let now = TimeVal::new(number(row, "bs"), number(row, "bf"));
        let expected = TimeVal::new(number(row, "rs"), number(row, "rf"));
        match row["op"].as_str() {
            "remaining" | "remaining_clock" => {
                s.add(&id, a).unwrap();
                s.delete(&id).unwrap();
                let result = s
                    .remaining(
                        &id,
                        if row["op"] == "remaining" {
                            Some(now)
                        } else {
                            None
                        },
                    )
                    .unwrap();
                assert_eq!(result.time, expected);
                assert_eq!(i64::from(result.status), number(row, "ret"));
            }
            "schedule" => {
                s.schedule(&id, a.seconds as i32, a.microseconds as i32)
                    .unwrap();
                assert_eq!(s.deadline(&id).unwrap(), expected);
                assert_eq!(i64::from(s.pending(&id).unwrap()), number(row, "ret"));
                s.delete(&id).unwrap();
            }
            "nearest" => {
                s.add(&id, a).unwrap();
                s.prepare().unwrap();
                assert_eq!(s.nearest(), Some(expected));
                assert_eq!(i64::from(s.nearest_ms().unwrap()), number(row, "ret"));
                s.delete(&id).unwrap();
            }
            _ => unreachable!(),
        }
        compared += 1;
    }
    assert_eq!(compared, 32);
}
#[test]
fn absolute_deadline_lifecycle_and_cache_match_c() {
    let mut s = scheduler();
    s.clock.gettimeofday_override_time = TimeVal::new(100, 500000);
    let mut d = Context {
        mode: "absolute_lifecycle".to_owned(),
        ..Context::default()
    };
    d.ids.push(s.create(None));
    let id = d.ids[0].clone();
    s.prepare().unwrap();
    trace(&s, &mut d, "empty", -1, s.nearest_ms().unwrap());
    s.add(&id, TimeVal::new(i64::MAX, 0)).unwrap();
    s.prepare().unwrap();
    trace(&s, &mut d, "max_nearest", -1, s.nearest_ms().unwrap());
    s.clock.gettimeofday_override_time = TimeVal::new(i64::MAX, 0);
    let ret = s.update(&mut d).unwrap();
    trace(&s, &mut d, "max_dispatch", -1, ret);
    s.clock.gettimeofday_override_time = TimeVal::new(100, 500000);
    s.add(&id, TimeVal::new(101, 500001)).unwrap();
    s.prepare().unwrap();
    trace(&s, &mut d, "future", -1, s.nearest_ms().unwrap());
    s.add(&id, s.deadline(&id).unwrap()).unwrap();
    trace(&s, &mut d, "readd", -1, s.nearest_ms().unwrap());
    s.delete(&id).unwrap();
    s.delete(&id).unwrap();
    trace(&s, &mut d, "deleted_cache", -1, s.nearest_ms().unwrap());
    s.add(&id, TimeVal::new(100, 500000)).unwrap();
    let ret = s.update(&mut d).unwrap();
    trace(&s, &mut d, "null_dispatch", -1, ret);
    s.prepare().unwrap();
    trace(&s, &mut d, "clean", -1, s.nearest_ms().unwrap());
    let expected: Vec<_> = rows()
        .into_iter()
        .filter(|r| r.get("case").map(String::as_str) == Some("absolute_lifecycle"))
        .collect();
    assert_eq!(d.trace, expected);
}

fn nested(s: &mut Scheduler, _: TimerId, d: &mut Context) {
    if d.mode == "immediate" {
        s.schedule(&d.ids[2], 0, 0).unwrap();
    }
    let count = s.check();
    let near = s.nearest();
    let calls = s.clock.source.calls;
    assert_eq!(s.update(d), Err(TimerError::NestedUpdate));
    assert_eq!(
        (s.check(), s.nearest(), s.clock.source.calls),
        (count, near, calls)
    );
    d.callbacks += 1;
    s.prepare().unwrap();
}
#[test]
fn nested_updates_are_rejected_with_empty_immediate_and_selected_peers() {
    for mode in ["empty", "immediate", "peer"] {
        let mut s = scheduler();
        let mut d = Context {
            mode: mode.to_owned(),
            ..Context::default()
        };
        d.ids = (0..3).map(|_| s.create(None)).collect();
        s.setup(&d.ids[0], Some(nested)).unwrap();
        if mode == "peer" {
            s.schedule(&d.ids[1], 0, 0).unwrap();
        }
        s.schedule(&d.ids[0], 0, 0).unwrap();
        // Disable override: rejected nested updates must not read the source.
        s.clock.gettimeofday_override = false;
        assert_eq!(s.update(&mut d), Ok(1));
        assert_eq!(d.callbacks, 1);
        assert_eq!(s.check(), usize::from(mode == "immediate"));
        assert_eq!(s.update(&mut d), Ok(i32::from(mode == "immediate")));
    }
}
fn panicking(s: &mut Scheduler, id: TimerId, d: &mut Context) {
    s.setup(&id, Some(replacement)).unwrap();
    s.schedule(&id, 0, 0).unwrap();
    s.setup(&d.ids[1], Some(replacement)).unwrap();
    panic!("callback panic");
}
#[test]
fn panic_preserves_undispatched_batch_and_callback_replacements() {
    let mut s = scheduler();
    let mut d = Context {
        ids: (0..3).map(|_| s.create(Some(callback))).collect(),
        ..Default::default()
    };
    for id in d.ids.iter().rev() {
        s.schedule(id, 0, 0).unwrap();
    }
    s.setup(&d.ids[0], Some(panicking)).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.update(&mut d))).is_err());
    assert_eq!(s.check(), 3);
    for id in &d.ids {
        assert!(s.pending(id).unwrap());
    }
    assert_eq!(s.update(&mut d), Ok(1));
    assert_eq!(s.check(), 0);
    assert_eq!(
        d.trace
            .iter()
            .map(|r| (r["event"].as_str(), number(r, "who")))
            .collect::<Vec<_>>(),
        [("replacement", 0), ("replacement", 1), ("callback", 2)]
    );
}
#[test]
fn handles_survive_activation_but_not_release_or_foreign_schedulers() {
    let mut s = scheduler();
    let mut other = scheduler();
    let id = s.create(None);
    s.schedule(&id, 0, 0).unwrap();
    s.delete(&id).unwrap();
    s.schedule(&id, 1, 0).unwrap();
    assert_eq!(s.pending(&id), Ok(true));
    assert_eq!(other.delete(&id), Err(TimerError::InvalidHandle));
    s.release(&id).unwrap();
    let fresh = s.create(None);
    assert_ne!(id, fresh);
    assert_eq!(
        s.add(&id, TimeVal::default()),
        Err(TimerError::InvalidHandle)
    );
    assert_eq!(s.pending(&fresh), Ok(false));
    assert_eq!(other.check(), 0);
}
#[test]
fn clock_and_arithmetic_errors_preserve_activation_and_cache() {
    let mut s = scheduler();
    let id = s.create(None);
    let mut d = Context::default();
    s.schedule(&id, 1, 0).unwrap();
    s.prepare().unwrap();
    let before = (s.deadline(&id).unwrap(), s.nearest(), s.check());
    s.clock.gettimeofday_override = false;
    s.clock.source.fail = true;
    assert_eq!(
        s.schedule(&id, 0, 0),
        Err(TimerError::Time(TimeError::SystemError(5)))
    );
    assert_eq!(
        s.prepare(),
        Err(TimerError::Time(TimeError::SystemError(5)))
    );
    assert_eq!(
        s.remaining(&id, None),
        Err(TimerError::Time(TimeError::SystemError(5)))
    );
    assert_eq!(
        s.update(&mut d),
        Err(TimerError::Time(TimeError::SystemError(5)))
    );
    s.clock.gettimeofday_override = true;
    s.clock.gettimeofday_override_time = TimeVal::new(i64::MAX, 999999);
    assert_eq!(
        s.schedule(&id, 0, 1),
        Err(TimerError::Time(TimeError::ArithmeticOverflow))
    );
    s.clock.gettimeofday_override_time = TimeVal::new(i64::MIN, 0);
    assert_eq!(
        s.prepare(),
        Err(TimerError::Time(TimeError::ArithmeticOverflow))
    );
    assert_eq!(
        s.remaining(&id, None),
        Err(TimerError::Time(TimeError::ArithmeticOverflow))
    );
    assert_eq!((s.deadline(&id).unwrap(), s.nearest(), s.check()), before);
    s.clock.gettimeofday_override_time = TimeVal::default();
    s.add(&id, TimeVal::new(2147483, 648000)).unwrap();
    s.prepare().unwrap();
    assert_eq!(s.nearest_ms(), Err(TimerError::MillisecondsOverflow));
}

fn release_peer(s: &mut Scheduler, _: TimerId, d: &mut Context) {
    let stale = d.ids[1].clone();
    s.release(&stale).unwrap();
    let fresh = s.create(Some(replacement));
    assert_ne!(stale, fresh);
    s.schedule(&fresh, 0, 0).unwrap();
    d.ids[1] = fresh;
    d.callbacks += 1;
}
#[test]
fn released_selected_peer_cannot_dispatch_reused_slot_in_the_old_batch() {
    let mut s = scheduler();
    let mut d = Context {
        ids: (0..2).map(|_| s.create(None)).collect(),
        ..Default::default()
    };
    s.setup(&d.ids[0], Some(release_peer)).unwrap();
    s.schedule(&d.ids[1], 0, 0).unwrap();
    s.schedule(&d.ids[0], 0, 0).unwrap();
    assert_eq!(s.update(&mut d), Ok(1));
    assert_eq!(s.check(), 1);
    assert!(d.trace.is_empty());
    assert_eq!(s.update(&mut d), Ok(1));
    assert_eq!(d.trace[0]["event"], "replacement");
}
fn just_panic(_: &mut Scheduler, _: TimerId, _: &mut Context) {
    panic!("plain panic");
}
#[test]
fn plain_panic_leaves_current_inactive_and_all_undispatched_selected_peers_active() {
    let mut s = scheduler();
    let mut d = Context {
        ids: (0..3).map(|_| s.create(None)).collect(),
        ..Default::default()
    };
    s.setup(&d.ids[0], Some(just_panic)).unwrap();
    for id in d.ids.iter().rev() {
        s.schedule(id, 0, 0).unwrap();
    }
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.update(&mut d))).is_err());
    assert_eq!(s.pending(&d.ids[0]), Ok(false));
    assert_eq!(s.pending(&d.ids[1]), Ok(true));
    assert_eq!(s.pending(&d.ids[2]), Ok(true));
    assert_eq!(s.check(), 2);
    assert_eq!(s.update(&mut d), Ok(1));
    assert_eq!(s.check(), 0);
}
