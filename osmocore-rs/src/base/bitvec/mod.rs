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

pub(crate) mod legacy;

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

#[cfg(test)]
mod tests;
