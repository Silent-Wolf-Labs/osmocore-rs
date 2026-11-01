//! Step 1 evidence integrity only: no Rust-versus-C conversion claim.
use super::conv_fixture::{self, Fixture, checks};
use crate::gzip_fixture;
use std::sync::LazyLock;
use std::sync::OnceLock;

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    gzip_fixture::text(include_bytes!(
        "../compatibility/vectors/conv/conv_vectors.ndjson.gz"
    ))
});
const REFERENCE: &str = include_str!("../compatibility/vectors/conv/conv_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/conv/conv_generator_provenance.json");

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| Fixture::read(&VECTORS))
}

#[test]
fn conv_direct_c_evidence_integrity_only() {
    conv_fixture::validate_fingerprint(&VECTORS);
    conv_fixture::validate_manifests(REFERENCE, GENERATOR, fixture());
    assert_eq!(fixture().records.len(), 38560);
}

#[test]
fn conv_synthetic_replacements_have_generated_parameters_and_inputs() {
    checks::synthetic_replacements(fixture());
}

#[test]
fn conv_fixed_fingerprint_rejects_changed_evidence() {
    checks::fingerprint_rejections(&VECTORS);
}

#[test]
fn conv_strict_parser_rejects_corruption_without_fingerprint() {
    checks::parser_rejections(&VECTORS);
}

#[test]
fn conv_descriptor_invariants_reject_corruption_without_fingerprint() {
    checks::descriptor_rejections(fixture());
}

#[test]
fn conv_operation_invariants_reject_corruption_without_fingerprint() {
    checks::operation_rejections(fixture());
}

#[test]
fn conv_coverage_rejects_missing_duplicate_and_renamed_cases_without_fingerprint() {
    checks::coverage_rejections(fixture());
}

#[test]
fn conv_reset_and_rewind_semantics_are_distinct() {
    checks::state_rejections(fixture());
}

#[test]
fn conv_x86_backends_agree_on_matched_case_and_api_observations_only() {
    assert_eq!(conv_fixture::compare_backends(fixture()), 10485);
    checks::backend_rejections(fixture());
}

#[test]
fn conv_provenance_rejects_corruption() {
    checks::provenance_rejections(fixture(), REFERENCE, GENERATOR);
}

#[test]
fn conv_compressed_fixture_rejects_corruption() {
    let compressed: &[u8] = include_bytes!("../compatibility/vectors/conv/conv_vectors.ndjson.gz");
    assert!(gzip_fixture::try_text(compressed).is_ok());
    let mut flipped = compressed.to_vec();
    flipped[compressed.len() / 2] ^= 0xff;
    assert!(gzip_fixture::try_text(&flipped).is_err());
    assert!(gzip_fixture::try_text(&compressed[..compressed.len() - 1]).is_err());
    let mut trailing = compressed.to_vec();
    trailing.push(0);
    assert!(gzip_fixture::try_text(&trailing).is_err());
}
