// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use osmocore_rs::{
    rate_ctr::{RateCtrPeriodicDriver, RateCtrRuntime},
    time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::{TimerId, TimerScheduler},
};
#[derive(Clone)]
struct TestClock(Rc<RefCell<(i64, i64)>>);
impl Clock for TestClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        Ok(TimeVal::new(self.0.borrow().0, 0))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        Ok(TimeSpec::new(self.0.borrow().1, 0))
    }
}
struct Other {
    fired: usize,
}
struct Mixed {
    first: TimerScheduler<TestClock, usize>,
    one: usize,
    second: TimerScheduler<TestClock, Other>,
    two: Other,
    rate: RateCtrPeriodicDriver<TestClock>,
    counters: RateCtrRuntime,
    progress: usize,
}
impl Application for Mixed {
    type Command = ();
    type Error = ();
    fn on_command(&mut self, _: &mut RuntimeContext<'_>, _: ()) -> Result<(), ()> {
        Ok(())
    }
    fn on_event(&mut self, _: &mut RuntimeContext<'_>, event: RuntimeEvent<'_>) -> Result<(), ()> {
        if matches!(event, RuntimeEvent::DriverProgress { .. }) {
            self.progress += 1;
        }
        Ok(())
    }
}
fn one(s: &mut TimerScheduler<TestClock, usize>, id: TimerId, n: &mut usize) {
    *n += 1;
    if *n == 1 {
        s.schedule(&id, 1, 0).unwrap();
    }
}
fn two(_: &mut TimerScheduler<TestClock, Other>, _: TimerId, c: &mut Other) {
    c.fired += 1;
}
#[test]
fn mixed_contexts_rearm_and_monotonic_catchup_survive_wall_changes_without_io() {
    let clock = TestClock(Rc::new(RefCell::new((100, 10))));
    let mut app = Mixed {
        first: TimerScheduler::new(clock.clone()),
        one: 0,
        second: TimerScheduler::new(clock.clone()),
        two: Other { fired: 0 },
        rate: RateCtrPeriodicDriver::new(clock.clone()),
        counters: RateCtrRuntime::new(),
        progress: 0,
    };
    let id = app.first.create(Some(one));
    app.first.schedule(&id, 0, 0).unwrap();
    let id2 = app.second.create(Some(two));
    app.second.schedule(&id2, 1, 0).unwrap();
    app.rate.start().unwrap();
    let mut drivers = DriverSet::default();
    drivers
        .add_driver(
            "one",
            TimerDriver::new(|a: &mut Mixed| (&mut a.first, &mut a.one)),
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    drivers
        .add_driver(
            "two",
            TimerDriver::new(|a: &mut Mixed| (&mut a.second, &mut a.two)),
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    drivers
        .add_driver(
            "rate",
            RateDriver::new(|a: &mut Mixed| (&mut a.rate, &mut a.counters)),
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
        .unwrap();
    assert_eq!(app.one, 1);
    assert_eq!(app.two.fired, 0);
    *clock.0.borrow_mut() = (90, 15);
    rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
        .unwrap();
    assert_eq!(app.one, 1);
    assert_eq!(app.counters.ticks(), 5);
    assert_eq!(app.rate.next_deadline(), Some(TimeSpec::new(16, 0)));
    *clock.0.borrow_mut() = (200, 15);
    rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
        .unwrap();
    assert_eq!(app.one, 2);
    assert_eq!(app.two.fired, 1);
    assert!(app.progress >= 4);
}
struct Failing;
impl RuntimeDriver<App> for Failing {
    fn prepare(&mut self, _: &mut App) -> Result<WakeHint, DriverFailure<()>> {
        Err(DriverFailure::Application(()))
    }
    fn drive(
        &mut self,
        _: &mut App,
        _: &mut RuntimeContext<'_>,
    ) -> Result<DriverProgress, DriverFailure<()>> {
        panic!("disabled driver invoked")
    }
}
#[test]
fn optional_failure_is_preserved_and_disabled_without_resource_identity() {
    let mut app = App::default();
    let mut drivers = DriverSet::default();
    drivers
        .add_driver("optional", Failing, DriverFailurePolicy::Optional)
        .unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    for _ in 0..2 {
        rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
            .unwrap();
    }
    assert_eq!(drivers.take_failures().count(), 1);
    assert_eq!(drivers.take_failures().count(), 0);
}

#[test]
fn sentinel_deadlines_recheck_without_normalizing_or_dispatching() {
    let clock = TestClock(Rc::new(RefCell::new((100, 10))));
    let mut app = Mixed {
        first: TimerScheduler::new(clock.clone()),
        one: 0,
        second: TimerScheduler::new(clock.clone()),
        two: Other { fired: 0 },
        rate: RateCtrPeriodicDriver::new(clock),
        counters: RateCtrRuntime::new(),
        progress: 0,
    };
    let id = app.first.create(Some(one));
    app.first.add(&id, TimeVal::new(i64::MAX, 0)).unwrap();
    let mut driver = TimerDriver::new(|a: &mut Mixed| (&mut a.first, &mut a.one));
    assert_eq!(driver.prepare(&mut app).unwrap(), WakeHint::Recheck);
    let mut drivers = DriverSet::default();
    drivers
        .add_driver("sentinel", driver, DriverFailurePolicy::Critical)
        .unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
        .unwrap();
    assert_eq!(app.one, 0);
    assert_eq!(app.first.deadline(&id).unwrap(), TimeVal::new(i64::MAX, 0));
}

#[test]
fn monotonic_backwards_error_surfaces_even_before_rate_deadline() {
    let clock = TestClock(Rc::new(RefCell::new((100, 10))));
    let mut app = Mixed {
        first: TimerScheduler::new(clock.clone()),
        one: 0,
        second: TimerScheduler::new(clock.clone()),
        two: Other { fired: 0 },
        rate: RateCtrPeriodicDriver::new(clock.clone()),
        counters: RateCtrRuntime::new(),
        progress: 0,
    };
    app.rate.start().unwrap();
    clock.0.borrow_mut().1 = 9;
    let mut drivers = DriverSet::default();
    drivers
        .add_driver(
            "rate",
            RateDriver::new(|a: &mut Mixed| (&mut a.rate, &mut a.counters)),
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    let failure = rt
        .step(&mut app, &mut drivers, Some(Duration::ZERO))
        .unwrap_err();
    assert!(
        matches!(failure.cause, FailureCause::Driver { label: "rate", failure: DriverFailure::Component(e) } if e.downcast_ref::<osmocore_rs::rate_ctr::RateCtrError>() == Some(&osmocore_rs::rate_ctr::RateCtrError::ClockWentBackwards))
    );
}
