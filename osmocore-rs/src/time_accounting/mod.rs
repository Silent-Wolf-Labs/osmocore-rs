// SPDX-License-Identifier: GPL-3.0-or-later

//! Event-driven cumulative monotonic accounting with wall-clock timer wakeups.
//!
//! See the [accounting guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/src/time_accounting/README.md)
//! for ownership, configuration, clock roles, reporting and error boundaries.

mod policy;
mod state;
#[cfg(test)]
mod tests;

use crate::tdef::{TDef, TDefError};
use crate::time::{Clock, ClockId};
use crate::timer::{TimerError, TimerId, TimerScheduler};
use policy::AccountingPolicy;
use state::{AccountingEffects, AccountingEvent};
use std::fmt;

/// Receives positive increments capped at the recorded C INT_MAX per report.
/// Implementations may route increments to an application's existing counters.
pub trait CounterSink {
    fn add(&mut self, increment: i32);
}
impl CounterSink for u64 {
    fn add(&mut self, increment: i32) {
        *self = self.wrapping_add(increment as u64);
    }
}

/// Mutable configuration; zero granularity means one second, zero threshold
/// means half the effective granularity, zero forgetting means keep surplus.
/// TDef storage is owned so callers can edit current values between events.
#[derive(Debug)]
pub struct TimeCcConfig<'d, S = u64> {
    pub gran_usec: u64,
    pub round_threshold_usec: u64,
    pub forget_sum_usec: u64,
    pub rate_ctr: Option<S>,
    pub t_gran: i32,
    pub t_round_threshold: i32,
    pub t_forget_sum: i32,
    pub t_defs: Option<Vec<TDef<'d>>>,
}
impl<S> Default for TimeCcConfig<'_, S> {
    fn default() -> Self {
        Self {
            gran_usec: 0,
            round_threshold_usec: 0,
            forget_sum_usec: 0,
            rate_ctr: None,
            t_gran: 0,
            t_round_threshold: 0,
            t_forget_sum: 0,
            t_defs: None,
        }
    }
}
impl<S> TimeCcConfig<'_, S> {
    pub fn granularity(&self) -> u64 {
        if self.gran_usec == 0 {
            1_000_000
        } else {
            self.gran_usec
        }
    }
    pub fn threshold(&self) -> u64 {
        if self.round_threshold_usec == 0 {
            self.granularity() / 2
        } else {
            self.round_threshold_usec.min(self.granularity())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeCcError {
    Timer(TimerError),
    TDef(TDefError),
}
impl From<TimerError> for TimeCcError {
    fn from(e: TimerError) -> Self {
        Self::Timer(e)
    }
}
impl From<TDefError> for TimeCcError {
    fn from(e: TDefError) -> Self {
        Self::TDef(e)
    }
}
impl fmt::Display for TimeCcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timer(e) => e.fmt(f),
            Self::TDef(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for TimeCcError {}

/// Public accounting state corresponds to C storage and supports inspection.
/// Direct edits are possible, but callers normally use set_flag and update.
#[derive(Debug)]
pub struct TimeCc<'d, S = u64> {
    pub cfg: TimeCcConfig<'d, S>,
    pub flag_state: bool,
    pub total_sum: u64,
    pub start_time: u64,
    pub last_counted_time: u64,
    pub sum: u64,
    pub reported_sum: u64,
    timer: Option<TimerId>,
    callback_error: Option<TimeCcError>,
}
impl<S> Default for TimeCc<'_, S> {
    fn default() -> Self {
        Self {
            cfg: TimeCcConfig::default(),
            flag_state: false,
            total_sum: 0,
            start_time: 0,
            last_counted_time: 0,
            sum: 0,
            reported_sum: 0,
            timer: None,
            callback_error: None,
        }
    }
}
impl<'d, S> TimeCc<'d, S> {
    /// Construction is init: no clock reads or scheduler mutation.
    pub fn new() -> Self {
        Self::default()
    }
    pub fn timer(&self) -> Option<&TimerId> {
        self.timer.as_ref()
    }
    /// Raw scheduler users must inspect this after dispatch. The update wrapper
    /// consumes and returns it automatically; callbacks never turn errors into a panic.
    pub fn take_callback_error(&mut self) -> Option<TimeCcError> {
        self.callback_error.take()
    }

    /// Cancel/release owned timer and reset accounting, preserving configuration
    /// and the sink's accumulated counter. Cleanup is idempotent and permits reuse.
    pub fn cleanup<C>(
        &mut self,
        scheduler: &mut TimerScheduler<C, Self>,
    ) -> Result<(), TimeCcError> {
        if let Some(timer) = &self.timer {
            scheduler.release(timer)?;
        }
        self.timer = None;
        self.flag_state = false;
        self.total_sum = 0;
        self.start_time = 0;
        self.last_counted_time = 0;
        self.sum = 0;
        self.reported_sum = 0;
        self.callback_error = None;
        Ok(())
    }
}
fn time_now_usec(clock: &mut impl Clock) -> u64 {
    match clock.clock_gettime(ClockId::MONOTONIC) {
        Err(_) => 0,
        Ok(time) => (time.seconds as u64)
            .wrapping_mul(1_000_000)
            .wrapping_add((time.nanoseconds / 1_000) as u64),
    }
}
impl<'d, S: CounterSink> TimeCc<'d, S> {
    // Commit reporting after the sink call, preserving sink-panic behavior.
    fn apply_accounting(&mut self, effects: AccountingEffects) {
        if let Some(report) = effects.report {
            if let Some(sink) = &mut self.cfg.rate_ctr {
                sink.add(report.increment);
            }
            self.reported_sum = self.reported_sum.wrapping_add(report.accounted_usec);
        }
        if let Some(flag) = effects.flag_after_report {
            self.flag_state = flag;
        }
    }

    fn observe_policy(&mut self, now: u64) -> Result<AccountingPolicy, TimeCcError> {
        let resolution = self.cfg.resolve_policy()?;
        if resolution.discard_pending && self.sum != 0 {
            self.discard_pending(now);
        }
        Ok(resolution.policy)
    }

    // This adapter accepts a monotonic deadline. None deliberately leaves an
    // existing timer alone, including a final callback after becoming inactive.
    fn arm_wakeup<C: Clock>(
        &mut self,
        scheduler: &mut TimerScheduler<C, Self>,
        now: u64,
        wakeup_at: Option<u64>,
    ) -> Result<(), TimeCcError> {
        let Some(deadline) = wakeup_at else {
            return Ok(());
        };
        let delay = deadline.saturating_sub(now);
        if self.timer.is_none() {
            self.timer = Some(scheduler.create(Some(Self::timer_callback::<C>)));
        }
        let timer = self.timer.as_ref().unwrap();
        scheduler.setup(timer, Some(Self::timer_callback::<C>))?;
        scheduler.delete(timer)?;
        // GCC on the recorded ABI narrows uint64 seconds modulo 2^32 to int.
        scheduler.schedule(
            timer,
            (delay / 1_000_000) as i32,
            (delay % 1_000_000) as i32,
        )?;
        Ok(())
    }

    fn process_event<C: Clock>(
        &mut self,
        scheduler: &mut TimerScheduler<C, Self>,
        event: AccountingEvent,
    ) -> Result<(), TimeCcError> {
        let now = time_now_usec(&mut scheduler.clock);
        // Each event has its own policy observation boundary. In particular,
        // flag changes report under current config before resolving TDefs.
        let policy = match event {
            AccountingEvent::FlagChanged(flag) => {
                if self.start_time == 0 {
                    self.cleanup(scheduler)?;
                    self.start_time = now;
                    self.last_counted_time = now;
                    self.observe_policy(now)?;
                    let policy = self.observe_policy(now)?;
                    self.arm_wakeup(scheduler, now, self.wakeup_at(now, policy))?;
                }
                if flag == self.flag_state {
                    return Ok(());
                }
                AccountingPolicy::from_config(&self.cfg)
            }
            AccountingEvent::Wakeup => self.observe_policy(now)?,
        };
        let effects = self.transition(now, event, policy);
        self.apply_accounting(effects);
        // This resolution can fail after reporting. Never roll back that prefix.
        let policy = self.observe_policy(now)?;
        self.arm_wakeup(scheduler, now, self.wakeup_at(now, policy))
    }

    fn timer_callback<C: Clock>(
        scheduler: &mut TimerScheduler<C, Self>,
        timer: TimerId,
        tc: &mut Self,
    ) {
        if tc.timer.as_ref() != Some(&timer) {
            tc.callback_error = Some(TimeCcError::Timer(TimerError::InvalidHandle));
            return;
        }
        if let Err(error) = tc.process_event(scheduler, AccountingEvent::Wakeup) {
            tc.callback_error = Some(error);
        }
    }

    pub fn set_flag<C: Clock>(
        &mut self,
        scheduler: &mut TimerScheduler<C, Self>,
        flag: bool,
    ) -> Result<(), TimeCcError> {
        if let Some(timer) = &self.timer {
            scheduler.pending(timer)?;
        }
        self.process_event(scheduler, AccountingEvent::FlagChanged(flag))
    }
    /// Dispatch through the paired scheduler and surface any callback error.
    /// Callers may use the scheduler directly if they consume callback errors.
    pub fn update<C: Clock>(
        &mut self,
        scheduler: &mut TimerScheduler<C, Self>,
    ) -> Result<i32, TimeCcError> {
        if let Some(timer) = &self.timer {
            scheduler.pending(timer)?;
        }
        if let Some(error) = self.take_callback_error() {
            return Err(error);
        }
        let work = scheduler.update(self)?;
        if let Some(error) = self.take_callback_error() {
            return Err(error);
        }
        Ok(work)
    }
}
