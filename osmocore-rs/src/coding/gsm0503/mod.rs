// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM 05.03 (3GPP TS 45.003) channel coding.
//!
//! [`xcch_encode`] turns a 23-byte SACCH, SDCCH or BCCH frame into four bursts of
//! unpacked bits, and [`xcch_decode`] recovers the frame from soft bits. Soft bits run
//! from -127 to 127: positive means 0, negative means 1 and 0 means unknown.
//!
//! ```
//! use osmocore_rs::coding::gsm0503::{xcch_decode, xcch_encode};
//! let frame = [0x2b; 23];
//! let soft = xcch_encode(&frame).map(|bit| if bit == 0 { 127 } else { -127 });
//! let decoded = xcch_decode(&soft);
//! assert_eq!(decoded.data, Some(frame));
//! assert_eq!(decoded.bit_errors, 0);
//! ```
//!
//! [`tch_fr_encode`] codes an FR or EFR speech frame, or a stolen FACCH frame, into
//! half of eight bursts, and [`tch_fr_decode`] decodes it again.
//!
//! [`Gsm0503Code`] gives every convolutional code the channels use, and the `*_CRC*`
//! constants their parity codes.
//!
//! ```
//! use osmocore_rs::base::conv;
//! use osmocore_rs::coding::gsm0503::Gsm0503Code;
//! let code = Gsm0503Code::TchFr.code();
//! let mut coded = vec![0; code.output_capacity().unwrap()];
//! assert_eq!(conv::encode(code, &[1; 185], &mut coded), Ok(378));
//! ```

mod codes;
mod coding;
mod interleaving;
mod mapping;
mod parity;
mod tables;
mod tch_fr;

pub use codes::Gsm0503Code;
pub use coding::{
    XCCH_BURST_BITS, XCCH_BURST_DATA_BITS, XCCH_BURSTS_BITS, XCCH_CODED_BITS, XCCH_L2_BYTES,
    XCCH_STEALING_FLAG_OFFSETS, XcchDecoded, xcch_code, xcch_decode, xcch_encode,
};
pub use interleaving::{
    tch_fr_deinterleave, tch_fr_interleave, xcch_deinterleave, xcch_interleave,
};
pub use mapping::{tch_burst_map, tch_burst_unmap, xcch_burst_map, xcch_burst_unmap};
pub use parity::{
    AMR_CRC6, AMR_CRC14, CS234_CRC16, FIRE_CRC40, MCS_CRC8_HDR, MCS_CRC12, RACH_CRC6, SCH_CRC10,
    TCH_EFR_CRC8, TCH_FR_CRC3,
};
pub use tch_fr::{
    FrBitOrder, TCH_BURST_BITS, TCH_BURST_DATA_BITS, TCH_FR_BURSTS_BITS, TCH_FR_CODED_BITS,
    TCH_FR_INTERLEAVED_BITS, TchFrCodec, TchFrDecoded, TchFrFrame, tch_fr_decode, tch_fr_encode,
    tch_fr_facch_decode, tch_fr_facch_encode,
};
