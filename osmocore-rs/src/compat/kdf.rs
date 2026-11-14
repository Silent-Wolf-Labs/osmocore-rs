// SPDX-License-Identifier: GPL-3.0-or-later

//! C's key derivation functions, `osmo_kdf_kc128` to `osmo_kdf_nas`, with C's argument
//! order and output buffers. Needs the `auth` feature.
//!
//! `osmo_kdf_kasme` takes SQN and AK apart, and `osmo_kdf_nas` takes any byte for the
//! algorithm type, where the preferred [`crate::gsm::kdf::knas`] takes a
//! [`NasKey`](crate::gsm::kdf::NasKey).

use crate::gsm::auth::{Ck, Ik};
use crate::gsm::kdf::{self, Kasme};

/// `osmo_kdf_kc128`: Kc128 from CK and IK.
pub fn osmo_kdf_kc128(ck: &[u8; 16], ik: &[u8; 16], kc128: &mut [u8; 16]) {
    *kc128 = *kdf::kc128(&Ck::new(*ck), &Ik::new(*ik)).as_bytes();
}

/// `osmo_kdf_kasme`: K_ASME from CK, IK, the PLMN identity, SQN and AK.
pub fn osmo_kdf_kasme(
    ck: &[u8; 16],
    ik: &[u8; 16],
    plmn_id: &[u8; 3],
    sqn: &[u8; 6],
    ak: &[u8; 6],
    kasme: &mut [u8; 32],
) {
    let concealed = std::array::from_fn(|i| sqn[i] ^ ak[i]);
    *kasme = *kdf::kasme(&Ck::new(*ck), &Ik::new(*ik), plmn_id, &concealed).as_bytes();
}

/// `osmo_kdf_enb`: K_eNB from K_ASME and the uplink NAS COUNT.
pub fn osmo_kdf_enb(kasme: &[u8; 32], ul_count: u32, kenb: &mut [u8; 32]) {
    *kenb = *kdf::kenb(&Kasme::new(*kasme), ul_count).as_bytes();
}

/// `osmo_kdf_nh`: an NH from K_ASME and the synchronisation input.
pub fn osmo_kdf_nh(kasme: &[u8; 32], sync_input: &[u8; 32], nh: &mut [u8; 32]) {
    *nh = *kdf::nh(&Kasme::new(*kasme), sync_input).as_bytes();
}

/// `osmo_kdf_nas`: the algorithm key for any type and identity byte.
pub fn osmo_kdf_nas(algo_type: u8, algo_id: u8, kasme: &[u8; 32], knas: &mut [u8; 16]) {
    *knas = kdf::algorithm_key(kasme, algo_type, algo_id);
}
