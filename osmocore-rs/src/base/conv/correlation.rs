// SPDX-License-Identifier: GPL-3.0-or-later

use super::{ConvCode, ConvDecoder, ConvError, Termination, decoder::filled};

/// Portable correlation decoder using the approved scalar/SSE/AVX tie rule.
/// Success is zero, not an accumulated error metric. Unsupported dimensions
/// and nonsystematic recursion are checked errors; output is atomic on error.
/// This does not select SIMD and does not claim NEON tie compatibility.
pub fn osmo_conv_decode_acc(
    code: &ConvCode,
    input: &[i8],
    output: &mut [u8],
) -> Result<i32, ConvError> {
    if !(2..=4).contains(&code.spec.n) {
        return Err(ConvError::InvalidRate);
    }
    if ![5, 7].contains(&code.spec.k) {
        return Err(ConvError::InvalidConstraintLength);
    }
    if code.spec.len < 1 {
        return Err(ConvError::InvalidFrameLength);
    }
    let (symbols, raw, transmitted) = input_sizes(code)?;
    check_buffers(code, input, output, transmitted)?;
    let mut decoder = CorrelationDecoder::new(code, symbols)?;
    let sequence = depuncture(code, input, raw)?;
    decoder.traverse(&sequence);
    if code.spec.term == Termination::TAIL_BITING {
        decoder.traverse(&sequence);
    }
    let decoded = decoder.traceback(code.spec.term, code.spec.len as usize)?;
    output[..decoded.len()].copy_from_slice(&decoded);
    Ok(0)
}

/// Public one-shot C dispatch policy. K=5/7 with N<=4 routes to correlation,
/// including N=1 which is rejected there without falling back. Other codes
/// use the generic fixed-zero-start decoder and return its accumulated error.
pub fn decode(code: &ConvCode, input: &[i8], output: &mut [u8]) -> Result<i32, ConvError> {
    if code.spec.n <= 4 && [5, 7].contains(&code.spec.k) {
        return osmo_conv_decode_acc(code, input, output);
    }
    let (_, _, transmitted) = input_sizes(code)?;
    check_buffers(code, input, output, transmitted)?;
    let mut decoder = ConvDecoder::new(code, 0, 0)?;
    let len = code.spec.len as usize;
    if code.spec.term == Termination::TAIL_BITING {
        decoder.scan(input, len)?;
        decoder.rewind()?;
    }
    let consumed = decoder.scan(input, len)?;
    let flush = code.spec.term == Termination::FLUSH;
    if flush {
        decoder.flush(&input[consumed..])?;
    }
    let metric = decoder.get_output(output, flush, -1)?;
    i32::try_from(metric).map_err(|_| ConvError::ArithmeticOverflow)
}

fn check_buffers(
    code: &ConvCode,
    input: &[i8],
    output: &[u8],
    transmitted: usize,
) -> Result<(), ConvError> {
    if input.len() < transmitted {
        return Err(ConvError::InputTooShort);
    }
    if output.len() < code.spec.len as usize {
        return Err(ConvError::OutputTooShort);
    }
    Ok(())
}

pub(super) fn input_sizes(code: &ConvCode) -> Result<(usize, usize, usize), ConvError> {
    let symbols = code
        .spec
        .len
        .checked_add(if code.spec.term == Termination::FLUSH {
            code.spec.k - 1
        } else {
            0
        })
        .ok_or(ConvError::ArithmeticOverflow)?;
    let raw = symbols
        .checked_mul(code.spec.n)
        .ok_or(ConvError::ArithmeticOverflow)? as usize;
    let punctures = code.spec.puncture.as_ref().map_or(0, |p| {
        p.partition_point(|&position| (position as usize) < raw)
    });
    Ok((symbols as usize, raw, raw - punctures))
}

fn depuncture(code: &ConvCode, input: &[i8], raw: usize) -> Result<Vec<i8>, ConvError> {
    let mut sequence = filled(raw, 0)?;
    let mut cursor = 0;
    let mut consumed = 0;
    for (position, sample) in sequence.iter_mut().enumerate() {
        if code
            .spec
            .puncture
            .as_ref()
            .and_then(|p| p.get(cursor))
            .is_some_and(|&p| p as usize == position)
        {
            cursor += 1;
        } else {
            *sample = input[consumed];
            consumed += 1;
        }
    }
    Ok(sequence)
}

/// Traceback follows an incoming transition selected by the stored path bit.
#[derive(Clone, Copy, Default)]
struct Transition {
    predecessor: usize,
    input: u8,
}

#[derive(Clone, Copy, Default)]
struct TrellisState {
    incoming: [Transition; 2],
    symbol: u8,
}

impl TrellisState {
    fn new(code: &ConvCode, state: usize, systematic: Option<usize>) -> Self {
        let k = code.spec.k as usize;
        let predecessor = (state << 1) & (code.state_count() - 2);
        let source = reverse_bits(predecessor, k - 1);
        let high = ((state >> (k - 2)) & 1) as u8;
        let input = high
            ^ systematic.map_or(0, |bit| {
                (code.spec.next_term_output.as_ref().unwrap()[source] >> bit) & 1
            });
        let symbol = code.spec.next_output[source][usize::from(input)];
        Self {
            incoming: std::array::from_fn(|path| Transition {
                predecessor: predecessor | path,
                input: input ^ if systematic.is_some() { path as u8 } else { 0 },
            }),
            symbol,
        }
    }
}

struct CorrelationDecoder {
    rate: usize,
    states: usize,
    symbols: usize,
    interval: usize,
    trellis: [TrellisState; 64],
    /// The sign each output bit contributes to a state's metric, by bit and then
    /// state. Only the lower half of the states needs one: the upper half uses the
    /// same metric negated. Separate rows of plain integers let the compiler
    /// vectorize the metric calculation.
    signs: [[i16; 32]; 4],
    sums: [i16; 64],
    paths: Vec<u8>,
}

impl CorrelationDecoder {
    fn new(code: &ConvCode, symbols: usize) -> Result<Self, ConvError> {
        let rate = code.spec.n as usize;
        let k = code.spec.k as usize;
        let states = code.state_count();
        let cells = symbols
            .checked_mul(states)
            .filter(|&cells| cells <= i32::MAX as usize)
            .ok_or(ConvError::ArithmeticOverflow)?;
        let systematic = if code.spec.next_term_output.is_some() {
            Some(
                (0..rate)
                    .find(|&bit| {
                        code.spec
                            .next_output
                            .iter()
                            .all(|row| row[0] & (1 << bit) == 0)
                    })
                    .ok_or(ConvError::UnsupportedRecursiveCode)?,
            )
        } else {
            None
        };
        // Construct states and incoming paths in ascending order. Stored path
        // indices keep their existing tie-resolution meaning.
        let trellis: [TrellisState; 64] = std::array::from_fn(|state| {
            if state < states {
                TrellisState::new(code, state, systematic)
            } else {
                TrellisState::default()
            }
        });
        let signs = std::array::from_fn(|bit| {
            std::array::from_fn(|state| {
                if bit >= rate || state >= states / 2 {
                    0
                } else if trellis[state].symbol & (1 << (rate - bit - 1)) == 0 {
                    1
                } else {
                    -1
                }
            })
        });
        let mut decoder = Self {
            rate,
            states,
            symbols,
            interval: i16::MAX as usize / (rate * i8::MAX as usize) - k,
            trellis,
            signs,
            sums: [0; 64],
            paths: filled(cells, 0)?,
        };
        if code.spec.term != Termination::TAIL_BITING {
            decoder.sums[0] = (i8::MAX as usize * rate * k) as i16;
        }
        Ok(decoder)
    }

    fn transition(&self, state: usize, path: u8) -> &Transition {
        &self.trellis[state].incoming[usize::from(path)]
    }

    /// Metrics saturate like the C SSE/AVX backends. The C scalar backend wraps
    /// instead. They differ only on -128, which is outside libosmocore's documented
    /// soft-bit range but accepted here, so -128 follows SSE/AVX.
    fn traverse(&mut self, sequence: &[i8]) {
        // `osmo_conv_decode_acc` accepts only K=5 and K=7.
        match self.states {
            16 => self.traverse_rows::<8>(sequence),
            64 => self.traverse_rows::<32>(sequence),
            states => unreachable!("no correlation decoder has {states} states"),
        }
    }

    /// A fixed state count, with fixed-size arrays and one loop per step, is what
    /// lets the compiler vectorize this. `docs/benchmarks.md` records the gain.
    fn traverse_rows<const HALF: usize>(&mut self, sequence: &[i8]) {
        for (row, sample) in sequence.chunks_exact(self.rate).enumerate() {
            let mut metric = [0i16; HALF];
            for (&soft, signs) in sample.iter().zip(&self.signs) {
                for (metric, &sign) in metric.iter_mut().zip(signs) {
                    // At most 4 * 128 in magnitude, so this sum can't overflow.
                    *metric += i16::from(soft) * sign;
                }
            }
            let mut even = [0i16; HALF];
            let mut odd = [0i16; HALF];
            let (pairs, _) = self.sums.as_chunks::<2>();
            for ((even, odd), pair) in even.iter_mut().zip(&mut odd).zip(pairs) {
                [*even, *odd] = *pair;
            }

            let (lower_sums, upper_sums) = self.sums.split_at_mut(HALF);
            let paths = &mut self.paths[row * self.states..][..self.states];
            let (lower_paths, upper_paths) = paths.split_at_mut(HALF);
            add_compare_select(&even, &odd, &metric, lower_sums, lower_paths);
            metric = metric.map(|metric| -metric);
            add_compare_select(&even, &odd, &metric, upper_sums, upper_paths);

            if row % self.interval == 0 {
                let sums = &mut self.sums[..self.states];
                let minimum = sums.iter().copied().min().unwrap_or(0);
                for sum in sums {
                    *sum = sum.saturating_sub(minimum);
                }
            }
        }
    }

    fn traceback(&self, term: Termination, len: usize) -> Result<Vec<u8>, ConvError> {
        let mut maximum = -1;
        let mut state = 0;
        if term == Termination::TAIL_BITING {
            for candidate in 0..self.states {
                let mut traced = candidate;
                for row in (0..len).rev() {
                    traced = self
                        .transition(traced, self.paths[row * self.states + traced])
                        .predecessor;
                }
                if traced == candidate && i32::from(self.sums[candidate]) > maximum {
                    maximum = i32::from(self.sums[candidate]);
                    state = candidate;
                }
            }
        }
        if maximum < 0 && term != Termination::FLUSH {
            for candidate in 0..self.states {
                if i32::from(self.sums[candidate]) > maximum {
                    maximum = i32::from(self.sums[candidate]);
                    state = candidate;
                }
            }
            if maximum < 0 {
                return Err(ConvError::NoSurvivor);
            }
        }
        for row in (len..self.symbols).rev() {
            state = self
                .transition(state, self.paths[row * self.states + state])
                .predecessor;
        }
        let mut decoded = filled(len, 0)?;
        for row in (0..len).rev() {
            let path = self.paths[row * self.states + state];
            let transition = self.transition(state, path);
            decoded[row] = transition.input;
            state = transition.predecessor;
        }
        Ok(decoded)
    }
}

fn add_compare_select(
    even: &[i16],
    odd: &[i16],
    metric: &[i16],
    sums: &mut [i16],
    paths: &mut [u8],
) {
    let inputs = even.iter().zip(odd).zip(metric);
    for (((&even, &odd), &metric), (sum, path)) in inputs.zip(sums.iter_mut().zip(paths)) {
        let left = even.saturating_add(metric);
        let right = odd.saturating_sub(metric);
        // Equality selects predecessor zero in scalar/SSE/AVX.
        *sum = left.max(right);
        *path = u8::from(left < right);
    }
}

fn reverse_bits(value: usize, bits: usize) -> usize {
    value.reverse_bits() >> (usize::BITS as usize - bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::conv::ConvCodeSpec;
    use crate::compat::conv::osmo_conv_get_output_length;

    fn code(n: i32, k: i32, len: i32, term: Termination) -> ConvCode {
        let states = 1usize << (k - 1);
        let mask = ((1u32 << n) - 1) as u8;
        ConvCode::new(ConvCodeSpec {
            n,
            k,
            len,
            term,
            next_output: vec![[0, mask]; states],
            next_state: (0..states)
                .map(|state| {
                    [
                        ((state << 1) & (states - 1)) as u8,
                        (((state << 1) | 1) & (states - 1)) as u8,
                    ]
                })
                .collect(),
            next_term_output: None,
            next_term_state: None,
            puncture: None,
        })
        .unwrap()
    }

    #[test]
    fn initialization_and_normalization_intervals_match_c() {
        for k in [5, 7] {
            for n in 2..=4 {
                for term in [
                    Termination::FLUSH,
                    Termination::TRUNCATION,
                    Termination::TAIL_BITING,
                    Termination(42),
                ] {
                    let code = code(n, k, 1, term);
                    let decoder = CorrelationDecoder::new(&code, 1).unwrap();
                    assert_eq!(decoder.interval, 32767 / (n as usize * 127) - k as usize);
                    assert_eq!(
                        decoder.sums[0],
                        if term == Termination::TAIL_BITING {
                            0
                        } else {
                            (127 * n * k) as i16
                        }
                    );
                    assert!(decoder.sums[1..decoder.states].iter().all(|&sum| sum == 0));
                }
            }
        }
    }

    #[test]
    fn erasure_ties_select_even_predecessors_and_negative_128_is_not_clamped() {
        let code = code(2, 5, 1, Termination::TAIL_BITING);
        let mut decoder = CorrelationDecoder::new(&code, 1).unwrap();
        decoder.traverse(&[0, 0]);
        assert_eq!(decoder.paths, vec![0; 16]);
        assert_eq!(&decoder.sums[..16], &[0; 16]);
        decoder.traverse(&[-128, -128]);
        assert_eq!(&decoder.paths[..8], &[1; 8]);
        assert_eq!(&decoder.paths[8..], &[0; 8]);
        assert_eq!(&decoder.sums[..16], &[0; 16]);
    }

    #[test]
    fn normalization_runs_at_zero_and_then_at_the_exact_interval() {
        let code = code(2, 5, 1, Termination::TAIL_BITING);
        let interval = CorrelationDecoder::new(&code, 1).unwrap().interval;
        for rows in [1, interval, interval + 1, interval + 2, 2 * interval + 1] {
            let mut decoder = CorrelationDecoder::new(&code, rows).unwrap();
            decoder.traverse(&vec![127; rows * 2]);
            assert!(
                decoder.sums[..16]
                    .iter()
                    .all(|&sum| sum == (254 * ((rows - 1) % interval)) as i16)
            );
        }
    }

    #[test]
    fn metrics_and_normalization_saturate() {
        let code = code(2, 5, 1, Termination::TRUNCATION);
        let mut decoder = CorrelationDecoder::new(&code, 1).unwrap();
        decoder.sums[..16].fill(i16::MAX);
        decoder.traverse(&[127; 2]);
        assert_eq!(&decoder.sums[..16], &[0; 16]);
        decoder.sums[..16].fill(i16::MIN);
        decoder.sums[0] = i16::MAX;
        decoder.traverse(&[0; 2]);
        assert_eq!(decoder.sums[0], i16::MAX);
        assert_eq!(decoder.sums[8], i16::MAX);
        assert_eq!(decoder.sums[1], 0);
    }

    #[test]
    fn tail_biting_prefers_closure_then_falls_back_and_flush_forces_zero() {
        let code = code(2, 5, 1, Termination::TAIL_BITING);
        let mut decoder = CorrelationDecoder::new(&code, 1).unwrap();
        decoder.sums[15] = 100;
        // All-zero paths close only at zero; closure outranks a larger metric.
        assert_eq!(decoder.traceback(Termination::TAIL_BITING, 1), Ok(vec![0]));
        assert_eq!(decoder.traceback(Termination::TRUNCATION, 1), Ok(vec![1]));
        assert_eq!(decoder.traceback(Termination::FLUSH, 1), Ok(vec![0]));
        // Exclude both one-step closing states (zero and fifteen).
        decoder.paths[0] = 1;
        assert_eq!(decoder.traceback(Termination::TAIL_BITING, 1), Ok(vec![1]));
        decoder.sums[..16].fill(-1);
        assert_eq!(
            decoder.traceback(Termination::TAIL_BITING, 1),
            Err(ConvError::NoSurvivor)
        );
        assert_eq!(
            decoder.traceback(Termination::TRUNCATION, 1),
            Err(ConvError::NoSurvivor)
        );
    }

    #[test]
    fn dispatch_rejections_do_not_fall_back_and_preserve_output() {
        for k in [5, 7] {
            let code = code(1, k, 1, Termination::TRUNCATION);
            let mut output = [0xa5; 5];
            assert_eq!(
                decode(&code, &[127], &mut output),
                Err(ConvError::InvalidRate)
            );
            assert_eq!(
                osmo_conv_decode_acc(&code, &[127], &mut output),
                Err(ConvError::InvalidRate)
            );
            assert_eq!(output, [0xa5; 5]);
            let code = self::code(2, k, 0, Termination::FLUSH);
            assert_eq!(
                decode(&code, &[], &mut output),
                Err(ConvError::InvalidFrameLength)
            );
            assert_eq!(output, [0xa5; 5]);
        }
        let code = code(2, 3, 1, Termination::TRUNCATION);
        let mut output = [0xa5; 5];
        assert_eq!(
            osmo_conv_decode_acc(&code, &[0; 2], &mut output),
            Err(ConvError::InvalidConstraintLength)
        );
        assert_eq!(decode(&code, &[0; 2], &mut output), Ok(0));
        assert_eq!(output, [0, 0xa5, 0xa5, 0xa5, 0xa5]);
    }

    #[test]
    fn short_buffers_are_atomic_for_both_dispatch_branches() {
        for k in [3, 5] {
            let code = code(2, k, 3, Termination::FLUSH);
            let capacity = code.decode_input_capacity().unwrap();
            let mut output = [0xa5; 7];
            assert_eq!(
                decode(&code, &vec![127; capacity - 1], &mut output),
                Err(ConvError::InputTooShort)
            );
            assert_eq!(output, [0xa5; 7]);
            assert_eq!(
                decode(&code, &vec![127; capacity], &mut output[..2]),
                Err(ConvError::OutputTooShort)
            );
            assert_eq!(output, [0xa5; 7]);
            assert_eq!(decode(&code, &vec![127; capacity + 4], &mut output), Ok(0));
            assert_eq!(output, [0, 0, 0, 0xa5, 0xa5, 0xa5, 0xa5]);
        }
    }

    #[test]
    fn unsupported_recursion_is_atomic_in_direct_and_dispatch_calls() {
        let mut spec = code(2, 5, 3, Termination::FLUSH).spec().clone();
        spec.next_output.fill([3, 0]);
        spec.next_term_output = Some(vec![3; 16]);
        spec.next_term_state = Some(vec![0; 16]);
        let code = ConvCode::new(spec).unwrap();
        let input = vec![0; code.decode_input_capacity().unwrap()];
        let mut output = [0xa5; 7];
        assert_eq!(
            osmo_conv_decode_acc(&code, &input, &mut output),
            Err(ConvError::UnsupportedRecursiveCode)
        );
        assert_eq!(
            decode(&code, &input, &mut output),
            Err(ConvError::UnsupportedRecursiveCode)
        );
        assert_eq!(output, [0xa5; 7]);
    }

    #[test]
    fn capacity_counts_only_encountered_punctures_and_all_puncturing_is_safe() {
        let mut spec = code(2, 5, 1, Termination::TAIL_BITING).spec().clone();
        spec.puncture = Some(vec![0, 1, 2, 3, 100]);
        let code = ConvCode::new(spec).unwrap();
        assert_eq!(osmo_conv_get_output_length(&code, 0), Ok(-3));
        assert_eq!(code.decode_input_capacity(), Ok(0));
        assert_eq!(code.output_capacity(), Err(ConvError::TailBitingTooShort));
        let mut output = [0xa5; 5];
        assert_eq!(osmo_conv_decode_acc(&code, &[], &mut output), Ok(0));
        assert_eq!(output, [0, 0xa5, 0xa5, 0xa5, 0xa5]);
    }

    #[test]
    fn signed_sizing_and_history_product_overflow_are_checked() {
        let code = code(4, 7, i32::MAX, Termination::FLUSH);
        let mut output = [0xa5; 5];
        assert_eq!(
            code.decode_input_capacity(),
            Err(ConvError::ArithmeticOverflow)
        );
        assert_eq!(
            decode(&code, &[], &mut output),
            Err(ConvError::ArithmeticOverflow)
        );
        assert_eq!(output, [0xa5; 5]);
        let code = self::code(2, 7, 1, Termination::TRUNCATION);
        assert!(matches!(
            CorrelationDecoder::new(&code, i32::MAX as usize / 64 + 1),
            Err(ConvError::ArithmeticOverflow)
        ));
    }

    #[test]
    fn recursive_trellis_uses_first_systematic_bit_and_path_xor() {
        let mut spec = code(2, 5, 1, Termination::TRUNCATION).spec().clone();
        // Both output bits qualify; C selects the lowest bit, not bit one.
        spec.next_term_output = Some(vec![1; 16]);
        spec.next_term_state = Some(vec![0; 16]);
        let code = ConvCode::new(spec).unwrap();
        let mut decoder = CorrelationDecoder::new(&code, 1).unwrap();
        for state in 0..16 {
            let input = u8::from(state < 8);
            assert_eq!(decoder.transition(state, 0).input, input);
            assert_eq!(decoder.transition(state, 1).input, input ^ 1);
            for path in 0..2 {
                assert_eq!(
                    decoder.transition(state, path).predecessor,
                    ((state << 1) & 14) | usize::from(path)
                );
            }
        }
        for (bit, sign) in [(0, -1), (1, -1), (2, 0), (3, 0)] {
            assert!(decoder.signs[bit][..8].iter().all(|&s| s == sign));
        }
        assert_eq!(decoder.traceback(Termination::FLUSH, 1), Ok(vec![1]));
        decoder.paths[0] = 1;
        assert_eq!(decoder.traceback(Termination::FLUSH, 1), Ok(vec![0]));
    }

    /// The add-compare-select step written one state at a time, for comparison with
    /// the vectorized loop.
    fn traverse_one_state_at_a_time(decoder: &mut CorrelationDecoder, sequence: &[i8]) {
        let half = decoder.states / 2;
        for (row, sample) in sequence.chunks_exact(decoder.rate).enumerate() {
            let mut next = [0i16; 64];
            for state in 0..half {
                let metric: i16 = sample
                    .iter()
                    .enumerate()
                    .map(|(bit, &soft)| {
                        let negative =
                            decoder.trellis[state].symbol & (1 << (decoder.rate - bit - 1)) != 0;
                        i16::from(soft) * if negative { -1 } else { 1 }
                    })
                    .sum();
                let even = decoder.sums[2 * state];
                let odd = decoder.sums[2 * state + 1];
                for (target, left, right) in [
                    (
                        state,
                        even.saturating_add(metric),
                        odd.saturating_sub(metric),
                    ),
                    (
                        state + half,
                        even.saturating_sub(metric),
                        odd.saturating_add(metric),
                    ),
                ] {
                    let path = u8::from(left < right);
                    next[target] = if path == 0 { left } else { right };
                    decoder.paths[row * decoder.states + target] = path;
                }
            }
            if row % decoder.interval == 0 {
                let minimum = *next[..decoder.states].iter().min().unwrap();
                for sum in &mut next[..decoder.states] {
                    *sum = sum.saturating_sub(minimum);
                }
            }
            decoder.sums = next;
        }
    }

    #[test]
    fn vectorized_traversal_matches_one_state_at_a_time() {
        let mut random = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random
        };
        for n in 2..=4 {
            for k in [5, 7] {
                for term in [Termination::FLUSH, Termination::TAIL_BITING] {
                    let mut spec = code(n, k, 300, term).spec().clone();
                    for row in &mut spec.next_output {
                        *row = [0, 1].map(|_| (next() & ((1 << n) - 1)) as u8);
                    }
                    let code = ConvCode::new(spec).unwrap();
                    let (symbols, raw, _) = input_sizes(&code).unwrap();
                    for choices in [
                        &[-128, 127][..],
                        &[-127, 127],
                        &[-127, 0, 127],
                        &[0],
                        &[1, 2, 3, -1, -2, -3],
                    ] {
                        let sequence: Vec<i8> = (0..raw)
                            .map(|_| choices[next() as usize % choices.len()])
                            .collect();
                        let mut actual = CorrelationDecoder::new(&code, symbols).unwrap();
                        let mut expected = CorrelationDecoder::new(&code, symbols).unwrap();
                        // A second pass starts from nonzero sums, as tail biting does.
                        for _ in 0..2 {
                            actual.traverse(&sequence);
                            traverse_one_state_at_a_time(&mut expected, &sequence);
                            assert_eq!(actual.sums, expected.sums);
                            assert_eq!(actual.paths, expected.paths);
                        }
                    }
                    let sequence: Vec<i8> = (0..raw).map(|_| next() as u8 as i8).collect();
                    let mut actual = CorrelationDecoder::new(&code, symbols).unwrap();
                    let mut expected = CorrelationDecoder::new(&code, symbols).unwrap();
                    actual.traverse(&sequence);
                    traverse_one_state_at_a_time(&mut expected, &sequence);
                    assert_eq!(actual.sums, expected.sums);
                    assert_eq!(actual.paths, expected.paths);
                }
            }
        }
    }

    #[test]
    fn second_tail_biting_pass_retains_sums_and_replaces_paths() {
        let code = code(2, 5, 1, Termination::TAIL_BITING);
        let mut decoder = CorrelationDecoder::new(&code, 1).unwrap();
        decoder.sums[1] = 100;
        decoder.traverse(&[0; 2]);
        assert_eq!(decoder.sums[0], 100);
        assert_eq!(decoder.sums[8], 100);
        assert_eq!(decoder.paths[0], 1);
        decoder.traverse(&[0; 2]);
        assert_eq!(decoder.sums[0], 100);
        assert_eq!(decoder.sums[4], 100);
        assert_eq!(decoder.paths[0], 0);
        assert_eq!(decoder.paths[4], 0);
    }

    #[test]
    fn correlation_success_is_not_the_generic_accumulated_error() {
        let code = code(2, 5, 1, Termination::TRUNCATION);
        let mut output = [0xa5; 5];
        let mut generic = ConvDecoder::new(&code, 0, 0).unwrap();
        generic.scan(&[64; 2], 1).unwrap();
        assert_eq!(generic.get_output(&mut output, false, -1), Ok(14));
        assert_eq!(osmo_conv_decode_acc(&code, &[64; 2], &mut output), Ok(0));
        assert_eq!(decode(&code, &[64; 2], &mut output), Ok(0));
        assert_eq!(output, [0, 0xa5, 0xa5, 0xa5, 0xa5]);
        let code = self::code(5, 5, 1, Termination::TRUNCATION);
        assert_eq!(decode(&code, &[64; 5], &mut output), Ok(35));
        assert_eq!(output, [0, 0xa5, 0xa5, 0xa5, 0xa5]);
    }
}
