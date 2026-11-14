//! Key derivation comparisons against committed C observations, through the preferred
//! `gsm::kdf` functions and `compat::kdf`. Both need the `auth` feature; without it, the
//! fixture's shape is still checked.

// Without the `auth` feature, only the shape check uses the reader.
#[cfg_attr(not(feature = "auth"), allow(dead_code))]
#[path = "../support/kdf_fixture.rs"]
mod kdf_fixture;

#[cfg(feature = "auth")]
mod behavior;
mod reference_vectors;
