// SPDX-License-Identifier: GPL-3.0-or-later

//! Taking single elements off the front of a buffer.

use super::*;

fn exhaust<T>(data: &mut &[u8]) -> Result<T, TlvError> {
    *data = &data[data.len()..];
    Err(TlvError::LengthBeyondBuffer)
}

/// Take `len` bytes from the front of `data`. On an error, `data` is left empty.
pub fn shift_v_fixed<'a>(data: &mut &'a [u8], len: usize) -> Result<&'a [u8], TlvError> {
    let Some((value, rest)) = data.split_at_checked(len) else {
        return exhaust(data);
    };
    *data = rest;
    Ok(value)
}

/// Take a tag and `len` value bytes from the front of `data`, if it starts with `tag`.
/// A different tag returns `None` and leaves `data` alone. On an error, `data` is left
/// empty.
pub fn match_shift_tv_fixed<'a>(
    data: &mut &'a [u8],
    tag: u8,
    len: usize,
) -> Result<Option<&'a [u8]>, TlvError> {
    match data.first() {
        None => exhaust(data),
        Some(first) if *first != tag => Ok(None),
        Some(_) if len > data.len() - 1 => exhaust(data),
        Some(_) => {
            let (value, rest) = data[1..].split_at(len);
            *data = rest;
            Ok(Some(value))
        }
    }
}

/// Take a tag, one-byte length and value from the front of `data`. On an error, `data`
/// is left empty.
pub fn shift_tlv<'a>(data: &mut &'a [u8]) -> Result<(u8, &'a [u8]), TlvError> {
    let &[tag, len, ref rest @ ..] = *data else {
        return exhaust(data);
    };
    let Some((value, rest)) = rest.split_at_checked(usize::from(len)) else {
        return exhaust(data);
    };
    *data = rest;
    Ok((tag, value))
}

/// Like [`shift_tlv`], but a different tag returns `None` and leaves `data` alone.
pub fn match_shift_tlv<'a>(data: &mut &'a [u8], tag: u8) -> Result<Option<&'a [u8]>, TlvError> {
    let before = *data;
    let (found, value) = shift_tlv(data)?;
    if found != tag {
        *data = before;
        return Ok(None);
    }
    Ok(Some(value))
}

/// Take a one-byte length and value from the front of `data`. On an error, `data` is
/// left empty.
pub fn shift_lv<'a>(data: &mut &'a [u8]) -> Result<&'a [u8], TlvError> {
    let &[len, ref rest @ ..] = *data else {
        return exhaust(data);
    };
    let Some((value, rest)) = rest.split_at_checked(usize::from(len)) else {
        return exhaust(data);
    };
    *data = rest;
    Ok(value)
}
