// SPDX-License-Identifier: GPL-3.0-or-later

//! UMTS authentication vectors: the subscriber, its sequence number and XOR-3G.

use super::{
    CK_BYTES, Ck, GsmVector, Ik, K, KC_BYTES, Kc, Op, Opc, RAND_BYTES, SRES_BYTES, SresDerivation,
    c2, c3,
};
use std::fmt;

/// Bytes in an authentication token.
pub const AUTN_BYTES: usize = 16;
/// Bytes in a resynchronization token.
pub const AUTS_BYTES: usize = 14;
/// The largest IND length C allows. The rest of SQN's 48 bits is SEQ.
pub const MAX_IND_BITLEN: u32 = 28;
/// SQN has 48 bits.
const SQN_BYTES: usize = 6;

/// The UMTS authentication algorithms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UmtsAlgorithm {
    /// Milenage (3GPP TS 35.206). Needs the `auth` feature.
    #[cfg(feature = "auth")]
    Milenage,
    /// TUAK (3GPP TS 35.231), with a 16 or 32-byte K and a 32-byte operator key. Needs the
    /// `auth` feature.
    #[cfg(feature = "auth")]
    Tuak,
    /// XOR-3G, the test algorithm of 3GPP TS 34.108 section 8.1.2. It ignores the operator
    /// key, and uses SQN as given, without incrementing it.
    Xor3g,
}

/// The operator key, as OP or as the OPc already derived from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperatorKey {
    /// OP: OPc is derived from it and K for each vector.
    Op(Op),
    /// OPc.
    Opc(Opc),
}

impl OperatorKey {
    #[cfg(feature = "auth")]
    fn len(&self) -> usize {
        match self {
            Self::Op(key) => key.as_bytes().len(),
            Self::Opc(key) => key.as_bytes().len(),
        }
    }
}

/// How long RES is. Milenage gives 4 or 8 bytes and TUAK 4, 8 or 16; XOR-3G always gives
/// 16.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResLength {
    /// 32 bits.
    Four,
    /// 64 bits.
    Eight,
    /// 128 bits, for TUAK.
    Sixteen,
}

impl ResLength {
    /// The length in bytes.
    pub const fn bytes(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }
}

/// A UMTS response of 4 to 16 bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Res {
    bytes: [u8; 16],
    len: u8,
}

impl Res {
    /// Wraps a response of 4 to 16 bytes.
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if !(4..=16).contains(&bytes.len()) {
            return None;
        }
        let mut stored = [0; 16];
        stored[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            bytes: stored,
            len: bytes.len() as u8,
        })
    }

    /// Returns the response.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len()]
    }

    /// Bytes in the response.
    pub fn len(&self) -> usize {
        usize::from(self.len)
    }

    /// Always false: a response has at least 4 bytes.
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl fmt::Debug for Res {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Res({:02x?})", self.as_bytes())
    }
}

/// One UMTS authentication vector, with the GSM SRES and Kc derived from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UmtsVector {
    /// The authentication token for the USIM: SQN hidden with AK, AMF and MAC-A.
    pub autn: [u8; AUTN_BYTES],
    /// The cipher key.
    pub ck: Ck,
    /// The integrity key.
    pub ik: Ik,
    /// The expected response.
    pub res: Res,
    /// The GSM response, by C2 function 1 of RES.
    pub sres: [u8; SRES_BYTES],
    /// The GSM cipher key, by C3 of CK and IK.
    pub kc: Kc,
}

/// The outcome of a resync.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resync {
    /// The USIM's SQN, from its AUTS.
    pub sqn_ms: u64,
    /// A vector after it.
    pub vector: UmtsVector,
}

/// Why a UMTS vector can't be generated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// Milenage and XOR-3G need a 16-byte K.
    KeyLength,
    /// Milenage needs a 16-byte OP or OPc, and TUAK a 32-byte one.
    OperatorKeyLength,
    /// Milenage gives RES of at most 8 bytes.
    ResLength(ResLength),
    /// The IND length is above [`MAX_IND_BITLEN`].
    IndBitlen(u32),
    /// IND doesn't fit in its length.
    IndRange {
        /// The IND slot.
        ind: u32,
        /// Its length in bits.
        ind_bitlen: u32,
    },
    /// The AUTS MAC-S doesn't match.
    AutsMismatch,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyLength => f.write_str("the algorithm needs a 16-byte K"),
            Self::OperatorKeyLength => {
                f.write_str("the operator key has the wrong length for the algorithm")
            }
            Self::ResLength(length) => {
                write!(f, "the algorithm has no {}-byte RES", length.bytes())
            }
            Self::IndBitlen(bits) => {
                write!(
                    f,
                    "IND has {bits} bits, above the limit of {MAX_IND_BITLEN}"
                )
            }
            Self::IndRange { ind, ind_bitlen } => {
                write!(f, "IND {ind} doesn't fit in {ind_bitlen} bits")
            }
            Self::AutsMismatch => f.write_str("the AUTS MAC-S doesn't match"),
        }
    }
}

impl std::error::Error for AuthError {}

/// A UMTS subscriber's keys and sequence number, as an authentication centre keeps them.
///
/// [`UmtsSubscriber::vector`] and [`UmtsSubscriber::resync`] change only `sqn`, and only
/// when they succeed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UmtsSubscriber {
    /// The algorithm.
    pub algorithm: UmtsAlgorithm,
    /// The secret key.
    pub k: K,
    /// The operator key. XOR-3G doesn't use it.
    pub operator: OperatorKey,
    /// The authentication management field, which goes in AUTN.
    pub amf: [u8; 2],
    /// The last SQN used. Milenage and TUAK use the next one for each vector and store it
    /// here.
    pub sqn: u64,
    /// How many low bits of SQN are IND, the slot each client gets. 0 makes Milenage and
    /// TUAK add 1 to SQN for each vector.
    pub ind_bitlen: u32,
    /// This client's IND slot.
    pub ind: u32,
}

impl UmtsSubscriber {
    /// A subscriber with AMF 0, SQN 0 and no IND.
    pub fn new(algorithm: UmtsAlgorithm, k: K, operator: OperatorKey) -> Self {
        Self {
            algorithm,
            k,
            operator,
            amf: [0; 2],
            sqn: 0,
            ind_bitlen: 0,
            ind: 0,
        }
    }

    /// Generates a vector for a challenge. Milenage and TUAK move `sqn` on to the next
    /// value with this client's IND, and use that; XOR-3G uses `sqn` as it is, and always
    /// gives a 16-byte RES. TUAK runs Keccak once per function, as C does.
    // Only Milenage and TUAK use the RES length.
    #[cfg_attr(not(feature = "auth"), allow(unused_variables))]
    pub fn vector(
        &mut self,
        rand: &[u8; RAND_BYTES],
        res_length: ResLength,
    ) -> Result<UmtsVector, AuthError> {
        match self.algorithm {
            #[cfg(feature = "auth")]
            UmtsAlgorithm::Milenage => {
                let milenage = self.milenage(res_length)?;
                let next = next_sqn(self.sqn, self.ind_bitlen, self.ind)?;
                let vector = milenage_vector(&milenage, &self.amf, next, rand, res_length);
                self.sqn = next;
                Ok(vector)
            }
            #[cfg(feature = "auth")]
            UmtsAlgorithm::Tuak => {
                let tuak = self.tuak()?;
                let next = next_sqn(self.sqn, self.ind_bitlen, self.ind)?;
                let vector = tuak_vector(&tuak, &self.amf, next, rand, res_length);
                self.sqn = next;
                Ok(vector)
            }
            UmtsAlgorithm::Xor3g => Ok(xor_3g(self.k_128()?, rand, self.sqn, &self.amf)),
        }
    }

    /// Resynchronizes from a USIM's AUTS, which it computed for `rand_auts`, then
    /// generates a vector for `rand`. Milenage and TUAK set `sqn` to the USIM's and move
    /// on from there, as [`UmtsSubscriber::vector`] does. XOR-3G checks AUTS against
    /// `rand`, not `rand_auts`, as in C, and uses the USIM's SQN itself in the vector.
    ///
    /// Unlike C, this checks K, the operator key, the RES length and IND before AUTS, and
    /// changes nothing when it fails. TUAK checks AUTS with K as long as it is, where C
    /// always takes 32 bytes of its key buffer, so C rejects the AUTS of a USIM with a
    /// 16-byte K.
    #[cfg_attr(not(feature = "auth"), allow(unused_variables))]
    pub fn resync(
        &mut self,
        auts: &[u8; AUTS_BYTES],
        rand_auts: &[u8; RAND_BYTES],
        rand: &[u8; RAND_BYTES],
        res_length: ResLength,
    ) -> Result<Resync, AuthError> {
        match self.algorithm {
            #[cfg(feature = "auth")]
            UmtsAlgorithm::Milenage => {
                let milenage = self.milenage(res_length)?;
                // Checked before AUTS, so nothing changes when it fails.
                next_sqn(0, self.ind_bitlen, self.ind)?;
                let sqn = milenage
                    .auts_sqn(rand_auts, auts)
                    .map_err(|_| AuthError::AutsMismatch)?;
                let sqn_ms = sqn_value(&sqn);
                let next = next_sqn(sqn_ms, self.ind_bitlen, self.ind)?;
                let vector = milenage_vector(&milenage, &self.amf, next, rand, res_length);
                self.sqn = next;
                Ok(Resync { sqn_ms, vector })
            }
            #[cfg(feature = "auth")]
            UmtsAlgorithm::Tuak => {
                let tuak = self.tuak()?;
                next_sqn(0, self.ind_bitlen, self.ind)?;
                let sqn = tuak
                    .auts_sqn(rand_auts, auts)
                    .map_err(|_| AuthError::AutsMismatch)?;
                let sqn_ms = sqn_value(&sqn);
                let next = next_sqn(sqn_ms, self.ind_bitlen, self.ind)?;
                let vector = tuak_vector(&tuak, &self.amf, next, rand, res_length);
                self.sqn = next;
                Ok(Resync { sqn_ms, vector })
            }
            UmtsAlgorithm::Xor3g => {
                let k = &self.k_128()?.clone();
                let sqn_ms = xor_3g_auts_sqn(k, rand, auts).ok_or(AuthError::AutsMismatch)?;
                self.sqn = sqn_ms;
                Ok(Resync {
                    sqn_ms,
                    vector: xor_3g(k, rand, sqn_ms, &self.amf),
                })
            }
        }
    }

    fn k_128(&self) -> Result<&[u8; 16], AuthError> {
        self.k.as_128().ok_or(AuthError::KeyLength)
    }

    #[cfg(feature = "auth")]
    fn milenage(&self, res_length: ResLength) -> Result<crate::gsm::milenage::Milenage, AuthError> {
        use crate::gsm::milenage::Milenage;
        let k = self.k_128()?;
        if self.operator.len() != 16 {
            return Err(AuthError::OperatorKeyLength);
        }
        if res_length == ResLength::Sixteen {
            return Err(AuthError::ResLength(res_length));
        }
        Ok(match &self.operator {
            OperatorKey::Op(op) => Milenage::with_op(k, op.as_128().expect("16 bytes")),
            OperatorKey::Opc(opc) => Milenage::new(k, opc.as_128().expect("16 bytes")),
        })
    }

    #[cfg(feature = "auth")]
    fn tuak(&self) -> Result<crate::gsm::tuak::Tuak, AuthError> {
        use crate::gsm::tuak::Tuak;
        let wide = |bytes: &[u8]| -> Result<[u8; 32], AuthError> {
            bytes.try_into().map_err(|_| AuthError::OperatorKeyLength)
        };
        Ok(match &self.operator {
            OperatorKey::Op(op) => Tuak::with_top(&self.k, &wide(op.as_bytes())?),
            OperatorKey::Opc(opc) => Tuak::new(&self.k, &wide(opc.as_bytes())?),
        })
    }
}

/// The SQN Milenage uses after `sqn` for an IND slot: SEQ, the bits above IND, goes up by
/// one and IND is set. With no IND bits, SQN goes up by one. It wraps at 2^64, as in C,
/// and AUTN carries only the low 48 bits.
pub fn next_sqn(sqn: u64, ind_bitlen: u32, ind: u32) -> Result<u64, AuthError> {
    if ind_bitlen > MAX_IND_BITLEN {
        return Err(AuthError::IndBitlen(ind_bitlen));
    }
    let step = 1u64 << ind_bitlen;
    if u64::from(ind) >= step {
        return Err(AuthError::IndRange { ind, ind_bitlen });
    }
    Ok((sqn.wrapping_add(step) & !(step - 1)) + u64::from(ind))
}

/// The low 48 bits of SQN, as AUTN carries them.
pub(crate) fn sqn_bytes(sqn: u64) -> [u8; SQN_BYTES] {
    sqn.to_be_bytes()[2..].try_into().expect("6 bytes")
}

pub(crate) fn sqn_value(bytes: &[u8; SQN_BYTES]) -> u64 {
    let mut full = [0; 8];
    full[2..].copy_from_slice(bytes);
    u64::from_be_bytes(full)
}

#[cfg(feature = "auth")]
pub(crate) fn milenage_vector(
    milenage: &crate::gsm::milenage::Milenage,
    amf: &[u8; 2],
    sqn: u64,
    rand: &[u8; RAND_BYTES],
    res_length: ResLength,
) -> UmtsVector {
    let (autn, outputs) = milenage.generate(rand, &sqn_bytes(sqn), amf);
    let res = Res::new(&outputs.res[..res_length.bytes()]).expect("4 or 8 bytes");
    UmtsVector {
        autn,
        sres: c2(&res, SresDerivation::Fold),
        kc: c3(&outputs.ck, &outputs.ik),
        ck: outputs.ck,
        ik: outputs.ik,
        res,
    }
}

#[cfg(feature = "auth")]
pub(crate) fn tuak_vector(
    tuak: &crate::gsm::tuak::Tuak,
    amf: &[u8; 2],
    sqn: u64,
    rand: &[u8; RAND_BYTES],
    res_length: ResLength,
) -> UmtsVector {
    let sqn = sqn_bytes(sqn);
    let (autn, res, ck, ik) = match res_length {
        ResLength::Four => {
            let (autn, out) = tuak.generate::<4>(rand, &sqn, amf);
            (autn, Res::new(&out.res), out.ck, out.ik)
        }
        ResLength::Eight => {
            let (autn, out) = tuak.generate::<8>(rand, &sqn, amf);
            (autn, Res::new(&out.res), out.ck, out.ik)
        }
        ResLength::Sixteen => {
            let (autn, out) = tuak.generate::<16>(rand, &sqn, amf);
            (autn, Res::new(&out.res), out.ck, out.ik)
        }
    };
    let res = res.expect("4 to 16 bytes");
    let (ck, ik) = (Ck::new(ck), Ik::new(ik));
    UmtsVector {
        autn,
        sres: c2(&res, SresDerivation::Fold),
        kc: c3(&ck, &ik),
        ck,
        ik,
        res,
    }
}

/// XOR-3G's outputs, all from XDOUT = K XOR RAND.
pub(crate) struct Xor3gOutputs {
    pub(crate) xdout: [u8; 16],
    pub(crate) ck: Ck,
    pub(crate) ik: Ik,
    pub(crate) ak: [u8; 6],
}

pub(crate) fn xor_3g_outputs(k: &[u8; 16], rand: &[u8; RAND_BYTES]) -> Xor3gOutputs {
    let xdout: [u8; 16] = std::array::from_fn(|i| k[i] ^ rand[i]);
    let rotated = |by: usize| {
        let mut out = xdout;
        out.rotate_left(by);
        out
    };
    Xor3gOutputs {
        xdout,
        ck: Ck::new(rotated(1)),
        ik: Ik::new(rotated(2)),
        ak: xdout[3..9].try_into().expect("6 bytes"),
    }
}

/// XOR-3G's MAC: the first 8 bytes of XDOUT XOR SQN and AMF.
fn xor_3g_mac(xdout: &[u8; 16], sqn: &[u8; SQN_BYTES], amf: &[u8; 2]) -> [u8; 8] {
    std::array::from_fn(|i| {
        xdout[i]
            ^ if i < SQN_BYTES {
                sqn[i]
            } else {
                amf[i - SQN_BYTES]
            }
    })
}

pub(crate) fn xor_3g(k: &[u8; 16], rand: &[u8; RAND_BYTES], sqn: u64, amf: &[u8; 2]) -> UmtsVector {
    let outputs = xor_3g_outputs(k, rand);
    let sqn = sqn_bytes(sqn);
    let mut autn = [0; AUTN_BYTES];
    for (out, (sqn, ak)) in autn.iter_mut().zip(sqn.iter().zip(&outputs.ak)) {
        *out = sqn ^ ak;
    }
    autn[SQN_BYTES..8].copy_from_slice(amf);
    autn[8..].copy_from_slice(&xor_3g_mac(&outputs.xdout, &sqn, amf));
    let res = Res::new(&outputs.xdout).expect("16 bytes");
    UmtsVector {
        autn,
        sres: c2(&res, SresDerivation::Fold),
        kc: c3(&outputs.ck, &outputs.ik),
        ck: outputs.ck,
        ik: outputs.ik,
        res,
    }
}

/// XOR-3G for a GSM subscriber: SRES and Kc from the same RES, CK and IK.
pub(crate) fn xor_3g_gsm(ki: &[u8; 16], rand: &[u8; RAND_BYTES]) -> GsmVector {
    let outputs = xor_3g_outputs(ki, rand);
    let res = Res::new(&outputs.xdout).expect("16 bytes");
    GsmVector {
        sres: c2(&res, SresDerivation::Fold),
        kc: c3(&outputs.ck, &outputs.ik),
    }
}

/// The USIM's SQN from an XOR-3G AUTS, if its MAC matches.
pub(crate) fn xor_3g_auts_sqn(
    k: &[u8; 16],
    rand: &[u8; RAND_BYTES],
    auts: &[u8; AUTS_BYTES],
) -> Option<u64> {
    let outputs = xor_3g_outputs(k, rand);
    let sqn: [u8; SQN_BYTES] = std::array::from_fn(|i| auts[i] ^ outputs.ak[i]);
    (xor_3g_mac(&outputs.xdout, &sqn, &[0; 2])[..] == auts[SQN_BYTES..]).then(|| sqn_value(&sqn))
}

const _: () = assert!(CK_BYTES == 16 && KC_BYTES == 8);
