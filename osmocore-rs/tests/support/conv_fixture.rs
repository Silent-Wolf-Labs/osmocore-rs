//! Shared strict reader for direct C convolutional observations.
//! This module validates evidence; it does not implement either decoder.
use std::collections::{BTreeMap, BTreeSet};

pub type Record = BTreeMap<String, String>;

pub fn unsigned(record: &Record, key: &str) -> u32 {
    record[key].parse().unwrap()
}

pub fn signed(record: &Record, key: &str) -> i32 {
    record[key].parse().unwrap()
}

pub fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0, "odd hex length");
    assert!(
        text.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn parse(line: &str, unsigned: &[&str], signed: &[&str], strings: &[&str]) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut record = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').unwrap();
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        let value = if strings.contains(&key) {
            let value = value.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(!value.contains(['"', '\\']) && !value.chars().any(char::is_control));
            value
        } else if signed.contains(&key) {
            assert_eq!(value.parse::<i32>().unwrap().to_string(), value);
            value
        } else {
            assert!(unsigned.contains(&key), "unknown field {key}");
            assert_eq!(value.parse::<u32>().unwrap().to_string(), value);
            value
        };
        assert!(record.insert(key.to_owned(), value.to_owned()).is_none());
    }
    record
}

pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

pub const FIXTURE_FINGERPRINT: u64 = 0x8f2c0427eb58d013;
const CASE_FINGERPRINT: u64 = 0xc8bf7e357d2c5f2d;
const BACKENDS: [&str; 4] = ["linked-dispatch", "scalar", "sse", "avx"];
const REFERENCE_SHA: &str = "dcc2ab8e2341b5c8ccd7942ac79b34f977d477cdd1c4a95a949a666a963d8067";
const GENERATOR_SHA: &str = "ea292ede99d0b7143fe7c4c78409da48733dede5bf673a516657a86e9dff41b8";
const LIBRARY_SHA: &str = "f452c264ec62f77420e96bfdf3c66a64b40ba025f1c8629ca8c1244bae45bf40";
const HARNESS_SHA: &str = "f4ba52b97c1dc2ed12eb620a17cea07e3ee9020edb88be19f6fd4dda76fc9203";
const RUNNER_SHA: &str = "7145c28c8681aa7bc08b83c3728bd6af369429593dcb4bf0535c5168156f84a5";
const STRINGS: &[&str] = &[
    "type",
    "backend",
    "runtime",
    "upstream_git_revision",
    "manifest_sha256",
    "library_sha256",
    "harness_sha256",
    "runner_sha256",
    "generator_manifest_sha256",
    "compiler",
    "id",
    "next_output",
    "next_state",
    "next_term_output",
    "next_term_state",
    "puncture",
    "case",
    "code",
    "api",
    "input_before",
    "input_after",
    "output_before",
    "output_after",
    "ae",
    "ae_next",
    "history",
];
const UNSIGNED: &[&str] = &[
    "schema",
    "neon_executed",
    "n",
    "k",
    "len",
    "term",
    "puncture_present",
    "seq",
    "state",
    "i_idx",
    "p_idx",
    "n_states",
    "o_idx",
    "ae_next_valid",
    "storage_released",
    "has_flush",
    "soft_mode",
];
const SIGNED: &[&str] = &["length_arg", "start_state", "end_state", "ret"];
const BUFFERS: &str = "input_before input_after output_before output_after ret";
const ENCODER: &str = "state i_idx p_idx";
const DECODER: &str = "len n_states o_idx p_idx ae ae_next_valid ae_next history";
const API_COUNTS: [(&str, usize); 17] = [
    ("get_input_length", 5097),
    ("get_output_length", 5097),
    ("encode", 1699),
    ("encode_init", 728),
    ("encode_load_state", 460),
    ("encode_raw", 720),
    ("encode_flush", 720),
    ("decode_init", 664),
    ("decode_reset", 1357),
    ("decode_deinit", 664),
    ("decode_scan", 1328),
    ("decode_flush", 228),
    ("decode_get_best_end_state", 797),
    ("decode_get_output", 1891),
    ("decode_rewind", 332),
    ("decode", 6608),
    ("decode_acc", 6796),
];

fn shape(record: &Record) -> Vec<&str> {
    let mut groups = vec!["type backend"];
    match record["type"].as_str() {
        "meta" => groups.push("schema runtime upstream_git_revision manifest_sha256 library_sha256 harness_sha256 runner_sha256 generator_manifest_sha256 compiler neon_executed"),
        "code" => groups.push("id n k len term next_output next_state next_term_output next_term_state puncture_present puncture"),
        "op" => {
            groups.push("case code seq api");
            match record["api"].as_str() {
                "get_input_length" | "get_output_length" => groups.push("length_arg ret"),
                "encode" => groups.push(BUFFERS),
                "encode_init" => groups.push(ENCODER),
                "encode_load_state" => groups.extend([BUFFERS, ENCODER]),
                "encode_raw" => groups.extend(["n", BUFFERS, ENCODER]),
                "encode_flush" => groups.extend([BUFFERS, ENCODER]),
                "decode_init" => groups.extend(["length_arg start_state", DECODER]),
                "decode_reset" => groups.extend(["start_state", DECODER]),
                "decode_rewind" => groups.push(DECODER),
                "decode_deinit" => groups.push("len n_states o_idx p_idx storage_released"),
                "decode_scan" => groups.extend(["n", BUFFERS, DECODER]),
                "decode_flush" => groups.extend([BUFFERS, DECODER]),
                "decode_get_best_end_state" => groups.extend(["ret", DECODER]),
                "decode_get_output" => groups.extend(["has_flush end_state", BUFFERS, DECODER]),
                "decode" | "decode_acc" => groups.extend(["soft_mode", BUFFERS]),
                api => panic!("unknown API {api}"),
            }
        }
        kind => panic!("unknown record type {kind}"),
    }
    groups.into_iter().flat_map(str::split_whitespace).collect()
}

pub fn validate_shape(record: &Record) {
    assert_eq!(
        record.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        shape(record).into_iter().collect::<BTreeSet<_>>()
    );
    assert!(BACKENDS.contains(&record["backend"].as_str()));
    for (key, value) in record {
        if UNSIGNED.contains(&key.as_str()) {
            assert_eq!(value.parse::<u32>().unwrap().to_string(), *value);
        } else if SIGNED.contains(&key.as_str()) {
            assert_eq!(value.parse::<i32>().unwrap().to_string(), *value);
        } else {
            assert!(STRINGS.contains(&key.as_str()));
            assert!(!value.contains(['"', '\\']) && !value.chars().any(char::is_control));
        }
    }
    for key in ["id", "case", "code"] {
        if let Some(value) = record.get(key) {
            assert!(!value.is_empty());
            assert!(
                value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            );
        }
    }
}

pub fn parse_line(line: &str) -> Record {
    let record = parse(line, UNSIGNED, SIGNED, STRINGS);
    validate_shape(&record);
    record
}

pub fn list(text: &str, min: i64, max: i64) -> Vec<i64> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(';')
        .map(|part| {
            let value = part.parse::<i64>().unwrap();
            assert_eq!(value.to_string(), part);
            assert!((min..=max).contains(&value));
            value
        })
        .collect()
}

fn bounded(record: &Record, key: &str, min: u32, max: u32) -> u32 {
    let value = unsigned(record, key);
    assert!((min..=max).contains(&value), "{key} out of bounds");
    value
}

pub fn validate_meta(record: &Record) {
    validate_shape(record);
    assert_eq!(record["schema"], "1");
    assert_eq!(record["neon_executed"], "0");
    assert_eq!(
        record["runtime"],
        if record["backend"] == "linked-dispatch" {
            "avx"
        } else {
            &record["backend"]
        }
    );
    for (key, value) in [
        ("upstream_git_revision", "unknown"),
        ("manifest_sha256", REFERENCE_SHA),
        ("generator_manifest_sha256", GENERATOR_SHA),
        ("library_sha256", LIBRARY_SHA),
        ("harness_sha256", HARNESS_SHA),
        ("runner_sha256", RUNNER_SHA),
        ("compiler", "13.3.0"),
    ] {
        assert_eq!(record[key], value, "provenance {key}");
    }
}

pub fn validate_descriptor(record: &Record) {
    validate_shape(record);
    let n = bounded(record, "n", 1, 9);
    let k = bounded(record, "k", 2, 10);
    assert!([1, 2, 3, 4, 5, 8, 9].contains(&n));
    assert!([2, 3, 5, 7, 9, 10].contains(&k));
    bounded(record, "len", 0, 8191);
    assert!([0, 1, 2, 42].contains(&unsigned(record, "term")));
    let ns = 1usize << (k - 1);
    for (key, length, limit) in [
        ("next_output", 2 * ns, 1u32 << n),
        ("next_state", 2 * ns, ns as u32),
        ("next_term_output", ns, 1u32 << n),
        ("next_term_state", ns, ns as u32),
    ] {
        let data = bytes(&record[key]);
        if key.starts_with("next_term") && data.is_empty() {
            continue;
        }
        assert_eq!(data.len(), length, "descriptor {key}");
        assert!(data.iter().all(|&b| u32::from(b) < limit));
    }
    assert_eq!(
        record["next_term_output"].is_empty(),
        record["next_term_state"].is_empty()
    );
    bounded(record, "puncture_present", 0, 1);
    let puncture = list(&record["puncture"], 0, 8192);
    assert!(puncture.windows(2).all(|w| w[0] < w[1]));
    if record["puncture_present"] == "0" {
        assert!(puncture.is_empty());
    }
}

fn raw_length(code: &Record) -> usize {
    (unsigned(code, "len")
        + if code["term"] == "0" {
            unsigned(code, "k") - 1
        } else {
            0
        }) as usize
        * unsigned(code, "n") as usize
}

fn encoded_length(code: &Record) -> usize {
    let raw = raw_length(code);
    raw - list(&code["puncture"], 0, 8192)
        .iter()
        .filter(|&&p| p < raw as i64)
        .count()
}

pub fn validate_operation(record: &Record, code: &Record) {
    validate_shape(record);
    assert_eq!(record["type"], "op");
    assert_eq!(record["code"], code["id"]);
    assert_eq!(record["backend"], code["backend"]);
    bounded(record, "seq", 0, 8191);
    let api = record["api"].as_str();
    let n = unsigned(code, "n") as usize;
    let k = unsigned(code, "k") as usize;
    let len = unsigned(code, "len") as usize;
    let ns = 1usize << (k - 1);
    let punctures = list(&code["puncture"], 0, 8192).len();
    if record.contains_key("n") {
        bounded(record, "n", 0, 8191);
    }
    if record.contains_key("soft_mode") {
        bounded(record, "soft_mode", 0, 5);
    }
    if record.contains_key("has_flush") {
        bounded(record, "has_flush", 0, 1);
    }
    for key in ["start_state", "end_state"] {
        if record.contains_key(key) {
            assert!((-1..=ns as i32).contains(&signed(record, key)));
            if key == "end_state" {
                assert!(signed(record, key) < ns as i32);
            }
        }
    }
    if record.contains_key("length_arg") {
        assert!((-1..8192).contains(&signed(record, "length_arg")));
    }
    if record.contains_key("state") {
        bounded(record, "state", 0, (ns.min(256) - 1) as u32);
        bounded(record, "i_idx", 0, (len + 2 * (k - 1)) as u32);
        bounded(record, "p_idx", 0, punctures as u32);
        if api == "encode_init" {
            for key in ["state", "i_idx", "p_idx"] {
                assert_eq!(record[key], "0");
            }
        }
        if api == "encode_flush" {
            assert_eq!(record["state"], "0");
        }
    }
    if record.contains_key("ae") {
        assert!(k <= 9, "unobserved wide generic state");
        assert_eq!(unsigned(record, "n_states") as usize, ns);
        let state_len = bounded(record, "len", 0, 8191) as usize;
        let oi = bounded(record, "o_idx", 0, (state_len + k - 1) as u32) as usize;
        bounded(record, "p_idx", 0, punctures as u32);
        assert_eq!(list(&record["ae"], 0, 0x00ffffff).len(), ns);
        bounded(record, "ae_next_valid", 0, 1);
        assert_eq!(
            list(&record["ae_next"], 0, 0x00ffffff).len(),
            if record["ae_next_valid"] == "1" {
                ns
            } else {
                0
            }
        );
        assert_eq!(list(&record["history"], -1, (ns - 1) as i64).len(), oi * ns);
        if ["decode_init", "decode_reset", "decode_rewind"].contains(&api) {
            assert_eq!(oi, 0);
            assert_eq!(record["p_idx"], "0");
        }
        if ["decode_init", "decode_reset"].contains(&api) {
            let start = signed(record, "start_state");
            let expected: Vec<_> = (0..ns)
                .map(|s| {
                    if start < 0 || s as i32 == start {
                        0
                    } else {
                        0x00ffffff
                    }
                })
                .collect();
            assert_eq!(list(&record["ae"], 0, 0x00ffffff), expected);
        }
        if api == "decode_init" {
            let arg = signed(record, "length_arg");
            assert_eq!(state_len, if arg <= 0 { len } else { arg as usize });
            assert_eq!(record["ae_next_valid"], "0");
        }
        if api == "decode_get_best_end_state" {
            assert!((-1..ns as i32).contains(&signed(record, "ret")));
        }
    }
    if api == "decode_deinit" {
        for key in ["len", "n_states", "o_idx", "p_idx"] {
            assert_eq!(record[key], "0");
        }
        assert_eq!(record["storage_released"], "1");
    }
    if api.starts_with("get_") {
        let arg = signed(record, "length_arg");
        let input_len = if arg <= 0 { len as i32 } else { arg };
        let expected = if api == "get_input_length" {
            input_len
        } else {
            (input_len
                + if code["term"] == "0" {
                    (k - 1) as i32
                } else {
                    0
                })
                * n as i32
                - punctures as i32
        };
        assert_eq!(signed(record, "ret"), expected);
    }
    if !record.contains_key("input_before") {
        return;
    }
    let input = bytes(&record["input_before"]);
    assert_eq!(input, bytes(&record["input_after"]), "input changed");
    let before = bytes(&record["output_before"]);
    let after = bytes(&record["output_after"]);
    assert_eq!(before.len(), after.len());
    assert!(before.iter().all(|&b| b == 0xa5));
    let ret = signed(record, "ret");
    let (input_len, output_len, written) = match api {
        "encode" => {
            assert_eq!(ret as usize, encoded_length(code));
            (len, ret as usize + 4, ret as usize)
        }
        "encode_load_state" => {
            assert_eq!(ret, 0);
            (k - 1, 0, 0)
        }
        "encode_raw" => {
            let count = unsigned(record, "n") as usize;
            assert!((0..=(count * n) as i32).contains(&ret));
            (count, count * n + 4, ret as usize)
        }
        "encode_flush" => {
            assert!((0..=((k - 1) * n) as i32).contains(&ret));
            (0, (k - 1) * n + 4, ret as usize)
        }
        "decode_scan" | "decode_flush" => {
            let count = if api == "decode_scan" {
                unsigned(record, "n") as usize
            } else {
                k - 1
            };
            assert!((0..=(count * n) as i32).contains(&ret));
            (ret as usize, 0, 0)
        }
        "decode_get_output" => {
            assert!(ret >= -1);
            let count = unsigned(record, "o_idx") as usize;
            let written = if ret < 0 {
                0
            } else {
                count - if record["has_flush"] == "1" { k - 1 } else { 0 }
            };
            (0, len + 4, written)
        }
        "decode" | "decode_acc" => {
            if api == "decode_acc" {
                assert!([-71, -22, 0].contains(&ret));
            } else {
                assert!(ret >= 0 || [-71, -22].contains(&ret));
            }
            (encoded_length(code), len + 4, if ret < 0 { 0 } else { len })
        }
        _ => unreachable!(),
    };
    assert_eq!(input.len(), input_len, "{api} input length");
    assert_eq!(before.len(), output_len, "{api} full output length");
    assert!(written <= after.len());
    assert!(after[..written].iter().all(|&b| b <= 1), "non-bit output");
    assert_eq!(
        after[written..],
        before[written..],
        "unwritten capacity changed"
    );
    if api == "encode_load_state" {
        let packed = input.iter().fold(0u8, |state, &byte| (state << 1) | byte);
        assert_eq!(unsigned(record, "state"), u32::from(packed));
    } else if api.starts_with("encode") {
        assert!(input.iter().all(|&b| b <= 1));
    }
    // All raw i8 bit patterns, including 0x80, are legal soft inputs.
}

fn validate_transition(record: &Record, previous: Option<&Record>, code: &Record) {
    let api = record["api"].as_str();
    if !record.contains_key("ae") || api == "decode_init" {
        return;
    }
    let previous = previous.expect("decoder operation without init");
    assert_eq!(record["len"], previous["len"]);
    if ["decode_reset", "decode_rewind"].contains(&api) {
        assert_eq!(record["ae_next_valid"], previous["ae_next_valid"]);
        assert_eq!(record["ae_next"], previous["ae_next"]);
        if api == "decode_rewind" {
            let old = list(&previous["ae"], 0, 0x00ffffff);
            let min = *old.iter().min().unwrap();
            assert_eq!(
                list(&record["ae"], 0, 0x00ffffff),
                old.iter().map(|v| v - min).collect::<Vec<_>>()
            );
        }
    } else if ["decode_scan", "decode_flush"].contains(&api) {
        let count = if api == "decode_scan" {
            unsigned(record, "n")
        } else {
            unsigned(code, "k") - 1
        };
        assert_eq!(
            unsigned(record, "o_idx"),
            unsigned(previous, "o_idx") + count
        );
        let consumed =
            count * unsigned(code, "n") - (unsigned(record, "p_idx") - unsigned(previous, "p_idx"));
        assert_eq!(signed(record, "ret"), consumed as i32);
        let old = list(&previous["history"], -1, 255);
        let new = list(&record["history"], -1, 255);
        assert_eq!(new[..old.len()], old);
        if count > 0 {
            assert_eq!(record["ae_next_valid"], "1");
            assert_eq!(record["ae"], record["ae_next"]);
        } else {
            for key in DECODER.split_whitespace() {
                assert_eq!(record[key], previous[key]);
            }
        }
    } else {
        for key in DECODER.split_whitespace() {
            assert_eq!(record[key], previous[key]);
        }
    }
}

// Follow recorded predecessors only to reject observation of invalid history.
// No survivor decisions, soft metrics, or decoded bits are computed here.
fn validate_output_history(record: &Record, selected: i32) {
    if selected < 0 {
        assert_eq!(signed(record, "ret"), -1);
        return;
    }
    let ns = unsigned(record, "n_states") as usize;
    let ae = list(&record["ae"], 0, 0x00ffffff);
    assert_eq!(signed(record, "ret") as i64, ae[selected as usize]);
    let history = list(&record["history"], -1, (ns - 1) as i64);
    let mut state = selected as usize;
    for row in history.chunks_exact(ns).rev() {
        assert!(row[state] >= 0, "traceback touches invalid history");
        state = row[state] as usize;
    }
}

fn validate_encoder_transition(record: &Record, previous: Option<&Record>, code: &Record) {
    let api = record["api"].as_str();
    if api == "encode_init" {
        return;
    }
    let previous = previous.expect("encoder operation without init");
    if api == "encode_load_state" {
        for key in ["i_idx", "p_idx"] {
            assert_eq!(record[key], previous[key]);
        }
    } else {
        let count = if api == "encode_raw" {
            unsigned(record, "n")
        } else {
            unsigned(code, "k") - 1
        };
        assert_eq!(
            unsigned(record, "i_idx"),
            unsigned(previous, "i_idx") + count
        );
        let written =
            count * unsigned(code, "n") - (unsigned(record, "p_idx") - unsigned(previous, "p_idx"));
        assert_eq!(signed(record, "ret"), written as i32);
    }
}

/// Full semantic records retained for later Rust-versus-C conformance stages.
pub struct Fixture {
    pub records: Vec<Record>,
}

impl Fixture {
    pub fn read(text: &str) -> Self {
        assert!(text.ends_with('\n'), "truncated final record");
        let records: Vec<_> = text.lines().map(parse_line).collect();
        validate_records(records.iter());
        Self { records }
    }

    pub fn descriptor(&self, backend: &str, id: &str) -> &Record {
        self.records
            .iter()
            .find(|r| r["type"] == "code" && r["backend"] == backend && r["id"] == id)
            .unwrap()
    }
}

/// Deliberately independent of the fixed byte fingerprint.
pub fn validate_records<'a>(records: impl IntoIterator<Item = &'a Record>) {
    let mut meta = BTreeSet::new();
    let mut codes = BTreeMap::new();
    let mut sequences = BTreeMap::new();
    let mut distinct = BTreeSet::new();
    let mut generic = BTreeSet::new();
    let mut counts = BTreeMap::new();
    let mut backend_counts = BTreeMap::new();
    let mut coverage = String::new();
    let mut decoder = BTreeMap::new();
    let mut encoder = BTreeMap::new();
    let mut best_end = BTreeMap::new();
    let mut case_codes = BTreeMap::new();
    let mut total = 0;
    let mut active_backend = None;
    for record in records {
        total += 1;
        validate_shape(record);
        let backend = record["backend"].as_str();
        match record["type"].as_str() {
            "meta" => {
                validate_meta(record);
                assert_eq!(backend, BACKENDS[meta.len()]);
                assert!(meta.insert(backend));
                active_backend = Some(backend);
            }
            "code" => {
                assert_eq!(active_backend, Some(backend));
                validate_descriptor(record);
                assert!(
                    codes
                        .insert((backend, record["id"].as_str()), record)
                        .is_none(),
                    "duplicate descriptor"
                );
            }
            "op" => {
                assert_eq!(active_backend, Some(backend));
                let code = codes[&(backend, record["code"].as_str())];
                validate_operation(record, code);
                let case = record["case"].as_str();
                let key = (backend, case);
                if let Some(old) = case_codes.insert(key, record["code"].as_str()) {
                    assert_eq!(old, record["code"]);
                }
                let next = sequences.entry(key).or_insert_with(|| {
                    coverage.push_str(&format!("{backend}:{case}\n"));
                    0
                });
                assert_eq!(unsigned(record, "seq"), *next, "case sequence {key:?}");
                *next += 1;
                distinct.insert(case);
                *counts.entry(record["api"].as_str()).or_insert(0usize) += 1;
                *backend_counts.entry(backend).or_insert(0usize) += 1;
                if record.contains_key("ae") {
                    assert_eq!(backend, "linked-dispatch");
                    generic.insert(case);
                    if record["api"] == "decode_init" {
                        assert!(!decoder.contains_key(&key), "init without deinit");
                    }
                    validate_transition(record, decoder.get(&key).copied(), code);
                    if record["api"] == "decode_get_best_end_state" {
                        best_end.insert(key, signed(record, "ret"));
                    } else if record["api"] == "decode_get_output" {
                        let forced = signed(record, "end_state");
                        let selected = if forced < 0 { best_end[&key] } else { forced };
                        validate_output_history(record, selected);
                    }
                    decoder.insert(key, record);
                } else if record["api"] == "decode_deinit" {
                    assert!(decoder.remove(&key).is_some());
                    best_end.remove(&key);
                }
                if record.contains_key("state") {
                    assert_eq!(backend, "linked-dispatch");
                    validate_encoder_transition(record, encoder.get(&key).copied(), code);
                    encoder.insert(key, record);
                }
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(total, 36256);
    assert_eq!(meta.len(), 4);
    assert_eq!(codes.len(), 1066);
    assert_eq!(distinct.len(), 1707);
    assert_eq!(generic.len(), 332);
    assert_eq!(sequences.len(), 6804);
    assert_eq!(
        fingerprint(&coverage),
        CASE_FINGERPRINT,
        "ordered case coverage"
    );
    assert_eq!(counts, API_COUNTS.into_iter().collect());
    assert_eq!(
        backend_counts,
        [
            ("linked-dispatch", 25133),
            ("scalar", 3351),
            ("sse", 3351),
            ("avx", 3351)
        ]
        .into_iter()
        .collect()
    );
    assert!(decoder.is_empty(), "missing decoder deinit");
    for backend in BACKENDS {
        assert_eq!(
            codes.keys().filter(|&&(b, _)| b == backend).count(),
            if backend == "linked-dispatch" {
                268
            } else {
                266
            }
        );
    }
}

pub fn validate_fingerprint(text: &str) {
    assert_eq!(
        fingerprint(text),
        FIXTURE_FINGERPRINT,
        "direct C evidence changed"
    );
}

/// Pins complete local manifests and joins their source entries to metadata.
/// No sibling repository or C installation is needed by ordinary tests.
pub fn validate_manifests(reference: &str, generator: &str, fixture: &Fixture) {
    assert_eq!(fingerprint(reference), 0x144cae78eeb44ce8);
    assert_eq!(fingerprint(generator), 0x44e8c13a9dc0a0f8);
    assert!(reference.contains("\"upstream_git_revision\": \"unknown\""));
    assert!(reference.contains("\"embedded_version_descriptive\": \"1.14.1.1-95043\""));
    assert!(generator.contains(&format!(
        "\"reference_manifest_sha256\": \"{REFERENCE_SHA}\""
    )));
    for record in fixture.records.iter().filter(|r| r["type"] == "meta") {
        validate_meta(record);
        for (manifest, file, field) in [
            (
                reference,
                "src/core/.libs/libosmocore.so.22.4.1",
                "library_sha256",
            ),
            (generator, "conv_vector.c", "harness_sha256"),
            (generator, "run_conv_vector.sh", "runner_sha256"),
        ] {
            assert!(manifest.contains(&format!("\"{file}\": \"{}\"", record[field])));
        }
    }
}

/// Compare correlated implementations only; generic metrics are not correlations.
pub fn compare_backends(fixture: &Fixture) -> usize {
    let mut observations: BTreeMap<(&str, &str, &str), Vec<&Record>> = BTreeMap::new();
    let descriptors: BTreeMap<_, _> = fixture
        .records
        .iter()
        .filter(|r| r["type"] == "code")
        .map(|r| ((r["backend"].as_str(), r["id"].as_str()), r))
        .collect();
    for record in fixture
        .records
        .iter()
        .filter(|r| r["type"] == "op" && ["decode", "decode_acc"].contains(&r["api"].as_str()))
    {
        observations
            .entry((&record["backend"], &record["case"], &record["api"]))
            .or_default()
            .push(record);
    }
    let mut compared = 0;
    for (&(backend, case, api), scalar) in &observations {
        if backend != "scalar" {
            continue;
        }
        for other in ["sse", "avx", "linked-dispatch"] {
            let candidates = &observations[&(other, case, api)];
            assert_eq!(scalar.len(), candidates.len());
            for (left, right) in scalar.iter().zip(candidates) {
                compare_observation(
                    left,
                    right,
                    descriptors[&(backend, left["code"].as_str())],
                    descriptors[&(other, right["code"].as_str())],
                    other,
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 3351 * 3);
    compared
}

pub fn compare_observation(
    left: &Record,
    right: &Record,
    left_code: &Record,
    right_code: &Record,
    backend: &str,
) {
    assert_eq!(left["backend"], "scalar");
    assert_eq!(right["backend"], backend);
    assert!(["sse", "avx", "linked-dispatch"].contains(&backend));
    validate_operation(left, left_code);
    validate_operation(right, right_code);
    for key in shape(left_code).into_iter().filter(|&k| k != "backend") {
        assert_eq!(left_code[key], right_code[key]);
    }
    for key in [
        "case",
        "api",
        "code",
        "soft_mode",
        "input_before",
        "input_after",
        "output_before",
        "output_after",
        "ret",
    ] {
        assert_eq!(
            left[key], right[key],
            "backend {backend}, case {}, API {}, {key}",
            left["case"], left["api"]
        );
    }
}

#[cfg(test)]
pub mod checks {
    use super::*;
    use std::panic::{UnwindSafe, catch_unwind};

    /// Independently check descriptor/input generation, never decoder outputs.
    pub fn synthetic_replacements(fixture: &Fixture) {
        let mut seed = 0x8f32d6a1u32;
        let input: Vec<_> = (0..97)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 31) as u8
            })
            .collect();
        for backend in BACKENDS {
            for (suffix, punctured) in [("unpunctured", false), ("punctured", true)] {
                let id = format!("synthetic-rate2-k7-l97-{suffix}");
                let code = fixture.descriptor(backend, &id);
                assert_eq!(
                    (
                        unsigned(code, "n"),
                        unsigned(code, "k"),
                        signed(code, "len"),
                        signed(code, "term")
                    ),
                    (2, 7, 97, 0)
                );
                let states = bytes(&code["next_state"]);
                let outputs = bytes(&code["next_output"]);
                for register in 0u32..128 {
                    assert_eq!(states[register as usize], (register & 63) as u8);
                    let symbol = (((register & 0x45).count_ones() & 1) << 1)
                        | ((register & 0x43).count_ones() & 1);
                    assert_eq!(outputs[register as usize], symbol as u8);
                }
                assert!(code["next_term_state"].is_empty());
                assert!(code["next_term_output"].is_empty());
                assert_eq!(unsigned(code, "puncture_present"), u32::from(punctured));
                let punctures = if punctured {
                    (0..206)
                        .step_by(3)
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(";")
                } else {
                    String::new()
                };
                assert_eq!(code["puncture"], punctures);
                if backend == "linked-dispatch" {
                    let encodes: Vec<_> = fixture
                        .records
                        .iter()
                        .filter(|r| {
                            r["type"] == "op"
                                && r["backend"] == backend
                                && r["code"] == id
                                && r["api"] == "encode"
                        })
                        .collect();
                    assert_eq!(encodes.len(), 6);
                    for record in encodes {
                        assert_eq!(bytes(&record["input_before"]), input);
                    }
                }
            }
        }
    }

    fn rejected(label: &str, check: impl FnOnce() + UnwindSafe) {
        assert!(catch_unwind(check).is_err(), "corruption accepted: {label}");
    }

    fn changed(record: &Record, key: &str, value: &str) -> Record {
        let mut record = record.clone();
        record.insert(key.to_owned(), value.to_owned());
        record
    }

    fn operation<'a>(fixture: &'a Fixture, api: &str) -> (&'a Record, &'a Record) {
        let record = fixture
            .records
            .iter()
            .find(|r| r["type"] == "op" && r["api"] == api)
            .unwrap();
        (
            record,
            fixture.descriptor(&record["backend"], &record["code"]),
        )
    }

    pub fn fingerprint_rejections(text: &str) {
        validate_fingerprint(text);
        rejected("truncated byte evidence", || {
            validate_fingerprint(&text[..text.len() - 1])
        });
        let changed = text.replacen("\"ret\":5", "\"ret\":4", 1);
        assert_ne!(changed, text);
        rejected("altered observation fingerprint", || {
            validate_fingerprint(&changed)
        });
        rejected("missing final newline", || {
            Fixture::read(&text[..text.len() - 1]);
        });
        let last = text.lines().last().unwrap();
        rejected("partial JSON", || {
            parse_line(&last[..last.len() - 1]);
        });
    }

    pub fn parser_rejections(text: &str) {
        let line = text.lines().nth(2).unwrap();
        for (label, bad) in [
            (
                "duplicate field",
                line.replacen("\"seq\":0", "\"seq\":0,\"seq\":0", 1),
            ),
            ("missing field", line.replacen(",\"seq\":0", "", 1)),
            (
                "unknown field",
                line.replacen("\"seq\":0", "\"seq\":0,\"bogus\":0", 1),
            ),
            (
                "unsigned negative",
                line.replacen("\"seq\":0", "\"seq\":-1", 1),
            ),
            (
                "unsigned overflow",
                line.replacen("\"seq\":0", "\"seq\":4294967296", 1),
            ),
            (
                "signed overflow",
                line.replacen("\"ret\":5", "\"ret\":2147483648", 1),
            ),
            ("leading zero", line.replacen("\"seq\":0", "\"seq\":00", 1)),
            (
                "negative zero",
                line.replacen("\"length_arg\":-1", "\"length_arg\":-0", 1),
            ),
            ("float", line.replacen("\"seq\":0", "\"seq\":0.0", 1)),
            (
                "numeric string",
                line.replacen("\"seq\":0", "\"seq\":\"0\"", 1),
            ),
            (
                "wrong string type",
                line.replacen("\"backend\":\"linked-dispatch\"", "\"backend\":0", 1),
            ),
            (
                "escape",
                line.replacen("linked-dispatch", "linked\\u002ddispatch", 1),
            ),
            (
                "unknown API",
                line.replacen("get_input_length", "decoder_magic", 1),
            ),
            (
                "backend confusion",
                line.replacen("linked-dispatch", "neon", 1),
            ),
        ] {
            assert_ne!(bad, line, "ineffective mutation {label}");
            rejected(label, || {
                parse_line(&bad);
            });
        }
        for bad in ["0", "GG", "AF", "0g", "-1", "00 "] {
            rejected("malformed hex", || {
                bytes(bad);
            });
        }
        for bad in [
            "01",
            "+1",
            "-0",
            "1;",
            ";1",
            "1;;2",
            " 1",
            "2147483648",
            "-2",
        ] {
            rejected("malformed/range decimal list", || {
                list(bad, -1, i32::MAX as i64);
            });
        }
        assert_eq!(list("-1;0;255", -1, 255), [-1, 0, 255]);
        let all: String = (0..=255).map(|b| format!("{b:02x}")).collect();
        assert_eq!(bytes(&all), (0..=255).collect::<Vec<u8>>());
    }

    pub fn descriptor_rejections(fixture: &Fixture) {
        let record = fixture
            .records
            .iter()
            .find(|r| r["type"] == "code")
            .unwrap();
        for (key, value) in [
            ("n", "0"),
            ("n", "10"),
            ("k", "1"),
            ("k", "11"),
            ("len", "8192"),
            ("term", "3"),
            ("next_output", "00"),
            ("next_state", "000100"),
            ("next_state", "00020001"),
            ("next_output", "00020100"),
            ("next_term_state", "0001"),
            ("next_term_output", "0001"),
            ("puncture_present", "2"),
            ("puncture", "0"),
            ("puncture", "-1"),
            ("puncture", "00"),
            ("puncture", "8193"),
        ] {
            rejected(key, || validate_descriptor(&changed(record, key, value)));
        }
        let present = changed(record, "puncture_present", "1");
        validate_descriptor(&present); // Empty differs from NULL but is valid.
        for value in ["1;1", "2;1"] {
            rejected("puncture ordering", || {
                validate_descriptor(&changed(&present, "puncture", value))
            });
        }
        let endpoint = changed(&present, "puncture", "0;6;13");
        validate_descriptor(&endpoint); // Beyond-frame punctures are observed, not discarded.
    }

    pub fn operation_rejections(fixture: &Fixture) {
        for (api, key, value) in [
            ("encode", "input_after", "01"),
            ("encode", "input_before", "02"),
            ("encode", "output_before", "00"),
            ("encode", "output_after", "00"),
            ("encode", "ret", "5"),
            ("encode_init", "state", "2"),
            ("encode_init", "i_idx", "1"),
            ("encode_load_state", "ret", "1"),
            ("encode_raw", "n", "8192"),
            ("encode_raw", "p_idx", "1"),
            ("decode", "soft_mode", "6"),
            ("decode", "input_before", "7g"),
            ("decode_acc", "ret", "1"),
            ("decode_init", "n_states", "3"),
            ("decode_init", "ae", "0"),
            ("decode_init", "ae", "0;16777216"),
            ("decode_init", "ae_next", "0;0"),
            ("decode_init", "history", "-1;0"),
            ("decode_init", "start_state", "3"),
            ("decode_init", "ae_next_valid", "2"),
            ("decode_scan", "history", "0;1;2;0"),
            ("decode_scan", "history", "0;1;-2;0"),
            ("decode_scan", "history", "0;1;1"),
            ("decode_scan", "o_idx", "100"),
            ("decode_scan", "ae_next", "0"),
            ("decode_get_output", "has_flush", "2"),
            ("decode_get_output", "end_state", "2"),
            ("decode_deinit", "storage_released", "0"),
            ("decode_deinit", "n_states", "2"),
            ("get_input_length", "ret", "4"),
            ("get_output_length", "ret", "5"),
        ] {
            let (record, code) = operation(fixture, api);
            rejected(&format!("{api}.{key}"), || {
                validate_operation(&changed(record, key, value), code)
            });
        }
        let (record, code) = operation(fixture, "encode");
        let mut missing = record.clone();
        missing.remove("ret");
        rejected("missing operation field", || {
            validate_operation(&missing, code)
        });
        rejected("backend/code mismatch", || {
            validate_operation(&changed(record, "backend", "scalar"), code)
        });
        let mut suffix = record.clone();
        let output = suffix.get_mut("output_after").unwrap();
        let end = output.len();
        output.replace_range(end - 2.., "00");
        rejected("changed suffix", || validate_operation(&suffix, code));
        let (soft, code) = operation(fixture, "decode_scan");
        let mut soft = soft.clone();
        soft.insert("input_before".into(), "80ff".into());
        soft.insert("input_after".into(), "80ff".into());
        validate_operation(&soft, code);
    }

    pub fn coverage_rejections(fixture: &Fixture) {
        let records = &fixture.records;
        rejected("truncated record count", || {
            validate_records(records[..records.len() - 1].iter())
        });
        rejected("duplicate descriptor", || {
            validate_records(records[..2].iter().chain(records[1..].iter()))
        });
        rejected("missing descriptor", || {
            validate_records(
                records
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != 1)
                    .map(|(_, r)| r),
            )
        });
        rejected("duplicate case/sequence", || {
            validate_records(records[..3].iter().chain(records[2..].iter()))
        });
        let wrong_seq = changed(&records[2], "seq", "1");
        rejected("nonconsecutive seq", || {
            validate_records(
                records
                    .iter()
                    .enumerate()
                    .map(|(i, r)| if i == 2 { &wrong_seq } else { r }),
            )
        });
        let first_case = &records[2]["case"];
        rejected("missing whole case", || {
            validate_records(records.iter().filter(|r| {
                !(r["type"] == "op"
                    && r["backend"] == "linked-dispatch"
                    && r["case"] == *first_case)
            }))
        });
        let renamed: BTreeMap<_, _> = records
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r["type"] == "op" && r["case"] == *first_case && r["backend"] == "linked-dispatch"
            })
            .map(|(i, r)| (i, changed(r, "case", "unknown-case")))
            .collect();
        rejected("coverage identifiers changed with counts intact", || {
            validate_records(
                records
                    .iter()
                    .enumerate()
                    .map(|(i, r)| renamed.get(&i).unwrap_or(r)),
            )
        });
    }

    pub fn state_rejections(fixture: &Fixture) {
        let mut previous: BTreeMap<&str, &Record> = BTreeMap::new();
        let descriptors: BTreeMap<_, _> = fixture
            .records
            .iter()
            .filter(|r| r["type"] == "code")
            .map(|r| ((r["backend"].as_str(), r["id"].as_str()), r))
            .collect();
        let mut reset_checked = false;
        let mut rewind_checked = false;
        let mut scan_checked = false;
        for record in fixture.records.iter().filter(|r| r.contains_key("ae")) {
            let case = record["case"].as_str();
            let code = descriptors[&(record["backend"].as_str(), record["code"].as_str())];
            let old = previous.get(case).copied();
            if record["api"] == "decode_reset" && record["ae_next_valid"] == "1" && !reset_checked {
                rejected("reset erased retained scratch", || {
                    validate_transition(&changed(record, "ae_next_valid", "0"), old, code)
                });
                reset_checked = true;
            }
            if record["api"] == "decode_rewind"
                && list(&record["ae"], 0, 0x00ffffff).iter().any(|&x| x != 0)
                && !rewind_checked
            {
                let zeros = vec!["0"; unsigned(record, "n_states") as usize].join(";");
                rejected("rewind treated as reset", || {
                    validate_transition(&changed(record, "ae", &zeros), old, code)
                });
                rewind_checked = true;
            }
            if record["api"] == "decode_scan" && !scan_checked {
                rejected("scan state increment", || {
                    validate_transition(&changed(record, "o_idx", "0"), old, code)
                });
                scan_checked = true;
            }
            previous.insert(case, record);
            if reset_checked && rewind_checked && scan_checked {
                break;
            }
        }
        assert!(reset_checked && rewind_checked && scan_checked);
        let (output, _) = operation(fixture, "decode_get_output");
        let history = changed(output, "history", "-1;-1;-1;-1;-1;-1;-1;-1;-1;-1;-1;-1");
        rejected("uninitialized traceback", || {
            validate_output_history(&history, 0)
        });
    }

    pub fn backend_rejections(fixture: &Fixture) {
        let left = fixture
            .records
            .iter()
            .find(|r| {
                r["type"] == "op"
                    && r["backend"] == "scalar"
                    && r["api"] == "decode_acc"
                    && r["ret"] == "0"
                    && !r["output_after"].starts_with("a5")
            })
            .unwrap();
        let right = fixture
            .records
            .iter()
            .find(|r| {
                r["type"] == "op"
                    && r["backend"] == "sse"
                    && r["case"] == left["case"]
                    && r["api"] == left["api"]
            })
            .unwrap();
        let lc = fixture.descriptor("scalar", &left["code"]);
        let rc = fixture.descriptor("sse", &right["code"]);
        compare_observation(left, right, lc, rc, "sse");
        let mut output = right["output_after"].clone();
        output.replace_range(..2, if output.starts_with("00") { "01" } else { "00" });
        let altered = changed(right, "output_after", &output);
        validate_operation(&altered, rc); // Structurally valid, but backend observation differs.
        rejected("altered backend output", || {
            compare_observation(left, &altered, lc, rc, "sse")
        });
        for (key, value) in [
            ("ret", "-22"),
            ("api", "decode"),
            ("case", "wrong-case"),
            ("backend", "avx"),
        ] {
            rejected(key, || {
                compare_observation(left, &changed(right, key, value), lc, rc, "sse")
            });
        }
    }

    pub fn provenance_rejections(fixture: &Fixture, reference: &str, generator: &str) {
        validate_manifests(reference, generator, fixture);
        let meta = &fixture.records[0];
        for (key, value) in [
            ("upstream_git_revision", "verified"),
            ("runtime", "scalar"),
            ("manifest_sha256", "0"),
            ("generator_manifest_sha256", "0"),
            ("library_sha256", "0"),
            ("harness_sha256", "0"),
            ("runner_sha256", "0"),
            ("schema", "2"),
            ("neon_executed", "1"),
        ] {
            rejected(key, || validate_meta(&changed(meta, key, value)));
        }
        let bad_reference = reference.replacen("unknown", "verified", 1);
        rejected("reference manifest", || {
            validate_manifests(&bad_reference, generator, fixture)
        });
        let bad_generator = generator.replacen("35186", "35185", 1);
        rejected("generator coverage manifest", || {
            validate_manifests(reference, &bad_generator, fixture)
        });
    }
}
