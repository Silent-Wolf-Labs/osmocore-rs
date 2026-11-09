//! Shared fixture reader for the authentication core, Milenage and XOR-3G. Checks the
//! shape of the C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0xe4f823de80e3a628;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("alg_name", 9),
    ("alg_parse", 21),
    ("type_name", 4),
    ("c2", 34),
    ("c3", 16),
    ("c4", 8),
    ("from_2g", 16),
    ("opc", 12),
    ("milenage", 28),
    ("auts", 24),
    ("check", 6),
    ("gen_vec", 224),
    ("gen_vec_auts", 47),
];
/// The byte the harness fills outputs with before each call.
pub const GUARD: u8 = 0x55;
/// C's `-EINVAL`, `-ENOENT` and `-ENOTSUP`.
pub const EINVAL: i128 = -22;
pub const ENOENT: i128 = -2;
pub const ENOTSUP: i128 = -95;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    /// Exactly this many bytes of lowercase hex.
    Hex(usize),
    /// Up to this many bytes of lowercase hex.
    HexUpTo(usize),
    Int,
    Text,
}

use Type::{Hex, HexUpTo, Int, Text};

const SUBSCRIBER: &[(&str, Type)] = &[
    ("type", Int),
    ("algo", Int),
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
];

fn layout(kind: &str) -> Option<Vec<(&'static str, Type)>> {
    let fixed: &[(&str, Type)] = match kind {
        "alg_name" => &[("algo", Int), ("name", Text)],
        "alg_parse" => &[("text", Text), ("algo", Int)],
        "type_name" => &[("type", Int), ("name", Text)],
        "c2" => &[("func", Int), ("res", HexUpTo(16)), ("sres", Hex(4))],
        "c3" => &[("ck", Hex(16)), ("ik", Hex(16)), ("kc", Hex(8))],
        "c4" => &[("kc", Hex(8)), ("ck", Hex(16))],
        "from_2g" => &[
            ("auth_types", Int),
            ("kc", Hex(8)),
            ("rc", Int),
            ("ck", Hex(16)),
            ("ik", Hex(16)),
        ],
        "opc" => &[("k", Hex(16)), ("op", Hex(16)), ("opc", Hex(16))],
        "milenage" => &[
            ("k", Hex(16)),
            ("opc", Hex(16)),
            ("rand", Hex(16)),
            ("sqn", Hex(6)),
            ("amf", Hex(2)),
            ("mac_a", Hex(8)),
            ("mac_s", Hex(8)),
            ("res", Hex(8)),
            ("ck", Hex(16)),
            ("ik", Hex(16)),
            ("ak", Hex(6)),
            ("ak_star", Hex(6)),
            ("sres", Hex(4)),
            ("kc", Hex(8)),
        ],
        "auts" => &[
            ("k", Hex(16)),
            ("opc", Hex(16)),
            ("rand", Hex(16)),
            ("auts", Hex(14)),
            ("rc", Int),
            ("sqn", Hex(6)),
        ],
        "check" => &[
            ("k", Hex(16)),
            ("opc", Hex(16)),
            ("sqn", Hex(6)),
            ("rand", Hex(16)),
            ("autn", Hex(16)),
            ("rc", Int),
            ("res_len", Int),
            ("res", Hex(8)),
            ("ck", Hex(16)),
            ("ik", Hex(16)),
            ("auts", Hex(14)),
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
    Some(fixed.to_vec())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bytes(Vec<u8>),
    Int(i128),
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

    pub fn has(&self, key: &str) -> bool {
        self.fields.iter().any(|(name, _)| name == key)
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

    pub fn text(&self, key: &str) -> &str {
        match self.value(key) {
            Value::Text(text) => text,
            other => panic!("{key} is {other:?}"),
        }
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
                Int => unreachable!(),
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
    let (kind, rest) = body
        .split_once('"')
        .ok_or_else(|| format!("bad kind: {line}"))?;
    let layout = layout(kind).ok_or_else(|| format!("unknown kind {kind}"))?;
    let mut rest = rest;
    let mut fields = Vec::with_capacity(layout.len() + 2);
    let mut take = |fields: &mut Vec<_>, layout: &[(&str, Type)]| {
        for &(key, expected) in layout {
            let prefix = format!(r#","{key}":"#);
            rest = rest
                .strip_prefix(prefix.as_str())
                .ok_or_else(|| format!("{kind}: expected {key} in {line}"))?;
            // Values contain no commas: hex, numbers and the recorded names.
            let end = rest.find(',').unwrap_or(rest.len());
            fields.push((key.to_owned(), value(&rest[..end], expected)?));
            rest = &rest[end..];
        }
        Ok::<_, String>(())
    };
    take(&mut fields, &layout)?;
    if kind.starts_with("gen_vec") {
        // The subscriber after the call: Ki for a GSM subscriber, otherwise the SQNs.
        let gsm = fields.first().map(|(_, value)| value) == Some(&Value::Int(1));
        let tail: &[(&str, Type)] = if gsm {
            &[("out_key", Hex(16))]
        } else {
            &[("out_sqn", Int), ("out_sqn_ms", Int)]
        };
        take(&mut fields, tail)?;
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

fn check(record: &Record) -> Result<(), String> {
    match record.kind.as_str() {
        "alg_parse" => known(record.int("algo"), &[-22, 0, 1, 2, 3, 4, 5, 6, 7], "algo"),
        "c2" => known(record.int("func"), &[1, 2], "c2 function"),
        "from_2g" => known(record.int("rc"), &[0, 1], "result"),
        "auts" => known(record.int("rc"), &[0, -1], "result"),
        "check" => known(record.int("rc"), &[0, -1, -2], "result"),
        "gen_vec" | "gen_vec_auts" => {
            known(record.int("algo"), &[0, 1, 2, 3, 4, 5, 6, 7], "algo")?;
            known(record.int("type"), &[0, 1, 2, 3], "type")?;
            known(
                record.int("rc"),
                &[0, -1, -2, -3, EINVAL, ENOTSUP],
                "result",
            )?;
            if record.int("rc") == 0 {
                return Ok(());
            }
            // C writes nothing to the vector when it fails.
            let untouched = ["out_rand", "autn", "ck", "ik", "res", "kc", "sres"]
                .iter()
                .all(|key| record.bytes(key).iter().all(|&byte| byte == GUARD))
                && record.int("res_len") == record.int("in_res_len")
                && record.int("auth_types") == 0x5555_5555;
            if untouched {
                Ok(())
            } else {
                Err(format!("failed {} wrote the vector", record.kind))
            }
        }
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"auth-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
    // The names cover each C enum value once, in order, and one past the end.
    for (index, record) in records.iter().take(9).enumerate() {
        if record.int("algo") != index as i128 {
            return Err(format!("algorithm names out of order at {index}"));
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
    let failed_vector = lines
        .iter()
        .find(|line| line.contains(r#""kind":"gen_vec""#) && line.contains(r#""rc":-22"#))
        .expect("a failed vector");
    let wrote = failed_vector.replacen(r#""sres":"55555555""#, r#""sres":"55555554""#, 1);
    for corrupt in [
        replace("auth-v1", "auth-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"alg_name""#, r#""kind":"other""#),
        replace(r#""kind":"c3""#, r#""kind":"c4""#),
        replace(r#""algo":1,"name""#, r#""algo":2,"name""#),
        replace(r#""algo":1,"name""#, r#""algo":01,"name""#),
        replace(r#""func":1"#, r#""func":3"#),
        replace(
            r#""kind":"c2","func":1,"res":"""#,
            r#""kind":"c2","func":1,"res":"0""#,
        ),
        replace(r#""kind":"opc","k":"465b"#, r#""kind":"opc","k":"465B"#),
        replace(r#""kind":"opc","k":"465b"#, r#""kind":"opc","k":"46"#),
        replace(r#""kind":"opc","k":"#, r#""kind":"opc","key":"#),
        replace(r#","rc":-2,"res_len""#, r#","rc":-4,"res_len""#),
        replace(r#""rc":-22"#, r#""rc":-21"#),
        replace(r#""type":2,"algo":5"#, r#""type":4,"algo":5"#),
        replace(r#""type":2,"algo":5"#, r#""type":2,"algo":8"#),
        replace(failed_vector, &wrote),
        replace(r#""name":"None"}"#, r#""name":"None","extra":1}"#),
        replace(r#","out_sqn_ms":0}"#, r#"}"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
