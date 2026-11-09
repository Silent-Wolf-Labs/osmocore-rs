//! Shared fixture reader for KASUMI, KGCORE and the count-based A5/3 and A5/4 streams.
//! Checks the shape of the C observations; the expected results come only from the
//! recording.

pub const FINGERPRINT: u64 = 0x7ad530966a25597b;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
pub const STREAM_BITS: usize = 114;
pub const GUARD: u8 = 0x55;
pub const GUARD_BYTES: usize = 4;
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("key_expand", 174),
    ("block", 344),
    ("kgcore", 111),
    ("a5_count", 168),
];
/// Upstream records of each kind, in the same order.
pub const UPSTREAM: &[usize] = &[9, 53, 5, 12];

/// The eight subkey arrays in C's order: KL1, KL2, KO1, KO2, KO3, KI1, KI2 and KI3.
pub type Subkeys = [[u16; 8]; 8];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    KeyExpand {
        upstream: bool,
        key: [u8; 16],
        subkeys: Subkeys,
    },
    Block {
        upstream: bool,
        key: [u8; 16],
        input: u64,
        output: u64,
    },
    Kgcore {
        upstream: bool,
        ca: u8,
        cb: u8,
        cc: u32,
        cd: u8,
        key: [u8; 16],
        cl: u16,
        /// The bytes written, then the guard bytes after them.
        out: Vec<u8>,
    },
    A5Count {
        upstream: bool,
        n: u8,
        key: Vec<u8>,
        count: u32,
        downlink: Vec<u8>,
        uplink: Vec<u8>,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::KeyExpand { .. } => "key_expand",
            Self::Block { .. } => "block",
            Self::Kgcore { .. } => "kgcore",
            Self::A5Count { .. } => "a5_count",
        }
    }

    fn upstream(&self) -> bool {
        match self {
            Self::KeyExpand { upstream, .. }
            | Self::Block { upstream, .. }
            | Self::Kgcore { upstream, .. }
            | Self::A5Count { upstream, .. } => *upstream,
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

fn fixed<const N: usize>(value: &str) -> Result<[u8; N], String> {
    hex(value)?
        .try_into()
        .map_err(|_| format!("expected {N} bytes: {value}"))
}

fn words(value: &str) -> Result<[u16; 8], String> {
    let bytes: [u8; 16] = fixed(value)?;
    Ok(std::array::from_fn(|index| {
        u16::from_be_bytes([bytes[index * 2], bytes[index * 2 + 1]])
    }))
}

fn block(value: &str) -> Result<u64, String> {
    Ok(u64::from_be_bytes(fixed(value)?))
}

fn bits(value: &str) -> Result<Vec<u8>, String> {
    let value = string(value)?;
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

fn source(value: &str) -> Result<bool, String> {
    match string(value)? {
        "upstream" => Ok(true),
        "sweep" => Ok(false),
        other => Err(format!("bad source {other}")),
    }
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = fields.iter().map(|(_, value)| *value).collect();
    match (keys.as_slice(), values.as_slice()) {
        (
            [
                "kind",
                "source",
                "key",
                "kl1",
                "kl2",
                "ko1",
                "ko2",
                "ko3",
                "ki1",
                "ki2",
                "ki3",
            ],
            [r#""key_expand""#, upstream, key, arrays @ ..],
        ) => {
            let mut subkeys = [[0; 8]; 8];
            for (subkey, value) in subkeys.iter_mut().zip(arrays) {
                *subkey = words(value)?;
            }
            Ok(Record::KeyExpand {
                upstream: source(upstream)?,
                key: fixed(key)?,
                subkeys,
            })
        }
        (["kind", "source", "key", "in", "out"], [r#""block""#, upstream, key, input, output]) => {
            Ok(Record::Block {
                upstream: source(upstream)?,
                key: fixed(key)?,
                input: block(input)?,
                output: block(output)?,
            })
        }
        (
            ["kind", "source", "ca", "cb", "cc", "cd", "key", "cl", "out"],
            [r#""kgcore""#, upstream, ca, cb, cc, cd, key, cl, out],
        ) => Ok(Record::Kgcore {
            upstream: source(upstream)?,
            ca: parsed(ca)?,
            cb: parsed(cb)?,
            cc: parsed(cc)?,
            cd: parsed(cd)?,
            key: fixed(key)?,
            cl: parsed(cl)?,
            out: hex(out)?,
        }),
        (
            ["kind", "source", "n", "key", "count", "dl", "ul"],
            [r#""a5_count""#, upstream, n, key, count, downlink, uplink],
        ) => Ok(Record::A5Count {
            upstream: source(upstream)?,
            n: parsed(n)?,
            key: hex(key)?,
            count: parsed(count)?,
            downlink: bits(downlink)?,
            uplink: bits(uplink)?,
        }),
        _ => Err(format!("bad record: {line}")),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        // KL2, KI1, KI2 and KI3 each hold the eight modified key words, rotated.
        Record::KeyExpand { subkeys, .. } => {
            let modified = subkeys[1];
            let rotated = |offset: usize| -> [u16; 8] {
                std::array::from_fn(|index| modified[(index + offset) % 8])
            };
            if [subkeys[5], subkeys[6], subkeys[7]] == [rotated(2), rotated(1), rotated(5)] {
                Ok(())
            } else {
                Err("KI subkeys aren't rotations of KL2".to_owned())
            }
        }
        Record::Kgcore { cl, out, .. } => {
            let written = usize::from(*cl).div_ceil(8);
            if out.len() != written + GUARD_BYTES {
                Err(format!("KGCORE of {cl} bits recorded {} bytes", out.len()))
            } else if out[written..].iter().any(|&byte| byte != GUARD) {
                Err(format!("KGCORE of {cl} bits changed its guard bytes"))
            } else {
                Ok(())
            }
        }
        Record::A5Count { n, key, .. } => match (n, key.len()) {
            (3, 8) | (4, 16) => Ok(()),
            _ => Err(format!("A5/{n} with a {}-byte key", key.len())),
        },
        Record::Block { .. } => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"kasumi-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let records = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    let mut remaining = records.as_slice();
    for ((kind, count), upstream) in SECTIONS.iter().zip(UPSTREAM) {
        if remaining.len() < *count || remaining[..*count].iter().any(|r| r.kind() != *kind) {
            return Err(format!("expected {count} {kind} records here"));
        }
        let section = &remaining[..*count];
        // Each section starts with its upstream records.
        if section.iter().filter(|r| r.upstream()).count() != *upstream
            || section[..*upstream].iter().any(|r| !r.upstream())
        {
            return Err(format!("expected {upstream} upstream {kind} records first"));
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
        replace("kasumi-v1", "kasumi-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"key_expand""#, r#""kind":"block""#),
        replace(r#""kind":"a5_count""#, r#""kind":"other""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"upstream""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""kl1":"57ac"#, r#""kl1":"57a"#),
        replace(r#""ki1":"6bf0"#, r#""ki1":"6bf1"#),
        replace(r#""in":"ea024714ad5c4d84""#, r#""in":"ea024714ad5c4d8""#),
        replace(r#""in":"ea024714ad5c4d84""#, r#""in":"EA024714AD5C4D84""#),
        replace(r#""ca":15,"cb":0"#, r#""ca":015,"cb":0"#),
        replace(r#""ca":15,"cb":0"#, r#""ca":256,"cb":0"#),
        replace(r#""cl":256,"#, r#""cl":248,"#),
        replace(r#"55555555"}"#, r#"55555554"}"#),
        replace(
            r#""n":3,"key":"2bd6459f82c5bc00""#,
            r#""n":4,"key":"2bd6459f82c5bc00""#,
        ),
        replace(r#""dl":"1000"#, r#""dl":"2000"#),
        replace(r#""dl":"1000"#, r#""dl":"000"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
