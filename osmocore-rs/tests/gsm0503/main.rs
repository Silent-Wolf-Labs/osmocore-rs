//! GSM 05.03 API comparisons against committed C observations.

#[path = "../support/gsm0503_codes_fixture.rs"]
mod gsm0503_codes_fixture;
#[path = "../support/gsm0503_fixture.rs"]
mod gsm0503_fixture;
#[path = "../support/gsm0503_tch_fr_fixture.rs"]
mod gsm0503_tch_fr_fixture;

mod codes;
mod tch_fr;

mod reference_vectors;
mod stealing_flags;
