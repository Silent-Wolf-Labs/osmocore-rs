// SPDX-License-Identifier: GPL-3.0-or-later

use crate::gsm::gsm_utils::{
    self, Band, Direction, TlliType, cm2_supports_a5, cm3_supports_a5, foreign_tlli, local_tlli,
};
use crate::gsm::gsm0502::{TDMA_HYPERFRAME, TDMA_SUPERFRAME};

const EINVAL_RESULT: i32 = -22;
const NO_FREQUENCY: u16 = 0xffff;
const NO_ARFCN: u16 = 0xffff;
const NO_TMSI: u32 = 0xffff_ffff;

fn direction(uplink: i32) -> Direction {
    if uplink != 0 {
        Direction::Uplink
    } else {
        Direction::Downlink
    }
}

/// Returns a band's name, or `"invalid"` for a value that names no band.
pub fn gsm_band_name(band: i32) -> &'static str {
    Band::from_code(band).map_or("invalid", Band::name)
}

/// Returns the C band value named in `text`, or -22.
pub fn gsm_band_parse(text: &str) -> i32 {
    Band::parse(text).map_or(EINVAL_RESULT, Band::code)
}

/// Returns a power class's power in dBm, or -22 for an unknown band or class.
pub fn ms_class_gmsk_dbm(band: i32, class: i32) -> i32 {
    let class = u8::try_from(class).ok();
    Band::from_code(band)
        .zip(class)
        .and_then(|(band, class)| band.ms_class_dbm(class))
        .unwrap_or(EINVAL_RESULT)
}

/// Returns the power control level for `dbm`, or -22 for an unknown band.
pub fn ms_pwr_ctl_lvl(band: i32, dbm: u32) -> i32 {
    Band::from_code(band).map_or(EINVAL_RESULT, |band| i32::from(band.ms_power_level(dbm)))
}

/// Returns a power control level's power in dBm, or -22 for an unknown band or a
/// reserved level.
pub fn ms_pwr_dbm(band: i32, level: u8) -> i32 {
    Band::from_code(band)
        .and_then(|band| band.ms_power_dbm(level))
        .unwrap_or(EINVAL_RESULT)
}

/// Stores an ARFCN's C band value in `band` and returns 0, or returns -1 and leaves
/// `band` alone.
pub fn gsm_arfcn2band_rc(arfcn: u16, band: &mut i32) -> i32 {
    match Band::from_arfcn(arfcn) {
        Some(found) => {
            *band = found.code();
            0
        }
        None => -1,
    }
}

/// Returns an ARFCN's frequency, or 0xffff. Any nonzero `uplink` means uplink.
pub fn gsm_arfcn2freq10(arfcn: u16, uplink: i32) -> u16 {
    gsm_utils::arfcn_to_freq10(arfcn, direction(uplink)).unwrap_or(NO_FREQUENCY)
}

/// Returns a frequency's ARFCN, or 0xffff. Any nonzero `uplink` means uplink.
pub fn gsm_freq102arfcn(freq10: u16, uplink: i32) -> u16 {
    gsm_utils::freq10_to_arfcn(freq10, direction(uplink)).unwrap_or(NO_ARFCN)
}

/// C's `struct gsm_time`, whose fields don't have to agree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GsmTimeFields {
    /// Frame number.
    pub frame: u32,
    /// Superframe count.
    pub t1: u16,
    /// Frame number modulo 26.
    pub t2: u8,
    /// Frame number modulo 51.
    pub t3: u8,
    /// 51-multiframe count modulo 8.
    pub tc: u8,
}

/// Splits a frame number into fields. `frame` isn't reduced, and `t1` truncates for
/// frames far past the hyperframe.
pub const fn gsm_fn2gsmtime(frame: u32) -> GsmTimeFields {
    GsmTimeFields {
        frame,
        t1: (frame / TDMA_SUPERFRAME) as u16,
        t2: (frame % 26) as u8,
        t3: (frame % 51) as u8,
        tc: ((frame / 51) % 8) as u8,
    }
}

/// Returns the frame number of `t1`, `t2` and `t3`, ignoring the other fields and
/// reducing the result to a hyperframe. Out-of-range fields give C's result: when
/// `t3 - t2` is a negative multiple of 26, the C modulo gives 26 rather than 0.
pub const fn gsm_gsmtime2fn(time: &GsmTimeFields) -> u32 {
    let difference = time.t3 as i32 - time.t2 as i32;
    let cycle = if difference < 0 {
        difference % 26 + 26
    } else {
        difference % 26
    };
    (51 * cycle as u32 + time.t3 as u32 + TDMA_SUPERFRAME * time.t1 as u32) % TDMA_HYPERFRAME
}

/// Formats the fields like [`GsmTime`](crate::gsm::gsm_utils::GsmTime). `tc` isn't shown.
pub fn osmo_dump_gsmtime(time: &GsmTimeFields) -> String {
    let mut text = String::new();
    gsm_utils::format_time(&mut text, time.frame, time.t1, time.t2, time.t3)
        .expect("writing to a String succeeds");
    text
}

/// Formats a frame number's fields. Unlike
/// [`GsmTime`](crate::gsm::gsm_utils::GsmTime), this accepts any frame number.
pub fn gsm_fn_as_gsmtime_str(frame: u32) -> String {
    osmo_dump_gsmtime(&gsm_fn2gsmtime(frame))
}

/// Returns a TLLI's type as a C `enum gprs_tlli_type` value.
pub const fn gprs_tlli_type(tlli: u32) -> i32 {
    match TlliType::of(tlli) {
        TlliType::Local => 0,
        TlliType::Foreign => 1,
        TlliType::Random => 2,
        TlliType::Auxiliary => 3,
        TlliType::Reserved => 4,
        TlliType::GRnti => 5,
        TlliType::RandomGRnti => 6,
    }
}

/// Returns the P-TMSI in a local or foreign TLLI, or 0xffffffff. A valid local TLLI
/// of 0xffffffff also returns that value.
pub const fn gprs_tlli2tmsi(tlli: u32) -> u32 {
    match gsm_utils::tlli_to_tmsi(tlli) {
        Some(tmsi) => tmsi,
        None => NO_TMSI,
    }
}

/// Returns the TLLI of a C type value, or 0 for anything but local and foreign.
pub const fn gprs_tmsi2tlli(p_tmsi: u32, tlli_type: i32) -> u32 {
    match tlli_type {
        0 => local_tlli(p_tmsi),
        1 => foreign_tlli(p_tmsi),
        _ => 0,
    }
}

/// Returns 1 if Classmark 2 says A5/`n` is supported, otherwise 0.
pub const fn ms_cm2_a5n_support(classmark: &[u8; 3], n: u32) -> i32 {
    cm2_supports_a5(classmark, n) as i32
}

/// Returns 1 if the first octet of Classmark 3 says A5/`n` is supported, otherwise 0.
pub const fn ms_cm3_a5n_support(first_octet: u8, n: u32) -> i32 {
    cm3_supports_a5(first_octet, n) as i32
}

/// Returns the RACH code for a retransmission count held in an `int`.
pub const fn rach_max_trans_val2raw(value: i32) -> i32 {
    (value >> 1) & 3
}

/// Returns the retransmission count of a RACH code held in an `int`, using its low
/// two bits.
pub const fn rach_max_trans_raw2val(raw: i32) -> i32 {
    gsm_utils::rach_max_trans_from_raw((raw & 3) as u8) as i32
}

/// Copies `bytes` to the start of `result` and returns their count, or returns -22
/// and leaves `result` alone if they don't fit.
fn copy_counted(result: &mut [u8], bytes: &[u8]) -> i32 {
    match result.get_mut(..bytes.len()) {
        Some(target) => {
            target.copy_from_slice(bytes);
            bytes.len() as i32
        }
        None => EINVAL_RESULT,
    }
}

/// C sees text only up to its first NUL.
fn c_string(data: &[u8]) -> &[u8] {
    data.split(|&byte| byte == 0).next().unwrap_or(data)
}

/// Returns the octets needed to pack `septets` septets.
pub const fn gsm_get_octet_len(septets: u8) -> u8 {
    gsm_utils::septets_to_octets(septets as usize) as u8
}

/// Writes the septets of `data`, up to its first NUL, to `result` and returns their
/// count. Returns -22 if they don't fit, where C would overrun.
pub fn gsm_septet_encode(result: &mut [u8], data: &[u8]) -> i32 {
    copy_counted(result, &gsm_utils::encode_septets(c_string(data)))
}

/// Writes packed septets to `result` and returns the octet count. Returns -22 if they
/// don't fit or `padding` is above 7, where C overruns or shifts by a negative amount.
pub fn gsm_septet_pack(result: &mut [u8], septets: &[u8], padding: u8) -> i32 {
    gsm_utils::pack_septets(septets, padding)
        .map_or(EINVAL_RESULT, |octets| copy_counted(result, &octets))
}

fn write_packed(result: &mut [u8], packed: &gsm_utils::Packed7bit) -> (i32, i32) {
    result[..packed.octets.len()].copy_from_slice(&packed.octets);
    (packed.septets as i32, packed.octets.len() as i32)
}

/// Encodes `data`, up to its first NUL, into `result`, whose length is C's `n`. Returns
/// the septet and octet counts.
pub fn gsm_7bit_encode_n(result: &mut [u8], data: &[u8]) -> (i32, i32) {
    let packed = gsm_utils::encode_7bit(c_string(data), result.len());
    write_packed(result, &packed)
}

/// Encodes `data`, up to its first NUL, for USSD into `result`, whose length is C's
/// `n`. Returns the septet and octet counts.
pub fn gsm_7bit_encode_n_ussd(result: &mut [u8], data: &[u8]) -> (i32, i32) {
    let packed = gsm_utils::encode_7bit_ussd(c_string(data), result.len());
    write_packed(result, &packed)
}

/// Writes decoded text and a NUL to `text` and returns the text's length.
fn write_text(text: &mut [u8], decoded: &[u8]) -> i32 {
    text[..decoded.len()].copy_from_slice(decoded);
    text[decoded.len()] = 0;
    decoded.len() as i32
}

/// Decodes `length` septets into `text`, whose length is C's `n`, adding a NUL. Returns
/// the text's length. An empty `text` returns 0, where C aborts. Octets past the end of
/// `user_data`, which C would read anyway, count as zero.
pub fn gsm_7bit_decode_n(text: &mut [u8], user_data: &[u8], length: u8) -> i32 {
    gsm_7bit_decode_n_hdr(text, user_data, length, 0)
}

/// Decodes like [`gsm_7bit_decode_n`], skipping a user data header if `ud_hdr_ind` isn't
/// zero. As in C, a header longer than the message makes the septet count wrap
/// around in 8 bits, and decoding then reads on past the octets `length` implies.
pub fn gsm_7bit_decode_n_hdr(text: &mut [u8], user_data: &[u8], length: u8, ud_hdr_ind: u8) -> i32 {
    let Some(limit) = text.len().checked_sub(1) else {
        return 0;
    };
    let octets = gsm_utils::septets_to_octets(usize::from(length));
    let first = match ud_hdr_ind {
        0 => 0,
        _ => gsm_utils::header_septets(user_data.first().copied().unwrap_or(0)),
    };
    let count = length.wrapping_sub(first as u8);
    let decoded = gsm_utils::unpack_text(user_data, first, usize::from(count), octets, limit);
    write_text(text, &decoded)
}

/// Decodes like [`gsm_7bit_decode_n`], then drops the last byte if the top seven bits
/// of the last octet hold `<CR>`.
pub fn gsm_7bit_decode_n_ussd(text: &mut [u8], user_data: &[u8], length: u8) -> i32 {
    let count = gsm_7bit_decode_n(text, user_data, length);
    let last = usize::from(gsm_get_octet_len(length)).wrapping_sub(1);
    if count > 0 && user_data.get(last).is_some_and(|octet| octet >> 1 == b'\r') {
        text[count as usize - 1] = 0;
        count - 1
    } else {
        count
    }
}
