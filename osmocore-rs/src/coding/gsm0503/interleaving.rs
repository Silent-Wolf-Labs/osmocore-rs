// SPDX-License-Identifier: GPL-3.0-or-later

//! Interleaving coded blocks across bursts.

use super::coding::{XCCH_BURST_DATA_BITS, XCCH_CODED_BITS};

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
