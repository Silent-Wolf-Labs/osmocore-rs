// SPDX-License-Identifier: GPL-3.0-or-later

//! TCH/FS, TCH/EFS and FACCH/F encoding and decoding.

use super::codes::Gsm0503Code;
use super::coding::{
    XCCH_L2_BYTES, XcchDecoded, decode_counting_errors, xcch_decode_coded, xcch_encode_coded,
};
use super::interleaving::tch_fr_position;
use super::parity::{TCH_EFR_CRC8, TCH_FR_CRC3};
use super::tables::{EFR_PROTECTED_BITS, FR_PARAMETER_BITS};
use crate::codec::{GSM_EFR_BYTES, GSM_FR_BYTES, GSM610_BITORDER, GSM660_BITORDER};
use crate::gsm::gsm0502::NB_GMSK_PAYLOAD_BITS;

/// Bits in a TCH burst, including the two stealing flags.
pub const TCH_BURST_BITS: usize = NB_GMSK_PAYLOAD_BITS;
/// Data bits in a TCH burst, without the stealing flags.
pub const TCH_BURST_DATA_BITS: usize = TCH_BURST_BITS - 2;
/// Bits in the eight bursts a TCH/F block is spread over.
pub const TCH_FR_BURSTS_BITS: usize = 8 * TCH_BURST_BITS;
/// Data bits in the eight bursts of a TCH/F block. A block fills half of them.
pub const TCH_FR_INTERLEAVED_BITS: usize = 8 * TCH_BURST_DATA_BITS;
/// Bits of a TCH/F block after channel coding.
pub const TCH_FR_CODED_BITS: usize = 456;

/// Bits of an FR frame, and of the coded speech of an EFR frame.
const SPEECH_BITS: usize = 260;
/// Bits of an EFR frame.
const EFR_BITS: usize = 244;
/// Class 1 bits, which the convolutional code protects.
const CLASS1_BITS: usize = 182;
/// Class 1a bits, which the 3-bit CRC protects.
const CLASS1A_BITS: usize = 50;
/// Convolutionally coded bits; the rest of a block are class 2 bits, sent uncoded.
const CONV_CODED_BITS: usize = 378;
/// Class 1 bits plus the CRC, the input of the convolutional code.
const UNCODED_BITS: usize = CLASS1_BITS + 3;
/// Bits before the frame bits in an RTP payload, holding the signature.
const SIGNATURE_BITS: usize = 4;
const FR_SIGNATURE: u8 = 0xd0;
const EFR_SIGNATURE: u8 = 0xc0;

/// How an EFR frame's bits `s` sit among the 260 coded speech bits `w`: each
/// `(from, to, len)` puts `s[from..from + len]` at `w[to..to + len]`.
const EFR_SEGMENTS: [(usize, usize, usize); 5] = [
    (0, 0, 71),
    (71, 73, 50),
    (121, 125, 53),
    (174, 180, 50),
    (224, 232, 20),
];
/// EFR bits sent three times: once in place and twice more after their segment.
const EFR_REPEATED: [(usize, usize); 4] = [(69, 71), (119, 123), (172, 178), (222, 230)];
const EFR_CRC_OFFSET: usize = 252;

/// How the 260 bits of an FR frame are ordered in its RTP payload. C's `net_order`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FrBitOrder {
    /// The codec's serial order, which [`GSM610_BITORDER`] indexes. C's `net_order` set.
    #[default]
    Network,
    /// Each of the 76 codec parameters with its bits reversed. C's `net_order` clear.
    Reversed,
}

/// The speech codec of a TCH/F channel, which [`tch_fr_decode`] needs to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TchFrCodec {
    /// Full rate, GSM 06.10.
    Fr(FrBitOrder),
    /// Enhanced full rate, GSM 06.60.
    Efr,
}

/// A frame carried on a TCH/F channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TchFrFrame {
    /// An FR frame in RTP format. The first four bits are its `0xd` signature.
    Fr([u8; GSM_FR_BYTES]),
    /// An EFR frame in RTP format. The first four bits are its `0xc` signature.
    Efr([u8; GSM_EFR_BYTES]),
    /// A FACCH/F L2 frame, which steals the block from speech.
    Facch([u8; XCCH_L2_BYTES]),
}

impl TchFrFrame {
    /// Picks the frame type from the length, as C's `gsm0503_tch_fr_encode` does: 33
    /// bytes are FR, 31 EFR and 23 FACCH. Returns `None` for any other length.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        match data.len() {
            GSM_FR_BYTES => data.try_into().ok().map(Self::Fr),
            GSM_EFR_BYTES => data.try_into().ok().map(Self::Efr),
            XCCH_L2_BYTES => data.try_into().ok().map(Self::Facch),
            _ => None,
        }
    }

    /// The frame's bytes.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Fr(data) => data,
            Self::Efr(data) => data,
            Self::Facch(data) => data,
        }
    }
}

/// The result of [`tch_fr_decode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TchFrDecoded {
    /// The frame, or `None` if a parity check failed. Decoded speech carries its
    /// codec's RTP signature, whatever the sender's first four bits were.
    pub frame: Option<TchFrFrame>,
    /// Whether the stealing flags marked the block as FACCH. It then decodes as FACCH
    /// or not at all.
    pub stolen: bool,
    /// Coded bits that disagree with the decoder's best path. An unknown soft bit of
    /// 0 counts as an error. Counted even when a parity check fails. Speech counts only
    /// its convolutionally coded bits, not the class 2 bits.
    pub bit_errors: u32,
    /// Coded bits compared to produce `bit_errors`: 456 for FACCH, 378 for speech.
    pub total_bits: u32,
}

/// Encodes a frame into a TCH/F block, or with `None` a block that the receiver will
/// take as a bad frame.
///
/// A block is spread over eight bursts: the even data bits of the first four and the
/// odd data bits of the last four. Only those positions and each burst's stealing flag
/// are written; the rest belong to the neighbouring blocks, which start four bursts
/// earlier and later. `order` applies only to FR frames.
///
/// The bad-frame block codes all-zero bits with all-zero parity, which no decoder
/// accepts, as C does for a zero-length frame.
///
/// ```
/// use osmocore_rs::coding::gsm0503::{
///     FrBitOrder, TCH_FR_BURSTS_BITS, TchFrCodec, TchFrFrame, tch_fr_decode, tch_fr_encode,
/// };
/// let mut frame = [0x55; 33];
/// frame[0] = 0xd5;
/// let mut bursts = [0; TCH_FR_BURSTS_BITS];
/// tch_fr_encode(&mut bursts, Some(&TchFrFrame::Fr(frame)), FrBitOrder::Network);
/// let soft = bursts.map(|bit| if bit == 0 { 127 } else { -127 });
/// let decoded = tch_fr_decode(&soft, TchFrCodec::Fr(FrBitOrder::Network));
/// assert_eq!(decoded.frame, Some(TchFrFrame::Fr(frame)));
/// assert_eq!((decoded.bit_errors, decoded.total_bits), (0, 378));
/// ```
pub fn tch_fr_encode(
    bursts: &mut [u8; TCH_FR_BURSTS_BITS],
    frame: Option<&TchFrFrame>,
    order: FrBitOrder,
) {
    let (coded, stolen) = match frame {
        Some(TchFrFrame::Fr(data)) => (speech_coded(&fr_disassemble(data, order)), false),
        Some(TchFrFrame::Efr(data)) => (speech_coded(&efr_to_channel_order(data)), false),
        Some(TchFrFrame::Facch(l2)) => (xcch_encode_coded(l2), true),
        None => (bad_frame_coded(), false),
    };
    map_block(bursts, &coded, u8::from(stolen));
}

/// Decodes a TCH/F block of soft bits. The stealing flags decide between FACCH and
/// speech in `codec`.
pub fn tch_fr_decode(bursts: &[i8; TCH_FR_BURSTS_BITS], codec: TchFrCodec) -> TchFrDecoded {
    let coded = unmap_block(bursts);
    if facch_stolen(bursts) {
        let decoded = xcch_decode_coded(&coded);
        return TchFrDecoded {
            frame: decoded.data.map(TchFrFrame::Facch),
            stolen: true,
            bit_errors: decoded.bit_errors,
            total_bits: decoded.total_bits,
        };
    }

    let mut uncoded = [0u8; UNCODED_BITS];
    let bit_errors = decode_counting_errors(
        Gsm0503Code::TchFr.code(),
        &coded[..CONV_CODED_BITS],
        &mut uncoded,
    );
    let mut d = [0u8; SPEECH_BITS];
    for i in 0..CLASS1_BITS / 2 {
        d[2 * i] = uncoded[i];
        d[2 * i + 1] = uncoded[UNCODED_BITS - 1 - i];
    }
    for (bit, soft) in d[CLASS1_BITS..].iter_mut().zip(&coded[CONV_CODED_BITS..]) {
        *bit = u8::from(*soft < 0);
    }
    let parity = &uncoded[CLASS1_BITS / 2..CLASS1_BITS / 2 + 3];
    let frame = TCH_FR_CRC3
        .check_bits(&d[..CLASS1A_BITS], parity)
        .expect("the sizes are fixed")
        .then(|| match codec {
            TchFrCodec::Fr(order) => Some(TchFrFrame::Fr(fr_reassemble(&d, order))),
            TchFrCodec::Efr => efr_from_channel_order(&d).map(TchFrFrame::Efr),
        })
        .flatten();
    TchFrDecoded {
        frame,
        stolen: false,
        bit_errors,
        total_bits: CONV_CODED_BITS as u32,
    }
}

/// Encodes a FACCH/F L2 frame into a TCH/F block, with every stealing flag set. Writes
/// the same positions as [`tch_fr_encode`].
pub fn tch_fr_facch_encode(bursts: &mut [u8; TCH_FR_BURSTS_BITS], l2: &[u8; XCCH_L2_BYTES]) {
    map_block(bursts, &xcch_encode_coded(l2), 1);
}

/// Decodes a FACCH/F block, or returns `None` if its stealing flags don't mark it as
/// FACCH.
pub fn tch_fr_facch_decode(bursts: &[i8; TCH_FR_BURSTS_BITS]) -> Option<XcchDecoded> {
    facch_stolen(bursts).then(|| xcch_decode_coded(&unmap_block(bursts)))
}

/// The stealing flag a block uses in burst `burst`: `hu` in the first four bursts and
/// `hl` in the last four.
const fn stealing_flag(burst: usize) -> usize {
    burst * TCH_BURST_BITS + if burst < 4 { 58 } else { 57 }
}

/// Whether a block is FACCH: its stealing flags sum to less than zero, so lean to 1.
fn facch_stolen(bursts: &[i8; TCH_FR_BURSTS_BITS]) -> bool {
    (0..8)
        .map(|burst| i32::from(bursts[stealing_flag(burst)]))
        .sum::<i32>()
        < 0
}

/// Where coded bit `k` goes in the eight bursts of a block.
const fn burst_position(k: usize) -> usize {
    let (burst, index) = tch_fr_position(k);
    burst * TCH_BURST_BITS + if index < 57 { index } else { index + 2 }
}

fn map_block(bursts: &mut [u8; TCH_FR_BURSTS_BITS], coded: &[u8; TCH_FR_CODED_BITS], flag: u8) {
    for (k, bit) in coded.iter().enumerate() {
        bursts[burst_position(k)] = *bit;
    }
    for burst in 0..8 {
        bursts[stealing_flag(burst)] = flag;
    }
}

fn unmap_block(bursts: &[i8; TCH_FR_BURSTS_BITS]) -> [i8; TCH_FR_CODED_BITS] {
    std::array::from_fn(|k| bursts[burst_position(k)])
}

/// Channel codes 260 speech bits in channel order, most important first.
fn speech_coded(d: &[u8; SPEECH_BITS]) -> [u8; TCH_FR_CODED_BITS] {
    let mut uncoded = [0u8; UNCODED_BITS];
    TCH_FR_CRC3
        .set_bits(
            &d[..CLASS1A_BITS],
            &mut uncoded[CLASS1_BITS / 2..CLASS1_BITS / 2 + 3],
        )
        .expect("the sizes are fixed");
    for i in 0..CLASS1_BITS / 2 {
        uncoded[i] = d[2 * i];
        uncoded[UNCODED_BITS - 1 - i] = d[2 * i + 1];
    }
    let mut coded = [0u8; TCH_FR_CODED_BITS];
    coded[CONV_CODED_BITS..].copy_from_slice(&d[CLASS1_BITS..]);
    conv_encode(&uncoded, &mut coded);
    coded
}

fn bad_frame_coded() -> [u8; TCH_FR_CODED_BITS] {
    let mut coded = [0u8; TCH_FR_CODED_BITS];
    conv_encode(&[0; UNCODED_BITS], &mut coded);
    coded
}

fn conv_encode(uncoded: &[u8; UNCODED_BITS], coded: &mut [u8; TCH_FR_CODED_BITS]) {
    crate::base::conv::encode(
        Gsm0503Code::TchFr.code(),
        uncoded,
        &mut coded[..CONV_CODED_BITS],
    )
    .expect("the sizes are fixed");
}

/// Bit `index` after the signature of an RTP payload.
fn payload_bit(data: &[u8], index: usize) -> u8 {
    let position = SIGNATURE_BITS + index;
    (data[position / 8] >> (7 - position % 8)) & 1
}

/// Packs bits after a signature into an RTP payload.
fn payload<const N: usize>(signature: u8, bits: &[u8]) -> [u8; N] {
    let mut data = [0u8; N];
    data[0] = signature;
    for (index, bit) in bits.iter().enumerate() {
        let position = SIGNATURE_BITS + index;
        data[position / 8] |= bit << (7 - position % 8);
    }
    data
}

/// Calls `visit(serial, payload)` for each FR bit: its index in serial order and in the
/// payload.
fn fr_positions(order: FrBitOrder, mut visit: impl FnMut(usize, usize)) {
    match order {
        FrBitOrder::Network => (0..SPEECH_BITS).for_each(|index| visit(index, index)),
        FrBitOrder::Reversed => {
            let mut start = 0;
            for len in FR_PARAMETER_BITS.map(usize::from) {
                for offset in 0..len {
                    visit(start + len - 1 - offset, start + offset);
                }
                start += len;
            }
        }
    }
}

/// Takes an FR frame's bits into channel order.
fn fr_disassemble(data: &[u8; GSM_FR_BYTES], order: FrBitOrder) -> [u8; SPEECH_BITS] {
    let mut serial = [0u8; SPEECH_BITS];
    fr_positions(order, |index, position| {
        serial[index] = payload_bit(data, position)
    });
    GSM610_BITORDER.map(|index| serial[usize::from(index)])
}

/// Reverses [`fr_disassemble`].
fn fr_reassemble(d: &[u8; SPEECH_BITS], order: FrBitOrder) -> [u8; GSM_FR_BYTES] {
    let mut serial = [0u8; SPEECH_BITS];
    for (bit, index) in d.iter().zip(GSM610_BITORDER) {
        serial[usize::from(index)] = *bit;
    }
    let mut bits = [0u8; SPEECH_BITS];
    fr_positions(order, |index, position| bits[position] = serial[index]);
    payload(FR_SIGNATURE, &bits)
}

fn efr_protected(s: &[u8; EFR_BITS]) -> [u8; EFR_PROTECTED_BITS.len()] {
    EFR_PROTECTED_BITS.map(|index| s[usize::from(index)])
}

/// Adds an EFR frame's CRC and repetitions and takes its bits into channel order.
fn efr_to_channel_order(data: &[u8; GSM_EFR_BYTES]) -> [u8; SPEECH_BITS] {
    let s: [u8; EFR_BITS] = std::array::from_fn(|index| payload_bit(data, index));
    let mut w = [0u8; SPEECH_BITS];
    TCH_EFR_CRC8
        .set_bits(&efr_protected(&s), &mut w[EFR_CRC_OFFSET..])
        .expect("the sizes are fixed");
    for (from, to, len) in EFR_SEGMENTS {
        w[to..to + len].copy_from_slice(&s[from..from + len]);
    }
    for (from, to) in EFR_REPEATED {
        w[to] = s[from];
        w[to + 1] = s[from];
    }
    GSM660_BITORDER.map(|index| w[usize::from(index)])
}

/// Reverses [`efr_to_channel_order`], taking each repeated bit by majority, or returns
/// `None` if the CRC fails.
fn efr_from_channel_order(d: &[u8; SPEECH_BITS]) -> Option<[u8; GSM_EFR_BYTES]> {
    let mut w = [0u8; SPEECH_BITS];
    for (bit, index) in d.iter().zip(GSM660_BITORDER) {
        w[usize::from(index)] = *bit;
    }
    let mut s = [0u8; EFR_BITS];
    for (from, to, len) in EFR_SEGMENTS {
        s[from..from + len].copy_from_slice(&w[to..to + len]);
    }
    for (index, copies) in EFR_REPEATED {
        s[index] = u8::from(s[index] + w[copies] + w[copies + 1] >= 2);
    }
    TCH_EFR_CRC8
        .check_bits(&efr_protected(&s), &w[EFR_CRC_OFFSET..])
        .expect("the sizes are fixed")
        .then(|| payload(EFR_SIGNATURE, &s))
}
