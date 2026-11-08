// SPDX-License-Identifier: GPL-3.0-or-later

//! Encoding elements, and the `Msgb` methods that append and prepend them.

use super::*;

/// Append one element in the given format. [`TlvType::None`] appends nothing, and
/// [`TlvType::T`] ignores `value`. [`TlvType::Fixed`] writes `value` whatever its
/// length. [`TlvType::SingleTv`] puts `tag` in the upper half, so it takes the
/// half-octet tag, not `0xN0`.
pub fn encode_one(msg: &mut Msgb, kind: TlvType, tag: u8, value: &[u8]) -> Result<(), TlvError> {
    match kind {
        TlvType::None => Ok(()),
        TlvType::Fixed => msg.put_tv_fixed(tag, value),
        TlvType::T => msg.put_v(tag),
        TlvType::Tv => msg.put_tv(tag, *value.first().ok_or(TlvError::ValueMissing)?),
        TlvType::Tlv => msg.put_tlv(tag, value),
        TlvType::Tl16v => msg.put_tl16v(tag, value),
        TlvType::Tvlv => msg.put_tvlv(tag, value),
        TlvType::SingleTv => {
            let low = *value.first().ok_or(TlvError::ValueMissing)? & 0x0f;
            msg.put_v((tag << 4) | low)
        }
        TlvType::VtvlvGan => msg.put_vtvlv_gan(u16::from(tag), value),
    }
}

/// Append every element of `parsed` in tag order, returning the bytes written. A tag
/// `definition` doesn't define is skipped. On an error, `msg` is left as it was.
pub fn encode(
    msg: &mut Msgb,
    definition: &TlvDefinition,
    parsed: &TlvParsed<'_>,
) -> Result<usize, TlvError> {
    encode_tags(msg, definition, parsed, 0..=255)
}

/// Append the elements of `parsed` in the order of `tags`, returning the bytes
/// written. A repeated tag is written each time, and an absent one is skipped. On an
/// error, `msg` is left as it was.
pub fn encode_ordered(
    msg: &mut Msgb,
    definition: &TlvDefinition,
    parsed: &TlvParsed<'_>,
    tags: &[u8],
) -> Result<usize, TlvError> {
    encode_tags(msg, definition, parsed, tags.iter().copied())
}

fn encode_tags(
    msg: &mut Msgb,
    definition: &TlvDefinition,
    parsed: &TlvParsed<'_>,
    tags: impl Iterator<Item = u8>,
) -> Result<usize, TlvError> {
    let before = msg.tail_offset();
    for tag in tags {
        let Some(value) = parsed.get(tag) else {
            continue;
        };
        if let Err(error) = encode_one(msg, definition.get(tag).kind, tag, value) {
            msg.get(msg.tail_offset() - before)?;
            return Err(error);
        }
    }
    Ok(msg.tail_offset() - before)
}

/// The bytes a length-value element takes.
pub const fn lv_gross_len(len: usize) -> usize {
    len + 1
}
/// The bytes a tag-length-value element takes.
pub const fn tlv_gross_len(len: usize) -> usize {
    len + 2
}
/// The bytes a tag-length-value element takes with `len` 16-bit values.
pub const fn tlv16_gross_len(len: usize) -> usize {
    2 * len + 2
}
/// The bytes an element with a two-byte length or tag takes.
pub const fn tl16v_gross_len(len: usize) -> usize {
    len + 3
}
/// The bytes a [`TlvType::Tvlv`] element takes.
pub const fn tvlv_gross_len(len: usize) -> usize {
    if len <= ONE_BYTE_MAX {
        tlv_gross_len(len)
    } else {
        tl16v_gross_len(len)
    }
}
/// The bytes of a [`TlvType::VtvlvGan`] tag and length.
pub const fn vtvl_gan_gross_len(tag: usize, len: usize) -> usize {
    2 + (tag > ONE_BYTE_MAX) as usize + (len > ONE_BYTE_MAX) as usize
}
/// The bytes a [`TlvType::VtvlvGan`] element takes.
pub const fn vtvlv_gan_gross_len(tag: usize, len: usize) -> usize {
    vtvl_gan_gross_len(tag, len) + len
}

pub(crate) fn tvlv_header(tag: u8, len: u16) -> ([u8; 3], usize) {
    if usize::from(len) <= ONE_BYTE_MAX {
        ([tag, 0x80 | len as u8, 0], 2)
    } else {
        ([tag, (len >> 8) as u8, len as u8], 3)
    }
}

/// One tag or length of the GAN format, in up to two bytes.
pub(crate) fn gan_field(value: u16, out: &mut [u8]) -> usize {
    if usize::from(value) > ONE_BYTE_MAX {
        out[0] = 0x80 | (value >> 8) as u8;
        out[1] = value as u8;
        2
    } else {
        out[0] = value as u8;
        1
    }
}

pub(crate) fn gan_header(tag: u16, len: u16) -> ([u8; 4], usize) {
    let mut header = [0; 4];
    let tag_len = gan_field(tag, &mut header);
    let total = tag_len + gan_field(len, &mut header[tag_len..]);
    (header, total)
}

/// Writing a two-byte length only needs these three bytes.
fn tl16_header(tag: u8, len: u16) -> [u8; 3] {
    [tag, (len >> 8) as u8, len as u8]
}

fn checked_len<T: TryFrom<usize>>(value: &[u8]) -> Result<T, TlvError> {
    T::try_from(value.len()).map_err(|_| TlvError::ValueTooLong)
}

fn checked_variable_len(len: usize) -> Result<u16, TlvError> {
    if len > VARIABLE_MAX {
        return Err(TlvError::ValueTooLong);
    }
    Ok(len as u16)
}

fn checked_variable_tag(tag: u16) -> Result<u16, TlvError> {
    if usize::from(tag) > VARIABLE_MAX {
        return Err(TlvError::TagTooLarge);
    }
    Ok(tag)
}

impl Msgb {
    /// Append `parts` as one block, so that running out of room writes nothing.
    pub(crate) fn put_parts(&mut self, parts: &[&[u8]]) -> Result<usize, TlvError> {
        let total = parts.iter().map(|part| part.len()).sum();
        let start = self.put(total)?;
        self.write_parts(start, parts);
        Ok(start)
    }
    /// Prepend `parts` as one block, so that running out of room writes nothing.
    pub(crate) fn push_parts(&mut self, parts: &[&[u8]]) -> Result<usize, TlvError> {
        let total = parts.iter().map(|part| part.len()).sum();
        let start = self.push(total)?;
        self.write_parts(start, parts);
        Ok(start)
    }
    pub(crate) fn write_parts(&mut self, mut offset: usize, parts: &[&[u8]]) {
        for part in parts {
            self.backing_mut()[offset..offset + part.len()].copy_from_slice(part);
            offset += part.len();
        }
    }

    /// Append a length and value.
    pub fn put_lv(&mut self, value: &[u8]) -> Result<(), TlvError> {
        let len: u8 = checked_len(value)?;
        self.put_parts(&[&[len], value]).map(drop)
    }
    /// Append a tag, one-byte length and value.
    pub fn put_tlv(&mut self, tag: u8, value: &[u8]) -> Result<(), TlvError> {
        let len: u8 = checked_len(value)?;
        self.put_parts(&[&[tag, len], value]).map(drop)
    }
    /// Append a tag, one-byte length and `len` zero bytes.
    pub fn put_tlv_zeroed(&mut self, tag: u8, len: u8) -> Result<(), TlvError> {
        let start = self.put(tlv_gross_len(usize::from(len)))?;
        self.write_parts(start, &[&[tag, len]]);
        self.backing_mut()[start + 2..start + 2 + usize::from(len)].fill(0);
        Ok(())
    }
    /// Append a tag and a zero length, returning the length byte's backing offset so
    /// it can be set once the value has been added.
    pub fn put_tl(&mut self, tag: u8) -> Result<usize, TlvError> {
        Ok(self.put_parts(&[&[tag, 0]])? + 1)
    }
    /// Append a tag, a one-byte count and that many 16-bit values in host byte order.
    pub fn put_tlv16(&mut self, tag: u8, values: &[u16]) -> Result<(), TlvError> {
        let len = u8::try_from(values.len()).map_err(|_| TlvError::ValueTooLong)?;
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        self.put_parts(&[&[tag, len], &bytes]).map(drop)
    }
    /// Append a tag, two-byte length and value.
    pub fn put_tl16v(&mut self, tag: u8, value: &[u8]) -> Result<(), TlvError> {
        let len: u16 = checked_len(value)?;
        self.put_parts(&[&tl16_header(tag, len), value]).map(drop)
    }
    /// Append a two-byte tag, one-byte length and value.
    pub fn put_t16lv(&mut self, tag: u16, value: &[u8]) -> Result<(), TlvError> {
        let len: u8 = checked_len(value)?;
        let [high, low] = tag.to_be_bytes();
        self.put_parts(&[&[high, low, len], value]).map(drop)
    }
    /// Append a two-byte length, tag and value.
    pub fn put_l16tv(&mut self, tag: u8, value: &[u8]) -> Result<(), TlvError> {
        let len: u16 = checked_len(value)?;
        let [high, low] = len.to_be_bytes();
        self.put_parts(&[&[high, low, tag], value]).map(drop)
    }
    /// Append a [`TlvType::Tvlv`] tag and length, and reserve `len` bytes for the
    /// value, without writing them. Returns the value's backing offset.
    pub fn put_tvl(&mut self, tag: u8, len: u16) -> Result<usize, TlvError> {
        let len = checked_variable_len(usize::from(len))?;
        let (header, header_len) = tvlv_header(tag, len);
        let start = self.put(header_len + usize::from(len))?;
        self.write_parts(start, &[&header[..header_len]]);
        Ok(start + header_len)
    }
    /// Append a [`TlvType::Tvlv`] element.
    pub fn put_tvlv(&mut self, tag: u8, value: &[u8]) -> Result<(), TlvError> {
        let (header, header_len) = tvlv_header(tag, checked_variable_len(value.len())?);
        self.put_parts(&[&header[..header_len], value]).map(drop)
    }
    /// Append a [`TlvType::Tvlv`] element holding a big-endian number.
    pub fn put_tvlv_u16(&mut self, tag: u8, value: u16) -> Result<(), TlvError> {
        self.put_tvlv(tag, &value.to_be_bytes())
    }
    /// Append a [`TlvType::Tvlv`] element holding a big-endian number.
    pub fn put_tvlv_u32(&mut self, tag: u8, value: u32) -> Result<(), TlvError> {
        self.put_tvlv(tag, &value.to_be_bytes())
    }
    /// Append a [`TlvType::VtvlvGan`] element, with a one- or two-byte tag.
    pub fn put_vtvlv_gan(&mut self, tag: u16, value: &[u8]) -> Result<(), TlvError> {
        let (header, header_len) = gan_header(
            checked_variable_tag(tag)?,
            checked_variable_len(value.len())?,
        );
        self.put_parts(&[&header[..header_len], value]).map(drop)
    }
    /// Append one byte.
    pub fn put_v(&mut self, value: u8) -> Result<(), TlvError> {
        self.put_parts(&[&[value]]).map(drop)
    }
    /// Append a tag and one value byte.
    pub fn put_tv(&mut self, tag: u8, value: u8) -> Result<(), TlvError> {
        self.put_parts(&[&[tag, value]]).map(drop)
    }
    /// Append a tag and value with no length.
    pub fn put_tv_fixed(&mut self, tag: u8, value: &[u8]) -> Result<(), TlvError> {
        self.put_parts(&[&[tag], value]).map(drop)
    }
    /// Append a tag and big-endian number.
    pub fn put_tv16(&mut self, tag: u8, value: u16) -> Result<(), TlvError> {
        self.put_parts(&[&[tag], &value.to_be_bytes()]).map(drop)
    }
    /// Append a tag and big-endian number.
    pub fn put_tv32(&mut self, tag: u8, value: u32) -> Result<(), TlvError> {
        self.put_parts(&[&[tag], &value.to_be_bytes()]).map(drop)
    }

    /// Prepend a tag, one-byte length and value.
    pub fn push_tlv(&mut self, tag: u8, value: &[u8]) -> Result<(), TlvError> {
        let len: u8 = checked_len(value)?;
        self.push_parts(&[&[tag, len], value]).map(drop)
    }
    /// Prepend a tag, a length of one and one value byte.
    pub fn push_tlv1(&mut self, tag: u8, value: u8) -> Result<(), TlvError> {
        self.push_tlv(tag, &[value])
    }
    /// Prepend a tag and one value byte.
    pub fn push_tv(&mut self, tag: u8, value: u8) -> Result<(), TlvError> {
        self.push_parts(&[&[tag, value]]).map(drop)
    }
    /// Prepend a tag and big-endian number.
    pub fn push_tv16(&mut self, tag: u8, value: u16) -> Result<(), TlvError> {
        self.push_parts(&[&[tag], &value.to_be_bytes()]).map(drop)
    }
    /// Prepend a tag and big-endian number.
    pub fn push_tv32(&mut self, tag: u8, value: u32) -> Result<(), TlvError> {
        self.push_parts(&[&[tag], &value.to_be_bytes()]).map(drop)
    }
    /// Prepend a [`TlvType::Tvlv`] element.
    pub fn push_tvlv(&mut self, tag: u8, value: &[u8]) -> Result<(), TlvError> {
        let (header, header_len) = tvlv_header(tag, checked_variable_len(value.len())?);
        self.push_parts(&[&header[..header_len], value]).map(drop)
    }
    /// Prepend a [`TlvType::VtvlvGan`] tag and length, with no value.
    pub fn push_vtvl_gan(&mut self, tag: u16, len: u16) -> Result<(), TlvError> {
        let (header, header_len) = gan_header(
            checked_variable_tag(tag)?,
            checked_variable_len(usize::from(len))?,
        );
        self.push_parts(&[&header[..header_len]]).map(drop)
    }
    /// Prepend a [`TlvType::VtvlvGan`] element.
    pub fn push_vtvlv_gan(&mut self, tag: u16, value: &[u8]) -> Result<(), TlvError> {
        let (header, header_len) = gan_header(
            checked_variable_tag(tag)?,
            checked_variable_len(value.len())?,
        );
        self.push_parts(&[&header[..header_len], value]).map(drop)
    }
}
