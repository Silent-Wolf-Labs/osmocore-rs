// SPDX-License-Identifier: GPL-3.0-or-later

use super::{ConvCode, ConvError, Termination};
use crate::compat::conv::osmo_conv_get_input_length;

const MAX_AE: u32 = 0x00ff_ffff;

/// Owned generic squared-error decoder borrowing an immutable checked code.
/// This is not the correlation backend. Fallible operations leave state and
/// output unchanged. Reset/rewind invalidate the active history span, not the
/// retained scratch errors; rewind also retains and normalizes path errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvDecoder<'a> {
    code: &'a ConvCode,
    len: usize,
    n_states: usize,
    o_idx: usize,
    p_idx: usize,
    ae: Vec<u32>,
    ae_next: Vec<u32>,
    ae_next_valid: bool,
    history: Vec<Option<u8>>,
}

/// One depunctured trellis sample. Zero retains the legacy erasure meaning.
struct StepInput {
    soft: [i8; 31],
    rate: usize,
}

impl StepInput {
    fn read<'a>(
        code: &ConvCode,
        position: usize,
        transmitted: &mut impl Iterator<Item = &'a i8>,
        puncture: &mut usize,
    ) -> Self {
        let rate = code.spec.n as usize;
        let mut sample = Self {
            soft: [0; 31],
            rate,
        };
        for (bit, soft) in sample.soft[..rate].iter_mut().enumerate() {
            let absolute = position * rate + bit;
            if code
                .spec
                .puncture
                .as_ref()
                .and_then(|p| p.get(*puncture))
                .is_some_and(|&p| p as usize == absolute)
            {
                *puncture += 1;
            } else {
                *soft = *transmitted.next().expect("input capacity was checked");
            }
        }
        sample
    }
}

/// Owned next-step metrics; predecessor rows belong to the staged operation.
struct StepSurvivors {
    metrics: Vec<u32>,
}

impl StepSurvivors {
    fn compute(
        &mut self,
        code: &ConvCode,
        current: &[u32],
        sample: &StepInput,
        flush: bool,
        predecessors: &mut [Option<u8>],
    ) -> Result<(), ConvError> {
        self.metrics.fill(MAX_AE);
        let candidates = current.iter().enumerate().flat_map(|(state, &error)| {
            (0..if flush { 1 } else { 2 }).map(move |bit| {
                let (symbol, target) =
                    match (&code.spec.next_term_output, &code.spec.next_term_state) {
                        (Some(output), Some(target)) if flush => (output[state], target[state]),
                        _ => (
                            code.spec.next_output[state][bit],
                            code.spec.next_state[state][bit],
                        ),
                    };
                (state, error, symbol, usize::from(target))
            })
        });
        // Ordered candidates and strict improvement preserve first-winner ties.
        for (state, error, symbol, target) in candidates {
            let metric = branch_error(error, symbol, &sample.soft[..sample.rate])?;
            if metric < self.metrics[target] {
                self.metrics[target] = metric;
                // The compatibility history deliberately narrows to one byte.
                predecessors[target] = Some(state as u8);
            }
        }
        Ok(())
    }
}

impl<'a> ConvDecoder<'a> {
    /// Positive length overrides the descriptor; any negative start selector
    /// enables all states. The frozen `state_count()` selector disables all
    /// survivors; larger nonnegative selectors are checked errors.
    pub fn new(code: &'a ConvCode, len: i32, start_state: i32) -> Result<Self, ConvError> {
        // C initializes a byte mask from a positive signed-int left shift.
        if code.spec.n > 31 {
            return Err(ConvError::InvalidRate);
        }
        let len = osmo_conv_get_input_length(code, len)? as usize;
        let n_states = code.state_count();
        validate_start_state(n_states, start_state)?;
        let cells = history_cells(code, len)?;
        let mut decoder = Self {
            code,
            len,
            n_states,
            o_idx: 0,
            p_idx: 0,
            ae: filled(n_states, MAX_AE)?,
            ae_next: filled(n_states, 0)?,
            ae_next_valid: false,
            history: filled(cells, None)?,
        };
        decoder.initialize_errors(start_state);
        Ok(decoder)
    }

    pub fn frame_length(&self) -> usize {
        self.len
    }

    pub fn state_count(&self) -> usize {
        self.n_states
    }

    pub(crate) fn output_index(&self) -> usize {
        self.o_idx
    }

    pub(crate) fn puncture_index(&self) -> usize {
        self.p_idx
    }

    pub(crate) fn accumulated_errors(&self) -> &[u32] {
        &self.ae
    }

    /// Initialized diagnostic scratch values, not additional active paths.
    pub(crate) fn next_accumulated_errors(&self) -> Option<&[u32]> {
        self.ae_next_valid.then_some(self.ae_next.as_slice())
    }

    /// Active rows, state-major within each trellis position. `None` denotes
    /// a cell for which no strict improvement initialized a predecessor.
    pub(crate) fn predecessor_history(&self) -> &[Option<u8>] {
        &self.history[..self.o_idx * self.n_states]
    }

    pub fn reset(&mut self, start_state: i32) -> Result<(), ConvError> {
        self.validate()?;
        validate_start_state(self.n_states, start_state)?;
        self.initialize_errors(start_state);
        self.o_idx = 0;
        self.p_idx = 0;
        Ok(())
    }

    fn initialize_errors(&mut self, start_state: i32) {
        for (state, error) in self.ae.iter_mut().enumerate() {
            *error = if start_state < 0 || state == start_state as usize {
                0
            } else {
                MAX_AE
            };
        }
    }

    pub fn rewind(&mut self) -> Result<(), ConvError> {
        self.validate()?;
        let minimum = self.ae.iter().copied().min().unwrap();
        for error in &mut self.ae {
            *error -= minimum;
        }
        self.o_idx = 0;
        self.p_idx = 0;
        Ok(())
    }

    fn validate(&self) -> Result<(), ConvError> {
        if self.code.spec.n > 31
            || self.n_states != self.code.state_count()
            || self.ae.len() != self.n_states
            || self.ae_next.len() != self.n_states
            || self.history.len() != history_cells(self.code, self.len)?
            || self.o_idx > self.len + (self.code.spec.k - 1) as usize
        {
            return Err(ConvError::InvalidDecoderState);
        }
        if self.ae.iter().any(|&error| error > MAX_AE)
            || (self.ae_next_valid && self.ae_next.iter().any(|&error| error > MAX_AE))
        {
            return Err(ConvError::ArithmeticOverflow);
        }
        let position = self
            .o_idx
            .checked_mul(self.code.spec.n as usize)
            .ok_or(ConvError::ArithmeticOverflow)?;
        let expected_cursor = self.code.spec.puncture.as_ref().map_or(0, |punctures| {
            punctures.partition_point(|&p| (p as usize) < position)
        });
        if self.p_idx != expected_cursor {
            return Err(ConvError::InvalidDecoderState);
        }
        Ok(())
    }

    /// Exact transmitted soft-bit count at the current absolute position.
    pub fn scan_input_capacity(&self, symbols: usize) -> Result<usize, ConvError> {
        self.validate()?;
        i32::try_from(symbols).map_err(|_| ConvError::ArithmeticOverflow)?;
        let end = self
            .o_idx
            .checked_add(symbols)
            .filter(|&end| end <= i32::MAX as usize)
            .ok_or(ConvError::ArithmeticOverflow)?;
        if end > self.len + (self.code.spec.k - 1) as usize {
            return Err(ConvError::InvalidOperation);
        }
        let rate = self.code.spec.n as usize;
        let raw = symbols
            .checked_mul(rate)
            .ok_or(ConvError::ArithmeticOverflow)?;
        let removed = if let Some(punctures) = &self.code.spec.puncture {
            if symbols == 0 {
                0
            } else {
                let endpoint = end
                    .checked_mul(rate)
                    .filter(|&endpoint| endpoint - 1 <= i32::MAX as usize)
                    .ok_or(ConvError::ArithmeticOverflow)?;
                punctures[self.p_idx..].partition_point(|&p| (p as usize) < endpoint)
            }
        } else {
            0
        };
        let consumed = raw
            .checked_sub(removed)
            .ok_or(ConvError::InvalidDecoderState)?;
        i32::try_from(consumed).map_err(|_| ConvError::ArithmeticOverflow)?;
        Ok(consumed)
    }

    pub fn flush_input_capacity(&self) -> Result<usize, ConvError> {
        self.scan_input_capacity((self.code.spec.k - 1) as usize)
    }

    /// Scan exactly `symbols` trellis steps, ignoring unused input suffixes.
    pub fn scan(&mut self, input: &[i8], symbols: usize) -> Result<usize, ConvError> {
        self.advance(input, symbols, false)
    }

    /// Always scan `K-1` termination steps, independent of one-shot policy.
    pub fn flush(&mut self, input: &[i8]) -> Result<usize, ConvError> {
        self.advance(input, (self.code.spec.k - 1) as usize, true)
    }

    fn advance(&mut self, input: &[i8], symbols: usize, flush: bool) -> Result<usize, ConvError> {
        let consumed = self.scan_input_capacity(symbols)?;
        let input = input.get(..consumed).ok_or(ConvError::InputTooShort)?;
        if symbols == 0 {
            return Ok(0);
        }
        // Stage the entire operation, so even metric/allocation failures cannot
        // leave a partial chunk or initialized history behind.
        let mut errors = filled(self.n_states, 0)?;
        errors.copy_from_slice(&self.ae);
        let mut survivors = StepSurvivors {
            metrics: filled(self.n_states, MAX_AE)?,
        };
        let mut history = filled(symbols * self.n_states, None)?;
        let mut puncture = self.p_idx;
        let mut transmitted = input.iter();
        for (step, predecessors) in history.chunks_exact_mut(self.n_states).enumerate() {
            let sample = StepInput::read(
                self.code,
                self.o_idx + step,
                &mut transmitted,
                &mut puncture,
            );
            survivors.compute(self.code, &errors, &sample, flush, predecessors)?;
            std::mem::swap(&mut errors, &mut survivors.metrics);
        }
        let start = self.o_idx * self.n_states;
        self.history[start..start + history.len()].copy_from_slice(&history);
        // Both public metric views describe the final step, not the old buffer
        // left in `survivors` by rotation. No fallible work follows this commit.
        self.ae_next.copy_from_slice(&errors);
        self.ae = errors;
        self.ae_next_valid = true;
        self.o_idx += symbols;
        self.p_idx = puncture;
        Ok(consumed)
    }

    /// First minimum wins. Tail biting prefers a closing traceback, then the
    /// lowest metric. C's `-1` return maps to `NoSurvivor`; flush selects zero.
    pub fn best_end_state(&self) -> Result<usize, ConvError> {
        self.validate()?;
        if self.code.spec.term == Termination::FLUSH {
            return Ok(0);
        }
        if self.code.spec.term == Termination::TAIL_BITING {
            if self.o_idx == 0 {
                return Err(ConvError::MissingHistory);
            }
            let mut best = None;
            let mut minimum = MAX_AE;
            for state in 0..self.n_states {
                let mut traced = state;
                for row in (0..self.o_idx).rev() {
                    traced = self.predecessor(row, traced)?;
                }
                if traced == state && self.ae[state] < minimum {
                    minimum = self.ae[state];
                    best = Some(state);
                }
            }
            if let Some(best) = best {
                return Ok(best);
            }
        }
        let mut minimum = MAX_AE;
        let mut best = None;
        for (state, &error) in self.ae.iter().enumerate() {
            if error < minimum {
                minimum = error;
                best = Some(state);
            }
        }
        best.ok_or(ConvError::NoSurvivor)
    }

    fn predecessor(&self, row: usize, state: usize) -> Result<usize, ConvError> {
        let previous =
            self.history[row * self.n_states + state].ok_or(ConvError::MissingHistory)?;
        if usize::from(previous) >= self.n_states {
            return Err(ConvError::UnreachableTraceback);
        }
        Ok(usize::from(previous))
    }

    /// Return the selected accumulated error, not BER or a hard error count.
    /// Negative end selectors request automatic selection. `has_flush` omits
    /// the last `K-1` bits; all traceback is checked before any output write.
    pub fn get_output(
        &self,
        output: &mut [u8],
        has_flush: bool,
        end_state: i32,
    ) -> Result<u32, ConvError> {
        self.validate()?;
        validate_selected_state(self.n_states, end_state)?;
        let tail = if has_flush {
            (self.code.spec.k - 1) as usize
        } else {
            0
        };
        let payload = self
            .o_idx
            .checked_sub(tail)
            .ok_or(ConvError::InvalidOperation)?;
        let selected = if end_state < 0 {
            self.best_end_state()?
        } else {
            end_state as usize
        };
        if output.len() < payload {
            return Err(ConvError::OutputTooShort);
        }
        let metric = self.ae[selected];
        // The C metric indexes the full selector, while traceback narrows it.
        let mut state = usize::from(selected as u8);
        let mut decoded = filled(payload, 0)?;
        for row in (0..self.o_idx).rev() {
            let previous = self.predecessor(row, state).map_err(|error| {
                if metric == MAX_AE {
                    ConvError::UnreachableTraceback
                } else {
                    error
                }
            })?;
            if row < payload {
                decoded[row] =
                    u8::from(usize::from(self.code.spec.next_state[previous][0]) != state);
            }
            state = previous;
        }
        output[..payload].copy_from_slice(&decoded);
        Ok(metric)
    }
}

fn validate_selected_state(states: usize, selected: i32) -> Result<(), ConvError> {
    if selected >= 0 && selected as usize >= states {
        Err(ConvError::InvalidState)
    } else {
        Ok(())
    }
}

fn validate_start_state(states: usize, selected: i32) -> Result<(), ConvError> {
    if selected >= 0 && selected as usize > states {
        Err(ConvError::InvalidState)
    } else {
        Ok(())
    }
}

fn history_cells(code: &ConvCode, len: usize) -> Result<usize, ConvError> {
    let rows = len
        .checked_add((code.spec.k - 1) as usize)
        .filter(|&rows| rows <= i32::MAX as usize)
        .ok_or(ConvError::ArithmeticOverflow)?;
    // C's history offsets multiply two signed ints.
    rows.checked_mul(code.state_count())
        .filter(|&cells| cells <= i32::MAX as usize)
        .ok_or(ConvError::ArithmeticOverflow)
}

pub(super) fn filled<T: Clone>(len: usize, value: T) -> Result<Vec<T>, ConvError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(len)
        .map_err(|_| ConvError::AllocationFailed)?;
    result.resize(len, value);
    Ok(result)
}

fn branch_error(mut error: u32, symbol: u8, input: &[i8]) -> Result<u32, ConvError> {
    let mut mask = (1u32 << (input.len() - 1)) as u8;
    for &sample in input {
        if sample != 0 {
            let expected = if symbol & mask != 0 { -127 } else { 127 };
            let delta = i32::from(sample) - expected;
            error = error
                .checked_add(((delta * delta) >> 9) as u32)
                .filter(|&metric| metric <= i32::MAX as u32)
                .ok_or(ConvError::ArithmeticOverflow)?;
        }
        mask >>= 1;
    }
    Ok(error)
}

pub fn osmo_conv_decode_scan(
    decoder: &mut ConvDecoder<'_>,
    input: &[i8],
    n: i32,
) -> Result<usize, ConvError> {
    decoder.scan(input, n.max(0) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conv::ConvCodeSpec;

    fn code(k: i32, len: i32, term: Termination) -> ConvCode {
        let states = 1usize << (k - 1);
        ConvCode::new(ConvCodeSpec {
            n: 1,
            k,
            len,
            term,
            next_output: vec![[0, 1]; states],
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

    fn rejects_without_mutation(decoder: &mut ConvDecoder<'_>, error: ConvError) {
        let previous = decoder.clone();
        let mut output = [0xa5; 10];
        assert_eq!(decoder.scan(&[127; 10], 1), Err(error));
        assert_eq!(decoder.flush(&[127; 10]), Err(error));
        assert_eq!(decoder.reset(0), Err(error));
        assert_eq!(decoder.rewind(), Err(error));
        assert_eq!(decoder.best_end_state(), Err(error));
        assert_eq!(decoder.get_output(&mut output, false, 0), Err(error));
        assert_eq!(*decoder, previous);
        assert_eq!(output, [0xa5; 10]);
    }

    #[test]
    fn malformed_owned_storage_and_cursors_are_checked() {
        let code = code(2, 4, Termination::TRUNCATION);
        let initial = ConvDecoder::new(&code, 0, 0).unwrap();
        for mutation in 0..7 {
            let mut decoder = initial.clone();
            match mutation {
                0 => {
                    decoder.ae.pop();
                }
                1 => {
                    decoder.ae_next.pop();
                }
                2 => {
                    decoder.history.pop();
                }
                3 => {
                    decoder.n_states = 1;
                }
                4 => {
                    decoder.o_idx = 6;
                }
                5 => {
                    decoder.p_idx = 1;
                }
                6 => {
                    decoder.len = 0;
                }
                _ => unreachable!(),
            }
            rejects_without_mutation(&mut decoder, ConvError::InvalidDecoderState);
        }
        let mut spec = code.spec().clone();
        spec.puncture = Some(vec![0, 2]);
        let punctured = ConvCode::new(spec).unwrap();
        let mut decoder = ConvDecoder::new(&punctured, 0, 0).unwrap();
        decoder.p_idx = 2;
        rejects_without_mutation(&mut decoder, ConvError::InvalidDecoderState);
    }

    #[test]
    fn descriptor_state_mismatch_is_checked() {
        let small = code(2, 4, Termination::TRUNCATION);
        let large = code(3, 4, Termination::TRUNCATION);
        let mut decoder = ConvDecoder::new(&small, 0, 0).unwrap();
        decoder.code = &large;
        rejects_without_mutation(&mut decoder, ConvError::InvalidDecoderState);
    }

    #[test]
    fn invalid_metrics_and_signed_branch_overflow_are_checked() {
        let code = code(2, 4, Termination::TRUNCATION);
        for value in [MAX_AE + 1, i32::MAX as u32, u32::MAX] {
            let mut decoder = ConvDecoder::new(&code, 0, 0).unwrap();
            decoder.ae[0] = value;
            rejects_without_mutation(&mut decoder, ConvError::ArithmeticOverflow);
            let mut decoder = ConvDecoder::new(&code, 0, 0).unwrap();
            decoder.ae_next_valid = true;
            decoder.ae_next[0] = value;
            rejects_without_mutation(&mut decoder, ConvError::ArithmeticOverflow);
        }
        assert_eq!(
            branch_error(i32::MAX as u32, 0, &[-128]),
            Err(ConvError::ArithmeticOverflow)
        );
        assert_eq!(
            branch_error(u32::MAX, 0, &[-128]),
            Err(ConvError::ArithmeticOverflow)
        );
        assert_eq!(
            branch_error(i32::MAX as u32 - 127, 0, &[-128]),
            Ok(i32::MAX as u32)
        );
    }

    #[test]
    fn missing_or_invalid_predecessor_never_partially_writes_output() {
        let code = code(2, 4, Termination::TRUNCATION);
        let mut decoder = ConvDecoder::new(&code, 0, -1).unwrap();
        decoder.scan(&[127; 4], 4).unwrap();
        for invalid in [None, Some(255)] {
            let mut damaged = decoder.clone();
            // The failure occurs after some later bits have been traced.
            damaged.history[..damaged.n_states].fill(invalid);
            let previous = damaged.clone();
            let mut output = [0xa5; 8];
            let error = if invalid.is_none() {
                ConvError::MissingHistory
            } else {
                ConvError::UnreachableTraceback
            };
            assert_eq!(damaged.get_output(&mut output, false, 0), Err(error));
            assert_eq!(damaged, previous);
            assert_eq!(output, [0xa5; 8]);
        }
    }

    #[test]
    fn unreachable_selected_state_and_no_survivor_mapping() {
        let mut spec = code(2, 1, Termination::TRUNCATION).spec().clone();
        spec.next_state.fill([0, 0]);
        let code = ConvCode::new(spec).unwrap();
        let mut decoder = ConvDecoder::new(&code, 0, 0).unwrap();
        decoder.scan(&[127], 1).unwrap();
        let before = decoder.clone();
        let mut output = [0xa5; 3];
        assert_eq!(
            decoder.get_output(&mut output, false, 1),
            Err(ConvError::UnreachableTraceback)
        );
        assert_eq!(decoder, before);
        assert_eq!(output, [0xa5; 3]);
        decoder.reset(2).unwrap();
        assert_eq!(decoder.best_end_state(), Err(ConvError::NoSurvivor));
        assert_eq!(
            decoder.get_output(&mut [], false, -1),
            Err(ConvError::NoSurvivor)
        );
        decoder.scan(&[127], 1).unwrap();
        assert_eq!(decoder.accumulated_errors(), &[MAX_AE; 2]);
        assert_eq!(decoder.predecessor_history(), &[None; 2]);
        assert_eq!(
            decoder.get_output(&mut [], false, -1),
            Err(ConvError::NoSurvivor)
        );
        // C rewind normalizes even an all-sentinel error array.
        decoder.rewind().unwrap();
        assert_eq!(decoder.accumulated_errors(), &[0; 2]);
    }

    #[test]
    fn sentinel_is_a_strict_survivor_cutoff_not_a_wider_metric() {
        let code = code(1, 1, Termination::TRUNCATION);
        let mut decoder = ConvDecoder::new(&code, 0, 0).unwrap();
        decoder.ae[0] = MAX_AE - 1;
        decoder.scan(&[1], 1).unwrap();
        assert_eq!(decoder.ae, [MAX_AE]);
        assert_eq!(decoder.history, [None]);
        assert_eq!(decoder.best_end_state(), Err(ConvError::NoSurvivor));
    }

    #[test]
    fn reset_and_rewind_preserve_scratch_but_do_not_revalidate_old_history() {
        let code = code(3, 3, Termination::TRUNCATION);
        let mut decoder = ConvDecoder::new(&code, 0, -1).unwrap();
        decoder.scan(&[-128, 35, -50], 3).unwrap();
        let scratch = decoder.ae_next.clone();
        let history = decoder.history.clone();
        let errors = decoder.ae.clone();
        let minimum = *errors.iter().min().unwrap();
        decoder.rewind().unwrap();
        assert_eq!(
            decoder.ae,
            errors.iter().map(|x| x - minimum).collect::<Vec<_>>()
        );
        assert_eq!(decoder.ae_next, scratch);
        assert_eq!(decoder.history, history);
        assert!(decoder.predecessor_history().is_empty());
        decoder.reset(0).unwrap();
        assert_eq!(decoder.ae_next, scratch);
        assert_eq!(decoder.history, history);
        decoder.scan(&[127], 1).unwrap();
        assert_eq!(
            decoder.predecessor_history(),
            &[Some(0), Some(0), None, None]
        );
        let mut output = [0xa5; 4];
        assert_eq!(
            decoder.get_output(&mut output, false, 3),
            Err(ConvError::UnreachableTraceback)
        );
        assert_eq!(output, [0xa5; 4]);
    }

    #[test]
    fn rotated_buffers_expose_final_metrics_for_odd_even_and_flush_steps() {
        let code = code(2, 4, Termination::TRUNCATION);
        let mut whole = ConvDecoder::new(&code, 0, -1).unwrap();
        let mut chunks = whole.clone();
        whole.scan(&[127, -127, 64], 3).unwrap();
        chunks.scan(&[127], 1).unwrap();
        assert_eq!(chunks.next_accumulated_errors(), Some([0, 126].as_slice()));
        chunks.scan(&[-127, 64], 2).unwrap();
        assert_eq!(chunks, whole);
        assert_eq!(whole.accumulated_errors(), [7, 71]);
        assert_eq!(whole.next_accumulated_errors(), Some([7, 71].as_slice()));
        let unchanged = whole.clone();
        assert_eq!(whole.scan(&[], 0), Ok(0));
        assert_eq!(whole.scan(&[], 1), Err(ConvError::InputTooShort));
        assert_eq!(whole, unchanged);
        whole.flush(&[64]).unwrap();
        assert_eq!(whole.accumulated_errors(), [14, MAX_AE]);
        assert_eq!(
            whole.next_accumulated_errors(),
            Some([14, MAX_AE].as_slice())
        );
    }

    #[test]
    fn tail_biting_checks_all_history_and_uses_defined_nonclosing_fallback() {
        let code = code(3, 3, Termination::TAIL_BITING);
        let mut short = ConvDecoder::new(&code, 0, 0).unwrap();
        short.scan(&[127], 1).unwrap();
        assert_eq!(short.best_end_state(), Err(ConvError::MissingHistory));
        let before = short.clone();
        let mut output = [0xa5; 4];
        assert_eq!(
            short.get_output(&mut output, false, -1),
            Err(ConvError::MissingHistory)
        );
        assert_eq!(short, before);
        assert_eq!(output, [0xa5; 4]);
        let mut spec = code.spec().clone();
        spec.k = 2;
        spec.len = 1;
        spec.next_output = vec![[0, 0]; 2];
        spec.next_state = vec![[1, 1], [0, 0]];
        let flip = ConvCode::new(spec).unwrap();
        let mut decoder = ConvDecoder::new(&flip, 0, -1).unwrap();
        decoder.scan(&[0], 1).unwrap();
        assert_eq!(decoder.predecessor_history(), &[Some(1), Some(0)]);
        assert_eq!(decoder.best_end_state(), Ok(0));
        assert_eq!(decoder.get_output(&mut output, false, -1), Ok(0));
        assert_eq!(output, [0, 0xa5, 0xa5, 0xa5]);
    }

    #[test]
    fn decoder_dimensions_and_sizing_overflow_are_checked() {
        let mut spec = code(2, 0, Termination::TRUNCATION).spec().clone();
        spec.n = 32;
        let wide = ConvCode::new(spec).unwrap();
        assert_eq!(ConvDecoder::new(&wide, 0, 0), Err(ConvError::InvalidRate));
        let code = code(2, 0, Termination::TRUNCATION);
        for len in [i32::MAX, i32::MAX / 2] {
            assert_eq!(
                ConvDecoder::new(&code, len, 0),
                Err(ConvError::ArithmeticOverflow)
            );
        }
        let decoder = ConvDecoder::new(&code, 0, 0).unwrap();
        assert_eq!(
            decoder.scan_input_capacity(usize::MAX),
            Err(ConvError::ArithmeticOverflow)
        );
        assert_eq!(
            decoder.scan_input_capacity(2),
            Err(ConvError::InvalidOperation)
        );
        assert_eq!(filled(usize::MAX, 0u32), Err(ConvError::AllocationFailed));
    }

    #[test]
    fn all_soft_bytes_and_byte_mask_narrowing_are_preserved() {
        for sample in i8::MIN..=i8::MAX {
            for bit in 0..=1 {
                let expected = if bit == 0 { 127 } else { -127 };
                let delta = i32::from(sample) - expected;
                let metric = if sample == 0 {
                    0
                } else {
                    ((delta * delta) >> 9) as u32
                };
                assert_eq!(branch_error(7, bit, &[sample]), Ok(7 + metric));
            }
        }
        assert_eq!(branch_error(0, 0, &[-128]), Ok(127));
        assert_eq!(branch_error(0, 1, &[-128]), Ok(0));
        for rate in [9, 31] {
            assert_eq!(branch_error(0, 255, &vec![127; rate]), Ok(0));
            assert_eq!(
                branch_error(0, 255, &vec![-128; rate]),
                Ok(rate as u32 * 127)
            );
        }
    }

    #[test]
    fn traceback_uses_transition_zero_test_not_the_winning_branch_bit() {
        let code = code(1, 1, Termination::TRUNCATION);
        let mut decoder = ConvDecoder::new(&code, 0, 0).unwrap();
        decoder.scan(&[-127], 1).unwrap();
        let mut output = [0xa5; 2];
        // Bit one has the lowest branch error, but both branches target zero.
        assert_eq!(decoder.get_output(&mut output, false, 0), Ok(0));
        assert_eq!(output, [0, 0xa5]);
    }
}
