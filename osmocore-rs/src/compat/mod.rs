// SPDX-License-Identifier: GPL-3.0-or-later

//! Safe Rust adapters for selected C contracts and conformance inspection.
//!
//! See the [compatibility guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/src/compat/README.md)
//! for adapter families, preferred APIs, feature availability and safety boundaries.

/// Recorded base64 compatibility contracts.
pub mod base64;

/// Recorded bcd compatibility contracts.
pub mod bcd;

/// Recorded prim compatibility contracts.
pub mod prim;

/// Recorded tdef compatibility contracts.
pub mod tdef;

/// Recorded timer compatibility contracts.
pub mod timer;

/// Recorded gsmtap compatibility contracts.
pub mod gsmtap;

/// Recorded bitvec compatibility contracts.
pub mod bitvec;

/// Recorded conv compatibility contracts.
pub mod conv;

/// Recorded fsm compatibility contracts.
pub mod fsm;

/// Recorded msgb compatibility contracts.
pub mod msgb;

/// Recorded crc compatibility contracts.
pub mod crc;
