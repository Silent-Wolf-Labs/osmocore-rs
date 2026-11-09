// SPDX-License-Identifier: GPL-3.0-or-later

//! C's KASUMI functions from `kasumi.h`, with the eight subkey arrays as separate
//! arguments and KGCORE's length in bits.
//!
//! libosmogsm declares these in a public header but doesn't export them. They're here
//! so the C results can be compared directly.

use crate::gsm::kasumi::{KEY_BYTES, Kasumi, Subkeys, kgcore_register, register};

/// Encrypts block `p` with the given subkeys.
#[allow(clippy::too_many_arguments)]
#[allow(non_snake_case)]
pub fn _kasumi(
    p: u64,
    KLi1: &[u16; 8],
    KLi2: &[u16; 8],
    KOi1: &[u16; 8],
    KOi2: &[u16; 8],
    KOi3: &[u16; 8],
    KIi1: &[u16; 8],
    KIi2: &[u16; 8],
    KIi3: &[u16; 8],
) -> u64 {
    Kasumi::from_subkeys(Subkeys {
        kl1: *KLi1,
        kl2: *KLi2,
        ko1: *KOi1,
        ko2: *KOi2,
        ko3: *KOi3,
        ki1: *KIi1,
        ki2: *KIi2,
        ki3: *KIi3,
    })
    .encrypt(p)
}

/// Writes a key's subkeys to the eight arrays.
#[allow(clippy::too_many_arguments)]
#[allow(non_snake_case)]
pub fn _kasumi_key_expand(
    key: &[u8; KEY_BYTES],
    KLi1: &mut [u16; 8],
    KLi2: &mut [u16; 8],
    KOi1: &mut [u16; 8],
    KOi2: &mut [u16; 8],
    KOi3: &mut [u16; 8],
    KIi1: &mut [u16; 8],
    KIi2: &mut [u16; 8],
    KIi3: &mut [u16; 8],
) {
    let subkeys = Subkeys::new(key);
    *KLi1 = subkeys.kl1;
    *KLi2 = subkeys.kl2;
    *KOi1 = subkeys.ko1;
    *KOi2 = subkeys.ko2;
    *KOi3 = subkeys.ko3;
    *KIi1 = subkeys.ki1;
    *KIi2 = subkeys.ki2;
    *KIi3 = subkeys.ki3;
}

/// Writes `cl` bits of KGCORE's keystream to `co`, rounded up to whole bytes, and leaves
/// the rest of `co` alone. The last byte holds keystream bits past `cl` too, as in C.
///
/// Takes CB and CD whole: a bearer past 5 bits or a direction past 1 bit overlaps the
/// fields above it, as in C.
///
/// # Panics
///
/// If `co` is shorter than `cl` bits, where C writes past it.
#[allow(non_snake_case)]
pub fn _kasumi_kgcore(
    CA: u8,
    cb: u8,
    cc: u32,
    cd: u8,
    ck: &[u8; KEY_BYTES],
    co: &mut [u8],
    cl: u16,
) {
    let bytes = usize::from(cl).div_ceil(8);
    assert!(co.len() >= bytes, "KGCORE output is shorter than {cl} bits");
    kgcore_register(ck, register(CA, cb, cc, cd), &mut co[..bytes]);
}
