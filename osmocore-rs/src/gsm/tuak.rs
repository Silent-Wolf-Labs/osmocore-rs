// SPDX-License-Identifier: GPL-3.0-or-later

//! TUAK, the UMTS authentication functions f1 to f5* of 3GPP TS 35.231, on the
//! Keccak-f\[1600\] permutation.
//!
//! [`Tuak`] holds a subscriber's K, of 16 or 32 bytes, and TOPc. Its methods give the
//! network side ([`Tuak::f1`], [`Tuak::f2345`], [`Tuak::generate`], [`Tuak::auts_sqn`]) and
//! the USIM's AUTS ([`Tuak::auts`]). TUAK defines several lengths for each output, which
//! the methods take as const parameters, so a length TUAK doesn't define doesn't compile.
//! [`crate::gsm::auth::UmtsSubscriber`] builds whole vectors with SQN handling on top of
//! these. Needs the `auth` feature.
//!
//! ```
//! use osmocore_rs::gsm::auth::K;
//! use osmocore_rs::gsm::tuak::Tuak;
//! // 3GPP TS 35.233 section 6.3, test set 1.
//! let k = K::new(&[0xab; 16]).unwrap();
//! let tuak = Tuak::with_top(&k, &[0x55; 32]);
//! let mac_a: [u8; 8] = tuak.f1(&[0x42; 16], &[0x11; 6], &[0xff; 2]);
//! assert_eq!(mac_a, [0xf9, 0xa5, 0x4e, 0x6a, 0xea, 0xa8, 0x61, 0x8d]);
//! let outputs = tuak.f2345::<4, 16, 16>(&[0x42; 16]);
//! assert_eq!(outputs.res, [0x65, 0x7a, 0xcd, 0x64]);
//! ```

pub use crate::gsm::milenage::AutsMismatch;

use crate::gsm::auth::{AUTN_BYTES, AUTS_BYTES, K, RAND_BYTES};
use std::fmt;
use std::num::NonZeroU32;

/// Bytes in TOP, the operator variant key, and in TOPc.
pub const TOP_BYTES: usize = 32;
/// Bytes in a sequence number.
pub const SQN_BYTES: usize = 6;
/// Bytes in an anonymity key.
pub const AK_BYTES: usize = 6;

/// Bytes in the Keccak state.
const STATE_BYTES: usize = 200;
/// The ALGONAME field, `TUAK1.0` (3GPP TS 35.231).
const ALGONAME: &[u8; 7] = b"TUAK1.0";
/// The AMF a USIM uses for MAC-S in a resync, from 3GPP TS 33.102 section 6.3.3.
const RESYNC_AMF: [u8; 2] = [0; 2];
/// The MAC length in AUTN and AUTS, which 3GPP uses.
const MAC_BYTES: usize = 8;

// The bits of INSTANCE, which tells the functions and output lengths apart (3GPP TS
// 35.231).
/// f1*.
const INSTANCE_F1_STAR: u8 = 0x80;
/// f2 to f5.
const INSTANCE_F2345: u8 = 0x40;
/// f5*.
const INSTANCE_F5_STAR: u8 = 0xc0;
/// A 256-bit CK.
const INSTANCE_CK_256: u8 = 0x04;
/// A 256-bit IK.
const INSTANCE_IK_256: u8 = 0x02;
/// A 256-bit K.
const INSTANCE_K_256: u8 = 0x01;

/// The INSTANCE bits for a MAC or RES length: 64, 128 and 256 bits, and for RES also 32.
const fn length_bits(bytes: usize, allow_32_bits: bool) -> u8 {
    match bytes {
        4 if allow_32_bits => 0x00,
        8 => 0x08,
        16 => 0x10,
        32 => 0x20,
        _ => panic!("TUAK has no output of this length"),
    }
}

/// The INSTANCE bit for a K, CK or IK length: 128 or 256 bits.
const fn key_bit(bytes: usize, bit: u8) -> u8 {
    match bytes {
        16 => 0,
        32 => bit,
        _ => panic!("TUAK keys have 16 or 32 bytes"),
    }
}

/// f2 to f5's outputs for one RAND, with RES, CK and IK of the lengths asked for.
#[derive(Clone, PartialEq, Eq)]
pub struct Outputs<const RES: usize, const CK: usize, const IK: usize> {
    /// f2: the response.
    pub res: [u8; RES],
    /// f3: the cipher key.
    pub ck: [u8; CK],
    /// f4: the integrity key.
    pub ik: [u8; IK],
    /// f5: the anonymity key that hides SQN in AUTN.
    pub ak: [u8; AK_BYTES],
}

impl<const RES: usize, const CK: usize, const IK: usize> fmt::Debug for Outputs<RES, CK, IK> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Outputs")
            .field("res", &self.res)
            .field("ck", &format_args!(".."))
            .field("ik", &format_args!(".."))
            .field("ak", &self.ak)
            .finish()
    }
}

/// Derives TOPc from K and TOP.
pub fn topc_from_top(k: &K, top: &[u8; TOP_BYTES], iterations: NonZeroU32) -> [u8; TOP_BYTES] {
    let state = permute(
        top,
        key_bit(k.as_bytes().len(), INSTANCE_K_256),
        &Fields::ZERO,
        k,
        iterations,
    );
    output(&state, 0)
}

/// RAND, AMF and SQN, the fields of the input that change between calls.
struct Fields<'a> {
    rand: &'a [u8; RAND_BYTES],
    amf: &'a [u8; 2],
    sqn: &'a [u8; SQN_BYTES],
}

impl Fields<'static> {
    const ZERO: Self = Self {
        rand: &[0; RAND_BYTES],
        amf: &[0; 2],
        sqn: &[0; SQN_BYTES],
    };
}

/// Builds the 1600-bit input and runs Keccak-f\[1600\] on it `iterations` times (3GPP TS
/// 35.231). Each field goes in with its last byte first, then K padded with
/// zeros to 256 bits, then the padding bits; the rest is zero.
fn permute(
    topc: &[u8; TOP_BYTES],
    instance: u8,
    fields: &Fields<'_>,
    k: &K,
    iterations: NonZeroU32,
) -> [u8; STATE_BYTES] {
    let mut input = [0; STATE_BYTES];
    let mut at = 0;
    let mut push = |bytes: &[u8]| {
        for (slot, byte) in input[at..].iter_mut().zip(bytes.iter().rev()) {
            *slot = *byte;
        }
        at += bytes.len();
    };
    push(topc);
    push(&[instance]);
    push(ALGONAME);
    push(fields.rand);
    push(fields.amf);
    push(fields.sqn);
    push(k.as_bytes());
    // Bits 768 to 772 are ones, and so is bit 1087.
    input[96] = 0x1f;
    input[135] = 0x80;

    // Keccak numbers the state's bits from the low bit of its first byte, so each 64-bit
    // lane is eight bytes in little-endian order.
    let words = input.as_chunks::<8>().0;
    let mut lanes: [u64; 25] = std::array::from_fn(|i| u64::from_le_bytes(words[i]));
    keccak::Keccak::new().with_f1600(|f1600| {
        for _ in 0..iterations.get() {
            f1600(&mut lanes);
        }
    });
    let mut state = [0; STATE_BYTES];
    for (bytes, lane) in state.as_chunks_mut::<8>().0.iter_mut().zip(lanes) {
        *bytes = lane.to_le_bytes();
    }
    state
}

/// An output field: `N` bytes from `offset`, last byte first.
fn output<const N: usize>(state: &[u8; STATE_BYTES], offset: usize) -> [u8; N] {
    let mut out: [u8; N] = state[offset..offset + N].try_into().expect("N bytes");
    out.reverse();
    out
}

/// A subscriber's K and TOPc, ready to compute TUAK. `Debug` doesn't show them.
#[derive(Clone)]
pub struct Tuak {
    k: K,
    topc: [u8; TOP_BYTES],
    iterations: NonZeroU32,
}

impl fmt::Debug for Tuak {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tuak(..)")
    }
}

impl Tuak {
    /// Prepares TUAK for a K and TOPc, running Keccak once per function, as 3GPP uses it.
    pub fn new(k: &K, topc: &[u8; TOP_BYTES]) -> Self {
        Self {
            k: k.clone(),
            topc: *topc,
            iterations: NonZeroU32::MIN,
        }
    }

    /// Prepares TUAK for a K and TOP, deriving TOPc with one Keccak run.
    pub fn with_top(k: &K, top: &[u8; TOP_BYTES]) -> Self {
        Self::new(k, &topc_from_top(k, top, NonZeroU32::MIN))
    }

    /// Runs Keccak this many times in each function, as TS 35.231 allows an operator to.
    /// TOPc is unchanged.
    pub const fn with_iterations(mut self, iterations: NonZeroU32) -> Self {
        self.iterations = iterations;
        self
    }

    fn permute(&self, instance: u8, fields: &Fields<'_>) -> [u8; STATE_BYTES] {
        let k_bit = key_bit(self.k.as_bytes().len(), INSTANCE_K_256);
        permute(
            &self.topc,
            instance | k_bit,
            fields,
            &self.k,
            self.iterations,
        )
    }

    /// f1: the network's MAC-A, of 8, 16 or 32 bytes, for a SQN and AMF.
    pub fn f1<const N: usize>(
        &self,
        rand: &[u8; RAND_BYTES],
        sqn: &[u8; SQN_BYTES],
        amf: &[u8; 2],
    ) -> [u8; N] {
        let instance = const { length_bits(N, false) };
        output(&self.permute(instance, &Fields { rand, amf, sqn }), 0)
    }

    /// f1*: the USIM's MAC-S, of 8, 16 or 32 bytes, for a SQN and AMF.
    pub fn f1_star<const N: usize>(
        &self,
        rand: &[u8; RAND_BYTES],
        sqn: &[u8; SQN_BYTES],
        amf: &[u8; 2],
    ) -> [u8; N] {
        let instance = const { INSTANCE_F1_STAR | length_bits(N, false) };
        output(&self.permute(instance, &Fields { rand, amf, sqn }), 0)
    }

    /// f2 to f5: RES of 4, 8, 16 or 32 bytes, CK and IK of 16 or 32, and AK.
    pub fn f2345<const RES: usize, const CK: usize, const IK: usize>(
        &self,
        rand: &[u8; RAND_BYTES],
    ) -> Outputs<RES, CK, IK> {
        let instance = const {
            INSTANCE_F2345
                | length_bits(RES, true)
                | key_bit(CK, INSTANCE_CK_256)
                | key_bit(IK, INSTANCE_IK_256)
        };
        let state = self.permute(
            instance,
            &Fields {
                rand,
                ..Fields::ZERO
            },
        );
        Outputs {
            res: output(&state, 0),
            ck: output(&state, 32),
            ik: output(&state, 64),
            ak: output(&state, 96),
        }
    }

    /// f5*: the anonymity key that hides SQN in AUTS.
    pub fn f5_star(&self, rand: &[u8; RAND_BYTES]) -> [u8; AK_BYTES] {
        let state = self.permute(
            INSTANCE_F5_STAR,
            &Fields {
                rand,
                ..Fields::ZERO
            },
        );
        output(&state, 96)
    }

    /// The network side for one RAND, SQN and AMF: AUTN, which is SQN hidden with AK, then
    /// AMF and an 8-byte MAC-A, and f2 to f5 with 16-byte CK and IK.
    pub fn generate<const RES: usize>(
        &self,
        rand: &[u8; RAND_BYTES],
        sqn: &[u8; SQN_BYTES],
        amf: &[u8; 2],
    ) -> ([u8; AUTN_BYTES], Outputs<RES, 16, 16>) {
        let outputs = self.f2345(rand);
        let mut autn = [0; AUTN_BYTES];
        for ((out, sqn), ak) in autn.iter_mut().zip(sqn).zip(&outputs.ak) {
            *out = sqn ^ ak;
        }
        autn[SQN_BYTES..8].copy_from_slice(amf);
        autn[8..].copy_from_slice(&self.f1::<MAC_BYTES>(rand, sqn, amf));
        (autn, outputs)
    }

    /// The AUTS a USIM at `sqn_ms` sends: its SQN hidden with f5*, then an 8-byte MAC-S
    /// with AMF 0.
    pub fn auts(&self, rand: &[u8; RAND_BYTES], sqn_ms: &[u8; SQN_BYTES]) -> [u8; AUTS_BYTES] {
        let mut auts = [0; AUTS_BYTES];
        for ((out, sqn), ak) in auts.iter_mut().zip(sqn_ms).zip(&self.f5_star(rand)) {
            *out = sqn ^ ak;
        }
        auts[SQN_BYTES..].copy_from_slice(&self.f1_star::<MAC_BYTES>(rand, sqn_ms, &RESYNC_AMF));
        auts
    }

    /// The network side of a resync: the USIM's SQN from its AUTS, if MAC-S matches.
    pub fn auts_sqn(
        &self,
        rand: &[u8; RAND_BYTES],
        auts: &[u8; AUTS_BYTES],
    ) -> Result<[u8; SQN_BYTES], AutsMismatch> {
        let ak = self.f5_star(rand);
        let sqn: [u8; SQN_BYTES] = std::array::from_fn(|i| auts[i] ^ ak[i]);
        if self.f1_star::<MAC_BYTES>(rand, &sqn, &RESYNC_AMF)[..] == auts[SQN_BYTES..] {
            Ok(sqn)
        } else {
            Err(AutsMismatch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_keys() {
        let tuak = Tuak::new(&K::new(&[0x5a; 32]).unwrap(), &[0x5a; 32]);
        assert_eq!(format!("{tuak:?}"), "Tuak(..)");
        let shown = format!("{:?}", tuak.f2345::<8, 16, 16>(&[0; 16]));
        assert_eq!(shown.matches("..").count(), 2, "{shown}");
    }

    #[test]
    fn auts_round_trips() {
        let tuak = Tuak::with_top(&K::new(&[7; 16]).unwrap(), &[9; 32]);
        let auts = tuak.auts(&[3; 16], &[0, 0, 0, 0, 1, 2]);
        assert_eq!(tuak.auts_sqn(&[3; 16], &auts), Ok([0, 0, 0, 0, 1, 2]));
        assert_eq!(tuak.auts_sqn(&[4; 16], &auts), Err(AutsMismatch));
    }
}
