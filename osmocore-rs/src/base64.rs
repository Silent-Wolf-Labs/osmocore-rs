// SPDX-License-Identifier: GPL-3.0-or-later

//! Base64 encoding and decoding with checked buffer operations.
//!
//! These functions retain the selected C API's observable contract: callers
//! supply the output buffer and its length is its slice length; `olen` is
//! updated exactly where the reference does. Encoding writes a trailing NUL
//! after its reported output, so a successful non-empty encode needs one more
//! byte than `olen`. `None` models a null decode destination for size probes.

/// C-compatible result for an output buffer that is too small (`-ENOBUFS`).
pub(crate) const BASE64_BUFFER_TOO_SMALL: i32 = -105;
/// C-compatible result for malformed encoded input (`-EINVAL`).
pub(crate) const BASE64_INVALID_INPUT: i32 = -22;

const ENCODE_MAP: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        b'=' => Some(64),
        _ => None,
    }
}

/// A borrowed input whose line grammar and terminal padding have been checked.
/// Validation deliberately precedes capacity checks and all destination writes.
struct ValidatedBase64<'a> {
    source: &'a [u8],
    characters: usize,
    padding: usize,
}

impl<'a> ValidatedBase64<'a> {
    fn parse(source: &'a [u8]) -> Option<Self> {
        let mut characters = 0;
        let mut padding = 0;
        for line in source.split_inclusive(|&byte| byte == b'\n') {
            // CR is accepted only as part of CRLF. Spaces may end a line
            // (before its CRLF/LF) or end the input, but not precede a token.
            let payload = match line.strip_suffix(b"\n") {
                Some(line) => line.strip_suffix(b"\r").unwrap_or(line),
                None => line,
            };
            let end = payload
                .iter()
                .rposition(|&byte| byte != b' ')
                .map_or(0, |i| i + 1);
            for &byte in &payload[..end] {
                match decode_value(byte)? {
                    64 if padding < 2 => padding += 1,
                    64 => return None,
                    _ if padding != 0 => return None,
                    _ => {}
                }
                characters += 1;
            }
        }
        Some(Self {
            source,
            characters,
            padding,
        })
    }

    fn required_capacity(&self) -> usize {
        ((self.characters * 6 + 7) >> 3) - self.padding
    }

    fn quartets(&self) -> impl Iterator<Item = DecodedQuartet> + '_ {
        // Only whitespace can fail classification after validation. Incomplete
        // tails contribute to the capacity estimate but produce no bytes.
        let mut tokens = self.source.iter().filter_map(|&byte| decode_value(byte));
        std::iter::from_fn(move || {
            let values = [
                tokens.next()?,
                tokens.next()?,
                tokens.next()?,
                tokens.next()?,
            ];
            Some(DecodedQuartet::new(values))
        })
    }
}

struct DecodedQuartet {
    bytes: [u8; 3],
    len: usize,
}

impl DecodedQuartet {
    fn new(values: [u8; 4]) -> Self {
        let [a, b, c, d] = values.map(|value| value & 63);
        Self {
            bytes: [(a << 2) | (b >> 4), (b << 4) | (c >> 2), (c << 6) | d],
            len: 3 - values.iter().filter(|&&value| value == 64).count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::legacy::*;
    use super::*;

    #[test]
    fn round_trip_valid_byte_strings() {
        for input in [b"f".as_slice(), b"fo", b"foo", b"foobar", &[0, 255, 1, 2]] {
            let mut encoded = vec![0; input.len().div_ceil(3) * 4 + 1];
            let mut encoded_len = usize::MAX;
            assert_eq!(osmo_base64_encode(&mut encoded, &mut encoded_len, input), 0);
            let mut decoded = vec![0; input.len()];
            let mut decoded_len = usize::MAX;
            assert_eq!(
                osmo_base64_decode(
                    Some(&mut decoded),
                    &mut decoded_len,
                    &encoded[..encoded_len]
                ),
                0
            );
            assert_eq!(&decoded[..decoded_len], input);
        }
    }

    #[test]
    fn empty_decode_preserves_olen_as_in_c() {
        let mut output = [];
        let mut olen = 7;
        assert_eq!(osmo_base64_decode(Some(&mut output), &mut olen, b""), 0);
        assert_eq!(olen, 7);
    }

    #[test]
    fn too_small_buffers_report_required_capacity() {
        let mut output = [0xa5; 4];
        let mut olen = 7;
        assert_eq!(
            osmo_base64_encode(&mut output, &mut olen, b"foo"),
            BASE64_BUFFER_TOO_SMALL
        );
        assert_eq!(olen, 5);
        assert_eq!(output, [0xa5; 4]);

        let mut olen = 7;
        assert_eq!(
            osmo_base64_decode(None, &mut olen, b"TWE="),
            BASE64_BUFFER_TOO_SMALL
        );
        assert_eq!(olen, 2);
    }

    #[test]
    fn invalid_input_does_not_change_olen() {
        let mut output = [0xa5; 2];
        let mut olen = 7;
        assert_eq!(
            osmo_base64_decode(Some(&mut output), &mut olen, b"TW$="),
            BASE64_INVALID_INPUT
        );
        assert_eq!(olen, 7);
        assert_eq!(output, [0xa5; 2]);
    }

    #[test]
    fn incomplete_tails_reserve_capacity_without_emitting_bytes() {
        for (source, required, decoded) in [
            (b"T".as_slice(), 1, b"".as_slice()),
            (b"TQ", 2, b""),
            (b"TWE", 3, b""),
            (b"TWFuTWE", 6, b"Man"),
            (b"TWFu==", 3, b"Man"),
            (b"TWFuTQ==", 4, b"ManM"),
            (b"==", 0, b""),
        ] {
            let mut olen = 99;
            assert_eq!(
                osmo_base64_decode(None, &mut olen, source),
                BASE64_BUFFER_TOO_SMALL
            );
            assert_eq!(olen, required);
            let mut destination = [0xa5; 8];
            assert_eq!(
                osmo_base64_decode(Some(&mut destination), &mut olen, source),
                0
            );
            assert_eq!(olen, decoded.len());
            assert_eq!(&destination[..olen], decoded);
            assert!(destination[olen..].iter().all(|&byte| byte == 0xa5));
        }
    }

    #[test]
    fn line_grammar_and_late_errors_preserve_output_and_olen() {
        for source in [b"TQ== \r\n".as_slice(), b"TQ== \n  ", b"TQ=\n= "] {
            let mut destination = [0xa5; 3];
            let mut olen = 99;
            assert_eq!(
                osmo_base64_decode(Some(&mut destination), &mut olen, source),
                0
            );
            assert_eq!((olen, destination), (1, [b'M', 0xa5, 0xa5]));
        }
        for source in [
            b"TWFu TQ==".as_slice(),
            b"TWFu\r",
            b"TWFu\r \n",
            b"TWFu\t",
            b"TQ==\nA",
        ] {
            let mut destination = [0xa5; 8];
            let mut olen = 99;
            assert_eq!(
                osmo_base64_decode(Some(&mut destination), &mut olen, source),
                BASE64_INVALID_INPUT
            );
            assert_eq!(olen, 99);
            assert_eq!(destination, [0xa5; 8]);
        }
    }
}

pub(crate) mod legacy {
    use super::*;
    /// Encodes `src` into `dst` using the historical C-compatible Base64 buffer contract.
    ///
    /// On a non-empty successful input, writes the encoded bytes, a trailing NUL,
    /// and sets `olen` to the encoded length excluding that NUL. If `dst` is too
    /// short, it writes nothing, sets `olen` to the required capacity including
    /// the NUL, and returns [`crate::compat::base64::BASE64_BUFFER_TOO_SMALL`]. Empty input sets `olen`
    /// to zero and leaves `dst` unchanged.
    pub fn osmo_base64_encode(dst: &mut [u8], olen: &mut usize, src: &[u8]) -> i32 {
        if src.is_empty() {
            *olen = 0;
            return 0;
        }

        let encoded_len = src.len().div_ceil(3) * 4;
        if dst.len() < encoded_len + 1 {
            *olen = encoded_len + 1;
            return BASE64_BUFFER_TOO_SMALL;
        }

        let output_index = encode_bytes(dst, src);
        *olen = output_index;
        dst[output_index] = 0;
        0
    }

    /// Decodes `src` using the historical C-compatible Base64 buffer contract.
    ///
    /// `None` performs a size probe. As in C, invalid input and input containing
    /// only ignorable whitespace return without changing `olen`; callers who need
    /// to observe that distinction should initialize it before calling.
    pub fn osmo_base64_decode(dst: Option<&mut [u8]>, olen: &mut usize, src: &[u8]) -> i32 {
        let Some(validated) = ValidatedBase64::parse(src) else {
            return BASE64_INVALID_INPUT;
        };
        if validated.characters == 0 {
            return 0;
        }
        let required = validated.required_capacity();
        let dst = match dst {
            Some(dst) if dst.len() >= required => dst,
            _ => {
                *olen = required;
                return BASE64_BUFFER_TOO_SMALL;
            }
        };

        let mut output_index = 0usize;
        for quartet in validated.quartets() {
            let end = output_index + quartet.len;
            dst[output_index..end].copy_from_slice(&quartet.bytes[..quartet.len]);
            output_index = end;
        }

        *olen = output_index;
        0
    }
}

fn encode_bytes(dst: &mut [u8], src: &[u8]) -> usize {
    let mut input_index = 0;
    let mut output_index = 0;
    while input_index + 3 <= src.len() {
        let first = src[input_index];
        let second = src[input_index + 1];
        let third = src[input_index + 2];
        dst[output_index] = ENCODE_MAP[(first >> 2) as usize];
        dst[output_index + 1] = ENCODE_MAP[(((first & 3) << 4) | (second >> 4)) as usize];
        dst[output_index + 2] = ENCODE_MAP[(((second & 15) << 2) | (third >> 6)) as usize];
        dst[output_index + 3] = ENCODE_MAP[(third & 63) as usize];
        input_index += 3;
        output_index += 4;
    }

    if input_index < src.len() {
        let first = src[input_index];
        let second = src.get(input_index + 1).copied().unwrap_or(0);
        dst[output_index] = ENCODE_MAP[(first >> 2) as usize];
        dst[output_index + 1] = ENCODE_MAP[(((first & 3) << 4) | (second >> 4)) as usize];
        dst[output_index + 2] = if input_index + 1 < src.len() {
            ENCODE_MAP[((second & 15) << 2) as usize]
        } else {
            b'='
        };
        dst[output_index + 3] = b'=';
        output_index += 4;
    }

    output_index
}

/// An encoding or decoding operation could not complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Base64Error {
    BufferTooSmall { required: usize },
    InvalidInput,
    ArithmeticOverflow,
}
impl std::fmt::Display for Base64Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Base64Error {}

/// Encode bytes without a trailing NUL, preserving the unused destination suffix.
/// Capacity errors leave the destination unchanged.
pub fn encode_into(dst: &mut [u8], src: &[u8]) -> Result<usize, Base64Error> {
    let required = src
        .len()
        .div_ceil(3)
        .checked_mul(4)
        .ok_or(Base64Error::ArithmeticOverflow)?;
    if dst.len() < required {
        return Err(Base64Error::BufferTooSmall { required });
    }
    Ok(encode_bytes(dst, src))
}
/// Decode using the recorded whitespace and incomplete-quartet rules.
/// Empty/whitespace-only input reports zero bytes. Invalid input and capacity
/// errors leave the destination unchanged; unused suffix bytes are preserved.
pub fn decode_into(dst: &mut [u8], src: &[u8]) -> Result<usize, Base64Error> {
    let mut len = 0;
    match legacy::osmo_base64_decode(Some(dst), &mut len, src) {
        0 => Ok(len),
        BASE64_BUFFER_TOO_SMALL => Err(Base64Error::BufferTooSmall { required: len }),
        _ => Err(Base64Error::InvalidInput),
    }
}
/// Encode into an owned ASCII string without a C terminator.
pub fn encode(src: &[u8]) -> Result<String, Base64Error> {
    let len = src
        .len()
        .div_ceil(3)
        .checked_mul(4)
        .ok_or(Base64Error::ArithmeticOverflow)?;
    let mut output = vec![0; len];
    encode_into(&mut output, src)?;
    Ok(String::from_utf8(output).expect("Base64 alphabet is ASCII"))
}
/// Decode into owned bytes with the same input policy as [`decode_into`].
pub fn decode(src: &[u8]) -> Result<Vec<u8>, Base64Error> {
    let mut required = 0;
    if legacy::osmo_base64_decode(None, &mut required, src) == BASE64_INVALID_INPUT {
        return Err(Base64Error::InvalidInput);
    }
    let mut output = vec![0; required];
    let written = decode_into(&mut output, src)?;
    output.truncate(written);
    Ok(output)
}
