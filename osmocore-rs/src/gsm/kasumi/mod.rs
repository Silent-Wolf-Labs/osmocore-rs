// SPDX-License-Identifier: GPL-3.0-or-later

//! The KASUMI block cipher and KGCORE, the keystream function that A5/3, A5/4, GEA3
//! and GEA4 are built on.
//!
//! [`Kasumi`] encrypts 64-bit blocks with a 128-bit key, as 3GPP TS 35.202 defines it.
//! [`kgcore`] runs it in the output feedback mode of the A5/3 and GEA3 specification
//! (3GPP TS 55.216) to give a keystream of any length. The code isn't constant-time and doesn't clear keys from
//! memory, as in libosmocore.
//!
//! ```
//! use osmocore_rs::gsm::kasumi::{Bearer, Kasumi, KgcoreInput, kgcore};
//!
//! // 3GPP TS 35.203 test set 1.
//! let key = [
//!     0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xb3, 0x00,
//!     0x95, 0x2c, 0x49, 0x10, 0x48, 0x81, 0xff, 0x48,
//! ];
//! assert_eq!(Kasumi::new(&key).encrypt(0xea024714ad5c4d84), 0xdf1f9b251c0bf45f);
//!
//! let input = KgcoreInput { ca: 0x0f, bearer: Bearer::ZERO, count: 0x24f20f, direction: false };
//! let mut keystream = [0; 4];
//! kgcore(&[0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xbc, 0x00, 0x2b, 0xd6, 0x45, 0x9f,
//!     0x82, 0xc5, 0xbc, 0x00], &input, &mut keystream);
//! assert_eq!(keystream, [0x88, 0x9e, 0xea, 0xaf]);
//! ```

mod sboxes;

use std::fmt;

use sboxes::{S7, S9};

/// Bytes in a KASUMI key.
pub const KEY_BYTES: usize = 16;

/// The constants each key word is XORed with to give the modified key words K'
/// (3GPP TS 35.202 section 4.6, table 2).
const KEY_CONSTANTS: [u16; 8] = [
    0x0123, 0x4567, 0x89ab, 0xcdef, 0xfedc, 0xba98, 0x7654, 0x3210,
];

/// The key modifier KM that KGCORE XORs into every key byte for its first block
/// (3GPP TS 55.216).
const KEY_MODIFIER: u8 = 0x55;

const ROUNDS: usize = 8;

/// The subkeys of each of the eight rounds, derived from a key as 3GPP TS 35.202
/// section 4.6 describes. Index `i` holds round `i + 1`'s subkey.
///
/// Its `Debug` output doesn't show the subkeys.
#[derive(Clone, PartialEq, Eq)]
pub struct Subkeys {
    /// KL1, the first FL subkey.
    pub kl1: [u16; ROUNDS],
    /// KL2, the second FL subkey.
    pub kl2: [u16; ROUNDS],
    /// KO1, the first FO subkey.
    pub ko1: [u16; ROUNDS],
    /// KO2, the second FO subkey.
    pub ko2: [u16; ROUNDS],
    /// KO3, the third FO subkey.
    pub ko3: [u16; ROUNDS],
    /// KI1, the first FI subkey.
    pub ki1: [u16; ROUNDS],
    /// KI2, the second FI subkey.
    pub ki2: [u16; ROUNDS],
    /// KI3, the third FI subkey.
    pub ki3: [u16; ROUNDS],
}

impl Subkeys {
    /// Derives the subkeys of a key.
    pub fn new(key: &[u8; KEY_BYTES]) -> Self {
        let words: [u16; ROUNDS] =
            std::array::from_fn(|j| u16::from_be_bytes([key[2 * j], key[2 * j + 1]]));
        let modified: [u16; ROUNDS] = std::array::from_fn(|j| words[j] ^ KEY_CONSTANTS[j]);
        // Round i takes key words from i onwards, wrapping round.
        let word =
            |i: usize, offset: usize, rotate: u32| words[(i + offset) % 8].rotate_left(rotate);
        let modified = |i: usize, offset: usize| modified[(i + offset) % 8];
        Self {
            kl1: std::array::from_fn(|i| word(i, 0, 1)),
            kl2: std::array::from_fn(|i| modified(i, 2)),
            ko1: std::array::from_fn(|i| word(i, 1, 5)),
            ko2: std::array::from_fn(|i| word(i, 5, 8)),
            ko3: std::array::from_fn(|i| word(i, 6, 13)),
            ki1: std::array::from_fn(|i| modified(i, 4)),
            ki2: std::array::from_fn(|i| modified(i, 3)),
            ki3: std::array::from_fn(|i| modified(i, 7)),
        }
    }
}

impl fmt::Debug for Subkeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Subkeys(..)")
    }
}

/// KASUMI with one key's subkeys derived.
///
/// Its `Debug` output doesn't show the subkeys.
#[derive(Clone, PartialEq, Eq)]
pub struct Kasumi {
    subkeys: Subkeys,
}

impl Kasumi {
    /// Derives a key's subkeys.
    pub fn new(key: &[u8; KEY_BYTES]) -> Self {
        Self::from_subkeys(Subkeys::new(key))
    }

    /// Uses subkeys given directly.
    pub const fn from_subkeys(subkeys: Subkeys) -> Self {
        Self { subkeys }
    }

    /// Returns the subkeys.
    pub const fn subkeys(&self) -> &Subkeys {
        &self.subkeys
    }

    /// Encrypts one 64-bit block, with its first byte in the top bits.
    pub fn encrypt(&self, block: u64) -> u64 {
        let (mut left, mut right) = ((block >> 32) as u32, block as u32);
        for round in 0..ROUNDS {
            // Odd rounds (the first is round 1) apply FL then FO; even rounds FO then FL.
            let mixed = if round % 2 == 0 {
                self.fo(round, self.fl(round, left))
            } else {
                self.fl(round, self.fo(round, left))
            };
            (left, right) = (right ^ mixed, left);
        }
        u64::from(left) << 32 | u64::from(right)
    }

    fn fl(&self, round: usize, input: u32) -> u32 {
        let s = &self.subkeys;
        let (left, right) = ((input >> 16) as u16, input as u16);
        let right = right ^ (left & s.kl1[round]).rotate_left(1);
        let left = left ^ (right | s.kl2[round]).rotate_left(1);
        u32::from(left) << 16 | u32::from(right)
    }

    fn fo(&self, round: usize, input: u32) -> u32 {
        let s = &self.subkeys;
        let (mut left, mut right) = ((input >> 16) as u16, input as u16);
        for (ko, ki) in [
            (s.ko1[round], s.ki1[round]),
            (s.ko2[round], s.ki2[round]),
            (s.ko3[round], s.ki3[round]),
        ] {
            (left, right) = (right, fi(left ^ ko, ki) ^ right);
        }
        u32::from(left) << 16 | u32::from(right)
    }
}

impl fmt::Debug for Kasumi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Kasumi(..)")
    }
}

/// FI: splits 16 bits into 9 and 7, and mixes them through S9 and S7 with a subkey
/// (3GPP TS 35.202 section 4.4).
fn fi(input: u16, subkey: u16) -> u16 {
    let nine = |value: u16| usize::from(value & 0x1ff);
    let seven = |value: u16| usize::from(value & 0x7f);
    let (left, right) = (input >> 7, input & 0x7f);
    let (left, right) = (right, S9[nine(left)] ^ right);
    let (left, right) = (
        right ^ (subkey & 0x1ff),
        u16::from(S7[seven(left)]) ^ (right & 0x7f) ^ (subkey >> 9),
    );
    let (left, right) = (right, S9[nine(left)] ^ right);
    let (left, right) = (u16::from(S7[seven(left)]) ^ (right & 0x7f), right);
    left << 9 | right
}

/// A 5-bit radio bearer identity, as KGCORE's CB input takes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Bearer(u8);

impl Bearer {
    /// Bearer 0, which A5/3, A5/4, GEA3 and GEA4 use.
    pub const ZERO: Self = Self(0);
    /// The highest bearer.
    pub const MAX: Self = Self(31);

    /// Returns a bearer, or `None` past 31.
    pub const fn new(bearer: u8) -> Option<Self> {
        if bearer <= Self::MAX.0 {
            Some(Self(bearer))
        } else {
            None
        }
    }

    /// Returns the bearer's number.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// KGCORE's inputs other than the key (3GPP TS 55.216). CE, the extra 16 bits for ECSD,
/// is always 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KgcoreInput {
    /// CA, the 8 bits that tell the users of KGCORE apart: 0x0f for A5/3 and A5/4,
    /// 0xff for GEA3 and GEA4.
    pub ca: u8,
    /// CB, the bearer.
    pub bearer: Bearer,
    /// CC, the 32-bit count.
    pub count: u32,
    /// CD, the direction bit.
    pub direction: bool,
}

impl KgcoreInput {
    /// The 64-bit register KGCORE starts from: CC, CB, CD, two zero bits, CA and CE.
    fn register(&self) -> u64 {
        register(self.ca, self.bearer.0, self.count, u8::from(self.direction))
    }
}

/// Builds KGCORE's register. A bearer past 5 bits or a direction past 1 bit overlaps the
/// fields above it, as in C.
pub(crate) fn register(ca: u8, cb: u8, cc: u32, cd: u8) -> u64 {
    u64::from(cc) << 32 | (u64::from(cb) << 3 | u64::from(cd) << 2) << 24 | u64::from(ca) << 16
}

/// Fills `out` with KGCORE's keystream for a key and its inputs. The keystream's first
/// bit is the top bit of `out[0]`.
pub fn kgcore(key: &[u8; KEY_BYTES], input: &KgcoreInput, out: &mut [u8]) {
    kgcore_register(key, input.register(), out);
}

pub(crate) fn kgcore_register(key: &[u8; KEY_BYTES], register: u64, out: &mut [u8]) {
    let modified = key.map(|byte| byte ^ KEY_MODIFIER);
    let whitened = Kasumi::new(&modified).encrypt(register);
    let cipher = Kasumi::new(key);
    let mut block = 0;
    for (counter, chunk) in (0u64..).zip(out.chunks_mut(8)) {
        block = cipher.encrypt(whitened ^ counter ^ block);
        chunk.copy_from_slice(&block.to_be_bytes()[..chunk.len()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sboxes_are_permutations() {
        let mut seven = S7.to_vec();
        seven.sort_unstable();
        assert!(seven.iter().copied().eq(0..128));
        let mut nine = S9.to_vec();
        nine.sort_unstable();
        assert!(nine.iter().copied().eq(0..512));
        // The worked examples of 3GPP TS 35.202 section 4.5.
        assert_eq!(S7[38], 58);
        assert_eq!(S9[138], 339);
    }

    #[test]
    fn bearers_have_five_bits() {
        assert_eq!(Bearer::new(31), Some(Bearer::MAX));
        assert_eq!(Bearer::new(32), None);
        assert_eq!(Bearer::new(7).map(Bearer::get), Some(7));
    }

    #[test]
    fn debug_hides_subkeys() {
        let kasumi = Kasumi::new(&[0xa5; KEY_BYTES]);
        assert_eq!(format!("{kasumi:?}"), "Kasumi(..)");
        assert_eq!(format!("{:?}", kasumi.subkeys()), "Subkeys(..)");
    }
}
