// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{marker::PhantomData, rc::Rc, time::Duration};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WakeHint {
    Idle,
    After(Duration),
    Recheck,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverProgress {
    Idle,
    Progress,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverFailurePolicy {
    Critical,
    Optional,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverRegistrationError {
    Capacity,
}
pub trait RuntimeDriver<A: Application> {
    fn prepare(&mut self, app: &mut A) -> Result<WakeHint, DriverFailure<A::Error>>;
    fn drive(
        &mut self,
        app: &mut A,
        cx: &mut RuntimeContext<'_>,
    ) -> Result<DriverProgress, DriverFailure<A::Error>>;
}
pub(crate) struct Entry<'a, A: Application> {
    pub label: &'static str,
    pub driver: Box<dyn RuntimeDriver<A> + 'a>,
    pub policy: DriverFailurePolicy,
    pub disabled: bool,
    pub failure: Option<DriverFailure<A::Error>>,
}
pub struct DriverSet<'a, A: Application> {
    pub(crate) entries: Vec<Entry<'a, A>>,
    pub(crate) cursor: usize,
    limit: usize,
    _local: PhantomData<Rc<()>>,
}
impl<'a, A: Application> DriverSet<'a, A> {
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            limit,
            _local: PhantomData,
        }
    }
    /// Consume preserved optional failures. At most one is stored per entry.
    pub fn take_failures(
        &mut self,
    ) -> impl Iterator<Item = (&'static str, DriverFailure<A::Error>)> + '_ {
        self.entries
            .iter_mut()
            .filter_map(|e| e.failure.take().map(|failure| (e.label, failure)))
    }
    pub fn add_driver<D: RuntimeDriver<A> + 'a>(
        &mut self,
        label: &'static str,
        driver: D,
        policy: DriverFailurePolicy,
    ) -> Result<(), DriverRegistrationError> {
        if self.entries.len() >= self.limit {
            return Err(DriverRegistrationError::Capacity);
        }
        self.entries.push(Entry {
            label,
            driver: Box::new(driver),
            policy,
            disabled: false,
            failure: None,
        });
        Ok(())
    }
}
impl<A: Application> Default for DriverSet<'_, A> {
    fn default() -> Self {
        Self::new(32)
    }
}

fn timer_hint<C: crate::time::Clock, D, E>(
    scheduler: &mut crate::timer::TimerScheduler<C, D>,
) -> Result<WakeHint, DriverFailure<E>> {
    scheduler
        .prepare()
        .map_err(|e| DriverFailure::Component(Box::new(e)))?;
    Ok(match scheduler.nearest() {
        None if scheduler.check() == 0 => WakeHint::Idle,
        None => WakeHint::Recheck,
        Some(t) => match t.to_duration() {
            Ok(d) => WakeHint::After(d),
            Err(_) => WakeHint::Recheck,
        },
    })
}
fn progress(n: i32) -> DriverProgress {
    if n == 0 {
        DriverProgress::Idle
    } else {
        DriverProgress::Progress
    }
}
/// Typed projections borrow application fields only for one operation.
pub struct TimerDriver<C, D, A> {
    projection: fn(&mut A) -> (&mut crate::timer::TimerScheduler<C, D>, &mut D),
}
impl<C, D, A> TimerDriver<C, D, A> {
    pub fn new(
        projection: fn(&mut A) -> (&mut crate::timer::TimerScheduler<C, D>, &mut D),
    ) -> Self {
        Self { projection }
    }
}
impl<C: crate::time::Clock, D, A: Application> RuntimeDriver<A> for TimerDriver<C, D, A> {
    fn prepare(&mut self, app: &mut A) -> Result<WakeHint, DriverFailure<A::Error>> {
        timer_hint((self.projection)(app).0)
    }
    fn drive(
        &mut self,
        app: &mut A,
        _: &mut RuntimeContext<'_>,
    ) -> Result<DriverProgress, DriverFailure<A::Error>> {
        let (s, d) = (self.projection)(app);
        s.update(d)
            .map(progress)
            .map_err(|e| DriverFailure::Component(Box::new(e)))
    }
}
type FsmPair<C, D, E> = (
    crate::timer::TimerScheduler<C, crate::fsm::FsmRuntime<C, D, E>>,
    crate::fsm::FsmRuntime<C, D, E>,
);
pub struct FsmDriver<C, D, E, A> {
    projection: fn(&mut A) -> &mut FsmPair<C, D, E>,
}
impl<C, D, E, A> FsmDriver<C, D, E, A> {
    pub fn new(projection: fn(&mut A) -> &mut FsmPair<C, D, E>) -> Self {
        Self { projection }
    }
}
impl<C: crate::time::Clock + 'static, D: 'static, E: 'static, A: Application> RuntimeDriver<A>
    for FsmDriver<C, D, E, A>
{
    fn prepare(&mut self, app: &mut A) -> Result<WakeHint, DriverFailure<A::Error>> {
        timer_hint(&mut (self.projection)(app).0)
    }
    fn drive(
        &mut self,
        app: &mut A,
        _: &mut RuntimeContext<'_>,
    ) -> Result<DriverProgress, DriverFailure<A::Error>> {
        let (s, f) = (self.projection)(app);
        f.update(s)
            .map(progress)
            .map_err(|e| DriverFailure::Component(Box::new(e)))
    }
}
type AccountingPair<'d, C, S> = (
    crate::timer::TimerScheduler<C, crate::time_accounting::TimeCc<'d, S>>,
    crate::time_accounting::TimeCc<'d, S>,
);
pub struct AccountingDriver<'d, C, S, A> {
    projection: fn(&mut A) -> &mut AccountingPair<'d, C, S>,
}
impl<'d, C, S, A> AccountingDriver<'d, C, S, A> {
    pub fn new(projection: fn(&mut A) -> &mut AccountingPair<'d, C, S>) -> Self {
        Self { projection }
    }
}
impl<C: crate::time::Clock, S: crate::time_accounting::CounterSink, A: Application> RuntimeDriver<A>
    for AccountingDriver<'_, C, S, A>
{
    fn prepare(&mut self, app: &mut A) -> Result<WakeHint, DriverFailure<A::Error>> {
        timer_hint(&mut (self.projection)(app).0)
    }
    fn drive(
        &mut self,
        app: &mut A,
        _: &mut RuntimeContext<'_>,
    ) -> Result<DriverProgress, DriverFailure<A::Error>> {
        let (s, cc) = (self.projection)(app);
        cc.update(s)
            .map(progress)
            .map_err(|e| DriverFailure::Component(Box::new(e)))
    }
}
pub struct RateDriver<C, A> {
    projection: fn(
        &mut A,
    ) -> (
        &mut crate::rate_ctr::RateCtrPeriodicDriver<C>,
        &mut crate::rate_ctr::RateCtrRuntime,
    ),
}
impl<C, A> RateDriver<C, A> {
    pub fn new(
        projection: fn(
            &mut A,
        ) -> (
            &mut crate::rate_ctr::RateCtrPeriodicDriver<C>,
            &mut crate::rate_ctr::RateCtrRuntime,
        ),
    ) -> Self {
        Self { projection }
    }
}
impl<C: crate::time::Clock, A: Application> RuntimeDriver<A> for RateDriver<C, A> {
    fn prepare(&mut self, app: &mut A) -> Result<WakeHint, DriverFailure<A::Error>> {
        let (d, _) = (self.projection)(app);
        let Some(next) = d.next_deadline() else {
            return Ok(WakeHint::Idle);
        };
        let now = d
            .clock
            .clock_gettime(crate::time::ClockId::MONOTONIC)
            .map_err(DriverFailure::Clock)?;
        now.to_duration().map_err(DriverFailure::Clock)?;
        let wait = if next <= now {
            Duration::ZERO
        } else {
            next.checked_sub(now)
                .and_then(|t| t.to_duration())
                .map_err(DriverFailure::Clock)?
        };
        Ok(WakeHint::After(wait))
    }
    fn drive(
        &mut self,
        app: &mut A,
        _: &mut RuntimeContext<'_>,
    ) -> Result<DriverProgress, DriverFailure<A::Error>> {
        let (d, c) = (self.projection)(app);
        d.update(c)
            .map(|n| {
                if n == 0 {
                    DriverProgress::Idle
                } else {
                    DriverProgress::Progress
                }
            })
            .map_err(|e| DriverFailure::Component(Box::new(e)))
    }
}
