//! Shared reader for the GSM 05.03 TCH/FS, TCH/EFS and FACCH/F fixture. Checks the
//! shape of the C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x61160d5de4bbf2e8;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Bytes after each destination, which the harness fills with `FILL`.
pub const GUARD: usize = 4;
pub const FILL: u8 = 0xa5;
pub const BURST_BITS: usize = 116;
pub const BURST_DATA_BITS: usize = 114;
pub const BURSTS_LEN: usize = 8 * BURST_BITS;
pub const INTERLEAVED_LEN: usize = 8 * BURST_DATA_BITS;
pub const CODED_LEN: usize = 456;
pub const FR_BYTES: usize = 33;
pub const EFR_BYTES: usize = 31;
pub const FACCH_BYTES: usize = 23;
/// Bits of one block in its eight bursts: half of each burst's data and one stealing
/// flag per burst. The encoders leave the other positions alone.
pub const BLOCK_BITS: usize = CODED_LEN + 8;
/// `upstream` decodes replaying `test_fr`, and `upstream_offset` FACCH/F decodes
/// replaying `test_facch`, in `tests/coding/coding_test.ok`.
pub const UPSTREAM_DECODES: usize = 5;
pub const UPSTREAM_FACCH_DECODES: usize = 9;

/// Record groups in file order, with their sizes.
pub const SECTIONS: &[(&str, usize)] = &[
    ("table", 4),
    ("interleave", 6),
    ("burst", 12),
    ("upstream", 22),
    ("encode", 808),
    ("decode", 654),
];

/// The tables, in file order, with their lengths.
pub const TABLES: [(&str, usize); 4] = [
    ("gsm610_bitorder", 260),
    ("gsm660_bitorder", 260),
    ("gsm0503_gsm_fr_map", 76),
    ("gsm0503_gsm_efr_protected_bits", 65),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Table {
        name: String,
        values: Vec<u16>,
    },
    /// `gsm0503_tch_fr_interleave()`, or with `inverse` `_deinterleave()`.
    Interleave {
        inverse: bool,
        case: String,
        input: Vec<u8>,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    /// `gsm0503_tch_burst_map()`. `h` of -1 passes no stealing flag.
    Map {
        odd: bool,
        h: i32,
        input: Vec<u8>,
        /// Includes the guard bytes.
        output: Vec<u8>,
    },
    /// `gsm0503_tch_burst_unmap()`, with the stealing flag it returned.
    Unmap {
        odd: bool,
        h: i8,
        input: Vec<i8>,
        /// Includes the guard bytes.
        output: Vec<i8>,
    },
    /// `gsm0503_tch_fr_encode()` into bursts that start as `FILL`.
    Encode {
        case: String,
        net_order: bool,
        data: Vec<u8>,
        rc: i32,
        /// Includes the guard bytes.
        bursts: Vec<u8>,
    },
    /// `gsm0503_tch_fr_decode()`. `n_errors` and `n_bits_total` start as -1.
    Decode {
        case: String,
        net_order: bool,
        efr: bool,
        input: Vec<i8>,
        rc: i32,
        n_errors: i32,
        n_bits_total: i32,
        /// Includes the guard bytes.
        data: Vec<u8>,
    },
    /// `gsm0503_tch_fr_facch_encode()` into bursts that start as `FILL`.
    FacchEncode {
        case: String,
        data: Vec<u8>,
        rc: i32,
        /// Includes the guard bytes.
        bursts: Vec<u8>,
    },
    /// `gsm0503_tch_fr_facch_decode()`. `n_errors` and `n_bits_total` start as -1.
    FacchDecode {
        case: String,
        input: Vec<i8>,
        rc: i32,
        n_errors: i32,
        n_bits_total: i32,
        /// Includes the guard bytes.
        data: Vec<u8>,
    },
    /// Upstream's `test_facch`: three FACCH/F blocks encoded four bursts apart into
    /// sixteen zeroed bursts.
    FacchOffsets {
        data: Vec<u8>,
        bursts: Vec<u8>,
    },
}

impl Record {
    fn section(&self) -> &'static str {
        match self {
            Record::Table { .. } => "table",
            Record::Interleave { .. } => "interleave",
            Record::Map { .. } | Record::Unmap { .. } => "burst",
            Record::FacchOffsets { .. } => "upstream",
            Record::Encode { case, .. } | Record::Decode { case, .. } if case == "upstream" => {
                "upstream"
            }
            Record::FacchDecode { case, .. } if case == "upstream_offset" => "upstream",
            Record::Encode { case, .. } | Record::FacchEncode { case, .. }
                if !case.ends_with("_source") =>
            {
                "encode"
            }
            _ => "decode",
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

fn soft(bytes: Vec<u8>) -> Vec<i8> {
    bytes.into_iter().map(|byte| byte as i8).collect()
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

fn flag(value: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("bad flag {value}")),
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
        Some(r#""table""#) => {
            expect(&["kind", "name", "values"])?;
            Ok(Record::Table {
                name: text(1)?,
                values: string(value(2))?
                    .split(' ')
                    .map(parsed)
                    .collect::<Result<_, _>>()?,
            })
        }
        Some(kind @ (r#""interleave""# | r#""deinterleave""#)) => {
            expect(&["kind", "case", "in", "out"])?;
            Ok(Record::Interleave {
                inverse: kind == r#""deinterleave""#,
                case: text(1)?,
                input: bytes(2)?,
                output: bytes(3)?,
            })
        }
        Some(r#""map""#) => {
            expect(&["kind", "odd", "h", "in", "out"])?;
            Ok(Record::Map {
                odd: flag(value(1))?,
                h: parsed(value(2))?,
                input: bytes(3)?,
                output: bytes(4)?,
            })
        }
        Some(r#""unmap""#) => {
            expect(&["kind", "odd", "h", "in", "out"])?;
            Ok(Record::Unmap {
                odd: flag(value(1))?,
                h: parsed(value(2))?,
                input: soft(bytes(3)?),
                output: soft(bytes(4)?),
            })
        }
        Some(r#""encode""#) => {
            expect(&["kind", "case", "net_order", "len", "data", "rc", "bursts"])?;
            let data = bytes(4)?;
            if parsed::<usize>(value(3))? != data.len() {
                return Err("encode length differs from its data".to_owned());
            }
            Ok(Record::Encode {
                case: text(1)?,
                net_order: flag(value(2))?,
                data,
                rc: parsed(value(5))?,
                bursts: bytes(6)?,
            })
        }
        Some(r#""decode""#) => {
            expect(&[
                "kind",
                "case",
                "net_order",
                "efr",
                "input",
                "rc",
                "n_errors",
                "n_bits_total",
                "data",
            ])?;
            Ok(Record::Decode {
                case: text(1)?,
                net_order: flag(value(2))?,
                efr: flag(value(3))?,
                input: soft(bytes(4)?),
                rc: parsed(value(5))?,
                n_errors: parsed(value(6))?,
                n_bits_total: parsed(value(7))?,
                data: bytes(8)?,
            })
        }
        Some(r#""facch_encode""#) => {
            expect(&["kind", "case", "data", "rc", "bursts"])?;
            Ok(Record::FacchEncode {
                case: text(1)?,
                data: bytes(2)?,
                rc: parsed(value(3))?,
                bursts: bytes(4)?,
            })
        }
        Some(r#""facch_decode""#) => {
            expect(&[
                "kind",
                "case",
                "input",
                "rc",
                "n_errors",
                "n_bits_total",
                "data",
            ])?;
            Ok(Record::FacchDecode {
                case: text(1)?,
                input: soft(bytes(2)?),
                rc: parsed(value(3))?,
                n_errors: parsed(value(4))?,
                n_bits_total: parsed(value(5))?,
                data: bytes(6)?,
            })
        }
        Some(r#""facch_offsets""#) => {
            expect(&["kind", "data", "bursts"])?;
            Ok(Record::FacchOffsets {
                data: bytes(1)?,
                bursts: bytes(2)?,
            })
        }
        other => Err(format!("unknown kind {other:?}")),
    }
}

fn guard<T: Copy + Into<i16>>(what: &str, output: &[T], len: usize) -> Result<(), String> {
    if output.len() != len + GUARD {
        return Err(format!("{what}: destination must include the guard"));
    }
    if output[len..]
        .iter()
        .any(|byte| (*byte).into() & 0xff != i16::from(FILL))
    {
        return Err(format!("{what}: wrote past the requested length"));
    }
    Ok(())
}

/// Checks a block's bursts: untouched, or exactly one block's positions written with
/// bits.
fn block_bursts(what: &str, bursts: &[u8], written: bool) -> Result<(), String> {
    guard(what, bursts, BURSTS_LEN)?;
    let bursts = &bursts[..BURSTS_LEN];
    if bursts.iter().any(|bit| *bit > 1 && *bit != FILL) {
        return Err(format!("{what}: bursts must be unpacked bits"));
    }
    let count = bursts.iter().filter(|bit| **bit != FILL).count();
    if count != if written { BLOCK_BITS } else { 0 } {
        return Err(format!("{what}: wrote {count} burst positions"));
    }
    Ok(())
}

fn soft_input(what: &str, input: &[i8]) -> Result<(), String> {
    if input.len() != BURSTS_LEN {
        return Err(format!("{what}: input must be eight bursts"));
    }
    if input.contains(&-128) {
        return Err(format!("{what}: -128 is outside the sbit_t range"));
    }
    Ok(())
}

/// Checks that only the decoded bytes were written.
fn decoded(what: &str, data: &[u8], capacity: usize, rc: i32) -> Result<(), String> {
    guard(what, data, capacity)?;
    let len = usize::try_from(rc).unwrap_or(0);
    if data[len..].iter().any(|byte| *byte != FILL) {
        return Err(format!("{what}: wrote past the decoded frame"));
    }
    Ok(())
}

fn check_table(record: &Record, (expected, len): (&str, usize)) -> Result<(), String> {
    let Record::Table { name, values } = record else {
        return Err(format!("expected the {expected} table"));
    };
    if name != expected || values.len() != len {
        return Err(format!("{name}: unexpected table"));
    }
    let permutation = |values: &[u16]| {
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        sorted.iter().enumerate().all(|(i, v)| usize::from(*v) == i)
    };
    let valid = match name.as_str() {
        "gsm610_bitorder" | "gsm660_bitorder" => permutation(values),
        "gsm0503_gsm_fr_map" => {
            values.iter().map(|v| usize::from(*v)).sum::<usize>() == 260
                && values.iter().all(|v| (1..=8).contains(v))
        }
        _ => {
            let mut sorted = values.clone();
            sorted.sort_unstable();
            sorted.dedup();
            sorted.len() == len && sorted.iter().all(|v| (1..=244).contains(v))
        }
    };
    if !valid {
        return Err(format!("{name}: table values are inconsistent"));
    }
    Ok(())
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Table { .. } => Ok(()),
        Record::Interleave {
            inverse,
            case,
            input,
            output,
        } => {
            let (input_len, output_len) = if *inverse {
                (INTERLEAVED_LEN, CODED_LEN)
            } else {
                (CODED_LEN, INTERLEAVED_LEN)
            };
            if input.len() != input_len {
                return Err(format!("{case}: unexpected interleaver input"));
            }
            guard(case, output, output_len)
        }
        Record::Map {
            h, input, output, ..
        } => {
            if !(-1..=1).contains(h)
                || input.len() != BURST_DATA_BITS
                || input.iter().any(|b| *b > 1)
            {
                return Err("map: unexpected input".to_owned());
            }
            guard("map", output, BURST_BITS)?;
            let written = output[..BURST_BITS].iter().filter(|b| **b != FILL).count();
            if written != BURST_DATA_BITS / 2 + usize::from(*h >= 0) {
                return Err(format!("map: wrote {written} positions"));
            }
            Ok(())
        }
        Record::Unmap { input, output, .. } => {
            if input.len() != BURST_BITS {
                return Err("unmap: unexpected input".to_owned());
            }
            guard("unmap", output, BURST_DATA_BITS)
        }
        Record::Encode {
            case,
            data,
            rc,
            bursts,
            ..
        } => {
            let valid = matches!(data.len(), 0 | FACCH_BYTES | EFR_BYTES | FR_BYTES);
            if *rc != if valid { 0 } else { -1 } {
                return Err(format!("{case}: unexpected result {rc}"));
            }
            block_bursts(case, bursts, valid)
        }
        Record::FacchEncode {
            case,
            data,
            rc,
            bursts,
        } => {
            if data.len() != FACCH_BYTES || *rc != 0 {
                return Err(format!("{case}: unexpected FACCH/F encode"));
            }
            block_bursts(case, bursts, true)
        }
        Record::Decode {
            case,
            efr,
            input,
            rc,
            n_errors,
            n_bits_total,
            data,
            ..
        } => {
            soft_input(case, input)?;
            let consistent = match rc {
                -1 => matches!(n_bits_total, 378 | 456),
                23 => *n_bits_total == 456,
                31 => *efr && *n_bits_total == 378,
                33 => !*efr && *n_bits_total == 378,
                _ => false,
            };
            if !consistent || !(0..=*n_bits_total).contains(n_errors) {
                return Err(format!("{case}: inconsistent decode result"));
            }
            decoded(case, data, FR_BYTES, *rc)
        }
        Record::FacchDecode {
            case,
            input,
            rc,
            n_errors,
            n_bits_total,
            data,
        } => {
            soft_input(case, input)?;
            let consistent = match (rc, n_bits_total) {
                (-1, -1) => *n_errors == -1,
                (-1 | 23, 456) => (0..=456).contains(n_errors),
                _ => false,
            };
            if !consistent {
                return Err(format!("{case}: inconsistent FACCH/F decode result"));
            }
            decoded(case, data, FACCH_BYTES, *rc)
        }
        Record::FacchOffsets { data, bursts } => {
            if data.len() != FACCH_BYTES
                || bursts.len() != 16 * BURST_BITS
                || bursts.iter().any(|bit| *bit > 1)
            {
                return Err("facch_offsets: unexpected record".to_owned());
            }
            Ok(())
        }
    }
}

/// Check everything but the fingerprint, so corrupted copies reach these checks.
pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm0503-tch-fr-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
    for (record, table) in records.iter().zip(TABLES) {
        check_table(record, table)?;
    }
    records.iter().try_for_each(check)?;
    let count = |matches: &dyn Fn(&Record) -> bool| records.iter().filter(|r| matches(r)).count();
    if count(&|r| matches!(r, Record::Decode { case, .. } if case == "upstream"))
        != UPSTREAM_DECODES
        || count(&|r| matches!(r, Record::FacchDecode { case, .. } if case == "upstream_offset"))
            != UPSTREAM_FACCH_DECODES
    {
        return Err("unexpected number of upstream replays".to_owned());
    }
    for rc in [-1, 23, 31, 33] {
        if count(&|r| matches!(r, Record::Decode { rc: actual, .. } if *actual == rc)) == 0 {
            return Err(format!("must record a decode returning {rc}"));
        }
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
        swapped.swap(4, 5);
        swapped.join("\n") + "\n"
    };
    for (index, corrupt) in [
        replace("gsm0503-tch-fr-v1", "gsm0503-tch-fr-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""name":"gsm660_bitorder""#, r#""name":"gsm610_bitorder""#),
        replace(r#""values":"0 47 "#, r#""values":"0 0 "#),
        replace(r#""kind":"map","odd":0"#, r#""kind":"map","odd":2"#),
        replace(r#""case":"upstream""#, r#""case":"replayed""#),
        replace(
            r#""case":"walking_bit_fr""#,
            r#""case":"walking_bit_fr_source""#,
        ),
        edit_line(r#""kind":"encode""#, r#""rc":0"#, r#""rc":-1"#),
        edit_line(r#""kind":"encode""#, r#""len":33"#, r#""len":32"#),
        edit_line(r#""kind":"encode""#, "a5a5a5a5\"}", "a5a5a500\"}"),
        edit_line(r#""kind":"facch_encode""#, r#""rc":0"#, r#""rc":1"#),
        edit_line(r#""kind":"decode""#, r#""rc":33"#, r#""rc":31"#),
        edit_line(
            r#""kind":"decode""#,
            r#""n_bits_total":378"#,
            r#""n_bits_total":456"#,
        ),
        edit_line(r#""kind":"decode""#, r#""n_errors":0"#, r#""n_errors":-1"#),
        edit_line(r#""kind":"decode""#, "a5a5a5a5\"}", "a5a5a500\"}"),
        edit_line(r#""kind":"decode""#, r#""input":"7f"#, r#""input":"80"#),
        edit_line(
            r#""kind":"facch_decode""#,
            r#""n_bits_total":-1"#,
            r#""n_bits_total":456"#,
        ),
        edit_line(r#""kind":"facch_decode""#, r#""rc":23"#, r#""rc":33"#),
        edit_line(r#""kind":"interleave""#, "a5a5a5a5\"}", "a5a5a5\"}"),
        edit_line(
            r#""kind":"facch_offsets""#,
            r#""bursts":"0"#,
            r#""bursts":"02"#,
        ),
        edit_line(r#""kind":"encode""#, r#""rc":0"#, r#""rc":0,"extra":1"#),
        truncated,
        extended,
        swapped,
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            parse(&corrupt).is_err(),
            "accepted corrupted fixture {index}"
        );
    }
}
