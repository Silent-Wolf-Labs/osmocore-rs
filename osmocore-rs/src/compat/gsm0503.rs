// SPDX-License-Identifier: GPL-3.0-or-later

use crate::gsm0503::{XCCH_BURSTS_BITS, XCCH_L2_BYTES, xcch_decode, xcch_encode};

/// Encodes a frame into `bursts` and returns 0.
pub fn gsm0503_xcch_encode(
    bursts: &mut [u8; XCCH_BURSTS_BITS],
    l2_data: &[u8; XCCH_L2_BYTES],
) -> i32 {
    *bursts = xcch_encode(l2_data);
    0
}

/// Decodes into `l2_data` and returns 0, or returns -1 if the parity check fails,
/// leaving `l2_data` unchanged. Sets both counts either way.
pub fn gsm0503_xcch_decode(
    l2_data: &mut [u8; XCCH_L2_BYTES],
    bursts: &[i8; XCCH_BURSTS_BITS],
    n_errors: &mut i32,
    n_bits_total: &mut i32,
) -> i32 {
    let decoded = xcch_decode(bursts);
    *n_errors = decoded.bit_errors as i32;
    *n_bits_total = decoded.total_bits as i32;
    match decoded.data {
        Some(data) => {
            *l2_data = data;
            0
        }
        None => -1,
    }
}
