// SPDX-License-Identifier: GPL-3.0-or-later

//! C's authentication API: `osmo_auth_gen_vec2`, `osmo_auth_gen_vec_auts2`, the name
//! helpers and the key conversions, with C's structs, status codes and changes to the
//! subscriber data.
//!
//! C keeps a subscriber's GSM and UMTS data in a union. `OsmoSubAuthData2` keeps them
//! apart, so a GSM algorithm reads `gsm.ki` and Milenage and TUAK read the `umts` fields
//! whatever the type, where C reads whatever the union holds. Milenage and TUAK are
//! available only with the `auth` feature, and return `-ENOENT` without it, as C does for
//! an algorithm it has no implementation of.
//!
//! As in C, a TUAK resync checks AUTS with all 32 bytes of `umts.k`, whatever `k_len`
//! says, so it rejects the AUTS of a USIM with a 16-byte K. The preferred
//! [`UmtsSubscriber::resync`](crate::gsm::auth::UmtsSubscriber::resync) uses K as long as
//! it is.

use crate::gsm::auth::{
    Algorithm, Kc, Ki, SresDerivation, c2_bytes, c3, c4, c5, xor_2g, xor_3g_auts_sqn,
    xor_3g_outputs,
};
use crate::gsm::comp128;

const EINVAL: i32 = -22;
const ENOENT: i32 = -2;
const ENOTSUP: i32 = -95;

/// No authentication.
pub const OSMO_AUTH_TYPE_NONE: i32 = 0;
/// GSM authentication.
pub const OSMO_AUTH_TYPE_GSM: i32 = 1;
/// UMTS authentication.
pub const OSMO_AUTH_TYPE_UMTS: i32 = 2;

/// No algorithm.
pub const OSMO_AUTH_ALG_NONE: i32 = 0;
/// COMP128v1.
pub const OSMO_AUTH_ALG_COMP128V1: i32 = 1;
/// COMP128v2.
pub const OSMO_AUTH_ALG_COMP128V2: i32 = 2;
/// COMP128v3.
pub const OSMO_AUTH_ALG_COMP128V3: i32 = 3;
/// XOR-3G.
pub const OSMO_AUTH_ALG_XOR_3G: i32 = 4;
/// Milenage.
pub const OSMO_AUTH_ALG_MILENAGE: i32 = 5;
/// XOR-2G.
pub const OSMO_AUTH_ALG_XOR_2G: i32 = 6;
/// TUAK.
pub const OSMO_AUTH_ALG_TUAK: i32 = 7;

/// The GSM part of `struct osmo_sub_auth_data2`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GsmAuthData {
    /// The secret key.
    pub ki: [u8; 16],
}

/// The UMTS part of `struct osmo_sub_auth_data2`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UmtsAuthData {
    /// OPc, or OP when `opc_is_op` is set.
    pub opc: [u8; 32],
    /// Bytes of `opc` in use.
    pub opc_len: u8,
    /// The secret key.
    pub k: [u8; 32],
    /// Bytes of `k` in use.
    pub k_len: u8,
    /// The authentication management field.
    pub amf: [u8; 2],
    /// In: the last SQN used. Out: the SQN of the new vector, for Milenage.
    pub sqn: u64,
    /// Nonzero when `opc` holds OP.
    pub opc_is_op: i32,
    /// Bits of SQN that are IND.
    pub ind_bitlen: u32,
    /// This client's IND slot.
    pub ind: u32,
    /// Out: the USIM's SQN from AUTS.
    pub sqn_ms: u64,
}

/// `struct osmo_sub_auth_data2`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OsmoSubAuthData2 {
    /// An `OSMO_AUTH_TYPE_*` value.
    pub auth_type: i32,
    /// An `OSMO_AUTH_ALG_*` value.
    pub algo: i32,
    /// Used when `auth_type` is GSM.
    pub gsm: GsmAuthData,
    /// Used when `auth_type` is UMTS.
    pub umts: UmtsAuthData,
}

/// `struct osmo_auth_vector`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OsmoAuthVector {
    /// The challenge.
    pub rand: [u8; 16],
    /// The authentication token.
    pub autn: [u8; 16],
    /// The cipher key.
    pub ck: [u8; 16],
    /// The integrity key.
    pub ik: [u8; 16],
    /// The response.
    pub res: [u8; 16],
    /// In, for Milenage: 4 or 8. Out, for XOR-3G: 16.
    pub res_len: u8,
    /// The GSM cipher key.
    pub kc: [u8; 8],
    /// The GSM response.
    pub sres: [u8; 4],
    /// A bitmask of `OSMO_AUTH_TYPE_*` values: GSM is 1, UMTS is 2.
    pub auth_types: u32,
}

/// Generates a vector into `vec` and returns 0, or returns a negative code and leaves
/// `vec` alone. Milenage and TUAK also store the new SQN in `aud.umts.sqn`.
pub fn osmo_auth_gen_vec2(
    vec: &mut OsmoAuthVector,
    aud: &mut OsmoSubAuthData2,
    rand: &[u8; 16],
) -> i32 {
    let rc = gen_vec(vec, aud, rand);
    if rc < 0 {
        return rc;
    }
    vec.rand = *rand;
    0
}

/// Checks a USIM's AUTS and generates a vector after its SQN, as
/// [`osmo_auth_gen_vec2`] does. Only Milenage, TUAK and XOR-3G support this.
///
/// When AUTS matches, `aud.umts.sqn_ms` and `aud.umts.sqn` take the USIM's SQN before
/// the vector is generated, so they stay changed if that then fails.
pub fn osmo_auth_gen_vec_auts2(
    vec: &mut OsmoAuthVector,
    aud: &mut OsmoSubAuthData2,
    auts: &[u8; 14],
    rand_auts: &[u8; 16],
    rand: &[u8; 16],
) -> i32 {
    let rc = match aud.algo {
        OSMO_AUTH_ALG_XOR_3G => xor_3g_auts(vec, aud, auts, rand),
        #[cfg(feature = "auth")]
        OSMO_AUTH_ALG_MILENAGE => milenage::gen_vec_auts(vec, aud, auts, rand_auts, rand),
        #[cfg(feature = "auth")]
        OSMO_AUTH_ALG_TUAK => tuak::gen_vec_auts(vec, aud, auts, rand_auts, rand),
        _ => {
            let _ = rand_auts;
            ENOENT
        }
    };
    if rc < 0 {
        return rc;
    }
    vec.rand = *rand;
    0
}

fn gen_vec(vec: &mut OsmoAuthVector, aud: &mut OsmoSubAuthData2, rand: &[u8; 16]) -> i32 {
    let ki = Ki::new(aud.gsm.ki);
    let gsm = match aud.algo {
        OSMO_AUTH_ALG_COMP128V1 => comp128::comp128v1(&ki, rand),
        OSMO_AUTH_ALG_COMP128V2 => comp128::comp128v2(&ki, rand),
        OSMO_AUTH_ALG_COMP128V3 => comp128::comp128v3(&ki, rand),
        OSMO_AUTH_ALG_XOR_2G if aud.auth_type == OSMO_AUTH_TYPE_GSM => xor_2g(&ki, rand),
        OSMO_AUTH_ALG_XOR_2G => return ENOTSUP,
        OSMO_AUTH_ALG_XOR_3G => return xor_3g(vec, aud, rand),
        #[cfg(feature = "auth")]
        OSMO_AUTH_ALG_MILENAGE => return milenage::gen_vec(vec, aud, rand),
        #[cfg(feature = "auth")]
        OSMO_AUTH_ALG_TUAK => return tuak::gen_vec(vec, aud, rand),
        _ => return ENOENT,
    };
    vec.sres = gsm.sres;
    vec.kc = *gsm.kc.as_bytes();
    vec.auth_types = OSMO_AUTH_TYPE_GSM as u32;
    0
}

/// XOR-3G's key: Ki for a GSM subscriber, a 16-byte K for a UMTS one.
fn xor_3g_key(aud: &OsmoSubAuthData2) -> Result<[u8; 16], i32> {
    match aud.auth_type {
        OSMO_AUTH_TYPE_GSM => Ok(aud.gsm.ki),
        OSMO_AUTH_TYPE_UMTS if aud.umts.k_len == 16 => {
            Ok(aud.umts.k[..16].try_into().expect("16 bytes"))
        }
        OSMO_AUTH_TYPE_UMTS => Err(EINVAL),
        _ => Err(ENOTSUP),
    }
}

fn xor_3g(vec: &mut OsmoAuthVector, aud: &OsmoSubAuthData2, rand: &[u8; 16]) -> i32 {
    let k = match xor_3g_key(aud) {
        Ok(k) => k,
        Err(rc) => return rc,
    };
    let outputs = xor_3g_outputs(&k, rand);
    vec.res = outputs.xdout;
    vec.res_len = 16;
    vec.ck = *outputs.ck.as_bytes();
    vec.ik = *outputs.ik.as_bytes();
    vec.sres = c2_bytes(&outputs.xdout, SresDerivation::Fold);
    vec.kc = *c3(&outputs.ck, &outputs.ik).as_bytes();
    if aud.auth_type != OSMO_AUTH_TYPE_UMTS {
        vec.auth_types = OSMO_AUTH_TYPE_GSM as u32;
        return 0;
    }
    let vector = crate::gsm::auth::xor_3g(&k, rand, aud.umts.sqn, &aud.umts.amf);
    vec.autn = vector.autn;
    vec.auth_types = (OSMO_AUTH_TYPE_GSM | OSMO_AUTH_TYPE_UMTS) as u32;
    0
}

fn xor_3g_auts(
    vec: &mut OsmoAuthVector,
    aud: &mut OsmoSubAuthData2,
    auts: &[u8; 14],
    rand: &[u8; 16],
) -> i32 {
    let k = match xor_3g_key(aud) {
        Ok(k) => k,
        Err(rc) => return rc,
    };
    let Some(sqn_ms) = xor_3g_auts_sqn(&k, rand, auts) else {
        return -1;
    };
    aud.umts.sqn_ms = sqn_ms;
    aud.umts.sqn = sqn_ms;
    xor_3g(vec, aud, rand)
}

#[cfg(feature = "auth")]
mod milenage {
    use super::*;
    use crate::gsm::auth::{AuthError, ResLength, milenage_vector, next_sqn, sqn_value};
    use crate::gsm::milenage::{Milenage, opc_from_op};

    fn prepare(aud: &OsmoSubAuthData2) -> Result<Milenage, i32> {
        if aud.umts.k_len != 16 || aud.umts.opc_len != 16 {
            return Err(EINVAL);
        }
        let k: &[u8; 16] = aud.umts.k[..16].try_into().expect("16 bytes");
        let mut opc: [u8; 16] = aud.umts.opc[..16].try_into().expect("16 bytes");
        if aud.auth_type == OSMO_AUTH_TYPE_UMTS && aud.umts.opc_is_op != 0 {
            opc = opc_from_op(k, &opc);
        }
        Ok(Milenage::new(k, &opc))
    }

    pub(super) fn gen_vec(
        vec: &mut OsmoAuthVector,
        aud: &mut OsmoSubAuthData2,
        rand: &[u8; 16],
    ) -> i32 {
        let milenage = match prepare(aud) {
            Ok(milenage) => milenage,
            Err(rc) => return rc,
        };
        let res_length = match vec.res_len {
            4 => ResLength::Four,
            8 => ResLength::Eight,
            _ => return EINVAL,
        };
        let next = match next_sqn(aud.umts.sqn, aud.umts.ind_bitlen, aud.umts.ind) {
            Ok(next) => next,
            Err(AuthError::IndBitlen(_)) => return -2,
            Err(_) => return -3,
        };
        let vector = milenage_vector(&milenage, &aud.umts.amf, next, rand, ResLength::Eight);
        vec.autn = vector.autn;
        vec.ik = *vector.ik.as_bytes();
        vec.ck = *vector.ck.as_bytes();
        // C writes all 8 bytes of f2's RES and leaves the RES length as given.
        vec.res[..8].copy_from_slice(vector.res.as_bytes());
        vec.kc = *vector.kc.as_bytes();
        vec.sres = c2_bytes(&vec.res[..res_length.bytes()], SresDerivation::Fold);
        vec.auth_types = (OSMO_AUTH_TYPE_GSM | OSMO_AUTH_TYPE_UMTS) as u32;
        aud.umts.sqn = next;
        0
    }

    pub(super) fn gen_vec_auts(
        vec: &mut OsmoAuthVector,
        aud: &mut OsmoSubAuthData2,
        auts: &[u8; 14],
        rand_auts: &[u8; 16],
        rand: &[u8; 16],
    ) -> i32 {
        let milenage = match prepare(aud) {
            Ok(milenage) => milenage,
            Err(rc) => return rc,
        };
        let Ok(sqn) = milenage.auts_sqn(rand_auts, auts) else {
            return -1;
        };
        aud.umts.sqn_ms = sqn_value(&sqn);
        aud.umts.sqn = aud.umts.sqn_ms;
        gen_vec(vec, aud, rand)
    }
}

#[cfg(feature = "auth")]
mod tuak {
    use super::*;
    use crate::gsm::auth::{AuthError, K, ResLength, next_sqn, sqn_value, tuak_vector};
    use crate::gsm::tuak::{Tuak, topc_from_top};
    use std::num::NonZeroU32;

    /// K, of `k_len` bytes, and TOPc, derived from OP for a UMTS subscriber.
    fn keys(aud: &OsmoSubAuthData2) -> Result<(K, [u8; 32]), i32> {
        if !matches!(aud.umts.k_len, 16 | 32) || aud.umts.opc_len != 32 {
            return Err(EINVAL);
        }
        let k = K::new(&aud.umts.k[..usize::from(aud.umts.k_len)]).expect("16 or 32 bytes");
        let mut topc = aud.umts.opc;
        if aud.auth_type == OSMO_AUTH_TYPE_UMTS && aud.umts.opc_is_op != 0 {
            topc = topc_from_top(&k, &topc, NonZeroU32::MIN);
        }
        Ok((k, topc))
    }

    pub(super) fn gen_vec(
        vec: &mut OsmoAuthVector,
        aud: &mut OsmoSubAuthData2,
        rand: &[u8; 16],
    ) -> i32 {
        let res_length = match vec.res_len {
            4 => ResLength::Four,
            8 => ResLength::Eight,
            16 => ResLength::Sixteen,
            _ => return EINVAL,
        };
        let (k, topc) = match keys(aud) {
            Ok(keys) => keys,
            Err(rc) => return rc,
        };
        let next = match next_sqn(aud.umts.sqn, aud.umts.ind_bitlen, aud.umts.ind) {
            Ok(next) => next,
            Err(AuthError::IndBitlen(_)) => return -2,
            Err(_) => return -3,
        };
        let vector = tuak_vector(&Tuak::new(&k, &topc), &aud.umts.amf, next, rand, res_length);
        vec.autn = vector.autn;
        vec.ik = *vector.ik.as_bytes();
        vec.ck = *vector.ck.as_bytes();
        // C writes RES only as long as asked, and leaves the length as given.
        vec.res[..res_length.bytes()].copy_from_slice(vector.res.as_bytes());
        vec.kc = *vector.kc.as_bytes();
        vec.sres = vector.sres;
        vec.auth_types = (OSMO_AUTH_TYPE_GSM | OSMO_AUTH_TYPE_UMTS) as u32;
        aud.umts.sqn = next;
        0
    }

    pub(super) fn gen_vec_auts(
        vec: &mut OsmoAuthVector,
        aud: &mut OsmoSubAuthData2,
        auts: &[u8; 14],
        rand_auts: &[u8; 16],
        rand: &[u8; 16],
    ) -> i32 {
        let topc = match keys(aud) {
            Ok((_, topc)) => topc,
            Err(rc) => return rc,
        };
        // C passes the size of the key buffer, not k_len.
        let whole_buffer = K::new(&aud.umts.k).expect("32 bytes");
        let Ok(sqn) = Tuak::new(&whole_buffer, &topc).auts_sqn(rand_auts, auts) else {
            return -1;
        };
        aud.umts.sqn_ms = sqn_value(&sqn);
        aud.umts.sqn = aud.umts.sqn_ms;
        gen_vec(vec, aud, rand)
    }
}

/// The name of an `OSMO_AUTH_ALG_*` value, or `unknown 0x..` in hex.
pub fn osmo_auth_alg_name(algo: i32) -> String {
    match u8::try_from(algo).ok().and_then(Algorithm::from_code) {
        Some(algorithm) => algorithm.name().to_owned(),
        None if algo == OSMO_AUTH_ALG_NONE => "None".to_owned(),
        None => format!("unknown 0x{:x}", algo as u32),
    }
}

/// The `OSMO_AUTH_ALG_*` value for a name, ignoring ASCII case, or -22.
pub fn osmo_auth_alg_parse(name: &str) -> i32 {
    if name.eq_ignore_ascii_case("None") {
        return OSMO_AUTH_ALG_NONE;
    }
    Algorithm::parse(name).map_or(EINVAL, |algorithm| i32::from(algorithm.code()))
}

/// The name of an `OSMO_AUTH_TYPE_*` value from `osmo_sub_auth_type_names`, or
/// `unknown 0x..`.
pub fn osmo_sub_auth_type_name(auth_type: i32) -> String {
    match auth_type {
        OSMO_AUTH_TYPE_NONE => "None".to_owned(),
        OSMO_AUTH_TYPE_GSM => "GSM".to_owned(),
        OSMO_AUTH_TYPE_UMTS => "UMTS".to_owned(),
        _ => format!("unknown 0x{:x}", auth_type as u32),
    }
}

/// C2 with derivation function 1 (fold) or 2 (truncate). Returns 0, or -22 with `sres`
/// left alone where C aborts: for another function or a RES over 16 bytes.
pub fn osmo_auth_c2(sres: &mut [u8; 4], res: &[u8], sres_deriv_func: u8) -> i32 {
    let derivation = match sres_deriv_func {
        1 => SresDerivation::Fold,
        2 => SresDerivation::Truncate,
        _ => return EINVAL,
    };
    if res.len() > 16 {
        return EINVAL;
    }
    *sres = c2_bytes(res, derivation);
    0
}

/// C3: Kc from CK and IK.
pub fn osmo_auth_c3(kc: &mut [u8; 8], ck: &[u8; 16], ik: &[u8; 16]) {
    *kc = *c3(&(*ck).into(), &(*ik).into()).as_bytes();
}

/// C4: CK from Kc.
pub fn osmo_c4(ck: &mut [u8; 16], kc: &[u8; 8]) {
    *ck = *c4(&Kc::new(*kc)).as_bytes();
}

/// Fills CK and IK of a GSM-only vector from its Kc, by C4 and C5, and returns 1.
/// Returns 0 and changes nothing for a vector that isn't GSM-only.
pub fn osmo_auth_3g_from_2g(vec: &mut OsmoAuthVector) -> i32 {
    let (gsm, umts) = (OSMO_AUTH_TYPE_GSM as u32, OSMO_AUTH_TYPE_UMTS as u32);
    if vec.auth_types & gsm == 0 || vec.auth_types & umts != 0 {
        return 0;
    }
    let kc = Kc::new(vec.kc);
    vec.ik = *c5(&kc).as_bytes();
    vec.ck = *c4(&kc).as_bytes();
    1
}
