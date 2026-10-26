// SPDX-License-Identifier: GPL-3.0-or-later

/// Read-only internal decoder state for conformance replay.
pub struct DecoderDiagnostics<'a> {
    pub output_index: usize,
    pub puncture_index: usize,
    pub accumulated_errors: &'a [u32],
    pub next_accumulated_errors: Option<&'a [u32]>,
    pub predecessor_history: &'a [Option<u8>],
}
/// Inspect initialized decoder state without exposing mutation of its scratch buffers.
pub fn diagnostics<'a>(decoder: &'a crate::conv::ConvDecoder<'_>) -> DecoderDiagnostics<'a> {
    DecoderDiagnostics {
        output_index: decoder.output_index(),
        puncture_index: decoder.puncture_index(),
        accumulated_errors: decoder.accumulated_errors(),
        next_accumulated_errors: decoder.next_accumulated_errors(),
        predecessor_history: decoder.predecessor_history(),
    }
}

pub use crate::conv::correlation::osmo_conv_decode_acc;
pub use crate::conv::decoder::osmo_conv_decode_scan;
pub use crate::conv::encoder::osmo_conv_encode_raw;
pub use crate::conv::legacy::{osmo_conv_get_input_length, osmo_conv_get_output_length};
