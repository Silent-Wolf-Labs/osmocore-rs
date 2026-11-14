// SPDX-License-Identifier: GPL-3.0-or-later

//! Key derivation: the GSM Kc128 of 3GPP TS 33.102, and the LTE K_ASME, K_eNB, NH and
//! NAS algorithm keys of 3GPP TS 33.401.
//!
//! Each key comes from the generic key derivation function of 3GPP TS 33.220 Annex B: an
//! HMAC-SHA-256, keyed with the key it derives from, over a function code (FC) followed
//! by each parameter and its two-byte length. Needs the `auth` feature.
//!
//! - [`kc128`] gives A5/4 and GEA4 their 128-bit key from a UMTS CK and IK.
//! - [`kasme`] gives the LTE root key from CK, IK, the serving network and the concealed
//!   SQN of the vector's AUTN.
//! - [`kenb`], [`nh`] and [`knas`] derive the base station and NAS keys from K_ASME.
//!
//! As elsewhere in [`crate::gsm::auth`], the derivation isn't constant-time, and keys
//! don't clear their memory when dropped.
//!
//! ```
//! use osmocore_rs::gsm::auth::{Ck, Ik};
//! use osmocore_rs::gsm::kdf::{self, NasKey};
//! let kc128 = kdf::kc128(&Ck::new([0; 16]), &Ik::new([0; 16]));
//! assert_eq!(kc128.as_bytes()[..4], [0xb1, 0x8d, 0x08, 0xc0]);
//!
//! let ck = Ck::new(std::array::from_fn(|i| i as u8));
//! let ik = Ik::new(std::array::from_fn(|i| 16 + i as u8));
//! // MCC 001, MNC 01, and an AUTN's SQN xor AK.
//! let kasme = kdf::kasme(&ck, &ik, &[0x00, 0xf1, 0x10], &[0xa5, 0x5a, 0x0f, 0xf0, 0x3c, 0xe2]);
//! assert_eq!(kasme.as_bytes()[..4], [0x8d, 0x43, 0x28, 0x67]);
//! let kenb = kdf::kenb(&kasme, 0);
//! let integrity = kdf::knas(&kasme, NasKey::Integrity, 2);
//! // Each NH comes from K_eNB or the NH before it.
//! let first = kdf::nh(&kasme, kenb.as_bytes());
//! let second = kdf::nh(&kasme, first.as_bytes());
//! # let _ = (integrity, second);
//! ```

use crate::gsm::auth::{Ck, Ik, fixed_key};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::fmt;

/// Bytes in K_ASME, K_eNB and NH.
pub const KASME_BYTES: usize = 32;
/// Bytes in a NAS algorithm key, and in Kc128.
pub const KNAS_BYTES: usize = 16;
/// Bytes in a PLMN identity, as 3GPP TS 24.008 encodes it.
pub const PLMN_ID_BYTES: usize = 3;
/// Bytes in a sequence number concealed by its anonymity key.
pub const SQN_BYTES: usize = 6;

fixed_key!(
    /// A 128-bit GSM cipher key, as A5/4 and GEA4 take it.
    Kc128,
    16
);
fixed_key!(
    /// The LTE key shared by the UE and the MME, from which the others derive.
    Kasme,
    32
);
fixed_key!(
    /// The key of the eNodeB the UE is attached to.
    Kenb,
    32
);
fixed_key!(
    /// A next hop parameter, for the keys of a target eNodeB at handover.
    Nh,
    32
);
fixed_key!(
    /// A NAS encryption or integrity key.
    Knas,
    16
);

/// The NAS algorithm key to derive. Its value is the algorithm type distinguisher of 3GPP
/// TS 33.401 Annex A.7.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NasKey {
    /// The key for NAS encryption (EEA).
    Encryption = 0x01,
    /// The key for NAS integrity protection (EIA).
    Integrity = 0x02,
}

impl fmt::Display for NasKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Encryption => "NAS encryption",
            Self::Integrity => "NAS integrity",
        })
    }
}

/// The function codes (FC) of 3GPP TS 33.102 Annex B.5 and TS 33.401 Annex A.
mod fc {
    pub const KC128: u8 = 0x32;
    pub const KASME: u8 = 0x10;
    pub const KENB: u8 = 0x11;
    pub const NH: u8 = 0x12;
    pub const ALGORITHM_KEY: u8 = 0x15;
}

/// The generic KDF of 3GPP TS 33.220 Annex B.2: HMAC-SHA-256 under `key` over FC, then
/// each parameter followed by its length in two bytes, big-endian.
fn derive(key: &[u8], fc: u8, parameters: &[&[u8]]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(&[fc]);
    for parameter in parameters {
        let len = u16::try_from(parameter.len()).expect("parameters fit a two-byte length");
        mac.update(parameter);
        mac.update(&len.to_be_bytes());
    }
    mac.finalize().into_bytes().into()
}

/// The key of CK followed by IK, as both derivations from them use it.
fn ck_ik(ck: &Ck, ik: &Ik) -> [u8; 32] {
    let mut key = [0; 32];
    key[..16].copy_from_slice(ck.as_bytes());
    key[16..].copy_from_slice(ik.as_bytes());
    key
}

/// Kc128 for A5/4 and GEA4, from a UMTS CK and IK (3GPP TS 33.102 Annex B.5). The
/// derivation has no parameters: only its FC.
pub fn kc128(ck: &Ck, ik: &Ik) -> Kc128 {
    let derived = derive(&ck_ik(ck, ik), fc::KC128, &[]);
    Kc128::new(derived[..16].try_into().expect("16 bytes"))
}

/// K_ASME from CK, IK, the serving network's PLMN identity and SQN xor AK (3GPP TS 33.401
/// Annex A.2). SQN xor AK is the first six bytes of the vector's AUTN.
pub fn kasme(
    ck: &Ck,
    ik: &Ik,
    plmn_id: &[u8; PLMN_ID_BYTES],
    sqn_xor_ak: &[u8; SQN_BYTES],
) -> Kasme {
    Kasme::new(derive(&ck_ik(ck, ik), fc::KASME, &[plmn_id, sqn_xor_ak]))
}

/// K_eNB from K_ASME and the uplink NAS COUNT (3GPP TS 33.401 Annex A.3).
pub fn kenb(kasme: &Kasme, uplink_nas_count: u32) -> Kenb {
    Kenb::new(derive(
        kasme.as_bytes(),
        fc::KENB,
        &[&uplink_nas_count.to_be_bytes()],
    ))
}

/// An NH from K_ASME and its synchronisation input: K_eNB for the first NH, and the NH
/// before it for each next one (3GPP TS 33.401 Annex A.4).
pub fn nh(kasme: &Kasme, sync_input: &[u8; KASME_BYTES]) -> Nh {
    Nh::new(derive(kasme.as_bytes(), fc::NH, &[sync_input]))
}

/// A NAS encryption or integrity key from K_ASME and the algorithm's identity: 1 for
/// EEA1 or EIA1, 2 for EEA2 or EIA2, and so on (3GPP TS 33.401 Annex A.7). The key is the
/// last 16 bytes of the derivation.
pub fn knas(kasme: &Kasme, key: NasKey, algorithm: u8) -> Knas {
    Knas::new(algorithm_key(kasme.as_bytes(), key as u8, algorithm))
}

/// The algorithm key derivation for any distinguisher and identity byte, as C takes them.
pub(crate) fn algorithm_key(key: &[u8; 32], distinguisher: u8, algorithm: u8) -> [u8; 16] {
    let derived = derive(key, fc::ALGORITHM_KEY, &[&[distinguisher], &[algorithm]]);
    derived[16..].try_into().expect("16 bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_keys() {
        let kasme = Kasme::new([0xab; 32]);
        assert_eq!(format!("{kasme:?}"), "Kasme(..)");
        let knas = knas(&kasme, NasKey::Encryption, 1);
        assert_eq!(format!("{knas:?}"), "Knas(..)");
        assert_eq!(NasKey::Integrity.to_string(), "NAS integrity");
    }
}
