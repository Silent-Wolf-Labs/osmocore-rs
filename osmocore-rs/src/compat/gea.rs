// SPDX-License-Identifier: GPL-3.0-or-later

//! C's GPRS ciphering API from `gprs_cipher.h`: `gprs_cipher_run` with its status
//! codes and limits, the input helpers, key lengths and names.
//!
//! C selects ciphers through registered plugins; here GEA3 and GEA4 are always there and
//! the others never are, as in a C build with no plugins loaded.

use crate::gsm::auth::{Kc, c4};
use crate::gsm::gea::{GEA3_KEY_BYTES, GEA4_KEY_BYTES, GprsAlgorithm, MAX_FRAME_BYTES};
use crate::gsm::kasumi::{KEY_BYTES, kgcore_register, register};

const EINVAL: i32 = -22;
const ERANGE: i32 = -34;

/// GEA0, no ciphering.
pub const GPRS_ALGO_GEA0: i32 = 0;
/// GEA1.
pub const GPRS_ALGO_GEA1: i32 = 1;
/// GEA2.
pub const GPRS_ALGO_GEA2: i32 = 2;
/// GEA3.
pub const GPRS_ALGO_GEA3: i32 = 3;
/// GEA4.
pub const GPRS_ALGO_GEA4: i32 = 4;
/// Mobile to SGSN.
pub const GPRS_CIPH_MS2SGSN: u32 = 0;
/// SGSN to mobile.
pub const GPRS_CIPH_SGSN2MS: u32 = 1;
/// The longest block `gprs_cipher_run` takes.
pub const GSM0464_CIPH_MAX_BLOCK: u16 = MAX_FRAME_BYTES as u16;

/// KGCORE's CA input for GEA3 and GEA4.
const GEA_CA: u8 = 0xff;

fn algorithm(algo: i32) -> Option<GprsAlgorithm> {
    u8::try_from(algo).ok().and_then(GprsAlgorithm::from_number)
}

/// Writes `len` bytes of keystream to `out` and returns 0.
///
/// Returns -34 for an algorithm number out of range, -22 for GEA0, GEA1 or GEA2, and
/// then -34 for a block longer than 1,523 bytes. Takes the direction whole: from 2 on it
/// overlaps the bearer bits, as in C. Returns -22 where C would read or write out of
/// bounds: for a key shorter than the algorithm's, or an output shorter than `len`. An
/// error leaves `out` alone.
pub fn gprs_cipher_run(out: &mut [u8], len: u16, algo: i32, kc: &[u8], iv: u32, dir: u32) -> i32 {
    let Some(algorithm) = algorithm(algo) else {
        return ERANGE;
    };
    if !algorithm.is_implemented() {
        return EINVAL;
    }
    if len > GSM0464_CIPH_MAX_BLOCK {
        return ERANGE;
    }
    let (Some(out), Some(key)) = (
        out.get_mut(..usize::from(len)),
        kc.get(..algorithm.key_bytes()),
    ) else {
        return EINVAL;
    };
    let key: [u8; KEY_BYTES] = match algorithm {
        GprsAlgorithm::Gea3 => {
            let kc: [u8; GEA3_KEY_BYTES] = key.try_into().expect("checked length");
            *c4(&Kc::new(kc)).as_bytes()
        }
        _ => <[u8; GEA4_KEY_BYTES]>::try_from(key).expect("checked length"),
    };
    // C passes the direction to KGCORE as a byte.
    kgcore_register(&key, register(GEA_CA, 0, iv, dir as u8), out);
    0
}

/// Returns 1 for GEA3 and GEA4, 0 for GEA0 to GEA2 and -34 out of range.
pub fn gprs_cipher_supported(algo: i32) -> i32 {
    algorithm(algo).map_or(ERANGE, |algorithm| i32::from(algorithm.is_implemented()))
}

/// Returns the bytes in an algorithm's key, or 0 for GEA0 and out of range.
pub fn gprs_cipher_key_length(algo: i32) -> u32 {
    algorithm(algo).map_or(0, |algorithm| algorithm.key_bytes() as u32)
}

/// The name of an algorithm from `gprs_cipher_names`, or `None` out of range.
pub fn gprs_cipher_name(algo: i32) -> Option<&'static str> {
    algorithm(algo).map(GprsAlgorithm::name)
}

/// The input for an unconfirmed frame. A SAPI from 16 on overflows an `int` in C; here
/// it wraps.
pub fn gprs_cipher_gen_input_ui(iov_ui: u32, sapi: u8, lfn: u32, oc: u32) -> u32 {
    let sx = (1u32 << 27)
        .wrapping_mul(u32::from(sapi))
        .wrapping_add(1 << 31);
    (iov_ui ^ sx).wrapping_add(lfn).wrapping_add(oc)
}

/// The input for an acknowledged frame.
pub fn gprs_cipher_gen_input_i(iov_i: u32, lfn: u32, oc: u32) -> u32 {
    crate::gsm::gea::input_i(iov_i, lfn, oc)
}
