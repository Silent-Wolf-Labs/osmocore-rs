// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM bands, ARFCNs, power levels, GSM time, reduced frame numbers, TLLIs,
//! classmarks, RACH parameters and the 7-bit default alphabet.
//!
//! ```
//! use osmocore_rs::gsm::gsm_utils::{Band, Direction, GsmTime, arfcn_to_freq10};
//! assert_eq!(Band::parse("DCS1800"), Some(Band::Dcs1800));
//! assert_eq!(Band::from_arfcn(512), Some(Band::Dcs1800));
//! assert_eq!(arfcn_to_freq10(0, Direction::Downlink), Some(9350));
//! assert_eq!(Band::Gsm900.ms_power_dbm(5), Some(33));
//!
//! let time = GsmTime::from_frame(1327).unwrap();
//! assert_eq!((time.t1(), time.t2(), time.t3()), (1, 1, 1));
//! assert_eq!(GsmTime::from_parts(1, 1, 1), Some(time));
//! ```
//!
//! ```
//! use osmocore_rs::gsm::gsm_utils::{decode_7bit, encode_7bit};
//! let packed = encode_7bit(b"test text", 140);
//! assert_eq!(packed.septets, 9);
//! assert_eq!(decode_7bit(&packed.octets, packed.septets).unwrap(), b"test text");
//! ```

mod numeric;
mod sms_7bit;

pub use numeric::*;
pub use sms_7bit::*;
