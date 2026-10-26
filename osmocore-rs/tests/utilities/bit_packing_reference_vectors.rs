use std::collections::{BTreeMap, BTreeSet};

use osmocore_rs::bits::{pack_into, unpack_into};

const LIBOSMOCORE_COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
const CASE_COUNT: usize = 9_016;

// The C runner emits flat objects, one per line. Read that fixed format
// strictly, without adding a JSON dependency to this crate's test suite.
fn parse_record(line: &str) -> BTreeMap<String, String> {
    let mut record = BTreeMap::new();
    let mut rest = line.trim().strip_prefix('{').expect("object starts with {");
    rest = rest.strip_suffix('}').expect("object ends with }");

    while !rest.is_empty() {
        rest = rest
            .strip_prefix('"')
            .expect("field name starts with quote");
        let (key, tail) = rest.split_once('"').expect("field name closes");
        rest = tail
            .strip_prefix(':')
            .expect("field name is followed by colon");
        let value;
        let numeric = matches!(key, "num_bits" | "dst_len" | "return_code");
        if let Some(string) = rest.strip_prefix('"') {
            assert!(!numeric, "{key} must be an integer, not a string");
            let (string, tail) = string.split_once('"').expect("string value closes");
            assert!(
                !string.contains('\\') && !string.chars().any(char::is_control),
                "fixture strings must be unescaped printable text"
            );
            value = string;
            rest = tail;
        } else {
            assert!(numeric, "{key} must be a string");
            let end = rest.find(',').unwrap_or(rest.len());
            value = &rest[..end];
            assert!(
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()),
                "{key} must be a nonnegative integer"
            );
            rest = &rest[end..];
        }
        assert!(record.insert(key.to_owned(), value.to_owned()).is_none());
        if rest.is_empty() {
            break;
        }
        rest = rest
            .strip_prefix(',')
            .expect("fields are separated by commas");
        assert!(!rest.is_empty(), "object must not have a trailing comma");
    }
    let fields: BTreeSet<&str> = [
        "case",
        "api",
        "libosmocore_commit",
        "num_bits",
        "src_hex",
        "dst_len",
        "dst_before_hex",
        "return_code",
        "dst_after_hex",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        record.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        fields
    );
    record
}

fn parse_fixture(fixture: &str) -> Vec<BTreeMap<String, String>> {
    let body = fixture
        .trim()
        .strip_prefix('[')
        .expect("fixture starts with [")
        .strip_suffix(']')
        .expect("fixture ends with ]");
    let lines: Vec<_> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let object = if index + 1 == lines.len() {
                *line
            } else {
                line.strip_suffix(',')
                    .expect("array entries are separated by commas")
            };
            parse_record(object)
        })
        .collect()
}

fn expected_case_names() -> Vec<String> {
    let mut names = Vec::new();
    for byte in 0..=255u8 {
        for bits in 1..=8 {
            for api in ["unpack", "pack"] {
                for size in ["exact", "oversized"] {
                    names.push(format!("{api}_byte_{byte:02x}_bits_{bits}_{size}"));
                }
            }
        }
    }
    for bits in [7, 8, 9, 15, 16, 17, 31, 32, 33] {
        let patterns = [
            "zero".to_owned(),
            "one".to_owned(),
            "alternating".to_owned(),
        ]
        .into_iter()
        .chain((0..bits).map(|i| format!("single_{i:02}")));
        for pattern in patterns {
            for api in ["unpack", "pack"] {
                for size in ["exact", "oversized"] {
                    names.push(format!("{api}_boundary_{bits}_{pattern}_{size}"));
                }
            }
        }
        for api in ["unpack", "pack"] {
            for tail in 0..=1 {
                names.push(format!("{api}_extra_bits_{bits}_tail_{tail}"));
            }
        }
    }
    for api in ["unpack", "pack"] {
        for high in 0..=1 {
            for size in ["backed", "oversized"] {
                names.push(format!("{api}_zero_bit_{high}_{size}"));
            }
        }
    }
    assert_eq!(names.len(), CASE_COUNT);
    names
}

fn required<'a>(record: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    record
        .get(key)
        .unwrap_or_else(|| panic!("fixture is missing {key}"))
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "hex must have complete bytes");
    assert!(
        hex.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn bit_conversions_match_all_c_reference_vectors() {
    let fixture = include_str!("../compatibility/vectors/bit_packing_vectors.json");
    let records = parse_fixture(fixture);
    let names = expected_case_names();
    assert_eq!(records.len(), CASE_COUNT, "complete frozen C fixture");

    for (record, expected_name) in records.iter().zip(&names) {
        let case = required(record, "case");
        assert_eq!(case, expected_name, "frozen case name and order");
        assert_eq!(
            required(record, "libosmocore_commit"),
            LIBOSMOCORE_COMMIT,
            "{case} provenance"
        );
        let bits: usize = required(record, "num_bits").parse().expect("bit count");
        let source = decode_hex(required(record, "src_hex"));
        let mut destination = decode_hex(required(record, "dst_before_hex"));
        let destination_len: usize = required(record, "dst_len")
            .parse()
            .expect("destination length");
        assert_eq!(
            destination.len(),
            destination_len,
            "{case} destination size"
        );
        let expected_api = if case.starts_with("pack_") {
            "osmo_ubit2pbit"
        } else {
            "osmo_pbit2ubit"
        };
        assert_eq!(required(record, "api"), expected_api, "{case} API");

        let result = match required(record, "api") {
            "osmo_ubit2pbit" => pack_into(&mut destination, &source, bits),
            "osmo_pbit2ubit" => unpack_into(&mut destination, &source, bits),
            api => panic!("unexpected API in {case}: {api}"),
        }
        .expect("fixture calls must have sufficient Rust slice capacity");
        let expected_result: usize = required(record, "return_code")
            .parse()
            .expect("C return value");
        assert_eq!(result, expected_result, "{case} return value");
        assert_eq!(
            destination,
            decode_hex(required(record, "dst_after_hex")),
            "{case} complete destination"
        );
    }
}

#[test]
fn fixture_reader_rejects_malformed_or_unexpected_records() {
    let fixture = include_str!("../compatibility/vectors/bit_packing_vectors.json");
    let first = fixture.lines().nth(1).unwrap().strip_suffix(',').unwrap();
    for record in [
        first.replace("\"num_bits\":1", "\"num_bits\":\"1\""),
        first.replace("\"num_bits\":1", "\"num_bits\":1.0"),
        first.replacen('{', "{\"unexpected\":\"value\",", 1),
        first.replace("\"dst_len\":1,", ""),
        first.replace('}', ",}"),
        "null".to_owned(),
    ] {
        let fixture = format!("[\n{record}\n]");
        assert!(std::panic::catch_unwind(|| parse_fixture(&fixture)).is_err());
    }
}
