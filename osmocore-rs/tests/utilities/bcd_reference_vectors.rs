use osmocore_rs::compat::bcd::{osmo_bcd2str, osmo_char2bcd, osmo_str2bcd};
use std::collections::BTreeSet;

use osmocore_rs::bcd::{BcdError, nibble_to_char};

const LIBOSMOCORE_COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
const BUFFER_CASES: &[&str] = &[
    "decode_low_first",
    "decode_odd_start",
    "decode_hex_allowed",
    "decode_hex_disallowed",
    "decode_filler_disallowed",
    "decode_exact_fit",
    "decode_oversized",
    "decode_truncated",
    "decode_invalid_after_truncation",
    "decode_invalid_before_truncation",
    "decode_one_byte",
    "decode_zero_capacity",
    "decode_empty_range",
    "decode_inverted_range",
    "decode_last_nibble",
    "encode_auto_even",
    "encode_auto_odd_filler",
    "encode_prefix_auto_filler",
    "encode_explicit_filler",
    "encode_explicit_odd_end",
    "encode_empty_range",
    "encode_inverted_range",
    "encode_upper_hex",
    "encode_lower_hex",
    "encode_hex_disallowed_first",
    "encode_hex_disallowed_after_write",
    "encode_invalid_first",
    "encode_invalid_after_write",
    "encode_embedded_nul",
    "encode_exact_fit",
    "encode_oversized",
    "encode_too_small",
    "encode_zero_capacity",
    "encode_last_nibble",
    "encode_empty_auto",
];

fn json_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let key = format!("\"{key}\":");
    let value = json.split_once(&key)?.1;
    if let Some(value) = value.strip_prefix('"') {
        return value.split_once('"').map(|(value, _)| value);
    }
    value.split([',', '}']).next().map(str::trim)
}

fn required<'a>(json: &'a str, key: &str) -> &'a str {
    json_value(json, key).unwrap_or_else(|| panic!("fixture must contain {key}"))
}

fn number<T: std::str::FromStr>(json: &str, key: &str) -> T {
    required(json, key)
        .parse()
        .unwrap_or_else(|_| panic!("fixture {key} must be a number"))
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0);
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

fn c_result(result: Result<usize, BcdError>) -> i32 {
    match result {
        Ok(count) => i32::try_from(count).expect("fixture count fits C int"),
        Err(BcdError::InvalidDigit) => -22,
        Err(BcdError::NoSpace) => -12,
        Err(BcdError::OutOfRange) => panic!("C fixture must use a safe slice range"),
    }
}

#[test]
fn c_bcd_vectors_match_return_and_complete_destination() {
    let fixture = include_str!("../compatibility/vectors/bcd_conversion_vectors.json");
    let mut observed_cases = BTreeSet::new();
    let mut expected_cases: BTreeSet<String> =
        BUFFER_CASES.iter().map(ToString::to_string).collect();
    for digit in 0..16 {
        expected_cases.insert(format!("bcd2char_{digit:x}"));
        expected_cases.insert(format!("char2bcd_{digit:x}"));
    }
    for digit in 10..16 {
        expected_cases.insert(format!("char2bcd_lower_{digit:x}"));
    }
    for name in ["char2bcd_invalid", "char2bcd_nul", "char2bcd_high_bit"] {
        expected_cases.insert(name.to_owned());
    }

    for line in fixture
        .lines()
        .filter(|line| line.starts_with("{\"case\":"))
    {
        let case = required(line, "case");
        assert!(
            observed_cases.insert(case.to_owned()),
            "duplicate case: {case}"
        );
        assert!(expected_cases.contains(case), "unexpected case: {case}");
        assert_eq!(required(line, "libosmocore_commit"), LIBOSMOCORE_COMMIT);

        let api = required(line, "api");
        let source = decode_hex(required(line, "src_hex"));
        let expected_return: i32 = number(line, "return_code");
        if api == "osmo_bcd2char" || api == "osmo_char2bcd" {
            assert_eq!(source.len(), 1, "{case} source byte");
            let actual = if api == "osmo_bcd2char" {
                i32::from(nibble_to_char(source[0]).expect("single BCD nibble"))
            } else {
                i32::from(osmo_char2bcd(source[0]))
            };
            assert_eq!(actual, expected_return, "{case} return code");
            continue;
        }

        let mut destination = decode_hex(required(line, "dst_before_hex"));
        let destination_len: usize = number(line, "dst_len");
        assert_eq!(destination.len(), destination_len, "{case} initial size");
        let start_nibble: usize = number(line, "start_nibble");
        let end_nibble: i32 = number(line, "end_nibble");
        let allow_hex = required(line, "allow_hex") == "true";
        let result = match api {
            "osmo_bcd2str" => {
                assert!(end_nibble >= 0, "{case} decode end");
                c_result(osmo_bcd2str(
                    &mut destination,
                    &source,
                    start_nibble,
                    end_nibble as usize,
                    allow_hex,
                ))
            }
            "osmo_str2bcd" => {
                assert!(source.contains(&0), "{case} C source terminator");
                c_result(osmo_str2bcd(
                    &mut destination,
                    &source,
                    start_nibble,
                    usize::try_from(end_nibble).ok(),
                    allow_hex,
                ))
            }
            other => panic!("unsupported fixture API: {other}"),
        };
        assert_eq!(result, expected_return, "{case} return code");
        assert_eq!(
            destination,
            decode_hex(required(line, "dst_after_hex")),
            "{case} destination"
        );
    }

    assert_eq!(observed_cases, expected_cases);
}

#[test]
fn preferred_owned_bcd_replays_the_nontruncating_frozen_domain() {
    use osmocore_rs::bcd;

    let fixture = include_str!("../compatibility/vectors/bcd_conversion_vectors.json");
    let mut compared = BTreeSet::new();
    for line in fixture
        .lines()
        .filter(|line| line.starts_with("{\"case\":"))
    {
        let case = required(line, "case");
        let source = decode_hex(required(line, "src_hex"));
        let result: i32 = number(line, "return_code");
        if required(line, "api") == "osmo_char2bcd" && source[0].is_ascii_hexdigit() {
            assert_eq!(bcd::char_to_nibble(source[0]), Ok(result as u8), "{case}");
            compared.insert(case.to_owned());
            continue;
        }
        if !matches!(required(line, "api"), "osmo_bcd2str" | "osmo_str2bcd") {
            continue;
        }
        let start: usize = number(line, "start_nibble");
        let end: i32 = number(line, "end_nibble");
        let capacity: usize = number(line, "dst_len");
        let allow_hex = required(line, "allow_hex") == "true";
        let expected = decode_hex(required(line, "dst_after_hex"));
        match required(line, "api") {
            "osmo_bcd2str" if end >= 0 && capacity > (end as usize).saturating_sub(start) => {
                let actual = bcd::decode(&source, start, end as usize, allow_hex);
                if result >= 0 {
                    assert_eq!(
                        actual.unwrap().as_bytes(),
                        &expected[..result as usize],
                        "{case}"
                    );
                } else {
                    assert_eq!(actual, Err(BcdError::InvalidDigit), "{case}");
                }
            }
            "osmo_str2bcd" if start == 0 && end == -1 && result >= 0 => {
                let nul = source.iter().position(|&b| b == 0).unwrap();
                if nul + 1 != source.len() {
                    continue;
                }
                let digits = std::str::from_utf8(&source[..nul]).unwrap();
                assert_eq!(
                    bcd::encode(digits, allow_hex).unwrap(),
                    expected[..result as usize],
                    "{case}"
                );
            }
            _ => continue,
        }
        compared.insert(case.to_owned());
    }
    assert_eq!(compared.len(), 39);
}
