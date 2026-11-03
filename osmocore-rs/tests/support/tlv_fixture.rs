//! Shared TLV fixture reader. Checks the shape of the C observations; the expected
//! results come only from the recording.

pub const FINGERPRINT: u64 = 0xa2afad7cb72d759f;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order.
pub const SECTIONS: &[(&str, usize)] = &[
    ("def", 4),
    ("parse_one", 5532),
    ("parse", 824),
    ("encode_one", 540),
    ("encode", 10),
    ("msgb", 783),
    ("gross", 55),
    ("shift", 1170),
    ("merge", 1),
    ("copy", 1),
    ("patch", 1),
    ("access", 8),
    ("prot", 100),
    ("prot_name", 7),
    ("prot_ie", 5),
];
/// A C output field left unwritten or set to NULL.
pub const UNTOUCHED: i64 = -1;
pub const IS_NULL: i64 = -2;
pub const FILL: u8 = 0xa5;
/// A message's total size, and its headroom for appending or prepending.
pub const MSG_SIZE: usize = 65000;
pub const PUT_HEADROOM: usize = 8000;
pub const PUSH_HEADROOM: usize = 55000;
/// Bytes already in a message before a `msgb` record's call.
pub const MSG_PREFIX: [u8; 2] = [0xc0, 0xc1];

/// The value bytes every encoding call was given.
pub fn value(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index * 37 + 0x5b) as u8).collect()
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

/// Recorded output, kept as a hash once it's long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Bytes(Vec<u8>),
    Hashed { len: usize, fnv: u64 },
}

impl Output {
    pub fn matches(&self, bytes: &[u8]) -> bool {
        match self {
            Self::Bytes(expected) => expected == bytes,
            Self::Hashed { len, fnv } => *len == bytes.len() && *fnv == fnv1a(bytes),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Hashed { len, .. } => *len,
        }
    }
}

/// A tag, its format name and its fixed length.
pub type DefEntry = (u8, String, u8);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Def {
        name: String,
        entries: Vec<DefEntry>,
    },
    ParseOne {
        def: String,
        input: Vec<u8>,
        rc: i32,
        tag: u8,
        len: u16,
        val: i64,
    },
    Parse {
        def: String,
        input: Vec<u8>,
        lv1: u8,
        lv2: u8,
        multiples: usize,
        rc: i32,
        /// Index, tag, value offset and length.
        dec: Vec<(usize, u8, usize, usize)>,
    },
    EncodeOne {
        kind: String,
        tag: u8,
        len: u32,
        rc: i32,
        out: Output,
    },
    Encode {
        def: String,
        encode_def: String,
        input: Vec<u8>,
        ordered: bool,
        order: Vec<u8>,
        rc: i32,
        out: Output,
    },
    Msgb {
        op: String,
        tag: u16,
        len: u16,
        num: u32,
        ret: i64,
        headroom: usize,
        out: Output,
    },
    Gross {
        len: u32,
        lv: u32,
        tlv: u32,
        tlv16: u32,
        tl16v: u32,
        l16tv: u32,
        t16lv: u32,
        tvlv: u16,
    },
    GrossGan {
        tag: u16,
        len: u16,
        vtvl: u16,
        vtvlv: u16,
    },
    Shift {
        op: String,
        arg: u32,
        input: Vec<u8>,
        rc: i32,
        consumed: usize,
        remaining: usize,
        tag: u8,
        val: i64,
        val_len: usize,
    },
    Merge {
        dst_input: Vec<u8>,
        src_input: Vec<u8>,
        rc: i32,
        /// Tag, length and the bytes at the value pointer.
        result: Vec<(u8, u16, Vec<u8>)>,
    },
    Copy {
        input: Vec<u8>,
        lens: Vec<(u8, u16)>,
        present: Vec<u8>,
    },
    Patch {
        dst: Vec<DefEntry>,
        src: Vec<DefEntry>,
        result: Vec<DefEntry>,
    },
    Access {
        tag: u8,
        present: bool,
        len: i32,
        val8: u8,
        val16be: i32,
        val32be: i64,
        val16_unal: i32,
        val32_unal: i64,
        min_len: Vec<bool>,
    },
    Prot {
        prot: String,
        msg_type: u8,
        input: Vec<u8>,
        multiples: usize,
        parse: i32,
        validate: i32,
    },
    ProtName {
        msg_type: u8,
        plain: String,
        named: String,
        flags: u32,
    },
    ProtIe {
        iei: u8,
        name: String,
    },
}

impl Record {
    fn section(&self) -> &'static str {
        match self {
            Self::Def { .. } => "def",
            Self::ParseOne { .. } => "parse_one",
            Self::Parse { .. } => "parse",
            Self::EncodeOne { .. } => "encode_one",
            Self::Encode { .. } => "encode",
            Self::Msgb { .. } => "msgb",
            Self::Gross { .. } | Self::GrossGan { .. } => "gross",
            Self::Shift { .. } => "shift",
            Self::Merge { .. } => "merge",
            Self::Copy { .. } => "copy",
            Self::Patch { .. } => "patch",
            Self::Access { .. } => "access",
            Self::Prot { .. } => "prot",
            Self::ProtName { .. } => "prot_name",
            Self::ProtIe { .. } => "prot_ie",
        }
    }
}

fn hex(text: &str) -> Result<Vec<u8>, String> {
    let lowercase = text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !lowercase || !text.len().is_multiple_of(2) {
        return Err(format!("bad hex {text}"));
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).map_err(|e| e.to_string()))
        .collect()
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

fn numbers<T: std::str::FromStr + ToString>(value: &str) -> Result<Vec<T>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(' ').map(parsed).collect()
}

fn output(value: &str) -> Result<Output, String> {
    let Some(rest) = value.strip_prefix('@') else {
        return Ok(Output::Bytes(hex(value)?));
    };
    let (len, fnv) = rest
        .split_once(':')
        .ok_or_else(|| format!("bad output {value}"))?;
    if fnv.len() != 16 {
        return Err(format!("bad output hash {value}"));
    }
    Ok(Output::Hashed {
        len: parsed(len)?,
        fnv: u64::from_str_radix(fnv, 16).map_err(|e| e.to_string())?,
    })
}

fn def_entries(value: &str) -> Result<Vec<DefEntry>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<&str> = value.split(' ').collect();
    if !parts.len().is_multiple_of(3) {
        return Err(format!("bad definition {value}"));
    }
    parts
        .as_chunks::<3>()
        .0
        .iter()
        .map(|entry| Ok((parsed(entry[0])?, entry[1].to_owned(), parsed(entry[2])?)))
        .collect()
}

fn dec_entries(value: &str) -> Result<Vec<(usize, u8, usize, usize)>, String> {
    let parts: Vec<usize> = numbers(value)?;
    if !parts.len().is_multiple_of(4) {
        return Err(format!("bad entries {value}"));
    }
    parts
        .as_chunks::<4>()
        .0
        .iter()
        .map(|entry| {
            let tag = u8::try_from(entry[1]).map_err(|e| e.to_string())?;
            Ok((entry[0], tag, entry[2], entry[3]))
        })
        .collect()
}

fn merge_entries(value: &str) -> Result<Vec<(u8, u16, Vec<u8>)>, String> {
    let parts: Vec<&str> = value.split(' ').collect();
    if !parts.len().is_multiple_of(3) {
        return Err(format!("bad merge result {value}"));
    }
    parts
        .as_chunks::<3>()
        .0
        .iter()
        .map(|entry| {
            let bytes = if entry[2] == "-" {
                Vec::new()
            } else {
                hex(entry[2])?
            };
            Ok((parsed(entry[0])?, parsed(entry[1])?, bytes))
        })
        .collect()
}

fn pairs(value: &str) -> Result<Vec<(u8, u16)>, String> {
    let parts: Vec<u16> = numbers(value)?;
    if !parts.len().is_multiple_of(2) {
        return Err(format!("bad pairs {value}"));
    }
    parts
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let tag = u8::try_from(pair[0]).map_err(|e| e.to_string())?;
            Ok((tag, pair[1]))
        })
        .collect()
}

fn flag(value: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        other => Err(format!("bad flag {other}")),
    }
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
        Some("def") => {
            expect(&["kind", "name", "entries"])?;
            Ok(Record::Def {
                name: value(1).to_owned(),
                entries: def_entries(value(2))?,
            })
        }
        Some("parse_one") => {
            expect(&["kind", "def", "input", "rc", "tag", "len", "val"])?;
            Ok(Record::ParseOne {
                def: value(1).to_owned(),
                input: hex(value(2))?,
                rc: parsed(value(3))?,
                tag: parsed(value(4))?,
                len: parsed(value(5))?,
                val: parsed(value(6))?,
            })
        }
        Some("parse") => {
            expect(&[
                "kind",
                "def",
                "input",
                "lv1",
                "lv2",
                "multiples",
                "rc",
                "dec",
            ])?;
            Ok(Record::Parse {
                def: value(1).to_owned(),
                input: hex(value(2))?,
                lv1: parsed(value(3))?,
                lv2: parsed(value(4))?,
                multiples: parsed(value(5))?,
                rc: parsed(value(6))?,
                dec: dec_entries(value(7))?,
            })
        }
        Some("encode_one") => {
            expect(&["kind", "type", "tag", "len", "rc", "out"])?;
            Ok(Record::EncodeOne {
                kind: value(1).to_owned(),
                tag: parsed(value(2))?,
                len: parsed(value(3))?,
                rc: parsed(value(4))?,
                out: output(value(5))?,
            })
        }
        Some("encode") => {
            expect(&[
                "kind",
                "def",
                "encode_def",
                "input",
                "ordered",
                "order",
                "rc",
                "out",
            ])?;
            Ok(Record::Encode {
                def: value(1).to_owned(),
                encode_def: value(2).to_owned(),
                input: hex(value(3))?,
                ordered: flag(value(4))?,
                order: numbers(value(5))?,
                rc: parsed(value(6))?,
                out: output(value(7))?,
            })
        }
        Some("msgb") => {
            expect(&["kind", "op", "tag", "len", "num", "ret", "headroom", "out"])?;
            Ok(Record::Msgb {
                op: value(1).to_owned(),
                tag: parsed(value(2))?,
                len: parsed(value(3))?,
                num: parsed(value(4))?,
                ret: parsed(value(5))?,
                headroom: parsed(value(6))?,
                out: output(value(7))?,
            })
        }
        Some("gross") => {
            expect(&[
                "kind", "len", "lv", "tlv", "tlv16", "tl16v", "l16tv", "t16lv", "tvlv",
            ])?;
            Ok(Record::Gross {
                len: parsed(value(1))?,
                lv: parsed(value(2))?,
                tlv: parsed(value(3))?,
                tlv16: parsed(value(4))?,
                tl16v: parsed(value(5))?,
                l16tv: parsed(value(6))?,
                t16lv: parsed(value(7))?,
                tvlv: parsed(value(8))?,
            })
        }
        Some("gross_gan") => {
            expect(&["kind", "tag", "len", "vtvl", "vtvlv"])?;
            Ok(Record::GrossGan {
                tag: parsed(value(1))?,
                len: parsed(value(2))?,
                vtvl: parsed(value(3))?,
                vtvlv: parsed(value(4))?,
            })
        }
        Some("shift") => {
            expect(&[
                "kind",
                "op",
                "arg",
                "input",
                "rc",
                "consumed",
                "remaining",
                "tag",
                "val",
                "val_len",
            ])?;
            Ok(Record::Shift {
                op: value(1).to_owned(),
                arg: parsed(value(2))?,
                input: hex(value(3))?,
                rc: parsed(value(4))?,
                consumed: parsed(value(5))?,
                remaining: parsed(value(6))?,
                tag: parsed(value(7))?,
                val: parsed(value(8))?,
                val_len: parsed(value(9))?,
            })
        }
        Some("merge") => {
            expect(&["kind", "rc", "dst_input", "src_input", "result"])?;
            Ok(Record::Merge {
                rc: parsed(value(1))?,
                dst_input: hex(value(2))?,
                src_input: hex(value(3))?,
                result: merge_entries(value(4))?,
            })
        }
        Some("copy") => {
            expect(&["kind", "input", "lens", "present"])?;
            Ok(Record::Copy {
                input: hex(value(1))?,
                lens: pairs(value(2))?,
                present: numbers(value(3))?,
            })
        }
        Some("patch") => {
            expect(&["kind", "dst", "src", "result"])?;
            Ok(Record::Patch {
                dst: def_entries(value(1))?,
                src: def_entries(value(2))?,
                result: def_entries(value(3))?,
            })
        }
        Some("access") => {
            expect(&[
                "kind",
                "tag",
                "present",
                "len",
                "val8",
                "val16be",
                "val32be",
                "val16_unal",
                "val32_unal",
                "min_len",
            ])?;
            let min_len = value(9)
                .split(' ')
                .map(flag)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Record::Access {
                tag: parsed(value(1))?,
                present: flag(value(2))?,
                len: parsed(value(3))?,
                val8: parsed(value(4))?,
                val16be: parsed(value(5))?,
                val32be: parsed(value(6))?,
                val16_unal: parsed(value(7))?,
                val32_unal: parsed(value(8))?,
                min_len,
            })
        }
        Some("prot") => {
            expect(&[
                "kind",
                "prot",
                "msg_type",
                "input",
                "multiples",
                "parse",
                "validate",
            ])?;
            Ok(Record::Prot {
                prot: value(1).to_owned(),
                msg_type: parsed(value(2))?,
                input: hex(value(3))?,
                multiples: parsed(value(4))?,
                parse: parsed(value(5))?,
                validate: parsed(value(6))?,
            })
        }
        Some("prot_name") => {
            expect(&["kind", "msg_type", "plain", "named", "flags"])?;
            Ok(Record::ProtName {
                msg_type: parsed(value(1))?,
                plain: value(2).to_owned(),
                named: value(3).to_owned(),
                flags: parsed(value(4))?,
            })
        }
        Some("prot_ie") => {
            expect(&["kind", "iei", "name"])?;
            Ok(Record::ProtIe {
                iei: parsed(value(1))?,
                name: value(2).to_owned(),
            })
        }
        other => Err(format!("unknown kind {other:?}")),
    }
}

const FORMATS: &[&str] = &[
    "none",
    "fixed",
    "t",
    "tv",
    "tlv",
    "tl16v",
    "tvlv",
    "single_tv",
    "vtvlv_gan",
];
const DEFS: &[&str] = &["mixed", "empty", "tvlv_att", "vtvlv_gan_att"];
const PARSE_ERRORS: &[i32] = &[-1, -2, -3];

fn known_def(name: &str) -> Result<(), String> {
    if DEFS.contains(&name) {
        Ok(())
    } else {
        Err(format!("unknown definition {name}"))
    }
}

fn check_entries(entries: &[DefEntry]) -> Result<(), String> {
    if !entries.windows(2).all(|pair| pair[0].0 < pair[1].0) {
        return Err("definition tags must ascend".to_owned());
    }
    match entries
        .iter()
        .find(|(_, format, _)| !FORMATS.contains(&format.as_str()))
    {
        Some((tag, format, _)) => Err(format!("tag {tag}: unknown format {format}")),
        None => Ok(()),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Def { name, entries } => {
            known_def(name)?;
            check_entries(entries)
        }
        Record::ParseOne {
            def,
            input,
            rc,
            len,
            val,
            ..
        } => {
            known_def(def)?;
            if *rc > 0 {
                if *rc as usize > input.len() {
                    return Err(format!("consumed {rc} of {} bytes", input.len()));
                }
                if *val < 0 || *val as usize + usize::from(*len) > *rc as usize {
                    return Err(format!("value outside the element: {val}+{len} of {rc}"));
                }
            } else if !PARSE_ERRORS.contains(rc) {
                return Err(format!("bad status {rc}"));
            } else if *val >= 0 {
                return Err(format!("status {rc} with a value offset"));
            } else if *rc != -2 && *val != UNTOUCHED || *rc == -2 && *val != IS_NULL {
                return Err(format!("status {rc} with value {val}"));
            }
            Ok(())
        }
        Record::Parse {
            def,
            input,
            multiples,
            rc,
            dec,
            ..
        } => {
            known_def(def)?;
            if *rc < 0 && !PARSE_ERRORS.contains(rc) {
                return Err(format!("bad status {rc}"));
            }
            for (index, _, offset, len) in dec {
                if index >= multiples {
                    return Err(format!("entry {index} past {multiples}"));
                }
                // A zero-length element can sit one past the end of the input.
                if offset + len > input.len() + usize::from(*len == 0) {
                    return Err(format!("value outside the input: {offset}+{len}"));
                }
            }
            Ok(())
        }
        Record::EncodeOne { kind, rc, out, .. } => {
            if !FORMATS.contains(&kind.as_str()) {
                return Err(format!("unknown format {kind}"));
            }
            if *rc != 0 {
                return Err(format!("known format with status {rc}"));
            }
            if kind == "none" && out.len() != 0 {
                return Err("format none wrote output".to_owned());
            }
            Ok(())
        }
        Record::Encode {
            def,
            encode_def,
            ordered,
            order,
            rc,
            out,
            ..
        } => {
            known_def(def)?;
            known_def(encode_def)?;
            if !ordered && !order.is_empty() {
                return Err("unordered encode with an order".to_owned());
            }
            if usize::try_from(*rc) != Ok(out.len()) {
                return Err(format!("status {rc} for {} bytes", out.len()));
            }
            Ok(())
        }
        Record::Msgb {
            op, headroom, out, ..
        } => {
            let expected = if op.ends_with("_push") {
                PUSH_HEADROOM
            } else {
                PUT_HEADROOM
            };
            let moved = expected
                .checked_sub(*headroom)
                .ok_or_else(|| format!("{op}: headroom grew"))?;
            if !op.ends_with("_push") && moved != 0 {
                return Err(format!("{op}: appending used headroom"));
            }
            if out.len() < MSG_PREFIX.len() {
                return Err(format!("{op}: lost existing bytes"));
            }
            Ok(())
        }
        Record::Gross { .. } | Record::GrossGan { .. } => Ok(()),
        Record::Shift {
            input,
            rc,
            consumed,
            remaining,
            val,
            val_len,
            ..
        } => {
            if consumed + remaining != input.len() {
                return Err(format!("{consumed}+{remaining} of {}", input.len()));
            }
            if *rc < -1 {
                return Err(format!("bad status {rc}"));
            }
            if *rc == -1 && *remaining != 0 {
                return Err("failure must consume the rest".to_owned());
            }
            if *val >= 0 && *val_len != 0xa5a5 && *val as usize + val_len > input.len() {
                return Err(format!("value outside the input: {val}+{val_len}"));
            }
            Ok(())
        }
        Record::Merge { rc, result, .. } => {
            if *rc != 0 || result.is_empty() {
                return Err("merge must succeed with a result".to_owned());
            }
            Ok(())
        }
        Record::Copy { lens, present, .. } => {
            if lens.is_empty() {
                return Err("copy must keep lengths".to_owned());
            }
            if !present.is_empty() {
                return Err("copy recorded values".to_owned());
            }
            Ok(())
        }
        Record::Patch { dst, src, result } => {
            check_entries(dst)?;
            check_entries(src)?;
            check_entries(result)
        }
        Record::Access {
            min_len,
            present,
            len,
            ..
        } => {
            if min_len.len() != 7 || !*present && *len != -1 {
                return Err("bad access record".to_owned());
            }
            Ok(())
        }
        Record::Prot {
            prot,
            multiples,
            parse,
            validate,
            ..
        } => {
            if prot != "plain" || !(1..=2).contains(multiples) {
                return Err("bad protocol case".to_owned());
            }
            if *parse >= 0 && parse != validate {
                return Err("parse and validate disagree".to_owned());
            }
            if ![0, -1, -2, -3, -50, -51].contains(parse) {
                return Err(format!("bad status {parse}"));
            }
            Ok(())
        }
        Record::ProtName { .. } | Record::ProtIe { .. } => Ok(()),
    }
}

/// Reads and checks the whole file. Panics on anything unexpected.
pub fn validate(text: &str) -> Vec<Record> {
    try_validate(text).unwrap_or_else(|error| panic!("{error}"))
}

fn try_validate(text: &str) -> Result<Vec<Record>, String> {
    let actual = fnv1a(text.as_bytes());
    if actual != FINGERPRINT {
        return Err(format!("fingerprint {actual:#x}"));
    }
    check_structure(text)
}

fn check_structure(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let meta = format!(r#"{{"kind":"meta","schema":"tlv-v1","commit":"{COMMIT}"}}"#);
    if lines.next() != Some(meta.as_str()) {
        return Err("bad metadata line".to_owned());
    }
    let records = lines
        .map(|line| {
            let record = record(line)?;
            check(&record).map_err(|error| format!("{error}: {line}"))?;
            Ok(record)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut rest = records.as_slice();
    for (section, count) in SECTIONS {
        let (these, after) = rest
            .split_at_checked(*count)
            .ok_or_else(|| format!("{section}: too few records"))?;
        if let Some(other) = these.iter().find(|record| record.section() != *section) {
            return Err(format!("{section}: found {}", other.section()));
        }
        rest = after;
    }
    if !rest.is_empty() {
        return Err(format!("{} extra records", rest.len()));
    }
    Ok(records)
}

/// Each edit must be rejected, with the fingerprint check out of the way.
pub fn check_corruptions(text: &str) {
    let replace = |from: &str, to: &str| {
        assert!(text.contains(from), "corruption target {from} is missing");
        text.replacen(from, to, 1)
    };
    let lines = text.lines().collect::<Vec<_>>();
    let edit_line = |kind: &str, from: &str, to: &str| {
        let line = lines
            .iter()
            .find(|line| line.contains(kind) && line.contains(from))
            .unwrap_or_else(|| panic!("no {kind} line contains {from}"));
        text.replacen(line, &line.replacen(from, to, 1), 1)
    };
    assert!(check_structure(text).is_ok());
    assert!(try_validate(&format!("{text} ")).is_err());
    for corrupt in [
        replace("tlv-v1", "tlv-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        lines[..lines.len() - 1].join("\n"),
        format!("{text}{}\n", lines[lines.len() - 1]),
        replace(r#""kind":"parse""#, r#""kind":"other""#),
        replace(r#""kind":"encode""#, r#""kind":"encode_one""#),
        replace(r#""name":"mixed""#, r#""name":"other""#),
        edit_line(r#""kind":"parse_one""#, r#""rc":-2"#, r#""rc":-9"#),
        edit_line(r#""kind":"parse_one""#, r#""val":-2"#, r#""val":-1"#),
        edit_line(r#""kind":"parse_one""#, r#""rc":1,"#, r#""rc":999,"#),
        edit_line(r#""kind":"parse""#, r#""multiples":1"#, r#""multiples":01"#),
        edit_line(r#""kind":"encode_one""#, r#""rc":0"#, r#""rc":1"#),
        edit_line(r#""type":"none""#, r#""out":"""#, r#""out":"00""#),
        edit_line(r#""kind":"encode""#, r#""rc":0"#, r#""rc":5"#),
        edit_line(
            r#""kind":"msgb""#,
            r#""headroom":8000"#,
            r#""headroom":7999"#,
        ),
        edit_line(r#""kind":"msgb""#, r#""out":"c0c1"#, r#""out":""#),
        edit_line(r#""kind":"shift""#, r#""consumed":0"#, r#""consumed":1"#),
        edit_line(r#""kind":"copy""#, r#""present":"""#, r#""present":"4""#),
        edit_line(r#""kind":"merge""#, r#""rc":0"#, r#""rc":-1"#),
        edit_line(r#""kind":"patch""#, r#""1 t 0"#, r#""1 x 0"#),
        edit_line(r#""kind":"prot""#, r#""parse":-51"#, r#""parse":-52"#),
        edit_line(r#""kind":"prot""#, r#""multiples":1"#, r#""multiples":3"#),
        edit_line(r#""kind":"access""#, r#""present":0"#, r#""present":2"#),
    ] {
        assert!(check_structure(&corrupt).is_err(), "accepted corruption");
    }
}
