// SPDX-License-Identifier: GPL-3.0-or-later

//! COMP128v1. A 32-byte state holds Ki in its first half and RAND, or the previous
//! round's result, in its second. Each of eight rounds compresses the state through five
//! levels of table lookups. The first seven then spread the resulting nibbles over the
//! second half with a fixed bit permutation, and the last gives SRES and Kc.

use super::tables::{LEVEL_0, LEVEL_1, LEVEL_2, LEVEL_3, LEVEL_4};
use crate::gsm::auth::{GsmVector, KC_BYTES, Ki, RAND_BYTES, SRES_BYTES};

const ROUNDS: usize = 8;
const HALF: usize = 16;
const NIBBLES: usize = 2 * HALF;
const STATE_BITS: usize = 4 * NIBBLES;

/// One level of compression: pairs of bytes a fixed distance apart are replaced by
/// lookups of their two weighted sums. Each level halves the distance, and the values
/// it leaves are a bit narrower.
fn compress(state: &mut [u8; NIBBLES]) {
    let levels: [&[u8]; 5] = [&LEVEL_0, &LEVEL_1, &LEVEL_2, &LEVEL_3, &LEVEL_4];
    for (level, table) in levels.into_iter().enumerate() {
        let distance = HALF >> level;
        let mask = table.len() - 1;
        for group in (0..NIBBLES).step_by(2 * distance) {
            for a in group..group + distance {
                let b = a + distance;
                let (x, y) = (usize::from(state[a]), usize::from(state[b]));
                state[a] = table[(x + 2 * y) & mask];
                state[b] = table[(2 * x + y) & mask];
            }
        }
    }
}

/// Takes the four low bits of every byte, most significant first, and writes them back
/// over the second half of the state, with bit `i` of the result taken from bit
/// `17 i mod 128` of the nibbles.
fn permute(state: &mut [u8; NIBBLES]) {
    let nibble_bit = |index: usize| state[index / 4] >> (3 - index % 4) & 1;
    let mut second = [0; HALF];
    for (index, byte) in second.iter_mut().enumerate() {
        for bit in 0..8 {
            let source = (8 * index + bit) * 17 % STATE_BITS;
            *byte |= nibble_bit(source) << (7 - bit);
        }
    }
    state[HALF..].copy_from_slice(&second);
}

/// Computes SRES and Kc with COMP128v1. Kc's last 10 bits are zero.
pub fn comp128v1(ki: &Ki, rand: &[u8; RAND_BYTES]) -> GsmVector {
    let mut state = [0; NIBBLES];
    state[HALF..].copy_from_slice(rand);
    for round in 0..ROUNDS {
        state[..HALF].copy_from_slice(ki.as_bytes());
        compress(&mut state);
        if round + 1 < ROUNDS {
            permute(&mut state);
        }
    }

    // SRES is the first eight nibbles. Kc is the 54 bits that start at the third bit of
    // nibble 18 and run to the end, then 10 zero bits.
    let mut output = [0; SRES_BYTES + KC_BYTES];
    for (byte, pair) in output[..SRES_BYTES].iter_mut().zip(state.chunks(2)) {
        *byte = pair[0] << 4 | pair[1];
    }
    let kc_nibbles = &state[18..];
    for (index, byte) in output[SRES_BYTES..SRES_BYTES + 6].iter_mut().enumerate() {
        let nibbles = &kc_nibbles[2 * index..];
        *byte = nibbles[0] << 6 | nibbles[1] << 2 | nibbles[2] >> 2;
    }
    output[SRES_BYTES + 6] = kc_nibbles[12] << 6 | kc_nibbles[13] << 2;
    GsmVector::from_output(&output)
}
