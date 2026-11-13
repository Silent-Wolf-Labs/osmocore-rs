// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM and UMTS authentication.
//!
//! In GSM (A3/A8), a subscriber key Ki and the network's random challenge RAND give the
//! signed response (SRES) the mobile sends back and the cipher key (Kc) for A5.
//! [`GsmAlgorithm`] selects COMP128 v1, v2 or v3 ([`crate::gsm::comp128`]), XOR-2G or
//! XOR-3G.
//!
//! In UMTS (AKA), a [`UmtsSubscriber`] also holds a sequence number, and each challenge
//! gives an authentication token (AUTN) for the mobile to check, its expected response
//! (RES), cipher and integrity keys (CK and IK), and the GSM SRES and Kc derived from them.
//! [`UmtsAlgorithm`] selects Milenage or TUAK, with the `auth` feature, or XOR-3G.
//!
//! COMP128v1 is broken, since a SIM's Ki can be recovered from its answers, and XOR-2G and
//! XOR-3G are meant only for test SIMs. Use them to interoperate with networks and SIMs,
//! not to protect anything.
//!
//! Nothing here claims to run in constant time, and key types don't clear their memory
//! when dropped, as in libosmocore.
//!
//! ```
//! use osmocore_rs::gsm::auth::{GsmAlgorithm, Ki};
//! let ki = Ki::new([0xaa; 16]);
//! let vector = GsmAlgorithm::Comp128v3.vector(&ki, &[0; 16]);
//! assert_eq!(vector.sres, [0x34, 0xb4, 0x22, 0x5b]);
//! assert_eq!(vector.kc.as_bytes(), &[0xf1, 0x6b, 0x96, 0xe1, 0x18, 0xa8, 0x59, 0x86]);
//! ```

mod keys;
mod umts;

pub use keys::{Ck, Ik, K, Kc, KeyLengthError, Ki, Op, Opc};
pub use umts::{
    AUTN_BYTES, AUTS_BYTES, AuthError, MAX_IND_BITLEN, OperatorKey, Res, ResLength, Resync,
    UmtsAlgorithm, UmtsSubscriber, UmtsVector, next_sqn,
};
#[cfg(feature = "auth")]
pub(crate) use umts::{milenage_vector, sqn_value, tuak_vector};
pub(crate) use umts::{xor_3g, xor_3g_auts_sqn, xor_3g_outputs};

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
/// Bytes in a UMTS cipher or integrity key.
pub const CK_BYTES: usize = 16;

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
            kc: Kc::new(kc.try_into().expect("8 bytes")),
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
    /// XOR-3G, the test algorithm of 3GPP TS 34.108 section 8.1.2, with the GSM SRES and
    /// Kc derived from its RES, CK and IK.
    Xor3g,
}

impl GsmAlgorithm {
    /// Computes SRES and Kc for a challenge.
    pub fn vector(self, ki: &Ki, rand: &[u8; RAND_BYTES]) -> GsmVector {
        match self {
            Self::Comp128v1 => comp128::comp128v1(ki, rand),
            Self::Comp128v2 => comp128::comp128v2(ki, rand),
            Self::Comp128v3 => comp128::comp128v3(ki, rand),
            Self::Xor2g => xor_2g(ki, rand),
            Self::Xor3g => umts::xor_3g_gsm(ki.as_bytes(), rand),
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

/// Every authentication algorithm libosmocore names, with its C name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Algorithm {
    /// COMP128v1.
    Comp128v1,
    /// COMP128v2.
    Comp128v2,
    /// COMP128v3.
    Comp128v3,
    /// XOR-3G.
    Xor3g,
    /// Milenage.
    Milenage,
    /// XOR-2G.
    Xor2g,
    /// TUAK.
    Tuak,
}

impl Algorithm {
    const ALL: [Self; 7] = [
        Self::Comp128v1,
        Self::Comp128v2,
        Self::Comp128v3,
        Self::Xor3g,
        Self::Milenage,
        Self::Xor2g,
        Self::Tuak,
    ];

    /// The name libosmocore uses, such as `COMP128v1` or `MILENAGE`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Comp128v1 => "COMP128v1",
            Self::Comp128v2 => "COMP128v2",
            Self::Comp128v3 => "COMP128v3",
            Self::Xor3g => "XOR-3G",
            Self::Milenage => "MILENAGE",
            Self::Xor2g => "XOR-2G",
            Self::Tuak => "TUAK",
        }
    }

    /// Finds an algorithm by name, ignoring ASCII case, as C does. `None` isn't an
    /// algorithm, so it isn't found.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|algorithm| algorithm.name().eq_ignore_ascii_case(name))
    }

    /// The value of C's `enum osmo_auth_algo`.
    pub const fn code(self) -> u8 {
        match self {
            Self::Comp128v1 => 1,
            Self::Comp128v2 => 2,
            Self::Comp128v3 => 3,
            Self::Xor3g => 4,
            Self::Milenage => 5,
            Self::Xor2g => 6,
            Self::Tuak => 7,
        }
    }

    /// The algorithm for a C `enum osmo_auth_algo` value, if it names one.
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|algorithm| algorithm.code() == code)
    }
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl From<GsmAlgorithm> for Algorithm {
    fn from(algorithm: GsmAlgorithm) -> Self {
        match algorithm {
            GsmAlgorithm::Comp128v1 => Self::Comp128v1,
            GsmAlgorithm::Comp128v2 => Self::Comp128v2,
            GsmAlgorithm::Comp128v3 => Self::Comp128v3,
            GsmAlgorithm::Xor2g => Self::Xor2g,
            GsmAlgorithm::Xor3g => Self::Xor3g,
        }
    }
}

impl From<UmtsAlgorithm> for Algorithm {
    fn from(algorithm: UmtsAlgorithm) -> Self {
        match algorithm {
            #[cfg(feature = "auth")]
            UmtsAlgorithm::Milenage => Self::Milenage,
            #[cfg(feature = "auth")]
            UmtsAlgorithm::Tuak => Self::Tuak,
            UmtsAlgorithm::Xor3g => Self::Xor3g,
        }
    }
}

/// How [`c2`] derives SRES from RES.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SresDerivation {
    /// Function 1 of 3GPP TS 33.102 section 6.8.1.2: the XOR of RES's four 32-bit words,
    /// with RES zero-padded to 16 bytes.
    Fold,
    /// Function 2: the first 4 bytes of RES, zero-padded.
    Truncate,
}

/// C2: derives a GSM SRES from a UMTS RES.
pub fn c2(res: &Res, derivation: SresDerivation) -> [u8; SRES_BYTES] {
    c2_bytes(res.as_bytes(), derivation)
}

/// C2 of up to 16 bytes.
pub(crate) fn c2_bytes(res: &[u8], derivation: SresDerivation) -> [u8; SRES_BYTES] {
    let mut padded = [0; 16];
    padded[..res.len()].copy_from_slice(res);
    let mut sres = [0; SRES_BYTES];
    match derivation {
        SresDerivation::Fold => {
            for word in padded.chunks(SRES_BYTES) {
                for (out, byte) in sres.iter_mut().zip(word) {
                    *out ^= byte;
                }
            }
        }
        SresDerivation::Truncate => sres.copy_from_slice(&padded[..SRES_BYTES]),
    }
    sres
}

/// C3: derives a GSM Kc from UMTS CK and IK, folding each to 8 bytes.
pub fn c3(ck: &Ck, ik: &Ik) -> Kc {
    let (ck, ik) = (ck.as_bytes(), ik.as_bytes());
    Kc::new(std::array::from_fn(|i| {
        ck[i] ^ ck[i + KC_BYTES] ^ ik[i] ^ ik[i + KC_BYTES]
    }))
}

/// C4: derives a UMTS CK from a GSM Kc, by repeating it.
pub fn c4(kc: &Kc) -> Ck {
    let kc = kc.as_bytes();
    Ck::new(std::array::from_fn(|i| kc[i % KC_BYTES]))
}

/// C5: derives a UMTS IK from a GSM Kc: the XOR of its halves, Kc itself, then the XOR
/// again.
pub fn c5(kc: &Kc) -> Ik {
    let kc = kc.as_bytes();
    let folded: [u8; 4] = std::array::from_fn(|i| kc[i] ^ kc[i + 4]);
    let mut ik = [0; CK_BYTES];
    ik[..4].copy_from_slice(&folded);
    ik[4..12].copy_from_slice(kc);
    ik[12..].copy_from_slice(&folded);
    Ik::new(ik)
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

    #[test]
    fn names_round_trip() {
        for algorithm in Algorithm::ALL {
            assert_eq!(Algorithm::parse(algorithm.name()), Some(algorithm));
            assert_eq!(Algorithm::from_code(algorithm.code()), Some(algorithm));
        }
        assert_eq!(Algorithm::parse("none"), None);
        assert_eq!(Algorithm::from_code(0), None);
    }
}
