//! Shared evidence reader: validates C fixtures without implementing time APIs.
use std::collections::BTreeMap;

pub type Record = BTreeMap<String, String>;
pub fn number(row: &Record, key: &str) -> i64 {
    row[key].parse().unwrap()
}
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
// Frozen only from reviewed direct C output and provenance, never Rust results.
const FIXTURE_HASH: u64 = 0x4dbf606dc8c53412;
const REFERENCE_HASH: u64 = 0xd015d75caf116081;
const GENERATOR_HASH: u64 = 0xf1893904528f8be6;
const REFERENCE_SHA: &str = "121fc015fbf3cb02918545f9f4a0a6d273aee4e53b20f7280822b1865485b204";
const GENERATOR_SHA: &str = "a595c39014c53401b45337bd8077a046ddfd1a355a524f74ac24a070a6d2d2c4";

fn parse(line: &str) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut row = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').unwrap();
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        assert!(!key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let string_field = matches!(
            key,
            "type" | "case" | "event" | "op" | "evidence" | "revision" | "reference" | "generator"
        );
        let value = if string_field {
            let text = value.strip_prefix('"').unwrap();
            let text = text.strip_suffix('"').unwrap();
            assert!(!text.contains(['"', '\\']) && !text.chars().any(char::is_control));
            text
        } else {
            assert_eq!(
                value.parse::<i64>().unwrap().to_string(),
                value,
                "noncanonical number"
            );
            value
        };
        assert!(
            row.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate key"
        );
    }
    row
}
fn shape(row: &Record, keys: &str, numeric: &str) {
    let mut expected: Vec<_> = keys.split_whitespace().collect();
    expected.sort_unstable();
    assert_eq!(row.keys().map(String::as_str).collect::<Vec<_>>(), expected);
    for key in numeric.split_whitespace() {
        assert_eq!(
            number(row, key).to_string(),
            row[key],
            "numeric field {key}"
        );
    }
}

pub const SECONDS: [i64; 3] = [-3, 0, 4];
pub const FRACTIONS: [i64; 12] = [
    -2_000_001, -2_000_000, -1_000_001, -1_000_000, -1, 0, 1, 999_999, 1_000_000, 1_000_001,
    2_000_000, 2_000_001,
];
fn nanoseconds(value: i64) -> i64 {
    if value == 999_999 {
        999_999_999
    } else {
        value / 1_000_000 * 1_000_000_000 + value % 1_000_000
    }
}
fn pair_shape(row: &Record) {
    shape(
        row,
        "type id op as af bs bf rs rf ret",
        "id as af bs bf rs rf ret",
    );
}

/// Check schema and exact ordered inputs independently of the frozen digest.
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
        "type schema evidence revision reference generator time_bits long_bits int_bits embedded",
        "schema time_bits long_bits int_bits embedded",
    );
    assert_eq!(meta["type"], "meta");
    assert_eq!(meta["evidence"], "exact-source");
    assert_eq!(meta["revision"], "unknown");
    assert_eq!(meta["reference"], REFERENCE_SHA);
    assert_eq!(meta["generator"], GENERATOR_SHA);
    for (key, value) in [
        ("schema", 1),
        ("time_bits", 64),
        ("long_bits", 64),
        ("int_bits", 32),
        ("embedded", 0),
    ] {
        assert_eq!(number(meta, key), value);
    }
    for op in ["initial", "read", "retained"] {
        let row = next();
        pair_shape(row);
        assert_eq!(row["type"], "wall");
        assert_eq!(number(row, "id"), 0);
        assert_eq!(row["op"], op);
    }
    for id in 0..8 {
        for op in ["initial", "read", "disabled_slot", "retained"] {
            let row = next();
            pair_shape(row);
            assert_eq!(row["type"], "clock");
            assert_eq!(number(row, "id"), id);
            assert_eq!(row["op"], op);
        }
    }
    for id in 0..8 {
        let row = next();
        pair_shape(row);
        assert_eq!(row["type"], "clock");
        assert_eq!(number(row, "id"), id);
        assert_eq!(row["op"], "independent");
    }
    let row = next();
    pair_shape(row);
    assert_eq!(row["type"], "clock");
    assert_eq!(number(row, "id"), i64::from(i32::MAX));
    assert_eq!(row["op"], "unsupported");
    for kind in ["timeval", "timespec"] {
        let mut id = 0;
        for as_ in SECONDS {
            for bs in SECONDS {
                for af in FRACTIONS {
                    for bf in FRACTIONS {
                        let (af, bf) = if kind == "timespec" {
                            (nanoseconds(af), nanoseconds(bf))
                        } else {
                            (af, bf)
                        };
                        for op in [
                            "add",
                            "sub",
                            "alias_add",
                            "alias_sub",
                            "cmp",
                            "isset",
                            "clear",
                            "override_add",
                        ] {
                            let row = next();
                            pair_shape(row);
                            assert_eq!(row["type"], kind);
                            assert_eq!(row["op"], op);
                            for (key, value) in
                                [("id", id), ("as", as_), ("af", af), ("bs", bs), ("bf", bf)]
                            {
                                assert_eq!(number(row, key), value, "coverage field {key}");
                            }
                        }
                        id += 1;
                    }
                }
            }
        }
    }
    for (id, seconds) in [
        i64::MIN,
        -31_536_001,
        -86_401,
        -3_601,
        -61,
        -1,
        0,
        1,
        59,
        60,
        3_599,
        3_600,
        86_399,
        86_400,
        31_536_000,
        i64::MAX,
    ]
    .into_iter()
    .enumerate()
    {
        for op in ["sec2min", "sec2hrs", "sec2day"] {
            let row = next();
            pair_shape(row);
            assert_eq!(row["type"], "seconds");
            assert_eq!(row["op"], op);
            assert_eq!(number(row, "id"), id as i64);
            assert_eq!(number(row, "as"), seconds);
            for key in ["af", "bs", "bf", "rf", "ret"] {
                assert_eq!(number(row, key), 0);
            }
        }
    }
    // Frozen event identities protect scenario coverage without a scheduler model.
    const EVENTS: &[(&str, &str)] = &[
        (
            "batch_order",
            "before:-1:0 callback:1:0 callback:0:0 callback:2:0 after:-1:0 second:-1:0 clean:-1:0",
        ),
        (
            "nested_empty",
            "before:-1:0 callback:0:0 nested_enter:0:1 nested_exit:0:1 after:-1:0 second:-1:0 clean:-1:0",
        ),
        (
            "nested_immediate",
            "before:-1:0 callback:0:0 nested_enter:0:1 callback:2:1 nested_exit:0:1 after:-1:0 second:-1:0 clean:-1:0",
        ),
        (
            "prepare_during_dispatch",
            "before:-1:0 callback:0:0 prepared:0:0 prepared_future:0:0 after:-1:0 second:-1:0 clean:-1:0",
        ),
        (
            "self_reschedule",
            "before:-1:0 callback:0:0 after:-1:0 callback:0:0 second:-1:0 clean:-1:0",
        ),
        (
            "cancel_peer",
            "before:-1:0 callback:0:0 after:-1:0 second:-1:0 clean:-1:0",
        ),
        (
            "reschedule_peer",
            "before:-1:0 callback:0:0 after:-1:0 callback:1:0 second:-1:0 clean:-1:0",
        ),
        (
            "immediate_deferred",
            "before:-1:0 callback:0:0 after:-1:0 callback:2:0 second:-1:0 clean:-1:0",
        ),
        (
            "replace_self",
            "before:-1:0 callback:0:0 after:-1:0 replacement:0:0 second:-1:0 clean:-1:0",
        ),
        (
            "replace_peer",
            "before:-1:0 callback:0:0 replacement:1:0 after:-1:0 second:-1:0 clean:-1:0",
        ),
    ];
    for (case, events) in EVENTS {
        for event in events.split_whitespace() {
            let row = next();
            shape(
                row,
                "type case event who depth ret count active near sec frac",
                "who depth ret count active near sec frac",
            );
            assert_eq!(row["type"], "timer");
            assert_eq!(row["case"], *case);
            assert_eq!(
                format!("{}:{}:{}", row["event"], row["who"], row["depth"]),
                event
            );
            assert!((0..16).contains(&number(row, "active")));
            assert!((0..=4).contains(&number(row, "count")));
            assert_eq!(
                number(row, "active").count_ones(),
                number(row, "count") as u32
            );
            assert!((0..=1).contains(&number(row, "near")));
        }
    }
    let inputs = [
        (100, 500000),
        (100, 499999),
        (100, 500001),
        (99, 1500000),
        (101, -500000),
        (0, 2000001),
        (-1, -2000001),
        (i64::MAX, 0),
    ];
    for (id, (seconds, fraction)) in inputs.into_iter().enumerate() {
        for op in ["remaining", "remaining_clock", "schedule"] {
            let row = next();
            pair_shape(row);
            assert_eq!(row["type"], "timer_api");
            assert_eq!(row["op"], op);
            for (key, value) in [
                ("id", id as i64),
                (
                    "as",
                    if op == "schedule" && id == 7 {
                        0
                    } else {
                        seconds
                    },
                ),
                ("af", fraction),
                ("bs", 100),
                ("bf", 500000),
            ] {
                assert_eq!(number(row, key), value);
            }
        }
    }
    for (id, (seconds, fraction)) in [
        (100, 500000),
        (100, 500001),
        (100, 500999),
        (100, 501000),
        (100, 501001),
        (101, 500001),
        (2147584, 147000),
        (101, 1500001),
    ]
    .into_iter()
    .enumerate()
    {
        let row = next();
        pair_shape(row);
        assert_eq!(row["type"], "timer_api");
        assert_eq!(row["op"], "nearest");
        for (key, value) in [
            ("id", id as i64),
            ("as", seconds),
            ("af", fraction),
            ("bs", 100),
            ("bf", 500000),
        ] {
            assert_eq!(number(row, key), value);
        }
    }
    for event in [
        "empty",
        "max_nearest",
        "max_dispatch",
        "future",
        "readd",
        "deleted_cache",
        "null_dispatch",
        "clean",
    ] {
        let row = next();
        shape(
            row,
            "type case event who depth ret count active near sec frac",
            "who depth ret count active near sec frac",
        );
        assert_eq!(row["type"], "timer");
        assert_eq!(row["case"], "absolute_lifecycle");
        assert_eq!(row["event"], event);
        assert_eq!(number(row, "who"), -1);
        assert_eq!(number(row, "depth"), 0);
        assert!((0..=1).contains(&number(row, "active")));
        assert_eq!(number(row, "active"), number(row, "count"));
        assert!((0..=1).contains(&number(row, "near")));
    }
    assert_eq!(cursor, rows.len(), "extra records");
    rows
}

pub fn validate(text: &str, reference: &str, generator: &str) -> Vec<Record> {
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
    let rows = validate_structure(text);
    assert_eq!(fingerprint(text), FIXTURE_HASH, "C observations changed");
    rows
}

#[cfg(test)]
pub mod checks {
    use super::*;
    pub fn integrity(text: &str, reference: &str, generator: &str) {
        assert_eq!(validate(text, reference, generator).len(), 20_933);
    }

    pub fn corruptions(text: &str, reference: &str, generator: &str) {
        let rejected = |text: &str, reference: &str, generator: &str| {
            assert!(std::panic::catch_unwind(|| validate(text, reference, generator)).is_err());
        };
        rejected(text, &reference.replace("unknown", "claimed"), generator);
        rejected(text, reference, &generator.replace("schema", "changed"));
        for (from, to) in [
            ("\"schema\":1", "\"schema\":2"),
            ("\"schema\":1", "\"schema\":\"1\""),
            ("\"schema\":1", "\"schema\":1,\"schema\":1"),
            ("\"schema\":1", "\"schema\":1,\"extra\":0"),
            ("\"revision\":\"unknown\"", "\"revision\":\"claimed\""),
            ("\"as\":-3", "\"as\":-4"),
            ("\"as\":-3", "\"as\":-03"),
            ("\"as\":-3", "\"as\":9223372036854775808"),
            ("\"op\":\"add\"", "\"op\":\"sub\""),
            ("\"case\":\"nested_empty\"", "\"case\":\"missing_case\""),
            ("\"event\":\"nested_exit\"", "\"event\":\"nested_enter\""),
            ("\"near\":1", "\"near\":2"),
            ("\"rs\":23", "\"rs\":24"),
        ] {
            let altered = text.replacen(from, to, 1);
            assert_ne!(altered, text, "unused corruption {from}");
            rejected(&altered, reference, generator);
        }
        rejected(text.trim_end(), reference, generator);
        rejected(
            &format!("{text}{}\n", text.lines().last().unwrap()),
            reference,
            generator,
        );
        let missing = text
            .lines()
            .enumerate()
            .filter(|(i, _)| *i != 100)
            .map(|(_, line)| format!("{line}\n"))
            .collect::<String>();
        rejected(&missing, reference, generator);
        // Output-only edits pass shape checks and fail the frozen C fingerprint.
        let altered = text.replacen("\"rs\":23", "\"rs\":24", 1);
        validate_structure(&altered);
        rejected(&altered, reference, generator);
    }
}
