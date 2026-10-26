// SPDX-License-Identifier: GPL-3.0-or-later

//! Raw-preserving time arithmetic and injectable clock shims.
//!
//! Addition carries once and subtraction borrows once, even for abnormal
//! fractional fields. Ordering compares fields, without normalization.
//! Overrides are owned by the caller instead of unsynchronized C globals.
use std::fmt;
use std::time::Duration;

/// Errors replacing signed C overflow and system-call errno results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeError {
    ArithmeticOverflow,
    InvalidFraction,
    NegativeDuration,
    SystemError(i32),
    UnsupportedPlatform,
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArithmeticOverflow => f.write_str("time arithmetic overflow"),
            Self::InvalidFraction => f.write_str("fractional time field is not canonical"),
            Self::NegativeDuration => f.write_str("negative time cannot be a Duration"),
            Self::SystemError(errno) => write!(f, "clock system call failed with errno {errno}"),
            Self::UnsupportedPlatform => f.write_str("native clocks require 64-bit Linux"),
        }
    }
}
impl std::error::Error for TimeError {}

fn arithmetic(
    a: (i64, i64),
    b: (i64, i64),
    base: i64,
    subtract: bool,
) -> Result<(i64, i64), TimeError> {
    let checked = |x: i64, y: i64| {
        if subtract {
            x.checked_sub(y)
        } else {
            x.checked_add(y)
        }
        .ok_or(TimeError::ArithmeticOverflow)
    };
    let mut seconds = checked(a.0, b.0)?;
    let mut fraction = checked(a.1, b.1)?;
    if subtract && fraction < 0 {
        seconds = seconds
            .checked_sub(1)
            .ok_or(TimeError::ArithmeticOverflow)?;
        fraction = fraction
            .checked_add(base)
            .ok_or(TimeError::ArithmeticOverflow)?;
    } else if !subtract && fraction >= base {
        seconds = seconds
            .checked_add(1)
            .ok_or(TimeError::ArithmeticOverflow)?;
        fraction -= base;
    }
    Ok((seconds, fraction))
}

macro_rules! raw_time {
    ($name:ident, $field:ident, $base:expr, $nanos:expr) => {
        /// Signed raw time fields on the selected 64-bit C ABI.
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            pub seconds: i64,
            pub $field: i64,
        }
        impl $name {
            pub const fn new(seconds: i64, $field: i64) -> Self {
                Self { seconds, $field }
            }
            pub const fn is_set(self) -> bool {
                self.seconds != 0 || self.$field != 0
            }
            pub fn clear(&mut self) {
                *self = Self::default();
            }
            /// Preserve C's single carry, including noncanonical results.
            pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
                let (seconds, fraction) = arithmetic(
                    (self.seconds, self.$field),
                    (other.seconds, other.$field),
                    $base,
                    false,
                )?;
                Ok(Self::new(seconds, fraction))
            }
            /// Preserve C's single borrow, including noncanonical results.
            pub fn checked_sub(self, other: Self) -> Result<Self, TimeError> {
                let (seconds, fraction) = arithmetic(
                    (self.seconds, self.$field),
                    (other.seconds, other.$field),
                    $base,
                    true,
                )?;
                Ok(Self::new(seconds, fraction))
            }
            /// Require canonical fields; do not silently normalize raw values.
            pub fn to_duration(self) -> Result<Duration, TimeError> {
                if !(0..$base).contains(&self.$field) {
                    return Err(TimeError::InvalidFraction);
                }
                if self.seconds < 0 {
                    return Err(TimeError::NegativeDuration);
                }
                Ok(Duration::new(
                    self.seconds as u64,
                    (self.$field * $nanos) as u32,
                ))
            }
            /// Truncate to this type's resolution (microseconds for TimeVal).
            pub fn from_duration(value: Duration) -> Result<Self, TimeError> {
                let seconds =
                    i64::try_from(value.as_secs()).map_err(|_| TimeError::ArithmeticOverflow)?;
                Ok(Self::new(seconds, i64::from(value.subsec_nanos()) / $nanos))
            }
        }
    };
}
raw_time!(TimeVal, microseconds, 1_000_000, 1_000);
raw_time!(TimeSpec, nanoseconds, 1_000_000_000, 1);

pub const fn minutes_component(seconds: i64) -> i64 {
    (seconds % 3600) / 60
}
pub const fn hours_component(seconds: i64) -> i64 {
    (seconds % 86400) / 3600
}
/// Match the C helper's fixed 365-day year.
pub const fn days_component(seconds: i64) -> i64 {
    (seconds % (86400 * 365)) / 86400
}
