// SPDX-License-Identifier: GPL-3.0-or-later

//! C's 3GPP TS 23.003 identity functions, with C's structs, any 16-bit MCC and MNC, and C's
//! status codes. Text is bytes ending at the first NUL, and `None` stands for a NULL
//! pointer.
//!
//! The preferred API is [`crate::gsm::gsm23003`], whose types hold only valid MCCs and
//! MNCs and whose domain parsers take only the form 3GPP TS 23.003 gives.

use std::cmp::Ordering;

/// `GSM23003_IMSI_MIN_DIGITS`.
pub const GSM23003_IMSI_MIN_DIGITS: usize = 6;
/// `GSM23003_IMSI_MAX_DIGITS`.
pub const GSM23003_IMSI_MAX_DIGITS: usize = 15;
/// `GSM23003_MSISDN_MIN_DIGITS`.
pub const GSM23003_MSISDN_MIN_DIGITS: usize = 1;
/// `GSM23003_MSISDN_MAX_DIGITS`.
pub const GSM23003_MSISDN_MAX_DIGITS: usize = 15;
/// `GSM23003_IMEI_NUM_DIGITS`.
pub const GSM23003_IMEI_NUM_DIGITS: usize = 15;
/// `GSM23003_IMEI_NUM_DIGITS_NO_CHK`.
pub const GSM23003_IMEI_NUM_DIGITS_NO_CHK: usize = 14;
/// `GSM23003_HOME_NETWORK_DOMAIN_LEN`: the text's length, without its NUL.
pub const GSM23003_HOME_NETWORK_DOMAIN_LEN: usize = 33;
/// `GSM23003_MME_DOMAIN_LEN`: the longest text with its NUL.
pub const GSM23003_MME_DOMAIN_LEN: usize = 55;

const EINVAL: i32 = 22;
/// `sscanf`'s `EOF`, which the domain parsers return as it is.
const EOF: i32 = -1;

/// `struct osmo_plmn_id`. `mnc_3_digits` matters only for an MNC under 100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoPlmnId {
    pub mcc: u16,
    pub mnc: u16,
    pub mnc_3_digits: bool,
}

/// `struct osmo_location_area_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoLocationAreaId {
    pub plmn: OsmoPlmnId,
    pub lac: u16,
}

/// `struct osmo_routing_area_id`. As in C, its LAI is in the field `lac`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoRoutingAreaId {
    pub lac: OsmoLocationAreaId,
    pub rac: u8,
}

/// `struct osmo_cell_global_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoCellGlobalId {
    pub lai: OsmoLocationAreaId,
    pub cell_identity: u16,
}

/// `struct osmo_cell_global_id_ps`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoCellGlobalIdPs {
    pub rai: OsmoRoutingAreaId,
    pub cell_identity: u16,
}

/// `struct osmo_service_area_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoServiceAreaId {
    pub lai: OsmoLocationAreaId,
    pub sac: u16,
}

/// `struct osmo_rnc_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoRncId {
    pub plmn: OsmoPlmnId,
    pub rnc_id: u16,
}

/// `struct osmo_mme_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoMmeId {
    pub group_id: u16,
    pub code: u8,
}

/// `struct osmo_gummei`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoGummei {
    pub plmn: OsmoPlmnId,
    pub mme: OsmoMmeId,
}

/// The text C sees: up to the first NUL.
pub(crate) fn c_text(text: &[u8]) -> &[u8] {
    text.iter()
        .position(|&byte| byte == 0)
        .map_or(text, |end| &text[..end])
}

fn digits_in(text: Option<&[u8]>, min: usize, max: usize) -> bool {
    match text.map(c_text) {
        None => min < 1,
        Some(text) => (min..=max).contains(&text.len()) && text.iter().all(u8::is_ascii_digit),
    }
}

/// `osmo_imsi_str_valid`.
pub fn osmo_imsi_str_valid(imsi: Option<&[u8]>) -> bool {
    digits_in(imsi, GSM23003_IMSI_MIN_DIGITS, GSM23003_IMSI_MAX_DIGITS)
}

/// `osmo_msisdn_str_valid`.
pub fn osmo_msisdn_str_valid(msisdn: Option<&[u8]>) -> bool {
    digits_in(
        msisdn,
        GSM23003_MSISDN_MIN_DIGITS,
        GSM23003_MSISDN_MAX_DIGITS,
    )
}

/// `osmo_imei_str_valid`. With `with_15th_digit`, the IMEI has fifteen digits and the last
/// is the Luhn check digit of the others; without, it has fourteen.
pub fn osmo_imei_str_valid(imei: Option<&[u8]>, with_15th_digit: bool) -> bool {
    if !with_15th_digit {
        return digits_in(imei, 14, 14);
    }
    let Some(imei) = imei.map(c_text) else {
        return false;
    };
    match std::str::from_utf8(imei) {
        Ok(text) => crate::gsm::gsm23003::is_valid_imei(text),
        Err(_) => false,
    }
}

/// `osmo_mnc_from_str`: one to three decimal digits, with the three-digit flag set for
/// three. Returns 0, or -22 and leaves `mnc` and `mnc_3_digits` as they were.
pub fn osmo_mnc_from_str(
    mnc_str: Option<&[u8]>,
    mnc: Option<&mut u16>,
    mnc_3_digits: Option<&mut bool>,
) -> i32 {
    let Some(text) = mnc_str.map(c_text) else {
        return -EINVAL;
    };
    let parsed = std::str::from_utf8(text)
        .ok()
        .and_then(|text| text.parse::<crate::gsm::gsm23003::Mnc>().ok());
    let Some(parsed) = parsed else {
        return -EINVAL;
    };
    if let Some(mnc) = mnc {
        *mnc = parsed.value();
    }
    if let Some(mnc_3_digits) = mnc_3_digits {
        *mnc_3_digits = text.len() > 2;
    }
    0
}

/// `osmo_mcc_from_str`: one to three decimal digits. Returns 0, or -22 and leaves `mcc` as
/// it was.
pub fn osmo_mcc_from_str(mcc_str: Option<&[u8]>, mcc: Option<&mut u16>) -> i32 {
    osmo_mnc_from_str(mcc_str, mcc, None)
}

/// `osmo_mcc_name`, and its `_c` variant.
pub fn osmo_mcc_name(mcc: u16) -> String {
    format!("{mcc:03}")
}

/// `osmo_mnc_name`, and its `_c` variant. A three-digit flag only adds leading zeros, so an
/// MNC from 100 on prints all its digits either way.
pub fn osmo_mnc_name(mnc: u16, mnc_3_digits: bool) -> String {
    if mnc_3_digits {
        format!("{mnc:03}")
    } else {
        format!("{mnc:02}")
    }
}

/// `osmo_plmn_name`, and its `name2` and `_c` variants.
pub fn osmo_plmn_name(plmn: &OsmoPlmnId) -> String {
    format!(
        "{}-{}",
        osmo_mcc_name(plmn.mcc),
        osmo_mnc_name(plmn.mnc, plmn.mnc_3_digits)
    )
}

/// `osmo_lai_name`, and its `_c` variant.
pub fn osmo_lai_name(lai: &OsmoLocationAreaId) -> String {
    format!("{}-{}", osmo_plmn_name(&lai.plmn), lai.lac)
}

/// `osmo_rai_name2`, and its `_c` variant.
pub fn osmo_rai_name2(rai: &OsmoRoutingAreaId) -> String {
    format!("{}-{}", osmo_lai_name(&rai.lac), rai.rac)
}

/// `osmo_cgi_name`, and its `name2` and `_c` variants.
pub fn osmo_cgi_name(cgi: &OsmoCellGlobalId) -> String {
    format!("{}-{}", osmo_lai_name(&cgi.lai), cgi.cell_identity)
}

/// `osmo_cgi_ps_name`, and its `name2` and `_c` variants.
pub fn osmo_cgi_ps_name(cgi_ps: &OsmoCellGlobalIdPs) -> String {
    format!("{}-{}", osmo_rai_name2(&cgi_ps.rai), cgi_ps.cell_identity)
}

/// `osmo_sai_name`, and its `name2` and `_c` variants.
pub fn osmo_sai_name(sai: &OsmoServiceAreaId) -> String {
    format!("{}-{}", osmo_lai_name(&sai.lai), sai.sac)
}

/// `osmo_rnc_id_name`, and its `_c` variant.
pub fn osmo_rnc_id_name(rnc_id: &OsmoRncId) -> String {
    format!("{}-{}", osmo_plmn_name(&rnc_id.plmn), rnc_id.rnc_id)
}

/// `osmo_gummei_name`, and its `_c` variant.
pub fn osmo_gummei_name(gummei: &OsmoGummei) -> String {
    format!(
        "{}-{:04x}-{:02x}",
        osmo_plmn_name(&gummei.plmn),
        gummei.mme.group_id,
        gummei.mme.code
    )
}

/// Writes text as `snprintf` does: as much as fits before a NUL, and nothing into an empty
/// buffer. The bytes after the NUL are left as they were.
pub(crate) fn write_truncated(buf: &mut [u8], text: &str) {
    if let Some(room) = buf.len().checked_sub(1) {
        let len = text.len().min(room);
        buf[..len].copy_from_slice(&text.as_bytes()[..len]);
        buf[len] = 0;
    }
}

macro_rules! name_buf {
    ($(#[$doc:meta] $buf_fn:ident => $name_fn:ident($ty:ty);)*) => {$(
        #[$doc]
        pub fn $buf_fn<'a>(buf: &'a mut [u8], id: &$ty) -> &'a mut [u8] {
            write_truncated(buf, &$name_fn(id));
            buf
        }
    )*};
}

name_buf! {
    /// `osmo_plmn_name_buf`: the name, cut to fit the buffer with its NUL.
    osmo_plmn_name_buf => osmo_plmn_name(OsmoPlmnId);
    /// `osmo_lai_name_buf`: the name, cut to fit the buffer with its NUL.
    osmo_lai_name_buf => osmo_lai_name(OsmoLocationAreaId);
    /// `osmo_rai_name2_buf`: the name, cut to fit the buffer with its NUL.
    osmo_rai_name2_buf => osmo_rai_name2(OsmoRoutingAreaId);
    /// `osmo_cgi_name_buf`: the name, cut to fit the buffer with its NUL.
    osmo_cgi_name_buf => osmo_cgi_name(OsmoCellGlobalId);
    /// `osmo_cgi_ps_name_buf`: the name, cut to fit the buffer with its NUL.
    osmo_cgi_ps_name_buf => osmo_cgi_ps_name(OsmoCellGlobalIdPs);
    /// `osmo_sai_name_buf`: the name, cut to fit the buffer with its NUL.
    osmo_sai_name_buf => osmo_sai_name(OsmoServiceAreaId);
    /// `osmo_rnc_id_name_buf`: the name, cut to fit the buffer with its NUL.
    osmo_rnc_id_name_buf => osmo_rnc_id_name(OsmoRncId);
    /// `osmo_gummei_name_buf`: the name, cut to fit the buffer with its NUL.
    osmo_gummei_name_buf => osmo_gummei_name(OsmoGummei);
}

/// `osmo_mcc_name_buf`: the name, cut to fit the buffer with its NUL.
pub fn osmo_mcc_name_buf(buf: &mut [u8], mcc: u16) -> &mut [u8] {
    write_truncated(buf, &osmo_mcc_name(mcc));
    buf
}

/// `osmo_mnc_name_buf`: the name, cut to fit the buffer with its NUL.
pub fn osmo_mnc_name_buf(buf: &mut [u8], mnc: u16, mnc_3_digits: bool) -> &mut [u8] {
    write_truncated(buf, &osmo_mnc_name(mnc, mnc_3_digits));
    buf
}

fn sign(ordering: Ordering) -> i32 {
    ordering as i32
}

/// `osmo_mnc_cmp`: by value, then two digits before three. The flag counts only under 100.
pub fn osmo_mnc_cmp(a_mnc: u16, a_mnc_3_digits: bool, b_mnc: u16, b_mnc_3_digits: bool) -> i32 {
    let flag = |mnc: u16, three: bool| mnc < 100 && three;
    sign(
        a_mnc
            .cmp(&b_mnc)
            .then(flag(a_mnc, a_mnc_3_digits).cmp(&flag(b_mnc, b_mnc_3_digits))),
    )
}

fn plmn_ordering(a: &OsmoPlmnId, b: &OsmoPlmnId) -> Ordering {
    a.mcc
        .cmp(&b.mcc)
        .then_with(|| osmo_mnc_cmp(a.mnc, a.mnc_3_digits, b.mnc, b.mnc_3_digits).cmp(&0))
}

fn lai_ordering(a: &OsmoLocationAreaId, b: &OsmoLocationAreaId) -> Ordering {
    plmn_ordering(&a.plmn, &b.plmn).then(a.lac.cmp(&b.lac))
}

fn rai_ordering(a: &OsmoRoutingAreaId, b: &OsmoRoutingAreaId) -> Ordering {
    lai_ordering(&a.lac, &b.lac).then(a.rac.cmp(&b.rac))
}

fn mme_id_ordering(a: &OsmoMmeId, b: &OsmoMmeId) -> Ordering {
    a.group_id.cmp(&b.group_id).then(a.code.cmp(&b.code))
}

/// `osmo_plmn_cmp`: MCC, then MNC as [`osmo_mnc_cmp`] orders it.
pub fn osmo_plmn_cmp(a: &OsmoPlmnId, b: &OsmoPlmnId) -> i32 {
    sign(plmn_ordering(a, b))
}

/// `osmo_lai_cmp`: PLMN, then LAC.
pub fn osmo_lai_cmp(a: &OsmoLocationAreaId, b: &OsmoLocationAreaId) -> i32 {
    sign(lai_ordering(a, b))
}

/// `osmo_rai_cmp`: LAI, then RAC.
pub fn osmo_rai_cmp(a: &OsmoRoutingAreaId, b: &OsmoRoutingAreaId) -> i32 {
    sign(rai_ordering(a, b))
}

/// `osmo_cgi_cmp`: LAI, then cell identity.
pub fn osmo_cgi_cmp(a: &OsmoCellGlobalId, b: &OsmoCellGlobalId) -> i32 {
    sign(lai_ordering(&a.lai, &b.lai).then(a.cell_identity.cmp(&b.cell_identity)))
}

/// `osmo_cgi_ps_cmp`: RAI, then cell identity.
pub fn osmo_cgi_ps_cmp(a: &OsmoCellGlobalIdPs, b: &OsmoCellGlobalIdPs) -> i32 {
    sign(rai_ordering(&a.rai, &b.rai).then(a.cell_identity.cmp(&b.cell_identity)))
}

/// `osmo_rnc_id_cmp`: PLMN, then RNC ID.
pub fn osmo_rnc_id_cmp(a: &OsmoRncId, b: &OsmoRncId) -> i32 {
    sign(plmn_ordering(&a.plmn, &b.plmn).then(a.rnc_id.cmp(&b.rnc_id)))
}

/// `osmo_mme_id_cmp`: MME group ID, then MME code.
pub fn osmo_mme_id_cmp(a: &OsmoMmeId, b: &OsmoMmeId) -> i32 {
    sign(mme_id_ordering(a, b))
}

/// `osmo_gummei_cmp`: PLMN, then MME ID.
pub fn osmo_gummei_cmp(a: &OsmoGummei, b: &OsmoGummei) -> i32 {
    sign(plmn_ordering(&a.plmn, &b.plmn).then(mme_id_ordering(&a.mme, &b.mme)))
}

/// Writes text and its NUL as `sprintf` does, returning the text's length.
fn write_text(out: &mut [u8], text: &str) -> i32 {
    out[..text.len()].copy_from_slice(text.as_bytes());
    out[text.len()] = 0;
    text.len() as i32
}

fn home_domain(plmn: &OsmoPlmnId) -> Option<String> {
    (plmn.mcc <= 999 && plmn.mnc <= 999)
        .then(|| format!("epc.mnc{:03}.mcc{:03}.3gppnetwork.org", plmn.mnc, plmn.mcc))
}

/// `osmo_gen_home_network_domain`: writes the domain and its NUL, and returns its length.
/// Returns -22 for an MCC or MNC past 999 and leaves `out` as it was.
///
/// C asks for a buffer of at least 33 bytes but writes 34. This one holds them.
pub fn osmo_gen_home_network_domain(
    out: &mut [u8; GSM23003_HOME_NETWORK_DOMAIN_LEN + 1],
    plmn: &OsmoPlmnId,
) -> i32 {
    match home_domain(plmn) {
        Some(domain) => write_text(out, &domain),
        None => -EINVAL,
    }
}

/// `osmo_gen_mme_domain`: writes the domain and its NUL, and returns its length. Returns
/// -22 for an MCC or MNC past 999 and leaves `out` as it was.
pub fn osmo_gen_mme_domain(out: &mut [u8; GSM23003_MME_DOMAIN_LEN], gummei: &OsmoGummei) -> i32 {
    match home_domain(&gummei.plmn) {
        Some(domain) => write_text(
            out,
            &format!(
                "mmec{:02x}.mmegi{:04x}.mme.{domain}",
                gummei.mme.code, gummei.mme.group_id
            ),
        ),
        None => -EINVAL,
    }
}

/// `osmo_gen_mme_group_domain`: writes the domain and its NUL, and returns its length.
/// Returns -22 for an MCC or MNC past 999 and leaves `out` as it was.
pub fn osmo_gen_mme_group_domain(
    out: &mut [u8; GSM23003_MME_DOMAIN_LEN],
    mmegi: u16,
    plmn: &OsmoPlmnId,
) -> i32 {
    match home_domain(plmn) {
        Some(domain) => write_text(out, &format!("mmegi{mmegi:04x}.mme.{domain}")),
        None => -EINVAL,
    }
}

/// One piece of a `sscanf` format: text to match, or an unsigned number of at most
/// `width` characters.
enum Piece {
    Text(&'static [u8]),
    Unsigned { width: usize, hex: bool },
}

/// Matches input against a format as glibc's `sscanf` does for these formats, calling
/// `store` with each number read, in order. Returns how many numbers were read, or `EOF`
/// if the input ran out before the first.
///
/// A number skips leading white space, then takes an optional sign and digits, and for hex
/// an optional `0x` prefix, up to `width` characters in all. A minus sign negates it modulo
/// 2^64, and `store` truncates it to its field. Text after the last number isn't checked.
fn scan(input: &[u8], format: &[Piece], mut store: impl FnMut(usize, u64)) -> i32 {
    let mut at = 0;
    let mut converted = 0;
    let ran_out = |converted: i32| if converted == 0 { EOF } else { converted };
    for piece in format {
        match piece {
            Piece::Text(text) => {
                for &expected in *text {
                    match input.get(at) {
                        None => return ran_out(converted),
                        Some(&byte) if byte == expected => at += 1,
                        Some(_) => return converted,
                    }
                }
            }
            &Piece::Unsigned { width, hex } => {
                while input.get(at).copied().is_some_and(c_space) {
                    at += 1;
                }
                if at == input.len() {
                    return ran_out(converted);
                }
                let field = &input[at..(at + width).min(input.len())];
                let (negative, digits) = match field[0] {
                    b'-' => (true, &field[1..]),
                    b'+' => (false, &field[1..]),
                    _ => (false, field),
                };
                let Some((value, taken)) = number(digits, hex) else {
                    return converted;
                };
                at += field.len() - digits.len() + taken;
                store(
                    converted as usize,
                    if negative {
                        value.wrapping_neg()
                    } else {
                        value
                    },
                );
                converted += 1;
            }
        }
    }
    converted
}

/// C's `isspace` in the C locale.
fn c_space(byte: u8) -> bool {
    byte == b' ' || (0x09..=0x0d).contains(&byte)
}

/// Reads digits from the start of a field, with an optional `0x` prefix for hex. Returns
/// the value and how many bytes it took, or `None` if there was no number. A prefix with no
/// digits after it reads as 0, as glibc does.
fn number(field: &[u8], hex: bool) -> Option<(u64, usize)> {
    let radix = if hex { 16 } else { 10 };
    let prefix = hex && field.len() >= 2 && field[0] == b'0' && matches!(field[1], b'x' | b'X');
    let start = if prefix { 2 } else { 0 };
    let digits = field[start..]
        .iter()
        .take_while(|byte| (**byte as char).is_digit(radix))
        .count();
    if digits == 0 && !prefix {
        return None;
    }
    let value = field[start..start + digits]
        .iter()
        .fold(0u64, |value, byte| {
            value
                .wrapping_mul(u64::from(radix))
                .wrapping_add(u64::from((*byte as char).to_digit(radix).unwrap_or(0)))
        });
    Some((value, start + digits))
}

/// `osmo_parse_home_network_domain`, with C's `sscanf` matching: returns 0 once both
/// numbers are read, whatever follows them, -1 if the input ends before the first, and
/// otherwise -22 with the MNC kept if it was read. `out` is cleared first, and the
/// three-digit flag is always false.
pub fn osmo_parse_home_network_domain(out: &mut OsmoPlmnId, input: &[u8]) -> i32 {
    *out = OsmoPlmnId::default();
    let format = [
        Piece::Text(b"epc.mnc"),
        Piece::Unsigned {
            width: 3,
            hex: false,
        },
        Piece::Text(b".mcc"),
        Piece::Unsigned {
            width: 3,
            hex: false,
        },
        Piece::Text(b".3gppnetwork.org"),
    ];
    let rc = scan(c_text(input), &format, |field, value| match field {
        0 => out.mnc = value as u16,
        _ => out.mcc = value as u16,
    });
    match rc {
        EOF => EOF,
        2 => 0,
        _ => -EINVAL,
    }
}

/// `osmo_parse_mme_domain`, with C's `sscanf` matching: returns 0 once all four numbers
/// are read, whatever follows them, -1 if the input ends before the first, and otherwise
/// -22 with the fields read so far kept. `out` is cleared first, and the three-digit flag is
/// always false.
pub fn osmo_parse_mme_domain(out: &mut OsmoGummei, input: &[u8]) -> i32 {
    *out = OsmoGummei::default();
    let format = [
        Piece::Text(b"mmec"),
        Piece::Unsigned {
            width: 2,
            hex: true,
        },
        Piece::Text(b".mmegi"),
        Piece::Unsigned {
            width: 4,
            hex: true,
        },
        Piece::Text(b".mme.epc.mnc"),
        Piece::Unsigned {
            width: 3,
            hex: false,
        },
        Piece::Text(b".mcc"),
        Piece::Unsigned {
            width: 3,
            hex: false,
        },
        Piece::Text(b".3gppnetwork.org"),
    ];
    let rc = scan(c_text(input), &format, |field, value| match field {
        0 => out.mme.code = value as u8,
        1 => out.mme.group_id = value as u16,
        2 => out.plmn.mnc = value as u16,
        _ => out.plmn.mcc = value as u16,
    });
    match rc {
        EOF => EOF,
        4 => 0,
        _ => -EINVAL,
    }
}

/// The last three decimal digits of a value, hundreds first, as C's encoders take them.
fn last_three_digits(value: u16) -> [u8; 3] {
    [value / 100 % 10, value / 10 % 10, value % 10].map(|digit| digit as u8)
}

/// `osmo_plmn_to_bcd`: 3GPP TS 24.008's three bytes. An MNC from 100 on has three digits
/// whatever the flag says, and a value past 999 gives its last three digits.
pub fn osmo_plmn_to_bcd(bcd_dst: &mut [u8; 3], plmn: &OsmoPlmnId) {
    let [mcc1, mcc2, mcc3] = last_three_digits(plmn.mcc);
    let [mnc1, mnc2, mnc3] = last_three_digits(plmn.mnc);
    *bcd_dst = if plmn.mnc > 99 || plmn.mnc_3_digits {
        [mcc2 << 4 | mcc1, mnc3 << 4 | mcc3, mnc2 << 4 | mnc1]
    } else {
        [mcc2 << 4 | mcc1, 0xf0 | mcc3, mnc3 << 4 | mnc2]
    };
}

/// `osmo_plmn_from_bcd`: each nibble weighted by its decimal place, unchecked, so hex
/// nibbles give values past 999. An MNC's third digit other than F sets the three-digit
/// flag.
pub fn osmo_plmn_from_bcd(bcd_src: &[u8; 3], plmn: &mut OsmoPlmnId) {
    let low = |byte: u8| u16::from(byte & 0x0f);
    let high = |byte: u8| u16::from(byte >> 4);
    plmn.mcc = low(bcd_src[0]) * 100 + high(bcd_src[0]) * 10 + low(bcd_src[1]);
    if high(bcd_src[1]) == 0x0f {
        plmn.mnc = low(bcd_src[2]) * 10 + high(bcd_src[2]);
        plmn.mnc_3_digits = false;
    } else {
        plmn.mnc = low(bcd_src[2]) * 100 + high(bcd_src[2]) * 10 + high(bcd_src[1]);
        plmn.mnc_3_digits = true;
    }
}
