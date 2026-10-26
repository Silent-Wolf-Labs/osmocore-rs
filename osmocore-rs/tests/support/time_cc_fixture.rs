//! Shared direct-C evidence validation. Contains no time_cc conversion model.
use std::collections::BTreeMap;
pub type Record = BTreeMap<String, String>;
pub fn signed(row: &Record, key: &str) -> i64 {
    row[key].parse().unwrap()
}
pub fn unsigned(row: &Record, key: &str) -> u64 {
    row[key].parse().unwrap()
}
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
const TEXT_HASH: u64 = 0xa2ee3659631ad0a1;
const REFERENCE_HASH: u64 = 0x04574c0d977731bc;
const GENERATOR_HASH: u64 = 0x6aa076d90713d7fa;
const DIAGNOSTICS_HASH: u64 = 0xe4fee62e58e5ab1a;
const INPUT_HASH: u64 = 0x557742451d06e0f1;
const REFERENCE_SHA: &str = "782c22f7112724bac91f6132bdf2e21ed679b09af57b70be950419078d98a2d7";
const GENERATOR_SHA: &str = "2b35e42c040f38b86bce8c2a7b04514936c755025592f4df14c8ee1da3082226";
const INPUT_KEYS: &str = "scenario step op arg mono_s mono_ns wall_s wall_us fail a b c d e";
const TRACE_KEYS: &str = "a active arg b bindings c counter d deadline_s deadline_us e fail flag forget gran last mono_ns mono_s op previous reported scenario sink start step sum threshold timers total type wall_s wall_us work";
fn parse(line: &str) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut row = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').unwrap();
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        assert!(!key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let value = if matches!(
            key,
            "type" | "op" | "evidence" | "revision" | "reference" | "generator"
        ) {
            let value = value.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(!value.contains(['"', '\\']) && !value.chars().any(char::is_control));
            value
        } else {
            if matches!(
                key,
                "arg"
                    | "mono_s"
                    | "mono_ns"
                    | "wall_s"
                    | "wall_us"
                    | "deadline_s"
                    | "deadline_us"
                    | "work"
            ) {
                assert_eq!(value.parse::<i64>().unwrap().to_string(), value);
            } else {
                assert_eq!(value.parse::<u64>().unwrap().to_string(), value);
            }
            value
        };
        assert!(row.insert(key.to_owned(), value.to_owned()).is_none());
    }
    row
}
fn shape(row: &Record, keys: &str) {
    let mut keys: Vec<_> = keys.split_whitespace().collect();
    keys.sort_unstable();
    assert_eq!(row.keys().map(String::as_str).collect::<Vec<_>>(), keys);
}
pub fn validate_structure(text: &str) -> Vec<Record> {
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let rows: Vec<_> = text.lines().map(parse).collect();
    assert_eq!(rows.len(), 5948);
    let meta = &rows[0];
    shape(
        meta,
        "type schema evidence revision reference generator ulong_bits long_bits int_bits",
    );
    assert_eq!(meta["type"], "meta");
    assert_eq!(unsigned(meta, "schema"), 1);
    assert_eq!(meta["evidence"], "exact-source");
    assert_eq!(meta["revision"], "unknown");
    assert_eq!(meta["reference"], REFERENCE_SHA);
    assert_eq!(meta["generator"], GENERATOR_SHA);
    assert_eq!(
        (
            unsigned(meta, "ulong_bits"),
            unsigned(meta, "long_bits"),
            unsigned(meta, "int_bits")
        ),
        (64, 64, 32)
    );
    let mut scenario = 0;
    let mut step = 0;
    let mut inputs = String::new();
    let mut initial = Vec::new();
    let mut counts = Vec::new();
    for row in &rows[1..] {
        shape(row, TRACE_KEYS);
        assert_eq!(row["type"], "trace");
        if unsigned(row, "scenario") != scenario {
            if scenario != 0 {
                counts.push(step);
            }
            scenario += 1;
            step = 0;
            assert_eq!(unsigned(row, "scenario"), scenario);
            assert_eq!(row["op"], "init");
            initial.push((
                unsigned(row, "a"),
                unsigned(row, "b"),
                unsigned(row, "c"),
                signed(row, "arg"),
            ));
        }
        assert_eq!(unsigned(row, "step"), step);
        step += 1;
        assert!(matches!(
            row["op"].as_str(),
            "init"
                | "flag"
                | "update"
                | "cleanup"
                | "config"
                | "bindings"
                | "tset"
                | "seed"
                | "counter"
        ));
        for key in ["active", "bindings", "fail", "flag", "sink"] {
            assert!(unsigned(row, key) <= 1);
        }
        for key in INPUT_KEYS.split_whitespace() {
            inputs.push_str(&row[key]);
            inputs.push('\n');
        }
    }
    counts.push(step);
    assert_eq!(scenario, 469);
    assert_eq!(&counts[..448], &[13; 448]);
    assert_eq!(
        &counts[448..],
        &[
            9, 8, 7, 6, 9, 14, 7, 5, 5, 5, 5, 4, 4, 4, 6, 4, 5, 4, 4, 4, 4
        ]
    );
    let mut cursor = 0;
    for gran in [0, 1, 2, 3, 10, 1000000, 2147483648000000, u64::MAX] {
        for threshold in [0, 1, 2, 5, 500000, 1000000, u64::MAX] {
            for forget in [0, 1, 1000000, u64::MAX] {
                for sink in 0..2 {
                    assert_eq!(initial[cursor], (gran, threshold, forget, sink));
                    cursor += 1;
                }
            }
        }
    }
    assert_eq!(fingerprint(&inputs), INPUT_HASH, "ordered C inputs changed");
    rows
}
pub fn validate(text: &str, reference: &str, generator: &str, diagnostics: &str) -> Vec<Record> {
    assert_eq!(
        fingerprint(reference),
        REFERENCE_HASH,
        "reference provenance changed"
    );
    assert_eq!(
        fingerprint(generator),
        GENERATOR_HASH,
        "generator provenance changed"
    );
    assert_eq!(
        fingerprint(diagnostics),
        DIAGNOSTICS_HASH,
        "diagnostic evidence changed"
    );
    assert!(diagnostics.contains(REFERENCE_SHA) && diagnostics.contains(GENERATOR_SHA));
    assert_eq!(diagnostics.matches("\"probe\":").count(), 5);
    let rows = validate_structure(text);
    assert_eq!(fingerprint(text), TEXT_HASH, "C observations changed");
    rows
}
#[cfg(test)]
pub mod checks {
    use super::*;
    pub fn integrity(t: &str, r: &str, g: &str, d: &str) {
        assert_eq!(validate(t, r, g, d).len(), 5948);
    }
    pub fn corruptions(t: &str, r: &str, g: &str, d: &str) {
        let reject = |t: &str, r: &str, g: &str, d: &str| {
            assert!(std::panic::catch_unwind(|| validate(t, r, g, d)).is_err())
        };
        reject(t, &r.replace("unknown", "claimed"), g, d);
        reject(t, r, &g.replace("schema", "changed"), d);
        reject(t, r, g, &d.replace("assertion", "changed"));
        for (a, b) in [
            ("\"schema\":1", "\"schema\":2"),
            ("\"arg\":0", "\"arg\":2"),
            ("\"sum\":0", "\"sum\":1"),
            ("\"active\":0", "\"active\":2"),
            ("\"a\":0", "\"a\":00"),
            ("\"a\":0", "\"a\":-1"),
            ("\"a\":18446744073709551615", "\"a\":18446744073709551616"),
            ("\"type\":\"trace\"", "\"type\":\"other\""),
            ("\"schema\":1", "\"schema\":1,\"schema\":1"),
            ("\"step\":0", "\"step\":1"),
            ("\"schema\":1", "\"schema\":1,\"extra\":0"),
        ] {
            let changed = t.replacen(a, b, 1);
            assert_ne!(t, changed);
            reject(&changed, r, g, d);
        }
        reject(t.trim_end(), r, g, d);
        let changed = t.replacen("\"sum\":0", "\"sum\":1", 1);
        validate_structure(&changed);
        reject(&changed, r, g, d);
    }
}
