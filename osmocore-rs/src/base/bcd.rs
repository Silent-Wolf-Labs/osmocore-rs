// SPDX-License-Identifier: GPL-3.0-or-later

//! BCD digit and checked buffer conversions.
//!
//! Nibble offsets use BCD order: offset zero is the low nibble of the first
//! byte, offset one is its high nibble. Input digits are ASCII bytes; the first
//! NUL ends the string, or the slice end does if there is no NUL.

/// Errors from a BCD buffer conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BcdError {
    /// C's `-EINVAL`: an encoded nibble is disallowed or a digit is invalid.
    InvalidDigit,
    /// C's `-ENOMEM`: the destination has no usable space or is too small.
    NoSpace,
    /// A Rust slice does not contain the requested nibble range.
    OutOfRange,
}

/// Converts one BCD nibble to an ASCII byte, using uppercase A through F.
///
/// C documents a single BCD digit as input; values above 15 have no BCD
/// meaning and return `None` here.
pub fn nibble_to_char(nibble: u8) -> Option<u8> {
    match nibble {
        0..=9 => Some(b'0' + nibble),
        10..=15 => Some(b'A' + nibble - 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::legacy::*;
    use super::*;

    #[test]
    fn nibble_helpers_cover_their_domains() {
        for nibble in 0..=15 {
            let character = nibble_to_char(nibble).unwrap();
            assert_eq!(osmo_char2bcd(character), nibble);
        }
        assert_eq!(nibble_to_char(16), None);
        assert_eq!(osmo_char2bcd(b'!'), 0);
    }

    #[test]
    fn decode_reports_full_length_but_checks_only_written_nibbles() {
        let mut dst = [0xa5; 2];
        assert_eq!(osmo_bcd2str(&mut dst, &[0x21, 0xfa], 0, 4, false), Ok(4));
        assert_eq!(dst, [b'1', 0]);
        let mut dst = [0xa5; 4];
        assert_eq!(
            osmo_bcd2str(&mut dst, &[0x21, 0xfa], 0, 4, false),
            Err(BcdError::InvalidDigit)
        );
        assert_eq!(dst, [b'1', b'2', b'A', 0]);
    }

    #[test]
    fn encode_preserves_prefix_and_prior_writes_on_error() {
        let mut dst = [0xa5, 0x5a, 0xc3];
        assert_eq!(
            osmo_str2bcd(&mut dst, b"1x\0", 1, Some(4), false),
            Err(BcdError::InvalidDigit)
        );
        assert_eq!(dst, [0x15, 0x5a, 0xc3]);
        assert_eq!(osmo_str2bcd(&mut dst, b"2\0", 1, None, false), Ok(1));
        assert_eq!(dst, [0x25, 0x5a, 0xc3]);
    }

    #[test]
    fn invalid_ranges_and_capacity_do_not_write() {
        let mut dst = [0xa5];
        assert_eq!(
            osmo_str2bcd(&mut dst, b"12", 0, Some(3), false),
            Err(BcdError::NoSpace)
        );
        assert_eq!(dst, [0xa5]);
        assert_eq!(
            osmo_bcd2str(&mut dst, &[0x12], 0, 3, false),
            Err(BcdError::OutOfRange)
        );
        assert_eq!(dst, [0xa5]);
    }
}

pub(crate) mod legacy {
    use super::*;
    /// Converts an ASCII digit to BCD. As in C, invalid bytes return zero.
    pub fn osmo_char2bcd(character: u8) -> u8 {
        match character {
            b'0'..=b'9' => character - b'0',
            b'A'..=b'F' => character - b'A' + 10,
            b'a'..=b'f' => character - b'a' + 10,
            _ => 0,
        }
    }

    /// Writes BCD nibbles as an ASCII, NUL-terminated string.
    ///
    /// A nonempty destination always receives a NUL. On success, the returned
    /// length is the full requested range, even if the output was truncated. A
    /// disallowed nibble among those actually written returns `InvalidDigit`, but
    /// its ASCII character and the trailing NUL are still written. Nibbles beyond
    /// the writable prefix are not examined. An empty destination returns
    /// `NoSpace`; an offset beyond `bcd` returns `OutOfRange` without writing.
    pub fn osmo_bcd2str(
        dst: &mut [u8],
        bcd: &[u8],
        start_nibble: usize,
        end_nibble: usize,
        allow_hex: bool,
    ) -> Result<usize, BcdError> {
        if dst.is_empty() {
            return Err(BcdError::NoSpace);
        }
        let available_nibbles = bcd.len().saturating_mul(2);
        if start_nibble > available_nibbles || end_nibble > available_nibbles {
            return Err(BcdError::OutOfRange);
        }

        let requested = end_nibble.saturating_sub(start_nibble);
        let written = requested.min(dst.len() - 1);
        let mut invalid_digit = false;
        for (output_index, nibble_index) in (start_nibble..start_nibble + written).enumerate() {
            let byte = bcd[nibble_index / 2];
            let nibble = if nibble_index % 2 == 0 {
                byte & 0x0f
            } else {
                byte >> 4
            };
            invalid_digit |= !allow_hex && nibble > 9;
            dst[output_index] = nibble_to_char(nibble).expect("a masked nibble is at most 15");
        }
        dst[written] = 0;
        if invalid_digit {
            Err(BcdError::InvalidDigit)
        } else {
            Ok(requested)
        }
    }

    /// Writes ASCII digits to BCD, preserving all untouched destination nibbles.
    ///
    /// `None` for `end_nibble` consumes the C-string input and pads the final
    /// started byte with an F nibble. An explicit end may also fill remaining
    /// nibbles with F after the input ends. Returns C's used-byte count, which is
    /// `end_nibble / 2` for an explicit odd end. On an invalid digit, earlier
    /// writes remain in place. Capacity and range errors leave `dst` unchanged.
    pub fn osmo_str2bcd(
        dst: &mut [u8],
        digits: &[u8],
        start_nibble: usize,
        end_nibble: Option<usize>,
        allow_hex: bool,
    ) -> Result<usize, BcdError> {
        if dst.is_empty() {
            return Err(BcdError::NoSpace);
        }
        let digit_count = digits
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(digits.len());
        let end_nibble = match end_nibble {
            Some(end) => end,
            None => {
                let end = start_nibble
                    .checked_add(digit_count)
                    .ok_or(BcdError::OutOfRange)?;
                end.checked_add(end % 2).ok_or(BcdError::OutOfRange)?
            }
        };
        let available_nibbles = dst.len().saturating_mul(2);
        if start_nibble > available_nibbles {
            return Err(BcdError::OutOfRange);
        }
        if end_nibble > available_nibbles {
            return Err(BcdError::NoSpace);
        }

        let mut digit_index = 0;
        for nibble_index in start_nibble..end_nibble {
            let nibble = if digit_index == digit_count {
                0x0f
            } else {
                let character = digits[digit_index];
                digit_index += 1;
                match character {
                    b'0'..=b'9' => character - b'0',
                    b'A'..=b'F' if allow_hex => character - b'A' + 10,
                    b'a'..=b'f' if allow_hex => character - b'a' + 10,
                    _ => return Err(BcdError::InvalidDigit),
                }
            };
            let byte = &mut dst[nibble_index / 2];
            if nibble_index % 2 == 0 {
                *byte = (*byte & 0xf0) | nibble;
            } else {
                *byte = (*byte & 0x0f) | (nibble << 4);
            }
        }
        Ok(end_nibble / 2)
    }
}

/// Convert an ASCII hexadecimal digit to its nibble; invalid input is an error.
pub fn char_to_nibble(character: u8) -> Result<u8, BcdError> {
    if !character.is_ascii_hexdigit() {
        return Err(BcdError::InvalidDigit);
    }
    Ok(legacy::osmo_char2bcd(character))
}
/// Decode a nibble range to an owned string. Offsets count low nibble first.
/// Unlike the C buffer adapter this convenience does not truncate or return a NUL.
pub fn decode(
    bcd: &[u8],
    start_nibble: usize,
    end_nibble: usize,
    allow_hex: bool,
) -> Result<String, BcdError> {
    let available = bcd.len().saturating_mul(2);
    if start_nibble > available || end_nibble > available {
        return Err(BcdError::OutOfRange);
    }
    let len = end_nibble.saturating_sub(start_nibble);
    let mut output = vec![0; len.checked_add(1).ok_or(BcdError::OutOfRange)?];
    legacy::osmo_bcd2str(&mut output, bcd, start_nibble, end_nibble, allow_hex)?;
    output.truncate(len);
    Ok(String::from_utf8(output).expect("BCD digits are ASCII"))
}

/// Encode ASCII digits into owned low-nibble-first BCD with final F padding.
/// Embedded NUL is rejected rather than truncating a Rust string.
pub fn encode(digits: &str, allow_hex: bool) -> Result<Vec<u8>, BcdError> {
    if digits.contains('\0') {
        return Err(BcdError::InvalidDigit);
    }
    if digits.is_empty() {
        return Ok(Vec::new());
    }
    let mut output = vec![0; digits.len().div_ceil(2)];
    legacy::osmo_str2bcd(&mut output, digits.as_bytes(), 0, None, allow_hex)?;
    Ok(output)
}
