// SPDX-License-Identifier: GPL-3.0-or-later

//! Placing data bits and stealing flags in bursts.

use super::coding::{HALF_BURST, XCCH_BURST_BITS, XCCH_BURST_DATA_BITS};

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
