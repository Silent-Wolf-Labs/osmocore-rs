//! Shared PRBS fixture reader. Checks the shape of the C observations; the
//! expected bits and states come only from the recording.

pub const FINGERPRINT: u64 = 0x18fc799fd079209a;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
pub const RECORD_COUNT: usize = 199;
/// Bytes after each destination, which the harness fills with `FILL`.
pub const GUARD: usize = 4;
pub const FILL: u8 = 0xa5;
pub const CASES: &[&str] = &[
    "prbs7_two_periods",
    "prbs9_two_periods",
    "prbs11_two_periods",
    "prbs15_two_periods",
    "prbs7_bit_steps",
    "prbs9_bit_steps",
    "prbs11_bit_steps",
    "prbs15_bit_steps",
    "prbs7_mixed_calls",
    "prbs9_zero_length",
    "custom_zero_coefficient",
    "custom_fixed_point",
    "custom_64_bit_register",
    "custom_length_ignored",
];
const FIELDS: &[&str] = &[
    "case", "step", "op", "name", "len", "coeff", "out_len", "ret", "dst_hex", "state",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Init,
    Ubit,
    Ubits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub case: String,
    pub step: usize,
    pub op: Op,
    pub name: String,
    pub len: u32,
    pub coeff: u64,
    pub out_len: usize,
    pub ret: i32,
    /// The complete destination after the call, including the guard bytes.
    pub dst: Vec<u8>,
    pub state: u64,
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

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    if keys != FIELDS {
        return Err(format!("unexpected fields {keys:?}"));
    }
    let value = |index: usize| fields[index].1;
    Ok(Record {
        case: value(0).to_owned(),
        step: parsed(value(1))?,
        op: match value(2) {
            "init" => Op::Init,
            "ubit" => Op::Ubit,
            "ubits" => Op::Ubits,
            other => return Err(format!("unknown op {other}")),
        },
        name: value(3).to_owned(),
        len: parsed(value(4))?,
        coeff: parsed(value(5))?,
        out_len: parsed(value(6))?,
        ret: parsed(value(7))?,
        dst: hex(value(8))?,
        state: parsed(value(9))?,
    })
}

fn check(record: &Record) -> Result<(), String> {
    let context = format!("{} step {}", record.case, record.step);
    if (record.op == Op::Init) != (record.step == 0) {
        return Err(format!(
            "{context}: each case must start with its only init"
        ));
    }
    match record.op {
        Op::Init if record.state != 1 => {
            return Err(format!("{context}: init must set state 1"));
        }
        Op::Init | Op::Ubit if record.out_len != 0 || !record.dst.is_empty() => {
            return Err(format!("{context}: only ubits has a destination"));
        }
        Op::Init if record.ret != 0 => return Err(format!("{context}: init has no result")),
        Op::Ubit if !matches!(record.ret, 0 | 1) => {
            return Err(format!("{context}: a bit must be 0 or 1"));
        }
        Op::Ubits => {
            if record.ret != record.out_len as i32 {
                return Err(format!("{context}: ubits must return its length"));
            }
            if record.dst.len() != record.out_len + GUARD {
                return Err(format!("{context}: destination must include the guard"));
            }
            let (bits, guard) = record.dst.split_at(record.out_len);
            if bits.iter().any(|bit| *bit > 1) {
                return Err(format!("{context}: output must be unpacked bits"));
            }
            if guard.iter().any(|byte| *byte != FILL) {
                return Err(format!("{context}: wrote past the requested length"));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Check everything but the fingerprint, so corrupted copies reach these checks.
pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"prbs-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let records = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    if records.len() != RECORD_COUNT {
        return Err(format!(
            "expected {RECORD_COUNT} records, got {}",
            records.len()
        ));
    }
    let mut seen = Vec::new();
    for (index, current) in records.iter().enumerate() {
        check(current)?;
        if current.step == 0 {
            seen.push(current.case.as_str());
            continue;
        }
        let previous = &records[index - 1];
        let continues = previous.case == current.case
            && previous.step + 1 == current.step
            && previous.name == current.name
            && previous.len == current.len
            && previous.coeff == current.coeff;
        if !continues {
            return Err(format!(
                "{} step {} is out of order",
                current.case, current.step
            ));
        }
    }
    if seen != CASES {
        return Err(format!("unexpected cases {seen:?}"));
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
    let without_last = text.lines().collect::<Vec<_>>();
    let truncated = without_last[..without_last.len() - 1].join("\n");
    for corrupt in [
        replace("prbs-v1", "prbs-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace("\"step\":1", "\"step\":2"),
        replace("\"op\":\"init\"", "\"op\":\"ubit\""),
        replace("\"step\":1,\"op\":\"ubits\"", "\"step\":1,\"op\":\"init\""),
        replace("\"op\":\"ubits\"", "\"op\":\"other\""),
        replace("\"ret\":127", "\"ret\":126"),
        replace("a5a5a5a5", "a5a5a500"),
        replace("\"dst_hex\":\"01", "\"dst_hex\":\"02"),
        replace("\"state\":1}", "\"state\":2}"),
        replace("\"state\":1}", "\"state\":01}"),
        replace("\"len\":7", "\"length\":7"),
        replace("prbs9_zero_length", "prbs9_other"),
        truncated,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
