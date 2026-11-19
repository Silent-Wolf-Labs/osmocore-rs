// SPDX-License-Identifier: GPL-3.0-or-later

//! C's 3GPP TS 24.008 BCD number functions, with C's output buffers and status codes, its
//! partial writes, its 8-bit length byte and its `max_len` that bounds the length byte
//! rather than the buffer.
//!
//! The preferred API is [`crate::gsm::gsm48_ie`], whose
//! [`BcdNumber`](crate::gsm::gsm48_ie::BcdNumber) writes the header too and refuses what
//! doesn't fit its length byte.

use crate::gsm::gsm48_ie::{DIGITS, FILLER, nibble};

const EIO: i32 = -5;
const EINVAL: i32 = -22;
const ENOSPC: i32 = -28;

/// The character C writes for a nibble: a digit, or a NUL for the filler.
fn character(nibble: u8) -> u8 {
    if nibble == FILLER {
        0
    } else {
        DIGITS[usize::from(nibble)]
    }
}

/// `gsm48_encode_bcd_number`: writes the length byte into `bcd_lv[0]`, then the digits of
/// `input` after `h_len` header bytes, which are left as they are. `input` ends at its first
/// NUL or its end. Returns the bytes written.
///
/// The length byte is the header and digit bytes modulo 256. Past `max_len`, the call
/// returns -5 with only the length byte written. `max_len` bounds the length byte, so a call
/// writes up to `max_len + 1` bytes. A character other than a digit, `*`, `#`, `a`, `b` or
/// `c` returns -22 after the digits before it are written.
///
/// Where C would write past `bcd_lv`, this returns -5 with only the length byte written, and
/// an empty `bcd_lv` returns -5 untouched.
pub fn gsm48_encode_bcd_number(bcd_lv: &mut [u8], max_len: u8, h_len: usize, input: &[u8]) -> i32 {
    let input = input.split(|&byte| byte == 0).next().unwrap_or_default();
    let extent = 1 + h_len + input.len().div_ceil(2);
    let Some(length) = bcd_lv.first_mut() else {
        return EIO;
    };
    *length = (extent - 1) as u8;
    if *length > max_len || bcd_lv.len() < extent {
        return EIO;
    }
    let digits = &mut bcd_lv[1 + h_len..extent];
    for (i, &byte) in input.iter().enumerate() {
        let Some(nibble) = nibble(byte) else {
            return EINVAL;
        };
        if i % 2 == 0 {
            digits[i / 2] = nibble;
        } else {
            digits[i / 2] |= nibble << 4;
        }
    }
    if input.len() % 2 == 1 {
        digits[input.len() / 2] |= FILLER << 4;
    }
    extent as i32
}

/// `gsm48_decode_bcd_number2`: decodes the digits of `bcd_lv` after `h_len` header bytes
/// into `output`, then a NUL. `bcd_lv` is the input buffer, and its length is C's
/// `input_len`; bytes past the ones its length byte counts are ignored.
///
/// The filler writes a NUL wherever it is, and the digits after it are still written. An
/// empty `output` returns -28 untouched. Otherwise `output` always gets a NUL: alone for -5,
/// no input, and for -22, a length byte past the input. Digits that don't fit with their NUL
/// return -28 after those that do, unless only a final filler is left.
pub fn gsm48_decode_bcd_number2(output: &mut [u8], bcd_lv: &[u8], h_len: usize) -> i32 {
    let Some(nul) = output.first_mut() else {
        return ENOSPC;
    };
    *nul = 0;
    let Some(&content) = bcd_lv.first() else {
        return EIO;
    };
    let Some(content) = bcd_lv.get(1..=usize::from(content)) else {
        return EINVAL;
    };
    let digits = content.get(h_len..).unwrap_or_default();
    let mut at = 0;
    let mut truncated = false;
    'bytes: for (i, &byte) in digits.iter().enumerate() {
        for (half, nibble) in [byte & 0x0f, byte >> 4].into_iter().enumerate() {
            // Each character needs room for a NUL after it.
            if output.len() - at <= 1 {
                let only_final_filler = half == 1 && i + 1 == digits.len() && nibble == FILLER;
                truncated = !only_final_filler;
                break 'bytes;
            }
            output[at] = character(nibble);
            at += 1;
        }
    }
    output[at] = 0;
    if truncated { ENOSPC } else { 0 }
}

/// `gsm48_decode_bcd_number`: [`gsm48_decode_bcd_number2`] with the length byte as the
/// input's length.
///
/// Where `bcd_lv` is shorter than its length byte says, C reads past it; this decodes only
/// `bcd_lv`, and so returns -5 or -22 as [`gsm48_decode_bcd_number2`] does.
pub fn gsm48_decode_bcd_number(output: &mut [u8], bcd_lv: &[u8], h_len: usize) -> i32 {
    gsm48_decode_bcd_number2(output, bcd_lv, h_len)
}
