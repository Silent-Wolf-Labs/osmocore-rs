// SPDX-License-Identifier: GPL-3.0-or-later

//! Byte CRC-16/CCITT and generic parameterized CRCs.
//!
//! Generic input bytes contribute only their low bit. Supplied CRC bytes are
//! compared exactly to 0/1. Negative input lengths consume no bytes, like C.
//! Descriptor fields are not pre-masked, and the final XOR is not masked.
//!
//! Widths outside 1..=storage width are errors, including on empty input. Full
//! widths 32/64 use an all-ones mask and wrapping arithmetic: this safely defines
//! positive-input cases where the C mask shifts by the operand width (UB).
//! Check/set validate the complete output slice before accessing it; set errors
//! leave all destination bytes unchanged. No C layout or pointer ABI is implied.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrcError {
    InvalidWidth,
    InputTooShort,
    OutputTooShort,
}

const fn table(poly: u16) -> [u16; 256] {
    let mut values = [0; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u16;
        let mut bit = 0;
        while bit < 8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { poly } else { 0 };
            bit += 1;
        }
        values[index] = crc;
        index += 1;
    }
    values
}

/// Bounded counterpart of `osmo_crc16_table` (reflected polynomial 0xa001).
pub(crate) const OSMO_CRC16_TABLE: [u16; 256] = table(0xa001);
/// Bounded counterpart of `osmo_crc16_ccitt_table` (reflected polynomial 0x8408).
pub(crate) const OSMO_CRC16_CCITT_TABLE: [u16; 256] = table(0x8408);

fn bounds(bits: i32, width: u32, input: &[u8], len: i32) -> Result<(usize, &[u8]), CrcError> {
    if bits < 1 || bits as u32 > width {
        return Err(CrcError::InvalidWidth);
    }
    let count = len.max(0) as usize;
    let input = input.get(..count).ok_or(CrcError::InputTooShort)?;
    Ok((bits as usize, input))
}

// Share the four typed implementations without introducing a public generic
// framework or weakening the width-specific descriptor/value types.
macro_rules! generic_crc {
    ($code:ident, $ty:ty, $compute:ident, $check:ident, $set:ident) => {
        /// Compute an unpacked-bit CRC; nonpositive lengths return init XOR remainder.
        /// Full storage widths use a safe all-ones mask; 32/64-bit positive-input
        /// full widths extend C behavior because the corresponding C call has UB.
        pub fn $compute(code: &$code, input: &[u8], len: i32) -> Result<$ty, CrcError> {
            let (bits, input) = bounds(code.bits, <$ty>::BITS, input, len)?;
            let mask = if bits == <$ty>::BITS as usize {
                <$ty>::MAX
            } else {
                ((1 as $ty) << bits) - 1
            };
            let top = (1 as $ty) << (bits - 1);
            let mut state = code.init;
            for byte in input {
                state ^= ((*byte & 1) as $ty) << (bits - 1);
                let feedback = state & top != 0;
                state = state.wrapping_shl(1);
                if feedback {
                    state ^= code.poly;
                }
                state &= mask;
            }
            Ok(state ^ code.remainder)
        }

        /// Return C's 0 for match or 1 for mismatch; supplied CRC bytes are not
        /// normalized. Even early mismatches require a complete CRC slice.
        pub fn $check(
            code: &$code,
            input: &[u8],
            len: i32,
            crc_bits: &[u8],
        ) -> Result<i32, CrcError> {
            let (bits, _) = bounds(code.bits, <$ty>::BITS, input, len)?;
            let crc_bits = crc_bits.get(..bits).ok_or(CrcError::OutputTooShort)?;
            let value = $compute(code, input, len)?;
            for (index, byte) in crc_bits.iter().enumerate() {
                if *byte != ((value >> (bits - index - 1)) & 1) as u8 {
                    return Ok(1);
                }
            }
            Ok(0)
        }

        /// Write exactly the configured number of MSB-first CRC bytes, leaving
        /// suffixes untouched. All bounds/width errors leave the output unchanged.
        pub fn $set(
            code: &$code,
            input: &[u8],
            len: i32,
            crc_bits: &mut [u8],
        ) -> Result<(), CrcError> {
            let (bits, _) = bounds(code.bits, <$ty>::BITS, input, len)?;
            let crc_bits = crc_bits.get_mut(..bits).ok_or(CrcError::OutputTooShort)?;
            let value = $compute(code, input, len)?;
            for (index, byte) in crc_bits.iter_mut().enumerate() {
                *byte = ((value >> (bits - index - 1)) & 1) as u8;
            }
            Ok(())
        }
    };
}

#[cfg(test)]
mod tests {
    use super::legacy::*;
    use super::*;

    // Independent full-polynomial division in u128, rather than the production
    // bit-at-a-time register algorithm. Input lengths stay <=64, so the complete
    // dividend fits in 128 bits for both 32- and 64-bit polynomials.
    fn divide(width: usize, poly: u64, init: u64, remainder: u64, input: &[u8]) -> u64 {
        let message = input
            .iter()
            .fold(0u128, |v, b| (v << 1) | u128::from(b & 1));
        let mut dividend = (u128::from(init) << input.len()) ^ (message << width);
        let divisor = (1u128 << width) | u128::from(poly);
        for position in (width..width + input.len()).rev() {
            if dividend & (1u128 << position) != 0 {
                dividend ^= divisor << (position - width);
            }
        }
        (dividend as u64) ^ remainder
    }

    #[test]
    fn full_width_extensions_match_independent_polynomial_division() {
        for len in [1, 7, 32, 64] {
            let input: Vec<_> = (0..len).map(|i| [0, 1, 2, 3, 0x80, 0xff][i % 6]).collect();
            for (poly, init, remainder) in [
                (0, 0, 0),
                (3, 1, u64::MAX),
                (u64::MAX, u64::MAX, 1),
                (0xaaaaaaaaaaaaaaaa, 0x5555555555555555, u64::MAX),
            ] {
                let c64 = Crc64Code {
                    bits: 64,
                    poly,
                    init,
                    remainder,
                };
                assert_eq!(
                    osmo_crc64gen_compute_bits(&c64, &input, len as i32).unwrap(),
                    divide(64, poly, init, remainder, &input)
                );
                let c32 = Crc32Code {
                    bits: 32,
                    poly: poly as u32,
                    init: init as u32,
                    remainder: remainder as u32,
                };
                assert_eq!(
                    osmo_crc32gen_compute_bits(&c32, &input, len as i32).unwrap(),
                    divide(
                        32,
                        poly as u32 as u64,
                        init as u32 as u64,
                        remainder as u32 as u64,
                        &input
                    ) as u32
                );
                let mut out64 = [0x7e; 67];
                osmo_crc64gen_set_bits(&c64, &input, len as i32, &mut out64).unwrap();
                let expected = divide(64, poly, init, remainder, &input);
                assert_eq!(
                    &out64[..64],
                    &(0..64)
                        .rev()
                        .map(|i| ((expected >> i) & 1) as u8)
                        .collect::<Vec<_>>()
                );
                assert_eq!(&out64[64..], &[0x7e; 3]);
                assert_eq!(
                    osmo_crc64gen_check_bits(&c64, &input, len as i32, &out64),
                    Ok(0)
                );
                out64[0] ^= 2;
                assert_eq!(
                    osmo_crc64gen_check_bits(&c64, &input, len as i32, &out64),
                    Ok(1)
                );
                let mut out32 = [0x7e; 35];
                osmo_crc32gen_set_bits(&c32, &input, len as i32, &mut out32).unwrap();
                let expected = divide(
                    32,
                    poly as u32 as u64,
                    init as u32 as u64,
                    remainder as u32 as u64,
                    &input,
                ) as u32;
                assert_eq!(
                    &out32[..32],
                    &(0..32)
                        .rev()
                        .map(|i| ((expected >> i) & 1) as u8)
                        .collect::<Vec<_>>()
                );
                assert_eq!(&out32[32..], &[0x7e; 3]);
                assert_eq!(
                    osmo_crc32gen_check_bits(&c32, &input, len as i32, &out32),
                    Ok(0)
                );
                out32[0] ^= 2;
                assert_eq!(
                    osmo_crc32gen_check_bits(&c32, &input, len as i32, &out32),
                    Ok(1)
                );
            }
        }
        // Hand-traced injection of a single 1 into a zero register yields the
        // polynomial after the first shift/reduction, not a compiler UB result.
        assert_eq!(
            osmo_crc64gen_compute_bits(
                &Crc64Code {
                    bits: 64,
                    poly: 3,
                    init: 0,
                    remainder: 0
                },
                &[1],
                1
            ),
            Ok(3)
        );
        assert_eq!(
            osmo_crc32gen_compute_bits(
                &Crc32Code {
                    bits: 32,
                    poly: 3,
                    init: 0,
                    remainder: 0
                },
                &[1],
                1
            ),
            Ok(3)
        );
    }

    #[test]
    fn invalid_inputs_are_checked_before_access_and_writes() {
        macro_rules! check_width {
            ($code:ident, $ty:ty, $compute:ident, $check:ident, $set:ident) => {
                for bits in [i32::MIN, -1, 0, <$ty>::BITS as i32 + 1, i32::MAX] {
                    let code = $code {
                        bits,
                        poly: 0,
                        init: 0,
                        remainder: 0,
                    };
                    for len in [i32::MIN, 0, 1] {
                        let mut out = [0x7e; 3];
                        assert_eq!($compute(&code, &[], len), Err(CrcError::InvalidWidth));
                        assert_eq!($check(&code, &[], len, &[]), Err(CrcError::InvalidWidth));
                        assert_eq!($set(&code, &[], len, &mut out), Err(CrcError::InvalidWidth));
                        assert_eq!(out, [0x7e; 3]);
                    }
                }
                let code = $code {
                    bits: <$ty>::BITS as i32,
                    poly: 3,
                    init: 1,
                    remainder: 1,
                };
                let mut out = [0x7e; 2];
                assert_eq!($compute(&code, &[], i32::MAX), Err(CrcError::InputTooShort));
                assert_eq!($check(&code, &[], 1, &[]), Err(CrcError::InputTooShort));
                assert_eq!($set(&code, &[], 1, &mut out), Err(CrcError::InputTooShort));
                assert_eq!($check(&code, &[], 0, &[2]), Err(CrcError::OutputTooShort));
                assert_eq!(
                    $set(&code, &[], -1, &mut out),
                    Err(CrcError::OutputTooShort)
                );
                assert_eq!(out, [0x7e; 2]);
            };
        }
        check_width!(
            Crc8Code,
            u8,
            osmo_crc8gen_compute_bits,
            osmo_crc8gen_check_bits,
            osmo_crc8gen_set_bits
        );
        check_width!(
            Crc16Code,
            u16,
            osmo_crc16gen_compute_bits,
            osmo_crc16gen_check_bits,
            osmo_crc16gen_set_bits
        );
        check_width!(
            Crc32Code,
            u32,
            osmo_crc32gen_compute_bits,
            osmo_crc32gen_check_bits,
            osmo_crc32gen_set_bits
        );
        check_width!(
            Crc64Code,
            u64,
            osmo_crc64gen_compute_bits,
            osmo_crc64gen_check_bits,
            osmo_crc64gen_set_bits
        );
        assert_eq!(osmo_crc16(1, &[], usize::MAX), Err(CrcError::InputTooShort));
        assert_eq!(osmo_crc16_ccitt(1, &[], 1), Err(CrcError::InputTooShort));
    }
}

pub(crate) mod legacy {
    use super::*;
    generic_crc!(
        Crc8Code,
        u8,
        osmo_crc8gen_compute_bits,
        osmo_crc8gen_check_bits,
        osmo_crc8gen_set_bits
    );
    generic_crc!(
        Crc16Code,
        u16,
        osmo_crc16gen_compute_bits,
        osmo_crc16gen_check_bits,
        osmo_crc16gen_set_bits
    );
    generic_crc!(
        Crc32Code,
        u32,
        osmo_crc32gen_compute_bits,
        osmo_crc32gen_check_bits,
        osmo_crc32gen_set_bits
    );
    generic_crc!(
        Crc64Code,
        u64,
        osmo_crc64gen_compute_bits,
        osmo_crc64gen_check_bits,
        osmo_crc64gen_set_bits
    );

    pub fn osmo_crc16_byte(crc: u16, data: u8) -> u16 {
        (crc >> 8) ^ OSMO_CRC16_TABLE[usize::from((crc ^ u16::from(data)) & 0xff)]
    }

    pub fn osmo_crc16_ccitt_byte(crc: u16, data: u8) -> u16 {
        (crc >> 8) ^ OSMO_CRC16_CCITT_TABLE[usize::from((crc ^ u16::from(data)) & 0xff)]
    }

    /// Compute byte CRC using caller-provided initial state, with no final XOR.
    pub fn osmo_crc16(crc: u16, buffer: &[u8], len: usize) -> Result<u16, CrcError> {
        let input = buffer.get(..len).ok_or(CrcError::InputTooShort)?;
        Ok(input
            .iter()
            .fold(crc, |state, byte| osmo_crc16_byte(state, *byte)))
    }

    /// Compute CCITT byte CRC using caller-provided state, with no final XOR.
    pub fn osmo_crc16_ccitt(crc: u16, buffer: &[u8], len: usize) -> Result<u16, CrcError> {
        let input = buffer.get(..len).ok_or(CrcError::InputTooShort)?;
        Ok(input
            .iter()
            .fold(crc, |state, byte| osmo_crc16_ccitt_byte(state, *byte)))
    }
}

macro_rules! code_type {
    ($code:ident, $ty:ty, $compute:ident, $check:ident, $set:ident) => {
        /// Raw descriptor; widths are checked on every call before arithmetic.
        /// High polynomial/init/remainder bits remain accepted and observable.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $code {
            pub bits: i32,
            pub poly: $ty,
            pub init: $ty,
            pub remainder: $ty,
        }

        impl $code {
            /// Compute a CRC over the whole unpacked-bit slice.
            pub fn compute_bits(&self, input: &[u8]) -> Result<$ty, CrcError> {
                let len = i32::try_from(input.len()).map_err(|_| CrcError::InputTooShort)?;
                legacy::$compute(self, input, len)
            }
            /// Compare supplied CRC bits, retaining the reference's raw byte comparisons.
            pub fn check_bits(&self, input: &[u8], crc_bits: &[u8]) -> Result<bool, CrcError> {
                let len = i32::try_from(input.len()).map_err(|_| CrcError::InputTooShort)?;
                legacy::$check(self, input, len, crc_bits).map(|status| status == 0)
            }
            /// Write the configured CRC bits, preserving the destination suffix.
            pub fn set_bits(&self, input: &[u8], crc_bits: &mut [u8]) -> Result<(), CrcError> {
                let len = i32::try_from(input.len()).map_err(|_| CrcError::InputTooShort)?;
                legacy::$set(self, input, len, crc_bits)
            }
        }
    };
}
code_type!(
    Crc8Code,
    u8,
    osmo_crc8gen_compute_bits,
    osmo_crc8gen_check_bits,
    osmo_crc8gen_set_bits
);
code_type!(
    Crc16Code,
    u16,
    osmo_crc16gen_compute_bits,
    osmo_crc16gen_check_bits,
    osmo_crc16gen_set_bits
);
code_type!(
    Crc32Code,
    u32,
    osmo_crc32gen_compute_bits,
    osmo_crc32gen_check_bits,
    osmo_crc32gen_set_bits
);
code_type!(
    Crc64Code,
    u64,
    osmo_crc64gen_compute_bits,
    osmo_crc64gen_check_bits,
    osmo_crc64gen_set_bits
);

/// Update CRC-16 over an entire byte slice.
pub fn crc16(initial: u16, input: &[u8]) -> u16 {
    input
        .iter()
        .fold(initial, |state, &byte| legacy::osmo_crc16_byte(state, byte))
}
/// Update reflected CRC-16/CCITT over an entire byte slice.
pub fn crc16_ccitt(initial: u16, input: &[u8]) -> u16 {
    input.iter().fold(initial, |state, &byte| {
        legacy::osmo_crc16_ccitt_byte(state, byte)
    })
}
