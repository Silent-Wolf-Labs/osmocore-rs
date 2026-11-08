// SPDX-License-Identifier: GPL-3.0-or-later

//! The C TLV contracts: integer status codes, partly written outputs, and lengths that
//! truncate to their field instead of failing.
//!
//! `msgb_put` aborts in C when a message has no room. These functions return
//! [`TlvError::Msgb`](crate::gsm::tlv::TlvError::Msgb) instead, having written nothing for that element.

use crate::base::msgb::Msgb;
use crate::gsm::tlv::{
    self, ParseFailure, TlvDefinition, TlvError, TlvParsed, TlvProtocol, TlvType, gan_header,
    parse_layout, tvlv_header,
};

const ERR_OFS_BEYOND_BUFFER: i32 = -1;
const ERR_OFS_LEN_BEYOND_BUFFER: i32 = -2;
const ERR_UNKNOWN_TLV_TYPE: i32 = -3;
const ERR_MAND_IE_MISSING: i32 = -50;
const ERR_IE_TOO_SHORT: i32 = -51;
const ERR_INVALID: i32 = -22;

/// The C status code for a parse or validation error.
pub const fn tlv_error_code(error: TlvError) -> i32 {
    match error {
        TlvError::OffsetBeyondBuffer => ERR_OFS_BEYOND_BUFFER,
        TlvError::LengthBeyondBuffer => ERR_OFS_LEN_BEYOND_BUFFER,
        TlvError::UnknownType => ERR_UNKNOWN_TLV_TYPE,
        TlvError::MandatoryIeMissing { .. } => ERR_MAND_IE_MISSING,
        TlvError::IeTooShort { .. } => ERR_IE_TOO_SHORT,
        TlvError::ValueTooLong
        | TlvError::ValueMissing
        | TlvError::TagTooLarge
        | TlvError::Msgb(_) => ERR_INVALID,
    }
}

/// Parses one element and returns the bytes used or a negative status. On failure, the
/// outputs hold whatever C has written by then: `o_tag` once a byte has been read,
/// `o_len` once the length is known, and `o_val` is `None` when the value runs past the
/// input or left alone otherwise.
pub fn tlv_parse_one<'a>(
    o_tag: &mut u8,
    o_len: &mut u16,
    o_val: &mut Option<&'a [u8]>,
    definition: &TlvDefinition,
    buf: &'a [u8],
) -> i32 {
    match parse_layout(definition, buf) {
        Ok((tag, start, len, used)) => {
            *o_tag = tag;
            *o_len = len;
            *o_val = Some(&buf[start..start + usize::from(len)]);
            used as i32
        }
        Err(ParseFailure {
            error,
            tag,
            len,
            clears_value,
        }) => {
            if let Some(tag) = tag {
                *o_tag = tag;
            }
            if let Some(len) = len {
                *o_len = len;
            }
            if clears_value {
                *o_val = None;
            }
            tlv_error_code(error)
        }
    }
}

fn lv_tags(lv_tag: u8, lv_tag2: u8) -> [Option<u8>; 2] {
    [lv_tag, lv_tag2].map(|tag| (tag != 0).then_some(tag))
}

/// Parses into `decoded[0]`, keeping the first occurrence of each tag. A zero `lv_tag`
/// means none. Returns the element count or a negative status.
pub fn tlv_parse<'a>(
    decoded: &mut TlvParsed<'a>,
    definition: &TlvDefinition,
    buf: &'a [u8],
    lv_tag: u8,
    lv_tag2: u8,
) -> i32 {
    tlv_parse2(
        std::slice::from_mut(decoded),
        definition,
        buf,
        lv_tag,
        lv_tag2,
    )
}

/// Parses into `decoded`, with each further occurrence of a tag in the next entry. A
/// zero `lv_tag` means none. Returns the element count or a negative status, leaving
/// the elements found before an error in `decoded`.
pub fn tlv_parse2<'a>(
    decoded: &mut [TlvParsed<'a>],
    definition: &TlvDefinition,
    buf: &'a [u8],
    lv_tag: u8,
    lv_tag2: u8,
) -> i32 {
    match tlv::parse_multiple(decoded, definition, buf, lv_tags(lv_tag, lv_tag2)) {
        Ok(count) => count as i32,
        Err(error) => tlv_error_code(error),
    }
}

/// Appends one element, truncating `len` to the format's length field as C does.
/// `value` must hold the truncated length. Formats with one value byte read
/// `value[0]`, which C does whatever `len` is; an empty `value` is
/// [`TlvError::ValueMissing`].
pub fn tlv_encode_one(
    msg: &mut Msgb,
    kind: TlvType,
    tag: u8,
    len: u32,
    value: &[u8],
) -> Result<i32, TlvError> {
    let bytes = |len: usize| value.get(..len).ok_or(TlvError::ValueMissing);
    match kind {
        TlvType::None => {}
        TlvType::Fixed => msg.put_tv_fixed(tag, bytes(len as usize)?)?,
        TlvType::T => msg.put_v(tag)?,
        TlvType::Tv => msg.put_tv(tag, *value.first().ok_or(TlvError::ValueMissing)?)?,
        TlvType::Tlv => msg.put_tlv(tag, bytes(usize::from(len as u8))?)?,
        TlvType::Tl16v => msg.put_tl16v(tag, bytes(usize::from(len as u16))?)?,
        TlvType::Tvlv => msgb_tvlv_put(msg, tag, len as u16, value)?,
        TlvType::SingleTv => {
            let low = *value.first().ok_or(TlvError::ValueMissing)? & 0x0f;
            msg.put_v((tag << 4) | low)?;
        }
        TlvType::VtvlvGan => msgb_vtvlv_gan_put(msg, u16::from(tag), len as u16, value)?,
    }
    Ok(0)
}

/// Appends every element in tag order and returns the bytes written. An error leaves
/// the elements before it in `msg`.
pub fn tlv_encode(
    msg: &mut Msgb,
    definition: &TlvDefinition,
    parsed: &TlvParsed<'_>,
) -> Result<i32, TlvError> {
    let tags: Vec<u8> = (0..=255).collect();
    tlv_encode_ordered(msg, definition, parsed, &tags)
}

/// Appends the elements in the order of `tags` and returns the bytes written. An error
/// leaves the elements before it in `msg`.
pub fn tlv_encode_ordered(
    msg: &mut Msgb,
    definition: &TlvDefinition,
    parsed: &TlvParsed<'_>,
    tags: &[u8],
) -> Result<i32, TlvError> {
    let before = msg.tail_offset();
    for &tag in tags {
        if let Some(value) = parsed.get(tag) {
            tlv_encode_one(
                msg,
                definition.get(tag).kind,
                tag,
                value.len() as u32,
                value,
            )?;
        }
    }
    Ok((msg.tail_offset() - before) as i32)
}

/// Appends a Tvlv element. C accepts lengths from `0x8000`, whose first length byte
/// has the high bit set, so a parser reads them as the one-byte form. `value` must hold
/// `len` bytes.
pub fn msgb_tvlv_put(msg: &mut Msgb, tag: u8, len: u16, value: &[u8]) -> Result<(), TlvError> {
    let value = value
        .get(..usize::from(len))
        .ok_or(TlvError::ValueMissing)?;
    let (header, header_len) = tvlv_header(tag, len);
    msg.put_parts(&[&header[..header_len], value]).map(drop)
}

/// Prepends a Tvlv element, with the same lengths as [`msgb_tvlv_put`].
pub fn msgb_tvlv_push(msg: &mut Msgb, tag: u8, len: u16, value: &[u8]) -> Result<(), TlvError> {
    let value = value
        .get(..usize::from(len))
        .ok_or(TlvError::ValueMissing)?;
    let (header, header_len) = tvlv_header(tag, len);
    msg.push_parts(&[&header[..header_len], value]).map(drop)
}

/// Appends a Tvlv tag and length and reserves the value, with the same lengths as
/// [`msgb_tvlv_put`]. Returns the value's backing offset. C documents the value as not
/// yet reserved, but reserves it.
pub fn msgb_tvl_put(msg: &mut Msgb, tag: u8, len: u16) -> Result<usize, TlvError> {
    let (header, header_len) = tvlv_header(tag, len);
    let start = msg.put(header_len + usize::from(len))?;
    msg.write_parts(start, &[&header[..header_len]]);
    Ok(start + header_len)
}

/// Appends a GAN element. C accepts tags and lengths from `0x8000`, whose high bit it
/// can't represent, so a parser reads them as 15-bit values. `value` must hold `len`
/// bytes.
pub fn msgb_vtvlv_gan_put(
    msg: &mut Msgb,
    tag: u16,
    len: u16,
    value: &[u8],
) -> Result<(), TlvError> {
    let value = value
        .get(..usize::from(len))
        .ok_or(TlvError::ValueMissing)?;
    let (header, header_len) = gan_header(tag, len);
    msg.put_parts(&[&header[..header_len], value]).map(drop)
}

/// Prepends a GAN element, with the same tags and lengths as [`msgb_vtvlv_gan_put`].
pub fn msgb_vtvlv_gan_push(
    msg: &mut Msgb,
    tag: u16,
    len: u16,
    value: &[u8],
) -> Result<(), TlvError> {
    let value = value
        .get(..usize::from(len))
        .ok_or(TlvError::ValueMissing)?;
    let (header, header_len) = gan_header(tag, len);
    msg.push_parts(&[&header[..header_len], value]).map(drop)
}

/// Prepends a GAN tag and length, with the same range as [`msgb_vtvlv_gan_put`].
pub fn msgb_vtvl_gan_push(msg: &mut Msgb, tag: u16, len: u16) -> Result<(), TlvError> {
    let (header, header_len) = gan_header(tag, len);
    msg.push_parts(&[&header[..header_len]]).map(drop)
}

/// C's `uint16_t` result, which wraps for lengths near `u16::MAX`.
pub const fn tvlv_gross_len(len: u16) -> u16 {
    if len <= 0x7f {
        len + 2
    } else {
        len.wrapping_add(3)
    }
}

/// C's `uint16_t` result, for a tag and length in `u16`.
pub const fn vtvl_gan_gross_len(tag: u16, len: u16) -> u16 {
    tlv::vtvl_gan_gross_len(tag as usize, len as usize) as u16
}

/// C's `uint16_t` result, which wraps for lengths near `u16::MAX`.
pub const fn vtvlv_gan_gross_len(tag: u16, len: u16) -> u16 {
    let total = if len <= 0x7f {
        len + 2
    } else {
        len.wrapping_add(3)
    };
    if tag > 0x7f {
        total.wrapping_add(1)
    } else {
        total
    }
}

/// Merges `source` into `destination` and returns 0, keeping C's result: each merged
/// tag ends with length zero. C copies the value but never sets the length, so a caller
/// using `TLVP_LEN` sees an empty element. [`TlvParsed::merge`] keeps the length.
pub fn osmo_tlvp_merge<'a>(destination: &mut TlvParsed<'a>, source: &TlvParsed<'a>) -> i32 {
    for ie in source.iter() {
        if !ie.value.is_empty() {
            destination.set(ie.tag, Some(&ie.value[..0]));
        }
    }
    0
}

/// Returns the lengths that C's copy keeps. C allocates each value only if the new,
/// still empty entry already has one, so it copies none: every tag of the copy is
/// absent, with its length left in place. Clone a [`TlvParsed`] for a real copy.
pub fn osmo_tlvp_copy(original: Option<&TlvParsed<'_>>) -> [u16; 256] {
    let mut lengths = [0; 256];
    if let Some(original) = original {
        for ie in original.iter() {
            lengths[usize::from(ie.tag)] = ie.value.len() as u16;
        }
    }
    lengths
}

/// Returns the first two value bytes in host byte order, or `None` if there are fewer,
/// where C reads past the value.
pub fn tlvp_val16_unal(parsed: &TlvParsed<'_>, tag: u8) -> Option<u16> {
    Some(u16::from_ne_bytes(*parsed.get(tag)?.first_chunk()?))
}

/// Returns the first four value bytes in host byte order, or `None` if there are fewer,
/// where C reads past the value.
pub fn tlvp_val32_unal(parsed: &TlvParsed<'_>, tag: u8) -> Option<u32> {
    Some(u32::from_ne_bytes(*parsed.get(tag)?.first_chunk()?))
}

/// Takes `len` bytes and returns `len`, or returns -1 and empties `data`.
pub fn osmo_shift_v_fixed<'a>(
    data: &mut &'a [u8],
    len: usize,
    value: Option<&mut &'a [u8]>,
) -> i32 {
    match tlv::shift_v_fixed(data, len) {
        Ok(found) => {
            if let Some(value) = value {
                *value = found;
            }
            len as i32
        }
        Err(_) => -1,
    }
}

/// Takes a tag and `len` bytes and returns `len + 1`. Returns 0 for a different tag, or
/// returns -1 and empties `data`.
pub fn osmo_match_shift_tv_fixed<'a>(
    data: &mut &'a [u8],
    tag: u8,
    len: usize,
    value: Option<&mut &'a [u8]>,
) -> i32 {
    match tlv::match_shift_tv_fixed(data, tag, len) {
        Ok(Some(found)) => {
            if let Some(value) = value {
                *value = found;
            }
            (len + 1) as i32
        }
        Ok(None) => 0,
        Err(_) => -1,
    }
}

/// Takes a tag, length and value and returns the bytes taken, or returns -1 and empties
/// `data`.
pub fn osmo_shift_tlv<'a>(
    data: &mut &'a [u8],
    tag: Option<&mut u8>,
    value: Option<&mut &'a [u8]>,
) -> i32 {
    match tlv::shift_tlv(data) {
        Ok((found, found_value)) => {
            if let Some(tag) = tag {
                *tag = found;
            }
            if let Some(value) = value {
                *value = found_value;
            }
            found_value.len() as i32 + 2
        }
        Err(_) => -1,
    }
}

/// Like [`osmo_shift_tlv`], but a different tag returns 0 and restores `data`. C has
/// still written `value` by then.
pub fn osmo_match_shift_tlv<'a>(
    data: &mut &'a [u8],
    expected_tag: u8,
    value: Option<&mut &'a [u8]>,
) -> i32 {
    let before = *data;
    let mut tag = 0;
    let rc = osmo_shift_tlv(data, Some(&mut tag), value);
    if rc > 0 && tag != expected_tag {
        *data = before;
        return 0;
    }
    rc
}

/// Takes a length and value and returns the bytes taken, or returns -1 and empties
/// `data`.
pub fn osmo_shift_lv<'a>(data: &mut &'a [u8], value: Option<&mut &'a [u8]>) -> i32 {
    match tlv::shift_lv(data) {
        Ok(found) => {
            if let Some(value) = value {
                *value = found;
            }
            found.len() as i32 + 1
        }
        Err(_) => -1,
    }
}

/// Returns 0 or the status of the first problem. C also logs each problem; Rust's
/// [`TlvProtocol::validate`] returns the first one with its tag.
pub fn osmo_tlv_prot_validate_tp(
    protocol: &TlvProtocol<'_>,
    message_type: u8,
    parsed: &TlvParsed<'_>,
) -> i32 {
    match protocol.validate(message_type, parsed) {
        Ok(()) => 0,
        Err(error) => tlv_error_code(error),
    }
}

/// Parses into `decoded` and validates `decoded[0]`, returning 0 or a status. Needs at
/// least one entry in `decoded`, as C reads the first.
pub fn osmo_tlv_prot_parse<'a>(
    protocol: &TlvProtocol<'_>,
    decoded: &mut [TlvParsed<'a>],
    message_type: u8,
    buf: &'a [u8],
    lv_tag: u8,
    lv_tag2: u8,
) -> i32 {
    match protocol.parse(decoded, message_type, buf, lv_tags(lv_tag, lv_tag2)) {
        Ok(()) => 0,
        Err(error) => tlv_error_code(error),
    }
}
