//! Shared fixture reader for the GPRS ciphering API: GEA3 and GEA4 through
//! `gprs_cipher_run`, the input helpers, key lengths and names. Checks the shape of the
//! C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0xbfa6451a0eb1c36d;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
pub const GUARD: u8 = 0x55;
pub const GUARD_BYTES: usize = 4;
pub const MAX_BLOCK: u16 = 1523;
pub const EINVAL_RESULT: i32 = -22;
pub const ERANGE_RESULT: i32 = -34;
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("algorithm", 8),
    ("input_ui", 97),
    ("input_i", 81),
    ("run", 328),
];
pub const UPSTREAM_RUNS: usize = 13;
pub const RESULT_RUNS: usize = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Upstream,
    Sweep,
    Result,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Algorithm {
        algo: i32,
        supported: i32,
        key_length: u32,
        name: Option<String>,
    },
    InputUi {
        iov_ui: u32,
        sapi: u8,
        lfn: u32,
        oc: u32,
        input: u32,
    },
    InputI {
        iov_i: u32,
        lfn: u32,
        oc: u32,
        input: u32,
    },
    Run {
        source: Source,
        algo: i32,
        key: Vec<u8>,
        iv: u32,
        dir: i32,
        len: u16,
        rc: i32,
        /// The bytes written, then the guard bytes after them.
        out: Vec<u8>,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::Algorithm { .. } => "algorithm",
            Self::InputUi { .. } => "input_ui",
            Self::InputI { .. } => "input_i",
            Self::Run { .. } => "run",
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
            Ok((key, value))
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

fn string(value: &str) -> Result<&str, String> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| format!("not a string: {value}"))
}

fn hex(value: &str) -> Result<Vec<u8>, String> {
    let value = string(value)?;
    if value.len() % 2 != 0
        || !value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!("bad hex {value}"));
    }
    (0..value.len() / 2)
        .map(|index| {
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(|_| format!("bad hex {value}"))
        })
        .collect()
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = fields.iter().map(|(_, value)| *value).collect();
    match (keys.as_slice(), values.as_slice()) {
        (
            ["kind", "algo", "supported", "key_length", "name"],
            [r#""algorithm""#, algo, supported, key_length, name],
        ) => Ok(Record::Algorithm {
            algo: parsed(algo)?,
            supported: parsed(supported)?,
            key_length: parsed(key_length)?,
            name: match *name {
                "null" => None,
                name => Some(string(name)?.to_owned()),
            },
        }),
        (
            ["kind", "iov_ui", "sapi", "lfn", "oc", "input"],
            [r#""input_ui""#, iov_ui, sapi, lfn, oc, input],
        ) => Ok(Record::InputUi {
            iov_ui: parsed(iov_ui)?,
            sapi: parsed(sapi)?,
            lfn: parsed(lfn)?,
            oc: parsed(oc)?,
            input: parsed(input)?,
        }),
        (["kind", "iov_i", "lfn", "oc", "input"], [r#""input_i""#, iov_i, lfn, oc, input]) => {
            Ok(Record::InputI {
                iov_i: parsed(iov_i)?,
                lfn: parsed(lfn)?,
                oc: parsed(oc)?,
                input: parsed(input)?,
            })
        }
        (
            [
                "kind",
                "source",
                "algo",
                "key",
                "iv",
                "dir",
                "len",
                "rc",
                "out",
            ],
            [r#""run""#, source, algo, key, iv, dir, len, rc, out],
        ) => Ok(Record::Run {
            source: match string(source)? {
                "upstream" => Source::Upstream,
                "sweep" => Source::Sweep,
                "result" => Source::Result,
                other => return Err(format!("bad source {other}")),
            },
            algo: parsed(algo)?,
            key: hex(key)?,
            iv: parsed(iv)?,
            dir: parsed(dir)?,
            len: parsed(len)?,
            rc: parsed(rc)?,
            out: hex(out)?,
        }),
        _ => Err(format!("bad record: {line}")),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Algorithm {
            algo,
            supported,
            name,
            ..
        } => {
            let in_range = (0..=4).contains(algo);
            let expected = match algo {
                3 | 4 => 1,
                0..=2 => 0,
                _ => ERANGE_RESULT,
            };
            if *supported != expected {
                Err(format!("GEA{algo} supported is {supported}"))
            } else if name.is_some() != in_range {
                Err(format!("GEA{algo} has the wrong name"))
            } else {
                Ok(())
            }
        }
        Record::InputUi { sapi, .. } if *sapi >= 16 => {
            Err(format!("SAPI {sapi} overflows C's arithmetic"))
        }
        Record::InputI {
            iov_i,
            lfn,
            oc,
            input,
        } if iov_i.wrapping_add(*lfn).wrapping_add(*oc) != *input => {
            Err(format!("input {input} isn't the sum of its parts"))
        }
        Record::Run {
            source,
            algo,
            key,
            len,
            rc,
            out,
            ..
        } => {
            let written = if *rc == 0 { usize::from(*len) } else { 0 };
            let key_lengths: &[usize] = match algo {
                3 => &[8],
                4 => &[16],
                _ => &[8, 16],
            };
            if !matches!(*rc, 0 | EINVAL_RESULT | ERANGE_RESULT) {
                Err(format!("unexpected result {rc}"))
            } else if (*rc == 0) != (matches!(algo, 3 | 4) && *len <= MAX_BLOCK) {
                Err(format!("GEA{algo} of {len} bytes returned {rc}"))
            } else if *source != Source::Result && *rc != 0 {
                Err(format!("a {source:?} run failed"))
            } else if !key_lengths.contains(&key.len()) {
                Err(format!("GEA{algo} with a {}-byte key", key.len()))
            } else if out.len() != written + GUARD_BYTES
                || out[written..].iter().any(|&byte| byte != GUARD)
            {
                Err(format!("GEA{algo} of {len} bytes changed its guard bytes"))
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gea-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
    let sources: Vec<Source> = records
        .iter()
        .filter_map(|record| match record {
            Record::Run { source, .. } => Some(*source),
            _ => None,
        })
        .collect();
    let (upstream, rest) = sources.split_at(UPSTREAM_RUNS);
    let (sweep, results) = rest.split_at(rest.len() - RESULT_RUNS);
    if upstream.iter().any(|s| *s != Source::Upstream)
        || sweep.iter().any(|s| *s != Source::Sweep)
        || results.iter().any(|s| *s != Source::Result)
    {
        return Err(format!(
            "expected {UPSTREAM_RUNS} upstream runs first and {RESULT_RUNS} result runs last"
        ));
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
        replace("gea-v1", "gea-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"algorithm""#, r#""kind":"run""#),
        replace(r#""kind":"input_i""#, r#""kind":"input_ui""#),
        replace(r#""algo":3,"supported":1"#, r#""algo":3,"supported":0"#),
        replace(r#""name":"GEA3""#, r#""name":null"#),
        replace(r#""name":null"#, r#""name":"GEA5""#),
        replace(r#""sapi":15,"#, r#""sapi":16,"#),
        replace(r#""sapi":15,"#, r#""sapi":+15,"#),
        replace(
            r#""iov_i":0,"lfn":0,"oc":4294967295,"input":4294967295"#,
            r#""iov_i":0,"lfn":0,"oc":4294967295,"input":4294967294"#,
        ),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"result""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""key":"2bd6459f82c5bc00""#, r#""key":"2bd6459f82c5bc0""#),
        replace(
            r#""key":"2bd6459f82c5bc00""#,
            r#""key":"2bd6459f82c5bc0000""#,
        ),
        replace(
            r#""rc":-34,"out":"55555555""#,
            r#""rc":-35,"out":"55555555""#,
        ),
        replace(
            r#""rc":-34,"out":"55555555""#,
            r#""rc":-34,"out":"5555555555""#,
        ),
        replace(r#""len":1523,"rc":0"#, r#""len":1524,"rc":0"#),
        replace(r#"55555555"}"#, r#"55555554"}"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
