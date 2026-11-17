//! Shared fixture reader for the osmo-auc-gen utility: one record per run of the C
//! program, with its arguments, exit status, stdout and stderr. Checks the shape of the C
//! observations; the expected results come only from the recording.

pub const FINGERPRINT: u64 = 0x0fa748fb61715b8b;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
/// Record counts in file order: the upstream test script's commands, then the harness's.
pub const SECTIONS: &[(&str, usize)] = &[("upstream", 19), ("harness", 118)];

/// One run of the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub source: String,
    pub name: String,
    /// The arguments after the program name.
    pub args: Vec<String>,
    pub status: u8,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    /// Stdout after the banner: the banner is every line before the first empty line, and
    /// the body starts after that empty line.
    pub fn body(&self) -> &str {
        split_banner(&self.stdout).map_or("", |(_, body)| body)
    }
}

/// Splits stdout into its banner and the rest, at the first empty line.
pub fn split_banner(stdout: &str) -> Option<(&str, &str)> {
    let at = stdout.find("\n\n")?;
    Some((&stdout[..at + 1], &stdout[at + 2..]))
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

/// Reads one JSON string from the start of `raw`, returning it and the rest. Only the
/// escapes the harness writes are accepted: `\"`, `\\`, `\n`, `\t` and `\u00XX` for other
/// control characters.
fn string(raw: &str) -> Result<(String, &str), String> {
    let mut chars = raw
        .strip_prefix('"')
        .ok_or_else(|| format!("not a string: {raw}"))?
        .char_indices();
    let mut text = String::new();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Ok((text, &raw[at + 2..])),
            '\\' => match chars.next().map(|(_, c)| c) {
                Some('"') => text.push('"'),
                Some('\\') => text.push('\\'),
                Some('n') => text.push('\n'),
                Some('t') => text.push('\t'),
                Some('u') => {
                    let hex: String = (0..4)
                        .filter_map(|_| chars.next().map(|(_, c)| c))
                        .collect();
                    let code = (hex.len() == 4 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                        .then(|| u8::from_str_radix(&hex, 16).ok())
                        .flatten()
                        .filter(|&code| code < 0x20 && !matches!(code, b'\n' | b'\t'))
                        .ok_or_else(|| format!("bad escape \\u{hex}"))?;
                    text.push(char::from(code));
                }
                other => return Err(format!("bad escape {other:?}")),
            },
            c if c.is_ascii_control() || !c.is_ascii() => {
                return Err(format!("unescaped character {c:?}"));
            }
            c => text.push(c),
        }
    }
    Err(format!("unterminated string: {raw}"))
}

fn expect<'a>(rest: &'a str, prefix: &str, line: &str) -> Result<&'a str, String> {
    rest.strip_prefix(prefix)
        .ok_or_else(|| format!("expected {prefix} in {line}"))
}

fn record(line: &str) -> Result<Run, String> {
    let rest = expect(line, r#"{"kind":"run","source":"#, line)?;
    let (source, rest) = string(rest)?;
    let rest = expect(rest, r#","name":"#, line)?;
    let (name, rest) = string(rest)?;
    let mut rest = expect(rest, r#","args":["#, line)?;
    let mut args = Vec::new();
    while !rest.starts_with(']') {
        if !args.is_empty() {
            rest = expect(rest, ",", line)?;
        }
        let (arg, after) = string(rest)?;
        args.push(arg);
        rest = after;
    }
    let rest = expect(rest, r#"],"status":"#, line)?;
    let end = rest
        .find(',')
        .ok_or_else(|| format!("no status in {line}"))?;
    let status: u8 = rest[..end]
        .parse()
        .map_err(|_| format!("bad status {}", &rest[..end]))?;
    if status.to_string() != rest[..end] {
        return Err(format!("non-canonical status {}", &rest[..end]));
    }
    let rest = expect(&rest[end..], r#","stdout":"#, line)?;
    let (stdout, rest) = string(rest)?;
    let rest = expect(rest, r#","stderr":"#, line)?;
    let (stderr, rest) = string(rest)?;
    if rest != "}" {
        return Err(format!("unexpected {rest} in {line}"));
    }
    Ok(Run {
        source,
        name,
        args,
        status,
        stdout,
        stderr,
    })
}

/// A kebab-case name, an exit status the program uses, and a banner like every other
/// record's.
fn check(run: &Run, banner: &str) -> Result<(), String> {
    let name_ok = !run.name.is_empty()
        && !run.name.starts_with('-')
        && !run.name.ends_with('-')
        && run
            .name
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'));
    if !name_ok {
        return Err(format!("bad name {}", run.name));
    }
    if run.status > 2 {
        return Err(format!("{}: unexpected status {}", run.name, run.status));
    }
    match split_banner(&run.stdout) {
        Some((first, _)) if first == banner => Ok(()),
        _ => Err(format!(
            "{}: stdout doesn't start with the banner",
            run.name
        )),
    }
}

pub fn parse(text: &str) -> Result<Vec<Run>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"osmo-auc-gen-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let runs = lines.map(record).collect::<Result<Vec<_>, _>>()?;
    let mut remaining = runs.as_slice();
    for (source, count) in SECTIONS {
        if remaining.len() < *count || remaining[..*count].iter().any(|r| r.source != *source) {
            return Err(format!("expected {count} {source} records here"));
        }
        remaining = &remaining[*count..];
    }
    if !remaining.is_empty() {
        return Err(format!(
            "{} records after the last section",
            remaining.len()
        ));
    }
    let banner = runs
        .first()
        .and_then(|run| split_banner(&run.stdout))
        .map(|(banner, _)| banner)
        .filter(|banner| !banner.is_empty())
        .ok_or("the first record has no banner")?;
    for (at, run) in runs.iter().enumerate() {
        check(run, banner)?;
        if runs[..at].iter().any(|other| other.name == run.name) {
            return Err(format!("duplicate name {}", run.name));
        }
    }
    Ok(runs)
}

pub fn validate(text: &str) -> Vec<Run> {
    assert_eq!(fingerprint(text), FINGERPRINT, "C observations changed");
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
        replace("osmo-auc-gen-v1", "osmo-auc-gen-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"run""#, r#""kind":"other""#),
        replace(r#""source":"upstream""#, r#""source":"harness""#),
        replace(r#""source":"harness""#, r#""source":"upstream""#),
        replace(r#""name":"sqn-1""#, r#""name":"sqn-0""#),
        replace(r#""name":"sqn-1""#, r#""name":"Sqn-1""#),
        replace(r#""name":"sqn-1""#, r#""name":"""#),
        replace(r#""status":2,"#, r#""status":3,"#),
        replace(r#""status":2,"#, r#""status":02,"#),
        replace(r#""status":0,"#, r#""status":256,"#),
        replace(r#"\t"#, r#"\r"#),
        replace(r#"\t"#, "\t"),
        replace(r#"\n"#, r#"\u000a"#),
        replace(r#""stdout":"osmo-auc-gen"#, r#""stdout":"osmo-auc-gem"#),
        replace(r#""args":["-3","#, r#""args":[-3,"#),
        replace(r#""args":["-3","#, r#""args":["-3""#),
        replace(r#""stderr":""}"#, r#""stderr":"","extra":1}"#),
        replace(r#""stderr":""}"#, r#""stderr":""#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
