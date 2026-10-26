use osmocore_rs::compat::gsmtap::{gsmtap_makemsg, gsmtap_makemsg_ex};
use osmocore_rs::gsmtap::GsmtapMessage;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const LIBOSMOCORE_COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
const EXPECTED_CASES: &[&str] = &[
    "gsmtap_abis_signed_boundaries",
    "gsmtap_makemsg_um_wrapper",
    "gsmtap_sim_atr",
    "gsmtap_um_pcs_empty_payload",
    "gsmtap_um_uplink_only",
    "gsmtap_um_uplink_pcs_boundary",
    "gsmtap_v2_basic_header",
];

struct Vector {
    case: String,
    api: String,
    libosmocore_commit: String,
    input: Input,
    return_code: i64,
    message_created: bool,
    length: usize,
    encoded_hex: String,
}

struct Input {
    message_type: Option<u8>,
    arfcn: u16,
    timeslot: u8,
    channel_type: u8,
    sub_slot: u8,
    frame_number: u32,
    signal_dbm: i8,
    snr_db: i8,
    payload_hex: String,
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
        api: required_string(json, "api"),
        libosmocore_commit: required_string(json, "libosmocore_commit"),
        input: Input {
            message_type: json_value(json, "type").map(|value| {
                value
                    .parse()
                    .expect("fixture type must be an unsigned byte")
            }),
            arfcn: required_number(json, "arfcn"),
            timeslot: required_number(json, "timeslot"),
            channel_type: required_number(json, "channel_type"),
            sub_slot: required_number(json, "sub_slot"),
            frame_number: required_number(json, "frame_number"),
            signal_dbm: required_number(json, "signal_dbm"),
            snr_db: required_number(json, "snr_db"),
            payload_hex: required_string(json, "payload_hex"),
        },
        return_code: required_number(json, "return_code"),
        message_created: json_value(json, "message_created") == Some("true"),
        length: required_number(json, "length"),
        encoded_hex: required_string(json, "encoded_hex"),
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
fn selected_c_constructor_vectors_match_byte_for_byte() {
    let vector_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compatibility/vectors");
    let mut observed_cases = BTreeSet::new();

    for entry in fs::read_dir(vector_dir).expect("committed vector directory must be readable") {
        let path = entry
            .expect("vector directory entry must be readable")
            .path();
        let is_gsmtap_fixture = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.starts_with("gsmtap_"),
            None => false,
        };
        if !is_gsmtap_fixture {
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }

        let contents = fs::read_to_string(&path).expect("committed vector must be readable");
        let vector = parse_vector(&contents);
        assert!(observed_cases.insert(vector.case.clone()), "duplicate case");
        assert!(EXPECTED_CASES.contains(&vector.case.as_str()));
        assert_eq!(vector.libosmocore_commit, LIBOSMOCORE_COMMIT);
        assert_eq!(vector.return_code, 0);
        assert!(vector.message_created);

        let payload = decode_hex(&vector.input.payload_hex);
        let encoded = match vector.api.as_str() {
            "gsmtap_makemsg_ex" => gsmtap_makemsg_ex(
                vector
                    .input
                    .message_type
                    .expect("extended input must have type"),
                vector.input.arfcn,
                vector.input.timeslot,
                vector.input.channel_type,
                vector.input.sub_slot,
                vector.input.frame_number,
                vector.input.signal_dbm,
                vector.input.snr_db,
                &payload,
            ),
            "gsmtap_makemsg" => gsmtap_makemsg(
                vector.input.arfcn,
                vector.input.timeslot,
                vector.input.channel_type,
                vector.input.sub_slot,
                vector.input.frame_number,
                vector.input.signal_dbm,
                vector.input.snr_db,
                &payload,
            ),
            other => panic!("unsupported fixture API: {other}"),
        };
        let expected = decode_hex(&vector.encoded_hex);
        assert_eq!(expected.len(), vector.length);
        let message = GsmtapMessage {
            message_type: vector
                .input
                .message_type
                .unwrap_or(osmocore_rs::gsmtap::GSMTAP_TYPE_UM),
            arfcn: vector.input.arfcn,
            timeslot: vector.input.timeslot,
            channel_type: vector.input.channel_type,
            sub_slot: vector.input.sub_slot,
            frame_number: vector.input.frame_number,
            signal_dbm: vector.input.signal_dbm,
            snr_db: vector.input.snr_db,
            payload: &payload,
        };
        assert_eq!(message.encode(), expected, "preferred API: {}", vector.case);
        assert_eq!(
            message.encode_msgb().unwrap().data(),
            expected,
            "Msgb API: {}",
            vector.case
        );
        if vector.api == "gsmtap_makemsg" {
            let mut um = GsmtapMessage::um(&payload);
            um.arfcn = message.arfcn;
            um.timeslot = message.timeslot;
            um.channel_type = message.channel_type;
            um.sub_slot = message.sub_slot;
            um.frame_number = message.frame_number;
            um.signal_dbm = message.signal_dbm;
            um.snr_db = message.snr_db;
            assert_eq!(um.encode(), expected);
            assert_eq!(um.encode_msgb().unwrap().data(), expected);
        }
        assert_eq!(encoded, expected, "C fixture differs for {}", vector.case);
    }

    assert_eq!(
        observed_cases,
        EXPECTED_CASES.iter().map(ToString::to_string).collect()
    );
}
