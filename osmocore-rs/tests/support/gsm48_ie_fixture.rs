//! Shared fixture reader for the 3GPP TS 24.008 BCD numbers. Checks the shape of the C
//! observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x7590ea472fc8351e;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[("encode", 1094), ("decode", 818), ("decode_legacy", 802)];
pub const UPSTREAM_COUNT: usize = 20;
/// The byte every buffer holds beforehand.
pub const GUARD: u8 = 0x55;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// `gsm48_encode_bcd_number` on `input` into a buffer of guard bytes. `out` holds every
    /// byte it may write, the length byte, the header and the digits, then one more.
    Encode {
        input: Vec<u8>,
        h_len: usize,
        max_len: u8,
        rc: i32,
        out: Vec<u8>,
    },
    /// `gsm48_decode_bcd_number2` on the first `input_len` bytes of `input`. `out` holds the
    /// output buffer up to `output_len`, but no further than two digits for each input byte
    /// and a NUL, then one byte more.
    Decode {
        input: Vec<u8>,
        input_len: usize,
        h_len: usize,
        output_len: usize,
        rc: i32,
        out: Vec<u8>,
    },
    /// `gsm48_decode_bcd_number` on `input`, whose length byte stays within it. `out` as for
    /// `Decode`, with all of `input` as the input length.
    DecodeLegacy {
        input: Vec<u8>,
        h_len: usize,
        output_len: usize,
        rc: i32,
        out: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub upstream: bool,
    pub body: Body,
}

impl Record {
    fn kind(&self) -> &'static str {
        match self.body {
            Body::Encode { .. } => "encode",
            Body::Decode { .. } => "decode",
            Body::DecodeLegacy { .. } => "decode_legacy",
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

fn quoted(value: &str) -> Result<&str, String> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| format!("not a string {value}"))
}

fn hex(value: &str) -> Result<Vec<u8>, String> {
    let value = quoted(value)?;
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

/// Takes fields in order, each of which must have the expected key.
struct Cursor<'a, 'b> {
    fields: &'b [(&'a str, &'a str)],
}

impl<'a> Cursor<'a, '_> {
    fn raw(&mut self, key: &str) -> Result<&'a str, String> {
        match self.fields.split_first() {
            Some(((actual, value), rest)) if *actual == key => {
                self.fields = rest;
                Ok(value)
            }
            Some(((actual, _), _)) => Err(format!("expected key {key}, got {actual}")),
            None => Err(format!("missing key {key}")),
        }
    }

    fn number<T: std::str::FromStr + ToString>(&mut self, key: &str) -> Result<T, String> {
        parsed(self.raw(key)?)
    }

    fn hex(&mut self, key: &str) -> Result<Vec<u8>, String> {
        hex(self.raw(key)?)
    }

    fn end(&self) -> Result<(), String> {
        match self.fields.first() {
            None => Ok(()),
            Some((key, _)) => Err(format!("unexpected key {key}")),
        }
    }
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let mut cursor = Cursor { fields: &fields };
    let kind = quoted(cursor.raw("kind")?)?;
    let upstream = match quoted(cursor.raw("source")?)? {
        "upstream" => true,
        "sweep" => false,
        other => return Err(format!("bad source {other}")),
    };
    let body = match kind {
        "encode" => Body::Encode {
            input: cursor.hex("input")?,
            h_len: cursor.number("h_len")?,
            max_len: cursor.number("max_len")?,
            rc: cursor.number("rc")?,
            out: cursor.hex("out")?,
        },
        "decode" => Body::Decode {
            input: cursor.hex("input")?,
            input_len: cursor.number("input_len")?,
            h_len: cursor.number("h_len")?,
            output_len: cursor.number("output_len")?,
            rc: cursor.number("rc")?,
            out: cursor.hex("out")?,
        },
        "decode_legacy" => Body::DecodeLegacy {
            input: cursor.hex("input")?,
            h_len: cursor.number("h_len")?,
            output_len: cursor.number("output_len")?,
            rc: cursor.number("rc")?,
            out: cursor.hex("out")?,
        },
        other => return Err(format!("unknown kind {other}")),
    };
    cursor.end()?;
    Ok(Record { upstream, body })
}

fn guard_only(what: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.iter().all(|&byte| byte == GUARD) {
        Ok(())
    } else {
        Err(format!("{what} wrote past its output"))
    }
}

/// The bytes a decode may write: up to `output_len`, but no further than two digits for
/// each input byte and a NUL.
fn decode_extent(input_len: usize, output_len: usize) -> usize {
    output_len.min(2 * input_len + 1)
}

/// A decoder's output: nothing at all for no room, and otherwise a NUL among the bytes it
/// may write, and the byte at `output_len` untouched where the record reaches it.
fn check_decoded(
    what: &str,
    input_len: usize,
    output_len: usize,
    rc: i32,
    out: &[u8],
) -> Result<(), String> {
    let extent = decode_extent(input_len, output_len);
    if out.len() != extent + 1 {
        return Err(format!(
            "{what} holds {} bytes, not {}",
            out.len(),
            extent + 1
        ));
    }
    if ![0, -5, -22, -28].contains(&rc) {
        return Err(format!("bad {what} result {rc}"));
    }
    if output_len == 0 {
        return match rc {
            -28 => guard_only(what, out),
            _ => Err(format!("{what} needs room for a NUL")),
        };
    }
    if !out[..extent].contains(&0) {
        return Err(format!("{what} must write a NUL"));
    }
    if extent == output_len {
        guard_only(what, &out[extent..])?;
    }
    match rc {
        -5 | -22 if out[0] != 0 || out[1..extent].iter().any(|&byte| byte != GUARD) => {
            Err(format!("{what} failed but wrote more than a NUL"))
        }
        -5 if input_len != 0 => Err(format!("{what} had input but found none")),
        _ => Ok(()),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match &record.body {
        Body::Encode {
            input,
            h_len,
            rc,
            out,
            ..
        } => {
            let extent = 1 + h_len + input.len().div_ceil(2);
            if out.len() != extent + 1 {
                return Err(format!(
                    "encode holds {} bytes, not {}",
                    out.len(),
                    extent + 1
                ));
            }
            if usize::from(out[0]) != (extent - 1) % 256 {
                return Err("encode must write the length byte first".to_owned());
            }
            guard_only("encode's header", &out[1..1 + h_len])?;
            match *rc {
                -5 => guard_only("encode", &out[1..]),
                -22 => guard_only("encode", &out[extent..]),
                rc if rc == extent as i32 => guard_only("encode", &out[extent..]),
                rc => Err(format!("bad encode result {rc}")),
            }
        }
        Body::Decode {
            input,
            input_len,
            output_len,
            rc,
            out,
            ..
        } => {
            if input_len > &input.len() {
                return Err("decode's input length passes its input".to_owned());
            }
            check_decoded("decode", *input_len, *output_len, *rc, out)
        }
        Body::DecodeLegacy {
            input,
            output_len,
            rc,
            out,
            ..
        } => {
            if input
                .first()
                .is_none_or(|&len| usize::from(len) >= input.len())
            {
                return Err("decode_legacy's length byte passes its input".to_owned());
            }
            check_decoded("decode_legacy", input.len(), *output_len, *rc, out)
        }
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm48_ie-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
    let upstream = records.iter().filter(|record| record.upstream).count();
    if upstream != UPSTREAM_COUNT {
        return Err(format!("expected {UPSTREAM_COUNT} upstream records"));
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
        replace("gsm48_ie-v1", "gsm48_ie-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"encode""#, r#""kind":"decode""#),
        replace(r#""kind":"decode_legacy""#, r#""kind":"other""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""h_len":0,"max_len""#, r#""h_len":00,"max_len""#),
        replace(
            r#""max_len":255,"rc":6,"out":"0521436587f955""#,
            r#""max_len":255,"rc":6,"out":"0521436587f9""#,
        ),
        replace(
            r#""max_len":255,"rc":6,"out":"0521436587f955""#,
            r#""max_len":255,"rc":6,"out":"0621436587f955""#,
        ),
        replace(
            r#""max_len":255,"rc":6,"out":"0521436587f955""#,
            r#""max_len":255,"rc":5,"out":"0521436587f955""#,
        ),
        replace(
            r#""max_len":255,"rc":8,"out":"075555555521436555""#,
            r#""max_len":255,"rc":8,"out":"07ffffffff21436555""#,
        ),
        replace(
            r#""max_len":9,"rc":-5,"out":"0a5555555555555555555555""#,
            r#""max_len":9,"rc":-5,"out":"0a2155555555555555555555""#,
        ),
        replace(
            r#""input_len":0,"h_len":0,"output_len":255,"rc":-5,"out":"0055""#,
            r#""input_len":1,"h_len":0,"output_len":255,"rc":-5,"out":"0055""#,
        ),
        replace(
            r#""output_len":4,"rc":-28,"out":"3132330055""#,
            r#""output_len":4,"rc":-28,"out":"3132333455""#,
        ),
        replace(
            r#""output_len":4,"rc":-28,"out":"3132330055""#,
            r#""output_len":4,"rc":-28,"out":"31323300""#,
        ),
        replace(
            r#""output_len":4,"rc":-28,"out":"3132330055""#,
            r#""output_len":4,"rc":-29,"out":"3132330055""#,
        ),
        replace(
            r#""output_len":255,"rc":-22,"out":"00555555555555555555""#,
            r#""output_len":255,"rc":-22,"out":"31555555555555555555""#,
        ),
        replace(
            r#""output_len":0,"rc":-28,"out":"55""#,
            r#""output_len":0,"rc":0,"out":"55""#,
        ),
        replace(
            r#""input":"0521436587f9","input_len":6"#,
            r#""input":"0521436587f9","input_len":7"#,
        ),
        replace(
            r#"{"kind":"decode_legacy","source":"upstream","input":"0521436587f9""#,
            r#"{"kind":"decode_legacy","source":"upstream","input":"0621436587f9""#,
        ),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
