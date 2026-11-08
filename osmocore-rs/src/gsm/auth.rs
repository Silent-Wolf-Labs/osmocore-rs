// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM authentication (A3/A8): a subscriber key and the network's random challenge give
//! the signed response (SRES) the mobile sends back and the cipher key (Kc) for A5.
//!
//! [`GsmAlgorithm`] selects COMP128 v1, v2 or v3 ([`crate::gsm::comp128`]) or the XOR-2G
//! test algorithm. COMP128v1 is broken, since a SIM's Ki can be recovered from its
//! answers, and XOR-2G is meant only for test SIMs. Use them to interoperate with
//! networks and SIMs, not to protect anything.
//!
//! None of these functions run in constant time, and key types don't clear their
//! memory when dropped, as in libosmocore.
//!
//! ```
//! use osmocore_rs::gsm::auth::{GsmAlgorithm, Ki};
//! let ki = Ki::new([0xaa; 16]);
//! let vector = GsmAlgorithm::Comp128v3.vector(&ki, &[0; 16]);
//! assert_eq!(vector.sres, [0x34, 0xb4, 0x22, 0x5b]);
//! assert_eq!(vector.kc.as_bytes(), &[0xf1, 0x6b, 0x96, 0xe1, 0x18, 0xa8, 0x59, 0x86]);
//! ```

use crate::gsm::comp128;
use std::fmt;

/// Bytes in a GSM subscriber key.
pub const KI_BYTES: usize = 16;
/// Bytes in the random challenge.
pub const RAND_BYTES: usize = 16;
/// Bytes in the signed response.
pub const SRES_BYTES: usize = 4;
/// Bytes in a GSM cipher key.
pub const KC_BYTES: usize = 8;

/// A GSM subscriber's secret key. `Debug` doesn't show it.
#[derive(Clone, PartialEq, Eq)]
pub struct Ki([u8; KI_BYTES]);

impl Ki {
    /// Wraps a key.
    pub const fn new(bytes: [u8; KI_BYTES]) -> Self {
        Self(bytes)
    }

    /// Returns the key.
    pub const fn as_bytes(&self) -> &[u8; KI_BYTES] {
        &self.0
    }
}

impl From<[u8; KI_BYTES]> for Ki {
    fn from(bytes: [u8; KI_BYTES]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for Ki {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Ki(..)")
    }
}

/// A GSM cipher key, as A5/1 to A5/3 take it. `Debug` doesn't show it.
#[derive(Clone, PartialEq, Eq)]
pub struct Kc([u8; KC_BYTES]);

impl Kc {
    /// Wraps a key.
    pub const fn new(bytes: [u8; KC_BYTES]) -> Self {
        Self(bytes)
    }

    /// Returns the key.
    pub const fn as_bytes(&self) -> &[u8; KC_BYTES] {
        &self.0
    }
}

impl From<[u8; KC_BYTES]> for Kc {
    fn from(bytes: [u8; KC_BYTES]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for Kc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Kc(..)")
    }
}

/// The result of GSM authentication for one challenge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GsmVector {
    /// The signed response.
    pub sres: [u8; SRES_BYTES],
    /// The cipher key.
    pub kc: Kc,
}

impl GsmVector {
    /// Splits the first 12 bytes of an algorithm's output into SRES and Kc.
    pub(crate) fn from_output(output: &[u8; SRES_BYTES + KC_BYTES]) -> Self {
        let (sres, kc) = output.split_at(SRES_BYTES);
        Self {
            sres: sres.try_into().expect("4 bytes"),
            kc: Kc(kc.try_into().expect("8 bytes")),
        }
    }
}

/// The GSM authentication algorithms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GsmAlgorithm {
    /// COMP128v1. Its Kc has 54 bits: the last 10 are zero.
    Comp128v1,
    /// COMP128v2. Its Kc has 54 bits: the last 10 are zero.
    Comp128v2,
    /// COMP128v3, with a 64-bit Kc.
    Comp128v3,
    /// XOR-2G, the test algorithm of 3GPP TS 51.010-1 annex 4.
    Xor2g,
}

impl GsmAlgorithm {
    /// Computes SRES and Kc for a challenge.
    pub fn vector(self, ki: &Ki, rand: &[u8; RAND_BYTES]) -> GsmVector {
        match self {
            Self::Comp128v1 => comp128::comp128v1(ki, rand),
            Self::Comp128v2 => comp128::comp128v2(ki, rand),
            Self::Comp128v3 => comp128::comp128v3(ki, rand),
            Self::Xor2g => xor_2g(ki, rand),
        }
    }
}

/// XOR-2G: SRES and Kc are the first 12 bytes of Ki XOR RAND.
pub fn xor_2g(ki: &Ki, rand: &[u8; RAND_BYTES]) -> GsmVector {
    let mut output = [0; SRES_BYTES + KC_BYTES];
    for ((out, key), challenge) in output.iter_mut().zip(ki.as_bytes()).zip(rand) {
        *out = key ^ challenge;
    }
    GsmVector::from_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_keys() {
        // SRES is zero and Kc is 0x5a (90) repeated.
        let mut rand = [0; RAND_BYTES];
        rand[..SRES_BYTES].fill(0x5a);
        let vector = xor_2g(&Ki::new([0x5a; KI_BYTES]), &rand);
        assert_eq!(format!("{:?}", Ki::new([0x5a; KI_BYTES])), "Ki(..)");
        let shown = format!("{vector:?}");
        assert!(shown.contains("Kc(..)"), "{shown}");
        assert!(!shown.contains("90"), "{shown}");
    }
}
