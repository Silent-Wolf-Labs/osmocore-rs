//! Shared GSM 05.03 xCCH fixture reader. Checks the shape of the C observations;
//! the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x9689e739175d8581;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. The `coding` section mixes `encode` and `decode`
/// records in the order the harness made them.
pub const SECTIONS: &[(&str, usize)] = &[
    ("code", 1),
    ("crc", 1),
    ("interleave", 6),
    ("map", 5),
    ("unmap", 4),
    ("coding", 1448),
];
pub const ENCODE_COUNT: usize = 200;
pub const DECODE_COUNT: usize = 1248;
pub const UPSTREAM_COUNT: usize = 3;
/// Bytes after each destination, which the harness fills with `FILL`.
pub const GUARD: usize = 4;
pub const FILL: u8 = 0xa5;
pub const L2_LEN: usize = 23;
pub const BURST_BITS: usize = 116;
pub const BURSTS_LEN: usize = 4 * BURST_BITS;
pub const CODED_LEN: usize = 456;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Code {
        n: i32,
        k: i32,
        len: i32,
        term: i32,
        next_output: Vec<u8>,
        next_state: Vec<u8>,
    },
    Crc {
        bits: i32,
        poly: u64,
        init: u64,
        remainder: u64,
    },
    Interleave {
        input: Vec<u8>,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    Deinterleave {
        input: Vec<u8>,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    Map {
        /// -1 when the call passed no flag.
        hl: i32,
        hn: i32,
        input: Vec<u8>,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    Unmap {
        hl: i8,
        hn: i8,
        input: Vec<u8>,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    Encode {
        case: String,
        l2: Vec<u8>,
        /// Includes the guard bytes.
        bursts: Vec<u8>,
    },
    Decode {
        case: String,
        input: Vec<i8>,
        rc: i32,
        n_errors: i32,
        n_bits_total: i32,
        /// Includes the guard bytes.
        l2: Vec<u8>,
    },
}

impl Record {
    fn section(&self) -> &'static str {
        match self {
            Self::Code { .. } => "code",
            Self::Crc { .. } => "crc",
            Self::Interleave { .. } | Self::Deinterleave { .. } => "interleave",
            Self::Map { .. } => "map",
            Self::Unmap { .. } => "unmap",
            Self::Encode { .. } | Self::Decode { .. } => "coding",
        }
    }
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
            Ok((key, value.trim_matches('"')))
        })
        .collect()
}

fn parsed<T: std::str::FromStr + ToString>(value: &str) -> Result<T, String> {
    let number: T = value.parse().map_err(|_| format!("bad number {value}"))?;
    if number.to_string() == value {
        Ok(number)
    } else {
        Err(format!("non-canonical number {value}"))
    }
}

fn list(value: &str) -> Result<Vec<u8>, String> {
    value.split(' ').map(parsed).collect()
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
    match fields.first().map(|(_, kind)| *kind) {
        Some("code") => {
            expect(&["kind", "n", "k", "len", "term", "next_output", "next_state"])?;
            Ok(Record::Code {
                n: parsed(value(1))?,
                k: parsed(value(2))?,
                len: parsed(value(3))?,
                term: parsed(value(4))?,
                next_output: list(value(5))?,
                next_state: list(value(6))?,
            })
        }
        Some("crc") => {
            expect(&["kind", "bits", "poly", "init", "remainder"])?;
            Ok(Record::Crc {
                bits: parsed(value(1))?,
                poly: parsed(value(2))?,
                init: parsed(value(3))?,
                remainder: parsed(value(4))?,
            })
        }
        Some("interleave") => {
            expect(&["kind", "case", "in", "out"])?;
            Ok(Record::Interleave {
                input: hex(value(2))?,
                output: hex(value(3))?,
            })
        }
        Some("deinterleave") => {
            expect(&["kind", "case", "in", "out"])?;
            Ok(Record::Deinterleave {
                input: hex(value(2))?,
                output: hex(value(3))?,
            })
        }
        Some("map") => {
            expect(&["kind", "hl", "hn", "in", "out"])?;
            Ok(Record::Map {
                hl: parsed(value(1))?,
                hn: parsed(value(2))?,
                input: hex(value(3))?,
                output: hex(value(4))?,
            })
        }
        Some("unmap") => {
            expect(&["kind", "hl", "hn", "in", "out"])?;
            Ok(Record::Unmap {
                hl: parsed(value(1))?,
                hn: parsed(value(2))?,
                input: hex(value(3))?,
                output: hex(value(4))?,
            })
        }
        Some("encode") => {
            expect(&["kind", "case", "l2", "bursts"])?;
            Ok(Record::Encode {
                case: value(1).to_owned(),
                l2: hex(value(2))?,
                bursts: hex(value(3))?,
            })
        }
        Some("decode") => {
            expect(&[
                "kind",
                "case",
                "input",
                "rc",
                "n_errors",
                "n_bits_total",
                "l2",
            ])?;
            Ok(Record::Decode {
                case: value(1).to_owned(),
                input: hex(value(2))?.into_iter().map(|byte| byte as i8).collect(),
                rc: parsed(value(3))?,
                n_errors: parsed(value(4))?,
                n_bits_total: parsed(value(5))?,
                l2: hex(value(6))?,
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
    Ok(())
}

fn check(record: &Record) -> Result<(), String> {
    let exact = |what: &str, actual: usize, len: usize| {
        if actual == len {
            Ok(())
        } else {
            Err(format!("{what}: expected {len} entries, got {actual}"))
        }
    };
    match record {
        Record::Code {
            next_output,
            next_state,
            ..
        } => {
            exact("code outputs", next_output.len(), 32)?;
            exact("code states", next_state.len(), 32)
        }
        Record::Crc { .. } => Ok(()),
        Record::Interleave { input, output } | Record::Deinterleave { input, output } => {
            exact("interleave input", input.len(), CODED_LEN)?;
            guarded("interleave", output, CODED_LEN)
        }
        Record::Map {
            hl,
            hn,
            input,
            output,
        } => {
            if !(-1..=1).contains(hl) || !(-1..=1).contains(hn) {
                return Err("map flags must be -1, 0 or 1".to_owned());
            }
            exact("map input", input.len(), BURST_BITS - 2)?;
            guarded("map", output, BURST_BITS)
        }
        Record::Unmap { input, output, .. } => {
            exact("unmap input", input.len(), BURST_BITS)?;
            guarded("unmap", output, BURST_BITS - 2)
        }
        Record::Encode { case, l2, bursts } => {
            exact(case, l2.len(), L2_LEN)?;
            guarded(case, bursts, BURSTS_LEN)?;
            if bursts[..BURSTS_LEN].iter().any(|bit| *bit > 1) {
                return Err(format!("{case}: output must be unpacked bits"));
            }
            let flags_set = (0..4).all(|burst| {
                bursts[burst * BURST_BITS + 57] == 1 && bursts[burst * BURST_BITS + 58] == 1
            });
            if !flags_set {
                return Err(format!("{case}: stealing flags must be set"));
            }
            Ok(())
        }
        Record::Decode {
            case,
            input,
            rc,
            n_errors,
            n_bits_total,
            l2,
        } => {
            exact(case, input.len(), BURSTS_LEN)?;
            guarded(case, l2, L2_LEN)?;
            if input.contains(&-128) {
                return Err(format!("{case}: -128 is outside the sbit_t range"));
            }
            if !matches!(rc, 0 | -1) {
                return Err(format!("{case}: unexpected result {rc}"));
            }
            if *n_bits_total != CODED_LEN as i32 {
                return Err(format!("{case}: unexpected bit total {n_bits_total}"));
            }
            if !(0..=CODED_LEN as i32).contains(n_errors) {
                return Err(format!("{case}: error count {n_errors} out of range"));
            }
            if *rc != 0 && l2.iter().any(|byte| *byte != FILL) {
                return Err(format!("{case}: failed decode wrote its destination"));
            }
            Ok(())
        }
    }
}

/// Check everything but the fingerprint, so corrupted copies reach these checks.
pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm0503-xcch-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let records = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    let mut remaining = records.as_slice();
    for (section, count) in SECTIONS {
        if remaining.len() < *count || remaining[..*count].iter().any(|r| r.section() != *section) {
            return Err(format!("expected {count} {section} records here"));
        }
        remaining = &remaining[*count..];
    }
    if !remaining.is_empty() {
        return Err(format!(
            "{} records after the last section",
            remaining.len()
        ));
    }
    records.iter().try_for_each(check)?;
    let count = |matches: fn(&Record) -> bool| records.iter().filter(|r| matches(r)).count();
    if count(|r| matches!(r, Record::Encode { .. })) != ENCODE_COUNT
        || count(|r| matches!(r, Record::Decode { .. })) != DECODE_COUNT
    {
        return Err("unexpected encode or decode count".to_owned());
    }
    if count(|r| matches!(r, Record::Decode { case, .. } if case == "upstream")) != UPSTREAM_COUNT {
        return Err(format!("expected {UPSTREAM_COUNT} upstream decodes"));
    }
    let outcome = |rc: i32| {
        records
            .iter()
            .any(|r| matches!(r, Record::Decode { rc: actual, .. } if *actual == rc))
    };
    if !outcome(0) || !outcome(-1) {
        return Err("must record both successful and failed decodes".to_owned());
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
    for corrupt in [
        replace("gsm0503-xcch-v1", "gsm0503-xcch-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"crc""#, r#""kind":"other""#),
        replace(r#""kind":"map""#, r#""kind":"unmap""#),
        replace(r#""n":2"#, r#""m":2"#),
        replace(r#""n":2"#, r#""n":02"#),
        replace(r#""next_output":"0 3"#, r#""next_output":"3"#),
        edit_line(r#""kind":"interleave""#, "a5a5a5a5", "a5a5a500"),
        edit_line(r#""kind":"map""#, r#""hl":-1"#, r#""hl":2"#),
        edit_line(r#""kind":"map""#, "a5a5a5a5", "a5a5a500"),
        edit_line(r#""kind":"encode""#, "a5a5a5a5", "a5a5a500"),
        edit_line(r#""kind":"encode""#, r#""l2":"03"#, r#""l2":""#),
        edit_line(r#""kind":"decode""#, r#""rc":0"#, r#""rc":1"#),
        edit_line(
            r#""kind":"decode""#,
            r#""n_bits_total":456"#,
            r#""n_bits_total":455"#,
        ),
        edit_line(
            r#""kind":"decode""#,
            r#""n_errors":60"#,
            r#""n_errors":457"#,
        ),
        edit_line(
            r#""kind":"decode""#,
            r#""case":"upstream""#,
            r#""case":"other""#,
        ),
        edit_line(r#""kind":"decode""#, r#""rc":-1"#, r#""rc":0,"extra":1"#),
        edit_line(r#""rc":-1"#, "a5a5a5a5a5a5", "00a5a5a5a5a5"),
        edit_line(r#""kind":"decode""#, "0303010000", "zz03010000"),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
    let flags = lines
        .iter()
        .find(|line| line.contains(r#""kind":"encode""#))
        .unwrap();
    let start = flags.find(r#""bursts":""#).unwrap() + r#""bursts":""#.len() + 2 * 57;
    let mut cleared = flags.to_string();
    cleared.replace_range(start..start + 2, "00");
    assert!(parse(&text.replacen(flags, &cleared, 1)).is_err());
    let mut unpacked = flags.to_string();
    unpacked.replace_range(start - 2..start, "02");
    assert!(parse(&text.replacen(flags, &unpacked, 1)).is_err());
    let negative = lines
        .iter()
        .find(|line| line.contains(r#""kind":"decode""#))
        .unwrap();
    let input = negative.find(r#""input":""#).unwrap() + r#""input":""#.len();
    let mut out_of_range = negative.to_string();
    out_of_range.replace_range(input..input + 2, "80");
    assert!(parse(&text.replacen(negative, &out_of_range, 1)).is_err());
}
