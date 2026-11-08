// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
fn c_hex(data: &[u8]) -> String {
    hex(data, 1365)
}
fn hex(data: &[u8], limit: usize) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for byte in data.iter().take(limit) {
        write!(out, "{byte:02x} ").expect("String write");
    }
    out
}
fn dump_segments(msg: &Msgb, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = msg.data;
    for (i, offset) in msg.layers.iter().enumerate() {
        let Some(offset) = *offset else { continue };
        if offset > msg.tail {
            continue;
        }
        if offset < msg.data {
            out.push(format!(
                "(L{}=data{:+}) ",
                i + 1,
                offset as i64 - msg.data as i64
            ));
        } else if offset < start {
            out.push(format!("(L{}{:+}) ", i + 1, start - offset));
        } else {
            out.push(format!(
                "{}[L{}]> ",
                hex(&msg.backing[start..offset], limit),
                i + 1
            ));
            start = offset;
        }
    }
    out.push(hex(&msg.backing[start..msg.tail], limit));
    for (i, offset) in msg.layers.iter().enumerate() {
        if let Some(offset) = *offset
            && offset > msg.tail
        {
            out.push(format!("(L{}=tail{:+}) ", i + 1, offset - msg.tail));
        }
    }
    out
}
/// Write a NUL-terminated dump, returning false for C's `"ERROR"` result.
/// Truncated output and untouched suffix bytes match defined C calls. Rust
/// also bounds-checks annotation writes for which C can overrun a short buffer.
pub fn msgb_hexdump_buf(buf: &mut [u8], msg: &Msgb) -> bool {
    let mut offset = 0;
    for segment in dump_segments(msg, 1365) {
        let remaining = buf.len() - offset;
        if remaining == 0 {
            return false;
        }
        let count = segment.len().min(remaining - 1);
        buf[offset..offset + count].copy_from_slice(&segment.as_bytes()[..count]);
        buf[offset + count] = 0;
        if segment.len() >= remaining {
            return false;
        }
        offset += count;
    }
    true
}
/// Owned result with C's 4100-byte static-buffer limit; returns `"ERROR"` on overflow.
pub fn msgb_hexdump(msg: &Msgb) -> String {
    let mut buf = vec![0; 4100];
    dump_string(msg, &mut buf)
}
/// Owned equivalent of `msgb_hexdump_c`, without a talloc context.
pub fn msgb_hexdump_owned(msg: &Msgb) -> String {
    let mut buf = vec![0; usize::from(msgb_length(msg)) * 3 + 100];
    dump_string(msg, &mut buf)
}
fn dump_string(msg: &Msgb, buf: &mut [u8]) -> String {
    if !msgb_hexdump_buf(buf, msg) {
        return "ERROR".into();
    }
    let end = buf.iter().position(|b| *b == 0).expect("terminated dump");
    String::from_utf8(buf[..end].to_vec()).expect("ASCII dump")
}
/// Dump a layer: unset or empty is `"[]"`; beyond-tail offsets are errors.
pub fn msgb_hexdump_layer(msg: &Msgb, layer: usize) -> Result<String, MsgbError> {
    if !(1..=4).contains(&layer) {
        return Err(MsgbError::InvalidLayer);
    }
    if msg.layers[layer - 1].is_none() {
        return Ok("[]".into());
    }
    let data = comparison_data(msg, layer)?;
    Ok(if data.is_empty() {
        "[]".into()
    } else {
        c_hex(data)
    })
}

pub fn msgb_hexdump_l1(msg: &Msgb) -> Result<String, MsgbError> {
    msgb_hexdump_layer(msg, 1)
}

pub fn msgb_hexdump_l2(msg: &Msgb) -> Result<String, MsgbError> {
    msgb_hexdump_layer(msg, 2)
}

pub fn msgb_hexdump_l3(msg: &Msgb) -> Result<String, MsgbError> {
    msgb_hexdump_layer(msg, 3)
}

pub fn msgb_hexdump_l4(msg: &Msgb) -> Result<String, MsgbError> {
    msgb_hexdump_layer(msg, 4)
}

/// Append already formatted UTF-8 bytes with C `msgb_printf` storage behavior.
/// A spare NUL byte is required and excluded from message length. On failure,
/// metadata stays unchanged but available tail bytes receive truncated output
/// and a NUL. Embedded NUL bytes are included in the message length.
pub fn msgb_append_str(msg: &mut Msgb, text: &str) -> Result<(), MsgbError> {
    let room = msg.backing.len() - msg.tail;
    if room == 0 {
        return Err(MsgbError::NotEnoughTailroom);
    }
    let count = text.len().min(room - 1);
    msg.backing[msg.tail..msg.tail + count].copy_from_slice(&text.as_bytes()[..count]);
    msg.backing[msg.tail + count] = 0;
    if text.len() >= room {
        return Err(MsgbError::NotEnoughTailroom);
    }
    msg.tail += text.len();
    Ok(())
}
/// Rust `format_args!` adapter. Format syntax is Rust's, not C variadic printf.
pub fn msgb_printf(msg: &mut Msgb, args: std::fmt::Arguments<'_>) -> Result<(), MsgbError> {
    msgb_append_str(msg, &std::fmt::format(args))
}

impl Msgb {
    /// Owned dump without the C static-buffer size limit.
    pub fn hexdump(&self) -> String {
        dump_segments(self, usize::MAX).concat()
    }
    /// See [`crate::compat::msgb::msgb_hexdump_layer`] for mutation and error details.
    pub fn hexdump_layer(&self, layer: usize) -> Result<String, MsgbError> {
        msgb_hexdump_layer(self, layer)
    }
    /// See [`crate::compat::msgb::msgb_hexdump_l1`] for mutation and error details.
    pub fn hexdump_l1(&self) -> Result<String, MsgbError> {
        msgb_hexdump_l1(self)
    }
    /// See [`crate::compat::msgb::msgb_hexdump_l2`] for mutation and error details.
    pub fn hexdump_l2(&self) -> Result<String, MsgbError> {
        msgb_hexdump_l2(self)
    }
    /// See [`crate::compat::msgb::msgb_hexdump_l3`] for mutation and error details.
    pub fn hexdump_l3(&self) -> Result<String, MsgbError> {
        msgb_hexdump_l3(self)
    }
    /// See [`crate::compat::msgb::msgb_hexdump_l4`] for mutation and error details.
    pub fn hexdump_l4(&self) -> Result<String, MsgbError> {
        msgb_hexdump_l4(self)
    }
    /// See [`crate::compat::msgb::msgb_append_str`] for mutation and error details.
    pub fn append_str(&mut self, text: &str) -> Result<(), MsgbError> {
        msgb_append_str(self, text)
    }
}
