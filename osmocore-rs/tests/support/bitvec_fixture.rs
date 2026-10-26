//! Shared fixture schema and frozen input manifest; generates no C outcomes.
use std::collections::{BTreeMap, BTreeSet};

pub const COUNT: usize = 5_944;
const NUMERIC: &[&str] = &[
    "case",
    "c_unsigned_bits",
    "data_len",
    "pattern",
    "cursor_before",
    "position",
    "value",
    "count",
    "return_code",
    "cursor_after",
];
const STRINGS: &[&str] = &[
    "api",
    "libosmocore_commit",
    "source_sha256",
    "header_sha256",
    "test_sha256",
    "before_hex",
    "after_hex",
];
pub type Record = BTreeMap<String, String>;

pub fn number(record: &Record, key: &str) -> usize {
    record[key]
        .parse()
        .expect("unsigned fixture value fits usize")
}

pub fn bytes(hex: &str) -> Vec<u8> {
    assert_eq!(
        hex.len(),
        8,
        "fixture records the entire four-byte backing array"
    );
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    (0..8)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

pub fn parse_record(line: &str, numeric: &[&str], strings: &[&str]) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut record = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').expect("field has a colon");
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        let value = if strings.contains(&key) {
            let text = value.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(!text.contains(['"', '\\']) && !text.chars().any(char::is_control));
            text
        } else {
            assert!(numeric.contains(&key), "unknown field {key}");
            let digits = if key == "return_code" {
                value.strip_prefix('-').unwrap_or(value)
            } else {
                value
            };
            assert!(!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));
            assert!(digits.len() == 1 || !digits.starts_with('0'));
            value.parse::<i64>().expect("fixture integer fits i64");
            value
        };
        assert!(
            record.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate field"
        );
    }
    assert_eq!(
        record.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        numeric.iter().chain(strings).copied().collect()
    );
    record
}

fn expected_inputs() -> Vec<(&'static str, usize, usize, usize, usize, usize, usize)> {
    let mut cases = Vec::new();
    for len in 0..=3 {
        let capacity = len * 8;
        for pattern in 0..3 {
            for cursor in [0, capacity, u32::MAX as usize] {
                for pi in 0..=capacity + 2 {
                    let pos = if pi == capacity + 2 {
                        u32::MAX as usize
                    } else {
                        pi
                    };
                    for api in ["bitvec_get_bit_pos", "bitvec_get_bit_pos_high"] {
                        cases.push((api, len, pattern, cursor, pos, 0, 0));
                    }
                    for value in 0..4 {
                        cases.push(("bitvec_set_bit_pos", len, pattern, cursor, pos, value, 0));
                    }
                }
            }
            for ci in 0..=capacity + 2 {
                let cursor = if ci == capacity + 2 {
                    u32::MAX as usize
                } else {
                    ci
                };
                for value in 0..4 {
                    cases.push(("bitvec_set_bit", len, pattern, cursor, 0, value, 0));
                }
                cases.push(("bitvec_get_bit_high", len, pattern, cursor, 0, 0, 0));
                for count in 0..=6 {
                    cases.push(("bitvec_set_bits", len, pattern, cursor, 0, 0, count));
                }
                for api in ["bitvec_zero", "bitvec_used_bytes", "bitvec_tailroom_bits"] {
                    cases.push((api, len, pattern, cursor, 0, 0, 0));
                }
            }
        }
    }
    for value in 0..4 {
        cases.push(("bit_value_to_char", 0, 0, 0, 0, value, 0));
    }
    assert_eq!(cases.len(), COUNT);
    cases
}

pub fn validated_records(fixture: &str) -> Vec<Record> {
    let body = fixture
        .trim()
        .strip_prefix('[')
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let lines: Vec<_> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), COUNT, "frozen C call count");
    let expected = expected_inputs();
    let mut records = Vec::with_capacity(COUNT);
    for (i, line) in lines.iter().enumerate() {
        let line = if i + 1 == COUNT {
            *line
        } else {
            line.strip_suffix(',').unwrap()
        };
        let record = parse_record(line, NUMERIC, STRINGS);
        assert_eq!(number(&record, "case"), i, "case order");
        assert_eq!(
            record["libosmocore_commit"],
            "950430e829a3dc1d162aa241bc0505745c5a7311"
        );
        assert_eq!(
            record["source_sha256"],
            "789ef706bc0c71f497400f831e920c9efad324dfaea99b13292fca717b4f480e"
        );
        assert_eq!(
            record["header_sha256"],
            "db895f6eb8bfada1d6764248d122128b0a0bb530ec5f4a198953f56f223215b6"
        );
        assert_eq!(
            record["test_sha256"],
            "5b4a1a3f8c5bafd4113f7b4634f3c80061c274356e05c328d270fb40dfb007f5"
        );
        assert_eq!(number(&record, "c_unsigned_bits"), 32);
        let input = (
            record["api"].as_str(),
            number(&record, "data_len"),
            number(&record, "pattern"),
            number(&record, "cursor_before"),
            number(&record, "position"),
            number(&record, "value"),
            number(&record, "count"),
        );
        assert_eq!(input, expected[i], "frozen inputs for case {i}");
        let seed = [0u8, 0xff, 0x2b][input.2];
        assert_eq!(
            bytes(&record["before_hex"]),
            [seed, seed ^ 0xff, 0x2b, 0xa5]
        );
        let after = bytes(&record["after_hex"]);
        // The declared vector excludes the backing suffix. C must not write it.
        assert_eq!(after[input.1..], bytes(&record["before_hex"])[input.1..]);
        assert!(u32::try_from(number(&record, "cursor_after")).is_ok());
        let rc = record["return_code"].parse::<i64>().unwrap();
        assert!((-22..=i64::from(u32::MAX)).contains(&rc));
        records.push(record);
    }
    records
}
