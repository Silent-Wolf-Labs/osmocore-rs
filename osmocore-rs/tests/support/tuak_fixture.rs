//! Shared fixture reader for TUAK: the TUAK functions, and TUAK vectors through the
//! authentication core. Checks the shape of the C observations; the expected results come
//! only from the recording.

pub const FINGERPRINT: u64 = 0xc88abc9fe5ba3b58;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("opc", 36),
    ("f1", 74),
    ("f2345", 73),
    ("f5star", 26),
    ("gen_vec", 205),
    ("gen_vec_auts", 39),
];
/// Upstream records at the start of each function's section, in the order of
/// `SECTIONS`: one per test set, and for f1 one for f1 and one for f1*.
pub const UPSTREAM: [usize; 4] = [6, 12, 6, 6];
/// The byte the harness fills outputs with before each call.
pub const GUARD: u8 = 0x55;
pub const GUARD_BYTES: usize = 4;
/// C's `-EINVAL`.
pub const EINVAL: i128 = -22;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    /// Exactly this many bytes of lowercase hex.
    Hex(usize),
    /// Up to this many bytes of lowercase hex.
    HexUpTo(usize),
    Int,
    Bool,
    Text,
}

use Type::{Bool, Hex, HexUpTo, Int, Text};

const KEYS: &[(&str, Type)] = &[("source", Text), ("k", Hex(32)), ("k_len", Int)];
const SUBSCRIBER: &[(&str, Type)] = &[
    ("type", Int),
    ("key", Hex(32)),
    ("k_len", Int),
    ("opc", Hex(32)),
    ("opc_len", Int),
    ("opc_is_op", Int),
    ("amf", Hex(2)),
    ("sqn", Int),
    ("ind_bitlen", Int),
    ("ind", Int),
    ("in_res_len", Int),
];
const OUTCOME: &[(&str, Type)] = &[
    ("rc", Int),
    ("out_rand", Hex(16)),
    ("autn", Hex(16)),
    ("ck", Hex(16)),
    ("ik", Hex(16)),
    ("res", Hex(16)),
    ("res_len", Int),
    ("kc", Hex(8)),
    ("sres", Hex(4)),
    ("auth_types", Int),
    ("out_sqn", Int),
    ("out_sqn_ms", Int),
];

fn layout(kind: &str) -> Option<Vec<(&'static str, Type)>> {
    let after_keys: &[(&str, Type)] = match kind {
        "opc" => &[
            ("top", Hex(32)),
            ("iterations", Int),
            ("rc", Int),
            ("topc", HexUpTo(32 + GUARD_BYTES)),
        ],
        "f1" => {
            let mut fields = vec![("source", Text), ("star", Bool)];
            fields.extend_from_slice(&KEYS[1..]);
            fields.extend_from_slice(&[
                ("opc", Hex(32)),
                ("rand", Hex(16)),
                ("sqn", Hex(6)),
                ("amf", Hex(2)),
                ("mac_len", Int),
                ("iterations", Int),
                ("rc", Int),
                ("mac", HexUpTo(32 + GUARD_BYTES)),
            ]);
            return Some(fields);
        }
        "f2345" => &[
            ("opc", Hex(32)),
            ("rand", Hex(16)),
            ("res_len", Int),
            ("ck_len", Int),
            ("ik_len", Int),
            ("iterations", Int),
            ("rc", Int),
            ("res", HexUpTo(32 + GUARD_BYTES)),
            ("ck", HexUpTo(32 + GUARD_BYTES)),
            ("ik", HexUpTo(32 + GUARD_BYTES)),
            ("ak", HexUpTo(6 + GUARD_BYTES)),
        ],
        "f5star" => &[
            ("opc", Hex(32)),
            ("rand", Hex(16)),
            ("iterations", Int),
            ("rc", Int),
            ("ak", HexUpTo(6 + GUARD_BYTES)),
        ],
        "gen_vec" | "gen_vec_auts" => {
            let mut fields = SUBSCRIBER.to_vec();
            if kind == "gen_vec_auts" {
                fields.push(("auts", Hex(14)));
                fields.push(("rand_auts", Hex(16)));
            }
            fields.push(("rand", Hex(16)));
            fields.extend_from_slice(OUTCOME);
            return Some(fields);
        }
        _ => return None,
    };
    let mut fields = KEYS.to_vec();
    fields.extend_from_slice(after_keys);
    Some(fields)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bytes(Vec<u8>),
    Int(i128),
    Bool(bool),
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

    pub fn int(&self, key: &str) -> i128 {
        match self.value(key) {
            Value::Int(value) => *value,
            other => panic!("{key} is {other:?}"),
        }
    }

    pub fn flag(&self, key: &str) -> bool {
        match self.value(key) {
            Value::Bool(value) => *value,
            other => panic!("{key} is {other:?}"),
        }
    }

    pub fn text(&self, key: &str) -> &str {
        match self.value(key) {
            Value::Text(text) => text,
            other => panic!("{key} is {other:?}"),
        }
    }

    /// The bytes a TUAK function wrote to `key`, without the guard bytes after them.
    pub fn written(&self, key: &str) -> &[u8] {
        let bytes = self.bytes(key);
        &bytes[..bytes.len() - GUARD_BYTES]
    }
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn value(raw: &str, expected: Type) -> Result<Value, String> {
    match expected {
        Int => {
            let number: i128 = raw.parse().map_err(|_| format!("bad number {raw}"))?;
            if number.to_string() == raw {
                Ok(Value::Int(number))
            } else {
                Err(format!("non-canonical number {raw}"))
            }
        }
        Bool => match raw {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("bad boolean {raw}")),
        },
        Text | Hex(_) | HexUpTo(_) => {
            let text = raw
                .strip_prefix('"')
                .and_then(|raw| raw.strip_suffix('"'))
                .ok_or_else(|| format!("not a string: {raw}"))?;
            if text.contains(['"', '\\']) {
                return Err(format!("unexpected escape in {raw}"));
            }
            let limit = match expected {
                Text => return Ok(Value::Text(text.to_owned())),
                Hex(len) if text.len() != 2 * len => {
                    return Err(format!("expected {len} bytes: {text}"));
                }
                Hex(len) | HexUpTo(len) => len,
                Int | Bool => unreachable!(),
            };
            if text.len() % 2 != 0
                || text.len() > 2 * limit
                || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            {
                return Err(format!("bad hex {text}"));
            }
            (0..text.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&text[at..at + 2], 16))
                .collect::<Result<_, _>>()
                .map(Value::Bytes)
                .map_err(|_| format!("bad hex {text}"))
        }
    }
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
    for (key, expected) in layout {
        let prefix = format!(r#","{key}":"#);
        rest = rest
            .strip_prefix(prefix.as_str())
            .ok_or_else(|| format!("{kind}: expected {key} in {line}"))?;
        // Values contain no commas: hex, numbers, booleans and the source names.
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

fn known(value: i128, allowed: &[i128], what: &str) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("unexpected {what} {value}"))
    }
}

/// Whether C accepts a function's lengths: K and, for each output, the lengths TUAK
/// defines.
fn lengths_valid(record: &Record) -> bool {
    let k = matches!(record.int("k_len"), 16 | 32);
    let wide = |key| matches!(record.int(key), 16 | 32);
    k && match record.kind.as_str() {
        "f1" => matches!(record.int("mac_len"), 8 | 16 | 32),
        "f2345" => {
            matches!(record.int("res_len"), 4 | 8 | 16 | 32) && wide("ck_len") && wide("ik_len")
        }
        _ => true,
    }
}

/// Each output holds what C wrote, then guard bytes it left alone.
fn outputs(record: &Record) -> Result<(), String> {
    let ok = record.int("rc") == 0;
    let len = |key: &str| if ok { record.int(key) as usize } else { 0 };
    let expected: Vec<(&str, usize)> = match record.kind.as_str() {
        "opc" => vec![("topc", if ok { 32 } else { 0 })],
        "f1" => vec![("mac", len("mac_len"))],
        "f2345" => vec![
            ("res", len("res_len")),
            ("ck", len("ck_len")),
            ("ik", len("ik_len")),
            ("ak", if ok { 6 } else { 0 }),
        ],
        _ => vec![("ak", if ok { 6 } else { 0 })],
    };
    for (key, written) in expected {
        let bytes = record.bytes(key);
        if bytes.len() != written + GUARD_BYTES || bytes[written..].iter().any(|&b| b != GUARD) {
            return Err(format!(
                "{} {key} has the wrong length or guard",
                record.kind
            ));
        }
    }
    Ok(())
}

fn check(record: &Record) -> Result<(), String> {
    match record.kind.as_str() {
        "opc" | "f1" | "f2345" | "f5star" => {
            if !matches!(record.text("source"), "upstream" | "sweep") {
                return Err(format!("bad source {}", record.text("source")));
            }
            if record.int("iterations") < 1 {
                return Err("Keccak runs at least once".to_owned());
            }
            known(record.int("rc"), &[0, EINVAL], "result")?;
            if (record.int("rc") == 0) != lengths_valid(record) {
                return Err(format!("{} lengths and result disagree", record.kind));
            }
            outputs(record)
        }
        _ => {
            known(record.int("type"), &[0, 1, 2, 3], "type")?;
            known(record.int("rc"), &[0, -1, -2, -3, EINVAL], "result")?;
            let guarded =
                |key: &str, from: usize| record.bytes(key)[from..].iter().all(|&b| b == GUARD);
            if record.int("rc") != 0 {
                // C writes nothing to the vector when it fails.
                let untouched = ["out_rand", "autn", "ck", "ik", "res", "kc", "sres"]
                    .iter()
                    .all(|key| guarded(key, 0))
                    && record.int("res_len") == record.int("in_res_len")
                    && record.int("auth_types") == 0x5555_5555;
                return if untouched {
                    Ok(())
                } else {
                    Err(format!("failed {} wrote the vector", record.kind))
                };
            }
            // TUAK writes RES only as long as asked, and keeps the length.
            let res_len = record.int("in_res_len");
            if !matches!(res_len, 4 | 8 | 16)
                || record.int("res_len") != res_len
                || !guarded("res", res_len as usize)
                || record.int("auth_types") != 3
                || record.bytes("out_rand") != record.bytes("rand")
            {
                Err(format!("{} vector has the wrong shape", record.kind))
            } else {
                Ok(())
            }
        }
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"tuak-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let records = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    let mut remaining = records.as_slice();
    for (index, (kind, count)) in SECTIONS.iter().enumerate() {
        if remaining.len() < *count || remaining[..*count].iter().any(|r| r.kind != *kind) {
            return Err(format!("expected {count} {kind} records here"));
        }
        if let Some(&upstream) = UPSTREAM.get(index) {
            let in_place = remaining[..*count]
                .iter()
                .enumerate()
                .all(|(at, r)| (r.text("source") == "upstream") == (at < upstream));
            if !in_place {
                return Err(format!("expected {upstream} upstream {kind} records first"));
            }
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
    let failed_vector = lines
        .iter()
        .find(|line| line.contains(r#""kind":"gen_vec""#) && line.contains(r#""rc":-22"#))
        .expect("a failed vector");
    let wrote = failed_vector.replacen(r#""sres":"55555555""#, r#""sres":"55555554""#, 1);
    let good_vector = lines
        .iter()
        .find(|line| {
            line.contains(r#""kind":"gen_vec""#)
                && line.contains(r#""in_res_len":4,"#)
                && line.contains(r#""rc":0,"#)
        })
        .expect("a 4-byte RES vector");
    let long_res = good_vector.replacen(r#""res_len":4,"#, r#""res_len":8,"#, 1);
    for corrupt in [
        replace("tuak-v1", "tuak-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"opc""#, r#""kind":"other""#),
        replace(r#""kind":"f5star""#, r#""kind":"f2345""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"upstream""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""star":false"#, r#""star":0"#),
        replace(r#""k_len":16,"#, r#""k_len":016,"#),
        replace(r#""iterations":1,"rc":0"#, r#""iterations":0,"rc":0"#),
        replace(r#""iterations":1,"rc":0"#, r#""iterations":1,"rc":-22"#),
        replace(r#""rc":-22"#, r#""rc":-21"#),
        replace(r#""mac_len":8,"#, r#""mac_len":12,"#),
        replace(r#""res_len":4,"ck_len""#, r#""res_len":2,"ck_len""#),
        replace(r#"55555555"}"#, r#"55555554"}"#),
        replace(r#"55555555"}"#, r#"5555555555"}"#),
        replace(r#""type":2,"#, r#""type":4,"#),
        replace(failed_vector, &wrote),
        replace(good_vector, &long_res),
        replace(r#","out_sqn_ms":0}"#, r#"}"#),
        replace(r#","out_sqn_ms":0}"#, r#","out_sqn_ms":0,"extra":1}"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
