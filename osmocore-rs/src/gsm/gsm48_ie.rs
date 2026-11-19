// SPDX-License-Identifier: GPL-3.0-or-later

//! The BCD numbers of 3GPP TS 24.008: the digits of the called, calling and connected party
//! numbers (section 10.5.4.7), and of the other IEs that share their layout.
//!
//! Such an IE's value is a length byte, a header the IE defines (the type of number and
//! numbering plan, and in some IEs presentation and screening), then two digits per byte,
//! low nibble first, with an F filler after an odd count. A [`BcdNumber`] holds the digits,
//! and the caller supplies and reads the header.
//!
//! ```
//! use osmocore_rs::gsm::gsm48_ie::BcdNumber;
//! let number: BcdNumber = "123456789".parse()?;
//! assert_eq!(number.encode(&[])?, [0x05, 0x21, 0x43, 0x65, 0x87, 0xf9]);
//!
//! // An international ISDN number, with a one-byte header.
//! let calling = number.encode(&[0x91])?;
//! assert_eq!(calling, [0x06, 0x91, 0x21, 0x43, 0x65, 0x87, 0xf9]);
//! assert_eq!(BcdNumber::decode(&calling, 1)?, number);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// The characters of the digit nibbles 0 to 14 (3GPP TS 24.008 table 10.5.118).
pub(crate) const DIGITS: &[u8; 15] = b"0123456789*#abc";
/// The nibble after an odd number of digits (3GPP TS 24.008 table 10.5.118).
pub(crate) const FILLER: u8 = 0x0f;
/// The most bytes the length byte counts: the header and the digits.
pub const MAX_CONTENT_BYTES: usize = 255;

/// The nibble of one character of a number.
pub(crate) fn nibble(character: u8) -> Option<u8> {
    DIGITS
        .iter()
        .position(|&digit| digit == character)
        .map(|nibble| nibble as u8)
}

/// Errors from building, encoding or decoding a BCD number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BcdNumberError {
    /// Text with a character other than a decimal digit, `*`, `#`, `a`, `b` or `c`. C's
    /// `-EINVAL`.
    Digits,
    /// A header and digits of more than [`MAX_CONTENT_BYTES`], more than the length byte
    /// counts. C writes the length byte modulo 256 instead.
    TooLong,
    /// No length byte. C's `-EIO`.
    Empty,
    /// A length byte past the end of the input. C's `-EINVAL`.
    Truncated,
}

impl fmt::Display for BcdNumberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Digits => "not the digits of a BCD number",
            Self::TooLong => "BCD number too long for its length byte",
            Self::Empty => "BCD number has no length byte",
            Self::Truncated => "BCD number is shorter than its length byte says",
        })
    }
}

impl Error for BcdNumberError {}

/// The digits of a BCD number: decimal digits, `*`, `#`, `a`, `b` and `c`, in lowercase as C
/// prints them. It may be empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BcdNumber(String);

impl BcdNumber {
    /// The digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The bytes [`encode`](Self::encode) gives with a header of `header_len` bytes: the
    /// length byte, the header and the digits.
    pub fn encoded_len(&self, header_len: usize) -> usize {
        1 + header_len + self.0.len().div_ceil(2)
    }

    /// The length byte, `header`, then the digits, with an F filler after an odd count. C's
    /// `gsm48_encode_bcd_number`, with the header written too.
    pub fn encode(&self, header: &[u8]) -> Result<Vec<u8>, BcdNumberError> {
        let len = self.encoded_len(header.len());
        let content = u8::try_from(len - 1).map_err(|_| BcdNumberError::TooLong)?;
        let mut lv = Vec::with_capacity(len);
        lv.push(content);
        lv.extend_from_slice(header);
        lv.extend(self.0.as_bytes().chunks(2).map(|pair| {
            let low = nibble(pair[0]).expect("checked digit");
            let high = pair
                .get(1)
                .map_or(FILLER, |&digit| nibble(digit).expect("checked digit"));
            high << 4 | low
        }));
        Ok(lv)
    }

    /// Decodes the digits of `lv`, a length byte and the bytes it counts, after a header of
    /// `header_len` bytes. Bytes past the ones the length byte counts are ignored, and a
    /// header past them gives no digits. The number ends at the first filler, where C's text
    /// ends. C's `gsm48_decode_bcd_number2`, without its output buffer.
    pub fn decode(lv: &[u8], header_len: usize) -> Result<Self, BcdNumberError> {
        let (&content, rest) = lv.split_first().ok_or(BcdNumberError::Empty)?;
        let content = rest
            .get(..usize::from(content))
            .ok_or(BcdNumberError::Truncated)?;
        let digits = content.get(header_len..).unwrap_or_default();
        let number = digits
            .iter()
            .flat_map(|byte| [byte & 0x0f, byte >> 4])
            .take_while(|&nibble| nibble != FILLER)
            .map(|nibble| char::from(DIGITS[usize::from(nibble)]))
            .collect();
        Ok(Self(number))
    }
}

impl FromStr for BcdNumber {
    type Err = BcdNumberError;

    fn from_str(text: &str) -> Result<Self, BcdNumberError> {
        if text.bytes().all(|character| nibble(character).is_some()) {
            Ok(Self(text.to_owned()))
        } else {
            Err(BcdNumberError::Digits)
        }
    }
}

impl fmt::Display for BcdNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
