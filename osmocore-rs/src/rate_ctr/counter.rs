// SPDX-License-Identifier: GPL-3.0-or-later

//! Rate counters, owned groups and periodic rollups on the recorded Linux ABI.
//!
//! Addition and subtraction wrap modulo 2^64. Difference reproduces the
//! recorded GCC unsigned-to-signed conversion, then updates `previous`.
//! Primitive operations preserve interval storage; owned runtimes calculate
//! rollups and replace the C process-global registry. Native select/timerfd
//! APIs remain outside the conversion boundary.

use crate::time_accounting::CounterSink;

pub const RATE_CTR_INTV_NUM: usize = 4;
pub const RATE_CTR_INTV_SEC: usize = 0;
pub const RATE_CTR_INTV_MIN: usize = 1;
pub const RATE_CTR_INTV_HOUR: usize = 2;
pub const RATE_CTR_INTV_DAY: usize = 3;

/// Stored interval state, updated explicitly by an owned rate-counter runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RateCtrPerIntv {
    pub last: u64,
    pub rate: u64,
}

/// Owned counter storage corresponding to C `struct rate_ctr`.
/// Rust references exclude the null-pointer calls diagnosed in C.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RateCtr {
    pub current: u64,
    pub previous: u64,
    pub intv: [RateCtrPerIntv; RATE_CTR_INTV_NUM],
}
impl RateCtr {
    /// Add a signed C-int increment, preserving all other state.
    pub fn add(&mut self, increment: i32) {
        self.current = self.current.wrapping_add(increment as u64);
    }

    pub fn inc(&mut self) {
        self.add(1);
    }

    /// Return the wrapping delta with the recorded signed interpretation.
    pub fn difference(&mut self) -> i64 {
        let result = self.current.wrapping_sub(self.previous) as i64;
        self.previous = self.current;
        result
    }

    /// Clear current, previous and every interval field.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
impl CounterSink for RateCtr {
    fn add(&mut self, increment: i32) {
        RateCtr::add(self, increment);
    }
}
