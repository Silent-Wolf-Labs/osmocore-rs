// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::tdef::TDefUnit;
use crate::time::{TimeError, TimeSpec, TimeVal, TimeZone};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct FixedClock;
impl Clock for FixedClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        Ok(TimeVal::new(100, 0))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        Ok(TimeSpec::new(0, 105_000))
    }
}

fn seeded() -> TimeCc<'static> {
    TimeCc {
        cfg: TimeCcConfig {
            gran_usec: 10,
            rate_ctr: Some(0),
            ..Default::default()
        },
        start_time: 100,
        last_counted_time: 100,
        sum: 5,
        ..Default::default()
    }
}

fn bind_granularity(tc: &mut TimeCc<'_>, value: u64) {
    tc.cfg.t_gran = 1;
    tc.cfg.t_defs = Some(vec![
        TDef {
            id: 1,
            val: value,
            unit: TDefUnit::US,
            ..Default::default()
        },
        TDef::default(),
    ]);
}

#[test]
fn transitions_accumulate_active_time_and_return_ordered_reporting_effects() {
    let mut tc = seeded();
    tc.sum = 0;
    let policy = AccountingPolicy::from_config(&tc.cfg);
    let activate = tc.transition(100, AccountingEvent::FlagChanged(true), policy);
    assert_eq!(activate.report, None);
    assert!(!tc.flag_state, "flag is committed after reporting effects");
    tc.apply_accounting(activate);
    let report = tc.transition(105, AccountingEvent::Wakeup, policy);
    assert_eq!((tc.total_sum, tc.sum, tc.reported_sum), (5, 5, 0));
    assert_eq!(report.report.as_ref().unwrap().increment, 1);
    tc.apply_accounting(report);
    assert_eq!((tc.reported_sum, tc.cfg.rate_ctr), (10, Some(1)));
    assert_eq!(tc.wakeup_at(105, policy), Some(115));
    let stop = tc.transition(108, AccountingEvent::FlagChanged(false), policy);
    assert_eq!(
        stop.report, None,
        "round-ahead time prevents a second report"
    );
    tc.apply_accounting(stop);
    assert_eq!((tc.total_sum, tc.sum), (8, 8));
    assert_eq!(tc.wakeup_at(108, policy), None);
}

#[test]
fn inactive_wakeups_forget_only_when_due_and_preserve_lifetime_accounting() {
    let mut tc = seeded();
    tc.cfg.forget_sum_usec = 10;
    tc.reported_sum = 10;
    tc.total_sum = 25;
    let policy = AccountingPolicy::from_config(&tc.cfg);
    assert_eq!(tc.wakeup_at(105, policy), Some(110));
    assert_eq!(
        tc.transition(109, AccountingEvent::Wakeup, policy),
        AccountingEffects::default()
    );
    assert_eq!(
        (tc.sum, tc.reported_sum, tc.last_counted_time),
        (5, 10, 100)
    );
    tc.transition(110, AccountingEvent::Wakeup, policy);
    assert_eq!((tc.sum, tc.reported_sum, tc.last_counted_time), (0, 0, 110));
    assert_eq!((tc.total_sum, tc.cfg.rate_ctr), (25, Some(0)));
    assert_eq!(tc.wakeup_at(110, policy), None);
}

#[test]
fn optional_deadlines_preserve_maximum_and_wrapping_boundary_behavior() {
    let mut tc = seeded();
    tc.sum = 0;
    tc.flag_state = true;
    let policy = AccountingPolicy::from_config(&tc.cfg);
    assert_eq!(tc.wakeup_at(u64::MAX - 5, policy), None);
    assert_eq!(tc.wakeup_at(u64::MAX - 4, policy), Some(0));
    tc.flag_state = false;
    tc.sum = 1;
    tc.cfg.forget_sum_usec = 5;
    let policy = AccountingPolicy::from_config(&tc.cfg);
    tc.last_counted_time = u64::MAX - 5;
    assert_eq!(tc.wakeup_at(0, policy), None);
    tc.last_counted_time += 1;
    assert_eq!(tc.wakeup_at(0, policy), Some(0));
}

#[test]
fn flag_changes_and_wakeups_observe_tdefs_on_opposite_sides_of_reporting() {
    let mut flag = seeded();
    bind_granularity(&mut flag, 20);
    flag.process_event(
        &mut TimerScheduler::new(FixedClock),
        AccountingEvent::FlagChanged(true),
    )
    .unwrap();
    assert_eq!(
        flag.cfg.rate_ctr,
        Some(1),
        "flag change reports using granularity 10"
    );
    assert_eq!(
        (flag.cfg.gran_usec, flag.sum, flag.reported_sum),
        (20, 0, 0)
    );

    let mut wakeup = seeded();
    wakeup.flag_state = true;
    bind_granularity(&mut wakeup, 20);
    wakeup
        .process_event(
            &mut TimerScheduler::new(FixedClock),
            AccountingEvent::Wakeup,
        )
        .unwrap();
    assert_eq!(
        wakeup.cfg.rate_ctr,
        Some(0),
        "wakeup discards pending time before reporting"
    );
    assert_eq!(
        (wakeup.cfg.gran_usec, wakeup.sum, wakeup.reported_sum),
        (20, 0, 0)
    );
}

#[test]
fn failed_policy_resolution_preserves_report_flag_and_configuration_prefix() {
    let mut tc = seeded();
    bind_granularity(&mut tc, 20);
    tc.cfg.t_round_threshold = 2;
    let result = tc.process_event(
        &mut TimerScheduler::new(FixedClock),
        AccountingEvent::FlagChanged(true),
    );
    assert_eq!(
        result,
        Err(TimeCcError::TDef(TDefError::NegativeFallback {
            id: 2,
            fallback: -1
        }))
    );
    assert_eq!(tc.cfg.gran_usec, 20);
    assert_eq!(
        (tc.cfg.rate_ctr, tc.flag_state, tc.sum, tc.reported_sum),
        (Some(1), true, 5, 10)
    );
    assert_eq!(tc.last_counted_time, 105);
    assert!(tc.timer().is_none());
}

#[test]
fn sink_panic_preserves_elapsed_prefix_without_committing_report_or_flag() {
    struct PanickingSink;
    impl CounterSink for PanickingSink {
        fn add(&mut self, _: i32) {
            panic!("sink failure");
        }
    }
    let mut tc = TimeCc {
        cfg: TimeCcConfig {
            gran_usec: 10,
            rate_ctr: Some(PanickingSink),
            ..Default::default()
        },
        flag_state: true,
        start_time: 100,
        last_counted_time: 100,
        ..Default::default()
    };
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            tc.process_event(
                &mut TimerScheduler::new(FixedClock),
                AccountingEvent::FlagChanged(false),
            )
            .unwrap();
        }))
        .is_err()
    );
    assert_eq!((tc.total_sum, tc.sum, tc.last_counted_time), (5, 5, 105));
    assert_eq!((tc.reported_sum, tc.flag_state), (0, true));
}

#[test]
fn absent_deadline_keeps_existing_timer_and_deadline() {
    let mut tc = seeded();
    let mut scheduler = TimerScheduler::new(FixedClock);
    tc.arm_wakeup(&mut scheduler, 100, Some(110)).unwrap();
    let handle = tc.timer().unwrap().clone();
    let deadline = scheduler.deadline(&handle).unwrap();
    tc.arm_wakeup(&mut scheduler, 110, None).unwrap();
    assert!(scheduler.pending(&handle).unwrap());
    assert_eq!(scheduler.deadline(&handle).unwrap(), deadline);
    assert_eq!(tc.timer(), Some(&handle));
}
