// SPDX-License-Identifier: GPL-3.0-or-later

use super::{RateCtrError, RateCtrRuntime};
use crate::time::{Clock, ClockId, SystemClock, TimeSpec};

/// Portable explicit monotonic polling adapter; native timerfd/select API shapes
/// and their failure quirks are outside its compatibility claim.
pub struct RateCtrPeriodicDriver<C = SystemClock> {
    pub clock: C,
    next: Option<TimeSpec>,
    last: Option<TimeSpec>,
}
impl<C: Clock> RateCtrPeriodicDriver<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            next: None,
            last: None,
        }
    }
    /// Stored monotonic deadline, without changing the rollup phase.
    pub fn next_deadline(&self) -> Option<TimeSpec> {
        self.next
    }
    pub fn is_started(&self) -> bool {
        self.next.is_some()
    }
    pub fn start(&mut self) -> Result<(), RateCtrError> {
        if self.is_started() {
            return Ok(());
        }
        let now = self.clock.clock_gettime(ClockId::MONOTONIC)?;
        now.to_duration()?;
        let next = now.checked_add(TimeSpec::new(1, 0))?;
        self.next = Some(next);
        self.last = Some(now);
        Ok(())
    }
    /// Does not reset the runtime tick phase or counters.
    pub fn stop(&mut self) {
        self.next = None;
        self.last = None;
    }
    pub fn update(&mut self, runtime: &mut RateCtrRuntime) -> Result<u64, RateCtrError> {
        let Some(next) = self.next else {
            return Ok(0);
        };
        let now = self.clock.clock_gettime(ClockId::MONOTONIC)?;
        now.to_duration()?;
        if self.last.is_some_and(|last| now < last) {
            return Err(RateCtrError::ClockWentBackwards);
        }
        if now < next {
            self.last = Some(now);
            return Ok(0);
        }
        let elapsed = now.checked_sub(next)?.to_duration()?;
        let count = elapsed
            .as_secs()
            .checked_add(1)
            .ok_or(RateCtrError::TimeOverflow)?;
        let seconds = i64::try_from(count).map_err(|_| RateCtrError::TimeOverflow)?;
        let new_next = next.checked_add(TimeSpec::new(seconds, 0))?;
        runtime.process_expirations(count)?;
        self.next = Some(new_next);
        self.last = Some(now);
        Ok(count)
    }
}
