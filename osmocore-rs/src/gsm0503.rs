// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM 05.03 (3GPP TS 45.003) channel coding for SACCH, SDCCH and BCCH blocks.
//!
//! [`xcch_encode`] turns a 23-byte L2 frame into four bursts of unpacked bits, and
//! [`xcch_decode`] recovers the frame from soft bits. Soft bits run from -127 to
//! 127: positive means 0, negative means 1 and 0 means unknown.
//!
//! ```
//! use osmocore_rs::gsm0503::{xcch_decode, xcch_encode};
//! let frame = [0x2b; 23];
//! let soft = xcch_encode(&frame).map(|bit| if bit == 0 { 127 } else { -127 });
//! let decoded = xcch_decode(&soft);
//! assert_eq!(decoded.data, Some(frame));
//! assert_eq!(decoded.bit_errors, 0);
//! ```

use crate::bits::{pack_into_offset, unpack_into_offset};
use crate::conv::{self, ConvCode, ConvCodeSpec, Termination};
use crate::crc::Crc64Code;
use crate::gsm0502::NB_GMSK_PAYLOAD_BITS;
use std::sync::OnceLock;

/// Bytes in an L2 frame.
pub const XCCH_L2_BYTES: usize = 23;
/// Bits in a burst, including the two stealing flags.
pub const XCCH_BURST_BITS: usize = NB_GMSK_PAYLOAD_BITS;
/// Bits in the four bursts of a block.
pub const XCCH_BURSTS_BITS: usize = 4 * XCCH_BURST_BITS;
/// Data bits in a burst, without the stealing flags.
pub const XCCH_BURST_DATA_BITS: usize = XCCH_BURST_BITS - 2;
/// Bits after convolutional coding.
pub const XCCH_CODED_BITS: usize = 4 * XCCH_BURST_DATA_BITS;
/// Where a burst's `hl` and `hn` stealing flags sit among its [`XCCH_BURST_BITS`], between
/// the two halves of its data. [`xcch_decode`] ignores them.
pub const XCCH_STEALING_FLAG_OFFSETS: [usize; 2] = [HALF_BURST, HALF_BURST + 1];

const L2_BITS: usize = 8 * XCCH_L2_BYTES;
const PARITY_BITS: usize = 40;
const UNCODED_BITS: usize = L2_BITS + PARITY_BITS;
/// Where each half of a burst's data sits around the stealing flags.
const HALF_BURST: usize = XCCH_BURST_DATA_BITS / 2;

/// The Fire code protecting an xCCH block, g(D) = (D^23 + 1)(D^17 + D^3 + 1), with
/// inverted parity.
pub const FIRE_CRC40: Crc64Code = Crc64Code {
    bits: PARITY_BITS as i32,
    poly: 0x00_0482_0009,
    init: 0,
    remainder: 0xff_ffff_ffff,
};

/// Returns the xCCH convolutional code: rate 1/2, constraint length 5, with
/// G0 = 1 + D^3 + D^4 and G1 = 1 + D + D^3 + D^4.
pub fn xcch_code() -> &'static ConvCode {
    static CODE: OnceLock<ConvCode> = OnceLock::new();
    CODE.get_or_init(|| {
        let transitions = |function: fn(u8, u8) -> u8| {
            (0..16)
                .map(|state| [function(state, 0), function(state, 1)])
                .collect::<Vec<_>>()
        };
        ConvCode::new(ConvCodeSpec {
            n: 2,
            k: 5,
            len: UNCODED_BITS as i32,
            term: Termination::FLUSH,
            next_output: transitions(|state, input| {
                let shared = input ^ (state >> 2 & 1) ^ (state >> 3 & 1);
                shared << 1 | (shared ^ (state & 1))
            }),
            next_state: transitions(|state, input| (state << 1 | input) & 15),
            next_term_output: None,
            next_term_state: None,
            puncture: None,
        })
        .expect("the xCCH code tables are valid")
    })
}

/// Returns where coded bit `k` goes in the interleaved block.
const fn interleaved_position(k: usize) -> usize {
    (k & 3) * XCCH_BURST_DATA_BITS + 2 * ((49 * k) % 57) + ((k & 7) >> 2)
}

/// Spreads a coded block across four bursts' worth of data bits.
pub fn xcch_interleave<T: Copy>(coded: &[T; XCCH_CODED_BITS]) -> [T; XCCH_CODED_BITS] {
    let mut interleaved = *coded;
    for (k, bit) in coded.iter().enumerate() {
        interleaved[interleaved_position(k)] = *bit;
    }
    interleaved
}

/// Reverses [`xcch_interleave`].
pub fn xcch_deinterleave<T: Copy>(interleaved: &[T; XCCH_CODED_BITS]) -> [T; XCCH_CODED_BITS] {
    std::array::from_fn(|k| interleaved[interleaved_position(k)])
}

/// Writes a burst's data bits around its stealing flags. A flag of `None` leaves
/// that position in `burst` unchanged.
pub fn xcch_burst_map(
    data: &[u8; XCCH_BURST_DATA_BITS],
    burst: &mut [u8; XCCH_BURST_BITS],
    hl: Option<u8>,
    hn: Option<u8>,
) {
    burst[..HALF_BURST].copy_from_slice(&data[..HALF_BURST]);
    burst[HALF_BURST + 2..].copy_from_slice(&data[HALF_BURST..]);
    if let Some(hl) = hl {
        burst[HALF_BURST] = hl;
    }
    if let Some(hn) = hn {
        burst[HALF_BURST + 1] = hn;
    }
}

/// Splits a burst of soft bits into its data bits and its `hl` and `hn` flags.
pub fn xcch_burst_unmap(burst: &[i8; XCCH_BURST_BITS]) -> ([i8; XCCH_BURST_DATA_BITS], i8, i8) {
    let data =
        std::array::from_fn(|index| burst[if index < HALF_BURST { index } else { index + 2 }]);
    (data, burst[HALF_BURST], burst[HALF_BURST + 1])
}

/// Encodes an L2 frame into four bursts of unpacked bits, with the stealing flags set.
pub fn xcch_encode(l2: &[u8; XCCH_L2_BYTES]) -> [u8; XCCH_BURSTS_BITS] {
    let mut uncoded = [0u8; UNCODED_BITS];
    let (data, parity) = uncoded.split_at_mut(L2_BITS);
    unpack_into_offset(data, 0, l2, 0, L2_BITS, true).expect("the sizes are fixed");
    FIRE_CRC40
        .set_bits(data, parity)
        .expect("the sizes are fixed");

    let mut coded = [0u8; XCCH_CODED_BITS];
    conv::encode(xcch_code(), &uncoded, &mut coded).expect("the sizes are fixed");

    let interleaved = xcch_interleave(&coded);
    let mut output = [0u8; XCCH_BURSTS_BITS];
    let (bursts, _) = output.as_chunks_mut::<XCCH_BURST_BITS>();
    let (data, _) = interleaved.as_chunks::<XCCH_BURST_DATA_BITS>();
    for (burst, data) in bursts.iter_mut().zip(data) {
        xcch_burst_map(data, burst, Some(1), Some(1));
    }
    output
}

/// The result of [`xcch_decode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XcchDecoded {
    /// The L2 frame, or `None` if the parity check failed.
    pub data: Option<[u8; XCCH_L2_BYTES]>,
    /// Coded bits that disagree with the decoder's best path. An unknown soft bit of
    /// 0 counts as an error. Counted even when the parity check fails.
    pub bit_errors: u32,
    /// Coded bits compared to produce `bit_errors`.
    pub total_bits: u32,
}

/// Decodes four bursts of soft bits, ignoring the stealing flags.
pub fn xcch_decode(bursts: &[i8; XCCH_BURSTS_BITS]) -> XcchDecoded {
    let mut interleaved = [0i8; XCCH_CODED_BITS];
    let (data, _) = interleaved.as_chunks_mut::<XCCH_BURST_DATA_BITS>();
    let (bursts, _) = bursts.as_chunks::<XCCH_BURST_BITS>();
    for (data, burst) in data.iter_mut().zip(bursts) {
        (*data, _, _) = xcch_burst_unmap(burst);
    }
    let coded = xcch_deinterleave(&interleaved);

    let mut uncoded = [0u8; UNCODED_BITS];
    conv::decode(xcch_code(), &coded, &mut uncoded).expect("the sizes are fixed");

    let mut recoded = [0u8; XCCH_CODED_BITS];
    conv::encode(xcch_code(), &uncoded, &mut recoded).expect("the sizes are fixed");
    let bit_errors = recoded
        .iter()
        .zip(&coded)
        .filter(|(bit, soft)| !matches!((**bit, **soft), (1, ..0) | (0, 1..)))
        .count() as u32;

    let (data, parity) = uncoded.split_at(L2_BITS);
    let parity_ok = FIRE_CRC40
        .check_bits(data, parity)
        .expect("the sizes are fixed");
    let data = parity_ok.then(|| {
        let mut l2 = [0u8; XCCH_L2_BYTES];
        pack_into_offset(&mut l2, 0, data, 0, L2_BITS, true).expect("the sizes are fixed");
        l2
    });
    XcchDecoded {
        data,
        bit_errors,
        total_bits: XCCH_CODED_BITS as u32,
    }
}
