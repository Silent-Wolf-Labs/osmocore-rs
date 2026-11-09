// SPDX-License-Identifier: GPL-3.0-or-later

use crate::gsm::a5::{
    A5_4_KEY_BYTES, CipherStream, KEY_BYTES, STREAM_BITS, a5_0, a5_1, a5_2, a5_3, a5_4, frame_count,
};

const EINVAL_RESULT: i32 = -22;
const ENOTSUP_RESULT: i32 = -95;

/// Returns a frame number's T1, T3 and T2 as the count the A5 ciphers load.
pub const fn osmo_a5_fn_count(frame: u32) -> u32 {
    frame_count(frame)
}

/// Writes A5/`n`'s stream to the directions given and returns 0.
///
/// A5/4 reads the first 16 bytes of `key` and the others its first 8. Returns -95 for a
/// number with no algorithm. Returns -22 where C crashes or reads past the key: for
/// A5/1 to A5/4 with no key, or one shorter than the algorithm reads. An error leaves
/// the outputs alone.
pub fn osmo_a5(
    n: i32,
    key: Option<&[u8]>,
    frame: u32,
    dl: Option<&mut [u8; STREAM_BITS]>,
    ul: Option<&mut [u8; STREAM_BITS]>,
) -> i32 {
    fn keyed<const N: usize>(
        key: Option<&[u8]>,
        frame: u32,
        cipher: fn(&[u8; N], u32) -> CipherStream,
    ) -> Result<CipherStream, i32> {
        key.and_then(|key| key.get(..N))
            .map(|key| cipher(key.try_into().expect("checked length"), frame))
            .ok_or(EINVAL_RESULT)
    }
    let stream = match n {
        0 => Ok(a5_0()),
        1 => keyed::<KEY_BYTES>(key, frame, a5_1),
        2 => keyed::<KEY_BYTES>(key, frame, a5_2),
        3 => keyed::<KEY_BYTES>(key, frame, a5_3),
        4 => keyed::<A5_4_KEY_BYTES>(key, frame, a5_4),
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
