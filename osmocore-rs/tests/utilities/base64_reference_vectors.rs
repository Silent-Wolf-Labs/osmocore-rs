use osmocore_rs::compat::base64::{osmo_base64_decode, osmo_base64_encode};
use std::collections::BTreeSet;

const LIBOSMOCORE_COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
const OLEN_SENTINEL: usize = 0xdecafbad;
const EXPECTED_CASES: &[&str] = &[
    "encode_empty",
    "encode_one_byte_padding",
    "encode_two_byte_padding",
    "encode_three_bytes",
    "encode_multi_block",
    "encode_upstream_test_vector",
    "encode_size_probe",
    "encode_too_small",
    "decode_padded",
    "decode_unpadded",
    "decode_trailing_spaces",
    "decode_newline",
    "decode_crlf",
    "decode_space_inside_quartet",
    "decode_invalid_character",
    "decode_misplaced_padding",
    "decode_excessive_padding",
    "decode_high_bit_character",
    "decode_null_size_probe",
    "decode_zero_size_probe",
    "decode_too_small",
    "decode_empty",
    "decode_spaces_only",
    "decode_newlines_only",
];

struct Vector {
    case: String,
    operation: String,
    libosmocore_commit: String,
    src_hex: String,
    dst_present: bool,
    dst_len: usize,
    return_code: i32,
    olen: usize,
    dst_after_hex: String,
}

fn json_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let key = format!("\"{key}\":");
    let value = json.split_once(&key)?.1;
    if let Some(value) = value.strip_prefix('\"') {
        return value.split_once('\"').map(|(value, _)| value);
    }
    value
        .split([',', '}'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_string(json: &str, key: &str) -> String {
    json_value(json, key)
        .unwrap_or_else(|| panic!("fixture must contain {key}"))
        .to_owned()
}

fn required_number<T: std::str::FromStr>(json: &str, key: &str) -> T {
    json_value(json, key)
        .unwrap_or_else(|| panic!("fixture must contain {key}"))
        .parse()
        .unwrap_or_else(|_| panic!("fixture {key} must be a number"))
}

fn parse_vector(json: &str) -> Vector {
    Vector {
        case: required_string(json, "case"),
        operation: required_string(json, "operation"),
        libosmocore_commit: required_string(json, "libosmocore_commit"),
        src_hex: required_string(json, "src_hex"),
        dst_present: json_value(json, "dst_present") == Some("true"),
        dst_len: required_number(json, "dst_len"),
        return_code: required_number(json, "return_code"),
        olen: required_number(json, "olen"),
        dst_after_hex: required_string(json, "dst_after_hex"),
    }
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "hex input must have whole bytes");
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn selected_c_base64_vectors_match_exactly() {
    let fixture = include_str!("../compatibility/vectors/base64_buffer_vectors.json");
    let mut observed_cases = BTreeSet::new();

    for object in fixture.split("{\"case\":").skip(1) {
        let vector = parse_vector(&format!("{{\"case\":{object}"));
        assert!(observed_cases.insert(vector.case.clone()), "duplicate case");
        assert!(EXPECTED_CASES.contains(&vector.case.as_str()));
        assert_eq!(vector.libosmocore_commit, LIBOSMOCORE_COMMIT);

        let source = decode_hex(&vector.src_hex);
        let mut destination = vec![0xa5; vector.dst_len];
        let mut olen = OLEN_SENTINEL;
        let return_code = match vector.operation.as_str() {
            "encode" => osmo_base64_encode(&mut destination, &mut olen, &source),
            "decode" if vector.dst_present => {
                osmo_base64_decode(Some(&mut destination), &mut olen, &source)
            }
            "decode" => osmo_base64_decode(None, &mut olen, &source),
            other => panic!("unsupported fixture operation: {other}"),
        };

        assert_eq!(
            return_code, vector.return_code,
            "{} return code",
            vector.case
        );
        assert_eq!(olen, vector.olen, "{} reported length", vector.case);
        assert_eq!(
            destination,
            decode_hex(&vector.dst_after_hex),
            "{} destination",
            vector.case
        );
    }

    assert_eq!(
        observed_cases,
        EXPECTED_CASES.iter().map(ToString::to_string).collect()
    );
}

#[test]
fn preferred_base64_apis_replay_frozen_buffers_with_rust_length_contracts() {
    use osmocore_rs::base64::{self, Base64Error};

    let fixture = include_str!("../compatibility/vectors/base64_buffer_vectors.json");
    let mut compared = BTreeSet::new();
    for object in fixture.split("{\"case\":").skip(1) {
        let vector = parse_vector(&format!("{{\"case\":{object}"));
        let source = decode_hex(&vector.src_hex);
        let expected = decode_hex(&vector.dst_after_hex);
        match vector.operation.as_str() {
            "encode" if vector.return_code == 0 => {
                let encoded = &expected[..vector.olen];
                assert_eq!(base64::encode(&source).unwrap().as_bytes(), encoded);
                // Rust accepts exactly the encoded length and writes no C terminator.
                let mut destination = vec![0xa5; vector.olen + 3];
                assert_eq!(
                    base64::encode_into(&mut destination[..vector.olen], &source),
                    Ok(vector.olen),
                    "{}",
                    vector.case
                );
                assert_eq!(&destination[..vector.olen], encoded);
                assert_eq!(&destination[vector.olen..], &[0xa5; 3]);
                if vector.olen > 0 {
                    let mut short = vec![0x7e; vector.olen - 1];
                    assert_eq!(
                        base64::encode_into(&mut short, &source),
                        Err(Base64Error::BufferTooSmall {
                            required: vector.olen
                        })
                    );
                    assert!(short.iter().all(|&b| b == 0x7e));
                }
            }
            "decode" if vector.dst_present => {
                let mut destination = vec![0xa5; vector.dst_len];
                let result = base64::decode_into(&mut destination, &source);
                match vector.return_code {
                    0 => {
                        // C preserves olen for empty/whitespace input; Rust reports zero.
                        let len = if vector.olen == OLEN_SENTINEL {
                            0
                        } else {
                            vector.olen
                        };
                        assert_eq!(result, Ok(len), "{}", vector.case);
                        assert_eq!(base64::decode(&source), Ok(expected[..len].to_vec()));
                    }
                    -22 => {
                        assert_eq!(result, Err(Base64Error::InvalidInput));
                        assert_eq!(base64::decode(&source), Err(Base64Error::InvalidInput));
                    }
                    -105 => assert_eq!(
                        result,
                        Err(Base64Error::BufferTooSmall {
                            required: vector.olen
                        })
                    ),
                    status => panic!("unexpected frozen status {status}"),
                }
                assert_eq!(
                    destination, expected,
                    "{} complete destination",
                    vector.case
                );
            }
            // C probes and failed C encodes have no successful encoded bytes to replay.
            _ => continue,
        }
        compared.insert(vector.case);
    }
    assert_eq!(compared.len(), 21);
}
