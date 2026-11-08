// SPDX-License-Identifier: GPL-3.0-or-later

use super::{TimeCc, policy::AccountingPolicy};

#[derive(Clone, Copy, Debug)]
pub(super) enum AccountingEvent {
    FlagChanged(bool),
    Wakeup,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CounterReport {
    pub increment: i32,
    pub accounted_usec: u64,
}

/// Ordered completion of a transition: report to the sink, commit represented
/// time, then change the flag. Deadline evaluation follows policy resolution.
#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct AccountingEffects {
    pub report: Option<CounterReport>,
    pub flag_after_report: Option<bool>,
}

// Directly mutate authoritative accounting fields; no scheduler or sink calls.
impl<S> TimeCc<'_, S> {
    pub(super) fn discard_pending(&mut self, now: u64) {
        self.sum = 0;
        self.reported_sum = 0;
        self.last_counted_time = self.last_counted_time.max(now);
    }

    pub(super) fn transition(
        &mut self,
        now: u64,
        event: AccountingEvent,
        policy: AccountingPolicy,
    ) -> AccountingEffects {
        let flag_after_report = match event {
            AccountingEvent::FlagChanged(flag) if flag == self.flag_state => {
                return AccountingEffects::default();
            }
            AccountingEvent::FlagChanged(flag) => Some(flag),
            AccountingEvent::Wakeup if !self.flag_state => {
                if self.sum != 0
                    && policy.forget_after.is_some_and(|duration| {
                        now >= self.last_counted_time.wrapping_add(duration)
                    })
                {
                    self.discard_pending(now);
                }
                return AccountingEffects::default();
            }
            AccountingEvent::Wakeup => None,
        };

        let elapsed = now.wrapping_sub(self.last_counted_time);
        self.last_counted_time = now;
        if self.flag_state {
            self.total_sum = self.total_sum.wrapping_add(elapsed);
            self.sum = self.sum.wrapping_add(elapsed);
        }

        let report = self
            .sum
            .checked_sub(self.reported_sum)
            .and_then(|unreported| {
                let units = (unreported / policy.granularity)
                    .wrapping_add(u64::from(
                        unreported % policy.granularity >= policy.threshold,
                    ))
                    .min(i32::MAX as u64);
                (units != 0).then_some(CounterReport {
                    increment: units as i32,
                    accounted_usec: units.wrapping_mul(policy.granularity),
                })
            });
        AccountingEffects {
            report,
            flag_after_report,
        }
    }

    /// Monotonic deadline data, independent of wall time and timer storage.
    pub(super) fn wakeup_at(&self, now: u64, policy: AccountingPolicy) -> Option<u64> {
        let deadline = if self.flag_state {
            Some(
                now.wrapping_add(self.reported_sum.wrapping_sub(self.sum))
                    .wrapping_add(policy.threshold),
            )
        } else if self.sum != 0 {
            policy
                .forget_after
                .map(|duration| self.last_counted_time.wrapping_add(duration))
        } else {
            None
        };
        // The recorded contract treats this particular calculated deadline as
        // no event. Internally absence is Option, never a sentinel accumulator.
        deadline.filter(|deadline| *deadline != u64::MAX)
    }
}
