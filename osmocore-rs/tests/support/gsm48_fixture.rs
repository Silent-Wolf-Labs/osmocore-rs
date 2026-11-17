//! Shared fixture reader for the 3GPP TS 24.008 identity encodings. Checks the shape of the
//! C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0xcaec114e46e14ee4;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("constant", 18),
    ("to_bcd", 390),
    ("from_bcd", 5120),
    ("lai_encode", 651),
    ("lai_decode", 71),
    ("ra_encode", 500),
    ("ra_decode", 79),
    ("ra_equal", 64),
    ("rai_name", 502),
    ("rai_name_buf", 52),
    ("mi_type_name", 256),
    ("mi_bytes", 2981),
    ("mi_to_string", 225),
    ("mi_name_buf", 157),
    ("generate_mid", 279),
    ("generate_mid_from_tmsi", 6),
    ("mi_encode", 1502),
    ("mi_str", 125),
    ("mi_cmp", 256),
    ("l3", 1654),
];
pub const UPSTREAM_COUNT: usize = 184;
/// What the harness puts in each decoded field beforehand.
pub const JUNK_U16: u16 = 4242;
/// What the harness puts in `osmo_mobile_identity_encoded_len`'s digit count beforehand.
pub const JUNK_INT: i32 = -4242;
/// The byte every output buffer holds beforehand.
pub const GUARD: u8 = 0x55;
/// The size of the output buffers, and the length the harness passes for "large enough".
pub const OUT_SIZE: usize = 96;
/// How many bytes of an encoder's output buffer a record holds.
pub const OUT_RECORDED: usize = 12;

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

/// A routing area as both C structs hold it: `osmo_routing_area_id` and `gprs_ra_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rai {
    pub lai: Lai,
    pub rac: u8,
}

/// A decoded `struct osmo_mobile_identity`: its type, its union read as a TMSI, and for a
/// type with digits, the digits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedMi {
    pub mi_type: u8,
    pub tmsi: u32,
    pub digits: Option<String>,
}

/// An identity the harness built to encode, print or compare: a TMSI for the TMSI type, and
/// digits in the union for any other type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiInput {
    pub mi_type: u8,
    pub tmsi: Option<u32>,
    pub digits: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Constant {
        name: String,
        value: i64,
    },
    /// `osmo_plmn_to_bcd`, and `gsm48_mcc_mnc_to_bcd` on the same MCC and MNC.
    ToBcd {
        plmn: Plmn,
        bcd: [u8; 3],
        legacy_bcd: [u8; 3],
    },
    /// `osmo_plmn_from_bcd`, `gsm48_mcc_mnc_from_bcd` and `gsm48_parse_ra`'s PLMN.
    FromBcd {
        bcd: [u8; 3],
        plmn: Plmn,
        legacy_mcc: u16,
        legacy_mnc: u16,
        parse_ra: Plmn,
    },
    /// `gsm48_generate_lai2`, and `gsm48_generate_lai` on the same values.
    LaiEncode {
        lai: Lai,
        bytes: [u8; 5],
        legacy_bytes: [u8; 5],
    },
    /// `gsm48_decode_lai2` and `gsm48_decode_lai`.
    LaiDecode {
        bytes: [u8; 5],
        lai: Lai,
        legacy_rc: i32,
        legacy_mcc: u16,
        legacy_mnc: u16,
        legacy_lac: u16,
    },
    /// `osmo_routing_area_id_encode_buf` into a buffer of `buflen`, and `gsm48_encode_ra` and
    /// `gsm48_construct_ra` on the same values.
    RaEncode {
        rai: Rai,
        buflen: usize,
        rc: i32,
        out: Vec<u8>,
        legacy_bytes: [u8; 6],
        construct_rc: i32,
    },
    /// `osmo_routing_area_id_decode` on `bytes`, into junk, and `gsm48_parse_ra` where there
    /// are six bytes.
    RaDecode {
        bytes: Vec<u8>,
        rc: i32,
        rai: Rai,
        parse_ra: Option<Rai>,
    },
    RaEqual {
        a: Rai,
        b: Rai,
        result: bool,
    },
    /// Every variant of `osmo_rai_name` gave this text.
    RaiName {
        rai: Rai,
        text: String,
    },
    /// `osmo_rai_name_buf` with a short buffer: the `buf_len` bytes and the one after.
    RaiNameBuf {
        rai: Rai,
        buf_len: usize,
        out: Vec<u8>,
    },
    MiTypeName {
        mi_type: u8,
        text: String,
    },
    /// One encoded identity through `osmo_mi_name`, `gsm48_mi_to_string` with 64 bytes, and
    /// `osmo_mobile_identity_decode` without and with hex digits. `bytes` is `None` for a NULL
    /// pointer, which `len` goes with.
    MiBytes {
        bytes: Option<Vec<u8>>,
        len: usize,
        name: String,
        to_string_rc: i32,
        to_string: Vec<u8>,
        rc: i32,
        decoded: DecodedMi,
        hex_rc: i32,
        hex_decoded: DecodedMi,
    },
    /// `gsm48_mi_to_string` with `str_len`: the `str_len` bytes and the one after.
    MiToString {
        bytes: Vec<u8>,
        str_len: usize,
        rc: i32,
        out: Vec<u8>,
    },
    /// `osmo_mi_name_buf` with `buf_len`: the `buf_len` bytes and the one after.
    MiNameBuf {
        bytes: Vec<u8>,
        buf_len: usize,
        out: Vec<u8>,
    },
    /// `gsm48_generate_mid`, and `gsm48_generate_mid_from_imsi` for the IMSI type.
    GenerateMid {
        id: Vec<u8>,
        mi_type: u8,
        rc: u8,
        out: Vec<u8>,
    },
    GenerateMidFromTmsi {
        tmsi: u32,
        rc: i32,
        out: Vec<u8>,
    },
    /// `osmo_mobile_identity_encoded_len` and `osmo_mobile_identity_encode_buf`. `mi` is
    /// `None` for a NULL identity.
    MiEncode {
        mi: Option<MiInput>,
        null_buf: bool,
        buflen: usize,
        allow_hex: bool,
        len_rc: i32,
        len_digits: i32,
        rc: i32,
        out: Vec<u8>,
    },
    /// `osmo_mobile_identity_to_str_buf`: with the large buffer, the text; otherwise the
    /// `buflen` bytes and the one after.
    MiStr {
        mi: Option<MiInput>,
        buflen: usize,
        rc: i32,
        out: Vec<u8>,
    },
    MiCmp {
        a: Option<MiInput>,
        b: Option<MiInput>,
        rc: i32,
    },
    /// `osmo_mobile_identity_decode_from_l3_buf`, into an identity of `0xff` bytes.
    L3 {
        l3: Vec<u8>,
        allow_hex: bool,
        rc: i32,
        decoded: DecodedMi,
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
            Body::ToBcd { .. } => "to_bcd",
            Body::FromBcd { .. } => "from_bcd",
            Body::LaiEncode { .. } => "lai_encode",
            Body::LaiDecode { .. } => "lai_decode",
            Body::RaEncode { .. } => "ra_encode",
            Body::RaDecode { .. } => "ra_decode",
            Body::RaEqual { .. } => "ra_equal",
            Body::RaiName { .. } => "rai_name",
            Body::RaiNameBuf { .. } => "rai_name_buf",
            Body::MiTypeName { .. } => "mi_type_name",
            Body::MiBytes { .. } => "mi_bytes",
            Body::MiToString { .. } => "mi_to_string",
            Body::MiNameBuf { .. } => "mi_name_buf",
            Body::GenerateMid { .. } => "generate_mid",
            Body::GenerateMidFromTmsi { .. } => "generate_mid_from_tmsi",
            Body::MiEncode { .. } => "mi_encode",
            Body::MiStr { .. } => "mi_str",
            Body::MiCmp { .. } => "mi_cmp",
            Body::L3 { .. } => "l3",
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

fn hex_array<const N: usize>(value: &str) -> Result<[u8; N], String> {
    hex(value)?
        .try_into()
        .map_err(|bytes: Vec<u8>| format!("{} bytes, not {N}", bytes.len()))
}

fn text_or_null(value: &str) -> Result<Option<String>, String> {
    match value {
        "null" => Ok(None),
        _ => Ok(Some(quoted(value)?.to_owned())),
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

    fn next_is(&self, key: &str) -> bool {
        self.fields
            .first()
            .is_some_and(|(actual, _)| *actual == key)
    }

    fn number<T: std::str::FromStr + ToString>(&mut self, key: &str) -> Result<T, String> {
        parsed(self.raw(key)?)
    }

    fn flag(&mut self, key: &str) -> Result<bool, String> {
        flag(self.raw(key)?)
    }

    fn hex(&mut self, key: &str) -> Result<Vec<u8>, String> {
        hex(self.raw(key)?)
    }

    fn text(&mut self, key: &str) -> Result<String, String> {
        Ok(quoted(self.raw(key)?)?.to_owned())
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

    fn decoded(&mut self, prefix: &str) -> Result<DecodedMi, String> {
        Ok(DecodedMi {
            mi_type: self.number(&format!("{prefix}type"))?,
            tmsi: self.number(&format!("{prefix}tmsi"))?,
            digits: text_or_null(self.raw(&format!("{prefix}digits"))?)?,
        })
    }

    fn mi_input(&mut self, prefix: &str) -> Result<Option<MiInput>, String> {
        if self.flag(&format!("{prefix}null"))? {
            return Ok(None);
        }
        let mi_type = self.number(&format!("{prefix}type"))?;
        let tmsi = match self.raw(&format!("{prefix}tmsi"))? {
            "null" => None,
            value => Some(parsed(value)?),
        };
        let digits = text_or_null(self.raw(&format!("{prefix}digits"))?)?;
        if tmsi.is_some() == digits.is_some() {
            return Err("an identity has a TMSI or digits".to_owned());
        }
        Ok(Some(MiInput {
            mi_type,
            tmsi,
            digits,
        }))
    }
}

fn body(kind: &str, fields: &[(&str, &str)]) -> Result<Body, String> {
    let mut c = Cursor { fields };
    let body = match kind {
        "to_bcd" => Body::ToBcd {
            plmn: c.plmn("")?,
            bcd: hex_array(c.raw("bcd")?)?,
            legacy_bcd: hex_array(c.raw("legacy_bcd")?)?,
        },
        "from_bcd" => Body::FromBcd {
            bcd: hex_array(c.raw("bcd")?)?,
            plmn: c.plmn("")?,
            legacy_mcc: c.number("legacy_mcc")?,
            legacy_mnc: c.number("legacy_mnc")?,
            parse_ra: c.plmn("parse_ra_")?,
        },
        "lai_encode" => Body::LaiEncode {
            lai: c.lai("")?,
            bytes: hex_array(c.raw("bytes")?)?,
            legacy_bytes: hex_array(c.raw("legacy_bytes")?)?,
        },
        "lai_decode" => Body::LaiDecode {
            bytes: hex_array(c.raw("bytes")?)?,
            lai: c.lai("")?,
            legacy_rc: c.number("legacy_rc")?,
            legacy_mcc: c.number("legacy_mcc")?,
            legacy_mnc: c.number("legacy_mnc")?,
            legacy_lac: c.number("legacy_lac")?,
        },
        "ra_encode" => Body::RaEncode {
            rai: c.rai("")?,
            buflen: c.number("buflen")?,
            rc: c.number("rc")?,
            out: c.hex("out")?,
            legacy_bytes: hex_array(c.raw("legacy_bytes")?)?,
            construct_rc: c.number("construct_rc")?,
        },
        "ra_decode" => Body::RaDecode {
            bytes: c.hex("bytes")?,
            rc: c.number("rc")?,
            rai: c.rai("")?,
            parse_ra: if c.next_is("parse_ra_mcc") {
                Some(c.rai("parse_ra_")?)
            } else {
                None
            },
        },
        "ra_equal" => Body::RaEqual {
            a: c.rai("a_")?,
            b: c.rai("b_")?,
            result: c.flag("result")?,
        },
        "rai_name" => Body::RaiName {
            rai: c.rai("")?,
            text: c.text("text")?,
        },
        "rai_name_buf" => Body::RaiNameBuf {
            rai: c.rai("")?,
            buf_len: c.number("buf_len")?,
            out: c.hex("out")?,
        },
        "mi_type_name" => Body::MiTypeName {
            mi_type: c.number("type")?,
            text: c.text("text")?,
        },
        "mi_bytes" => {
            let (bytes, len) = if c.flag("null")? {
                (None, c.number("len")?)
            } else {
                let bytes = c.hex("bytes")?;
                let len = bytes.len();
                (Some(bytes), len)
            };
            Body::MiBytes {
                bytes,
                len,
                name: c.text("name")?,
                to_string_rc: c.number("to_string_rc")?,
                to_string: c.hex("to_string")?,
                rc: c.number("rc")?,
                decoded: c.decoded("")?,
                hex_rc: c.number("hex_rc")?,
                hex_decoded: c.decoded("hex_")?,
            }
        }
        "mi_to_string" => Body::MiToString {
            bytes: c.hex("bytes")?,
            str_len: c.number("str_len")?,
            rc: c.number("rc")?,
            out: c.hex("out")?,
        },
        "mi_name_buf" => Body::MiNameBuf {
            bytes: c.hex("bytes")?,
            buf_len: c.number("buf_len")?,
            out: c.hex("out")?,
        },
        "generate_mid" => Body::GenerateMid {
            id: c.hex("id")?,
            mi_type: c.number("type")?,
            rc: c.number("rc")?,
            out: c.hex("out")?,
        },
        "generate_mid_from_tmsi" => Body::GenerateMidFromTmsi {
            tmsi: c.number("tmsi")?,
            rc: c.number("rc")?,
            out: c.hex("out")?,
        },
        "mi_encode" => Body::MiEncode {
            mi: c.mi_input("")?,
            null_buf: c.flag("null_buf")?,
            buflen: c.number("buflen")?,
            allow_hex: c.flag("allow_hex")?,
            len_rc: c.number("len_rc")?,
            len_digits: c.number("len_digits")?,
            rc: c.number("rc")?,
            out: c.hex("out")?,
        },
        "mi_str" => Body::MiStr {
            mi: c.mi_input("")?,
            buflen: c.number("buflen")?,
            rc: c.number("rc")?,
            out: c.hex("out")?,
        },
        "mi_cmp" => Body::MiCmp {
            a: c.mi_input("a_")?,
            b: c.mi_input("b_")?,
            rc: c.number("rc")?,
        },
        "l3" => Body::L3 {
            l3: c.hex("l3")?,
            allow_hex: c.flag("allow_hex")?,
            rc: c.number("rc")?,
            decoded: c.decoded("")?,
        },
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

fn guard_only(what: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.iter().all(|&byte| byte == GUARD) {
        Ok(())
    } else {
        Err(format!("{what} wrote past its output"))
    }
}

/// A short buffer's bytes and the one after: up to a NUL, then nothing but guard bytes.
fn short_buffer(what: &str, len: usize, out: &[u8]) -> Result<(), String> {
    if out.len() != len + 1 {
        return Err(format!("{what} holds {} bytes, not {}", out.len(), len + 1));
    }
    match out.iter().position(|&byte| byte == 0) {
        Some(nul) if nul < len => guard_only(what, &out[nul + 1..]),
        None if len == 0 => guard_only(what, out),
        _ => Err(format!("{what} must end at a NUL within its length")),
    }
}

fn check_decoded(what: &str, rc: i32, mi: &DecodedMi) -> Result<(), String> {
    let has_digits = matches!(mi.mi_type, 1..=3);
    if has_digits != mi.digits.is_some() {
        return Err(format!("{what} digits disagree with the type"));
    }
    if rc != 0 && (mi.mi_type != 0 || mi.digits.is_some()) {
        return Err(format!("a failed {what} left an identity"));
    }
    Ok(())
}

fn check(record: &Record) -> Result<(), String> {
    match &record.body {
        Body::Constant { .. }
        | Body::ToBcd { .. }
        | Body::LaiEncode { .. }
        | Body::RaEqual { .. }
        | Body::MiTypeName { .. } => Ok(()),
        Body::FromBcd {
            plmn,
            legacy_mcc,
            legacy_mnc,
            ..
        } => {
            if (*legacy_mcc, *legacy_mnc) == (plmn.mcc, plmn.mnc) {
                Ok(())
            } else {
                Err("the legacy decoder disagrees on the MCC and MNC".to_owned())
            }
        }
        Body::LaiDecode { legacy_rc, .. } => check_rc("decode_lai", *legacy_rc, &[0]),
        Body::RaEncode {
            buflen,
            rc,
            out,
            construct_rc,
            ..
        } => {
            check_rc("construct_ra", *construct_rc, &[6])?;
            if out.len() != OUT_RECORDED {
                return Err(format!("encoder output of {} bytes", out.len()));
            }
            match rc {
                6 if *buflen >= 6 => guard_only("ra_encode", &out[6..]),
                -28 if *buflen < 6 => guard_only("ra_encode", out),
                _ => Err(format!("bad ra_encode result {rc} for {buflen} bytes")),
            }
        }
        Body::RaDecode {
            bytes,
            rc,
            rai,
            parse_ra,
        } => {
            if parse_ra.is_some() != (bytes.len() >= 6) {
                return Err("gsm48_parse_ra needs six bytes".to_owned());
            }
            let junk = Rai {
                lai: Lai {
                    plmn: Plmn {
                        mcc: JUNK_U16,
                        mnc: JUNK_U16,
                        mnc_3_digits: true,
                    },
                    lac: JUNK_U16,
                },
                rac: 0xa5,
            };
            match rc {
                6 if bytes.len() >= 6 => Ok(()),
                -28 if bytes.len() < 6 && *rai == junk => Ok(()),
                _ => Err(format!("bad ra_decode result {rc}")),
            }
        }
        Body::RaiName { text, .. } => {
            if text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'-')
            {
                Ok(())
            } else {
                Err(format!("bad name {text}"))
            }
        }
        Body::RaiNameBuf { buf_len, out, .. } => short_buffer("rai_name_buf", *buf_len, out),
        Body::MiBytes {
            bytes,
            len,
            to_string_rc,
            to_string,
            rc,
            decoded,
            hex_rc,
            hex_decoded,
            ..
        } => {
            if bytes.as_ref().is_some_and(|bytes| bytes.len() != *len) {
                return Err("length disagrees with the bytes".to_owned());
            }
            if to_string.contains(&0) || *to_string_rc < 0 {
                return Err("bad mi_to_string result".to_owned());
            }
            check_rc("decode", *rc, &[0, -22, -74])?;
            check_rc("decode", *hex_rc, &[0, -22, -74])?;
            check_decoded("decode", *rc, decoded)?;
            check_decoded("decode", *hex_rc, hex_decoded)
        }
        Body::MiToString { str_len, out, .. } => {
            if *str_len == 0 {
                guard_only("mi_to_string", out)
            } else {
                short_buffer("mi_to_string", *str_len, out)
            }
        }
        Body::MiNameBuf { buf_len, out, .. } => short_buffer("mi_name_buf", *buf_len, out),
        Body::GenerateMid { rc, out, .. } => {
            if out.len() != usize::from(*rc) || out.len() < 3 {
                return Err("generate_mid output disagrees with its length".to_owned());
            }
            if out[0] != 0x17 || usize::from(out[1]) + 2 != out.len() {
                return Err("generate_mid must write a Mobile Identity TLV".to_owned());
            }
            Ok(())
        }
        Body::GenerateMidFromTmsi { rc, out, .. } => {
            check_rc("generate_mid_from_tmsi", *rc, &[7])?;
            if out.len() == 7 {
                Ok(())
            } else {
                Err("generate_mid_from_tmsi writes seven bytes".to_owned())
            }
        }
        Body::MiEncode {
            buflen,
            len_rc,
            len_digits,
            rc,
            out,
            null_buf,
            ..
        } => {
            if out.len() != OUT_RECORDED {
                return Err(format!("encoder output of {} bytes", out.len()));
            }
            check_rc("encoded_len", *len_rc, &[-22, -95, 4, 5, 8, 9])?;
            if (*len_rc < 0) != (*len_digits == JUNK_INT) {
                return Err("encoded_len wrote its digits on failure, or not on success".to_owned());
            }
            match rc {
                1..=9 if *rc == *len_rc => guard_only("mi_encode", &out[*rc as usize..]),
                -5 if *null_buf || *buflen == 0 => guard_only("mi_encode", out),
                -28 if (*buflen as i32) < *len_rc => guard_only("mi_encode", out),
                -22 | -95 => Ok(()),
                _ => Err(format!("bad mi_encode result {rc}")),
            }
        }
        Body::MiStr {
            buflen, rc, out, ..
        } => {
            if *rc < 0 {
                return Err("to_str_buf counts characters".to_owned());
            }
            match buflen {
                &OUT_SIZE if out.len() == *rc as usize => Ok(()),
                &OUT_SIZE => Err("to_str_buf text disagrees with its length".to_owned()),
                0 => guard_only("mi_str", out),
                _ => short_buffer("mi_str", *buflen, out),
            }
        }
        Body::MiCmp { a, b, rc } => {
            if a == b && *rc != 0 {
                Err("an identity must equal itself".to_owned())
            } else {
                Ok(())
            }
        }
        Body::L3 { rc, decoded, .. } => {
            check_rc("decode_from_l3", *rc, &[0, -22, -74, -95])?;
            check_decoded("decode_from_l3", *rc, decoded)
        }
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsm48-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        replace("gsm48-v1", "gsm48-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"constant""#, r#""kind":"other""#),
        replace(r#""kind":"to_bcd""#, r#""kind":"from_bcd""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(
            r#""bcd":"00f000","legacy_bcd""#,
            r#""bcd":"00f0","legacy_bcd""#,
        ),
        replace(
            r#""legacy_mcc":0,"legacy_mnc":0"#,
            r#""legacy_mcc":0,"legacy_mnc":1"#,
        ),
        replace(r#""legacy_rc":0"#, r#""legacy_rc":-1"#),
        replace(
            r#""buflen":0,"rc":-28,"out":"55"#,
            r#""buflen":0,"rc":6,"out":"55"#,
        ),
        replace(r#""construct_rc":6"#, r#""construct_rc":5"#),
        replace(r#""rc":-28,"mcc":4242"#, r#""rc":-28,"mcc":4241"#),
        replace(r#""text":"001-02-23-42""#, r#""text":"001 02-23-42""#),
        replace(r#""buf_len":1,"out":"0055""#, r#""buf_len":1,"out":"3155""#),
        replace(r#""null":1,"len":0"#, r#""null":1,"len":00"#),
        replace(
            r#""rc":-74,"type":0,"tmsi":0,"digits":null"#,
            r#""rc":-74,"type":1,"tmsi":0,"digits":null"#,
        ),
        replace(
            r#""str_len":0,"rc":0,"out":"55""#,
            r#""str_len":0,"rc":0,"out":"00""#,
        ),
        replace(
            r#""type":1,"rc":10,"out":"1708"#,
            r#""type":1,"rc":11,"out":"1708"#,
        ),
        replace(
            r#""len_rc":-95,"len_digits":-4242"#,
            r#""len_rc":-95,"len_digits":0"#,
        ),
        replace(
            r#""a_null":1,"b_null":1,"rc":0"#,
            r#""a_null":1,"b_null":1,"rc":1"#,
        ),
        replace(r#""allow_hex":0,"rc":-95"#, r#""allow_hex":0,"rc":-96"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
