//! TUAK comparisons against committed C observations: the TUAK functions, and TUAK vectors
//! through the preferred and compat authentication APIs. TUAK needs the `auth` feature;
//! without it, the fixture's shape and compat's `-ENOENT` are still checked.

// Without the `auth` feature, only the shape and compat checks use the reader.
#[cfg_attr(not(feature = "auth"), allow(dead_code))]
#[path = "../support/tuak_fixture.rs"]
mod tuak_fixture;

mod behavior;
mod reference_vectors;
