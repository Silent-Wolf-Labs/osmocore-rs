//! Shared fixture reader for the A5 cipher streams. Checks the shape of the C
//! observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x172077c448e72cbc;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
pub const STREAM_BITS: usize = 114;
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[("fn_count", 77), ("stream", 459), ("result", 11)];
pub const UPSTREAM_STREAMS: usize = 3;
pub const ENOTSUP_RESULT: i32 = -95;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    FnCount {
        frame: u32,
        count: u32,
    },
    Stream {
        upstream: bool,
        n: u8,
        key: [u8; 8],
        frame: u32,
        downlink: Vec<u8>,
        uplink: Vec<u8>,
    },
    Result {
        n: i32,
        rc: i32,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::FnCount { .. } => "fn_count",
            Self::Stream { .. } => "stream",
            Self::Result { .. } => "result",
        }
    }
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
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

fn key(value: &str) -> Result<[u8; 8], String> {
    let mut key = [0; 8];
    if value.len() != 16
        || !value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!("bad key {value}"));
    }
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("bad key {value}"))?;
    }
    Ok(key)
}

fn bits(value: &str) -> Result<Vec<u8>, String> {
    if value.len() != STREAM_BITS {
        return Err(format!("expected {STREAM_BITS} bits: {value}"));
    }
    value
        .bytes()
        .map(|bit| match bit {
            b'0' | b'1' => Ok(bit - b'0'),
            _ => Err(format!("bad bits {value}")),
        })
        .collect()
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = fields.iter().map(|(_, value)| *value).collect();
    match (keys.as_slice(), values.as_slice()) {
        (["kind", "fn", "count"], ["fn_count", frame, count]) => Ok(Record::FnCount {
            frame: parsed(frame)?,
            count: parsed(count)?,
        }),
        (
            ["kind", "source", "n", "key", "fn", "dl", "ul"],
            ["stream", source, n, stream_key, frame, downlink, uplink],
        ) => Ok(Record::Stream {
            upstream: match *source {
                "upstream" => true,
                "sweep" => false,
                _ => return Err(format!("bad source {source}")),
            },
            n: parsed(n)?,
            key: key(stream_key)?,
            frame: parsed(frame)?,
            downlink: bits(downlink)?,
            uplink: bits(uplink)?,
        }),
        (["kind", "n", "rc"], ["result", n, rc]) => Ok(Record::Result {
            n: parsed(n)?,
            rc: parsed(rc)?,
        }),
        _ => Err(format!("bad record: {line}")),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Stream { n, .. } if *n > 2 => Err(format!("unexpected cipher A5/{n}")),
        Record::Stream {
            n: 0,
            downlink,
            uplink,
            ..
        } if downlink.iter().chain(uplink).any(|&bit| bit != 0) => {
            Err("A5/0 stream isn't zero".to_owned())
        }
        Record::Result { n, rc } => {
            let expected = if (0..=2).contains(n) {
                0
            } else {
                ENOTSUP_RESULT
            };
            if *rc == expected {
                Ok(())
            } else {
                Err(format!("A5/{n} returned {rc}"))
            }
        }
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"a5-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let records = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    let mut remaining = records.as_slice();
    for (kind, count) in SECTIONS {
        if remaining.len() < *count || remaining[..*count].iter().any(|r| r.kind() != *kind) {
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
    let upstream = records
        .iter()
        .filter(|record| matches!(record, Record::Stream { upstream: true, .. }))
        .count();
    if upstream != UPSTREAM_STREAMS {
        return Err(format!("expected {UPSTREAM_STREAMS} upstream records"));
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
    for corrupt in [
        replace("a5-v1", "a5-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"fn_count""#, r#""kind":"other""#),
        replace(r#""kind":"result""#, r#""kind":"fn_count""#),
        replace(r#""fn":1,"count":33"#, r#""fn":01,"count":33"#),
        replace(r#""fn":1,"count":33"#, r#""fm":1,"count":33"#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""n":1,"key""#, r#""n":3,"key""#),
        replace(r#""key":"0123456789abcdef""#, r#""key":"0123456789abcde""#),
        replace(r#""key":"0123456789abcdef""#, r#""key":"0123456789abcdeg""#),
        replace(r#""dl":"000"#, r#""dl":"100"#),
        replace(r#""dl":"000"#, r#""dl":"00"#),
        replace(r#""dl":"000"#, r#""dl":"002"#),
        replace(r#""n":5,"rc":-95"#, r#""n":5,"rc":0"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
