// SPDX-License-Identifier: GPL-3.0-or-later

//! The GPRS ciphers GEA3 and GEA4, and the LLC frame input they take.
//!
//! Each gives a keystream to XOR with an LLC frame's information field, from a key, a
//! 32-bit input and the direction (3GPP TS 44.064 annex A). GEA3 takes the 64-bit Kc
//! and GEA4 a 128-bit key; both run [KGCORE](crate::gsm::kasumi). libosmocore doesn't
//! implement GEA1 or GEA2, and nor does this crate. The code isn't constant-time and
//! doesn't clear keys from memory, as in libosmocore.
//!
//! ```
//! use osmocore_rs::gsm::gea::{Direction, gea3};
//!
//! // 3GPP TS 55.217 test set 1.
//! let mut keystream = [0; 4];
//! gea3(&[0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xbc, 0x00], 0x8e9421a3, Direction::Uplink, &mut keystream);
//! assert_eq!(keystream, [0x5f, 0x35, 0x97, 0x09]);
//! ```

use crate::gsm::auth::{Kc, c4};
use crate::gsm::kasumi::{self, Bearer, KgcoreInput};

/// Bytes in a GEA3 key.
pub const GEA3_KEY_BYTES: usize = 8;
/// Bytes in a GEA4 key.
pub const GEA4_KEY_BYTES: usize = 16;
/// The longest frame, in bytes, that C's `gprs_cipher_run` ciphers (3GPP TS 44.064).
pub const MAX_FRAME_BYTES: usize = 1523;

/// KGCORE's CA input for GEA3 and GEA4.
const GEA_CA: u8 = 0xff;

/// The direction a frame travels in (3GPP TS 44.064 table A.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Mobile to SGSN.
    Uplink,
    /// SGSN to mobile.
    Downlink,
}

impl Direction {
    /// Returns the direction bit: 0 uplink, 1 downlink.
    pub const fn bit(self) -> u8 {
        match self {
            Self::Uplink => 0,
            Self::Downlink => 1,
        }
    }
}

/// A GPRS ciphering algorithm (3GPP TS 24.008 section 10.5.5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GprsAlgorithm {
    /// No ciphering.
    Gea0,
    /// GEA1, which libosmocore doesn't implement.
    Gea1,
    /// GEA2, which libosmocore doesn't implement.
    Gea2,
    /// GEA3, on KASUMI with a 64-bit key.
    Gea3,
    /// GEA4, on KASUMI with a 128-bit key.
    Gea4,
}

impl GprsAlgorithm {
    /// Every algorithm, in the order of their numbers.
    pub const ALL: [Self; 5] = [Self::Gea0, Self::Gea1, Self::Gea2, Self::Gea3, Self::Gea4];

    /// Returns the algorithm with a number from 0 to 4.
    pub const fn from_number(number: u8) -> Option<Self> {
        match number {
            0 => Some(Self::Gea0),
            1 => Some(Self::Gea1),
            2 => Some(Self::Gea2),
            3 => Some(Self::Gea3),
            4 => Some(Self::Gea4),
            _ => None,
        }
    }

    /// Returns the algorithm's number.
    pub const fn number(self) -> u8 {
        self as u8
    }

    /// Returns the name, `GEA0` to `GEA4`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gea0 => "GEA0",
            Self::Gea1 => "GEA1",
            Self::Gea2 => "GEA2",
            Self::Gea3 => "GEA3",
            Self::Gea4 => "GEA4",
        }
    }

    /// Returns the bytes in the algorithm's key: none for GEA0, 16 for GEA4 and 8 for
    /// the others.
    pub const fn key_bytes(self) -> usize {
        match self {
            Self::Gea0 => 0,
            Self::Gea1 | Self::Gea2 | Self::Gea3 => GEA3_KEY_BYTES,
            Self::Gea4 => GEA4_KEY_BYTES,
        }
    }

    /// Returns whether this crate implements the algorithm: GEA3 and GEA4.
    pub const fn is_implemented(self) -> bool {
        matches!(self, Self::Gea3 | Self::Gea4)
    }
}

/// Fills `out` with GEA3's keystream. GEA3 is GEA4 with the key repeated, by C4.
pub fn gea3(kc: &[u8; GEA3_KEY_BYTES], input: u32, direction: Direction, out: &mut [u8]) {
    gea4(c4(&Kc::new(*kc)).as_bytes(), input, direction, out);
}

/// Fills `out` with GEA4's keystream.
pub fn gea4(key: &[u8; GEA4_KEY_BYTES], input: u32, direction: Direction, out: &mut [u8]) {
    let input = KgcoreInput {
        ca: GEA_CA,
        bearer: Bearer::ZERO,
        count: input,
        direction: direction == Direction::Downlink,
    };
    kasumi::kgcore(key, &input, out);
}

/// The input for an unconfirmed (UI) frame: IOV-UI XORed with the SAPI, then the LLC
/// frame number and overflow counter added (3GPP TS 44.064 section A.2.1). Returns
/// `None` for a SAPI past 15, which has no 4-bit form.
pub const fn input_ui(iov_ui: u32, sapi: u8, lfn: u32, oc: u32) -> Option<u32> {
    if sapi > 15 {
        return None;
    }
    let sx = (sapi as u32) << 27 | 1 << 31;
    Some((iov_ui ^ sx).wrapping_add(lfn).wrapping_add(oc))
}

/// The input for an acknowledged (I) frame: IOV-I plus the LLC frame number and
/// overflow counter (3GPP TS 44.064 section A.2.1).
pub const fn input_i(iov_i: u32, lfn: u32, oc: u32) -> u32 {
    iov_i.wrapping_add(lfn).wrapping_add(oc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithms_round_trip_their_numbers() {
        for (number, algorithm) in (0..).zip(GprsAlgorithm::ALL) {
            assert_eq!(GprsAlgorithm::from_number(number), Some(algorithm));
            assert_eq!(algorithm.number(), number);
        }
        assert_eq!(GprsAlgorithm::from_number(5), None);
    }

    #[test]
    fn ui_input_needs_a_four_bit_sapi() {
        assert_eq!(input_ui(0, 15, 0, 0), Some(0xf800_0000));
        assert_eq!(input_ui(0, 16, 0, 0), None);
    }
}
