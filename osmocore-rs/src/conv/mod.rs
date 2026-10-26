// SPDX-License-Identifier: GPL-3.0-or-later

//! Checked convolutional encoding, stateful decoding and one-shot dispatch.
//!
//! Compatibility lengths are signed C-domain results, not allocation sizes.
//! Use operation-derived capacities for buffers. Portable correlation follows
//! the approved scalar/SSE/AVX tie policy, not NEON or host CPU selection.

pub(crate) mod correlation;
pub(crate) mod decoder;
pub(crate) mod encoder;

pub use correlation::decode;
pub use decoder::ConvDecoder;
pub use encoder::{ConvEncoder, encode};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvError {
    InvalidRate,
    InvalidConstraintLength,
    InvalidFrameLength,
    InvalidTableLength,
    InvalidTransition,
    InvalidTerminationTables,
    InvalidPuncture,
    ArithmeticOverflow,
    InputTooShort,
    OutputTooShort,
    InvalidBit,
    InvalidState,
    TailBitingTooShort,
    InvalidDecoderState,
    InvalidOperation,
    MissingHistory,
    UnreachableTraceback,
    NoSurvivor,
    AllocationFailed,
    UnsupportedRecursiveCode,
}

/// Unknown numeric values retain C's non-flushing, non-tail-biting policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Termination(pub i32);

impl Termination {
    pub const FLUSH: Self = Self(0);
    pub const TRUNCATION: Self = Self(1);
    pub const TAIL_BITING: Self = Self(2);
}

/// Owned descriptor input. Punctures are active absolute positions, without a
/// negative sentinel. `None` and `Some(Vec::new())` remain distinguishable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvCodeSpec {
    pub n: i32,
    pub k: i32,
    pub len: i32,
    pub term: Termination,
    pub next_output: Vec<[u8; 2]>,
    pub next_state: Vec<[u8; 2]>,
    pub next_term_output: Option<Vec<u8>>,
    pub next_term_state: Option<Vec<u8>>,
    pub puncture: Option<Vec<i32>>,
}

/// Immutable validated code. Encoder shifts support `N=1..=32` and descriptor
/// state-count shifts support `K=1..=31`; these are not decoder support ranges.
/// Output symbols retain all byte bits, even those ignored at smaller rates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvCode {
    spec: ConvCodeSpec,
}

impl ConvCode {
    pub fn new(spec: ConvCodeSpec) -> Result<Self, ConvError> {
        if !(1..=32).contains(&spec.n) {
            return Err(ConvError::InvalidRate);
        }
        if !(1..=31).contains(&spec.k) {
            return Err(ConvError::InvalidConstraintLength);
        }
        if spec.len < 0 {
            return Err(ConvError::InvalidFrameLength);
        }
        let states = 1usize << (spec.k - 1);
        if spec.next_output.len() != states || spec.next_state.len() != states {
            return Err(ConvError::InvalidTableLength);
        }
        if spec
            .next_state
            .iter()
            .flatten()
            .any(|&state| usize::from(state) >= states)
        {
            return Err(ConvError::InvalidTransition);
        }
        match (&spec.next_term_output, &spec.next_term_state) {
            (None, None) => {}
            (Some(output), Some(state)) => {
                if output.len() != states || state.len() != states {
                    return Err(ConvError::InvalidTableLength);
                }
                if state.iter().any(|&state| usize::from(state) >= states) {
                    return Err(ConvError::InvalidTransition);
                }
            }
            _ => return Err(ConvError::InvalidTerminationTables),
        }
        if let Some(puncture) = &spec.puncture {
            if puncture.iter().any(|&position| position < 0)
                || puncture.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(ConvError::InvalidPuncture);
            }
            i32::try_from(puncture.len()).map_err(|_| ConvError::ArithmeticOverflow)?;
        }
        Ok(Self { spec })
    }

    pub fn spec(&self) -> &ConvCodeSpec {
        &self.spec
    }

    pub fn state_count(&self) -> usize {
        1usize << (self.spec.k - 1)
    }

    /// Exact one-shot soft-input count, ignoring punctures beyond the stream.
    /// Unlike encoding capacity, decoding does not load a tail-biting state
    /// from the final input bits and imposes no encoder tail-loading minimum.
    /// Backend-specific support and traceback checks happen when decoding.
    pub fn decode_input_capacity(&self) -> Result<usize, ConvError> {
        correlation::input_sizes(self).map(|(_, _, transmitted)| transmitted)
    }

    /// Exact one-shot write capacity, counting only encountered punctures.
    /// Also rejects a tail-biting frame shorter than the state-loading span.
    pub fn output_capacity(&self) -> Result<usize, ConvError> {
        let len = self.spec.len as usize;
        let tail = (self.spec.k - 1) as usize;
        if self.spec.term == Termination::TAIL_BITING && len < tail {
            return Err(ConvError::TailBitingTooShort);
        }
        let symbols = len
            .checked_add(if self.spec.term == Termination::FLUSH {
                tail
            } else {
                0
            })
            .ok_or(ConvError::ArithmeticOverflow)?;
        ConvEncoder::new(self).raw_output_capacity(symbols)
    }
}

pub(crate) mod legacy {
    use super::*;
    /// Positive overrides select that length; nonpositive overrides select `len`.
    pub fn osmo_conv_get_input_length(code: &ConvCode, len: i32) -> Result<i32, ConvError> {
        Ok(if len > 0 { len } else { code.spec.len })
    }

    /// Preserve C's signed result, including negative values. Every bounded active
    /// puncture reduces this result, even beyond the stream endpoint. Check each
    /// arithmetic step in C evaluation order; do not use this for buffer sizing.
    pub fn osmo_conv_get_output_length(code: &ConvCode, len: i32) -> Result<i32, ConvError> {
        let mut output = osmo_conv_get_input_length(code, len)?
            .checked_mul(code.spec.n)
            .ok_or(ConvError::ArithmeticOverflow)?;
        if code.spec.term == Termination::FLUSH {
            let tail = code.spec.n * (code.spec.k - 1);
            output = output
                .checked_add(tail)
                .ok_or(ConvError::ArithmeticOverflow)?;
        }
        let punctures = code.spec.puncture.as_ref().map_or(0, Vec::len) as i32;
        output
            .checked_sub(punctures)
            .ok_or(ConvError::ArithmeticOverflow)
    }
}
