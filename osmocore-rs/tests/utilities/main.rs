//! Utilities API comparisons against committed C observations.

#[path = "../support/bitvec_fixture.rs"]
mod bitvec_fixture;
#[path = "../support/gzip_fixture.rs"]
mod gzip_fixture;
#[path = "../support/prbs_fixture.rs"]
mod prbs_fixture;
#[path = "../support/soft_bits_fixture.rs"]
mod soft_bits_fixture;

mod base64_reference_vectors;
mod bcd_reference_vectors;
mod bit_packing_ext_reference_vectors;
mod bit_packing_reference_vectors;
mod bitvec_adapters_reference_vectors;
mod bitvec_buffers_reference_vectors;
mod bitvec_operations_reference_vectors;
mod bitvec_reference_vectors;
mod crc_reference_vectors;
mod hexparse_reference_vectors;
mod prbs_reference_vectors;
mod prim_reference_vectors;
mod soft_bits_reference_vectors;

mod operation_sequences;
