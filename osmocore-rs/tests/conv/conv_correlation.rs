//! Independent direct-correlation and public-dispatch frozen C replays.
use super::conv_encoder::descriptor;
use super::conv_fixture::{Fixture, bytes, signed};
use crate::gzip_fixture;
use osmocore_rs::compat::conv::osmo_conv_decode_acc;
use osmocore_rs::conv::*;
use std::collections::BTreeMap;
use std::sync::LazyLock;

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    gzip_fixture::text(include_bytes!(
        "../compatibility/vectors/conv/conv_vectors.ndjson.gz"
    ))
});

type Decode = fn(&ConvCode, &[i8], &mut [u8]) -> Result<i32, ConvError>;

fn replay(api: &str, decode: Decode) {
    let fixture = Fixture::read(&VECTORS);
    let codes: BTreeMap<_, _> = fixture
        .records
        .iter()
        .filter(|r| r["type"] == "code")
        .map(|r| ((r["backend"].as_str(), r["id"].as_str()), descriptor(r)))
        .collect();
    let mut counts = BTreeMap::new();
    let mut failures = 0;
    let mut positive_metrics = 0;
    for r in fixture
        .records
        .iter()
        .filter(|r| r["type"] == "op" && r["api"] == api)
    {
        let backend = r["backend"].as_str();
        let code = &codes[&(backend, r["code"].as_str())];
        let input: Vec<_> = bytes(&r["input_before"])
            .into_iter()
            .map(|b| b as i8)
            .collect();
        let before = input.clone();
        let mut output = bytes(&r["output_before"]);
        let ret = signed(r, "ret");
        let expected = match ret {
            -22 if !(2..=4).contains(&code.spec().n) => Err(ConvError::InvalidRate),
            -22 if ![5, 7].contains(&code.spec().k) => Err(ConvError::InvalidConstraintLength),
            -22 => Err(ConvError::InvalidFrameLength),
            -71 => Err(ConvError::UnsupportedRecursiveCode),
            _ => Ok(ret),
        };
        let context = format!(
            "{backend} {} {} seq {} {api}",
            r["code"], r["case"], r["seq"]
        );
        assert_eq!(decode(code, &input, &mut output), expected, "{context}");
        assert_eq!(input, before, "{context}: input mutation");
        assert_eq!(
            input.iter().map(|&b| b as u8).collect::<Vec<_>>(),
            bytes(&r["input_after"]),
            "{context}"
        );
        assert_eq!(
            output,
            bytes(&r["output_after"]),
            "{context}: complete destination"
        );
        *counts.entry(backend).or_insert(0usize) += 1;
        failures += usize::from(ret < 0);
        positive_metrics += usize::from(ret > 0);
    }
    // Scalar has half of the 144 full-scale cases: it omits out-of-range -128 input.
    let per_backend = if api == "decode_acc" { 1843 } else { 1796 };
    assert_eq!(
        counts,
        [
            ("linked-dispatch", per_backend),
            ("scalar", per_backend - 72),
            ("sse", per_backend),
            ("avx", per_backend),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(failures, if api == "decode_acc" { 4708 } else { 844 });
    if api == "decode_acc" {
        assert_eq!(positive_metrics, 0);
    } else {
        assert!(
            positive_metrics > 0,
            "generic dispatch must preserve its error metric"
        );
    }
}

#[test]
fn replay_every_c_direct_correlation_observation() {
    replay("decode_acc", osmo_conv_decode_acc);
}

#[test]
fn replay_every_c_public_dispatch_observation() {
    replay("decode", decode);
}
