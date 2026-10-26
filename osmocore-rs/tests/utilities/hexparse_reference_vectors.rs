use std::collections::BTreeSet;

use osmocore_rs::hexparse::parse_into;

const LIBOSMOCORE_COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
const EXPECTED_CASES: &[&str] = &[
    "empty",
    "empty_zero_capacity",
    "whitespace_only",
    "zero_capacity_whitespace",
    "zero_capacity_digit",
    "lowercase",
    "uppercase",
    "mixed_case",
    "whitespace_between_digits",
    "whitespace_after_full_buffer",
    "exact_fit",
    "oversized_destination",
    "too_small_destination",
    "odd_single_digit",
    "odd_after_complete_byte",
    "invalid_first_character",
    "invalid_after_complete_byte",
    "invalid_after_high_nibble",
    "invalid_after_capacity",
    "embedded_nul",
    "high_bit_character",
];

fn json_value<'a>(json: &'a str, key: &str) -> &'a str {
    let key = format!("\"{key}\":");
    let value = json
        .split_once(&key)
        .unwrap_or_else(|| panic!("fixture must contain {key}"))
        .1;
    if let Some(value) = value.strip_prefix('"') {
        return value.split_once('"').expect("terminated JSON string").0;
    }
    value.split([',', '}']).next().expect("JSON value").trim()
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "hex input must have whole bytes");
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn c_hexparse_vectors_match_return_and_complete_destination() {
    let fixture = include_str!("../compatibility/vectors/hexparse_buffer_vectors.json");
    let mut observed_cases = BTreeSet::new();

    for object in fixture.split("{\"case\":").skip(1) {
        let vector = format!("{{\"case\":{object}");
        let case = json_value(&vector, "case");
        assert!(
            observed_cases.insert(case.to_owned()),
            "duplicate case: {case}"
        );
        assert!(EXPECTED_CASES.contains(&case), "unexpected case: {case}");
        assert_eq!(
            json_value(&vector, "libosmocore_commit"),
            LIBOSMOCORE_COMMIT
        );

        let source = decode_hex(json_value(&vector, "src_hex"));
        assert_eq!(source.last(), Some(&0), "{case} C source terminator");
        let mut destination = decode_hex(json_value(&vector, "dst_before_hex"));
        let destination_len: usize = json_value(&vector, "dst_len").parse().unwrap();
        assert_eq!(destination.len(), destination_len, "{case} initial size");

        let observed_return = match parse_into(&source, &mut destination) {
            Ok(count) => i32::try_from(count).expect("fixture count fits C int"),
            Err(_) => -1,
        };
        let expected_return: i32 = json_value(&vector, "return_code").parse().unwrap();
        assert_eq!(observed_return, expected_return, "{case} return code");
        assert_eq!(
            destination,
            decode_hex(json_value(&vector, "dst_after_hex")),
            "{case} destination"
        );
    }

    assert_eq!(
        observed_cases,
        EXPECTED_CASES.iter().map(ToString::to_string).collect()
    );
}
