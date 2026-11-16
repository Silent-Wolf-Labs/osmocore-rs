//! Shared fixture reader for the 3GPP TS 23.003 identities. Checks the shape of the C
//! observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x983b404f7845f5c9;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("constant", 9),
    ("valid", 587),
    ("from_str", 46),
    ("name", 747),
    ("name_buf", 49),
    ("cmp", 1106),
    ("gen", 346),
    ("parse", 50),
];
pub const UPSTREAM_COUNT: usize = 75;
/// What the harness puts in each `from_str` result beforehand.
pub const JUNK_U16: u16 = 4242;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plmn {
    pub mcc: u16,
    pub mnc: u16,
    pub mnc_3_digits: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lai {
    pub plmn: Plmn,
    pub lac: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rai {
    pub lai: Lai,
    pub rac: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cgi {
    pub lai: Lai,
    pub ci: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgiPs {
    pub rai: Rai,
    pub ci: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sai {
    pub lai: Lai,
    pub sac: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RncId {
    pub plmn: Plmn,
    pub rnc_id: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MmeId {
    pub group_id: u16,
    pub code: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gummei {
    pub plmn: Plmn,
    pub mme: MmeId,
}

/// One identity, as a record's `type` and fields give it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Mcc(u16),
    Mnc { mnc: u16, mnc_3_digits: bool },
    Plmn(Plmn),
    Lai(Lai),
    Rai(Rai),
    Cgi(Cgi),
    CgiPs(CgiPs),
    Sai(Sai),
    RncId(RncId),
    MmeId(MmeId),
    Gummei(Gummei),
}

/// The check `valid` records: `osmo_imsi_str_valid`, `osmo_msisdn_str_valid`, and
/// `osmo_imei_str_valid` without and with the check digit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Imsi,
    Msisdn,
    Imei,
    ImeiWithCheckDigit,
}

/// The input of a domain name generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenInput {
    Home(Plmn),
    Mme(Gummei),
    MmeGroup { group_id: u16, plmn: Plmn },
}

/// A parsed domain name: the PLMN of a home network domain or the GUMMEI of an MME's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    Home(Plmn),
    Mme(Gummei),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Constant {
        name: String,
        value: i64,
    },
    Valid {
        check: Check,
        /// `None` for a NULL text.
        input: Option<Vec<u8>>,
        result: bool,
    },
    /// `osmo_mnc_from_str` and `osmo_mcc_from_str` on one text, each into junk results.
    FromStr {
        input: Option<Vec<u8>>,
        rc: i32,
        mnc: u16,
        mnc_3_digits: bool,
        mcc_rc: i32,
        mcc: u16,
    },
    /// Every name variant gave this text.
    Name {
        id: Id,
        text: String,
    },
    /// A `_name_buf` call with a short buffer, which held `0x55` beforehand.
    NameBuf {
        id: Id,
        buf: Vec<u8>,
    },
    Cmp {
        a: Id,
        b: Id,
        result: i32,
    },
    /// The text written, or `None` where the call failed and left the buffer untouched.
    Gen {
        input: GenInput,
        rc: i32,
        out: Option<Vec<u8>>,
    },
    Parse {
        input: Vec<u8>,
        rc: i32,
        out: Parsed,
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
            Body::Constant { .. } => "constant",
            Body::Valid { .. } => "valid",
            Body::FromStr { .. } => "from_str",
            Body::Name { .. } => "name",
            Body::NameBuf { .. } => "name_buf",
            Body::Cmp { .. } => "cmp",
            Body::Gen { .. } => "gen",
            Body::Parse { .. } => "parse",
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

    fn flag(&mut self, key: &str) -> Result<bool, String> {
        flag(self.raw(key)?)
    }

    fn end(&self) -> Result<(), String> {
        match self.fields.first() {
            None => Ok(()),
            Some((key, _)) => Err(format!("unexpected key {key}")),
        }
    }

    fn plmn(&mut self, prefix: &str) -> Result<Plmn, String> {
        Ok(Plmn {
            mcc: self.number(&format!("{prefix}mcc"))?,
            mnc: self.number(&format!("{prefix}mnc"))?,
            mnc_3_digits: self.flag(&format!("{prefix}mnc_3_digits"))?,
        })
    }

    fn lai(&mut self, prefix: &str) -> Result<Lai, String> {
        Ok(Lai {
            plmn: self.plmn(prefix)?,
            lac: self.number(&format!("{prefix}lac"))?,
        })
    }

    fn rai(&mut self, prefix: &str) -> Result<Rai, String> {
        Ok(Rai {
            lai: self.lai(prefix)?,
            rac: self.number(&format!("{prefix}rac"))?,
        })
    }

    fn mme_id(&mut self, prefix: &str) -> Result<MmeId, String> {
        Ok(MmeId {
            group_id: self.number(&format!("{prefix}group_id"))?,
            code: self.number(&format!("{prefix}code"))?,
        })
    }

    fn gummei(&mut self, prefix: &str) -> Result<Gummei, String> {
        Ok(Gummei {
            plmn: self.plmn(prefix)?,
            mme: self.mme_id(prefix)?,
        })
    }

    fn id(&mut self, kind: &str, prefix: &str) -> Result<Id, String> {
        let key = |name: &str| format!("{prefix}{name}");
        Ok(match kind {
            "mcc" => Id::Mcc(self.number(&key("mcc"))?),
            "mnc" => Id::Mnc {
                mnc: self.number(&key("mnc"))?,
                mnc_3_digits: self.flag(&key("mnc_3_digits"))?,
            },
            "plmn" => Id::Plmn(self.plmn(prefix)?),
            "lai" => Id::Lai(self.lai(prefix)?),
            "rai" => Id::Rai(self.rai(prefix)?),
            "cgi" => Id::Cgi(Cgi {
                lai: self.lai(prefix)?,
                ci: self.number(&key("ci"))?,
            }),
            "cgi_ps" => Id::CgiPs(CgiPs {
                rai: self.rai(prefix)?,
                ci: self.number(&key("ci"))?,
            }),
            "sai" => Id::Sai(Sai {
                lai: self.lai(prefix)?,
                sac: self.number(&key("sac"))?,
            }),
            "rnc_id" => Id::RncId(RncId {
                plmn: self.plmn(prefix)?,
                rnc_id: self.number(&key("rnc_id"))?,
            }),
            "mme_id" => Id::MmeId(self.mme_id(prefix)?),
            "gummei" => Id::Gummei(self.gummei(prefix)?),
            _ => return Err(format!("bad type {kind}")),
        })
    }
}

fn body(kind: &str, fields: &[(&str, &str)]) -> Result<Body, String> {
    let mut c = Cursor { fields };
    let body = match kind {
        "valid" => Body::Valid {
            check: match quoted(c.raw("check")?)? {
                "imsi" => Check::Imsi,
                "msisdn" => Check::Msisdn,
                "imei" => Check::Imei,
                "imei_check" => Check::ImeiWithCheckDigit,
                other => return Err(format!("bad check {other}")),
            },
            input: hex_or_null(c.raw("input")?)?,
            result: c.flag("result")?,
        },
        "from_str" => Body::FromStr {
            input: hex_or_null(c.raw("input")?)?,
            rc: c.number("rc")?,
            mnc: c.number("mnc")?,
            mnc_3_digits: c.flag("mnc_3_digits")?,
            mcc_rc: c.number("mcc_rc")?,
            mcc: c.number("mcc")?,
        },
        "name" => {
            let kind = quoted(c.raw("type")?)?;
            Body::Name {
                id: c.id(kind, "")?,
                text: quoted(c.raw("text")?)?.to_owned(),
            }
        }
        "name_buf" => {
            let kind = quoted(c.raw("type")?)?;
            let id = c.id(kind, "")?;
            let buf_len: usize = c.number("buf_len")?;
            let buf = hex(c.raw("buf")?)?;
            if buf.len() != buf_len {
                return Err(format!("buffer of {} bytes, not {buf_len}", buf.len()));
            }
            Body::NameBuf { id, buf }
        }
        "cmp" => {
            let kind = quoted(c.raw("type")?)?;
            Body::Cmp {
                a: c.id(kind, "a_")?,
                b: c.id(kind, "b_")?,
                result: c.number("result")?,
            }
        }
        "gen" => {
            let input = match quoted(c.raw("type")?)? {
                "home" => GenInput::Home(c.plmn("")?),
                "mme" => GenInput::Mme(c.gummei("")?),
                "mme_group" => GenInput::MmeGroup {
                    group_id: c.number("group_id")?,
                    plmn: c.plmn("")?,
                },
                other => return Err(format!("bad domain {other}")),
            };
            Body::Gen {
                input,
                rc: c.number("rc")?,
                out: hex_or_null(c.raw("out")?)?,
            }
        }
        "parse" => {
            let domain = quoted(c.raw("type")?)?;
            let input = hex(c.raw("input")?)?;
            let rc = c.number("rc")?;
            let out = match domain {
                "home" => Parsed::Home(c.plmn("")?),
                "mme" => Parsed::Mme(c.gummei("")?),
                other => return Err(format!("bad domain {other}")),
            };
            Body::Parse { input, rc, out }
        }
        _ => return Err(format!("bad kind {kind}")),
    };
    c.end()?;
    Ok(body)
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

fn check_rc(what: &str, rc: i32, allowed: &[i32]) -> Result<(), String> {
    if allowed.contains(&rc) {
        Ok(())
    } else {
        Err(format!("bad {what} result {rc}"))
    }
}

fn same_type(a: &Id, b: &Id) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

fn check(record: &Record) -> Result<(), String> {
    match &record.body {
        Body::Constant { .. } | Body::Valid { .. } => Ok(()),
        Body::FromStr {
            rc,
            mnc,
            mnc_3_digits,
            mcc_rc,
            mcc,
            ..
        } => {
            check_rc("from_str", *rc, &[0, -22])?;
            if rc != mcc_rc {
                return Err("MNC and MCC parsing disagree".to_owned());
            }
            let untouched = (*mnc, *mnc_3_digits, *mcc) == (JUNK_U16, true, JUNK_U16);
            match (*rc == 0, untouched) {
                (true, _) if *mnc > 999 || mnc != mcc => Err("bad parsed value".to_owned()),
                (false, false) => Err("a failed parse wrote its results".to_owned()),
                _ => Ok(()),
            }
        }
        Body::Name { text, .. } => {
            if text.is_empty() || !text.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
                Err(format!("bad name {text}"))
            } else {
                Ok(())
            }
        }
        Body::NameBuf { buf, .. } => match buf.split_last() {
            None => Ok(()),
            Some((0, text)) if !text.contains(&0) => Ok(()),
            _ => Err("a short buffer must end at its only NUL".to_owned()),
        },
        Body::Cmp { a, b, result } => {
            if !same_type(a, b) {
                return Err("comparison of different types".to_owned());
            }
            check_rc("cmp", *result, &[-1, 0, 1])
        }
        Body::Gen { rc, out, .. } => match (rc, out) {
            (-22, None) => Ok(()),
            (rc, Some(out)) if usize::try_from(*rc) == Ok(out.len()) => Ok(()),
            _ => Err(format!("domain result {rc} disagrees with its text")),
        },
        Body::Parse { rc, .. } => check_rc("parse", *rc, &[0, -1, -22]),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm23003-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        replace("gsm23003-v1", "gsm23003-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"constant""#, r#""kind":"other""#),
        replace(r#""kind":"valid""#, r#""kind":"cmp""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""check":"imsi""#, r#""check":"imei_luhn""#),
        replace(r#""input":null,"result":0"#, r#""input":null,"result":2"#),
        replace(r#""rc":-22,"mnc":4242"#, r#""rc":-22,"mnc":4241"#),
        replace(r#""rc":0,"mnc":0"#, r#""rc":0,"mnc":00"#),
        replace(r#""mcc_rc":-22"#, r#""mcc_rc":0"#),
        replace(r#""text":"901-70""#, r#""text":"901 70""#),
        replace(r#""type":"plmn","mcc""#, r#""type":"lai","mcc""#),
        replace(r#""buf_len":1,"buf":"00""#, r#""buf_len":1,"buf":"0000""#),
        replace(r#""buf_len":1,"buf":"00""#, r#""buf_len":1,"buf":"55""#),
        replace(r#""result":-1}"#, r#""result":-2}"#),
        replace(r#""a_mcc":1,"a_mnc""#, r#""a_mnc":1,"a_mcc""#),
        replace(r#""rc":-22,"out":null"#, r#""rc":0,"out":null"#),
        replace(r#""rc":33,"out""#, r#""rc":34,"out""#),
        replace(r#""type":"home","input""#, r#""type":"mme","input""#),
        replace(r#""rc":-1,"mcc""#, r#""rc":-2,"mcc""#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
