// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing elements into a table by tag.

use super::*;

/// One parsed element.
///
/// [`TlvType::T`] has an empty value. [`TlvType::SingleTv`] has one value byte, which
/// is the whole input byte including the tag's half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ie<'a> {
    pub tag: u8,
    pub value: &'a [u8],
}

/// Where a parse stopped, with the outputs C has written by then.
pub(crate) struct ParseFailure {
    pub(crate) error: TlvError,
    pub(crate) tag: Option<u8>,
    pub(crate) len: Option<u16>,
    /// Whether C clears its value pointer rather than leaving it alone.
    pub(crate) clears_value: bool,
}

/// Returns the tag, value offset, value length and bytes used.
pub(crate) fn parse_layout(
    definition: &TlvDefinition,
    buf: &[u8],
) -> Result<(u8, usize, u16, usize), ParseFailure> {
    let Some(&first) = buf.first() else {
        return Err(ParseFailure {
            error: TlvError::OffsetBeyondBuffer,
            tag: None,
            len: None,
            clears_value: false,
        });
    };
    if definition.get(first >> 4).kind == TlvType::SingleTv {
        return Ok((first >> 4, 0, 1, 1));
    }
    if definition.get(first & 0xf0).kind == TlvType::SingleTv {
        return Ok((first & 0xf0, 0, 1, 1));
    }
    let def = definition.get(first);
    let short = |error| ParseFailure {
        error,
        tag: Some(first),
        len: None,
        clears_value: false,
    };
    let byte = |index: usize| {
        buf.get(index)
            .copied()
            .map(u16::from)
            .ok_or(short(TlvError::OffsetBeyondBuffer))
    };
    let two_byte = |first_mask: u16| -> Result<(usize, u16), ParseFailure> {
        // C checks the whole header at once, so one byte short is the same failure.
        let high = byte(1)?;
        let low = byte(2)?;
        Ok((3, ((high & first_mask) << 8) | low))
    };
    let (start, len) = match def.kind {
        TlvType::T => (0, 0),
        TlvType::Tv => (1, 1),
        TlvType::Fixed => (1, u16::from(def.fixed_len)),
        TlvType::Tlv => (2, byte(1)?),
        TlvType::VtvlvGan => {
            if byte(1)? & 0x80 != 0 {
                two_byte(0x7f)?
            } else {
                (2, byte(1)?)
            }
        }
        TlvType::Tvlv => {
            let length = byte(1)?;
            if length & 0x80 != 0 {
                (2, length & 0x7f)
            } else {
                two_byte(0xff)?
            }
        }
        TlvType::Tl16v => two_byte(0xff)?,
        TlvType::None | TlvType::SingleTv => return Err(short(TlvError::UnknownType)),
    };
    let used = match def.kind {
        TlvType::T => 1,
        _ => start + usize::from(len),
    };
    if buf.len() < used {
        return Err(ParseFailure {
            error: TlvError::LengthBeyondBuffer,
            tag: Some(first),
            len: Some(len),
            clears_value: true,
        });
    }
    Ok((first, start, len, used))
}

/// Parse the first element of `buf` and return it with the number of bytes it used.
///
/// A tag whose upper half names a [`TlvType::SingleTv`] entry wins over the tag's own
/// entry, using either the half on its own or, for older definitions, the half in
/// place as `0xN0`. A tag defined as `SingleTv` that matches neither form is
/// [`TlvError::UnknownType`].
pub fn parse_one<'a>(
    definition: &TlvDefinition,
    buf: &'a [u8],
) -> Result<(Ie<'a>, usize), TlvError> {
    let (tag, start, len, used) = parse_layout(definition, buf).map_err(|failure| failure.error)?;
    let value = &buf[start..start + usize::from(len)];
    Ok((Ie { tag, value }, used))
}

/// The elements of a message, by tag, borrowing from the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlvParsed<'a> {
    values: [Option<&'a [u8]>; 256],
}

impl Default for TlvParsed<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> TlvParsed<'a> {
    pub const fn new() -> Self {
        Self {
            values: [None; 256],
        }
    }
    /// The value of a tag, which is empty for a present element with no value.
    pub fn get(&self, tag: u8) -> Option<&'a [u8]> {
        self.values[usize::from(tag)]
    }
    pub fn set(&mut self, tag: u8, value: Option<&'a [u8]>) {
        self.values[usize::from(tag)] = value;
    }
    pub fn is_present(&self, tag: u8) -> bool {
        self.get(tag).is_some()
    }
    /// The value of a tag, if it's present with at least `min_len` bytes.
    pub fn get_min_len(&self, tag: u8, min_len: usize) -> Option<&'a [u8]> {
        self.get(tag).filter(|value| value.len() >= min_len)
    }
    /// The first value byte of a tag, or `default` if it has none.
    pub fn val8(&self, tag: u8, default: u8) -> u8 {
        self.get_min_len(tag, 1).map_or(default, |value| value[0])
    }
    /// The first two value bytes of a tag as a big-endian number.
    pub fn val16be(&self, tag: u8) -> Option<u16> {
        let value = self.get(tag)?.first_chunk()?;
        Some(u16::from_be_bytes(*value))
    }
    /// The first four value bytes of a tag as a big-endian number.
    pub fn val32be(&self, tag: u8) -> Option<u32> {
        let value = self.get(tag)?.first_chunk()?;
        Some(u32::from_be_bytes(*value))
    }
    /// Present elements in tag order.
    pub fn iter(&self) -> impl Iterator<Item = Ie<'a>> + '_ {
        (0..=255).filter_map(|tag| {
            Some(Ie {
                tag,
                value: self.get(tag)?,
            })
        })
    }
    /// Replace each tag that `source` has with a non-empty value. An empty value in
    /// `source` doesn't replace or add anything.
    pub fn merge(&mut self, source: &Self) {
        for (value, source) in self.values.iter_mut().zip(&source.values) {
            if source.is_some_and(|source| !source.is_empty()) {
                *value = *source;
            }
        }
    }
}

/// Parse a message, keeping the first occurrence of each tag and returning the number
/// of elements found. `lv_tags` name up to two length-value elements at the start of
/// `buf`, which have no tag on the wire.
pub fn parse<'a>(
    definition: &TlvDefinition,
    buf: &'a [u8],
    lv_tags: [Option<u8>; 2],
) -> Result<(TlvParsed<'a>, usize), TlvError> {
    let mut parsed = [TlvParsed::new()];
    let count = parse_multiple(&mut parsed, definition, buf, lv_tags)?;
    let [parsed] = parsed;
    Ok((parsed, count))
}

/// Parse a message into several [`TlvParsed`]: the first occurrence of each tag goes
/// in the first, the second in the second, and so on. Further occurrences still count
/// but are dropped. Returns the number of elements found.
///
/// The `lv_tags` elements are different. If both name one tag, the second replaces the
/// first in `decoded[0]`, or is dropped when `decoded` has one entry. This matches C.
///
/// On an error, `decoded` keeps the elements found before it.
pub fn parse_multiple<'a>(
    decoded: &mut [TlvParsed<'a>],
    definition: &TlvDefinition,
    buf: &'a [u8],
    lv_tags: [Option<u8>; 2],
) -> Result<usize, TlvError> {
    decoded.fill(TlvParsed::new());
    let mut offset = 0;
    let mut count = 0;
    for tag in lv_tags.into_iter().flatten() {
        // An exhausted input is reported as a bad length, as C does.
        let &len = buf.get(offset).ok_or(TlvError::LengthBeyondBuffer)?;
        let value = buf
            .get(offset + 1..offset + 1 + usize::from(len))
            .ok_or(TlvError::LengthBeyondBuffer)?;
        offset += 1 + usize::from(len);
        count += 1;
        if decoded.iter().any(|parsed| !parsed.is_present(tag))
            && let Some(first) = decoded.first_mut()
        {
            first.set(tag, Some(value));
        }
    }
    while offset < buf.len() {
        let (ie, used) = parse_one(definition, &buf[offset..])?;
        if let Some(parsed) = decoded.iter_mut().find(|parsed| !parsed.is_present(ie.tag)) {
            parsed.set(ie.tag, Some(ie.value));
        }
        offset += used;
        count += 1;
    }
    Ok(count)
}
