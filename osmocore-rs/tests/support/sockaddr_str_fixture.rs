//! Shared fixture reader for sockaddr_str. Checks the shape of the C observations; the
//! expected results come only from the recording.

pub const FINGERPRINT: u64 = 0xb57b7d88d02897d3;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("constant", 8),
    ("type", 80),
    ("case", 655),
    ("from", 701),
    ("cmp", 1521),
];
pub const UPSTREAM_COUNT: usize = 624;
/// The bytes of a `sockaddr_storage` recorded. The harness checks the rest are zero.
pub const STORAGE_RECORDED: usize = 28;
const MAX_IP_LEN: usize = 45;

/// A `struct osmo_sockaddr_str`, with the text up to its NUL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sockaddr {
    pub af: i32,
    pub ip: Vec<u8>,
    pub port: u16,
}

/// Every conversion from one stored address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub input: Sockaddr,
    pub fmt: Vec<u8>,
    pub set: bool,
    pub nonzero: bool,
    pub in_rc: i32,
    pub in_addr: Vec<u8>,
    pub in6_rc: i32,
    pub in6_addr: Vec<u8>,
    pub u32_rc: i32,
    /// The value's bytes in memory, in network order.
    pub u32_bytes: Vec<u8>,
    pub u32h_rc: i32,
    pub u32h: u32,
    pub sin_rc: i32,
    pub sin: Vec<u8>,
    pub sin6_rc: i32,
    pub sin6: Vec<u8>,
    pub ss_rc: i32,
    /// The first [`STORAGE_RECORDED`] bytes.
    pub ss: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FromOp {
    InAddr,
    In6Addr,
    /// The input is the value's bytes in memory, in network order.
    U32,
    /// The input is the value, most significant byte first.
    U32h,
    SockaddrIn,
    SockaddrIn6,
    /// The input is the first [`STORAGE_RECORDED`] bytes.
    Sockaddr,
    Str,
    Str2,
}

impl FromOp {
    fn parse(op: &str) -> Result<Self, String> {
        Ok(match op {
            "in_addr" => Self::InAddr,
            "in6_addr" => Self::In6Addr,
            "32" => Self::U32,
            "32h" => Self::U32h,
            "sockaddr_in" => Self::SockaddrIn,
            "sockaddr_in6" => Self::SockaddrIn6,
            "sockaddr" => Self::Sockaddr,
            "str" => Self::Str,
            "str2" => Self::Str2,
            _ => return Err(format!("bad op {op}")),
        })
    }

    /// The input's length in bytes, or `None` for text.
    fn input_len(self) -> Option<usize> {
        match self {
            Self::InAddr | Self::U32 | Self::U32h => Some(4),
            Self::In6Addr => Some(16),
            Self::SockaddrIn => Some(16),
            Self::SockaddrIn6 => Some(28),
            Self::Sockaddr => Some(STORAGE_RECORDED),
            Self::Str | Self::Str2 => None,
        }
    }

    fn takes_port(self) -> bool {
        !matches!(
            self,
            Self::SockaddrIn | Self::SockaddrIn6 | Self::Sockaddr | Self::Str2
        )
    }
}

/// A conversion into an address, over a destination holding junk beforehand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversion {
    pub op: FromOp,
    /// `None` for a NULL text.
    pub input: Option<Vec<u8>>,
    pub port: Option<u16>,
    pub rc: i32,
    pub out: Sockaddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Constant { name: String, value: i64 },
    Type { ip: Option<Vec<u8>>, af: i32 },
    Case(Case),
    From(Conversion),
    Cmp { a: Sockaddr, b: Sockaddr, sign: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub upstream: bool,
    pub body: Body,
}

impl Record {
    fn kind(&self) -> &'static str {
        match self.body {
            Body::Constant { .. } => "constant",
            Body::Type { .. } => "type",
            Body::Case(_) => "case",
            Body::From(_) => "from",
            Body::Cmp { .. } => "cmp",
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

fn hex_len(value: &str, len: usize) -> Result<Vec<u8>, String> {
    let bytes = hex(value)?;
    if bytes.len() == len {
        Ok(bytes)
    } else {
        Err(format!("expected {len} bytes, got {}", bytes.len()))
    }
}

fn hex_or_null(value: &str) -> Result<Option<Vec<u8>>, String> {
    match value {
        "null" => Ok(None),
        _ => hex(value).map(Some),
    }
}

fn flag(value: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("bad flag {value}")),
    }
}

/// The fields after `kind` and `source`, which must have exactly these keys.
struct Fields<'a> {
    values: Vec<&'a str>,
}

impl<'a> Fields<'a> {
    fn take(fields: &[(&'a str, &'a str)], keys: &[&str]) -> Result<Self, String> {
        let actual: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
        if actual != keys {
            return Err(format!("expected keys {keys:?}, got {actual:?}"));
        }
        Ok(Self {
            values: fields.iter().map(|(_, value)| *value).collect(),
        })
    }

    fn sockaddr(&self, at: usize) -> Result<Sockaddr, String> {
        Ok(Sockaddr {
            af: parsed(self.values[at])?,
            ip: hex(self.values[at + 1])?,
            port: parsed(self.values[at + 2])?,
        })
    }
}

fn sockaddr_keys(prefix: &str) -> [String; 3] {
    ["af", "ip", "port"].map(|key| format!("{prefix}{key}"))
}

fn case(fields: &[(&str, &str)]) -> Result<Case, String> {
    let f = Fields::take(
        fields,
        &[
            "af", "ip", "port", "fmt", "set", "nonzero", "in_rc", "in", "in6_rc", "in6", "u32_rc",
            "u32", "u32h_rc", "u32h", "sin_rc", "sin", "sin6_rc", "sin6", "ss_rc", "ss",
        ],
    )?;
    let v = &f.values;
    Ok(Case {
        input: f.sockaddr(0)?,
        fmt: hex(v[3])?,
        set: flag(v[4])?,
        nonzero: flag(v[5])?,
        in_rc: parsed(v[6])?,
        in_addr: hex_len(v[7], 4)?,
        in6_rc: parsed(v[8])?,
        in6_addr: hex_len(v[9], 16)?,
        u32_rc: parsed(v[10])?,
        u32_bytes: hex_len(v[11], 4)?,
        u32h_rc: parsed(v[12])?,
        u32h: parsed(v[13])?,
        sin_rc: parsed(v[14])?,
        sin: hex_len(v[15], 16)?,
        sin6_rc: parsed(v[16])?,
        sin6: hex_len(v[17], 28)?,
        ss_rc: parsed(v[18])?,
        ss: hex_len(v[19], STORAGE_RECORDED)?,
    })
}

fn conversion(fields: &[(&str, &str)]) -> Result<Conversion, String> {
    let out = sockaddr_keys("out_");
    let keys = ["op", "input", "port", "rc", &out[0], &out[1], &out[2]];
    let keys: Vec<&str> = keys.iter().map(|key| key.as_ref()).collect();
    let f = Fields::take(fields, &keys)?;
    let op = FromOp::parse(quoted(f.values[0])?)?;
    let input = match op.input_len() {
        Some(len) => Some(hex_len(f.values[1], len)?),
        None => hex_or_null(f.values[1])?,
    };
    let port = match (op.takes_port(), f.values[2]) {
        (false, "null") => None,
        (true, port) => Some(parsed(port)?),
        (false, port) => return Err(format!("{op:?} takes no port, got {port}")),
    };
    Ok(Conversion {
        op,
        input,
        port,
        rc: parsed(f.values[3])?,
        out: f.sockaddr(4)?,
    })
}

fn body(kind: &str, fields: &[(&str, &str)]) -> Result<Body, String> {
    match kind {
        "type" => {
            let f = Fields::take(fields, &["ip", "af"])?;
            Ok(Body::Type {
                ip: hex_or_null(f.values[0])?,
                af: parsed(f.values[1])?,
            })
        }
        "case" => case(fields).map(Body::Case),
        "from" => conversion(fields).map(Body::From),
        "cmp" => {
            let (a, b) = (sockaddr_keys("a_"), sockaddr_keys("b_"));
            let keys = [&a[0], &a[1], &a[2], &b[0], &b[1], &b[2], "sign"];
            let keys: Vec<&str> = keys.iter().map(|key| key.as_ref()).collect();
            let f = Fields::take(fields, &keys)?;
            Ok(Body::Cmp {
                a: f.sockaddr(0)?,
                b: f.sockaddr(3)?,
                sign: parsed(f.values[6])?,
            })
        }
        _ => Err(format!("bad kind {kind}")),
    }
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    match fields.as_slice() {
        [("kind", "\"constant\""), ("name", name), ("value", value)] => Ok(Record {
            upstream: false,
            body: Body::Constant {
                name: quoted(name)?.to_owned(),
                value: parsed(value)?,
            },
        }),
        [("kind", kind), ("source", source), rest @ ..] => Ok(Record {
            upstream: match quoted(source)? {
                "upstream" => true,
                "sweep" => false,
                other => return Err(format!("bad source {other}")),
            },
            body: body(quoted(kind)?, rest)?,
        }),
        _ => Err(format!("bad record: {line}")),
    }
}

fn check_sockaddr(what: &str, s: &Sockaddr) -> Result<(), String> {
    if s.ip.len() > MAX_IP_LEN {
        Err(format!("{what} text is longer than the field"))
    } else if s.ip.contains(&0) {
        Err(format!("{what} text holds a NUL"))
    } else {
        Ok(())
    }
}

fn check_rc(what: &str, rc: i32, allowed: &[i32]) -> Result<(), String> {
    if allowed.contains(&rc) {
        Ok(())
    } else {
        Err(format!("bad {what} result {rc}"))
    }
}

fn check(record: &Record) -> Result<(), String> {
    match &record.body {
        Body::Constant { .. } => Ok(()),
        Body::Type { af, .. } => check_rc("type", *af, &[0, 2, 10]),
        Body::Case(c) => {
            check_sockaddr("case", &c.input)?;
            check_rc("in_addr", c.in_rc, &[0, -22, -97])?;
            for (what, rc) in [
                ("in6_addr", c.in6_rc),
                ("32", c.u32_rc),
                ("32h", c.u32h_rc),
                ("sockaddr_in", c.sin_rc),
                ("sockaddr_in6", c.sin6_rc),
                ("sockaddr", c.ss_rc),
            ] {
                check_rc(what, rc, &[0, -22, -97])?;
            }
            if c.nonzero && !c.set {
                return Err("nonzero but not set".to_owned());
            }
            if (c.in_rc == 0) != (c.u32_rc == 0) || (c.u32_rc == 0) != (c.u32h_rc == 0) {
                return Err("IPv4 conversions disagree".to_owned());
            }
            Ok(())
        }
        Body::From(f) => {
            check_sockaddr("from", &f.out)?;
            check_rc("from", f.rc, &[0, -5, -22, -28])
        }
        Body::Cmp { a, b, sign } => {
            check_sockaddr("cmp", a)?;
            check_sockaddr("cmp", b)?;
            check_rc("cmp", *sign, &[-1, 0, 1])
        }
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"sockaddr_str-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        replace("sockaddr_str-v1", "sockaddr_str-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"constant""#, r#""kind":"other""#),
        replace(r#""kind":"type""#, r#""kind":"case""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""ip":null,"af":0"#, r#""ip":null,"af":1"#),
        replace(r#""ip":null,"af":0"#, r#""ip":null,"af":00"#),
        replace(r#""set":1,"nonzero":1"#, r#""set":0,"nonzero":1"#),
        replace(r#""in_rc":0,"in":"01020304""#, r#""in_rc":0,"in":"010203""#),
        replace(
            r#""in_rc":0,"in":"01020304""#,
            r#""in_rc":-1,"in":"01020304""#,
        ),
        replace(
            r#""in_rc":0,"in":"01020304""#,
            r#""in_rc":-22,"in":"01020304""#,
        ),
        replace(
            r#""op":"str","input":"312e322e332e34","port":5"#,
            r#""op":"str2","input":"312e322e332e34","port":5"#,
        ),
        replace(
            r#""op":"in_addr","input":"01020304""#,
            r#""op":"in6_addr","input":"01020304""#,
        ),
        replace(
            r#""out_ip":"312e322e332e34""#,
            r#""out_ip":"312e002e332e34""#,
        ),
        replace(
            r#""out_ip":"312e322e332e34""#,
            r#""out_ip":"312e322e332e3""#,
        ),
        replace(r#""sign":0"#, r#""sign":2"#),
        replace(r#""a_af":2,"a_ip""#, r#""a_ip":2,"a_af""#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
