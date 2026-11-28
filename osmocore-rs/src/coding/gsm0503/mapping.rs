// SPDX-License-Identifier: GPL-3.0-or-later

//! Placing data bits and stealing flags in bursts.

use super::coding::{HALF_BURST, XCCH_BURST_BITS, XCCH_BURST_DATA_BITS};
use super::tch_fr::{TCH_BURST_BITS, TCH_BURST_DATA_BITS};

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

/// Where a TCH data bit sits in its burst, around the stealing flags.
const fn tch_burst_position(index: usize) -> usize {
    if index < HALF_BURST { index } else { index + 2 }
}

/// The stealing flag that goes with a half of a TCH burst's data bits: `hu` with the even
/// bits, `hl` with the odd ones.
const fn tch_stealing_flag(odd: bool) -> usize {
    if odd { HALF_BURST } else { HALF_BURST + 1 }
}

/// Writes the odd or even data bits of a TCH burst and, unless it is `None`, the
/// matching stealing flag. Every other position in `burst` is left unchanged.
pub fn tch_burst_map(
    data: &[u8; TCH_BURST_DATA_BITS],
    burst: &mut [u8; TCH_BURST_BITS],
    h: Option<u8>,
    odd: bool,
) {
    for index in (usize::from(odd)..TCH_BURST_DATA_BITS).step_by(2) {
        burst[tch_burst_position(index)] = data[index];
    }
    if let Some(h) = h {
        burst[tch_stealing_flag(odd)] = h;
    }
}

/// Reads the odd or even data bits of a TCH burst into `data`, leaving its other
/// positions unchanged, and returns the matching stealing flag.
pub fn tch_burst_unmap(
    burst: &[i8; TCH_BURST_BITS],
    data: &mut [i8; TCH_BURST_DATA_BITS],
    odd: bool,
) -> i8 {
    for index in (usize::from(odd)..TCH_BURST_DATA_BITS).step_by(2) {
        data[index] = burst[tch_burst_position(index)];
    }
    burst[tch_stealing_flag(odd)]
}
