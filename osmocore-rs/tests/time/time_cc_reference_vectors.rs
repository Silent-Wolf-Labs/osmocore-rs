use super::time_cc_fixture;
use osmocore_rs::compat::tdef::osmo_tdef_set;
const TEXT: &str = include_str!("../compatibility/vectors/time_cc/time_cc_vectors.ndjson");
const REFERENCE: &str = include_str!("../compatibility/vectors/time_cc/time_cc_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/time_cc/time_cc_generator_provenance.json");
const DIAGNOSTICS: &str = include_str!("../compatibility/vectors/time_cc/time_cc_diagnostics.json");
#[test]
fn time_cc_evidence_integrity() {
    time_cc_fixture::checks::integrity(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}
#[test]
fn time_cc_evidence_corruptions() {
    time_cc_fixture::checks::corruptions(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}

use osmocore_rs::{
    rate_ctr::{RateCtr, RateCtrPerIntv},
    tdef::{TDef, TDefError, TDefUnit},
    time::{Clock, ClockId, ClockOverrides, TimeError, TimeSpec, TimeVal, TimeZone},
    time_accounting::{CounterSink, TimeCc, TimeCcError},
    timer::{TimerError, TimerId, TimerScheduler},
};
use std::{cell::RefCell, rc::Rc};
use time_cc_fixture::{signed as s, unsigned as u};

#[derive(Default)]
struct Source {
    wall_error: bool,
}
impl Clock for Source {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        if self.wall_error {
            Err(TimeError::SystemError(5))
        } else {
            Ok(TimeVal::new(100, 0))
        }
    }
    fn clock_gettime(&mut self, id: ClockId) -> Result<TimeSpec, TimeError> {
        assert_eq!(id, ClockId::MONOTONIC);
        Err(TimeError::SystemError(5))
    }
}
#[derive(Default)]
struct Counter {
    current: u64,
    previous: u64,
    marker: u64,
    increments: Vec<i32>,
}
#[derive(Clone)]
struct Sink(Rc<RefCell<Counter>>);
impl CounterSink for Sink {
    fn add(&mut self, increment: i32) {
        assert!(increment > 0);
        let mut counter = self.0.borrow_mut();
        counter.current = counter.current.wrapping_add(increment as u64);
        counter.increments.push(increment);
    }
}
fn clock() -> ClockOverrides<Source> {
    let mut c = ClockOverrides::new(Source::default());
    c.gettimeofday_override = true;
    c.gettimeofday_override_time = TimeVal::new(100, 0);
    c.clock_override_enable(ClockId::MONOTONIC, true);
    *c.clock_override_gettimespec_mut(ClockId::MONOTONIC)
        .unwrap() = TimeSpec::new(1, 0);
    c
}
fn definitions(unit: TDefUnit, gran: u64, threshold: u64, forget: u64) -> Vec<TDef<'static>> {
    vec![
        TDef {
            id: -23,
            unit,
            val: gran,
            ..TDef::default()
        },
        TDef {
            id: -24,
            unit,
            val: threshold,
            ..TDef::default()
        },
        TDef {
            id: -25,
            unit,
            val: forget,
            ..TDef::default()
        },
        TDef::default(),
    ]
}

#[test]
fn time_cc_c_replay_and_combined_stages_1_through_4() {
    let rows = time_cc_fixture::validate(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut scheduler = TimerScheduler::new(clock());
    let mut tc: TimeCc<'static, Sink> = TimeCc::new();
    let mut counter = Rc::new(RefCell::new(Counter::default()));
    let mut capped = 0;
    for row in &rows[1..] {
        scheduler.clock.gettimeofday_override_time =
            TimeVal::new(s(row, "wall_s"), s(row, "wall_us"));
        *scheduler
            .clock
            .clock_override_gettimespec_mut(ClockId::MONOTONIC)
            .unwrap() = TimeSpec::new(s(row, "mono_s"), s(row, "mono_ns"));
        scheduler
            .clock
            .clock_override_enable(ClockId::MONOTONIC, u(row, "fail") == 0);
        let arg = s(row, "arg");
        let mut work = -1;
        let sink = |counter: &Rc<RefCell<Counter>>| {
            if arg != 0 {
                Some(Sink(counter.clone()))
            } else {
                None
            }
        };
        match row["op"].as_str() {
            "init" => {
                assert_eq!(scheduler.check(), 0);
                tc = TimeCc::new();
                counter = Rc::new(RefCell::new(Counter {
                    previous: 19,
                    marker: 39,
                    ..Counter::default()
                }));
                tc.cfg.gran_usec = u(row, "a");
                tc.cfg.round_threshold_usec = u(row, "b");
                tc.cfg.forget_sum_usec = u(row, "c");
                tc.cfg.rate_ctr = sink(&counter);
            }
            "flag" => tc.set_flag(&mut scheduler, arg != 0).unwrap(),
            "update" => work = tc.update(&mut scheduler).unwrap(),
            "cleanup" => tc.cleanup(&mut scheduler).unwrap(),
            "config" => {
                tc.cfg.gran_usec = u(row, "a");
                tc.cfg.round_threshold_usec = u(row, "b");
                tc.cfg.forget_sum_usec = u(row, "c");
                tc.cfg.rate_ctr = sink(&counter);
            }
            "bindings" => {
                tc.cfg.t_defs = Some(definitions(
                    TDefUnit(arg as i32),
                    u(row, "a"),
                    u(row, "b"),
                    u(row, "c"),
                ));
                tc.cfg.t_gran = -23;
                tc.cfg.t_round_threshold = -24;
                tc.cfg.t_forget_sum = -25;
            }
            "tset" => {
                let defs = tc.cfg.t_defs.as_deref_mut().unwrap();
                for (id, key) in [(-23, "a"), (-24, "b"), (-25, "c")] {
                    osmo_tdef_set(Some(defs), id, u(row, key), TDefUnit(arg as i32)).unwrap();
                }
                work = 0;
            }
            "seed" => {
                tc.start_time = u(row, "a");
                tc.last_counted_time = u(row, "b");
                tc.sum = u(row, "c");
                tc.reported_sum = u(row, "d");
                tc.total_sum = u(row, "e");
                tc.flag_state = arg != 0;
            }
            "counter" => counter.borrow_mut().current = u(row, "a"),
            op => panic!("unknown operation {op}"),
        }
        let check = |key: &str, actual: u64| {
            assert_eq!(
                actual,
                u(row, key),
                "scenario {} step {} field {key}",
                row["scenario"],
                row["step"]
            )
        };
        check("gran", tc.cfg.gran_usec);
        check("threshold", tc.cfg.round_threshold_usec);
        check("forget", tc.cfg.forget_sum_usec);
        check("sink", u64::from(tc.cfg.rate_ctr.is_some()));
        check("bindings", u64::from(tc.cfg.t_defs.is_some()));
        check("flag", u64::from(tc.flag_state));
        check("total", tc.total_sum);
        check("start", tc.start_time);
        check("last", tc.last_counted_time);
        check("sum", tc.sum);
        check("reported", tc.reported_sum);
        check("counter", counter.borrow().current);
        check("previous", counter.borrow().previous);
        assert_eq!(counter.borrow().marker, 39);
        let active = tc
            .timer()
            .map(|t| scheduler.pending(t).unwrap())
            .unwrap_or(false);
        let deadline = tc
            .timer()
            .map(|t| scheduler.deadline(t).unwrap())
            .unwrap_or_default();
        check("active", u64::from(active));
        check("timers", scheduler.check() as u64);
        assert_eq!(
            deadline,
            TimeVal::new(s(row, "deadline_s"), s(row, "deadline_us")),
            "scenario {} step {}",
            row["scenario"],
            row["step"]
        );
        assert_eq!(work as i64, s(row, "work"));
        capped += usize::from(counter.borrow().increments.last() == Some(&i32::MAX));
    }
    assert!(capped > 0, "INT_MAX cap exercised against an attached sink");
    assert_eq!(scheduler.check(), 0);
}

#[test]
fn time_cc_c_replay_with_standalone_rate_counter() {
    let rows = time_cc_fixture::validate(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut scheduler = TimerScheduler::new(clock());
    let mut tc: TimeCc<'static, RateCtr> = TimeCc::new();
    let intervals = std::array::from_fn(|i| RateCtrPerIntv {
        last: u64::MAX - i as u64,
        rate: (1u64 << 63) + i as u64,
    });
    let mut counter = RateCtr::default();
    for row in &rows[1..] {
        scheduler.clock.gettimeofday_override_time =
            TimeVal::new(s(row, "wall_s"), s(row, "wall_us"));
        *scheduler
            .clock
            .clock_override_gettimespec_mut(ClockId::MONOTONIC)
            .unwrap() = TimeSpec::new(s(row, "mono_s"), s(row, "mono_ns"));
        scheduler
            .clock
            .clock_override_enable(ClockId::MONOTONIC, u(row, "fail") == 0);
        let arg = s(row, "arg");
        let mut work = -1;
        // Retain the detached counter when configuration disconnects the sink.
        if let Some(attached) = tc.cfg.rate_ctr {
            counter = attached;
        }
        let sink = |counter: &RateCtr| {
            if arg != 0 { Some(*counter) } else { None }
        };
        match row["op"].as_str() {
            "init" => {
                assert_eq!(scheduler.check(), 0);
                tc = TimeCc::new();
                counter = RateCtr {
                    previous: 19,
                    intv: intervals,
                    ..RateCtr::default()
                };
                tc.cfg.gran_usec = u(row, "a");
                tc.cfg.round_threshold_usec = u(row, "b");
                tc.cfg.forget_sum_usec = u(row, "c");
                tc.cfg.rate_ctr = sink(&counter);
            }
            "flag" => tc.set_flag(&mut scheduler, arg != 0).unwrap(),
            "update" => work = tc.update(&mut scheduler).unwrap(),
            "cleanup" => tc.cleanup(&mut scheduler).unwrap(),
            "config" => {
                tc.cfg.gran_usec = u(row, "a");
                tc.cfg.round_threshold_usec = u(row, "b");
                tc.cfg.forget_sum_usec = u(row, "c");
                tc.cfg.rate_ctr = sink(&counter);
            }
            "bindings" => {
                tc.cfg.t_defs = Some(definitions(
                    TDefUnit(arg as i32),
                    u(row, "a"),
                    u(row, "b"),
                    u(row, "c"),
                ));
                tc.cfg.t_gran = -23;
                tc.cfg.t_round_threshold = -24;
                tc.cfg.t_forget_sum = -25;
            }
            "tset" => {
                let defs = tc.cfg.t_defs.as_deref_mut().unwrap();
                for (id, key) in [(-23, "a"), (-24, "b"), (-25, "c")] {
                    osmo_tdef_set(Some(defs), id, u(row, key), TDefUnit(arg as i32)).unwrap();
                }
                work = 0;
            }
            "seed" => {
                tc.start_time = u(row, "a");
                tc.last_counted_time = u(row, "b");
                tc.sum = u(row, "c");
                tc.reported_sum = u(row, "d");
                tc.total_sum = u(row, "e");
                tc.flag_state = arg != 0;
            }
            "counter" => {
                counter.current = u(row, "a");
                if let Some(attached) = &mut tc.cfg.rate_ctr {
                    attached.current = counter.current;
                }
            }
            op => panic!("unknown operation {op}"),
        }
        if let Some(attached) = tc.cfg.rate_ctr {
            counter = attached;
        }
        let check = |key: &str, actual: u64| {
            assert_eq!(
                actual,
                u(row, key),
                "scenario {} step {} field {key}",
                row["scenario"],
                row["step"]
            )
        };
        check("gran", tc.cfg.gran_usec);
        check("threshold", tc.cfg.round_threshold_usec);
        check("forget", tc.cfg.forget_sum_usec);
        check("sink", u64::from(tc.cfg.rate_ctr.is_some()));
        check("bindings", u64::from(tc.cfg.t_defs.is_some()));
        check("flag", u64::from(tc.flag_state));
        check("total", tc.total_sum);
        check("start", tc.start_time);
        check("last", tc.last_counted_time);
        check("sum", tc.sum);
        check("reported", tc.reported_sum);
        check("counter", counter.current);
        check("previous", counter.previous);
        assert_eq!(
            counter.intv, intervals,
            "time_cc must preserve every interval field"
        );
        let active = tc
            .timer()
            .map(|t| scheduler.pending(t).unwrap())
            .unwrap_or(false);
        let deadline = tc
            .timer()
            .map(|t| scheduler.deadline(t).unwrap())
            .unwrap_or_default();
        check("active", u64::from(active));
        check("timers", scheduler.check() as u64);
        assert_eq!(
            deadline,
            TimeVal::new(s(row, "deadline_s"), s(row, "deadline_us")),
            "scenario {} step {}",
            row["scenario"],
            row["step"]
        );
        assert_eq!(work as i64, s(row, "work"));
    }
    assert_eq!(scheduler.check(), 0);
}

#[test]
fn time_cc_safe_tdef_failures_and_callback_errors() {
    let mut scheduler = TimerScheduler::new(clock());
    let mut tc: TimeCc = TimeCc::new();
    tc.cfg.t_defs = Some(definitions(TDefUnit::US, 10, 1, 0));
    tc.cfg.t_gran = 123;
    let missing = TimeCcError::TDef(TDefError::NegativeFallback {
        id: 123,
        fallback: -1,
    });
    assert_eq!(tc.set_flag(&mut scheduler, true), Err(missing));
    assert_eq!(tc.start_time, 1_000_000);
    assert_eq!(scheduler.check(), 0);
    tc.cleanup(&mut scheduler).unwrap();
    tc.cfg.t_defs = Some(definitions(TDefUnit(-1), 1, 1, 0));
    tc.cfg.t_gran = -23;
    assert_eq!(
        tc.set_flag(&mut scheduler, true),
        Err(TimeCcError::TDef(TDefError::InvalidConversion {
            from: TDefUnit(-1),
            to: TDefUnit::US
        }))
    );
    tc.cleanup(&mut scheduler).unwrap();
    tc.cfg.t_defs = Some(vec![]);
    assert_eq!(
        tc.set_flag(&mut scheduler, true),
        Err(TimeCcError::TDef(TDefError::MissingTerminator))
    );
    tc.cleanup(&mut scheduler).unwrap();
    tc.cfg.t_defs = Some(definitions(TDefUnit::US, 10, 1, 0));
    tc.cfg.t_gran = -23;
    tc.cfg.t_round_threshold = 123;
    assert_eq!(
        tc.set_flag(&mut scheduler, true),
        Err(TimeCcError::TDef(TDefError::NegativeFallback {
            id: 123,
            fallback: -1
        }))
    );
    assert_eq!(
        tc.cfg.gran_usec, 10,
        "valid configuration prefix already applied"
    );
    tc.cleanup(&mut scheduler).unwrap();
    tc.cfg.t_round_threshold = 0;
    tc.set_flag(&mut scheduler, true).unwrap();
    tc.cfg.t_gran = 123;
    scheduler.clock.gettimeofday_override_time = TimeVal::new(101, 0);
    assert_eq!(
        tc.update(&mut scheduler),
        Err(TimeCcError::TDef(TDefError::NegativeFallback {
            id: 123,
            fallback: -1
        }))
    );
    assert_eq!(scheduler.check(), 0, "failed callback remains consumed");
    assert!(tc.take_callback_error().is_none());
    tc.cleanup(&mut scheduler).unwrap();
}

#[test]
fn time_cc_clock_errors_zero_start_and_ownership() {
    let mut scheduler = TimerScheduler::new(clock());
    let mut tc: TimeCc = TimeCc::new();
    tc.cfg.gran_usec = 10;
    tc.cfg.rate_ctr = Some(0);
    scheduler.clock.gettimeofday_override = false;
    scheduler.clock.source.wall_error = true;
    assert_eq!(
        tc.set_flag(&mut scheduler, true),
        Err(TimeCcError::Timer(TimerError::Time(
            TimeError::SystemError(5)
        )))
    );
    assert!(tc.flag_state);
    assert_eq!(tc.start_time, 1_000_000);
    assert_eq!(scheduler.check(), 0);
    tc.cleanup(&mut scheduler).unwrap();
    scheduler.clock.gettimeofday_override = true;
    tc.set_flag(&mut scheduler, true).unwrap();
    let mut foreign = TimerScheduler::new(clock());
    assert_eq!(
        tc.cleanup(&mut foreign),
        Err(TimeCcError::Timer(TimerError::InvalidHandle))
    );
    assert_eq!(scheduler.check(), 1);
    assert_eq!(
        tc.set_flag(&mut foreign, true),
        Err(TimeCcError::Timer(TimerError::InvalidHandle))
    );
    assert_eq!(
        tc.update(&mut foreign),
        Err(TimeCcError::Timer(TimerError::InvalidHandle))
    );
    assert_eq!(scheduler.check(), 1);
    let handle = tc.timer().unwrap().clone();
    tc.cleanup(&mut scheduler).unwrap();
    assert_eq!(scheduler.pending(&handle), Err(TimerError::InvalidHandle));
    assert_eq!(tc.cfg.rate_ctr, Some(0));
    scheduler
        .clock
        .clock_override_enable(ClockId::MONOTONIC, false); // source fails -> defined zero
    tc.set_flag(&mut scheduler, true).unwrap();
    assert_eq!(tc.start_time, 0);
    tc.sum = 9;
    tc.total_sum = 11;
    tc.set_flag(&mut scheduler, true).unwrap();
    assert_eq!((tc.sum, tc.total_sum), (0, 0));
    tc.cleanup(&mut scheduler).unwrap();
}

#[test]
fn time_cc_combines_with_application_timers_and_stage_3_config() {
    type Context = TimeCc<'static>;
    fn reconfigure(
        scheduler: &mut TimerScheduler<ClockOverrides<Source>, Context>,
        _: TimerId,
        tc: &mut Context,
    ) {
        let defs = tc.cfg.t_defs.as_deref_mut().unwrap();
        osmo_tdef_set(Some(defs), -23, 20, TDefUnit::US).unwrap();
        // Changing the TDef alone doesn't observe it yet. Unchanged flags remain noops.
        tc.set_flag(scheduler, true).unwrap();
    }
    let mut scheduler = TimerScheduler::new(clock());
    let mut tc: Context = TimeCc::new();
    tc.cfg.t_defs = Some(definitions(TDefUnit::US, 10, 0, 0));
    tc.cfg.t_gran = -23;
    tc.cfg.rate_ctr = Some(0);
    tc.set_flag(&mut scheduler, true).unwrap();
    let peer = scheduler.create(Some(reconfigure));
    scheduler.schedule(&peer, 0, 2).unwrap();
    scheduler.clock.gettimeofday_override_time = TimeVal::new(100, 2);
    *scheduler
        .clock
        .clock_override_gettimespec_mut(ClockId::MONOTONIC)
        .unwrap() = TimeSpec::new(1, 2000);
    assert_eq!(tc.update(&mut scheduler), Ok(1));
    assert_eq!(tc.cfg.gran_usec, 10);
    scheduler.clock.gettimeofday_override_time = TimeVal::new(100, 5);
    *scheduler
        .clock
        .clock_override_gettimespec_mut(ClockId::MONOTONIC)
        .unwrap() = TimeSpec::new(1, 5000);
    assert_eq!(tc.update(&mut scheduler), Ok(1));
    assert_eq!(tc.cfg.gran_usec, 20);
    assert_eq!(tc.sum, 5);
    assert_eq!(tc.cfg.rate_ctr, Some(0));
    // Monotonic accrues independently; the wall deadline still gates dispatch.
    *scheduler
        .clock
        .clock_override_gettimespec_mut(ClockId::MONOTONIC)
        .unwrap() = TimeSpec::new(1, 30000);
    assert_eq!(tc.update(&mut scheduler), Ok(0));
    assert_eq!(tc.sum, 5);
    scheduler.clock.gettimeofday_override_time = TimeVal::new(100, 10);
    assert_eq!(tc.update(&mut scheduler), Ok(1));
    assert_eq!(tc.sum, 30);
    assert_eq!(tc.cfg.rate_ctr, Some(2));
    tc.cleanup(&mut scheduler).unwrap();
    scheduler.release(&peer).unwrap();
    assert_eq!(scheduler.check(), 0);
}

#[test]
fn time_cc_rearm_failure_preserves_counted_prefix_and_consumes_timer() {
    let mut scheduler = TimerScheduler::new(clock());
    let mut tc: TimeCc = TimeCc::new();
    tc.cfg.gran_usec = 10;
    tc.cfg.rate_ctr = Some(0);
    tc.set_flag(&mut scheduler, true).unwrap();
    *scheduler
        .clock
        .clock_override_gettimespec_mut(ClockId::MONOTONIC)
        .unwrap() = TimeSpec::new(1, 5000);
    // Timer update uses the override successfully. Its application callback
    // disables the wall override before time_cc's own callback tries to rearm.
    fn fail_wall(
        s: &mut TimerScheduler<ClockOverrides<Source>, TimeCc<'static>>,
        _: TimerId,
        _: &mut TimeCc<'static>,
    ) {
        s.clock.gettimeofday_override = false;
        s.clock.source.wall_error = true;
    }
    let peer = scheduler.create(Some(fail_wall));
    scheduler.schedule(&peer, 0, 5).unwrap();
    scheduler.clock.gettimeofday_override_time = TimeVal::new(100, 5);
    assert_eq!(
        tc.update(&mut scheduler),
        Err(TimeCcError::Timer(TimerError::Time(
            TimeError::SystemError(5)
        )))
    );
    assert_eq!((tc.sum, tc.reported_sum, tc.cfg.rate_ctr), (5, 10, Some(1)));
    assert_eq!(scheduler.check(), 0);
    tc.cleanup(&mut scheduler).unwrap();
    scheduler.release(&peer).unwrap();
}
