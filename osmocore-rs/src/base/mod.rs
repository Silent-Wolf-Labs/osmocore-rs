// SPDX-License-Identifier: GPL-3.0-or-later

//! Core utilities, the counterpart of libosmocore's `src/core`.
//!
//! - Bits and encodings: [`bits`], [`bitvec`], [`prbs`], [`base64`], [`bcd`],
//!   [`hexparse`], [`crc`] and [`conv`].
//! - Messages: [`msgb`], [`prim`] and [`gsmtap`].
//! - Time, timers and state: [`time`], [`timer`], [`tdef`], [`time_accounting`], [`fsm`]
//!   and [`rate_ctr`].
//! - Small utilities: [`use_count`], [`strrb`] and [`sockaddr_str`].

pub mod base64;
pub mod bcd;
pub mod bits;
pub mod bitvec;
pub mod conv;
pub mod crc;
pub mod fsm;
pub mod gsmtap;
pub mod hexparse;
pub mod msgb;
pub mod prbs;
pub mod prim;
pub mod rate_ctr;
pub mod sockaddr_str;
pub mod strrb;
pub mod tdef;
pub mod time;
pub mod time_accounting;
pub mod timer;
pub mod use_count;
