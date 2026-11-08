//! Shared fixture reader for the constants in gsmtap.h. Checks the shape of the C
//! observations; the expected values come only from the recording.

pub const FINGERPRINT: u64 = 0x0a091a7a4fbd38f7;
pub const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
pub const CONSTANT_COUNT: usize = 44;

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn constant(line: &str) -> Result<(String, u16), String> {
    let rest = line
        .strip_prefix(r#"{"kind":"constant","name":""#)
        .and_then(|rest| rest.strip_suffix('}'))
        .ok_or_else(|| format!("bad record: {line}"))?;
    let (name, value) = rest
        .split_once(r#"","value":"#)
        .ok_or_else(|| format!("bad record: {line}"))?;
    if !name.starts_with("GSMTAP_")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!("bad name {name}"));
    }
    let number: u16 = value.parse().map_err(|_| format!("bad value {value}"))?;
    if number.to_string() != value {
        return Err(format!("non-canonical value {value}"));
    }
    Ok((name.to_owned(), number))
}

/// The constants in file order, as names and values.
pub fn parse(text: &str) -> Result<Vec<(String, u16)>, String> {
    let mut lines = text.lines();
    let metadata = format!(r#"{{"schema":"gsmtap_constants-v1","libosmocore_commit":"{COMMIT}"}}"#);
    if lines.next() != Some(metadata.as_str()) {
        return Err("metadata must name the schema and pinned revision".to_owned());
    }
    let constants = lines.map(constant).collect::<Result<Vec<_>, _>>()?;
    if constants.len() != CONSTANT_COUNT {
        return Err(format!("expected {CONSTANT_COUNT} constants"));
    }
    for (index, (name, _)) in constants.iter().enumerate() {
        if constants[..index]
            .iter()
            .any(|(earlier, _)| earlier == name)
        {
            return Err(format!("{name} appears twice"));
        }
    }
    Ok(constants)
}

pub fn validate(text: &str) -> Vec<(String, u16)> {
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
        replace("gsmtap_constants-v1", "gsmtap_constants-v2"),
        replace(COMMIT, "0000000000000000000000000000000000000000"),
        replace(r#""kind":"constant""#, r#""kind":"other""#),
        replace(
            r#""GSMTAP_UDP_PORT","value":4729"#,
            r#""GSMTAP_UDP_PORT","value":04729"#,
        ),
        replace(
            r#""GSMTAP_UDP_PORT","value":4729"#,
            r#""GSMTAP_UDP_PORT","value":-1"#,
        ),
        replace(r#""name":"GSMTAP_UDP_PORT""#, r#""name":"gsmtap_udp_port""#),
        replace(r#""name":"GSMTAP_TYPE_ABIS""#, r#""name":"GSMTAP_TYPE_UM""#),
        truncated,
        extended,
    ] {
        assert!(parse(&corrupt).is_err(), "accepted a corrupted fixture");
    }
}
