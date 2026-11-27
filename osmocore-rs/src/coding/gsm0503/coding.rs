// SPDX-License-Identifier: GPL-3.0-or-later

//! xCCH encoding and decoding.

use super::codes::Gsm0503Code;
use super::interleaving::{xcch_deinterleave, xcch_interleave};
use super::mapping::{xcch_burst_map, xcch_burst_unmap};
use super::parity::FIRE_CRC40;
use crate::base::bits::{pack_into_offset, unpack_into_offset};
use crate::base::conv::{self, ConvCode};
use crate::gsm::gsm0502::NB_GMSK_PAYLOAD_BITS;

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
const PARITY_BITS: usize = FIRE_CRC40.bits as usize;
const UNCODED_BITS: usize = L2_BITS + PARITY_BITS;
/// Where each half of a burst's data sits around the stealing flags.
pub(super) const HALF_BURST: usize = XCCH_BURST_DATA_BITS / 2;

/// Returns the xCCH convolutional code, [`Gsm0503Code::Xcch`]: rate 1/2, constraint
/// length 5, with G0 = 1 + D^3 + D^4 and G1 = 1 + D + D^3 + D^4.
pub fn xcch_code() -> &'static ConvCode {
    Gsm0503Code::Xcch.code()
}

/// Encodes an L2 frame into four bursts of unpacked bits, with the stealing flags set.
pub fn xcch_encode(l2: &[u8; XCCH_L2_BYTES]) -> [u8; XCCH_BURSTS_BITS] {
    let interleaved = xcch_interleave(&xcch_encode_coded(l2));
    let mut output = [0u8; XCCH_BURSTS_BITS];
    let (bursts, _) = output.as_chunks_mut::<XCCH_BURST_BITS>();
    let (data, _) = interleaved.as_chunks::<XCCH_BURST_DATA_BITS>();
    for (burst, data) in bursts.iter_mut().zip(data) {
        xcch_burst_map(data, burst, Some(1), Some(1));
    }
    output
}

/// Adds the Fire code parity to an L2 frame and convolutionally codes it. FACCH uses
/// the same coding.
pub(super) fn xcch_encode_coded(l2: &[u8; XCCH_L2_BYTES]) -> [u8; XCCH_CODED_BITS] {
    let mut uncoded = [0u8; UNCODED_BITS];
    let (data, parity) = uncoded.split_at_mut(L2_BITS);
    unpack_into_offset(data, 0, l2, 0, L2_BITS, true).expect("the sizes are fixed");
    FIRE_CRC40
        .set_bits(data, parity)
        .expect("the sizes are fixed");

    let mut coded = [0u8; XCCH_CODED_BITS];
    conv::encode(xcch_code(), &uncoded, &mut coded).expect("the sizes are fixed");
    coded
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
    xcch_decode_coded(&xcch_deinterleave(&interleaved))
}

/// Reverses [`xcch_encode_coded`] on soft bits.
pub(super) fn xcch_decode_coded(coded: &[i8; XCCH_CODED_BITS]) -> XcchDecoded {
    let mut uncoded = [0u8; UNCODED_BITS];
    let bit_errors = decode_counting_errors(xcch_code(), coded, &mut uncoded);

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

/// Decodes an unpunctured code and counts the coded bits that disagree with the
/// re-encoded result. A soft bit of 0 counts as an error.
pub(super) fn decode_counting_errors(code: &ConvCode, coded: &[i8], uncoded: &mut [u8]) -> u32 {
    conv::decode(code, coded, uncoded).expect("the sizes are fixed");
    let mut recoded = [0u8; XCCH_CODED_BITS];
    let len = conv::encode(code, uncoded, &mut recoded).expect("the sizes are fixed");
    recoded[..len]
        .iter()
        .zip(coded)
        .filter(|(bit, soft)| !matches!((**bit, **soft), (1, ..0) | (0, 1..)))
        .count() as u32
}
