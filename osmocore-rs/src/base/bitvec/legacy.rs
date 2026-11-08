// SPDX-License-Identifier: GPL-3.0-or-later

//! The C-named operations behind `BitVec` and `compat::bitvec`.

use super::*;

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
pub fn bitvec_set_uint(bv: &mut BitVec<'_>, value: u32, num_bits: u32) -> Result<(), BitVecError> {
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
pub fn bitvec_fill(bv: &mut BitVec<'_>, num_bits: u32, fill: BitValue) -> Result<(), BitVecError> {
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
            bv.data[start + i + 1] = (bv.data[start + i + 1] & !tail_mask) | (byte << (8 - offset));
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
pub fn bitvec_rl_curbit(bv: &mut BitVec<'_>, one: bool, max_bits: u32) -> Result<u32, BitVecError> {
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
pub fn bitvec_add_array(bv: &mut BitVec<'_>, array: &[u32], dry_run: bool, num_bits: u32) -> u32 {
    if dry_run {
        return 1u32.wrapping_add((array.len() as u32).wrapping_mul(1u32.wrapping_add(num_bits)));
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
    match crate::base::hexparse::parse_into(src, bv.data) {
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
