// SPDX-License-Identifier: GPL-3.0-or-later

use crate::gsm::a5::{CipherStream, KEY_BYTES, STREAM_BITS, a5_0, a5_1, a5_2, frame_count};

const EINVAL_RESULT: i32 = -22;
const ENOTSUP_RESULT: i32 = -95;

/// Returns a frame number's T1, T3 and T2 as the count the A5 ciphers load.
pub const fn osmo_a5_fn_count(frame: u32) -> u32 {
    frame_count(frame)
}

/// Writes A5/`n`'s stream to the directions given and returns 0.
///
/// Returns -95 for a number with no algorithm, and for A5/3 and A5/4, which C supports
/// and this crate doesn't. Returns -22 for A5/1 or A5/2 with no key, where C crashes.
/// An error leaves the outputs alone.
pub fn osmo_a5(
    n: i32,
    key: Option<&[u8; KEY_BYTES]>,
    frame: u32,
    dl: Option<&mut [u8; STREAM_BITS]>,
    ul: Option<&mut [u8; STREAM_BITS]>,
) -> i32 {
    let keyed = |cipher: fn(&[u8; KEY_BYTES], u32) -> CipherStream| {
        key.map(|key| cipher(key, frame)).ok_or(EINVAL_RESULT)
    };
    let stream = match n {
        0 => Ok(a5_0()),
        1 => keyed(a5_1),
        2 => keyed(a5_2),
        _ => Err(ENOTSUP_RESULT),
    };
    match stream {
        Ok(stream) => {
            if let Some(dl) = dl {
                *dl = stream.downlink;
            }
            if let Some(ul) = ul {
                *ul = stream.uplink;
            }
            0
        }
        Err(rc) => rc,
    }
}
