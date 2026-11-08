//! Shared fixture reader for COMP128 and XOR-2G. Checks the shape of the C
//! observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x41dc148a0d9601ee;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[("comp128", 3083), ("xor_2g", 297)];
/// Upstream's comp128_test.c checks 1024 vectors each for v2 and v3.
pub const UPSTREAM_VECTORS: usize = 2048;
/// The Ki of every upstream vector.
pub const UPSTREAM_KI: [u8; 16] = [0xaa; 16];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Comp128 {
        upstream: bool,
        version: u8,
        ki: [u8; 16],
        rand: [u8; 16],
        sres: [u8; 4],
        kc: [u8; 8],
    },
    Xor2g {
        ki: [u8; 16],
        rand: [u8; 16],
        sres: [u8; 4],
        kc: [u8; 8],
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::Comp128 { .. } => "comp128",
            Self::Xor2g { .. } => "xor_2g",
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

fn string(value: &str) -> Result<&str, String> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| format!("not a string: {value}"))
}

fn bytes<const N: usize>(value: &str) -> Result<[u8; N], String> {
    let value = string(value)?;
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!("expected {N} bytes of lowercase hex: {value}"));
    }
    let mut out = [0; N];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("bad hex {value}"))?;
    }
    Ok(out)
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = fields.iter().map(|(_, value)| *value).collect();
    match (keys.as_slice(), values.as_slice()) {
        (
            ["kind", "source", "version", "ki", "rand", "sres", "kc"],
            [r#""comp128""#, source, version, ki, rand, sres, kc],
        ) => Ok(Record::Comp128 {
            upstream: match string(source)? {
                "upstream" => true,
                "sweep" => false,
                other => return Err(format!("bad source {other}")),
            },
            version: match *version {
                "1" => 1,
                "2" => 2,
                "3" => 3,
                other => return Err(format!("bad version {other}")),
            },
            ki: bytes(ki)?,
            rand: bytes(rand)?,
            sres: bytes(sres)?,
            kc: bytes(kc)?,
        }),
        (["kind", "ki", "rand", "sres", "kc"], [r#""xor_2g""#, ki, rand, sres, kc]) => {
            Ok(Record::Xor2g {
                ki: bytes(ki)?,
                rand: bytes(rand)?,
                sres: bytes(sres)?,
                kc: bytes(kc)?,
            })
        }
        _ => Err(format!("bad record: {line}")),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Comp128 {
            upstream: true,
            version,
            ki,
            ..
        } if *version == 1 || *ki != UPSTREAM_KI => {
            Err(format!("unexpected upstream COMP128v{version} vector"))
        }
        // v1 and v2 give a 54-bit Kc: its last 10 bits are zero.
        Record::Comp128 {
            version: 1 | 2, kc, ..
        } if kc[7] != 0 || kc[6] & 0x03 != 0 => Err(format!("Kc {kc:02x?} has more than 54 bits")),
        // XOR-2G's SRES and Kc are the first 12 bytes of Ki XOR RAND.
        Record::Xor2g { ki, rand, sres, kc }
            if ki
                .iter()
                .zip(rand)
                .map(|(k, r)| k ^ r)
                .take(12)
                .ne(sres.iter().chain(kc).copied()) =>
        {
            Err(format!("XOR-2G result for {ki:02x?} isn't Ki XOR RAND"))
        }
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"comp128-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        .filter(|record| matches!(record, Record::Comp128 { upstream: true, .. }))
        .count();
    if upstream != UPSTREAM_VECTORS {
        return Err(format!("expected {UPSTREAM_VECTORS} upstream records"));
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
    let zero = "00000000000000000000000000000000";
    for corrupt in [
        replace("comp128-v1", "comp128-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"comp128""#, r#""kind":"other""#),
        replace(r#""kind":"xor_2g""#, r#""kind":"comp128""#),
        replace(
            r#""source":"upstream","version":2"#,
            r#""source":"sweep","version":2"#,
        ),
        replace(
            r#""source":"upstream","version":2"#,
            r#""source":"upstream","version":1"#,
        ),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""version":1"#, r#""version":4"#),
        replace(r#""version":1"#, r#""version":01"#),
        replace(r#""version":1"#, r#""version":"1""#),
        replace(r#""ki":"aaaa"#, r#""ki":"AAaa"#),
        replace(&format!(r#""ki":"{zero}""#), r#""ki":"0""#),
        replace(r#""kc":"174757783dc40400""#, r#""kc":"174757783dc40401""#),
        replace(r#""kc":"174757783dc40400""#, r#""kc":"174757783dc40700""#),
        replace(
            r#""kind":"xor_2g","ki":"00000000000000000000000000000000","rand":"ffffffffffffffffffffffffffffffff","sres":"ffffffff""#,
            r#""kind":"xor_2g","ki":"00000000000000000000000000000000","rand":"ffffffffffffffffffffffffffffffff","sres":"fffffffe""#,
        ),
        replace(r#""sres":"ffffffff""#, r#""srex":"ffffffff""#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
