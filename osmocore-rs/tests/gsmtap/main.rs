//! GSMTAP constructor comparisons against committed C observations.

#[path = "../support/gsmtap_constants_fixture.rs"]
mod gsmtap_constants_fixture;

mod constants_reference_vectors;
mod gsmtap_reference_vectors;

mod owned_encoding;
#[cfg(feature = "runtime")]
mod runtime_integration;
