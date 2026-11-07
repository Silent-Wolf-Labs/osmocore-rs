//! Shared fixture reader for strrb. Checks the shape of the C observations; the expected
//! results come only from the recording.

pub const FINGERPRINT: u64 = 0x488c8b446c61f77a;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[("constant", 1), ("event", 4437)];
pub const UPSTREAM_COUNT: usize = 462;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Create {
        requested: usize,
        size: u16,
    },
    Add {
        text: Vec<u8>,
        rc: i32,
    },
    State {
        start: u16,
        end: u16,
        elements: usize,
        empty: bool,
        /// Whether each buffer index from 0 is valid.
        valid: Vec<bool>,
    },
    Nth {
        index: u32,
        text: Option<Vec<u8>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Constant {
        name: String,
        value: i64,
    },
    Event {
        upstream: bool,
        object: String,
        event: Event,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::Constant { .. } => "constant",
            Self::Event { .. } => "event",
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

fn flag(value: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("bad flag {value}")),
    }
}

fn event(op: &str, keys: &[&str], values: &[&str]) -> Result<Event, String> {
    match (op, keys, values) {
        ("create", ["requested", "size"], [requested, size]) => Ok(Event::Create {
            requested: parsed(requested)?,
            size: parsed(size)?,
        }),
        ("add", ["text", "rc"], [text, rc]) => Ok(Event::Add {
            text: hex(text)?,
            rc: parsed(rc)?,
        }),
        (
            "state",
            ["start", "end", "elements", "empty", "valid"],
            [start, end, elements, empty, valid],
        ) => Ok(Event::State {
            start: parsed(start)?,
            end: parsed(end)?,
            elements: parsed(elements)?,
            empty: flag(empty)?,
            valid: valid
                .chars()
                .map(|bit| flag(bit.encode_utf8(&mut [0; 4])))
                .collect::<Result<_, _>>()?,
        }),
        ("nth", ["index", "text"], [index, text]) => Ok(Event::Nth {
            index: parsed(index)?,
            text: match *text {
                "null" => None,
                _ => Some(hex(text)?),
            },
        }),
        _ => Err(format!("bad {op} event")),
    }
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = fields.iter().map(|(_, value)| *value).collect();
    match (keys.as_slice(), values.as_slice()) {
        (["kind", "name", "value"], ["constant", name, value]) => Ok(Record::Constant {
            name: (*name).to_owned(),
            value: parsed(value)?,
        }),
        (
            ["kind", "source", "op", "object", more_keys @ ..],
            ["event", source, op, object, more @ ..],
        ) => Ok(Record::Event {
            upstream: match *source {
                "upstream" => true,
                "sweep" => false,
                _ => return Err(format!("bad source {source}")),
            },
            object: (*object).to_owned(),
            event: event(op, more_keys, more)?,
        }),
        _ => Err(format!("bad record: {line}")),
    }
}

fn check(record: &Record) -> Result<(), String> {
    let Record::Event { event, .. } = record else {
        return Ok(());
    };
    match event {
        Event::Create { requested, size } if *requested % 65536 != usize::from(*size) => {
            Err(format!("{requested} isn't size {size} in 16 bits"))
        }
        Event::Add { rc, .. } if !matches!(rc, 0 | 1) => Err(format!("bad add result {rc}")),
        Event::Add { rc: 1, text } if text.len() < 240 => {
            Err("reported a short text as truncated".to_owned())
        }
        Event::Add { rc: 0, text } if text.len() >= 240 => {
            Err("didn't report a long text as truncated".to_owned())
        }
        Event::Add { text, .. } if text.contains(&0) => Err("text holds a NUL".to_owned()),
        Event::State {
            elements, empty, ..
        } if (*elements == 0) != *empty => Err("empty disagrees with elements".to_owned()),
        Event::State { valid, .. } if valid.is_empty() => Err("no valid flags".to_owned()),
        Event::Nth {
            text: Some(text), ..
        } if text.len() >= 240 => Err("stored text is too long".to_owned()),
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"strrb-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
        .filter(|record| matches!(record, Record::Event { upstream: true, .. }))
        .count();
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
        replace("strrb-v1", "strrb-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"constant""#, r#""kind":"other""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""op":"nth""#, r#""op":"add""#),
        replace(r#""requested":3,"size":3"#, r#""requested":3,"size":4"#),
        replace(r#""requested":3,"size":3"#, r#""requested":03,"size":3"#),
        replace(
            r#""text":"68656c6c6f","rc":0"#,
            r#""text":"68656c6c6f","rc":1"#,
        ),
        replace(
            r#""text":"68656c6c6f","rc":0"#,
            r#""text":"68656c6c6","rc":0"#,
        ),
        replace(
            r#""text":"68656c6c6f","rc":0"#,
            r#""text":"68006c6c6f","rc":0"#,
        ),
        replace(r#""elements":0,"empty":1"#, r#""elements":0,"empty":0"#),
        replace(r#""empty":1,"valid":"00""#, r#""empty":1,"valid":"02""#),
        replace(r#""empty":1,"valid":"00""#, r#""empty":1,"valid":"""#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
