//! Shared fixture reader for soft-bit conversions. Checks the shape of the C observations;
//! the expected outputs come only from the recording.

pub const FINGERPRINT: u64 = 0x82aeb45876a3ab8c;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";

/// What C gives for one input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conversion {
    pub input: i16,
    pub output: i16,
}

/// `osmo_ubit2sbit` on every byte value, then `osmo_sbit2ubit` on every soft bit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftBits {
    pub ubit2sbit: Vec<Conversion>,
    pub sbit2ubit: Vec<Conversion>,
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn number(value: &str) -> Result<i16, String> {
    let parsed: i16 = value.parse().map_err(|_| format!("bad number {value}"))?;
    if parsed.to_string() == value {
        Ok(parsed)
    } else {
        Err(format!("non-canonical number {value}"))
    }
}

fn record(line: &str, kind: &str) -> Result<Conversion, String> {
    let rest = line
        .strip_prefix(&format!(r#"{{"kind":"{kind}","input":"#))
        .and_then(|rest| rest.strip_suffix('}'))
        .ok_or_else(|| format!("expected a {kind} record: {line}"))?;
    let (input, output) = rest
        .split_once(r#","output":"#)
        .ok_or_else(|| format!("bad record: {line}"))?;
    Ok(Conversion {
        input: number(input)?,
        output: number(output)?,
    })
}

pub fn parse(text: &str) -> Result<SoftBits, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"soft_bits-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let lines: Vec<&str> = lines.collect();
    if lines.len() != 512 {
        return Err("expected 256 records of each kind".to_owned());
    }
    let ubit2sbit = lines[..256]
        .iter()
        .map(|line| record(line, "ubit2sbit"))
        .collect::<Result<Vec<_>, _>>()?;
    let sbit2ubit = lines[256..]
        .iter()
        .map(|line| record(line, "sbit2ubit"))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, conversion) in ubit2sbit.iter().enumerate() {
        if conversion.input != index as i16 || !matches!(conversion.output, -127 | 127) {
            return Err(format!("bad ubit2sbit record {conversion:?}"));
        }
    }
    for (index, conversion) in sbit2ubit.iter().enumerate() {
        if conversion.input != index as i16 - 128 || !matches!(conversion.output, 0 | 1) {
            return Err(format!("bad sbit2ubit record {conversion:?}"));
        }
    }
    Ok(SoftBits {
        ubit2sbit,
        sbit2ubit,
    })
}

pub fn validate(text: &str) -> SoftBits {
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
        replace("soft_bits-v1", "soft_bits-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(
            r#""kind":"ubit2sbit","input":0,"#,
            r#""kind":"sbit2ubit","input":0,"#,
        ),
        replace(r#""input":0,"output":127"#, r#""input":0,"output":126"#),
        replace(r#""input":0,"output":127"#, r#""input":00,"output":127"#),
        replace(r#""input":1,"output":-127"#, r#""input":2,"output":-127"#),
        replace(r#""input":-1,"output":1"#, r#""input":-1,"output":2"#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
