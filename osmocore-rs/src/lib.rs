// SPDX-License-Identifier: GPL-3.0-or-later

//! Rust-native telecom and core primitives with owned buffers and explicit runtimes.
//!
//! Functional modules live at the crate root. Use owned runtimes, explicit
//! schedulers, stable handles and typed buffer APIs for new code. Distinct C
//! buffer/status/sentinel contracts and evidence introspection live in [`compat`].
//! Compatibility behavior is documented per subsystem; no C ABI is exported.
//!
//! The optional `tracing` feature emits structured diagnostics with
//! application-owned subscribers and filters. See [`diagnostics`] for event
//! fields, levels, context propagation and a runnable configuration example.
//! The [diagnostics architecture](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md#features-and-structured-diagnostics)
//! explains feature relationships and design boundaries.
//!
//! ```
//! use osmocore_rs::{base64, bitvec::BitVec, msgb::Msgb};
//! let encoded = base64::encode(b"telecom").unwrap();
//! assert_eq!(base64::decode(encoded.as_bytes()).unwrap(), b"telecom");
//! let mut bytes = [0u8; 1];
//! let mut bits = BitVec::new(&mut bytes);
//! bits.write_uint(5, 3).unwrap();
//! assert_eq!(bits.data(), &[0xa0]);
//! let mut message = Msgb::with_headroom(16, 4).unwrap();
//! message.put_u16(0x1234).unwrap();
//! assert_eq!(message.data(), &[0x12, 0x34]);
//! ```

pub mod diagnostics;

pub mod base64;
pub mod bcd;
pub mod bits;
pub mod bitvec;
pub mod compat;
pub mod conv;
pub mod crc;
pub mod fsm;
pub mod gsmtap;
pub mod hexparse;
pub mod msgb;
#[cfg(feature = "net")]
pub mod net;
pub mod prim;
pub mod rate_ctr;
#[cfg(feature = "runtime")]
pub mod runtime;
pub mod tdef;
pub mod time;
pub mod time_accounting;
pub mod timer;
