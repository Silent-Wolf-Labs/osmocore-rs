// SPDX-License-Identifier: GPL-3.0-or-later

//! COMP128 v2 and v3. Both run the same eight rounds over Ki and RAND, each stored
//! last byte first. v2 then clears the last 10 bits of Kc.

use super::tables::{T0, T1};
use crate::gsm::auth::{GsmVector, KC_BYTES, Ki, RAND_BYTES, SRES_BYTES};

const ROUNDS: usize = 8;
const LEVELS: usize = 5;
const HALF: usize = 16;

fn substitute(value: u8, key: u8) -> u8 {
    T0[usize::from(T1[usize::from(value)] ^ key)]
}

fn reversed(bytes: &[u8; HALF]) -> [u8; HALF] {
    let mut out = *bytes;
    out.reverse();
    out
}

/// One round: mixes the round key into the state through five butterfly levels, then
/// picks every 19th bit of the result as the new state.
fn round(state: &[u8; HALF], key: &[u8; HALF]) -> [u8; HALF] {
    let mut mix = [0; 2 * HALF];
    mix[..HALF].copy_from_slice(state);
    mix[HALF..].copy_from_slice(key);
    for level in 0..LEVELS {
        let mut temp = [0; HALF];
        for (index, value) in temp.iter_mut().enumerate() {
            *value = substitute(mix[HALF + index], mix[index]);
        }
        // Groups of 2^level bytes, interleaved: the stride grows each level.
        for offset in 0..1 << level {
            for group in 0..HALF >> level {
                let from = (group << level) + offset;
                mix[((2 * group + 1) << level) + offset] = substitute(temp[from], mix[from + HALF]);
                mix[(group << (level + 1)) + offset] = temp[from];
            }
        }
    }

    let mut out = [0; HALF];
    for (index, byte) in out.iter_mut().enumerate() {
        for bit in 0..8 {
            let position = (19 * (8 * index + bit) + 19) % 256;
            let source = mix[position / 8] >> ((3 * bit + 3) % 8) & 1;
            *byte |= source << bit;
        }
    }
    out
}

/// The 16-byte output of v3, before it's split into SRES and Kc.
fn output(ki: &Ki, rand: &[u8; RAND_BYTES]) -> [u8; HALF] {
    let mut state = reversed(rand);
    let mut key = reversed(ki.as_bytes());
    for (key, value) in key.iter_mut().zip(&state) {
        *key ^= value;
    }
    for _ in 0..ROUNDS {
        state = round(&state, &key);
    }
    reversed(&state)
}

/// Computes SRES and Kc with COMP128v3, which gives a 64-bit Kc.
pub fn comp128v3(ki: &Ki, rand: &[u8; RAND_BYTES]) -> GsmVector {
    let full = output(ki, rand);
    // Bytes 4 to 7 aren't used.
    let mut out = [0; SRES_BYTES + KC_BYTES];
    out[..SRES_BYTES].copy_from_slice(&full[..SRES_BYTES]);
    out[SRES_BYTES..].copy_from_slice(&full[HALF - KC_BYTES..]);
    GsmVector::from_output(&out)
}

/// Computes SRES and Kc with COMP128v2. It's v3 with Kc's last 10 bits cleared.
pub fn comp128v2(ki: &Ki, rand: &[u8; RAND_BYTES]) -> GsmVector {
    let GsmVector { sres, kc } = comp128v3(ki, rand);
    let mut kc = *kc.as_bytes();
    kc[6] &= 0xfc;
    kc[7] = 0;
    GsmVector {
        sres,
        kc: kc.into(),
    }
}
