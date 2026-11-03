//! Shared fixture reader for the numeric gsm_utils helpers. Checks the shape of the
//! C observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0xcb2f7eab2f6585c2;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[
    ("constant", 21),
    ("band_name", 12),
    ("band_parse", 41),
    ("class", 144),
    ("ctl_level", 564),
    ("pwr_dbm", 3072),
    ("rxlev", 256),
    ("dbm", 106),
    ("arfcn", 7196),
    ("freq", 15560),
    ("time", 1772),
    ("time_fields", 216),
    ("rfn", 395),
    ("tlli", 196),
    ("tmsi", 90),
    ("cm2", 264),
    ("cm3", 88),
    ("rach_max", 19),
    ("rach_tx", 256),
];
pub const UPSTREAM_RFN_COUNT: usize = 5;
pub const EINVAL_RESULT: i32 = -22;
pub const BAND_CODES: &[i32] = &[1, 2, 4, 8, 0x10, 0x20, 0x40, 0x80];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Constant {
        name: String,
        value: i64,
    },
    BandName {
        band: i32,
        name: String,
    },
    BandParse {
        input: String,
        band: i32,
    },
    Class {
        band: i32,
        class: i32,
        dbm: i32,
    },
    CtlLevel {
        band: i32,
        dbm: u32,
        level: i32,
    },
    PwrDbm {
        band: i32,
        level: u8,
        dbm: i32,
    },
    Rxlev {
        rxlev: u8,
        dbm: i32,
    },
    Dbm {
        dbm: i32,
        rxlev: u8,
    },
    Arfcn {
        arfcn: u16,
        rc: i32,
        /// -1 when the call left its output alone.
        band: i32,
        uplink: u16,
        downlink: u16,
    },
    Freq {
        freq10: u16,
        uplink: u16,
        downlink: u16,
    },
    Time {
        frame: u32,
        t1: u16,
        t2: u8,
        t3: u8,
        tc: u8,
        rfn: u16,
        back: u32,
        text: String,
    },
    TimeFields {
        input_frame: u32,
        t1: u16,
        t2: u8,
        t3: u8,
        frame: u32,
        text: String,
    },
    Rfn {
        upstream: bool,
        rfn: u16,
        current: u32,
        frame: u32,
    },
    Tlli {
        tlli: u32,
        tlli_type: i32,
        tmsi: u32,
    },
    Tmsi {
        tmsi: u32,
        tlli_type: i32,
        tlli: u32,
    },
    Cm2 {
        first: u8,
        third: u8,
        n: u32,
        supported: i32,
    },
    Cm3 {
        first: u8,
        n: u32,
        supported: i32,
    },
    RachMax {
        value: i32,
        val2raw: i32,
        raw2val: i32,
    },
    RachTx {
        raw: u8,
        value: u8,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::Constant { .. } => "constant",
            Self::BandName { .. } => "band_name",
            Self::BandParse { .. } => "band_parse",
            Self::Class { .. } => "class",
            Self::CtlLevel { .. } => "ctl_level",
            Self::PwrDbm { .. } => "pwr_dbm",
            Self::Rxlev { .. } => "rxlev",
            Self::Dbm { .. } => "dbm",
            Self::Arfcn { .. } => "arfcn",
            Self::Freq { .. } => "freq",
            Self::Time { .. } => "time",
            Self::TimeFields { .. } => "time_fields",
            Self::Rfn { .. } => "rfn",
            Self::Tlli { .. } => "tlli",
            Self::Tmsi { .. } => "tmsi",
            Self::Cm2 { .. } => "cm2",
            Self::Cm3 { .. } => "cm3",
            Self::RachMax { .. } => "rach_max",
            Self::RachTx { .. } => "rach_tx",
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

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let expect = |expected: &[&str]| {
        if keys.len() > 1 && keys[0] == "kind" && keys[1..] == *expected {
            Ok(())
        } else {
            Err(format!("unexpected fields {keys:?}"))
        }
    };
    let value = |index: usize| fields[index].1;
    let Some((_, kind)) = fields.first() else {
        return Err("empty record".to_owned());
    };
    Ok(match *kind {
        "constant" => {
            expect(&["name", "value"])?;
            Record::Constant {
                name: value(1).to_owned(),
                value: parsed(value(2))?,
            }
        }
        "band_name" => {
            expect(&["band", "name"])?;
            Record::BandName {
                band: parsed(value(1))?,
                name: value(2).to_owned(),
            }
        }
        "band_parse" => {
            expect(&["input", "band"])?;
            Record::BandParse {
                input: value(1).to_owned(),
                band: parsed(value(2))?,
            }
        }
        "class" => {
            expect(&["band", "class", "dbm"])?;
            Record::Class {
                band: parsed(value(1))?,
                class: parsed(value(2))?,
                dbm: parsed(value(3))?,
            }
        }
        "ctl_level" => {
            expect(&["band", "dbm", "level"])?;
            Record::CtlLevel {
                band: parsed(value(1))?,
                dbm: parsed(value(2))?,
                level: parsed(value(3))?,
            }
        }
        "pwr_dbm" => {
            expect(&["band", "level", "dbm"])?;
            Record::PwrDbm {
                band: parsed(value(1))?,
                level: parsed(value(2))?,
                dbm: parsed(value(3))?,
            }
        }
        "rxlev" => {
            expect(&["rxlev", "dbm"])?;
            Record::Rxlev {
                rxlev: parsed(value(1))?,
                dbm: parsed(value(2))?,
            }
        }
        "dbm" => {
            expect(&["dbm", "rxlev"])?;
            Record::Dbm {
                dbm: parsed(value(1))?,
                rxlev: parsed(value(2))?,
            }
        }
        "arfcn" => {
            expect(&["arfcn", "rc", "band", "ul", "dl"])?;
            Record::Arfcn {
                arfcn: parsed(value(1))?,
                rc: parsed(value(2))?,
                band: parsed(value(3))?,
                uplink: parsed(value(4))?,
                downlink: parsed(value(5))?,
            }
        }
        "freq" => {
            expect(&["freq10", "ul", "dl"])?;
            Record::Freq {
                freq10: parsed(value(1))?,
                uplink: parsed(value(2))?,
                downlink: parsed(value(3))?,
            }
        }
        "time" => {
            expect(&["fn", "t1", "t2", "t3", "tc", "rfn", "back", "text"])?;
            Record::Time {
                frame: parsed(value(1))?,
                t1: parsed(value(2))?,
                t2: parsed(value(3))?,
                t3: parsed(value(4))?,
                tc: parsed(value(5))?,
                rfn: parsed(value(6))?,
                back: parsed(value(7))?,
                text: value(8).to_owned(),
            }
        }
        "time_fields" => {
            expect(&["input_fn", "t1", "t2", "t3", "fn", "text"])?;
            Record::TimeFields {
                input_frame: parsed(value(1))?,
                t1: parsed(value(2))?,
                t2: parsed(value(3))?,
                t3: parsed(value(4))?,
                frame: parsed(value(5))?,
                text: value(6).to_owned(),
            }
        }
        "rfn" => {
            expect(&["source", "rfn", "curr", "fn"])?;
            Record::Rfn {
                upstream: match value(1) {
                    "upstream" => true,
                    "sweep" => false,
                    other => return Err(format!("unknown source {other}")),
                },
                rfn: parsed(value(2))?,
                current: parsed(value(3))?,
                frame: parsed(value(4))?,
            }
        }
        "tlli" => {
            expect(&["tlli", "type", "tmsi"])?;
            Record::Tlli {
                tlli: parsed(value(1))?,
                tlli_type: parsed(value(2))?,
                tmsi: parsed(value(3))?,
            }
        }
        "tmsi" => {
            expect(&["tmsi", "type", "tlli"])?;
            Record::Tmsi {
                tmsi: parsed(value(1))?,
                tlli_type: parsed(value(2))?,
                tlli: parsed(value(3))?,
            }
        }
        "cm2" => {
            expect(&["first", "third", "n", "supported"])?;
            Record::Cm2 {
                first: parsed(value(1))?,
                third: parsed(value(2))?,
                n: parsed(value(3))?,
                supported: parsed(value(4))?,
            }
        }
        "cm3" => {
            expect(&["first", "n", "supported"])?;
            Record::Cm3 {
                first: parsed(value(1))?,
                n: parsed(value(2))?,
                supported: parsed(value(3))?,
            }
        }
        "rach_max" => {
            expect(&["value", "val2raw", "raw2val"])?;
            Record::RachMax {
                value: parsed(value(1))?,
                val2raw: parsed(value(2))?,
                raw2val: parsed(value(3))?,
            }
        }
        "rach_tx" => {
            expect(&["raw", "value"])?;
            Record::RachTx {
                raw: parsed(value(1))?,
                value: parsed(value(2))?,
            }
        }
        other => return Err(format!("unknown kind {other}")),
    })
}

fn check(record: &Record) -> Result<(), String> {
    let band_result = |band: &i32| BAND_CODES.contains(band) || *band == EINVAL_RESULT;
    match record {
        Record::BandParse { input, band } if !band_result(band) => {
            Err(format!("parsing {input:?} gave band {band}"))
        }
        Record::Arfcn {
            arfcn, rc, band, ..
        } => {
            let valid = match rc {
                0 => BAND_CODES.contains(band),
                -1 => *band == -1,
                _ => false,
            };
            if valid {
                Ok(())
            } else {
                Err(format!("ARFCN {arfcn}: result {rc} with band {band}"))
            }
        }
        Record::Time {
            frame, t2, t3, tc, ..
        } if u32::from(*t2) != frame % 26 || u32::from(*t3) != frame % 51 || *tc > 7 => {
            Err(format!("frame {frame}: inconsistent GSM time"))
        }
        Record::Rfn { rfn, .. } if *rfn >= 42432 => {
            Err(format!("RFN {rfn} would abort the C call"))
        }
        Record::Rfn { frame, .. } if *frame >= 2715648 => {
            Err(format!("frame {frame} is past the hyperframe"))
        }
        Record::Tlli { tlli_type, .. } if !(0..=6).contains(tlli_type) => {
            Err(format!("TLLI type {tlli_type} out of range"))
        }
        Record::Cm2 { supported, .. } | Record::Cm3 { supported, .. }
            if !(0..=1).contains(supported) =>
        {
            Err(format!("support result {supported} is not a flag"))
        }
        Record::RachMax { raw2val, .. } if ![1, 2, 4, 7].contains(raw2val) => {
            Err(format!("RACH retransmissions {raw2val} out of range"))
        }
        _ => Ok(()),
    }
}

/// Check everything but the fingerprint, so corrupted copies reach these checks.
pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata =
        format!(r#"{{"schema":"gsm-utils-numeric-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        .filter(|record| matches!(record, Record::Rfn { upstream: true, .. }))
        .count();
    if upstream != UPSTREAM_RFN_COUNT {
        return Err(format!("expected {UPSTREAM_RFN_COUNT} upstream records"));
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
        replace("gsm-utils-numeric-v1", "gsm-utils-numeric-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"constant""#, r#""kind":"other""#),
        replace(r#""kind":"freq""#, r#""kind":"arfcn""#),
        replace(r#""value":2715648"#, r#""valeu":2715648"#),
        replace(r#""value":2715648"#, r#""value":02715648"#),
        replace(r#""input":"450","band":16"#, r#""input":"450","band":3"#),
        replace(
            r#""arfcn":0,"rc":0,"band":2"#,
            r#""arfcn":0,"rc":0,"band":-1"#,
        ),
        replace(
            r#""arfcn":0,"rc":0,"band":2"#,
            r#""arfcn":0,"rc":1,"band":2"#,
        ),
        replace(r#""fn":1,"t1":0,"t2":1"#, r#""fn":1,"t1":0,"t2":2"#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""rfn":5070,"curr""#, r#""rfn":42432,"curr""#),
        replace(
            r#""rfn":0,"curr":0,"fn":0"#,
            r#""rfn":0,"curr":0,"fn":2715648"#,
        ),
        replace(r#""type":0,"tmsi""#, r#""type":7,"tmsi""#),
        replace(r#""supported":1"#, r#""supported":2"#),
        replace(r#""raw2val":1"#, r#""raw2val":3"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
