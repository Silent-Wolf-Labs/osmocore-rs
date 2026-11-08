// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing and encoding of tag-length-value information elements.
//!
//! A [`TlvDefinition`] gives the format of each of the 256 tags. [`parse`] splits a
//! message into a [`TlvParsed`], whose values borrow from the input buffer, and
//! [`encode`] writes one back into a [`Msgb`], which also gains a method for each
//! format, such as [`Msgb::put_tlv`].
//!
//! ```
//! use osmocore_rs::base::msgb::Msgb;
//! use osmocore_rs::gsm::tlv::{TlvDef, TlvDefinition, TlvType, encode, parse};
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

use crate::base::msgb::{Msgb, MsgbError};
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

mod encode;
mod parse;
mod protocol;
mod shift;

pub use encode::*;
pub use parse::*;
pub use protocol::*;
pub use shift::*;
