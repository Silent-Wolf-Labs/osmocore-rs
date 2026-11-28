// SPDX-License-Identifier: GPL-3.0-or-later

//! Interleaving coded blocks across bursts.

use super::coding::{XCCH_BURST_DATA_BITS, XCCH_CODED_BITS};
use super::tch_fr::{TCH_BURST_DATA_BITS, TCH_FR_CODED_BITS, TCH_FR_INTERLEAVED_BITS};

/// Returns where xCCH coded bit `k` goes in the interleaved block.
const fn xcch_position(k: usize) -> usize {
    (k & 3) * XCCH_BURST_DATA_BITS + 2 * ((49 * k) % 57) + ((k & 7) >> 2)
}

/// Spreads a coded block across four bursts' worth of data bits.
pub fn xcch_interleave<T: Copy>(coded: &[T; XCCH_CODED_BITS]) -> [T; XCCH_CODED_BITS] {
    let mut interleaved = *coded;
    for (k, bit) in coded.iter().enumerate() {
        interleaved[xcch_position(k)] = *bit;
    }
    interleaved
}

/// Reverses [`xcch_interleave`].
pub fn xcch_deinterleave<T: Copy>(interleaved: &[T; XCCH_CODED_BITS]) -> [T; XCCH_CODED_BITS] {
    std::array::from_fn(|k| interleaved[xcch_position(k)])
}

/// Returns the burst and data bit where TCH/F coded bit `k` goes. Bursts 0 to 3 take
/// even data bits and bursts 4 to 7 odd ones.
pub(super) const fn tch_fr_position(k: usize) -> (usize, usize) {
    (k & 7, 2 * ((49 * k) % 57) + ((k & 7) >> 2))
}

/// Spreads a TCH/F coded block across eight bursts' worth of data bits. Writes only
/// the block's half of `interleaved`; the other half belongs to the neighbouring
/// blocks.
pub fn tch_fr_interleave<T: Copy>(
    coded: &[T; TCH_FR_CODED_BITS],
    interleaved: &mut [T; TCH_FR_INTERLEAVED_BITS],
) {
    for (k, bit) in coded.iter().enumerate() {
        let (burst, index) = tch_fr_position(k);
        interleaved[burst * TCH_BURST_DATA_BITS + index] = *bit;
    }
}

/// Reverses [`tch_fr_interleave`], reading only the block's half of `interleaved`.
pub fn tch_fr_deinterleave<T: Copy>(
    interleaved: &[T; TCH_FR_INTERLEAVED_BITS],
) -> [T; TCH_FR_CODED_BITS] {
    std::array::from_fn(|k| {
        let (burst, index) = tch_fr_position(k);
        interleaved[burst * TCH_BURST_DATA_BITS + index]
    })
}
