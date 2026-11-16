//! The `osmo_auc_gen` example against committed recordings of libosmocore's `osmo-auc-gen`
//! utility: every command of the C program's test script and the harness's cases. The
//! example needs the `auth` feature; without it, the fixture's shape is still checked.

// Without the `auth` feature, only the shape check uses the reader.
#[cfg_attr(not(feature = "auth"), allow(dead_code))]
#[path = "../support/osmo_auc_gen_fixture.rs"]
mod osmo_auc_gen_fixture;

#[cfg(feature = "auth")]
#[path = "../../examples/osmo_auc_gen.rs"]
mod example;

#[cfg(feature = "auth")]
mod behavior;
mod reference_vectors;
