//! Shared fixture reader for use_count. Checks the shape of the C observations; the
//! expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x5cb729c9e74d66a7;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order. Each kind has one contiguous section.
pub const SECTIONS: &[(&str, usize)] = &[("event", 2340), ("no_object", 1)];
pub const UPSTREAM_COUNT: usize = 115;
pub const POLICIES: &[&str] = &["none", "ok", "fail", "clamp", "free_zero", "foo"];
pub const RESULTS: &[i32] = &[0, -5, -34];

/// An object's state after an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub total: i32,
    pub text: String,
    /// Every entry in list order, as `token=count;`, with `<null>` for a NULL token.
    pub entries: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    New {
        policy: String,
        state: State,
    },
    Call {
        token: Option<String>,
        change: i32,
    },
    Callback {
        token: Option<String>,
        old: i32,
        new: i32,
        state: State,
    },
    Result {
        rc: i32,
        by: i32,
        state: State,
    },
    Static {
        n: usize,
        state: State,
    },
    Free {
        token: Option<String>,
        state: State,
    },
    ToStr {
        n: usize,
        rc: i32,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Event {
        upstream: bool,
        object: String,
        event: Event,
    },
    NoObject {
        rc: i32,
        total: i32,
        by: i32,
    },
}

impl Record {
    fn kind(&self) -> &'static str {
        match self {
            Self::Event { .. } => "event",
            Self::NoObject { .. } => "no_object",
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

fn hex_text(value: &str) -> Result<String, String> {
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(format!("bad hex {value}")),
    };
    if !value.len().is_multiple_of(2) {
        return Err(format!("odd hex {value}"));
    }
    let bytes = value
        .as_bytes()
        .chunks(2)
        .map(|pair| Ok(digit(pair[0])? * 16 + digit(pair[1])?))
        .collect::<Result<Vec<u8>, String>>()?;
    String::from_utf8(bytes).map_err(|_| format!("not text {value}"))
}

fn token(value: &str) -> Result<Option<String>, String> {
    match value {
        "null" => Ok(None),
        _ => hex_text(value).map(Some),
    }
}

fn state(total: &str, text: &str, entries: &str) -> Result<State, String> {
    Ok(State {
        total: parsed(total)?,
        text: hex_text(text)?,
        entries: hex_text(entries)?,
    })
}

fn event(op: &str, keys: &[&str], values: &[&str]) -> Result<Event, String> {
    match (op, keys, values) {
        ("new", ["policy", "total", "text", "entries"], [policy, total, text, entries]) => {
            Ok(Event::New {
                policy: (*policy).to_owned(),
                state: state(total, text, entries)?,
            })
        }
        ("call", ["use", "change"], [token_text, change]) => Ok(Event::Call {
            token: token(token_text)?,
            change: parsed(change)?,
        }),
        (
            "callback",
            ["use", "old", "new", "total", "text", "entries"],
            [token_text, old, new, total, text, entries],
        ) => Ok(Event::Callback {
            token: token(token_text)?,
            old: parsed(old)?,
            new: parsed(new)?,
            state: state(total, text, entries)?,
        }),
        ("result", ["rc", "by", "total", "text", "entries"], [rc, by, total, text, entries]) => {
            Ok(Event::Result {
                rc: parsed(rc)?,
                by: parsed(by)?,
                state: state(total, text, entries)?,
            })
        }
        ("static", ["n", "total", "text", "entries"], [n, total, text, entries]) => {
            Ok(Event::Static {
                n: parsed(n)?,
                state: state(total, text, entries)?,
            })
        }
        ("free", ["use", "total", "text", "entries"], [token_text, total, text, entries]) => {
            Ok(Event::Free {
                token: token(token_text)?,
                state: state(total, text, entries)?,
            })
        }
        ("to_str", ["n", "rc", "text"], [n, rc, text]) => Ok(Event::ToStr {
            n: parsed(n)?,
            rc: parsed(rc)?,
            text: hex_text(text)?,
        }),
        _ => Err(format!("bad {op} event")),
    }
}

fn record(line: &str) -> Result<Record, String> {
    let fields = fields(line)?;
    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = fields.iter().map(|(_, value)| *value).collect();
    match (keys.as_slice(), values.as_slice()) {
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
        (["kind", "rc", "total", "by"], ["no_object", rc, total, by]) => Ok(Record::NoObject {
            rc: parsed(rc)?,
            total: parsed(total)?,
            by: parsed(by)?,
        }),
        _ => Err(format!("bad record: {line}")),
    }
}

fn check(record: &Record) -> Result<(), String> {
    match record {
        Record::Event {
            event: Event::New { policy, .. },
            ..
        } if !POLICIES.contains(&policy.as_str()) => Err(format!("bad policy {policy}")),
        Record::Event {
            event: Event::Result { rc, .. },
            ..
        } if !RESULTS.contains(rc) => Err(format!("bad result {rc}")),
        Record::Event {
            event: Event::ToStr { n, rc, text },
            ..
        } if text.len() >= (*n).max(1)
            || usize::try_from(*rc).map_or(true, |rc| rc < text.len()) =>
        {
            Err("bad text size".to_owned())
        }
        Record::NoObject { rc, total, by } if (*rc, *total, *by) != (-22, 0, 0) => {
            Err("bad result with no object".to_owned())
        }
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"use-count-v1","libosmocore_commit":"{COMMIT}"}}"#);
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
    let count = |wanted: fn(&Record) -> bool| records.iter().filter(|r| wanted(r)).count();
    if count(|r| matches!(r, Record::Event { upstream: true, .. })) != UPSTREAM_COUNT {
        return Err(format!("expected {UPSTREAM_COUNT} upstream records"));
    }
    let calls = count(|r| {
        matches!(
            r,
            Record::Event {
                event: Event::Call { .. },
                ..
            }
        )
    });
    let results = count(|r| {
        matches!(
            r,
            Record::Event {
                event: Event::Result { .. },
                ..
            }
        )
    });
    if calls != results {
        return Err(format!("{calls} calls but {results} results"));
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
        replace("use-count-v1", "use-count-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"event""#, r#""kind":"other""#),
        replace(r#""kind":"no_object""#, r#""kind":"event""#),
        replace(r#""source":"upstream""#, r#""source":"sweep""#),
        replace(r#""source":"sweep""#, r#""source":"other""#),
        replace(r#""op":"call""#, r#""op":"cal""#),
        replace(r#""op":"result""#, r#""op":"call""#),
        replace(r#""policy":"foo""#, r#""policy":"bar""#),
        replace(r#""change":1"#, r#""change":01"#),
        replace(r#""use":null"#, r#""use":"zz""#),
        replace(r#""use":null"#, r#""use":"6""#),
        replace(r#""rc":-34"#, r#""rc":-1"#),
        replace(r#""n":0,"rc":"#, r#""n":0,"rc":-"#),
        replace(r#""rc":-22"#, r#""rc":0"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
