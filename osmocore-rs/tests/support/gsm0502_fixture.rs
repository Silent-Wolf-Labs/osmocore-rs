//! Shared GSM 05.02 fixture reader. Checks the shape of the C observations; the
//! expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x0e7fb297061530d8;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("constant", 24),
    ("frame", 489),
    ("remap", 1398),
    ("ccch", 105),
    ("hop", 2072),
    ("paging", 2048),
    ("group", 240),
];
pub const UPSTREAM_REMAP_COUNT: usize = 258;
pub const CHANNELS: &[&str] = &[
    "TCH/F", "TCH/H0", "TCH/H1", "FACCH/F", "FACCH/H0", "FACCH/H1",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Constant {
        name: String,
        value: u64,
    },
    Frame {
        a: u32,
        b: u32,
        sum: u32,
        sub: u32,
        diff: u32,
        cmp: i32,
        inc: u32,
        dec: u32,
    },
    Remap {
        upstream: bool,
        channel: String,
        frame: u32,
        begin: u32,
    },
    Ccch {
        frame: u32,
        block: i32,
    },
    Hop {
        case: String,
        start: u32,
        step: u32,
        hsn: u8,
        maio: u8,
        n: usize,
        /// Empty when the call passed no mobile allocation.
        allocation: Vec<u16>,
        values: Vec<u16>,
    },
    Paging {
        conf: u8,
        ag: u8,
        pa: u8,
        imsi: u64,
        blocks: u32,
        subchannels: i32,
        channels: i32,
        group: u32,
    },
    Group {
        imsi: u64,
        channels: u32,
        blocks: i32,
        ccch: u32,
        paging: u32,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::Constant { .. } => "constant",
            Self::Frame { .. } => "frame",
            Self::Remap { .. } => "remap",
            Self::Ccch { .. } => "ccch",
            Self::Hop { .. } => "hop",
            Self::Paging { .. } => "paging",
            Self::Group { .. } => "group",
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

fn list(value: &str) -> Result<Vec<u16>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
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
        Some("constant") => {
            expect(&["kind", "name", "value"])?;
            Ok(Record::Constant {
                name: value(1).to_owned(),
                value: parsed(value(2))?,
            })
        }
        Some("frame") => {
            expect(&["kind", "a", "b", "sum", "sub", "diff", "cmp", "inc", "dec"])?;
            Ok(Record::Frame {
                a: parsed(value(1))?,
                b: parsed(value(2))?,
                sum: parsed(value(3))?,
                sub: parsed(value(4))?,
                diff: parsed(value(5))?,
                cmp: parsed(value(6))?,
                inc: parsed(value(7))?,
                dec: parsed(value(8))?,
            })
        }
        Some("remap") => {
            expect(&["kind", "source", "channel", "fn", "begin"])?;
            Ok(Record::Remap {
                upstream: match value(1) {
                    "upstream" => true,
                    "sweep" => false,
                    other => return Err(format!("unknown source {other}")),
                },
                channel: value(2).to_owned(),
                frame: parsed(value(3))?,
                begin: parsed(value(4))?,
            })
        }
        Some("ccch") => {
            expect(&["kind", "fn", "block"])?;
            Ok(Record::Ccch {
                frame: parsed(value(1))?,
                block: parsed(value(2))?,
            })
        }
        Some("hop") => {
            expect(&[
                "kind", "case", "start", "step", "hsn", "maio", "n", "ma", "values",
            ])?;
            Ok(Record::Hop {
                case: value(1).to_owned(),
                start: parsed(value(2))?,
                step: parsed(value(3))?,
                hsn: parsed(value(4))?,
                maio: parsed(value(5))?,
                n: parsed(value(6))?,
                allocation: list(value(7))?,
                values: list(value(8))?,
            })
        }
        Some("paging") => {
            expect(&[
                "kind",
                "conf",
                "ag",
                "pa",
                "imsi",
                "blocks",
                "subchannels",
                "channels",
                "group",
            ])?;
            Ok(Record::Paging {
                conf: parsed(value(1))?,
                ag: parsed(value(2))?,
                pa: parsed(value(3))?,
                imsi: parsed(value(4))?,
                blocks: parsed(value(5))?,
                subchannels: parsed(value(6))?,
                channels: parsed(value(7))?,
                group: parsed(value(8))?,
            })
        }
        Some("group") => {
            expect(&["kind", "imsi", "channels", "blocks", "ccch", "paging"])?;
            Ok(Record::Group {
                imsi: parsed(value(1))?,
                channels: parsed(value(2))?,
                blocks: parsed(value(3))?,
                ccch: parsed(value(4))?,
                paging: parsed(value(5))?,
            })
        }
        other => Err(format!("unknown kind {other:?}")),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Frame { cmp, .. } if !(-1..=1).contains(cmp) => {
            Err(format!("comparison result {cmp} out of range"))
        }
        Record::Remap { channel, .. } if !CHANNELS.contains(&channel.as_str()) => {
            Err(format!("unknown channel {channel}"))
        }
        Record::Ccch { block, .. } if !(-4..=8).contains(block) => {
            Err(format!("CCCH block {block} out of range"))
        }
        Record::Hop {
            case,
            hsn,
            n,
            allocation,
            values,
            ..
        } => {
            if *hsn > 63 {
                return Err(format!("{case}: HSN {hsn} would read past the C table"));
            }
            if *n == 0 {
                return Err(format!("{case}: an empty allocation divides by zero in C"));
            }
            if !allocation.is_empty() && allocation.len() != *n {
                return Err(format!("{case}: allocation length differs from n"));
            }
            if values.is_empty() {
                return Err(format!("{case}: row has no values"));
            }
            let in_range = |value: &u16| {
                if allocation.is_empty() {
                    usize::from(*value) < *n
                } else {
                    allocation.contains(value)
                }
            };
            if !values.iter().all(in_range) {
                return Err(format!("{case}: value outside the allocation"));
            }
            Ok(())
        }
        Record::Paging { conf, ag, pa, .. } if *conf > 7 || *ag > 7 || *pa > 7 => {
            Err("paging fields hold three bits".to_owned())
        }
        _ => Ok(()),
    }
}

/// Check everything but the fingerprint, so corrupted copies reach these checks.
pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm0502-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        .filter(|record| matches!(record, Record::Remap { upstream: true, .. }))
        .count();
    if upstream != UPSTREAM_REMAP_COUNT {
        return Err(format!("expected {UPSTREAM_REMAP_COUNT} upstream records"));
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
    let hop = lines
        .iter()
        .find(|line| line.contains(r#""kind":"hop""#))
        .unwrap();
    for corrupt in [
        replace("gsm0502-v1", "gsm0502-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"constant""#, r#""kind":"other""#),
        replace(r#""kind":"ccch""#, r#""kind":"group""#),
        replace(r#""value":4615384"#, r#""valeu":4615384"#),
        replace(r#""value":4615384"#, r#""value":04615384"#),
        replace(r#""cmp":1"#, r#""cmp":2"#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""channel":"TCH/F""#, r#""channel":"TCH/G""#),
        replace(r#""block":-1"#, r#""block":-5"#),
        replace(r#""block":0"#, r#""block":9"#),
        replace(r#""hsn":0"#, r#""hsn":64"#),
        replace(r#""n":1,"#, r#""n":0,"#),
        replace(
            r#""maio":1,"n":1,"ma":"512""#,
            r#""maio":1,"n":2,"ma":"512""#,
        ),
        text.replacen(hop, &hop.replace(r#""values":"0"#, r#""values":"1000"#), 1),
        text.replacen(hop, &hop.replace(r#""values":"0"#, r#""values":"x"#), 1),
        replace(r#""conf":0"#, r#""conf":8"#),
        replace(r#""ag":0"#, r#""ag":8"#),
        replace(r#""pa":0"#, r#""pa":8"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
