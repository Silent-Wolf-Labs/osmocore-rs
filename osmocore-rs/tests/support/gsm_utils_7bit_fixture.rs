//! Shared fixture reader for the gsm_utils 7-bit functions. Checks the shape of the
//! C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0xdc6d124a755e36ca;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("octet_len", 256),
    ("septet", 258),
    ("pack", 210),
    ("encode", 281),
    ("encode_ussd", 548),
    ("decode", 607),
    ("decode_ussd", 185),
    ("decode_hdr", 624),
];
/// Records made from upstream's test data, in the pack, encode and decode sections.
pub const UPSTREAM_COUNT: usize = 469;
/// Records where a header longer than the message makes C's septet count wrap.
pub const WRAP_COUNT: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Upstream,
    Sweep,
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeMode {
    Plain,
    Ussd,
    /// The value of the C function's user data header indicator.
    Header(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    OctetLen {
        septets: u8,
        octets: u8,
    },
    Septet {
        text: Vec<u8>,
        septets: Vec<u8>,
    },
    Pack {
        source: Source,
        septets: Vec<u8>,
        padding: u8,
        octets: Vec<u8>,
    },
    Encode {
        ussd: bool,
        source: Source,
        text: Vec<u8>,
        n: usize,
        septets: i32,
        octets: Vec<u8>,
    },
    Decode {
        mode: DecodeMode,
        source: Source,
        input: Vec<u8>,
        length: u8,
        n: usize,
        text: Vec<u8>,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::OctetLen { .. } => "octet_len",
            Self::Septet { .. } => "septet",
            Self::Pack { .. } => "pack",
            Self::Encode { ussd: false, .. } => "encode",
            Self::Encode { ussd: true, .. } => "encode_ussd",
            Self::Decode {
                mode: DecodeMode::Plain,
                ..
            } => "decode",
            Self::Decode {
                mode: DecodeMode::Ussd,
                ..
            } => "decode_ussd",
            Self::Decode {
                mode: DecodeMode::Header(_),
                ..
            } => "decode_hdr",
        }
    }

    fn source(&self) -> Option<Source> {
        match self {
            Self::Pack { source, .. }
            | Self::Encode { source, .. }
            | Self::Decode { source, .. } => Some(*source),
            _ => None,
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

fn hex(value: &str) -> Result<Vec<u8>, String> {
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(format!("bad hex {value}")),
    };
    if !value.len().is_multiple_of(2) {
        return Err(format!("odd hex {value}"));
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|pair| Ok(digit(pair[0])? * 16 + digit(pair[1])?))
        .collect()
}

fn source(value: &str) -> Result<Source, String> {
    match value {
        "upstream" => Ok(Source::Upstream),
        "sweep" => Ok(Source::Sweep),
        "wrap" => Ok(Source::Wrap),
        _ => Err(format!("bad source {value}")),
    }
}

fn counted(count: &str, bytes: &[u8]) -> Result<(), String> {
    if parsed::<usize>(count)? == bytes.len() {
        Ok(())
    } else {
        Err(format!("count {count} but {} bytes", bytes.len()))
    }
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = fields.iter().map(|(_, value)| *value).collect();
    let kind = *values.first().ok_or("empty record")?;
    if keys.first() != Some(&"kind") {
        return Err(format!("no kind: {line}"));
    }
    match (kind, &keys[1..], &values[1..]) {
        ("octet_len", ["septets", "octets"], [septets, octets]) => Ok(Record::OctetLen {
            septets: parsed(septets)?,
            octets: parsed(octets)?,
        }),
        ("septet", ["text", "count", "septets"], [text, count, septets]) => {
            let septets = hex(septets)?;
            counted(count, &septets)?;
            Ok(Record::Septet {
                text: hex(text)?,
                septets,
            })
        }
        (
            "pack",
            ["source", "septets", "padding", "count", "octets"],
            [source_name, septets, padding, count, octets],
        ) => {
            let octets = hex(octets)?;
            counted(count, &octets)?;
            Ok(Record::Pack {
                source: source(source_name)?,
                septets: hex(septets)?,
                padding: parsed(padding)?,
                octets,
            })
        }
        (
            "encode" | "encode_ussd",
            ["source", "text", "n", "count", "written", "octets"],
            [source_name, text, n, count, written, octets],
        ) => {
            let octets = hex(octets)?;
            counted(written, &octets)?;
            Ok(Record::Encode {
                ussd: kind == "encode_ussd",
                source: source(source_name)?,
                text: hex(text)?,
                n: parsed(n)?,
                septets: parsed(count)?,
                octets,
            })
        }
        (
            "decode" | "decode_ussd",
            ["source", "input", "length", "n", "count", "text"],
            [source_name, input, length, n, count, text],
        ) => decoded(
            if kind == "decode" {
                DecodeMode::Plain
            } else {
                DecodeMode::Ussd
            },
            [source_name, input, length, n, count, text],
        ),
        (
            "decode_hdr",
            ["source", "input", "length", "hdr", "n", "count", "text"],
            [source_name, input, length, hdr, n, count, text],
        ) => decoded(
            DecodeMode::Header(parsed(hdr)?),
            [source_name, input, length, n, count, text],
        ),
        _ => Err(format!("bad record: {line}")),
    }
}

fn decoded(mode: DecodeMode, values: [&str; 6]) -> Result<Record, String> {
    let [source_name, input, length, n, count, text] = values;
    let text = hex(text)?;
    counted(count, &text)?;
    Ok(Record::Decode {
        mode,
        source: source(source_name)?,
        input: hex(input)?,
        length: parsed(length)?,
        n: parsed(n)?,
        text,
    })
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Septet { text, .. } if text.contains(&0) || text.contains(&0xff) => {
            Err("text holds a byte C can't take".to_owned())
        }
        Record::Pack { padding, .. } if *padding > 7 => Err("padding above 7".to_owned()),
        Record::Encode { n, octets, .. } if octets.len() > *n => {
            Err("encoded past the output size".to_owned())
        }
        Record::Encode { text, .. } if text.contains(&0) || text.contains(&0xff) => {
            Err("text holds a byte C can't take".to_owned())
        }
        Record::Decode { n, text, .. } if text.len() >= *n => {
            Err("decoded past the output size".to_owned())
        }
        Record::Decode { text, .. } if text.contains(&0) => {
            Err("decoded text holds a NUL".to_owned())
        }
        Record::Decode {
            source: Source::Wrap,
            mode: DecodeMode::Plain | DecodeMode::Ussd,
            ..
        } => Err("wrap outside the header section".to_owned()),
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm-utils-7bit-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
    let sourced = |wanted| {
        records
            .iter()
            .filter(|record| record.source() == Some(wanted))
            .count()
    };
    if sourced(Source::Upstream) != UPSTREAM_COUNT {
        return Err(format!("expected {UPSTREAM_COUNT} upstream records"));
    }
    if sourced(Source::Wrap) != WRAP_COUNT {
        return Err(format!("expected {WRAP_COUNT} wrap records"));
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
        replace("gsm-utils-7bit-v1", "gsm-utils-7bit-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"octet_len""#, r#""kind":"other""#),
        replace(r#""kind":"decode""#, r#""kind":"decode_ussd""#),
        replace(r#""septets":1,"octets":1"#, r#""septets":01,"octets":1"#),
        replace(r#""septets":1,"octets":1"#, r#""septtes":1,"octets":1"#),
        replace(r#""text":"41","count":1"#, r#""text":"4","count":1"#),
        replace(r#""text":"41","count":1"#, r#""text":"4g","count":1"#),
        replace(r#""text":"41","count":1"#, r#""text":"41","count":2"#),
        replace(r#""text":"41","count":1"#, r#""text":"ff","count":1"#),
        replace(r#""padding":1"#, r#""padding":8"#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""source":"wrap""#, r#""source":"sweep""#),
        replace(r#""n":16,"count":1"#, r#""n":1,"count":1"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
