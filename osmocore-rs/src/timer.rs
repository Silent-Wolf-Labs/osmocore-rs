// SPDX-License-Identifier: GPL-3.0-or-later

//! Owned wall-clock timer scheduling, preserving the local C batch ordering.
//!
//! Callbacks are function pointers with caller-owned context. Dispatch copies
//! the pointer and never restores it, so setup during a callback is permanent.
//! Nested updates are rejected; prepare and timer mutation remain available.
use super::time::{Clock, SystemClock, TimeError, TimeVal};
use crate::diagnostics::{event, observe};
use crate::owner::OwnerId;
use std::collections::BTreeMap;
use std::fmt;

/// Identity survives activation changes, but not release and slot reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerId {
    owner: OwnerId,
    slot: usize,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerError {
    Time(TimeError),
    InvalidHandle,
    NestedUpdate,
    GenerationExhausted,
    MillisecondsOverflow,
}
impl From<TimeError> for TimerError {
    fn from(error: TimeError) -> Self {
        Self::Time(error)
    }
}
impl fmt::Display for TimerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time(error) => error.fmt(f),
            Self::InvalidHandle => f.write_str("foreign or released timer handle"),
            Self::NestedUpdate => f.write_str("nested timer update is unsupported"),
            Self::GenerationExhausted => f.write_str("timer generation exhausted"),
            Self::MillisecondsOverflow => f.write_str("nearest milliseconds exceed signed C int"),
        }
    }
}
impl std::error::Error for TimerError {}

/// C status is -1 iff the raw difference's seconds field is negative.
/// Equality returns zero time and status 0, although update dispatches equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remaining {
    pub time: TimeVal,
    pub status: i32,
}
pub type TimerCallback<C, D> = fn(&mut TimerScheduler<C, D>, TimerId, &mut D);
struct Slot<C, D> {
    generation: u64,
    allocated: bool,
    deadline: TimeVal,
    activation: Option<u64>,
    callback: Option<TimerCallback<C, D>>,
    #[cfg(feature = "tracing")]
    context: tracing::Span,
}

/// Explicit scheduler ownership replaces C thread-local timer storage.
/// No native clock reads occur in construction. No async runtime is required.
pub struct TimerScheduler<C = SystemClock, D = ()> {
    pub clock: C,
    owner: OwnerId,
    slots: Vec<Slot<C, D>>,
    active: BTreeMap<(TimeVal, u64), usize>,
    sequence: u64,
    nearest: Option<TimeVal>,
    dispatching: bool,
    /// Reused by `update`, so expiring timers doesn't allocate on every call. Empty
    /// outside `update`.
    batch: Vec<(usize, u64, u64)>,
}
impl<C, D> TimerScheduler<C, D> {
    #[cfg(feature = "tracing")]
    fn diagnostic_context(&self, id: Option<&TimerId>) -> crate::diagnostics::TimerContext {
        let valid = id.filter(|id| self.index(id).is_ok());
        crate::diagnostics::TimerContext {
            timer_slot: valid.map(|id| id.slot),
            timer_generation: valid.map(|id| id.generation),
        }
    }
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            owner: OwnerId::next(),
            slots: Vec::new(),
            active: BTreeMap::new(),
            sequence: 0,
            nearest: None,
            dispatching: false,
            batch: Vec::new(),
        }
    }
    pub fn create(&mut self, callback: Option<TimerCallback<C, D>>) -> TimerId {
        let slot = self
            .slots
            .iter()
            .position(|s| !s.allocated)
            .unwrap_or(self.slots.len());
        if slot == self.slots.len() {
            self.slots.push(Slot {
                generation: 0,
                allocated: false,
                deadline: TimeVal::default(),
                activation: None,
                callback: None,
                #[cfg(feature = "tracing")]
                context: tracing::Span::none(),
            });
        }
        let entry = &mut self.slots[slot];
        entry.allocated = true;
        entry.deadline = TimeVal::default();
        entry.callback = callback;
        TimerId {
            owner: self.owner,
            slot,
            generation: entry.generation,
        }
    }
    fn index(&self, id: &TimerId) -> Result<usize, TimerError> {
        if self.owner == id.owner
            && let Some(slot) = self.slots.get(id.slot)
            && slot.allocated
            && slot.generation == id.generation
        {
            return Ok(id.slot);
        }
        Err(TimerError::InvalidHandle)
    }
    fn deactivate(&mut self, slot: usize) {
        let entry = &mut self.slots[slot];
        #[cfg(feature = "tracing")]
        {
            entry.context = tracing::Span::none();
        }
        if let Some(activation) = entry.activation.take() {
            self.active.remove(&(entry.deadline, activation));
        }
    }
    /// Replace only the callback; the deadline and activation are untouched.
    pub fn setup(
        &mut self,
        id: &TimerId,
        callback: Option<TimerCallback<C, D>>,
    ) -> Result<(), TimerError> {
        observe!(self, Some(id), "setup", diagnostics, {
            let slot = self.index(id)?;
            self.slots[slot].callback = callback;
            Ok(())
        })
    }
    /// Set a raw absolute deadline and (re)activate, without a clock read.
    pub fn add(&mut self, id: &TimerId, deadline: TimeVal) -> Result<(), TimerError> {
        observe!(self, Some(id), "arm", diagnostics, {
            let slot = self.index(id)?;
            let activation = self
                .sequence
                .checked_add(1)
                .ok_or(TimerError::GenerationExhausted)?;
            self.deactivate(slot);
            self.sequence = activation;
            self.slots[slot].deadline = deadline;
            self.slots[slot].activation = Some(activation);
            self.active.insert((deadline, activation), slot);
            #[cfg(feature = "tracing")]
            {
                self.slots[slot].context = tracing::Span::current();
            }
            event!(
                debug,
                "osmocore_rs::timer",
                operation = "arm",
                timer_slot = slot,
                timer_generation = id.generation,
                deadline_seconds = deadline.seconds,
                deadline_microseconds = deadline.microseconds
            );
            Ok(())
        })
    }
    /// Idempotent cancellation retains the deadline and callback.
    pub fn delete(&mut self, id: &TimerId) -> Result<(), TimerError> {
        observe!(self, Some(id), "cancel", diagnostics, {
            let slot = self.index(id)?;
            #[cfg(feature = "tracing")]
            let was_pending = self.slots[slot].activation.is_some();
            self.deactivate(slot);
            event!(
                debug,
                "osmocore_rs::timer",
                operation = "cancel",
                timer_slot = slot,
                timer_generation = id.generation,
                was_pending
            );
            Ok(())
        })
    }
    /// Release invalidates handles independently of activation/batch versions.
    pub fn release(&mut self, id: &TimerId) -> Result<(), TimerError> {
        observe!(self, Some(id), "release", diagnostics, {
            let slot = self.index(id)?;
            let generation = self.slots[slot]
                .generation
                .checked_add(1)
                .ok_or(TimerError::GenerationExhausted)?;
            self.deactivate(slot);
            self.slots[slot].generation = generation;
            self.slots[slot].allocated = false;
            self.slots[slot].callback = None;
            event!(
                debug,
                "osmocore_rs::timer",
                operation = "release",
                timer_slot = slot,
                timer_generation = id.generation
            );
            Ok(())
        })
    }
    pub fn pending(&self, id: &TimerId) -> Result<bool, TimerError> {
        Ok(self.slots[self.index(id)?].activation.is_some())
    }
    pub fn deadline(&self, id: &TimerId) -> Result<TimeVal, TimerError> {
        Ok(self.slots[self.index(id)?].deadline)
    }
    pub fn check(&self) -> usize {
        self.active.len()
    }
    /// Return the cached value; only prepare refreshes it.
    pub fn nearest(&self) -> Option<TimeVal> {
        self.nearest
    }
    /// Nonembedded C rounding; unrepresentable C-int results become errors.
    pub fn nearest_ms(&self) -> Result<i32, TimerError> {
        let Some(time) = self.nearest else {
            return Ok(-1);
        };
        let ms = time
            .seconds
            .checked_mul(1000)
            .and_then(|s| {
                time.microseconds
                    .checked_add(999)
                    .and_then(|f| s.checked_add(f / 1000))
            })
            .ok_or(TimerError::MillisecondsOverflow)?;
        i32::try_from(ms).map_err(|_| TimerError::MillisecondsOverflow)
    }
}
impl<C: Clock, D> TimerScheduler<C, D> {
    /// C relative arguments are signed 32-bit ints; arithmetic retains raw fields.
    pub fn schedule(
        &mut self,
        id: &TimerId,
        seconds: i32,
        microseconds: i32,
    ) -> Result<(), TimerError> {
        let deadline = observe!(self, Some(id), "schedule", diagnostics, {
            self.index(id)?;
            let now = self.clock.gettimeofday(None)?;
            Ok::<_, TimerError>(
                TimeVal::new(i64::from(seconds), i64::from(microseconds)).checked_add(now)?,
            )
        })?;
        self.add(id, deadline)
    }
    pub fn remaining(
        &mut self,
        id: &TimerId,
        now: Option<TimeVal>,
    ) -> Result<Remaining, TimerError> {
        let deadline = self.deadline(id)?;
        let now = match now {
            Some(now) => now,
            None => self.clock.gettimeofday(None)?,
        };
        let time = deadline.checked_sub(now)?;
        Ok(Remaining {
            time,
            status: if time.seconds < 0 { -1 } else { 0 },
        })
    }
    pub fn prepare(&mut self) -> Result<(), TimerError> {
        let now = self.clock.gettimeofday(None)?;
        let nearest = match self.active.keys().next() {
            // LONG_MAX is special only in C's nearest-cache calculation.
            Some((deadline, _)) if deadline.seconds != i64::MAX => Some(if *deadline > now {
                deadline.checked_sub(now)?
            } else {
                TimeVal::default()
            }),
            _ => None,
        };
        self.nearest = nearest;
        Ok(())
    }
    /// Return 0/1 work, not callback count. Selected timers stay active until
    /// dispatch, preserving every undispatched timer if a callback panics.
    /// The panic propagates after the guard is reset; callback mutations persist.
    pub fn update(&mut self, context: &mut D) -> Result<i32, TimerError> {
        observe!(self, None, "update", diagnostics, {
            if self.dispatching {
                return Err(TimerError::NestedUpdate);
            }
            let now = self.clock.gettimeofday(None)?;
            if !matches!(self.active.first_key_value(), Some(((deadline, _), _)) if *deadline <= now)
            {
                return Ok(0);
            }
            let mut batch = std::mem::take(&mut self.batch);
            batch.extend(
                self.active
                    .iter()
                    .take_while(|((deadline, _), _)| *deadline <= now)
                    .map(|((_, activation), slot)| {
                        (*slot, self.slots[*slot].generation, *activation)
                    }),
            );
            self.dispatching = true;
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut work = 0;
                for &(slot, generation, activation) in batch.iter().rev() {
                    let entry = &self.slots[slot];
                    if !entry.allocated
                        || entry.generation != generation
                        || entry.activation != Some(activation)
                    {
                        continue;
                    }
                    let callback = entry.callback;
                    #[cfg(feature = "tracing")]
                    let deadline = entry.deadline;
                    #[cfg(feature = "tracing")]
                    let activation_context = entry.context.clone();
                    let id = TimerId {
                        owner: self.owner,
                        slot,
                        generation,
                    };
                    self.deactivate(slot);
                    work = 1;
                    // The old activation is detached before a callback can rearm it.
                    #[cfg(feature = "tracing")]
                    let _context_guard = activation_context.enter();
                    #[cfg(feature = "tracing")]
                    let span = tracing::trace_span!(target: "osmocore_rs::timer", "timer_callback",
                    operation = "callback", timer_slot = slot, timer_generation = generation);
                    #[cfg(feature = "tracing")]
                    let _span_guard = span.enter();
                    event!(
                        debug,
                        "osmocore_rs::timer",
                        operation = "expire",
                        timer_slot = slot,
                        timer_generation = generation,
                        deadline_seconds = deadline.seconds,
                        deadline_microseconds = deadline.microseconds,
                        now_seconds = now.seconds,
                        now_microseconds = now.microseconds
                    );
                    if let Some(callback) = callback {
                        callback(self, id, context);
                    }
                }
                work
            }));
            self.dispatching = false;
            batch.clear();
            self.batch = batch;
            match outcome {
                Ok(work) => Ok(work),
                Err(panic) => {
                    event!(
                        error,
                        "osmocore_rs::timer",
                        operation = "update",
                        error_category = "callback_panic"
                    );
                    std::panic::resume_unwind(panic)
                }
            }
        })
    }
}

pub(crate) mod legacy {
    use super::*;

    pub fn osmo_timer_pending<C, D>(
        scheduler: &TimerScheduler<C, D>,
        id: &TimerId,
    ) -> Result<i32, TimerError> {
        scheduler.pending(id).map(i32::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{ClockId, TimeSpec, TimeZone};

    struct FixedClock;

    impl Clock for FixedClock {
        fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
            Ok(TimeVal::new(1_000, 0))
        }
        fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
            unreachable!("timers use wall time")
        }
    }

    type Scheduler = TimerScheduler<FixedClock, Vec<usize>>;

    fn record(_: &mut Scheduler, id: TimerId, fired: &mut Vec<usize>) {
        fired.push(id.slot);
    }

    fn record_then_panic(_: &mut Scheduler, id: TimerId, fired: &mut Vec<usize>) {
        fired.push(id.slot);
        std::panic::panic_any("callback panic");
    }

    fn arm(scheduler: &mut Scheduler, id: &TimerId) {
        scheduler.add(id, TimeVal::new(1_000, 0)).unwrap();
    }

    #[test]
    fn batch_buffer_is_kept_and_empty_between_updates() {
        let mut scheduler = Scheduler::new(FixedClock);
        let ids: Vec<_> = (0..8).map(|_| scheduler.create(Some(record))).collect();
        ids.iter().for_each(|id| arm(&mut scheduler, id));

        let mut fired = Vec::new();
        assert_eq!(scheduler.update(&mut fired), Ok(1));
        assert_eq!(fired.len(), 8);
        assert!(scheduler.batch.is_empty());
        let capacity = scheduler.batch.capacity();
        assert!(capacity >= 8);

        // Nothing is due, so nothing fires again and the buffer stays.
        assert_eq!(scheduler.update(&mut fired), Ok(0));
        assert_eq!(fired.len(), 8);
        assert_eq!(scheduler.batch.capacity(), capacity);

        ids.iter().for_each(|id| arm(&mut scheduler, id));
        assert_eq!(scheduler.update(&mut fired), Ok(1));
        assert_eq!(fired.len(), 16);
        assert_eq!(scheduler.batch.capacity(), capacity);
    }

    #[test]
    fn panicking_callback_leaves_buffer_empty_and_each_timer_fires_once() {
        let mut scheduler = Scheduler::new(FixedClock);
        let ids: Vec<_> = (0..4).map(|_| scheduler.create(Some(record))).collect();
        scheduler.setup(&ids[2], Some(record_then_panic)).unwrap();
        ids.iter().for_each(|id| arm(&mut scheduler, id));

        // Dispatch runs in reverse, so slot 3 fires and slot 2 panics.
        let mut fired = Vec::new();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            scheduler.update(&mut fired)
        }));
        assert!(outcome.is_err());
        assert_eq!(fired, [3, 2]);
        assert!(scheduler.batch.is_empty());
        assert!(scheduler.batch.capacity() >= 4);

        assert_eq!(scheduler.update(&mut fired), Ok(1));
        assert_eq!(fired, [3, 2, 1, 0]);
        assert!(scheduler.batch.is_empty());
        assert_eq!(scheduler.update(&mut fired), Ok(0));
        assert_eq!(fired, [3, 2, 1, 0]);
    }
}
