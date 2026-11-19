//! Shared fixture reader for the 3GPP TS 23.236 Network Resource Indicators. Checks the
//! shape of the C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x7713fd87b9d6af0a;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("tmsi_get", 193),
    ("tmsi_set", 463),
    ("validate", 621),
    ("range_validate", 767),
    ("add", 1594),
    ("del", 1600),
    ("matches", 357),
    ("overlaps", 1604),
    ("limit", 3963),
    ("tmsi_limit", 1005),
    ("to_str", 88),
    ("to_str_c", 10),
    ("vty_add", 424),
    ("vty_del", 132),
];
pub const UPSTREAM_COUNT: usize = 327;
/// The byte every text buffer holds beforehand.
pub const GUARD: u8 = 0x55;
/// What an out-parameter holds beforehand, to show whether a call wrote it.
pub const SENTINEL: i16 = 0x5555;
/// The signal a limit call ended with when C divided by zero.
pub const SIGFPE: i32 = 8;

/// A list as C holds it: `(first, last)` entries in list order.
pub type Ranges = Vec<(i16, i16)>;

/// What a limit call did: ended by a signal, or returned with its result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<T> {
    Signal(i32),
    Returned { rc: i32, out: T },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// `osmo_tmsi_nri_v_get` with `nri_v` holding [`SENTINEL`] beforehand.
    TmsiGet {
        tmsi: u32,
        nri_bitlen: u8,
        rc: i32,
        nri_v: i16,
    },
    /// `osmo_tmsi_nri_v_set` on `tmsi`, which becomes `out`.
    TmsiSet {
        tmsi: u32,
        nri_v: i16,
        nri_bitlen: u8,
        rc: i32,
        out: u32,
    },
    /// `osmo_nri_v_validate`.
    Validate { nri_v: i16, nri_bitlen: u8, rc: i32 },
    /// `osmo_nri_range_validate`.
    RangeValidate {
        first: i16,
        last: i16,
        nri_bitlen: u8,
        rc: i32,
    },
    /// `osmo_nri_ranges_add` on `ranges`, which become `out`.
    Add {
        ranges: Ranges,
        first: i16,
        last: i16,
        rc: i32,
        out: Ranges,
    },
    /// `osmo_nri_ranges_del` on `ranges`, which become `out`.
    Del {
        ranges: Ranges,
        first: i16,
        last: i16,
        rc: i32,
        out: Ranges,
    },
    /// `osmo_nri_v_matches_ranges`.
    Matches {
        ranges: Ranges,
        nri_v: i16,
        matches: bool,
    },
    /// `osmo_nri_range_overlaps_ranges`.
    Overlaps {
        ranges: Ranges,
        first: i16,
        last: i16,
        overlaps: bool,
    },
    /// `osmo_nri_v_limit_by_ranges` on `nri_v`, in a child process.
    Limit {
        ranges: Ranges,
        nri_v: i16,
        nri_bitlen: u32,
        outcome: Outcome<i16>,
    },
    /// `osmo_tmsi_nri_v_limit_by_ranges` on `tmsi`, in a child process.
    TmsiLimit {
        ranges: Ranges,
        tmsi: u32,
        nri_bitlen: u8,
        outcome: Outcome<u32>,
    },
    /// `osmo_nri_ranges_to_str_buf` into `buflen` of a buffer of guard bytes. `out` holds
    /// the buffer up to `buflen` and one byte more.
    ToStr {
        ranges: Ranges,
        buflen: usize,
        rc: i32,
        out: Vec<u8>,
    },
    /// `osmo_nri_ranges_to_str_c`.
    ToStrC { ranges: Ranges, out: Vec<u8> },
    /// `osmo_nri_ranges_vty_add` with `argc` of the arguments, `parsed` holding [`SENTINEL`]
    /// beforehand and `ranges` becoming `out`.
    VtyAdd {
        ranges: Ranges,
        argv: Vec<Option<Vec<u8>>>,
        nri_bitlen: u8,
        rc: i32,
        message: Option<String>,
        parsed: (i16, i16),
        out: Ranges,
    },
    /// `osmo_nri_ranges_vty_del`, as for `VtyAdd`.
    VtyDel {
        ranges: Ranges,
        argv: Vec<Option<Vec<u8>>>,
        rc: i32,
        message: Option<String>,
        parsed: (i16, i16),
        out: Ranges,
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
            Body::TmsiGet { .. } => "tmsi_get",
            Body::TmsiSet { .. } => "tmsi_set",
            Body::Validate { .. } => "validate",
            Body::RangeValidate { .. } => "range_validate",
            Body::Add { .. } => "add",
            Body::Del { .. } => "del",
            Body::Matches { .. } => "matches",
            Body::Overlaps { .. } => "overlaps",
            Body::Limit { .. } => "limit",
            Body::TmsiLimit { .. } => "tmsi_limit",
            Body::ToStr { .. } => "to_str",
            Body::ToStrC { .. } => "to_str_c",
            Body::VtyAdd { .. } => "vty_add",
            Body::VtyDel { .. } => "vty_del",
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

/// A list written as `[first..last]` for each entry.
fn ranges(value: &str) -> Result<Ranges, String> {
    let text = quoted(value)?;
    let mut rest = text;
    let mut ranges = Vec::new();
    while !rest.is_empty() {
        let (entry, after) = rest
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']'))
            .ok_or_else(|| format!("bad ranges {text}"))?;
        let (first, last) = entry
            .split_once("..")
            .ok_or_else(|| format!("bad range {entry}"))?;
        ranges.push((parsed(first)?, parsed(last)?));
        rest = after;
    }
    Ok(ranges)
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

    fn optional<T>(
        &mut self,
        key: &str,
        read: impl FnOnce(&'a str) -> Result<T, String>,
    ) -> Result<Option<T>, String> {
        match self.raw(key)? {
            "null" => Ok(None),
            value => read(value).map(Some),
        }
    }

    fn boolean(&mut self, key: &str) -> Result<bool, String> {
        match self.raw(key)? {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("bad boolean {other}")),
        }
    }

    fn hex(&mut self, key: &str) -> Result<Vec<u8>, String> {
        hex(self.raw(key)?)
    }

    fn ranges(&mut self, key: &str) -> Result<Ranges, String> {
        ranges(self.raw(key)?)
    }

    fn outcome<T: std::str::FromStr + ToString>(&mut self) -> Result<Outcome<T>, String> {
        let signal: i32 = self.number("signal")?;
        let rc = self.optional("rc", parsed::<i32>)?;
        let out = self.optional("out", parsed::<T>)?;
        match (signal, rc, out) {
            (0, Some(rc), Some(out)) => Ok(Outcome::Returned { rc, out }),
            (signal, None, None) if signal != 0 => Ok(Outcome::Signal(signal)),
            _ => Err("a limit call either returns its results or ends by a signal".to_owned()),
        }
    }

    fn vty(&mut self) -> Result<Vec<Option<Vec<u8>>>, String> {
        let argc: usize = self.number("argc")?;
        let argv = [self.optional("argv0", hex)?, self.optional("argv1", hex)?];
        match argc {
            1 if argv[1].is_some() => Err("argc 1 passes no second argument".to_owned()),
            1 | 2 => Ok(argv[..argc].to_vec()),
            _ => Err(format!("bad argc {argc}")),
        }
    }

    fn message(&mut self) -> Result<Option<String>, String> {
        self.optional("message", |value| quoted(value).map(str::to_owned))
    }

    fn parsed_range(&mut self) -> Result<(i16, i16), String> {
        Ok((self.number("parsed_first")?, self.number("parsed_last")?))
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
    let mut c = Cursor { fields: &fields };
    let kind = quoted(c.raw("kind")?)?;
    let upstream = match quoted(c.raw("source")?)? {
        "upstream" => true,
        "sweep" => false,
        other => return Err(format!("bad source {other}")),
    };
    let body = match kind {
        "tmsi_get" => Body::TmsiGet {
            tmsi: c.number("tmsi")?,
            nri_bitlen: c.number("nri_bitlen")?,
            rc: c.number("rc")?,
            nri_v: c.number("nri_v")?,
        },
        "tmsi_set" => Body::TmsiSet {
            tmsi: c.number("tmsi")?,
            nri_v: c.number("nri_v")?,
            nri_bitlen: c.number("nri_bitlen")?,
            rc: c.number("rc")?,
            out: c.number("out")?,
        },
        "validate" => Body::Validate {
            nri_v: c.number("nri_v")?,
            nri_bitlen: c.number("nri_bitlen")?,
            rc: c.number("rc")?,
        },
        "range_validate" => Body::RangeValidate {
            first: c.number("first")?,
            last: c.number("last")?,
            nri_bitlen: c.number("nri_bitlen")?,
            rc: c.number("rc")?,
        },
        "add" => Body::Add {
            ranges: c.ranges("ranges")?,
            first: c.number("first")?,
            last: c.number("last")?,
            rc: c.number("rc")?,
            out: c.ranges("out")?,
        },
        "del" => Body::Del {
            ranges: c.ranges("ranges")?,
            first: c.number("first")?,
            last: c.number("last")?,
            rc: c.number("rc")?,
            out: c.ranges("out")?,
        },
        "matches" => Body::Matches {
            ranges: c.ranges("ranges")?,
            nri_v: c.number("nri_v")?,
            matches: c.boolean("matches")?,
        },
        "overlaps" => Body::Overlaps {
            ranges: c.ranges("ranges")?,
            first: c.number("first")?,
            last: c.number("last")?,
            overlaps: c.boolean("overlaps")?,
        },
        "limit" => Body::Limit {
            ranges: c.ranges("ranges")?,
            nri_v: c.number("nri_v")?,
            nri_bitlen: c.number("nri_bitlen")?,
            outcome: c.outcome()?,
        },
        "tmsi_limit" => Body::TmsiLimit {
            ranges: c.ranges("ranges")?,
            tmsi: c.number("tmsi")?,
            nri_bitlen: c.number("nri_bitlen")?,
            outcome: c.outcome()?,
        },
        "to_str" => Body::ToStr {
            ranges: c.ranges("ranges")?,
            buflen: c.number("buflen")?,
            rc: c.number("rc")?,
            out: c.hex("out")?,
        },
        "to_str_c" => Body::ToStrC {
            ranges: c.ranges("ranges")?,
            out: c.hex("out")?,
        },
        "vty_add" => Body::VtyAdd {
            ranges: c.ranges("ranges")?,
            argv: c.vty()?,
            nri_bitlen: c.number("nri_bitlen")?,
            rc: c.number("rc")?,
            message: c.message()?,
            parsed: c.parsed_range()?,
            out: c.ranges("out")?,
        },
        "vty_del" => Body::VtyDel {
            ranges: c.ranges("ranges")?,
            argv: c.vty()?,
            rc: c.number("rc")?,
            message: c.message()?,
            parsed: c.parsed_range()?,
            out: c.ranges("out")?,
        },
        other => return Err(format!("unknown kind {other}")),
    };
    c.end()?;
    Ok(Record { upstream, body })
}

/// The lists C keeps: sorted, with valid entries that neither overlap nor touch.
fn check_ranges(ranges: &Ranges) -> Result<(), String> {
    if ranges
        .iter()
        .any(|&(first, last)| first < 0 || first > last)
    {
        return Err(format!("invalid entry in {ranges:?}"));
    }
    if ranges
        .windows(2)
        .any(|pair| i32::from(pair[1].0) <= i32::from(pair[0].1) + 1)
    {
        return Err(format!("entries out of order or touching in {ranges:?}"));
    }
    Ok(())
}

fn one_of(what: &str, rc: i32, allowed: &[i32]) -> Result<(), String> {
    if allowed.contains(&rc) {
        Ok(())
    } else {
        Err(format!("bad {what} result {rc}"))
    }
}

fn in_ranges(ranges: &Ranges, nri_v: i16) -> bool {
    ranges
        .iter()
        .any(|&(first, last)| (first..=last).contains(&nri_v))
}

fn check_outcome<T: PartialEq + Copy>(
    what: &str,
    upstream: bool,
    outcome: Outcome<T>,
    input: T,
) -> Result<(), String> {
    match outcome {
        Outcome::Signal(SIGFPE) if !upstream => Ok(()),
        Outcome::Signal(signal) => Err(format!("{what} ended by signal {signal}")),
        Outcome::Returned { rc: -1, out } if out != input => {
            Err(format!("{what} failed but changed its value"))
        }
        Outcome::Returned { rc, .. } => one_of(what, rc, &[0, -1]),
    }
}

fn check_vty(
    what: &str,
    ranges: &Ranges,
    rc: i32,
    message: &Option<String>,
    parsed: (i16, i16),
    out: &Ranges,
    allowed: &[i32],
) -> Result<(), String> {
    check_ranges(ranges)?;
    check_ranges(out)?;
    one_of(what, rc, allowed)?;
    if (rc == 0) != message.is_none() {
        return Err(format!(
            "{what} gives a message exactly when it doesn't return 0"
        ));
    }
    if message.as_deref() == Some("Error: cannot parse NRI range") && parsed != (SENTINEL, SENTINEL)
    {
        return Err(format!("{what} wrote a range it could not parse"));
    }
    if rc == -1 && out != ranges {
        return Err(format!("{what} failed but changed the list"));
    }
    Ok(())
}

fn check(record: &Record) -> Result<(), String> {
    let upstream = record.upstream;
    match &record.body {
        Body::TmsiGet { rc, nri_v, .. } => match (*rc, *nri_v) {
            (-1, -1) => Ok(()),
            (0, nri_v) if nri_v >= 0 => Ok(()),
            _ => Err(format!("bad tmsi_get result {rc} with {nri_v}")),
        },
        Body::TmsiSet { tmsi, rc, out, .. } => {
            one_of("tmsi_set", *rc, &[0, -1])?;
            if *rc == -1 && out != tmsi {
                return Err("tmsi_set failed but changed the TMSI".to_owned());
            }
            Ok(())
        }
        Body::Validate { rc, .. } => one_of("validate", *rc, &[-1, 0, 1]),
        Body::RangeValidate { rc, .. } => one_of("range_validate", *rc, &[-3, -2, -1, 0, 1, 2]),
        Body::Add {
            ranges, rc, out, ..
        }
        | Body::Del {
            ranges, rc, out, ..
        } => {
            check_ranges(ranges)?;
            check_ranges(out)?;
            one_of("add or del", *rc, &[0, -1])?;
            if *rc == -1 && out != ranges {
                return Err("add or del failed but changed the list".to_owned());
            }
            Ok(())
        }
        Body::Matches { ranges, .. } | Body::Overlaps { ranges, .. } => check_ranges(ranges),
        Body::Limit {
            ranges,
            nri_v,
            outcome,
            ..
        } => {
            check_ranges(ranges)?;
            check_outcome("limit", upstream, *outcome, *nri_v)?;
            match outcome {
                Outcome::Returned { rc: 0, out } if !in_ranges(ranges, *out) => {
                    Err("limit gave a value outside its list".to_owned())
                }
                _ => Ok(()),
            }
        }
        Body::TmsiLimit {
            ranges,
            tmsi,
            outcome,
            ..
        } => {
            check_ranges(ranges)?;
            check_outcome("tmsi_limit", upstream, *outcome, *tmsi)
        }
        Body::ToStr {
            ranges,
            buflen,
            rc,
            out,
        } => {
            check_ranges(ranges)?;
            if out.len() != buflen + 1 || out[*buflen] != GUARD {
                return Err("to_str must hold its buffer and one untouched byte".to_owned());
            }
            if *buflen > 0 && !out[..*buflen].contains(&0) {
                return Err("to_str must write a NUL".to_owned());
            }
            if *rc <= 0 {
                return Err(format!("bad to_str result {rc}"));
            }
            Ok(())
        }
        Body::ToStrC { ranges, out } => {
            check_ranges(ranges)?;
            if out.is_empty() || !out.is_ascii() || out.contains(&0) {
                return Err("to_str_c must give text".to_owned());
            }
            Ok(())
        }
        Body::VtyAdd {
            ranges,
            rc,
            message,
            parsed,
            out,
            ..
        } => check_vty("vty_add", ranges, *rc, message, *parsed, out, &[-1, 0, 1]),
        Body::VtyDel {
            ranges,
            rc,
            message,
            parsed,
            out,
            ..
        } => check_vty("vty_del", ranges, *rc, message, *parsed, out, &[-1, 0]),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm23236-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        replace("gsm23236-v1", "gsm23236-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"tmsi_get""#, r#""kind":"tmsi_set""#),
        replace(r#""kind":"vty_del""#, r#""kind":"other""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(
            r#""tmsi":0,"nri_bitlen":10"#,
            r#""tmsi":00,"nri_bitlen":10"#,
        ),
        replace(
            r#""nri_bitlen":16,"rc":-1,"nri_v":-1"#,
            r#""nri_bitlen":16,"rc":-1,"nri_v":0"#,
        ),
        replace(
            r#""nri_bitlen":16,"rc":-1,"out":19088743"#,
            r#""nri_bitlen":16,"rc":-1,"out":19088744"#,
        ),
        replace(
            r#""nri_v":-32768,"nri_bitlen":10,"rc":-1"#,
            r#""nri_v":-32768,"nri_bitlen":10,"rc":-2"#,
        ),
        replace(
            r#""first":100,"last":200,"rc":0,"out":"[100..200]""#,
            r#""first":100,"last":200,"rc":0,"out":"[100..200"#,
        ),
        replace(
            r#""ranges":"","first":-1,"last":-1,"rc":-1,"out":"""#,
            r#""ranges":"","first":-1,"last":-1,"rc":-1,"out":"[0..0]""#,
        ),
        replace(r#""out":"[111..200]""#, r#""out":"[200..111]""#),
        replace(r#""out":"[111..200]""#, r#""out":"[111..150][151..200]""#),
        replace(r#""matches":false"#, r#""matches":0"#),
        replace(
            r#""signal":0,"rc":0,"out":10}"#,
            r#""signal":0,"rc":0,"out":11}"#,
        ),
        replace(
            r#""signal":0,"rc":0,"out":10}"#,
            r#""signal":0,"rc":null,"out":10}"#,
        ),
        replace(r#""signal":8,"rc":null"#, r#""signal":11,"rc":null"#),
        replace(
            r#""signal":0,"rc":-1,"out":-32768}"#,
            r#""signal":0,"rc":-1,"out":0}"#,
        ),
        replace(
            r#""buflen":0,"rc":5,"out":"55""#,
            r#""buflen":0,"rc":5,"out":"""#,
        ),
        replace(
            r#""buflen":1,"rc":5,"out":"0055""#,
            r#""buflen":1,"rc":5,"out":"6555""#,
        ),
        replace(r#""out":"656d707479""#, r#""out":"""#),
        replace(
            r#""argc":1,"argv0":"30","argv1":null"#,
            r#""argc":1,"argv0":"30","argv1":"30""#,
        ),
        replace(
            r#""argc":1,"argv0":"30","argv1":null"#,
            r#""argc":3,"argv0":"30","argv1":null"#,
        ),
        replace(r#""rc":0,"message":null"#, r#""rc":0,"message":"oops""#),
        replace(
            r#""message":"Error: cannot parse NRI range","parsed_first":21845"#,
            r#""message":"Error: cannot parse NRI range","parsed_first":0"#,
        ),
        replace(
            r#""rc":-1,"message":"Error: failed to remove NRI range","parsed_first":3,"parsed_last":0,"out":"[0..10]""#,
            r#""rc":-1,"message":"Error: failed to remove NRI range","parsed_first":3,"parsed_last":0,"out":"[0..9]""#,
        ),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
