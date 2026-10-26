// SPDX-License-Identifier: GPL-3.0-or-later

//! Raw time values, injectable clock sources and owned deterministic overrides.
mod clock;
mod value;
pub use clock::{Clock, ClockId, ClockOverrides, SystemClock, TimeZone};
pub use value::{TimeError, TimeSpec, TimeVal};
pub use value::{days_component, hours_component, minutes_component};
