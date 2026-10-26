// SPDX-License-Identifier: GPL-3.0-or-later

use super::{ConvCode, ConvError, Termination};

/// Reusable encoder borrowing an immutable descriptor. All fallible operations
/// validate before changing state or output; reset starts another frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvEncoder<'a> {
    code: &'a ConvCode,
    state: u8,
    i_idx: usize,
    p_idx: usize,
}

impl<'a> ConvEncoder<'a> {
    pub fn new(code: &'a ConvCode) -> Self {
        Self {
            code,
            state: 0,
            i_idx: 0,
            p_idx: 0,
        }
    }

    pub fn reset(&mut self) {
        self.state = 0;
        self.i_idx = 0;
        self.p_idx = 0;
    }

    pub fn state(&self) -> u8 {
        self.state
    }

    pub fn input_index(&self) -> usize {
        self.i_idx
    }

    pub fn puncture_index(&self) -> usize {
        self.p_idx
    }

    /// Pack the first `K-1` bytes, narrowing to a byte on each iteration.
    /// C accepts nonbinary bytes here. This does not walk transitions or reset
    /// counters; subsequent table-using calls check the resulting state.
    pub fn load_state(&mut self, input: &[u8]) -> Result<(), ConvError> {
        let input = input
            .get(..(self.code.spec.k - 1) as usize)
            .ok_or(ConvError::InputTooShort)?;
        self.state = input
            .iter()
            .fold(0u8, |state, &bit| state.wrapping_shl(1) | bit);
        Ok(())
    }

    fn capacity(&self, symbols: usize) -> Result<usize, ConvError> {
        i32::try_from(symbols).map_err(|_| ConvError::ArithmeticOverflow)?;
        let end_index = self
            .i_idx
            .checked_add(symbols)
            .filter(|&index| index <= i32::MAX as usize)
            .ok_or(ConvError::ArithmeticOverflow)?;
        let rate = self.code.spec.n as usize;
        let raw = symbols
            .checked_mul(rate)
            .ok_or(ConvError::ArithmeticOverflow)?;
        let mut removed = 0;
        if let Some(puncture) = &self.code.spec.puncture
            && symbols != 0
        {
            let end = end_index
                .checked_mul(rate)
                .filter(|&end| end - 1 <= i32::MAX as usize)
                .ok_or(ConvError::ArithmeticOverflow)?;
            removed = puncture[self.p_idx..]
                .iter()
                .take_while(|&&position| (position as usize) < end)
                .count();
        }
        let written = raw - removed;
        i32::try_from(written).map_err(|_| ConvError::ArithmeticOverflow)?;
        Ok(written)
    }

    /// Exact transmitted count for a chunk at the current stream position.
    pub fn raw_output_capacity(&self, symbols: usize) -> Result<usize, ConvError> {
        self.capacity(symbols)
    }

    /// Exact transmitted count for an explicit `K-1` transition flush.
    pub fn flush_output_capacity(&self) -> Result<usize, ConvError> {
        self.capacity((self.code.spec.k - 1) as usize)
    }

    /// Encode the entire supplied hard-bit slice, retaining state and cursors.
    pub fn encode_raw(&mut self, input: &[u8], output: &mut [u8]) -> Result<usize, ConvError> {
        let written = self.raw_output_capacity(input.len())?;
        validate_bits(input)?;
        if !input.is_empty() && usize::from(self.state) >= self.code.state_count() {
            return Err(ConvError::InvalidState);
        }
        if output.len() < written {
            return Err(ConvError::OutputTooShort);
        }
        let mut offset = 0;
        for &bit in input {
            let state = usize::from(self.state);
            let symbol = self.code.spec.next_output[state][usize::from(bit)];
            self.state = self.code.spec.next_state[state][usize::from(bit)];
            offset += self.emit(symbol, &mut output[offset..]);
        }
        Ok(written)
    }

    /// Flush even for non-flush descriptors. Repeated flushes are allowed.
    pub fn flush(&mut self, output: &mut [u8]) -> Result<usize, ConvError> {
        let written = self.flush_output_capacity()?;
        if self.code.spec.k > 1 && usize::from(self.state) >= self.code.state_count() {
            return Err(ConvError::InvalidState);
        }
        if output.len() < written {
            return Err(ConvError::OutputTooShort);
        }
        let mut offset = 0;
        for _ in 0..self.code.spec.k - 1 {
            let state = usize::from(self.state);
            let symbol = if let Some(term_output) = &self.code.spec.next_term_output {
                let symbol = term_output[state];
                self.state = self.code.spec.next_term_state.as_ref().unwrap()[state];
                symbol
            } else {
                let symbol = self.code.spec.next_output[state][0];
                self.state = self.code.spec.next_state[state][0];
                symbol
            };
            offset += self.emit(symbol, &mut output[offset..]);
        }
        Ok(written)
    }

    fn emit(&mut self, symbol: u8, output: &mut [u8]) -> usize {
        let rate = self.code.spec.n as usize;
        let mut written = 0;
        for bit in 0..rate {
            if self
                .code
                .spec
                .puncture
                .as_ref()
                .and_then(|puncture| puncture.get(self.p_idx))
                .is_some_and(|&puncture| puncture as usize == self.i_idx * rate + bit)
            {
                self.p_idx += 1;
            } else {
                output[written] = ((u32::from(symbol) >> (rate - bit - 1)) & 1) as u8;
                written += 1;
            }
        }
        self.i_idx += 1;
        written
    }
}

fn validate_bits(input: &[u8]) -> Result<(), ConvError> {
    if input.iter().any(|&bit| bit > 1) {
        Err(ConvError::InvalidBit)
    } else {
        Ok(())
    }
}

/// Signed nonpositive counts are C-compatible no-ops; unused input is ignored.
pub fn osmo_conv_encode_raw(
    encoder: &mut ConvEncoder<'_>,
    input: &[u8],
    output: &mut [u8],
    n: i32,
) -> Result<usize, ConvError> {
    let input = input
        .get(..n.max(0) as usize)
        .ok_or(ConvError::InputTooShort)?;
    encoder.encode_raw(input, output)
}

/// Encode exactly descriptor `len` bits. Extra input is ignored; extra output
/// is untouched. Tail-biting loads the final `K-1` data bits without padding.
pub fn encode(code: &ConvCode, input: &[u8], output: &mut [u8]) -> Result<usize, ConvError> {
    let capacity = code.output_capacity()?;
    let input = input
        .get(..code.spec.len as usize)
        .ok_or(ConvError::InputTooShort)?;
    validate_bits(input)?;
    if output.len() < capacity {
        return Err(ConvError::OutputTooShort);
    }
    let mut encoder = ConvEncoder::new(code);
    if code.spec.term == Termination::TAIL_BITING {
        encoder.load_state(&input[input.len() - (code.spec.k - 1) as usize..])?;
    }
    let written = encoder.encode_raw(input, output)?;
    if code.spec.term == Termination::FLUSH {
        encoder.flush(&mut output[written..])?;
    }
    Ok(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::conv::osmo_conv_get_output_length;
    use crate::conv::ConvCodeSpec;

    fn code(n: i32, puncture: Option<Vec<i32>>) -> ConvCode {
        ConvCode::new(ConvCodeSpec {
            n,
            k: 1,
            len: 0,
            term: Termination::TRUNCATION,
            next_output: vec![[0, 255]],
            next_state: vec![[0, 0]],
            next_term_output: None,
            next_term_state: None,
            puncture,
        })
        .unwrap()
    }

    #[test]
    fn unpunctured_stream_checks_symbol_counter_not_unused_absolute_position() {
        let code = code(32, None);
        let mut encoder = ConvEncoder::new(&code);
        encoder.i_idx = i32::MAX as usize - 1;
        let mut output = [0xa5; 36];
        assert_eq!(encoder.encode_raw(&[1], &mut output), Ok(32));
        assert_eq!(&output[..24], &[0; 24]);
        assert_eq!(&output[24..32], &[1; 8]);
        assert_eq!(&output[32..], &[0xa5; 4]);
        let previous = encoder.clone();
        let bytes = output;
        assert_eq!(encoder.encode_raw(&[], &mut []), Ok(0));
        assert_eq!(
            encoder.encode_raw(&[0], &mut output),
            Err(ConvError::ArithmeticOverflow)
        );
        assert_eq!(encoder, previous);
        assert_eq!(output, bytes);
    }

    #[test]
    fn present_puncture_list_checks_absolute_positions_even_when_exhausted() {
        for puncture in [vec![], vec![0]] {
            let code = code(32, Some(puncture));
            let mut encoder = ConvEncoder::new(&code);
            encoder.p_idx = code.spec.puncture.as_ref().unwrap().len();
            encoder.i_idx = i32::MAX as usize / 32;
            assert_eq!(encoder.raw_output_capacity(1), Ok(32));
            assert_eq!(encoder.encode_raw(&[0], &mut [0; 32]), Ok(32));
            assert_eq!(encoder.raw_output_capacity(0), Ok(0));
            let previous = encoder.clone();
            let mut output = [0xa5; 32];
            assert_eq!(
                encoder.encode_raw(&[0], &mut output),
                Err(ConvError::ArithmeticOverflow)
            );
            assert_eq!(encoder, previous);
            assert_eq!(output, [0xa5; 32]);
        }
    }

    #[test]
    fn compatibility_intermediate_overflow_does_not_poison_actual_capacity() {
        let mut spec = code(2, Some(vec![0, 1])).spec().clone();
        spec.len = i32::MAX / 2 + 1;
        let code = ConvCode::new(spec).unwrap();
        assert_eq!(
            osmo_conv_get_output_length(&code, 0),
            Err(ConvError::ArithmeticOverflow)
        );
        assert_eq!(code.output_capacity(), Ok(i32::MAX as usize - 1));
    }

    #[test]
    fn arbitrary_state_packing_is_defined_but_invalid_table_access_is_rejected() {
        let mut spec = code(1, None).spec().clone();
        spec.k = 2;
        spec.next_output.push([0, 1]);
        spec.next_state.push([0, 1]);
        let code = ConvCode::new(spec).unwrap();
        let mut encoder = ConvEncoder::new(&code);
        assert_eq!(encoder.load_state(&[255]), Ok(()));
        assert_eq!(encoder.state(), 255);
        assert_eq!(encoder.encode_raw(&[], &mut []), Ok(0));
        let previous = encoder.clone();
        let mut output = [0xa5; 4];
        assert_eq!(
            encoder.encode_raw(&[0], &mut output),
            Err(ConvError::InvalidState)
        );
        assert_eq!(encoder.flush(&mut output), Err(ConvError::InvalidState));
        assert_eq!(encoder, previous);
        assert_eq!(output, [0xa5; 4]);
        assert_eq!(encoder.load_state(&[1]), Ok(()));
        assert_eq!(encoder.encode_raw(&[0], &mut output), Ok(1));
    }

    #[test]
    fn c_named_adapters_check_prefixes_and_preserve_signed_no_ops() {
        let mut spec = code(1, None).spec().clone();
        spec.k = 2;
        spec.next_output = vec![[0, 1]; 2];
        spec.next_state = vec![[0, 1]; 2];
        let code = ConvCode::new(spec).unwrap();
        let mut encoder = ConvEncoder::new(&code);
        assert_eq!((encoder).load_state(&[1, 255]), Ok(()));
        let previous = encoder.clone();
        let mut output = [0xa5; 4];
        for count in [i32::MIN, -1, 0] {
            assert_eq!(
                osmo_conv_encode_raw(&mut encoder, &[255], &mut output, count),
                Ok(0)
            );
            assert_eq!(encoder, previous);
            assert_eq!(output, [0xa5; 4]);
        }
        assert_eq!(
            osmo_conv_encode_raw(&mut encoder, &[1], &mut output, 2),
            Err(ConvError::InputTooShort)
        );
        assert_eq!(encoder, previous);
        assert_eq!(output, [0xa5; 4]);
        assert_eq!(
            osmo_conv_encode_raw(&mut encoder, &[1, 255], &mut output, 1),
            Ok(1)
        );
        assert_eq!(output, [1, 0xa5, 0xa5, 0xa5]);
        assert_eq!((encoder).flush(&mut output[1..]), Ok(1));
        assert_eq!(output, [1, 0, 0xa5, 0xa5]);
        assert_eq!(encoder.input_index(), 2);
        assert_eq!(encoder.state(), 0);
    }
}
