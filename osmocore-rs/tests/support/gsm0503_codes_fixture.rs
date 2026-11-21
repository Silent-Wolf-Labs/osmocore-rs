//! Shared reader for the GSM 05.03 convolutional-code and parity fixture. Checks the
//! shape of the C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x746d1a7ba369c25a;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Bytes after each destination, which the harness fills with `FILL`.
pub const GUARD: usize = 4;
pub const FILL: u8 = 0xa5;

/// Every code with the input and output lengths that upstream's
/// `tests/conv/conv_gsm0503_test.ok` prints for it, in that file's order.
pub const UPSTREAM_LENGTHS: [(&str, i32, i32); 45] = [
    ("gsm0503_xcch", 224, 456),
    ("gsm0503_tch_f24", 72, 456),
    ("gsm0503_tch_h24", 72, 228),
    ("gsm0503_tch_f48", 148, 456),
    ("gsm0503_tch_f96", 240, 456),
    ("gsm0503_tch_f144", 290, 456),
    ("gsm0503_rach", 14, 36),
    ("gsm0503_rach_ext", 17, 36),
    ("gsm0503_sch", 35, 78),
    ("gsm0503_cs2", 290, 456),
    ("gsm0503_cs3", 334, 456),
    ("gsm0503_cs2_np", 290, 588),
    ("gsm0503_cs3_np", 334, 676),
    ("gsm0503_tch_afs_12_2", 250, 448),
    ("gsm0503_tch_afs_10_2", 210, 448),
    ("gsm0503_tch_afs_7_95", 165, 448),
    ("gsm0503_tch_afs_7_4", 154, 448),
    ("gsm0503_tch_afs_6_7", 140, 448),
    ("gsm0503_tch_afs_5_9", 124, 448),
    ("gsm0503_tch_afs_5_15", 109, 448),
    ("gsm0503_tch_afs_4_75", 101, 448),
    ("gsm0503_tch_fr", 185, 378),
    ("gsm0503_tch_hr", 98, 211),
    ("gsm0503_tch_ahs_7_95", 129, 188),
    ("gsm0503_tch_ahs_7_4", 126, 196),
    ("gsm0503_tch_ahs_6_7", 116, 200),
    ("gsm0503_tch_ahs_5_9", 108, 208),
    ("gsm0503_tch_ahs_5_15", 97, 212),
    ("gsm0503_tch_ahs_4_75", 89, 212),
    ("gsm0503_tch_axs_sid_update", 49, 212),
    ("gsm0503_mcs1_dl_hdr", 36, 108),
    ("gsm0503_mcs1_ul_hdr", 39, 117),
    ("gsm0503_mcs1", 190, 588),
    ("gsm0503_mcs2", 238, 732),
    ("gsm0503_mcs3", 310, 948),
    ("gsm0503_mcs4", 366, 1116),
    ("gsm0503_mcs5_dl_hdr", 33, 99),
    ("gsm0503_mcs5_ul_hdr", 45, 135),
    ("gsm0503_mcs5", 462, 1404),
    ("gsm0503_mcs6", 606, 1836),
    ("gsm0503_mcs7_dl_hdr", 45, 135),
    ("gsm0503_mcs7_ul_hdr", 54, 162),
    ("gsm0503_mcs7", 462, 1404),
    ("gsm0503_mcs8", 558, 1692),
    ("gsm0503_mcs9", 606, 1836),
];

/// The parity descriptors with their CRC width, in file order.
pub const CRCS: [(&str, u32); 10] = [
    ("gsm0503_fire_crc40", 64),
    ("gsm0503_cs234_crc16", 16),
    ("gsm0503_mcs_crc8_hdr", 8),
    ("gsm0503_mcs_crc12", 16),
    ("gsm0503_rach_crc6", 8),
    ("gsm0503_sch_crc10", 16),
    ("gsm0503_tch_fr_crc3", 8),
    ("gsm0503_tch_efr_crc8", 8),
    ("gsm0503_amr_crc6", 8),
    ("gsm0503_amr_crc14", 16),
];

/// The records after each code record, in file order.
pub const CODE_CASES: [(&str, &str); 26] = [
    ("encode", "zeros"),
    ("decode", "zeros_clean"),
    ("encode", "ones"),
    ("decode", "ones_clean"),
    ("encode", "alternating"),
    ("encode", "first_bit"),
    ("encode", "last_bit"),
    ("encode", "random"),
    ("encode", "random"),
    ("encode", "random"),
    ("encode", "decoded_frame"),
    ("decode", "clean_127"),
    ("decode", "clean_1"),
    ("decode", "flips"),
    ("decode", "flips"),
    ("decode", "flips"),
    ("decode", "flips"),
    ("decode", "noise_64"),
    ("decode", "noise_64"),
    ("decode", "noise_128"),
    ("decode", "noise_128"),
    ("decode", "erasures"),
    ("decode", "erased_prefix"),
    ("decode", "all_zero"),
    ("decode", "random_soft"),
    ("decode", "random_soft"),
];

/// The `crc_bits` cases after each `crc` record, with their input lengths.
pub const CRC_CASES: [(&str, usize); 7] = [
    ("random_1", 1),
    ("random_9", 9),
    ("random_50", 50),
    ("random_184", 184),
    ("random_456", 456),
    ("zeros_184", 184),
    ("ones_184", 184),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Code {
        code: String,
        n: i32,
        k: i32,
        len: i32,
        term: i32,
        /// Pairs flattened: entry `2 * state + input`.
        next_output: Vec<u8>,
        next_state: Vec<u8>,
        next_term_output: Option<Vec<u8>>,
        next_term_state: Option<Vec<u8>>,
        /// Without C's negative sentinel.
        puncture: Option<Vec<i32>>,
        input_length: i32,
        output_length: i32,
    },
    Encode {
        code: String,
        case: String,
        input: Vec<u8>,
        rc: i32,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    Decode {
        code: String,
        /// `flips` cases carry their flip count as a suffix, such as `flips_10`.
        case: String,
        input: Vec<i8>,
        rc: i32,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    Crc {
        crc: String,
        width: u32,
        bits: i32,
        poly: u64,
        init: u64,
        remainder: u64,
    },
    CrcBits {
        crc: String,
        case: String,
        input: Vec<u8>,
        value: u64,
        /// The parity bits written by `set_bits`, including the guard bytes.
        set: Vec<u8>,
        check: i32,
        /// The check result with the first parity bit flipped.
        check_flipped: i32,
    },
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn hex(text: &str) -> Result<Vec<u8>, String> {
    let lowercase = text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !lowercase || !text.len().is_multiple_of(2) {
        return Err(format!("bad hex {text}"));
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Splits a flat JSON object into keys and raw values, keeping string quotes.
fn fields(line: &str) -> Result<Vec<(&str, &str)>, String> {
    let body = line
        .strip_prefix('{')
        .and_then(|line| line.strip_suffix('}'))
        .ok_or_else(|| format!("not an object: {line}"))?;
    body.split(',')
        .map(|field| {
            let (key, value) = field
                .split_once(':')
                .ok_or_else(|| format!("bad field {field}"))?;
            let key = key
                .strip_prefix('"')
                .and_then(|key| key.strip_suffix('"'))
                .ok_or_else(|| format!("bad key {key}"))?;
            Ok((key, value))
        })
        .collect()
}

fn string(value: &str) -> Result<&str, String> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .filter(|value| !value.contains('"'))
        .ok_or_else(|| format!("not a string: {value}"))
}

fn parsed<T: std::str::FromStr + ToString>(value: &str) -> Result<T, String> {
    let number: T = value.parse().map_err(|_| format!("bad number {value}"))?;
    if number.to_string() == value {
        Ok(number)
    } else {
        Err(format!("non-canonical number {value}"))
    }
}

fn list<T: std::str::FromStr + ToString>(value: &str) -> Result<Vec<T>, String> {
    string(value)?.split(' ').map(parsed).collect()
}

fn optional_list<T: std::str::FromStr + ToString>(value: &str) -> Result<Option<Vec<T>>, String> {
    if value == "null" {
        Ok(None)
    } else {
        list(value).map(Some)
    }
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let expect = |expected: &[&str]| {
        if keys == expected {
            Ok(())
        } else {
            Err(format!("unexpected fields {keys:?}"))
        }
    };
    let value = |index: usize| fields[index].1;
    let text = |index: usize| string(fields[index].1).map(str::to_owned);
    let bytes = |index: usize| string(fields[index].1).and_then(hex);
    match fields.first().map(|(_, kind)| *kind) {
        Some(r#""code""#) => {
            expect(&[
                "kind",
                "code",
                "n",
                "k",
                "len",
                "term",
                "next_output",
                "next_state",
                "next_term_output",
                "next_term_state",
                "puncture",
                "input_length",
                "output_length",
            ])?;
            Ok(Record::Code {
                code: text(1)?,
                n: parsed(value(2))?,
                k: parsed(value(3))?,
                len: parsed(value(4))?,
                term: parsed(value(5))?,
                next_output: list(value(6))?,
                next_state: list(value(7))?,
                next_term_output: optional_list(value(8))?,
                next_term_state: optional_list(value(9))?,
                puncture: optional_list(value(10))?,
                input_length: parsed(value(11))?,
                output_length: parsed(value(12))?,
            })
        }
        Some(r#""encode""#) => {
            expect(&["kind", "code", "case", "in", "rc", "out"])?;
            Ok(Record::Encode {
                code: text(1)?,
                case: text(2)?,
                input: bytes(3)?,
                rc: parsed(value(4))?,
                output: bytes(5)?,
            })
        }
        Some(r#""decode""#) => {
            expect(&["kind", "code", "case", "in", "rc", "out"])?;
            Ok(Record::Decode {
                code: text(1)?,
                case: text(2)?,
                input: bytes(3)?.into_iter().map(|byte| byte as i8).collect(),
                rc: parsed(value(4))?,
                output: bytes(5)?,
            })
        }
        Some(r#""crc""#) => {
            expect(&["kind", "crc", "width", "bits", "poly", "init", "remainder"])?;
            Ok(Record::Crc {
                crc: text(1)?,
                width: parsed(value(2))?,
                bits: parsed(value(3))?,
                poly: parsed(value(4))?,
                init: parsed(value(5))?,
                remainder: parsed(value(6))?,
            })
        }
        Some(r#""crc_bits""#) => {
            expect(&[
                "kind",
                "crc",
                "case",
                "in",
                "value",
                "set",
                "check",
                "check_flipped",
            ])?;
            Ok(Record::CrcBits {
                crc: text(1)?,
                case: text(2)?,
                input: bytes(3)?,
                value: parsed(value(4))?,
                set: bytes(5)?,
                check: parsed(value(6))?,
                check_flipped: parsed(value(7))?,
            })
        }
        other => Err(format!("unknown kind {other:?}")),
    }
}

fn guarded(what: &str, output: &[u8], len: usize) -> Result<(), String> {
    if output.len() != len + GUARD {
        return Err(format!("{what}: destination must include the guard"));
    }
    if output[len..].iter().any(|byte| *byte != FILL) {
        return Err(format!("{what}: wrote past the requested length"));
    }
    if output[..len].iter().any(|bit| *bit > 1) {
        return Err(format!("{what}: output must be unpacked bits"));
    }
    Ok(())
}

/// The descriptor facts the later records of a code depend on.
struct Code<'a> {
    name: &'a str,
    n: i32,
    len: usize,
    output_length: usize,
}

fn check_code<'a>(record: &'a Record, expected: (&str, i32, i32)) -> Result<Code<'a>, String> {
    let Record::Code {
        code,
        n,
        k,
        len,
        term,
        next_output,
        next_state,
        next_term_output,
        next_term_state,
        puncture,
        input_length,
        output_length,
    } = record
    else {
        return Err(format!("expected the {} code record", expected.0));
    };
    if (code.as_str(), *input_length, *output_length) != expected {
        return Err(format!("{code}: differs from upstream's lengths"));
    }
    if !(1..=6).contains(n)
        || !matches!(k, 5 | 7)
        || !matches!(term, 0 | 2)
        || *len != *input_length
    {
        return Err(format!("{code}: unexpected parameters"));
    }
    let states = 1usize << (k - 1);
    if next_output.len() != 2 * states || next_state.len() != 2 * states {
        return Err(format!("{code}: transition tables must cover every state"));
    }
    if next_output
        .iter()
        .any(|output| u32::from(*output) >= 1 << n)
        || next_state.iter().any(|state| usize::from(*state) >= states)
    {
        return Err(format!("{code}: transition out of range"));
    }
    match (next_term_output, next_term_state) {
        (None, None) => {}
        (Some(output), Some(state)) if output.len() == states && state.len() == states => {
            if state.iter().any(|state| usize::from(*state) >= states) {
                return Err(format!("{code}: termination state out of range"));
            }
        }
        _ => return Err(format!("{code}: termination tables must come together")),
    }
    if let Some(puncture) = puncture
        && (puncture.is_empty()
            || puncture[0] < 0
            || puncture.windows(2).any(|pair| pair[0] >= pair[1]))
    {
        return Err(format!("{code}: punctures must be increasing positions"));
    }
    Ok(Code {
        name: code,
        n: *n,
        len: *len as usize,
        output_length: *output_length as usize,
    })
}

fn check_case(record: &Record, code: &Code, expected: (&str, &str)) -> Result<(), String> {
    let (kind, case_name) = expected;
    match record {
        Record::Encode {
            code: name,
            case,
            input,
            rc,
            output,
        } if kind == "encode" => {
            if name != code.name || case != case_name {
                return Err(format!("{}: expected encode {case_name}", code.name));
            }
            if input.len() != code.len || input.iter().any(|bit| *bit > 1) {
                return Err(format!("{name} {case}: input must be one frame of bits"));
            }
            if *rc as usize != code.output_length {
                return Err(format!("{name} {case}: unexpected encoded length {rc}"));
            }
            guarded(case, output, code.output_length)
        }
        Record::Decode {
            code: name,
            case,
            input,
            rc,
            output,
        } if kind == "decode" => {
            let matches = if case_name == "flips" {
                case.strip_prefix("flips_")
                    .is_some_and(|count| parsed::<usize>(count).is_ok())
            } else {
                case == case_name
            };
            if name != code.name || !matches {
                return Err(format!("{}: expected decode {case_name}", code.name));
            }
            if input.len() != code.output_length {
                return Err(format!("{name} {case}: input must be one coded frame"));
            }
            if input.contains(&-128) {
                return Err(format!("{name} {case}: -128 is outside the sbit_t range"));
            }
            // The correlation decoder returns 0; the generic one, for N > 4, a metric.
            if *rc < 0 || (code.n <= 4 && *rc != 0) {
                return Err(format!("{name} {case}: unexpected result {rc}"));
            }
            guarded(case, output, code.len)
        }
        _ => Err(format!("{}: expected {kind} {case_name}", code.name)),
    }
}

fn check_crc(records: &[Record], expected: (&str, u32)) -> Result<(), String> {
    let Some(Record::Crc {
        crc,
        width,
        bits,
        poly,
        init,
        remainder,
    }) = records.first()
    else {
        return Err(format!("expected the {} crc record", expected.0));
    };
    if (crc.as_str(), *width) != expected || !(1..=*width as i32).contains(bits) {
        return Err(format!("{crc}: unexpected descriptor"));
    }
    let mask = u64::MAX >> (64 - bits);
    if poly & !mask != 0 || init & !mask != 0 || remainder & !mask != 0 {
        return Err(format!("{crc}: descriptor wider than its bits"));
    }
    for (record, (case_name, len)) in records[1..].iter().zip(CRC_CASES) {
        let Record::CrcBits {
            crc: name,
            case,
            input,
            value,
            set,
            check,
            check_flipped,
        } = record
        else {
            return Err(format!("{crc}: expected crc_bits {case_name}"));
        };
        if name != crc || case != case_name {
            return Err(format!("{crc}: expected crc_bits {case_name}"));
        }
        if input.len() != len || input.iter().any(|bit| *bit > 1) || value & !mask != 0 {
            return Err(format!("{crc} {case}: unexpected input or value"));
        }
        guarded(case, set, *bits as usize)?;
        if *check != 0 || *check_flipped == 0 {
            return Err(format!("{crc} {case}: checks must pass and then fail"));
        }
    }
    Ok(())
}

/// Check everything but the fingerprint, so corrupted copies reach these checks.
pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm0503-codes-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let records = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    let per_code = 1 + CODE_CASES.len();
    let per_crc = 1 + CRC_CASES.len();
    let expected = UPSTREAM_LENGTHS.len() * per_code + CRCS.len() * per_crc;
    if records.len() != expected {
        return Err(format!(
            "expected {expected} records, got {}",
            records.len()
        ));
    }
    let (code_records, crc_records) = records.split_at(UPSTREAM_LENGTHS.len() * per_code);
    for (block, upstream) in code_records.chunks(per_code).zip(UPSTREAM_LENGTHS) {
        let code = check_code(&block[0], upstream)?;
        for (record, case) in block[1..].iter().zip(CODE_CASES) {
            check_case(record, &code, case)?;
        }
    }
    for (block, crc) in crc_records.chunks(per_crc).zip(CRCS) {
        check_crc(block, crc)?;
    }
    Ok(records)
}

pub fn validate(text: &str) -> Vec<Record> {
    assert_eq!(
        fingerprint(text),
        FINGERPRINT,
        "direct C observations changed"
    );
    parse(text).expect("committed fixture must be well formed")
}

/// Each edit breaks one rule, and `parse` must report it.
pub fn check_corruptions(text: &str) {
    let replace = |from: &str, to: &str| {
        assert!(text.contains(from), "corruption target {from} is missing");
        text.replacen(from, to, 1)
    };
    let lines = text.lines().collect::<Vec<_>>();
    let truncated = lines[..lines.len() - 1].join("\n");
    let extended = format!("{text}{}\n", lines[lines.len() - 1]);
    let edit_line = |kind: &str, from: &str, to: &str| {
        let line = lines
            .iter()
            .find(|line| line.contains(kind) && line.contains(from))
            .unwrap_or_else(|| panic!("no {kind} line contains {from}"));
        text.replacen(line, &line.replacen(from, to, 1), 1)
    };
    let swapped = {
        let mut swapped = lines.clone();
        swapped.swap(2, 3);
        swapped.join("\n") + "\n"
    };
    for corrupt in [
        replace("gsm0503-codes-v1", "gsm0503-codes-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"crc""#, r#""kind":"other""#),
        replace(r#""code":"gsm0503_tch_f24""#, r#""code":"gsm0503_tch_h24""#),
        replace(r#""input_length":224"#, r#""input_length":225"#),
        replace(r#""output_length":456"#, r#""output_length":0456"#),
        replace(r#""next_output":"0 3"#, r#""next_output":"3"#),
        replace(r#""next_state":"0 1"#, r#""next_state":"16 1"#),
        replace(r#""next_term_output":null"#, r#""next_term_output":"0""#),
        replace(r#""puncture":"11 26"#, r#""puncture":"11 11 26"#),
        replace(r#""case":"zeros""#, r#""case":"ones""#),
        replace(r#""case":"flips_"#, r#""case":"flops_"#),
        edit_line(r#""kind":"encode""#, r#""rc":456"#, r#""rc":455"#),
        edit_line(r#""kind":"encode""#, "a5a5a5a5", "a5a5a500"),
        edit_line(r#""kind":"decode""#, "a5a5a5a5", "a5a5a500"),
        edit_line(r#""kind":"decode""#, r#""rc":0"#, r#""rc":1"#),
        edit_line(r#""kind":"decode""#, r#""in":"7f7f"#, r#""in":"807f"#),
        edit_line(r#""kind":"decode""#, r#""in":"7f7f"#, r#""in":"7f"#),
        edit_line(r#""kind":"crc_bits""#, r#""check":0"#, r#""check":1"#),
        edit_line(
            r#""kind":"crc_bits""#,
            r#""check_flipped":1"#,
            r#""check_flipped":0"#,
        ),
        edit_line(r#""kind":"crc_bits""#, "a5a5a5a5", "a5a5a502"),
        edit_line(r#""kind":"crc""#, r#""width":64"#, r#""width":16"#),
        edit_line(r#""kind":"encode""#, r#""rc":456"#, r#""rc":456,"extra":1"#),
        truncated,
        extended,
        swapped,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
    let encode = lines
        .iter()
        .find(|line| line.contains(r#""kind":"encode""#))
        .unwrap();
    let output = encode.find(r#""out":""#).unwrap() + r#""out":""#.len();
    let mut unpacked = encode.to_string();
    unpacked.replace_range(output..output + 2, "02");
    assert!(parse(&text.replacen(encode, &unpacked, 1)).is_err());
}
