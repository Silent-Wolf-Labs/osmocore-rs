// SPDX-License-Identifier: GPL-3.0-or-later

//! MSB-first packed and unpacked bit conversions.
//!
//! Packed bits are ordered most-significant bit first. Unpacked input bytes
//! follow C semantics: each byte is shifted into its bit position, so values
//! outside 0 and 1 are not rejected. The extended APIs additionally support
//! offsets and LSB-first ordering; their packer treats nonzero bytes as one.

/// A bit conversion could not be performed with the provided slices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitError {
    /// The source slice cannot hold all bytes read by the conversion.
    InputTooShort,
    /// The destination slice cannot hold all bytes written by the conversion.
    OutputTooShort,
    /// An offset plus the requested bit count cannot be represented by `usize`.
    ArithmeticOverflow,
}

fn packed_len(num_bits: usize) -> usize {
    num_bits / 8 + usize::from(!num_bits.is_multiple_of(8))
}

fn extended_ends(
    in_ofs: usize,
    out_ofs: usize,
    num_bits: usize,
) -> Result<(usize, usize), BitError> {
    let input_end = in_ofs
        .checked_add(num_bits)
        .ok_or(BitError::ArithmeticOverflow)?;
    let output_end = out_ofs
        .checked_add(num_bits)
        .ok_or(BitError::ArithmeticOverflow)?;
    Ok((input_end, output_end))
}

/// Packs individual bits into a selected destination bit range.
///
/// `in_ofs` indexes unpacked source bytes; `out_ofs` indexes packed destination
/// bits. Any nonzero source byte sets a bit, while zero clears it. `lsb_mode`
/// selects least-significant-bit-first ordering within each destination byte;
/// otherwise ordering is most-significant bit first. All bits outside the
/// requested range are preserved, including neighbors in partial bytes.
///
/// Returns the ending packed-byte position, `ceil((out_ofs + num_bits) / 8)`,
/// rather than the number of bytes newly written. Zero-bit calls access neither
/// slice, accepting empty slices and any offsets. They return `ceil(out_ofs / 8)`
/// for positive output offsets. At output offset zero, they preserve C's
/// unsigned-underflow result of 536870912 on the reference's 32-bit unsigned-int
/// ABI. This compatibility value is independent of Rust's `usize` width.
///
/// For positive counts, arithmetic overflow is checked first, then source
/// capacity, then destination capacity. Errors leave the destination unchanged.
/// Other large offsets/counts use safe `usize` arithmetic rather than C wrapping.
pub fn pack_into_offset(
    dst: &mut [u8],
    out_ofs: usize,
    src: &[u8],
    in_ofs: usize,
    num_bits: usize,
    lsb_mode: bool,
) -> Result<usize, BitError> {
    if num_bits == 0 {
        return Ok(if out_ofs == 0 {
            (u32::MAX >> 3) as usize + 1
        } else {
            packed_len(out_ofs)
        });
    }
    let (input_end, output_end) = extended_ends(in_ofs, out_ofs, num_bits)?;
    if src.len() < input_end {
        return Err(BitError::InputTooShort);
    }
    let output_len = packed_len(output_end);
    if dst.len() < output_len {
        return Err(BitError::OutputTooShort);
    }
    for (i, &bit) in src[in_ofs..input_end].iter().enumerate() {
        let position = out_ofs + i;
        let shift = if lsb_mode {
            position % 8
        } else {
            7 - position % 8
        };
        let mask = 1 << shift;
        if bit != 0 {
            dst[position / 8] |= mask;
        } else {
            dst[position / 8] &= !mask;
        }
    }
    Ok(output_len)
}

/// Unpacks a selected source bit range into byte-sized bits.
///
/// `in_ofs` indexes packed source bits; `out_ofs` indexes destination bytes.
/// `lsb_mode` selects least-significant-bit-first ordering within each source
/// byte; otherwise ordering is most-significant bit first. Writes zero or one
/// to the selected destination range, preserving all other destination bytes.
///
/// Returns the ending output position `out_ofs + num_bits`, rather than the
/// count newly written. Zero-bit calls return `out_ofs` and access neither
/// slice, even with empty slices or offsets beyond their capacities. This
/// differs from basic [`unpack_into`]'s zero-bit read/write behavior.
///
/// For positive counts, arithmetic overflow is checked first, then source
/// capacity, then destination capacity. Errors leave the destination unchanged.
pub fn unpack_into_offset(
    dst: &mut [u8],
    out_ofs: usize,
    src: &[u8],
    in_ofs: usize,
    num_bits: usize,
    lsb_mode: bool,
) -> Result<usize, BitError> {
    if num_bits == 0 {
        return Ok(out_ofs);
    }
    let (input_end, output_end) = extended_ends(in_ofs, out_ofs, num_bits)?;
    if src.len() < packed_len(input_end) {
        return Err(BitError::InputTooShort);
    }
    if dst.len() < output_end {
        return Err(BitError::OutputTooShort);
    }
    for (i, output) in dst[out_ofs..output_end].iter_mut().enumerate() {
        let position = in_ofs + i;
        let shift = if lsb_mode {
            position % 8
        } else {
            7 - position % 8
        };
        *output = (src[position / 8] >> shift) & 1;
    }
    Ok(output_end)
}

/// Packs unpacked bits into bytes, most-significant bit first.
///
/// Reads `num_bits` source bytes and writes `ceil(num_bits / 8)` output bytes.
/// Any unused low bits in the final output byte are cleared. Returns the
/// number of packed bytes. A zero-bit call returns zero without accessing
/// either slice, including empty slices. Capacity errors leave the destination
/// unchanged. Counts use `usize`; packed-size calculation cannot overflow.
pub fn pack_into(dst: &mut [u8], src: &[u8], num_bits: usize) -> Result<usize, BitError> {
    let output_len = packed_len(num_bits);
    if src.len() < num_bits {
        return Err(BitError::InputTooShort);
    }
    if dst.len() < output_len {
        return Err(BitError::OutputTooShort);
    }

    for output in &mut dst[..output_len] {
        *output = 0;
    }
    for (i, &bit) in src[..num_bits].iter().enumerate() {
        dst[i / 8] |= bit.wrapping_shl((7 - (i % 8)) as u32);
    }
    Ok(output_len)
}

/// Unpacks bytes into individual bits, most-significant bit first.
///
/// Reads `ceil(num_bits / 8)` source bytes and writes `num_bits` output bytes,
/// each containing zero or one. Returns the number of unpacked bytes. To match
/// the C function, a zero-bit call with nonempty slices still reads the first
/// source byte, writes its high bit to `dst[0]`, and returns one. Since Rust
/// slices guarantee accessible storage, a zero-bit call with an empty source
/// or destination returns the corresponding capacity error instead.
/// Capacity errors leave the destination unchanged.
pub fn unpack_into(dst: &mut [u8], src: &[u8], num_bits: usize) -> Result<usize, BitError> {
    if num_bits == 0 {
        if dst.is_empty() {
            return Err(BitError::OutputTooShort);
        }
        let Some(&first) = src.first() else {
            return Err(BitError::InputTooShort);
        };
        dst[0] = first >> 7;
        return Ok(1);
    }

    let input_len = packed_len(num_bits);
    if src.len() < input_len {
        return Err(BitError::InputTooShort);
    }
    if dst.len() < num_bits {
        return Err(BitError::OutputTooShort);
    }

    for (i, output) in dst[..num_bits].iter_mut().enumerate() {
        *output = (src[i / 8] >> (7 - (i % 8))) & 1;
    }
    Ok(num_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    type ExtendedConversion =
        fn(&mut [u8], usize, &[u8], usize, usize, bool) -> Result<usize, BitError>;

    #[test]
    fn extended_zero_bit_calls_accept_empty_slices_and_large_offsets() {
        for lsb in [false, true] {
            assert_eq!(
                unpack_into_offset(&mut [], 0, &[], usize::MAX, 0, lsb),
                Ok(0)
            );
            assert_eq!(
                pack_into_offset(&mut [], 0, &[], usize::MAX, 0, lsb),
                Ok(536870912)
            );
            for out in [1, 7, 8, 9, 16, usize::MAX] {
                assert_eq!(
                    pack_into_offset(&mut [], out, &[], usize::MAX, 0, lsb),
                    Ok(packed_len(out))
                );
                assert_eq!(
                    unpack_into_offset(&mut [], out, &[], usize::MAX, 0, lsb),
                    Ok(out)
                );
            }
            let mut dst = [0xa5, 0x5a];
            assert_eq!(unpack_into_offset(&mut dst, 0, &[0xff], 0, 0, lsb), Ok(0));
            assert_eq!(
                pack_into_offset(&mut dst, 0, &[0xff], 0, 0, lsb),
                Ok(536870912)
            );
            assert_eq!(dst, [0xa5, 0x5a]);
        }
    }

    #[test]
    fn extended_errors_are_atomic_and_have_defined_precedence() {
        for convert in [pack_into_offset as ExtendedConversion, unpack_into_offset] {
            for lsb in [false, true] {
                let mut dst = [0xa5, 0x5a];
                for (out, input, count, source, error) in [
                    (0, usize::MAX, 1, &[][..], BitError::ArithmeticOverflow),
                    (usize::MAX, 0, 1, &[][..], BitError::ArithmeticOverflow),
                    (0, 1, usize::MAX, &[][..], BitError::ArithmeticOverflow),
                    (0, 0, 1, &[][..], BitError::InputTooShort),
                    (usize::MAX - 1, 0, 1, &[][..], BitError::InputTooShort),
                    (0, 8, 1, &[0xff][..], BitError::InputTooShort),
                    (16, 0, 1, &[1][..], BitError::OutputTooShort),
                ] {
                    assert_eq!(
                        convert(&mut dst, out, source, input, count, lsb),
                        Err(error)
                    );
                    assert_eq!(dst, [0xa5, 0x5a]);
                }
                assert_eq!(
                    convert(&mut [], 0, &[1], 0, 1, lsb),
                    Err(BitError::OutputTooShort)
                );
            }
        }
    }

    #[test]
    fn packing_is_msb_first_and_clears_partial_byte_padding() {
        let mut dst = [0xa5, 0x5a];
        assert_eq!(pack_into(&mut dst, &[1, 0, 1, 1, 0], 5), Ok(1));
        assert_eq!(dst, [0xb0, 0x5a]);
    }

    #[test]
    fn unpacking_is_msb_first_and_preserves_unused_destination() {
        let mut dst = [0xa5; 10];
        assert_eq!(unpack_into(&mut dst, &[0xb1], 5), Ok(5));
        assert_eq!(dst, [1, 0, 1, 1, 0, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]);
    }

    #[test]
    fn zero_bit_calls_match_c_when_backing_slices_are_present() {
        let mut packed = [0xa5];
        assert_eq!(pack_into(&mut packed, &[1], 0), Ok(0));
        assert_eq!(packed, [0xa5]);

        let mut unpacked = [0xa5];
        assert_eq!(unpack_into(&mut unpacked, &[0x80], 0), Ok(1));
        assert_eq!(unpacked, [1]);
    }

    #[test]
    fn zero_bit_packing_accepts_empty_slices_without_writing() {
        assert_eq!(pack_into(&mut [], &[], 0), Ok(0));
        let mut dst = [0xa5];
        assert_eq!(pack_into(&mut dst, &[], 0), Ok(0));
        assert_eq!(dst, [0xa5]);
        assert_eq!(unpack_into(&mut [], &[], 0), Err(BitError::OutputTooShort));
    }

    #[test]
    fn maximum_bit_count_fails_safely_without_overflow_or_writes() {
        let mut dst = [0xa5];
        assert_eq!(
            pack_into(&mut dst, &[1], usize::MAX),
            Err(BitError::InputTooShort)
        );
        assert_eq!(dst, [0xa5]);
        assert_eq!(
            unpack_into(&mut dst, &[0x80], usize::MAX),
            Err(BitError::InputTooShort)
        );
        assert_eq!(dst, [0xa5]);
    }

    #[test]
    fn packing_shifts_raw_input_bytes_instead_of_normalizing_them() {
        // C shifts each entire input byte; 2 in position one sets bit seven.
        let mut dst = [0xa5];
        assert_eq!(pack_into(&mut dst, &[0, 2], 2), Ok(1));
        assert_eq!(dst, [0x80]);
    }

    #[test]
    fn insufficient_slices_return_errors_without_writing() {
        let mut dst = [0xa5];
        assert_eq!(pack_into(&mut dst, &[1], 9), Err(BitError::InputTooShort));
        assert_eq!(dst, [0xa5]);
        assert_eq!(
            pack_into(&mut dst, &[1; 9], 9),
            Err(BitError::OutputTooShort)
        );
        assert_eq!(dst, [0xa5]);
        assert_eq!(
            unpack_into(&mut dst, &[0; 2], 9),
            Err(BitError::OutputTooShort)
        );
        assert_eq!(dst, [0xa5]);
        assert_eq!(unpack_into(&mut dst, &[], 1), Err(BitError::InputTooShort));
        assert_eq!(dst, [0xa5]);
        assert_eq!(unpack_into(&mut [], &[0], 0), Err(BitError::OutputTooShort));
        assert_eq!(unpack_into(&mut dst, &[], 0), Err(BitError::InputTooShort));
        assert_eq!(dst, [0xa5]);
    }
}
