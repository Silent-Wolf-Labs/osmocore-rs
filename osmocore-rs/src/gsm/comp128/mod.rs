// SPDX-License-Identifier: GPL-3.0-or-later

//! COMP128 v1, v2 and v3, the GSM A3/A8 algorithms many SIMs use.
//!
//! Each takes a subscriber key and a random challenge and gives SRES and Kc. COMP128 was
//! never published officially: these follow public descriptions recovered from SIMs.
//! COMP128v1 is broken, since a SIM's Ki can be recovered from its answers. The
//! [`crate::gsm::auth`] module selects among these and XOR-2G.
//!
//! ```
//! use osmocore_rs::gsm::{auth::Ki, comp128::comp128v2};
//! let vector = comp128v2(&Ki::new([0xaa; 16]), &[0; 16]);
//! assert_eq!(vector.sres, [0x34, 0xb4, 0x22, 0x5b]);
//! assert_eq!(vector.kc.as_bytes(), &[0xf1, 0x6b, 0x96, 0xe1, 0x18, 0xa8, 0x58, 0x00]);
//! ```

mod tables;
mod v1;
mod v23;

pub use v1::comp128v1;
pub use v23::{comp128v2, comp128v3};
