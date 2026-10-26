// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent bounded reader shared byte-for-byte with the reference project.

pub const HEADER: &str =
    "# network-establishment-v1: case bound connected nonblocking reuse_address dscp payload_match";
const CASES: [&str; 6] = [
    "udp_bound",
    "udp_configured",
    "invalid_dscp",
    "udp_connected",
    "tcp_listener",
    "tcp_connected",
];
#[derive(Debug, PartialEq, Eq)]
pub struct Observation<'a> {
    pub case: &'a str,
    pub values: Vec<u8>,
}
pub fn parse(input: &str) -> Result<Vec<Observation<'_>>, &'static str> {
    let mut lines = input.lines();
    if lines.next() != Some(HEADER) {
        return Err("header");
    }
    let mut observations = Vec::new();
    for expected in CASES {
        let mut fields = lines.next().ok_or("missing case")?.split('\t');
        let case = fields.next().ok_or("case name")?;
        if case != expected {
            return Err("case coverage/order");
        }
        let values: Vec<_> = fields
            .map(|s| {
                if s.is_empty()
                    || (s.len() > 1 && s.starts_with('0'))
                    || !s.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err("noncanonical number");
                }
                s.parse::<u8>().map_err(|_| "number overflow")
            })
            .collect::<Result<_, _>>()?;
        if case == "invalid_dscp" {
            if values.len() != 1 || values[0] > 1 {
                return Err("rejection fields");
            }
        } else if values.len() != 6
            || values
                .iter()
                .enumerate()
                .any(|(i, v)| *v > if i == 4 { 63 } else { 1 })
        {
            return Err("observation fields");
        }
        observations.push(Observation { case, values });
    }
    if lines.next().is_some() || !input.ends_with('\n') {
        return Err("framing");
    }
    Ok(observations)
}
