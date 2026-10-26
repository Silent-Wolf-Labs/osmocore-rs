//! Shared direct-C evidence validation only; no Rust TDef implementation.
use std::collections::BTreeMap;
pub type Record = BTreeMap<String, String>;
pub fn unsigned(row: &Record, key: &str) -> u64 {
    row[key].parse().unwrap()
}
pub fn signed(row: &Record, key: &str) -> i64 {
    row[key].parse().unwrap()
}
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
const FIXTURE_HASH: u64 = 0xa693ea7abb5b6dc5;
const REFERENCE_HASH: u64 = 0x2540bd851292c9ce;
const GENERATOR_HASH: u64 = 0xbaa3ad4c9b687d18;
const DIAGNOSTICS_HASH: u64 = 0x60636bd0f1608f93;
const REFERENCE_SHA: &str = "54a1a6dd5d19fe4c1f41f70248fda49a32f79a5bcfee5ac2b022351a8deb01ed";
const GENERATOR_SHA: &str = "840d937c9bd51dd035f3e65e7b1281893ab81bb207237c8887b941583ef525f0";

fn parse(line: &str) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut row = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').unwrap();
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        assert!(!key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let value = if matches!(
            key,
            "type"
                | "evidence"
                | "revision"
                | "reference"
                | "generator"
                | "values"
                | "initial"
                | "final"
                | "name"
        ) {
            let text = value.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(!text.contains(['"', '\\']) && !text.chars().any(char::is_control));
            text
        } else {
            if matches!(
                key,
                "value" | "get" | "before" | "after" | "min" | "max" | "default"
            ) {
                assert_eq!(value.parse::<u64>().unwrap().to_string(), value);
            } else {
                assert_eq!(value.parse::<i64>().unwrap().to_string(), value);
            }
            value
        };
        assert!(
            row.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate field"
        );
    }
    row
}
fn shape(row: &Record, kind: &str, keys: &str) {
    assert_eq!(row["type"], kind);
    let mut keys: Vec<_> = keys.split_whitespace().collect();
    keys.sort_unstable();
    assert_eq!(row.keys().map(String::as_str).collect::<Vec<_>>(), keys);
}
fn inputs(row: &Record, entries: &[(&str, i64)]) {
    for (key, value) in entries {
        assert_eq!(signed(row, key), *value, "coverage field {key}");
    }
}
fn values(row: &Record, count: usize) {
    let fields: Vec<_> = row["values"].split(' ').collect();
    assert_eq!(fields.len(), count);
    for field in fields {
        assert_eq!(field.parse::<u64>().unwrap().to_string(), field);
    }
}
pub const CONVERSION_VALUES: [u64; 36] = [
    0,
    1,
    2,
    59,
    60,
    61,
    999,
    1000,
    1001,
    59999,
    60000,
    60001,
    999999,
    1000000,
    1000001,
    59999999,
    60000000,
    60000001,
    u64::MAX / 60 - 1,
    u64::MAX / 60,
    u64::MAX / 60 + 1,
    u64::MAX / 1000 - 1,
    u64::MAX / 1000,
    u64::MAX / 1000 + 1,
    u64::MAX / 60000 - 1,
    u64::MAX / 60000,
    u64::MAX / 60000 + 1,
    u64::MAX / 1000000 - 1,
    u64::MAX / 1000000,
    u64::MAX / 1000000 + 1,
    u64::MAX / 60000000 - 1,
    u64::MAX / 60000000,
    u64::MAX / 60000000 + 1,
    i64::MAX as u64,
    u64::MAX - 1,
    u64::MAX,
];
pub const BOUNDS: [(u64, u64); 8] = [
    (0, 0),
    (0, 1),
    (1, 1),
    (5, 10),
    (5, 0),
    (10, 5),
    (u64::MAX, 0),
    (u64::MAX, u64::MAX),
];

/// Validate exact ordered input coverage and shapes without calculating outputs.
pub fn validate_structure(text: &str) -> Vec<Record> {
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let rows: Vec<_> = text.lines().map(parse).collect();
    let mut cursor = 0;
    let mut next = || {
        let row = &rows[cursor];
        cursor += 1;
        row
    };
    let meta = next();
    shape(
        meta,
        "meta",
        "type schema evidence revision reference generator ulong_bits long_bits int_bits",
    );
    inputs(
        meta,
        &[
            ("schema", 1),
            ("ulong_bits", 64),
            ("long_bits", 64),
            ("int_bits", 32),
        ],
    );
    assert_eq!(meta["evidence"], "exact-source");
    assert_eq!(meta["revision"], "unknown");
    assert_eq!(meta["reference"], REFERENCE_SHA);
    assert_eq!(meta["generator"], GENERATOR_SHA);
    for from in 0..5 {
        for to in 0..5 {
            for value in CONVERSION_VALUES {
                let row = next();
                shape(row, "conversion", "type from to value get ret before after");
                inputs(row, &[("from", from), ("to", to), ("before", 73)]);
                assert_eq!(unsigned(row, "value"), value);
            }
        }
    }
    for (from, to) in [
        (-1, -1),
        (5, 5),
        (i32::MIN as i64, i32::MIN as i64),
        (i32::MAX as i64, i32::MAX as i64),
        (-1, 3),
        (3, -1),
        (5, 3),
        (3, 5),
        (i32::MIN as i64, 3),
        (3, i32::MAX as i64),
    ] {
        for value in [0, 1, u64::MAX] {
            let row = next();
            shape(
                row,
                "invalid_safe",
                "type from to value get ret before after",
            );
            inputs(row, &[("from", from), ("to", to), ("before", 73)]);
            assert_eq!(unsigned(row, "value"), value);
        }
    }
    for from in [-1, 0, 1, 2, 3, 4, 5, i32::MIN as i64, i32::MAX as i64] {
        for to in [-1, 0, 1, 2, 3, 4, 5, i32::MIN as i64, i32::MAX as i64] {
            let row = next();
            shape(row, "zero_units", "type from to value get ret before after");
            inputs(
                row,
                &[("from", from), ("to", to), ("value", 0), ("before", 73)],
            );
        }
    }
    for id in [0, -7, 7, i32::MIN as i64, i32::MAX as i64, 123, 999] {
        let row = next();
        shape(row, "lookup", "type id index fallback get ret values");
        inputs(row, &[("id", id), ("fallback", i64::MAX)]);
        values(row, 8);
    }
    let row = next();
    shape(row, "reset", "type values");
    values(row, 8);
    for unit in -1..=5 {
        for fallback in 0..=1 {
            let row = next();
            shape(row, "missing", "type unit fallback get entry ret");
            inputs(row, &[("unit", unit), ("fallback", fallback)]);
        }
    }
    shape(next(), "traversal", "type entries hidden");
    shape(next(), "groups", "type entries");
    shape(next(), "traversal_null", "type entries");
    shape(next(), "groups_null", "type entries");
    let reset_inputs: &[&[u64]] = &[
        &[0, 1, 0, 0, 1, 0, 0, 1, u64::MAX],
        &[0, 1, 0, 0, 1, 0, 1],
        &[1, 1, 1],
        &[5, 6, 9, 10],
        &[5, 6, u64::MAX],
        &[],
        &[u64::MAX; 3],
        &[u64::MAX; 5],
    ];
    for (index, (min, max)) in BOUNDS.into_iter().enumerate() {
        let boundary_values = [
            0,
            1,
            min.saturating_sub(1),
            min,
            min.saturating_add(1),
            max.saturating_sub(1),
            max,
            max.saturating_add(1),
            u64::MAX,
        ];
        for value in boundary_values {
            for from in 0..5 {
                let row = next();
                shape(row, "range", "type min max value from in ret before after");
                assert_eq!(unsigned(row, "min"), min);
                assert_eq!(unsigned(row, "max"), max);
                assert_eq!(unsigned(row, "value"), value);
                inputs(row, &[("from", from), ("before", 73)]);
            }
        }
        for default in reset_inputs[index] {
            let row = next();
            shape(row, "reset_range", "type min max default before after");
            assert_eq!(unsigned(row, "min"), min);
            assert_eq!(unsigned(row, "max"), max);
            assert_eq!(unsigned(row, "default"), *default);
            inputs(row, &[("before", 73)]);
        }
        for capacity in 0..=50 {
            let row = next();
            shape(row, "format", "type min max capacity ret initial final");
            assert_eq!(unsigned(row, "min"), min);
            assert_eq!(unsigned(row, "max"), max);
            inputs(row, &[("capacity", capacity)]);
            assert_eq!(row["initial"], "a5".repeat(64));
            assert_eq!(row["final"].len(), 128);
            assert!(
                row["final"]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
        }
    }
    for unit in [0, 1, 2, 3, 4, -1, 5, i32::MIN as i64, i32::MAX as i64] {
        let row = next();
        shape(row, "name", "type unit name");
        inputs(row, &[("unit", unit)]);
    }
    for index in 0..6 {
        let row = next();
        shape(row, "name_table", "type index unit present name");
        inputs(row, &[("index", index)]);
    }
    for state in 0..32 {
        for id in [0, -7, 7, i32::MIN as i64, i32::MAX as i64] {
            for keep in 0..2 {
                let row = next();
                shape(
                    row,
                    "state",
                    "type state id keep present index out_id out_keep",
                );
                inputs(row, &[("state", state), ("id", id), ("keep", keep)]);
            }
        }
    }
    assert_eq!(cursor, rows.len(), "extra records");
    assert_eq!(rows.len(), 2175);
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
    assert_eq!(diagnostics.matches("\"probe\":").count(), 95);
    let rows = validate_structure(text);
    assert_eq!(fingerprint(text), FIXTURE_HASH, "C observations changed");
    rows
}
#[cfg(test)]
pub mod checks {
    use super::*;
    pub fn integrity(text: &str, reference: &str, generator: &str, diagnostics: &str) {
        assert_eq!(
            validate(text, reference, generator, diagnostics).len(),
            2175
        );
    }
    pub fn corruptions(text: &str, reference: &str, generator: &str, diagnostics: &str) {
        let rejected = |t: &str, r: &str, g: &str, d: &str| {
            assert!(std::panic::catch_unwind(|| validate(t, r, g, d)).is_err())
        };
        rejected(
            text,
            &reference.replace("unknown", "claimed"),
            generator,
            diagnostics,
        );
        rejected(
            text,
            reference,
            &generator.replace("schema", "changed"),
            diagnostics,
        );
        rejected(
            text,
            reference,
            generator,
            &diagnostics.replace("5 92 93", "5 11 93"),
        );
        for (from, to) in [
            ("\"schema\":1", "\"schema\":2"),
            ("\"schema\":1", "\"schema\":1,\"extra\":0"),
            ("\"schema\":1", "\"schema\":1,\"schema\":1"),
            ("\"from\":0", "\"from\":5"),
            ("\"value\":0", "\"value\":-1"),
            ("\"value\":0", "\"value\":00"),
            (
                "\"value\":18446744073709551615",
                "\"value\":18446744073709551616",
            ),
            ("\"get\":0", "\"get\":1"),
            ("\"capacity\":0", "\"capacity\":51"),
            (
                "\"values\":\"5 6 7 8 9 10 97 98\"",
                "\"values\":\"5 6 7 8 9 10 0 98\"",
            ),
            ("\"type\":\"state\"", "\"type\":\"unknown\""),
        ] {
            let changed = text.replacen(from, to, 1);
            assert_ne!(changed, text);
            rejected(&changed, reference, generator, diagnostics);
        }
        rejected(text.trim_end(), reference, generator, diagnostics);
        let missing = text
            .lines()
            .skip(1)
            .map(|l| format!("{l}\n"))
            .collect::<String>();
        rejected(&missing, reference, generator, diagnostics);
        let extra = format!("{text}{}\n", text.lines().last().unwrap());
        rejected(&extra, reference, generator, diagnostics);
        let output = text.replacen("\"get\":0", "\"get\":1", 1);
        validate_structure(&output);
        rejected(&output, reference, generator, diagnostics);
    }
}
