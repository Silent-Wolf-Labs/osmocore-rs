// SPDX-License-Identifier: GPL-3.0-or-later

//! Milenage, the UMTS authentication functions f1 to f5* of 3GPP TS 35.206, on AES-128.
//!
//! [`Milenage`] holds a subscriber's K and OPc. Its methods give the network side
//! ([`Milenage::f1`], [`Milenage::f2345`], [`Milenage::generate`],
//! [`Milenage::auts_sqn`]) and the USIM side ([`Milenage::check`], [`Milenage::auts`]).
//! [`crate::gsm::auth::UmtsSubscriber`] builds whole vectors with SQN handling on top of
//! these. Needs the `auth` feature.
//!
//! ```
//! use osmocore_rs::gsm::milenage::{Milenage, opc_from_op};
//! // The inputs of 3GPP TS 35.207 test set 1.
//! let k = [0x46, 0x5b, 0x5c, 0xe8, 0xb1, 0x99, 0xb4, 0x9f, 0xaa, 0x5f, 0x0a, 0x2e, 0xe2, 0x38, 0xa6, 0xbc];
//! let op = [0xcd, 0xc2, 0x02, 0xd5, 0x12, 0x3e, 0x20, 0xf6, 0x2b, 0x6d, 0x67, 0x6a, 0xc7, 0x2c, 0xb3, 0x18];
//! let rand = [0x23, 0x55, 0x3c, 0xbe, 0x96, 0x37, 0xa8, 0x9d, 0x21, 0x8a, 0xe6, 0x4d, 0xae, 0x47, 0xbf, 0x35];
//! let milenage = Milenage::new(&k, &opc_from_op(&k, &op));
//! let macs = milenage.f1(&rand, &[0xff, 0x9b, 0xb4, 0xd0, 0xb6, 0x07], &[0xb9, 0xb9]);
//! assert_eq!(macs.mac_a, [0x4a, 0x9f, 0xfa, 0xc3, 0x54, 0xdf, 0xaf, 0xb3]);
//! assert_eq!(milenage.f2345(&rand).res, [0xa5, 0x42, 0x11, 0xd5, 0xe3, 0xba, 0x50, 0xbf]);
//! ```

use crate::gsm::auth::{
    AUTN_BYTES, AUTS_BYTES, Ck, GsmVector, Ik, RAND_BYTES, Res, SRES_BYTES, SresDerivation, c2, c3,
};
use aes::Aes128;
use aes::cipher::{Array, BlockCipherEncrypt, KeyInit};
use std::fmt;

/// Bytes in a sequence number.
pub const SQN_BYTES: usize = 6;
/// Bytes in an anonymity key.
pub const AK_BYTES: usize = 6;
/// Bytes in f2's RES.
pub const RES_BYTES: usize = 8;

const BLOCK: usize = 16;
/// The AMF a USIM uses for MAC-S in a resync, from 3GPP TS 33.102 section 6.3.3.
const RESYNC_AMF: [u8; 2] = [0; 2];

/// Derives OPc from K and OP: AES-128 of OP under K, XOR OP.
pub fn opc_from_op(k: &[u8; 16], op: &[u8; 16]) -> [u8; 16] {
    let mut opc = encrypt(&Aes128::new(&Array::from(*k)), op);
    xor_into(&mut opc, op);
    opc
}

fn encrypt(cipher: &Aes128, block: &[u8; BLOCK]) -> [u8; BLOCK] {
    let mut block = Array::from(*block);
    cipher.encrypt_block(&mut block);
    block.into()
}

fn xor_into(target: &mut [u8], other: &[u8]) {
    for (byte, other) in target.iter_mut().zip(other) {
        *byte ^= other;
    }
}

/// f1's two message authentication codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Macs {
    /// MAC-A, from f1, which goes in AUTN.
    pub mac_a: [u8; 8],
    /// MAC-S, from f1*, which goes in AUTS.
    pub mac_s: [u8; 8],
}

/// The outputs of f2 to f5* for one RAND.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outputs {
    /// f2: the response.
    pub res: [u8; RES_BYTES],
    /// f3: the cipher key.
    pub ck: Ck,
    /// f4: the integrity key.
    pub ik: Ik,
    /// f5: the anonymity key that hides SQN in AUTN.
    pub ak: [u8; AK_BYTES],
    /// f5*: the anonymity key that hides SQN in AUTS.
    pub ak_star: [u8; AK_BYTES],
}

/// What a USIM makes of an AUTN in [`Milenage::check`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// AUTN's SQN is newer than the USIM's and its MAC is right.
    Accepted {
        /// The response to send.
        res: [u8; RES_BYTES],
        /// The cipher key.
        ck: Ck,
        /// The integrity key.
        ik: Ik,
    },
    /// AUTN's SQN isn't newer than the USIM's. The USIM sends AUTS so the network can
    /// resynchronize.
    Resync {
        /// The USIM's SQN, hidden with f5*, then MAC-S.
        auts: [u8; AUTS_BYTES],
    },
    /// AUTN's MAC is wrong.
    MacFailure,
}

/// An AUTS whose MAC-S doesn't match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutsMismatch;

impl fmt::Display for AutsMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the AUTS MAC-S doesn't match")
    }
}

impl std::error::Error for AutsMismatch {}

/// A subscriber's K and OPc, ready to compute Milenage. `Debug` doesn't show them.
#[derive(Clone)]
pub struct Milenage {
    cipher: Aes128,
    opc: [u8; BLOCK],
}

impl fmt::Debug for Milenage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Milenage(..)")
    }
}

impl Milenage {
    /// Prepares Milenage for a K and OPc.
    pub fn new(k: &[u8; 16], opc: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128::new(&Array::from(*k)),
            opc: *opc,
        }
    }

    /// Prepares Milenage for a K and OP, deriving OPc.
    pub fn with_op(k: &[u8; 16], op: &[u8; 16]) -> Self {
        Self::new(k, &opc_from_op(k, op))
    }

    /// E_K(RAND XOR OPc), which every function starts from.
    fn temp(&self, rand: &[u8; RAND_BYTES]) -> [u8; BLOCK] {
        let mut input = *rand;
        xor_into(&mut input, &self.opc);
        encrypt(&self.cipher, &input)
    }

    /// OUT = E_K(rot(x XOR OPc, r) XOR c) XOR OPc, with x already XORed with OPc and
    /// rotated left by `rotate` bytes, and `constant` the last byte of c.
    fn out(&self, mut block: [u8; BLOCK], rotate: usize, constant: u8) -> [u8; BLOCK] {
        block.rotate_left(rotate);
        block[BLOCK - 1] ^= constant;
        let mut out = encrypt(&self.cipher, &block);
        xor_into(&mut out, &self.opc);
        out
    }

    /// f1 and f1*: the network's MAC-A and the USIM's MAC-S for a SQN and AMF.
    pub fn f1(&self, rand: &[u8; RAND_BYTES], sqn: &[u8; SQN_BYTES], amf: &[u8; 2]) -> Macs {
        let temp = self.temp(rand);
        let mut in1 = [0; BLOCK];
        for half in in1.chunks_mut(8) {
            half[..SQN_BYTES].copy_from_slice(sqn);
            half[SQN_BYTES..].copy_from_slice(amf);
        }
        xor_into(&mut in1, &self.opc);
        // r1 is 64 bits and c1 is zero. TEMP goes in after the rotation.
        in1.rotate_left(8);
        xor_into(&mut in1, &temp);
        let out1 = self.out(in1, 0, 0);
        Macs {
            mac_a: out1[..8].try_into().expect("8 bytes"),
            mac_s: out1[8..].try_into().expect("8 bytes"),
        }
    }

    /// f2 to f5*: RES, CK, IK and both anonymity keys.
    pub fn f2345(&self, rand: &[u8; RAND_BYTES]) -> Outputs {
        let mut base = self.temp(rand);
        xor_into(&mut base, &self.opc);
        // Rotations r2 to r5 are 0, 32, 64 and 96 bits, and c2 to c5 end in 1, 2, 4 and 8.
        let out2 = self.out(base, 0, 1);
        let out3 = self.out(base, 4, 2);
        let out4 = self.out(base, 8, 4);
        let out5 = self.out(base, 12, 8);
        Outputs {
            res: out2[8..].try_into().expect("8 bytes"),
            ck: Ck::new(out3),
            ik: Ik::new(out4),
            ak: out2[..AK_BYTES].try_into().expect("6 bytes"),
            ak_star: out5[..AK_BYTES].try_into().expect("6 bytes"),
        }
    }

    /// The network side for one RAND, SQN and AMF: AUTN, which is SQN hidden with AK, then
    /// AMF and MAC-A, and the outputs of f2 to f5*.
    pub fn generate(
        &self,
        rand: &[u8; RAND_BYTES],
        sqn: &[u8; SQN_BYTES],
        amf: &[u8; 2],
    ) -> ([u8; AUTN_BYTES], Outputs) {
        let outputs = self.f2345(rand);
        let mac_a = self.f1(rand, sqn, amf).mac_a;
        let mut autn = [0; AUTN_BYTES];
        autn[..SQN_BYTES].copy_from_slice(sqn);
        xor_into(&mut autn[..SQN_BYTES], &outputs.ak);
        autn[SQN_BYTES..8].copy_from_slice(amf);
        autn[8..].copy_from_slice(&mac_a);
        (autn, outputs)
    }

    /// GSM SRES and Kc from Milenage, by C2 and C3 of RES, CK and IK.
    pub fn gsm(&self, rand: &[u8; RAND_BYTES]) -> GsmVector {
        let outputs = self.f2345(rand);
        let res = Res::new(&outputs.res).expect("8 bytes");
        let sres: [u8; SRES_BYTES] = c2(&res, SresDerivation::Fold);
        GsmVector {
            sres,
            kc: c3(&outputs.ck, &outputs.ik),
        }
    }

    /// The AUTS a USIM at `sqn_ms` sends: its SQN hidden with f5*, then MAC-S with AMF 0.
    pub fn auts(&self, rand: &[u8; RAND_BYTES], sqn_ms: &[u8; SQN_BYTES]) -> [u8; AUTS_BYTES] {
        let mut auts = [0; AUTS_BYTES];
        auts[..SQN_BYTES].copy_from_slice(sqn_ms);
        xor_into(&mut auts[..SQN_BYTES], &self.f2345(rand).ak_star);
        auts[SQN_BYTES..].copy_from_slice(&self.f1(rand, sqn_ms, &RESYNC_AMF).mac_s);
        auts
    }

    /// The network side of a resync: the USIM's SQN from its AUTS, if MAC-S matches.
    pub fn auts_sqn(
        &self,
        rand: &[u8; RAND_BYTES],
        auts: &[u8; AUTS_BYTES],
    ) -> Result<[u8; SQN_BYTES], AutsMismatch> {
        let mut sqn: [u8; SQN_BYTES] = auts[..SQN_BYTES].try_into().expect("6 bytes");
        xor_into(&mut sqn, &self.f2345(rand).ak_star);
        if self.f1(rand, &sqn, &RESYNC_AMF).mac_s[..] == auts[SQN_BYTES..] {
            Ok(sqn)
        } else {
            Err(AutsMismatch)
        }
    }

    /// The USIM side: checks an AUTN against the USIM's SQN.
    ///
    /// As in C, this compares SQN before checking the MAC, so an AUTN with an old SQN
    /// gets [`Check::Resync`] even if its MAC is wrong. Any SQN above the USIM's is
    /// accepted: there's no IND slot or age limit.
    pub fn check(
        &self,
        rand: &[u8; RAND_BYTES],
        autn: &[u8; AUTN_BYTES],
        sqn: &[u8; SQN_BYTES],
    ) -> Check {
        let outputs = self.f2345(rand);
        let mut received: [u8; SQN_BYTES] = autn[..SQN_BYTES].try_into().expect("6 bytes");
        xor_into(&mut received, &outputs.ak);
        if received <= *sqn {
            return Check::Resync {
                auts: self.auts(rand, sqn),
            };
        }
        let amf: [u8; 2] = autn[SQN_BYTES..8].try_into().expect("2 bytes");
        if self.f1(rand, &received, &amf).mac_a[..] != autn[8..] {
            return Check::MacFailure;
        }
        Check::Accepted {
            res: outputs.res,
            ck: outputs.ck,
            ik: outputs.ik,
        }
    }
}
