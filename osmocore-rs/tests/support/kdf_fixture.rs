//! Shared fixture reader for the key derivation functions: Kc128, K_ASME, K_eNB, NH and
//! the NAS algorithm keys. Checks the shape of the C observations; the expected results
//! come only from the recording.

pub const FINGERPRINT: u64 = 0x2de884daa6e41ad1;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("kc128", 20),
    ("kasme", 24),
    ("enb", 32),
    ("nh", 24),
    ("nas", 158),
];
/// The byte the harness fills outputs with before each call.
pub const GUARD: u8 = 0x55;
pub const GUARD_BYTES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    /// Exactly this many bytes of lowercase hex.
    Hex(usize),
    /// An integer from 0 to this.
    Int(u64),
    Text,
}

use Type::{Hex, Int, Text};

fn layout(kind: &str) -> Option<&'static [(&'static str, Type)]> {
    Some(match kind {
        "kc128" => &[
            ("source", Text),
            ("ck", Hex(16)),
            ("ik", Hex(16)),
            ("kc128", Hex(16 + GUARD_BYTES)),
        ],
        "kasme" => &[
            ("source", Text),
            ("ck", Hex(16)),
            ("ik", Hex(16)),
            ("plmn_id", Hex(3)),
            ("sqn", Hex(6)),
            ("ak", Hex(6)),
            ("kasme", Hex(32 + GUARD_BYTES)),
        ],
        "enb" => &[
            ("source", Text),
            ("kasme", Hex(32)),
            ("ul_count", Int(u32::MAX as u64)),
            ("kenb", Hex(32 + GUARD_BYTES)),
        ],
        "nh" => &[
            ("source", Text),
            ("kasme", Hex(32)),
            ("sync_input", Hex(32)),
            ("nh", Hex(32 + GUARD_BYTES)),
        ],
        "nas" => &[
            ("source", Text),
            ("algo_type", Int(u8::MAX as u64)),
            ("algo_id", Int(u8::MAX as u64)),
            ("kasme", Hex(32)),
            ("knas", Hex(16 + GUARD_BYTES)),
        ],
        _ => return None,
    })
}

/// The sources each kind's records may name.
fn sources(kind: &str) -> &'static [&'static str] {
    match kind {
        "nh" => &["pattern", "chain", "random"],
        "nas" => &["sweep", "edge", "pattern", "random"],
        _ => &["pattern", "random"],
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bytes(Vec<u8>),
    Int(u64),
    Text(String),
}

/// One observation: its kind and its fields in file order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: String,
    pub fields: Vec<(String, Value)>,
}

impl Record {
    fn value(&self, key: &str) -> &Value {
        self.fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("{} has no {key}", self.kind))
    }

    pub fn bytes(&self, key: &str) -> &[u8] {
        match self.value(key) {
            Value::Bytes(bytes) => bytes,
            other => panic!("{key} is {other:?}"),
        }
    }

    pub fn array<const N: usize>(&self, key: &str) -> [u8; N] {
        self.bytes(key)
            .try_into()
            .unwrap_or_else(|_| panic!("{key} isn't {N} bytes"))
    }

    pub fn int(&self, key: &str) -> u64 {
        match self.value(key) {
            Value::Int(value) => *value,
            other => panic!("{key} is {other:?}"),
        }
    }

    pub fn text(&self, key: &str) -> &str {
        match self.value(key) {
            Value::Text(text) => text,
            other => panic!("{key} is {other:?}"),
        }
    }

    /// The output: the bytes C wrote, without the guard bytes after them.
    pub fn output<const N: usize>(&self) -> [u8; N] {
        let bytes = self.bytes(output_key(&self.kind));
        bytes[..bytes.len() - GUARD_BYTES]
            .try_into()
            .unwrap_or_else(|_| panic!("{} output isn't {N} bytes", self.kind))
    }
}

fn output_key(kind: &str) -> &'static str {
    match kind {
        "kc128" => "kc128",
        "kasme" => "kasme",
        "enb" => "kenb",
        "nh" => "nh",
        _ => "knas",
    }
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn value(raw: &str, expected: Type) -> Result<Value, String> {
    if let Int(max) = expected {
        let number: u64 = raw.parse().map_err(|_| format!("bad number {raw}"))?;
        return if number.to_string() != raw {
            Err(format!("non-canonical number {raw}"))
        } else if number > max {
            Err(format!("{number} is past {max}"))
        } else {
            Ok(Value::Int(number))
        };
    }
    let text = raw
        .strip_prefix('"')
        .and_then(|raw| raw.strip_suffix('"'))
        .ok_or_else(|| format!("not a string: {raw}"))?;
    if text.contains(['"', '\\']) {
        return Err(format!("unexpected escape in {raw}"));
    }
    let Hex(len) = expected else {
        return Ok(Value::Text(text.to_owned()));
    };
    if text.len() != 2 * len || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(format!("expected {len} bytes of hex: {text}"));
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16))
        .collect::<Result<_, _>>()
        .map(Value::Bytes)
        .map_err(|_| format!("bad hex {text}"))
}

fn record(line: &str) -> Result<Record, String> {
    let body = line
        .strip_prefix(r#"{"kind":""#)
        .and_then(|line| line.strip_suffix('}'))
        .ok_or_else(|| format!("not a record: {line}"))?;
    let (kind, mut rest) = body
        .split_once('"')
        .ok_or_else(|| format!("bad kind: {line}"))?;
    let layout = layout(kind).ok_or_else(|| format!("unknown kind {kind}"))?;
    let mut fields = Vec::with_capacity(layout.len());
    for &(key, expected) in layout {
        let prefix = format!(r#","{key}":"#);
        rest = rest
            .strip_prefix(prefix.as_str())
            .ok_or_else(|| format!("{kind}: expected {key} in {line}"))?;
        // Values contain no commas: hex, numbers and the source names.
        let end = rest.find(',').unwrap_or(rest.len());
        fields.push((key.to_owned(), value(&rest[..end], expected)?));
        rest = &rest[end..];
    }
    if !rest.is_empty() {
        return Err(format!("{kind}: unexpected {rest}"));
    }
    Ok(Record {
        kind: kind.to_owned(),
        fields,
    })
}

/// A known source, and an output of the right length followed by untouched guard bytes.
fn check(record: &Record) -> Result<(), String> {
    if !sources(&record.kind).contains(&record.text("source")) {
        return Err(format!(
            "bad {} source {}",
            record.kind,
            record.text("source")
        ));
    }
    let output = record.bytes(output_key(&record.kind));
    if output[output.len() - GUARD_BYTES..]
        .iter()
        .any(|&b| b != GUARD)
    {
        return Err(format!("{} wrote past its output", record.kind));
    }
    Ok(())
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"kdf-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let records = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    let mut remaining = records.as_slice();
    for (kind, count) in SECTIONS {
        if remaining.len() < *count || remaining[..*count].iter().any(|r| r.kind != *kind) {
            return Err(format!("expected {count} {kind} records here"));
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
    for corrupt in [
        replace("kdf-v1", "kdf-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"kc128""#, r#""kind":"other""#),
        replace(r#""kind":"nh""#, r#""kind":"enb""#),
        replace(r#""source":"pattern""#, r#""source":"other""#),
        replace(r#""source":"chain""#, r#""source":"sweep""#),
        replace(r#""ul_count":4294967295,"#, r#""ul_count":4294967296,"#),
        replace(r#""ul_count":1,"#, r#""ul_count":01,"#),
        replace(r#""algo_type":0,"#, r#""algo_type":256,"#),
        replace(r#""plmn_id":"00f110""#, r#""plmn_id":"00f1""#),
        replace(r#""plmn_id":"00f110""#, r#""plmn_id":"00F110""#),
        replace(r#"55555555"}"#, r#"55555554"}"#),
        replace(r#"55555555"}"#, r#"5555555555"}"#),
        replace(r#"55555555"}"#, r#"55555555","extra":1}"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
