// SPDX-License-Identifier: GPL-3.0-or-later

//! Buffer-oriented hexadecimal parsing compatible with `parse_into()`.
//!
//! The input is bytes so invalid ASCII and embedded NUL can be represented.
//! The first NUL ends input, as it does for a C string. If there is no NUL,
//! the end of the slice ends input. The destination slice supplies the C
//! buffer and its capacity; there is no separate length to disagree with it.

/// The C parser returns `-1` for invalid input, an odd digit count, or a full
/// destination. These errors have the same result and mutation contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HexParseError;

/// Parses ASCII hexadecimal digits into `dst`.
///
/// Clears every byte of `dst` before reading input. Spaces, tabs, line feeds,
/// and carriage returns are skipped, including after the destination fills.
/// On success, returns the number of parsed bytes. On error, returns
/// [`HexParseError`] and leaves any completed bytes or high nibble already
/// written in `dst`; the rest remains zero. A non-whitespace character after
/// the destination fills is an error, even if that character is invalid.
///
/// Unlike C pointers and `unsigned int` lengths, Rust slices always describe
/// accessible storage. Counts use `usize`, so no signed C return conversion or
/// `max_len << 1` overflow is needed for very large slices.
pub fn parse_into(src: &[u8], dst: &mut [u8]) -> Result<usize, HexParseError> {
    dst.fill(0);
    let mut nibble_count = 0usize;

    for &byte in src {
        if byte == 0 {
            break;
        }
        if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
            continue;
        }
        if nibble_count / 2 >= dst.len() {
            return Err(HexParseError);
        }

        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return Err(HexParseError),
        };

        let output = &mut dst[nibble_count / 2];
        *output |= if nibble_count.is_multiple_of(2) {
            nibble << 4
        } else {
            nibble
        };
        nibble_count += 1;
    }

    if !nibble_count.is_multiple_of(2) {
        return Err(HexParseError);
    }
    Ok(nibble_count / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_case_and_whitespace_between_nibbles() {
        let mut dst = [0xa5; 4];
        assert_eq!(parse_into(b"a B\tc\nD\r", &mut dst), Ok(2));
        assert_eq!(dst, [0xab, 0xcd, 0, 0]);
    }

    #[test]
    fn failure_preserves_partial_nibble_after_clearing() {
        let mut dst = [0xa5; 3];
        assert_eq!(parse_into(b"12 a?", &mut dst), Err(HexParseError));
        assert_eq!(dst, [0x12, 0xa0, 0]);
    }

    #[test]
    fn nul_ends_input_before_invalid_bytes() {
        let mut dst = [0xa5; 2];
        assert_eq!(parse_into(b"0f\0zz", &mut dst), Ok(1));
        assert_eq!(dst, [0x0f, 0]);
    }
}
