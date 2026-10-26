// SPDX-License-Identifier: GPL-3.0-or-later

//! MSB-first bit vectors with a cursor and GSM padding-relative bit values.
//!
//! Operations preserve the historical C contract's partial writes and cursor progress on
//! failure. The backing slice supplies the C `data` and `data_len` fields.

/// A binary bit, or a bit relative to the repeating GSM `0x2b` padding byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum BitValue {
    Zero = 0,
    One = 1,
    /// The bit agrees with the padding pattern at its position.
    L = 2,
    /// The bit differs from the padding pattern at its position.
    H = 3,
}

/// A bit-vector operation cannot be represented safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitVecError {
    /// A bit position, cursor, or supplied buffer exceeds available storage.
    OutOfRange,
    /// A bit capacity or cursor increment overflows `usize`.
    ArithmeticOverflow,
    /// A write width or nonzero read bit requires an unsupported shift.
    TooManyBits,
}

/// A borrowed byte buffer and the cursor of its next sequential operation.
///
/// Creating a vector preserves the bytes and starts the cursor at zero.
/// `cur_bit` can be positioned independently, including beyond capacity;
/// accesses then fail without changing the bytes or cursor. Positional
/// accesses use `usize`. Helpers that reproduce the reference ABI's
/// wrapping unsigned calculations document their 32-bit behavior explicitly.
#[derive(Debug)]
pub struct BitVec<'a> {
    data: &'a mut [u8],
    pub cur_bit: usize,
}

impl<'a> BitVec<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self { data, cur_bit: 0 }
    }

    /// Returns the complete backing slice, including unused bytes.
    pub fn data(&self) -> &[u8] {
        self.data
    }
}

// C's byte operations validate the span including a byte for an unaligned
// cursor even when count is zero. Validate before touching either buffer.
fn byte_span(bv: &BitVec<'_>, count: usize) -> Result<(usize, usize, usize), BitVecError> {
    let start = bv.cur_bit / 8;
    let offset = bv.cur_bit % 8;
    let end = start
        .checked_add(count)
        .and_then(|n| n.checked_add(usize::from(offset != 0)))
        .ok_or(BitVecError::ArithmeticOverflow)?;
    if end > bv.data.len() {
        return Err(BitVecError::OutOfRange);
    }
    let next = count
        .checked_mul(8)
        .and_then(|n| bv.cur_bit.checked_add(n))
        .ok_or(BitVecError::ArithmeticOverflow)?;
    Ok((start, offset, next))
}

fn leading_bits(byte: u8, one: bool) -> u32 {
    if one {
        byte.leading_ones()
    } else {
        byte.leading_zeros()
    }
}

/// Returns the C bit-value character. Invalid C enum values cannot be
/// represented by `BitValue`, avoiding C's panic on such input.
pub const fn bit_value_to_char(bit: BitValue) -> char {
    match bit {
        BitValue::Zero => '0',
        BitValue::One => '1',
        BitValue::L => 'L',
        BitValue::H => 'H',
    }
}

#[cfg(test)]
mod tests {
    use super::legacy::*;
    use super::*;

    #[test]
    fn short_adapter_buffers_and_invalid_format_cursors_fail_atomically() {
        let mut data = [0xa5, 0x5a];
        let mut bv = BitVec::new(&mut data);
        bv.cur_bit = 9;
        let mut output = [0x7e];
        assert_eq!(bitvec_pack(&bv, &mut output), Err(BitVecError::OutOfRange));
        assert_eq!(bitvec_unpack(&mut bv, &[0]), Err(BitVecError::OutOfRange));
        assert_eq!(
            bitvec_to_string_r(&bv, &mut output),
            Err(BitVecError::OutOfRange)
        );
        // C's documented cur_bit+1 capacity omits byte separator spaces.
        let mut output = [0x7e; 10];
        assert_eq!(
            bitvec_to_string_r(&bv, &mut output),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(output, [0x7e; 10]);
        bv.cur_bit = usize::MAX;
        let mut output = [0x7e; 32];
        assert_eq!(
            bitvec_to_string_r(&bv, &mut output),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(output, [0x7e; 32]);
        assert_eq!(bv.cur_bit, usize::MAX);
        assert_eq!(bv.data(), [0xa5, 0x5a]);
    }

    #[test]
    fn empty_adapter_slices_and_rust_terminated_hex_are_safe() {
        let mut bv = BitVec::new(&mut []);
        bv.cur_bit = usize::MAX;
        assert_eq!(bitvec_pack(&bv, &mut []), Ok(0));
        assert_eq!(bitvec_unpack(&mut bv, &[]), Ok(0));
        assert_eq!(bv.cur_bit, usize::MAX);
        assert_eq!(bitvec_unhex(&mut bv, &[]), 0);
        assert_eq!(bv.cur_bit, 0);
        assert_eq!(
            bitvec_to_string_r(&bv, &mut []),
            Err(BitVecError::OutOfRange)
        );
        let mut output = [0x7e; 2];
        assert_eq!(bitvec_to_string_r(&bv, &mut output), Ok(()));
        assert_eq!(output, [0, 0x7e]);

        let mut data = [0xa5; 2];
        let mut bv = BitVec::new(&mut data);
        assert_eq!(bitvec_unhex(&mut bv, b"a1b2"), 0);
        assert_eq!(bv.cur_bit, 16);
        assert_eq!(bv.data(), [0xa1, 0xb2]);
    }

    #[test]
    fn wrapped_field_spans_and_undefined_shifts_do_not_access_storage() {
        let mut data = [0x80];
        let mut bv = BitVec::new(&mut data);
        bv.cur_bit = 3;
        let mut index = u32::MAX;
        assert_eq!(
            bitvec_read_field(&mut bv, &mut index, 2),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!((bv.cur_bit, index), (3, u32::MAX));
        assert_eq!(bv.data(), [0x80]);

        let mut data = [0u8; 9];
        data[0] = 0x80;
        let mut bv = BitVec::new(&mut data);
        bv.cur_bit = 7;
        let mut index = 0;
        assert_eq!(
            bitvec_read_field(&mut bv, &mut index, 65),
            Err(BitVecError::TooManyBits)
        );
        assert_eq!((bv.cur_bit, index), (0, 0));
        assert_eq!(bv.data()[0], 0x80);
    }

    #[test]
    fn unsafe_shift_and_run_spans_return_errors() {
        let mut data = [0xa5];
        let mut bv = BitVec::new(&mut data);
        bv.cur_bit = 8;
        assert_eq!(bitvec_shiftl(&mut bv, 1), Err(BitVecError::OutOfRange));
        assert_eq!(bv.cur_bit, 8);
        assert_eq!(bv.data(), [0xa5]);

        let mut data = [0xa5, 0x5a];
        let mut bv = BitVec::new(&mut data);
        bv.cur_bit = 32;
        assert_eq!(bitvec_shiftl(&mut bv, 24), Err(BitVecError::OutOfRange));
        assert_eq!(bv.cur_bit, 32);
        assert_eq!(bv.data(), [0xa5, 0x5a]);

        let mut bv = BitVec::new(&mut []);
        bv.cur_bit = 1;
        assert_eq!(bitvec_rl(&bv, false), Err(BitVecError::OutOfRange));
        assert_eq!(
            bitvec_rl_curbit(&mut bv, false, 8),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(bv.cur_bit, 1);
    }

    #[test]
    fn run_outcomes_preserve_cursor_including_partial_storage_failures() {
        for (bytes, start, one, limit, result, cursor) in [
            (vec![], 1, false, 8, Err(BitVecError::OutOfRange), 1),
            (vec![], 1, true, 8, Ok(0), 0),
            (vec![0], 1, false, 16, Err(BitVecError::OutOfRange), 8),
            (vec![0], 1, false, 4, Ok(3), 3),
            (vec![0, 0xff], 1, false, 9, Ok(7), 8),
            (vec![0, 0], 1, false, 16, Ok(15), 16),
            (vec![0, 0, 0x10], 8, false, 17, Ok(11), 19),
            (vec![], 16, false, 8, Ok(u32::MAX - 7), 8),
            (vec![], 1, false, 1, Ok(0), 0),
        ] {
            let mut storage = bytes.clone();
            let mut bv = BitVec::new(&mut storage);
            bv.cur_bit = start;
            assert_eq!(bitvec_rl_curbit(&mut bv, one, limit), result);
            assert_eq!(bv.cur_bit, cursor);
            assert_eq!(bv.data(), bytes);
        }
    }

    #[test]
    fn signed_numeric_results_remain_successful() {
        let mut data = [0xff, 0xff, 0xff, 0xea];
        let mut bv = BitVec::new(&mut data);
        assert_eq!(bitvec_get_uint(&mut bv, 32), Ok(-22));
        assert_eq!(bv.cur_bit, 32);
        assert_eq!(bitvec_get_uint(&mut bv, 1), Err(BitVecError::OutOfRange));
        assert_eq!(bv.cur_bit, 32);
    }

    #[test]
    fn undefined_numeric_shifts_and_short_scalar_storage_fail_safely() {
        let mut data = [0x40];
        let mut bv = BitVec::new(&mut data);
        // First bit is zero; the next one would require a shift by 32 in C.
        assert_eq!(bitvec_get_uint(&mut bv, 34), Err(BitVecError::TooManyBits));
        assert_eq!(bv.cur_bit, 1);
        bv.cur_bit = 16;
        assert_eq!(bitvec_get_int16_msb(&bv, 9), Err(BitVecError::OutOfRange));
        assert_eq!(bv.cur_bit, 16);
        assert_eq!(
            bitvec_set_u64(&mut bv, u64::MAX, 65, true),
            Err(BitVecError::TooManyBits)
        );
        assert_eq!(bv.data(), [0x40]);
        assert_eq!(bv.cur_bit, 16);
    }

    #[test]
    fn rust_only_positions_and_byte_arithmetic_fail_without_writes() {
        let mut data = [0xa5];
        let mut dst = [0x5a];
        let mut bv = BitVec::new(&mut data);
        bv.cur_bit = 8;
        assert_eq!(
            byte_span(&bv, usize::MAX),
            Err(BitVecError::ArithmeticOverflow)
        );
        bv.cur_bit = usize::MAX;
        assert_eq!(
            bitvec_get_bytes(&mut bv, &mut dst),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(
            bitvec_set_bytes(&mut bv, &[0]),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(bitvec_get_uint(&mut bv, 0), Ok(0));
        assert_eq!(bv.cur_bit, usize::MAX);
        assert_eq!(bv.data(), [0xa5]);
        assert_eq!(dst, [0x5a]);
        if usize::BITS > 32 {
            bv.cur_bit = (u32::MAX as usize).checked_add(1).unwrap();
            assert_eq!(bitvec_tailroom_bits(&bv), 8);
            assert_eq!(
                bitvec_tailroom_bits_checked(&bv),
                Err(BitVecError::OutOfRange)
            );
            assert_eq!(
                bitvec_fill(&mut bv, 1, BitValue::One),
                Err(BitVecError::ArithmeticOverflow)
            );
        }
    }

    #[test]
    fn extreme_positions_fail_without_mutation_or_cursor_progress() {
        let mut data = [0xa5];
        let mut bv = BitVec::new(&mut data);
        bv.cur_bit = usize::MAX;
        assert_eq!(
            bitvec_set_bit(&mut bv, BitValue::H),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(bitvec_get_bit_high(&mut bv), Err(BitVecError::OutOfRange));
        assert_eq!(
            bitvec_get_bit_pos(&bv, usize::MAX),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(
            bitvec_set_bit_pos(&mut bv, usize::MAX, BitValue::One),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(bitvec_tailroom_bits(&bv), 9);
        assert_eq!(
            bitvec_tailroom_bits_checked(&bv),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(bitvec_used_bytes(&bv), usize::MAX / 8 + 1);
        assert_eq!(bitvec_set_bits(&mut bv, &[]), Ok(()));
        assert_eq!(bv.cur_bit, usize::MAX);
        assert_eq!(bv.data(), [0xa5]);
    }

    #[test]
    fn empty_storage_supports_zero_and_empty_writes() {
        let mut bv = BitVec::new(&mut []);
        assert_eq!(bitvec_tailroom_bits(&bv), 0);
        assert_eq!(bitvec_set_bits(&mut bv, &[]), Ok(()));
        assert_eq!(bitvec_get_bit_high(&mut bv), Err(BitVecError::OutOfRange));
        bv.cur_bit = usize::MAX;
        bitvec_zero(&mut bv);
        assert_eq!(bv.cur_bit, 0);
        assert_eq!(bitvec_used_bytes(&bv), 0);
    }
}

/// Scan outcomes retain the last readable cursor even when storage is missing.
enum RunEnd {
    PrefixStopped { cursor: u32 },
    ByteMismatch { position: u32 },
    Exhausted { position: u32 },
    MissingStorage,
}

struct RunMeasurement {
    prefix: u32,
    byte_start: u32,
    end: RunEnd,
}

impl RunMeasurement {
    fn measure(data: &[u8], start: u32, one: bool, limit: u32) -> Self {
        let mut position = start;
        let mut prefix = 0;
        for _ in 0..(8 - start % 8) % 8 {
            if position >= limit {
                return Self {
                    prefix,
                    byte_start: position,
                    end: RunEnd::PrefixStopped {
                        cursor: position.wrapping_sub(1),
                    },
                };
            }
            // The existing adapter treats unavailable one-bit reads as zero,
            // without advancing the cursor. Preserve that behavior for both runs.
            let value = match data.get(position as usize / 8) {
                Some(&byte) => {
                    let value = byte & (1 << (7 - position % 8)) != 0;
                    position = position.wrapping_add(1);
                    value
                }
                None => false,
            };
            if value != one {
                return Self {
                    prefix,
                    byte_start: position,
                    end: RunEnd::PrefixStopped {
                        cursor: position.wrapping_sub(1),
                    },
                };
            }
            prefix += 1;
        }
        let byte_start = position;
        let first_byte = byte_start / 8;
        let end_byte = limit.div_ceil(8);
        let mut measurement = Self {
            prefix,
            byte_start,
            end: RunEnd::MissingStorage,
        };
        for byte_index in first_byte..end_byte {
            let Some(&byte) = data.get(byte_index as usize) else {
                return measurement;
            };
            if byte != if one { 0xff } else { 0 } {
                measurement.end = RunEnd::ByteMismatch {
                    position: byte_start
                        .wrapping_add((byte_index - first_byte).wrapping_mul(8))
                        .wrapping_add(leading_bits(byte, one)),
                };
                return measurement;
            }
        }
        measurement.end = RunEnd::Exhausted {
            position: byte_start.wrapping_add(end_byte.saturating_sub(first_byte).wrapping_mul(8)),
        };
        measurement
    }

    fn compatibility_result(self, limit: u32) -> (u32, Result<u32, BitVecError>) {
        match self.end {
            RunEnd::PrefixStopped { cursor } => (cursor, Ok(self.prefix)),
            RunEnd::MissingStorage => (self.byte_start, Err(BitVecError::OutOfRange)),
            RunEnd::ByteMismatch { position } => (
                position,
                Ok(position
                    .wrapping_sub(self.byte_start)
                    .wrapping_add(self.prefix)),
            ),
            RunEnd::Exhausted { position } => {
                // The prefix participates twice in the legacy exhausted result.
                let cursor = position.wrapping_add(self.prefix).min(limit);
                (
                    cursor,
                    Ok(cursor
                        .wrapping_sub(self.byte_start)
                        .wrapping_add(self.prefix)),
                )
            }
        }
    }
}

pub(crate) mod legacy {
    use super::*;
    /// Reads a binary bit at an absolute position without moving the cursor.
    pub fn bitvec_get_bit_pos(bv: &BitVec<'_>, bitnr: usize) -> Result<BitValue, BitVecError> {
        let byte = bv.data.get(bitnr / 8).ok_or(BitVecError::OutOfRange)?;
        Ok(if byte & (1 << (7 - bitnr % 8)) == 0 {
            BitValue::Zero
        } else {
            BitValue::One
        })
    }

    /// Reads a padding-relative bit without moving the cursor.
    pub fn bitvec_get_bit_pos_high(bv: &BitVec<'_>, bitnr: usize) -> Result<BitValue, BitVecError> {
        let bit = bitvec_get_bit_pos(bv, bitnr)?;
        let padding_is_one = 0x2bu8 & (1 << (7 - bitnr % 8)) != 0;
        Ok(if (bit == BitValue::One) == padding_is_one {
            BitValue::L
        } else {
            BitValue::H
        })
    }

    /// Sets one absolute bit, preserving all other bits and the cursor.
    pub fn bitvec_set_bit_pos(
        bv: &mut BitVec<'_>,
        bitnr: usize,
        bit: BitValue,
    ) -> Result<(), BitVecError> {
        let byte = bv.data.get_mut(bitnr / 8).ok_or(BitVecError::OutOfRange)?;
        let mask = 1u8 << (7 - bitnr % 8);
        let one = match bit {
            BitValue::Zero => false,
            BitValue::One => true,
            BitValue::L => 0x2b & mask != 0,
            BitValue::H => 0x2b & mask == 0,
        };
        *byte = (*byte & !mask) | if one { mask } else { 0 };
        Ok(())
    }

    /// Sets the cursor bit and advances only after a successful write.
    pub fn bitvec_set_bit(bv: &mut BitVec<'_>, bit: BitValue) -> Result<(), BitVecError> {
        // Validate the access before arithmetic, including an out-of-range MAX cursor.
        bitvec_get_bit_pos(bv, bv.cur_bit)?;
        let next = bv
            .cur_bit
            .checked_add(1)
            .ok_or(BitVecError::ArithmeticOverflow)?;
        bitvec_set_bit_pos(bv, bv.cur_bit, bit)?;
        bv.cur_bit = next;
        Ok(())
    }

    /// Reads the cursor bit as L/H and advances only after a successful read.
    pub fn bitvec_get_bit_high(bv: &mut BitVec<'_>) -> Result<BitValue, BitVecError> {
        let bit = bitvec_get_bit_pos_high(bv, bv.cur_bit)?;
        bv.cur_bit = bv
            .cur_bit
            .checked_add(1)
            .ok_or(BitVecError::ArithmeticOverflow)?;
        Ok(bit)
    }

    /// Writes sequential bits, retaining the written prefix and cursor on error.
    /// An empty input performs no accesses, even with an out-of-range cursor.
    pub fn bitvec_set_bits(bv: &mut BitVec<'_>, bits: &[BitValue]) -> Result<(), BitVecError> {
        for &bit in bits {
            bitvec_set_bit(bv, bit)?;
        }
        Ok(())
    }

    /// Rounds the cursor up to bytes, independently of backing capacity.
    pub fn bitvec_used_bytes(bv: &BitVec<'_>) -> usize {
        bv.cur_bit / 8 + usize::from(!bv.cur_bit.is_multiple_of(8))
    }

    /// Returns tailroom with the reference ABI's 32-bit unsigned wraparound.
    /// The length and cursor are reduced to their low 32 bits, as C unsigned fields
    /// would be. This includes cursors beyond capacity and is independent of the
    /// host's `usize` width. Use the checked helper for a capacity validation.
    pub fn bitvec_tailroom_bits(bv: &BitVec<'_>) -> u32 {
        (bv.data.len() as u32)
            .wrapping_mul(8)
            .wrapping_sub(bv.cur_bit as u32)
    }

    /// Returns actual remaining capacity, rejecting arithmetic overflow and cursors
    /// beyond the slice. This additional Rust helper does not replace the C API.
    pub fn bitvec_tailroom_bits_checked(bv: &BitVec<'_>) -> Result<usize, BitVecError> {
        let capacity = bv
            .data
            .len()
            .checked_mul(8)
            .ok_or(BitVecError::ArithmeticOverflow)?;
        capacity
            .checked_sub(bv.cur_bit)
            .ok_or(BitVecError::OutOfRange)
    }

    /// Writes the low `num_bits` of `value`, most-significant selected bit first.
    /// Widths above 64 fail before accessing data. Capacity errors retain successful
    /// writes and cursor progress. In L/H mode, zero maps to L and one to H.
    pub fn bitvec_set_u64(
        bv: &mut BitVec<'_>,
        value: u64,
        num_bits: u8,
        use_lh: bool,
    ) -> Result<(), BitVecError> {
        if num_bits > 64 {
            return Err(BitVecError::TooManyBits);
        }
        for shift in (0..num_bits).rev() {
            let one = value & (1u64 << shift) != 0;
            let bit = match (use_lh, one) {
                (false, false) => BitValue::Zero,
                (false, true) => BitValue::One,
                (true, false) => BitValue::L,
                (true, true) => BitValue::H,
            };
            bitvec_set_bit(bv, bit)?;
        }
        Ok(())
    }

    /// Writes a 32-bit unsigned value. As in the C wrapper, the count is narrowed
    /// to an unsigned byte before delegating: 256 bits becomes zero, 257 becomes one.
    /// Counts 33..64 write leading zeros before the 32-bit value.
    pub fn bitvec_set_uint(
        bv: &mut BitVec<'_>,
        value: u32,
        num_bits: u32,
    ) -> Result<(), BitVecError> {
        bitvec_set_u64(bv, u64::from(value), num_bits as u8, false)
    }

    /// Reads bits at the cursor and returns the reference's signed 32-bit pattern.
    /// A successful read can be negative; `Ok` distinguishes that from an error.
    /// Capacity errors retain cursor progress. A one bit requiring a shift of 32
    /// or more returns `TooManyBits` rather than executing C's undefined shift;
    /// leading zero bits for such widths retain their defined C behavior.
    pub fn bitvec_get_uint(bv: &mut BitVec<'_>, num_bits: u32) -> Result<i32, BitVecError> {
        let mut value = 0u32;
        for i in 0..num_bits {
            let bit = bitvec_get_bit_pos(bv, bv.cur_bit)?;
            let shift = num_bits - i - 1;
            if bit == BitValue::One {
                if shift >= 32 {
                    return Err(BitVecError::TooManyBits);
                }
                value |= 1u32 << shift;
            }
            bv.cur_bit = bv
                .cur_bit
                .checked_add(1)
                .ok_or(BitVecError::ArithmeticOverflow)?;
        }
        Ok(value as i32)
    }

    /// Reads up to 15 bits from the beginning of the vector, without moving the
    /// cursor. C requires `cur_bit >= num_bits`. Rust additionally checks that the
    /// slice contains the bytes C loads. Zero bits return zero without access.
    pub fn bitvec_get_int16_msb(bv: &BitVec<'_>, num_bits: u32) -> Result<i16, BitVecError> {
        if num_bits > 15 || bv.cur_bit < num_bits as usize {
            return Err(BitVecError::OutOfRange);
        }
        if num_bits == 0 {
            return Ok(0);
        }
        let first = *bv.data.first().ok_or(BitVecError::OutOfRange)?;
        let value = if num_bits <= 8 {
            u16::from(first) >> (8 - num_bits)
        } else {
            let second = *bv.data.get(1).ok_or(BitVecError::OutOfRange)?;
            u16::from_be_bytes([first, second]) >> (16 - num_bits)
        };
        Ok(value as i16)
    }

    /// Fills bits at the cursor, preserving C's wrapping 32-bit stop calculation.
    /// When that calculation wraps below the cursor, no bits are written. Otherwise
    /// errors retain partial writes and cursor progress. Rust-only cursors exceeding
    /// a C unsigned field are rejected before access.
    pub fn bitvec_fill(
        bv: &mut BitVec<'_>,
        num_bits: u32,
        fill: BitValue,
    ) -> Result<(), BitVecError> {
        let start = u32::try_from(bv.cur_bit).map_err(|_| BitVecError::ArithmeticOverflow)?;
        let stop = start.wrapping_add(num_bits);
        for _ in start..stop {
            bitvec_set_bit(bv, fill)?;
        }
        Ok(())
    }

    /// Pads with L through the inclusive final bit position. Preserves the C ABI's
    /// wrapping unsigned subtraction and signed 32-bit count; nonpositive counts
    /// do nothing. Partial failures retain bytes and cursor progress.
    pub fn bitvec_spare_padding(bv: &mut BitVec<'_>, up_to_bit: u32) -> Result<(), BitVecError> {
        let cursor = u32::try_from(bv.cur_bit).map_err(|_| BitVecError::ArithmeticOverflow)?;
        let count = up_to_bit.wrapping_sub(cursor).wrapping_add(1) as i32;
        if count < 1 {
            return Ok(());
        }
        bitvec_fill(bv, count as u32, BitValue::L)
    }

    /// Copies sequential bytes into `dst`, advancing the cursor by `dst.len() * 8`.
    /// Unaligned bytes are combined MSB first. A capacity error leaves both buffers
    /// and the cursor unchanged. Empty transfers still validate the cursor span.
    pub fn bitvec_get_bytes(bv: &mut BitVec<'_>, dst: &mut [u8]) -> Result<(), BitVecError> {
        let (start, offset, next) = byte_span(bv, dst.len())?;
        if offset == 0 {
            dst.copy_from_slice(&bv.data[start..start + dst.len()]);
        } else {
            for (i, byte) in dst.iter_mut().enumerate() {
                *byte = (bv.data[start + i] << offset) | (bv.data[start + i + 1] >> (8 - offset));
            }
        }
        bv.cur_bit = next;
        Ok(())
    }

    /// Writes sequential bytes from `src`, preserving neighboring bits when
    /// unaligned. A capacity error leaves bytes and cursor unchanged. Empty
    /// transfers still validate the cursor span, matching the C byte APIs.
    pub fn bitvec_set_bytes(bv: &mut BitVec<'_>, src: &[u8]) -> Result<(), BitVecError> {
        let (start, offset, next) = byte_span(bv, src.len())?;
        if offset == 0 {
            bv.data[start..start + src.len()].copy_from_slice(src);
        } else {
            for (i, &byte) in src.iter().enumerate() {
                let head_mask = 0xffu8 >> offset;
                let tail_mask = 0xffu8 << (8 - offset);
                bv.data[start + i] = (bv.data[start + i] & !head_mask) | (byte >> offset);
                bv.data[start + i + 1] =
                    (bv.data[start + i + 1] & !tail_mask) | (byte << (8 - offset));
            }
        }
        bv.cur_bit = next;
        Ok(())
    }

    /// Reads an indexed field, setting the cursor to the index on successful
    /// bounds validation. A bounds error corresponds to C's zero return with
    /// `errno = EOVERFLOW` and leaves both cursor and index unchanged. Success
    /// advances both; even a zero-width read sets the cursor to the index.
    /// Nonzero bits needing shifts beyond 63 fail safely rather than executing
    /// C's undefined shift. Rust returns errors explicitly instead of setting errno.
    pub fn bitvec_read_field(
        bv: &mut BitVec<'_>,
        read_index: &mut u32,
        len: u32,
    ) -> Result<u64, BitVecError> {
        if len != 0 {
            let last = read_index.wrapping_add(len).wrapping_sub(1);
            if last as usize / 8 >= bv.data.len() {
                return Err(BitVecError::OutOfRange);
            }
            // C's wrapping last-bit calculation can admit an unsafe span. Keep
            // storage errors atomic in those cases rather than accessing a pointer.
            let end = u64::from(*read_index) + u64::from(len);
            if end > (bv.data.len() as u64).saturating_mul(8) {
                return Err(BitVecError::OutOfRange);
            }
        }
        bv.cur_bit = *read_index as usize;
        let mut value = 0u64;
        for i in 0..len {
            if bitvec_get_bit_pos(bv, bv.cur_bit)? == BitValue::One {
                let shift = len - i - 1;
                if shift >= 64 {
                    return Err(BitVecError::TooManyBits);
                }
                value |= 1u64 << shift;
            }
            bv.cur_bit = (bv.cur_bit as u32).wrapping_add(1) as usize;
        }
        *read_index = read_index.wrapping_add(len);
        Ok(value)
    }

    /// Writes from the supplied index, resetting the cursor before validation.
    /// On failure the index is unchanged, but writes and cursor progress remain.
    /// C narrows the write width to a byte, yet advances the index by the original
    /// 32-bit count on success; the cursor and index can therefore diverge.
    pub fn bitvec_write_field(
        bv: &mut BitVec<'_>,
        write_index: &mut u32,
        value: u64,
        len: u32,
    ) -> Result<(), BitVecError> {
        bv.cur_bit = *write_index as usize;
        bitvec_set_u64(bv, value, len as u8, false)?;
        *write_index = write_index.wrapping_add(len);
        Ok(())
    }

    /// Returns the position of the one-based Nth binary one bit. Like C, zero
    /// means either "not found" (including n=0) or a match at position zero.
    /// The cursor is unused. Capacity uses the reference's wrapping 32-bit count.
    pub fn bitvec_get_nth_set_bit(bv: &BitVec<'_>, n: u32) -> u32 {
        let capacity = (bv.data.len() as u32).wrapping_mul(8);
        let mut found = 0u32;
        for pos in 0..capacity {
            if bv.data[pos as usize / 8] & (1 << (7 - pos % 8)) != 0 {
                found += 1;
                if found == n {
                    return pos;
                }
            }
        }
        0
    }

    /// Finds a binary bit at or after n, leaving the cursor unchanged. L/H never
    /// match because C compares binary reads. Preserves C's -1 sentinel and signed
    /// 32-bit position conversion rather than interpreting the value as an error.
    pub fn bitvec_find_bit_pos(bv: &BitVec<'_>, n: u32, value: BitValue) -> i32 {
        let capacity = (bv.data.len() as u32).wrapping_mul(8);
        for pos in n..capacity {
            let bit = if bv.data[pos as usize / 8] & (1 << (7 - pos % 8)) == 0 {
                BitValue::Zero
            } else {
                BitValue::One
            };
            if bit == value {
                return pos as i32;
            }
        }
        -1
    }

    /// Counts the initial run through the bytes used by the cursor. A differing
    /// final byte is inspected in full, so C's count can exceed `cur_bit`.
    /// Missing backing bytes return an error rather than C's out-of-bounds read;
    /// a mismatch in accessible storage still returns its defined C result.
    pub fn bitvec_rl(bv: &BitVec<'_>, one: bool) -> Result<u32, BitVecError> {
        let cursor = u32::try_from(bv.cur_bit).map_err(|_| BitVecError::ArithmeticOverflow)?;
        let used = cursor / 8 + u32::from(cursor % 8 != 0);
        for i in 0..used {
            let byte = *bv.data.get(i as usize).ok_or(BitVecError::OutOfRange)?;
            if byte != if one { 0xff } else { 0 } {
                return Ok(i * 8 + leading_bits(byte, one));
            }
        }
        Ok(cursor)
    }

    /// Counts a run starting at the cursor, preserving the reference's cursor and
    /// wrapping return calculations. C examines full bytes even at a partial limit;
    /// a mismatch can exceed that limit. An unaligned cursor already at the limit
    /// moves back one bit; an aligned cursor beyond it clamps and wraps the count.
    /// Actual missing storage is reported as an error instead of dereferencing it.
    pub fn bitvec_rl_curbit(
        bv: &mut BitVec<'_>,
        one: bool,
        max_bits: u32,
    ) -> Result<u32, BitVecError> {
        let start = u32::try_from(bv.cur_bit).map_err(|_| BitVecError::ArithmeticOverflow)?;
        let measurement = RunMeasurement::measure(bv.data, start, one, max_bits);
        let (cursor, result) = measurement.compatibility_result(max_bits);
        bv.cur_bit = cursor as usize;
        result
    }

    /// Preserves the local C left-shift implementation, including its unchanged
    /// moved tail bytes and skipped penultimate byte in the residual-bit loop.
    /// Zero is a no-op; n >= cur_bit zeroes the entire slice. Other shifts require
    /// at least two bytes and an in-range move span, preventing C's unsafe accesses
    /// for smaller buffers. Safety errors leave both bytes and cursor unchanged.
    pub fn bitvec_shiftl(bv: &mut BitVec<'_>, n: u32) -> Result<(), BitVecError> {
        if n == 0 {
            return Ok(());
        }
        let cursor = u32::try_from(bv.cur_bit).map_err(|_| BitVecError::ArithmeticOverflow)?;
        if n >= cursor {
            bitvec_zero(bv);
            return Ok(());
        }
        let offset = n as usize / 8;
        if bv.data.len() < 2 || offset > bv.data.len() {
            return Err(BitVecError::OutOfRange);
        }
        bv.data.copy_within(offset.., 0);
        let shift = n % 8;
        for i in 0..bv.data.len() - 2 {
            let pair = u16::from_be_bytes([bv.data[i], bv.data[i + 1]]) << shift;
            bv.data[i] = (pair >> 8) as u8;
        }
        let last = bv.data.len() - 1;
        bv.data[last] <<= shift;
        bv.cur_bit = cursor.wrapping_sub(n) as usize;
        Ok(())
    }

    /// Adds C's CSN.1 repeated `{1, value}` records followed by a zero stop bit.
    /// Dry runs return a wrapping 32-bit estimate and make no changes. Real runs
    /// deliberately ignore setter errors, continue after partial failures, and
    /// return zero, matching C; inspect cursor/storage for the amount written.
    pub fn bitvec_add_array(
        bv: &mut BitVec<'_>,
        array: &[u32],
        dry_run: bool,
        num_bits: u32,
    ) -> u32 {
        if dry_run {
            return 1u32
                .wrapping_add((array.len() as u32).wrapping_mul(1u32.wrapping_add(num_bits)));
        }
        for &value in array {
            let _ = bitvec_set_bit(bv, BitValue::One);
            let _ = bitvec_set_uint(bv, value, num_bits);
        }
        let _ = bitvec_set_bit(bv, BitValue::Zero);
        0
    }

    /// Copies the entire backing array to `dst`, including unused bytes, without
    /// changing the cursor. Returns the copied byte count. Additional output bytes
    /// are unchanged. Unlike C, an undersized output fails before writing.
    pub fn bitvec_pack(bv: &BitVec<'_>, dst: &mut [u8]) -> Result<usize, BitVecError> {
        if dst.len() < bv.data.len() {
            return Err(BitVecError::OutOfRange);
        }
        dst[..bv.data.len()].copy_from_slice(bv.data);
        Ok(bv.data.len())
    }

    /// Copies the first backing-length bytes from `src`, leaving the cursor
    /// unchanged. Returns the copied byte count; extra input bytes are ignored.
    /// An undersized input fails before changing the vector.
    pub fn bitvec_unpack(bv: &mut BitVec<'_>, src: &[u8]) -> Result<usize, BitVecError> {
        if src.len() < bv.data.len() {
            return Err(BitVecError::OutOfRange);
        }
        let len = bv.data.len();
        bv.data.copy_from_slice(&src[..len]);
        Ok(len)
    }

    /// Loads hexadecimal bytes using `parse_into`, returning C's 0 on success
    /// and 1 on error. Parsing clears the entire buffer first and preserves partial
    /// writes on failure. Only success moves the cursor to parsed bytes times eight.
    /// Input ends at the first NUL or the Rust slice's end.
    pub fn bitvec_unhex(bv: &mut BitVec<'_>, src: &[u8]) -> i32 {
        match crate::hexparse::parse_into(src, bv.data) {
            Ok(count) => match count.checked_mul(8) {
                Some(cursor) => {
                    bv.cur_bit = cursor;
                    0
                }
                None => 1,
            },
            Err(_) => 1,
        }
    }

    /// Formats the first `cur_bit` binary bits as C's NUL-terminated text. A space
    /// precedes each group of up to eight bits, including the first group; L/H
    /// are displayed as their stored binary bits. Empty vectors write only NUL.
    /// Required output length is `cur_bit + ceil(cur_bit / 8) + 1`, which exceeds
    /// the size stated in C's comment. Rust validates storage and output capacity
    /// before any writes; the cursor and bytes after the terminator are unchanged.
    pub fn bitvec_to_string_r(bv: &BitVec<'_>, dst: &mut [u8]) -> Result<(), BitVecError> {
        let used = bitvec_used_bytes(bv);
        if used > bv.data.len() {
            return Err(BitVecError::OutOfRange);
        }
        let needed = bv
            .cur_bit
            .checked_add(used)
            .and_then(|n| n.checked_add(1))
            .ok_or(BitVecError::ArithmeticOverflow)?;
        if dst.len() < needed {
            return Err(BitVecError::OutOfRange);
        }
        let mut out = 0;
        for bit in 0..bv.cur_bit {
            if bit % 8 == 0 {
                dst[out] = b' ';
                out += 1;
            }
            dst[out] = if bv.data[bit / 8] & (1 << (7 - bit % 8)) == 0 {
                b'0'
            } else {
                b'1'
            };
            out += 1;
        }
        dst[out] = 0;
        Ok(())
    }

    /// Clears the complete buffer and resets the cursor.
    pub fn bitvec_zero(bv: &mut BitVec<'_>) {
        bv.data.fill(0);
        bv.cur_bit = 0;
    }
}

impl BitVec<'_> {
    /// See [`crate::compat::bitvec::bitvec_get_bit_pos`] for mutation and error details.
    pub fn get(&self, bitnr: usize) -> Result<BitValue, BitVecError> {
        legacy::bitvec_get_bit_pos(self, bitnr)
    }
    /// See [`crate::compat::bitvec::bitvec_get_bit_pos_high`] for mutation and error details.
    pub fn get_padding_bit(&self, bitnr: usize) -> Result<BitValue, BitVecError> {
        legacy::bitvec_get_bit_pos_high(self, bitnr)
    }
    /// See [`crate::compat::bitvec::bitvec_set_bit_pos`] for mutation and error details.
    pub fn set(&mut self, bitnr: usize, bit: BitValue) -> Result<(), BitVecError> {
        legacy::bitvec_set_bit_pos(self, bitnr, bit)
    }
    /// See [`crate::compat::bitvec::bitvec_set_bit`] for mutation and error details.
    pub fn write_bit(&mut self, bit: BitValue) -> Result<(), BitVecError> {
        legacy::bitvec_set_bit(self, bit)
    }
    /// See [`crate::compat::bitvec::bitvec_get_bit_high`] for mutation and error details.
    pub fn read_padding_bit(&mut self) -> Result<BitValue, BitVecError> {
        legacy::bitvec_get_bit_high(self)
    }
    /// See [`crate::compat::bitvec::bitvec_set_bits`] for mutation and error details.
    pub fn write_bits(&mut self, bits: &[BitValue]) -> Result<(), BitVecError> {
        legacy::bitvec_set_bits(self, bits)
    }
    /// See [`crate::compat::bitvec::bitvec_used_bytes`] for mutation and error details.
    pub fn used_bytes(&self) -> usize {
        legacy::bitvec_used_bytes(self)
    }
    /// See [`crate::compat::bitvec::bitvec_tailroom_bits_checked`] for mutation and error details.
    pub fn remaining_bits(&self) -> Result<usize, BitVecError> {
        legacy::bitvec_tailroom_bits_checked(self)
    }
    /// See [`crate::compat::bitvec::bitvec_set_u64`] for mutation and error details.
    pub fn write_u64(&mut self, value: u64, num_bits: u8, use_lh: bool) -> Result<(), BitVecError> {
        legacy::bitvec_set_u64(self, value, num_bits, use_lh)
    }
    /// See [`crate::compat::bitvec::bitvec_set_uint`] for mutation and error details.
    pub fn write_uint(&mut self, value: u32, num_bits: u32) -> Result<(), BitVecError> {
        legacy::bitvec_set_uint(self, value, num_bits)
    }
    /// See [`crate::compat::bitvec::bitvec_get_uint`] for mutation and error details.
    pub fn read_uint(&mut self, num_bits: u32) -> Result<i32, BitVecError> {
        legacy::bitvec_get_uint(self, num_bits)
    }
    /// See [`crate::compat::bitvec::bitvec_get_int16_msb`] for mutation and error details.
    pub fn read_int16_msb(&self, num_bits: u32) -> Result<i16, BitVecError> {
        legacy::bitvec_get_int16_msb(self, num_bits)
    }
    /// See [`crate::compat::bitvec::bitvec_fill`] for mutation and error details.
    pub fn fill(&mut self, num_bits: u32, fill: BitValue) -> Result<(), BitVecError> {
        legacy::bitvec_fill(self, num_bits, fill)
    }
    /// See [`crate::compat::bitvec::bitvec_spare_padding`] for mutation and error details.
    pub fn spare_padding(&mut self, up_to_bit: u32) -> Result<(), BitVecError> {
        legacy::bitvec_spare_padding(self, up_to_bit)
    }
    /// See [`crate::compat::bitvec::bitvec_get_bytes`] for mutation and error details.
    pub fn read_bytes(&mut self, dst: &mut [u8]) -> Result<(), BitVecError> {
        legacy::bitvec_get_bytes(self, dst)
    }
    /// See [`crate::compat::bitvec::bitvec_set_bytes`] for mutation and error details.
    pub fn write_bytes(&mut self, src: &[u8]) -> Result<(), BitVecError> {
        legacy::bitvec_set_bytes(self, src)
    }
    /// See [`crate::compat::bitvec::bitvec_read_field`] for mutation and error details.
    pub fn read_field(&mut self, read_index: &mut u32, len: u32) -> Result<u64, BitVecError> {
        legacy::bitvec_read_field(self, read_index, len)
    }
    /// See [`crate::compat::bitvec::bitvec_write_field`] for mutation and error details.
    pub fn write_field(
        &mut self,
        write_index: &mut u32,
        value: u64,
        len: u32,
    ) -> Result<(), BitVecError> {
        legacy::bitvec_write_field(self, write_index, value, len)
    }
    /// See [`crate::compat::bitvec::bitvec_rl`] for mutation and error details.
    pub fn rl(&self, one: bool) -> Result<u32, BitVecError> {
        legacy::bitvec_rl(self, one)
    }
    /// See [`crate::compat::bitvec::bitvec_rl_curbit`] for mutation and error details.
    pub fn rl_curbit(&mut self, one: bool, max_bits: u32) -> Result<u32, BitVecError> {
        legacy::bitvec_rl_curbit(self, one, max_bits)
    }
    /// See [`crate::compat::bitvec::bitvec_shiftl`] for mutation and error details.
    pub fn shift_left(&mut self, n: u32) -> Result<(), BitVecError> {
        legacy::bitvec_shiftl(self, n)
    }
    /// See [`crate::compat::bitvec::bitvec_pack`] for mutation and error details.
    pub fn pack(&self, dst: &mut [u8]) -> Result<usize, BitVecError> {
        legacy::bitvec_pack(self, dst)
    }
    /// See [`crate::compat::bitvec::bitvec_unpack`] for mutation and error details.
    pub fn unpack(&mut self, src: &[u8]) -> Result<usize, BitVecError> {
        legacy::bitvec_unpack(self, src)
    }
    /// See [`crate::compat::bitvec::bitvec_zero`] for mutation and error details.
    pub fn zero(&mut self) {
        legacy::bitvec_zero(self)
    }
}
