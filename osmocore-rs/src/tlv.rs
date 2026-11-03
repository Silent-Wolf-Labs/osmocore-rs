// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing and encoding of tag-length-value information elements.
//!
//! A [`TlvDefinition`] gives the format of each of the 256 tags. [`parse`] splits a
//! message into a [`TlvParsed`], whose values borrow from the input buffer, and
//! [`encode`] writes one back into a [`Msgb`], which also gains a method for each
//! format, such as [`Msgb::put_tlv`].
//!
//! ```
//! use osmocore_rs::msgb::Msgb;
//! use osmocore_rs::tlv::{TlvDef, TlvDefinition, TlvType, encode, parse};
//!
//! let mut definition = TlvDefinition::new();
//! definition.set(0x10, TlvDef::new(TlvType::Tlv));
//! definition.set(0x20, TlvDef::new(TlvType::Tv));
//!
//! let message = [0x10, 0x02, 0xaa, 0xbb, 0x20, 0xcc];
//! let (parsed, count) = parse(&definition, &message, [None, None]).unwrap();
//! assert_eq!(count, 2);
//! assert_eq!(parsed.get(0x10), Some(&[0xaa, 0xbb][..]));
//! assert_eq!(parsed.val8(0x20, 0), 0xcc);
//!
//! let mut msg = Msgb::new(16).unwrap();
//! encode(&mut msg, &definition, &parsed).unwrap();
//! assert_eq!(msg.data(), message);
//! ```

use crate::msgb::{Msgb, MsgbError};
use std::borrow::Cow;
use std::fmt;

/// The longest value or tag the variable-length formats can represent. Their first
/// byte's high bit selects the form, which leaves 15 bits.
pub const VARIABLE_MAX: usize = 0x7fff;
const ONE_BYTE_MAX: usize = 0x7f;

/// Why a TLV operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlvError {
    /// The input ended inside a tag or length.
    OffsetBeyondBuffer,
    /// A length runs past the end of the input.
    LengthBeyondBuffer,
    /// The definition has no usable format for this tag.
    UnknownType,
    /// A message lacks this mandatory element.
    MandatoryIeMissing { iei: u8 },
    /// This element is shorter than its protocol's minimum.
    IeTooShort { iei: u8, len: u16, min_len: u16 },
    /// A value doesn't fit its format's length field.
    ValueTooLong,
    /// A format that needs a value byte was given none.
    ValueMissing,
    /// A tag doesn't fit its format's tag field.
    TagTooLarge,
    /// The message buffer has no room.
    Msgb(MsgbError),
}

impl fmt::Display for TlvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OffsetBeyondBuffer => f.write_str("TLV header runs past the input"),
            Self::LengthBeyondBuffer => f.write_str("TLV length runs past the input"),
            Self::UnknownType => f.write_str("tag has no TLV format"),
            Self::MandatoryIeMissing { iei } => write!(f, "mandatory IE {iei:#04x} is missing"),
            Self::IeTooShort { iei, len, min_len } => {
                write!(f, "IE {iei:#04x} has {len} bytes, needs {min_len}")
            }
            Self::ValueTooLong => f.write_str("value doesn't fit the TLV length field"),
            Self::ValueMissing => f.write_str("TLV format needs a value byte"),
            Self::TagTooLarge => f.write_str("tag doesn't fit the TLV tag field"),
            Self::Msgb(error) => write!(f, "message buffer: {error:?}"),
        }
    }
}

impl std::error::Error for TlvError {}

impl From<MsgbError> for TlvError {
    fn from(error: MsgbError) -> Self {
        Self::Msgb(error)
    }
}

/// The wire format of a tag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TlvType {
    /// Not defined.
    #[default]
    None,
    /// A tag and [`TlvDef::fixed_len`] value bytes.
    Fixed,
    /// A tag alone.
    T,
    /// A tag and one value byte.
    Tv,
    /// A tag, one length byte and the value.
    Tlv,
    /// A tag, a two-byte length and the value.
    Tl16v,
    /// A tag, a one- or two-byte length and the value. A set high bit in the first
    /// length byte marks the one-byte form.
    Tvlv,
    /// A four-bit tag and four-bit value in one byte.
    SingleTv,
    /// A tag, a one- or two-byte length and the value, as in 3GPP TS 44.318. A set
    /// high bit in the first length byte marks the two-byte form.
    VtvlvGan,
}

/// The format of one tag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlvDef {
    pub kind: TlvType,
    /// The value length for [`TlvType::Fixed`].
    pub fixed_len: u8,
}

impl TlvDef {
    pub const fn new(kind: TlvType) -> Self {
        Self { kind, fixed_len: 0 }
    }
    pub const fn fixed(fixed_len: u8) -> Self {
        Self {
            kind: TlvType::Fixed,
            fixed_len,
        }
    }
}

/// The format of all 256 tags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlvDefinition {
    defs: [TlvDef; 256],
}

impl Default for TlvDefinition {
    fn default() -> Self {
        Self::new()
    }
}

impl TlvDefinition {
    /// A definition with no tag defined.
    pub const fn new() -> Self {
        Self::all(TlvType::None)
    }
    /// A definition giving every tag the same format.
    pub const fn all(kind: TlvType) -> Self {
        Self {
            defs: [TlvDef::new(kind); 256],
        }
    }
    pub fn set(&mut self, tag: u8, def: TlvDef) {
        self.defs[usize::from(tag)] = def;
    }
    pub fn get(&self, tag: u8) -> TlvDef {
        self.defs[usize::from(tag)]
    }
    /// Fill every undefined tag from `source`, leaving defined tags alone.
    pub fn patch(&mut self, source: &Self) {
        for (def, source) in self.defs.iter_mut().zip(&source.defs) {
            if def.kind == TlvType::None {
                *def = *source;
            }
        }
    }
}

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

/// What a protocol requires of one message type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlvMessageDef<'a> {
    pub name: Option<&'a str>,
    /// Tags this message must have.
    pub mandatory: &'a [u8],
    /// Free for the protocol's own use, such as direction.
    pub flags: u32,
}

/// What a protocol requires of one tag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlvIeDef<'a> {
    pub min_len: u16,
    pub name: Option<&'a str>,
}

/// A protocol's message and element rules, on top of a [`TlvDefinition`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlvProtocol<'a> {
    pub name: &'a str,
    pub definition: &'a TlvDefinition,
    /// Names for message types with no [`TlvMessageDef::name`].
    pub message_names: Option<&'a [(u8, &'a str)]>,
    messages: [TlvMessageDef<'a>; 256],
    ies: [TlvIeDef<'a>; 256],
}

impl<'a> TlvProtocol<'a> {
    pub fn new(name: &'a str, definition: &'a TlvDefinition) -> Self {
        Self {
            name,
            definition,
            message_names: None,
            messages: [TlvMessageDef {
                name: None,
                mandatory: &[],
                flags: 0,
            }; 256],
            ies: [TlvIeDef {
                min_len: 0,
                name: None,
            }; 256],
        }
    }
    pub fn set_message(&mut self, message_type: u8, def: TlvMessageDef<'a>) {
        self.messages[usize::from(message_type)] = def;
    }
    pub fn set_ie(&mut self, iei: u8, def: TlvIeDef<'a>) {
        self.ies[usize::from(iei)] = def;
    }
    pub fn flags(&self, message_type: u8) -> u32 {
        self.messages[usize::from(message_type)].flags
    }
    pub fn message_name(&self, message_type: u8) -> Cow<'a, str> {
        if let Some(name) = self.messages[usize::from(message_type)].name {
            return Cow::Borrowed(name);
        }
        match self.message_names {
            Some(names) => names
                .iter()
                .find(|(value, _)| *value == message_type)
                .map_or_else(
                    || Cow::Owned(format!("unknown 0x{message_type:x}")),
                    |(_, name)| Cow::Borrowed(*name),
                ),
            None => Cow::Owned(format!("Unknown msg_type 0x{message_type:02x}")),
        }
    }
    pub fn ie_name(&self, iei: u8) -> Cow<'a, str> {
        match self.ies[usize::from(iei)].name {
            Some(name) => Cow::Borrowed(name),
            None => Cow::Owned(format!("Unknown IEI 0x{iei:02x}")),
        }
    }
    /// Check a parsed message: first that its mandatory elements are present, in the
    /// order listed, then that its elements meet their minimum lengths, in tag order.
    /// Returns the first problem.
    pub fn validate(&self, message_type: u8, parsed: &TlvParsed<'_>) -> Result<(), TlvError> {
        let message = &self.messages[usize::from(message_type)];
        if let Some(&iei) = message
            .mandatory
            .iter()
            .find(|iei| !parsed.is_present(**iei))
        {
            return Err(TlvError::MandatoryIeMissing { iei });
        }
        for ie in parsed.iter() {
            let min_len = self.ies[usize::from(ie.tag)].min_len;
            if ie.value.len() < usize::from(min_len) {
                return Err(TlvError::IeTooShort {
                    iei: ie.tag,
                    len: ie.value.len() as u16,
                    min_len,
                });
            }
        }
        Ok(())
    }
    /// Parse a message with [`parse_multiple`] and validate `decoded[0]`. With an empty
    /// `decoded`, only the mandatory and length rules for no elements apply.
    pub fn parse<'b>(
        &self,
        decoded: &mut [TlvParsed<'b>],
        message_type: u8,
        buf: &'b [u8],
        lv_tags: [Option<u8>; 2],
    ) -> Result<(), TlvError> {
        parse_multiple(decoded, self.definition, buf, lv_tags)?;
        match decoded.first() {
            Some(first) => self.validate(message_type, first),
            None => self.validate(message_type, &TlvParsed::new()),
        }
    }
}
