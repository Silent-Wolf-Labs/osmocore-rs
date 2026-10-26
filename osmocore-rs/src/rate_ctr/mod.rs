// SPDX-License-Identifier: GPL-3.0-or-later

//! Owned counters, group registry, interval rollups and explicit monotonic polling.
mod counter;
mod driver;
mod group;
mod runtime;
pub use counter::{
    RATE_CTR_INTV_DAY, RATE_CTR_INTV_HOUR, RATE_CTR_INTV_MIN, RATE_CTR_INTV_NUM, RATE_CTR_INTV_SEC,
    RateCtr, RateCtrPerIntv,
};
pub use driver::RateCtrPeriodicDriver;
pub use group::{RateCtrDesc, RateCtrError, RateCtrGroup, RateCtrGroupDesc, RateCtrGroupId};
pub use runtime::RateCtrRuntime;
