// SPDX-License-Identifier: GPL-3.0-or-later

//! C's 3GPP TS 24.008 identity functions, with C's structs, any 16-bit MCC and MNC, the
//! legacy `gprs_ra_id` and Mobile Identity functions, and C's status codes. Encoded
//! structs are byte arrays of their packed size, and `None` stands for a NULL pointer.
//!
//! The preferred API is [`crate::gsm::gsm48`], whose decoders take only decimal digits and
//! whose [`MobileIdentity`](crate::gsm::gsm48::MobileIdentity) holds only identities that
//! encode.

use super::gsm23003::{
    OsmoLocationAreaId, OsmoPlmnId, OsmoRoutingAreaId, c_text, osmo_mcc_name, osmo_mnc_name,
    osmo_plmn_from_bcd, osmo_plmn_to_bcd, write_truncated,
};
use crate::base::bcd::legacy::{osmo_bcd2str, osmo_char2bcd, osmo_str2bcd};
use std::cmp::Ordering;

/// `GSM_MI_TYPE_MASK`.
pub const GSM_MI_TYPE_MASK: u8 = 0x07;
/// `GSM_MI_TYPE_NONE`.
pub const GSM_MI_TYPE_NONE: u8 = 0x00;
/// `GSM_MI_TYPE_IMSI`.
pub const GSM_MI_TYPE_IMSI: u8 = 0x01;
/// `GSM_MI_TYPE_IMEI`.
pub const GSM_MI_TYPE_IMEI: u8 = 0x02;
/// `GSM_MI_TYPE_IMEISV`.
pub const GSM_MI_TYPE_IMEISV: u8 = 0x03;
/// `GSM_MI_TYPE_TMSI`.
pub const GSM_MI_TYPE_TMSI: u8 = 0x04;
/// `GSM_MI_ODD`: the flag for an odd number of digits.
pub const GSM_MI_ODD: u8 = 0x08;
/// `GSM48_IE_MOBILE_ID`: the Mobile Identity's tag.
pub const GSM48_IE_MOBILE_ID: u8 = 0x17;
/// `GSM48_TMSI_LEN`: a TMSI identity's bytes after the tag and length.
pub const GSM48_TMSI_LEN: usize = 5;
/// `GSM48_MID_TMSI_LEN`: a TMSI identity with its tag and length.
pub const GSM48_MID_TMSI_LEN: usize = GSM48_TMSI_LEN + 2;
/// `GSM48_MID_MAX_SIZE`: the most bytes `gsm48_generate_mid` writes.
pub const GSM48_MID_MAX_SIZE: usize = 11;
/// `GSM48_MI_SIZE`: the size of the digit buffer of `osmo_mi_name`.
pub const GSM48_MI_SIZE: usize = 32;
/// `GSM_RESERVED_TMSI`.
pub const GSM_RESERVED_TMSI: u32 = 0xffff_ffff;
/// `GSM23003_TMSI_NUM_BYTES`.
pub const GSM23003_TMSI_NUM_BYTES: usize = 4;
/// `GSM23003_IMEISV_NUM_DIGITS`.
pub const GSM23003_IMEISV_NUM_DIGITS: usize = 16;
/// `sizeof(struct gsm48_loc_area_id)`.
pub const GSM48_LOC_AREA_ID_LEN: usize = 5;
/// `sizeof(struct gsm48_ra_id)`.
pub const GSM48_RA_ID_LEN: usize = 6;

const EIO: i32 = -5;
const EINVAL: i32 = -22;
const ENOSPC: i32 = -28;
const EBADMSG: i32 = -74;
const ENOTSUP: i32 = -95;

/// `struct gprs_ra_id`, the legacy routing area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GprsRaId {
    pub mcc: u16,
    pub mnc: u16,
    pub mnc_3_digits: bool,
    pub lac: u16,
    pub rac: u8,
}

/// `struct osmo_mobile_identity`: a type byte and the union of the IMSI, IMEI and IMEISV
/// text and the TMSI. The union is its largest member, the IMEISV's 17 bytes; the TMSI is its
/// first four in this machine's byte order, as C reads them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OsmoMobileIdentity {
    pub mi_type: u8,
    pub union: [u8; 17],
}

impl OsmoMobileIdentity {
    /// An identity of `mi_type` with `digits` in its union, as a caller fills one.
    pub fn with_digits(mi_type: u8, digits: &[u8]) -> Self {
        let mut identity = Self {
            mi_type,
            ..Self::default()
        };
        identity.union[..digits.len()].copy_from_slice(digits);
        identity
    }

    /// A TMSI identity.
    pub fn with_tmsi(tmsi: u32) -> Self {
        let mut identity = Self {
            mi_type: GSM_MI_TYPE_TMSI,
            ..Self::default()
        };
        identity.set_tmsi(tmsi);
        identity
    }

    /// The union read as the TMSI.
    pub fn tmsi(&self) -> u32 {
        u32::from_ne_bytes(self.union[..4].try_into().expect("four bytes"))
    }

    pub fn set_tmsi(&mut self, tmsi: u32) {
        self.union[..4].copy_from_slice(&tmsi.to_ne_bytes());
    }

    /// The union read as text, up to its NUL. C's `strlen` would read past an unterminated
    /// union; here the text ends with the union.
    pub fn digits(&self) -> &[u8] {
        c_text(&self.union)
    }
}

fn lai_from_gprs(raid: &GprsRaId) -> OsmoLocationAreaId {
    OsmoLocationAreaId {
        plmn: OsmoPlmnId {
            mcc: raid.mcc,
            mnc: raid.mnc,
            mnc_3_digits: raid.mnc_3_digits,
        },
        lac: raid.lac,
    }
}

/// `gsm48_mcc_mnc_to_bcd`: [`osmo_plmn_to_bcd`] with a two-digit flag.
pub fn gsm48_mcc_mnc_to_bcd(bcd_dst: &mut [u8; 3], mcc: u16, mnc: u16) {
    let plmn = OsmoPlmnId {
        mcc,
        mnc,
        mnc_3_digits: false,
    };
    osmo_plmn_to_bcd(bcd_dst, &plmn);
}

/// `gsm48_mcc_mnc_from_bcd`: [`osmo_plmn_from_bcd`] without the flag.
pub fn gsm48_mcc_mnc_from_bcd(bcd_src: &[u8; 3], mcc: &mut u16, mnc: &mut u16) {
    let mut plmn = OsmoPlmnId::default();
    osmo_plmn_from_bcd(bcd_src, &mut plmn);
    *mcc = plmn.mcc;
    *mnc = plmn.mnc;
}

/// `gsm48_generate_lai2`: the PLMN in BCD, then the LAC, most significant byte first.
pub fn gsm48_generate_lai2(lai48: &mut [u8; GSM48_LOC_AREA_ID_LEN], lai: &OsmoLocationAreaId) {
    let mut bcd = [0; 3];
    osmo_plmn_to_bcd(&mut bcd, &lai.plmn);
    lai48[..3].copy_from_slice(&bcd);
    lai48[3..].copy_from_slice(&lai.lac.to_be_bytes());
}

/// `gsm48_generate_lai`: [`gsm48_generate_lai2`] with a two-digit flag.
pub fn gsm48_generate_lai(lai48: &mut [u8; GSM48_LOC_AREA_ID_LEN], mcc: u16, mnc: u16, lac: u16) {
    let lai = OsmoLocationAreaId {
        plmn: OsmoPlmnId {
            mcc,
            mnc,
            mnc_3_digits: false,
        },
        lac,
    };
    gsm48_generate_lai2(lai48, &lai);
}

/// `gsm48_decode_lai2`.
pub fn gsm48_decode_lai2(lai: &[u8; GSM48_LOC_AREA_ID_LEN], decoded: &mut OsmoLocationAreaId) {
    osmo_plmn_from_bcd(&[lai[0], lai[1], lai[2]], &mut decoded.plmn);
    decoded.lac = u16::from_be_bytes([lai[3], lai[4]]);
}

/// `gsm48_decode_lai`: [`gsm48_decode_lai2`] without the flag. Returns 0.
pub fn gsm48_decode_lai(
    lai: &[u8; GSM48_LOC_AREA_ID_LEN],
    mcc: &mut u16,
    mnc: &mut u16,
    lac: &mut u16,
) -> i32 {
    let mut decoded = OsmoLocationAreaId::default();
    gsm48_decode_lai2(lai, &mut decoded);
    *mcc = decoded.plmn.mcc;
    *mnc = decoded.plmn.mnc;
    *lac = decoded.lac;
    0
}

/// `osmo_routing_area_id_decode`: the first six bytes, or `-ENOSPC` with `dst` untouched.
pub fn osmo_routing_area_id_decode(dst: &mut OsmoRoutingAreaId, ra_data: &[u8]) -> i32 {
    let Some(ra) = ra_data.first_chunk::<GSM48_RA_ID_LEN>() else {
        return ENOSPC;
    };
    gsm48_decode_lai2(ra[..5].try_into().expect("five bytes"), &mut dst.lac);
    dst.rac = ra[5];
    GSM48_RA_ID_LEN as i32
}

/// `osmo_routing_area_id_encode_buf`: six bytes, or `-ENOSPC` with `buf` untouched.
pub fn osmo_routing_area_id_encode_buf(buf: &mut [u8], src: &OsmoRoutingAreaId) -> i32 {
    let Some(ra) = buf.first_chunk_mut::<GSM48_RA_ID_LEN>() else {
        return ENOSPC;
    };
    let mut lai = [0; GSM48_LOC_AREA_ID_LEN];
    gsm48_generate_lai2(&mut lai, &src.lac);
    ra[..5].copy_from_slice(&lai);
    ra[5] = src.rac;
    GSM48_RA_ID_LEN as i32
}

/// `gsm48_parse_ra`: decodes as [`osmo_routing_area_id_decode`] does, into the legacy
/// struct.
pub fn gsm48_parse_ra(raid: &mut GprsRaId, buf: &[u8; GSM48_RA_ID_LEN]) {
    let mut decoded = OsmoRoutingAreaId::default();
    osmo_routing_area_id_decode(&mut decoded, buf);
    osmo_rai_to_gprs(raid, &decoded);
}

/// `gsm48_encode_ra`: encodes as [`osmo_routing_area_id_encode_buf`] does, from the
/// legacy struct.
pub fn gsm48_encode_ra(out: &mut [u8; GSM48_RA_ID_LEN], raid: &GprsRaId) {
    let mut rai = OsmoRoutingAreaId::default();
    gprs_rai_to_osmo(&mut rai, raid);
    osmo_routing_area_id_encode_buf(out, &rai);
}

/// `gsm48_construct_ra`: [`gsm48_encode_ra`], returning 6.
pub fn gsm48_construct_ra(buf: &mut [u8; GSM48_RA_ID_LEN], raid: &GprsRaId) -> i32 {
    gsm48_encode_ra(buf, raid);
    GSM48_RA_ID_LEN as i32
}

/// `gsm48_ra_equal`: every field equal, including the MNC flag from 100 on.
pub fn gsm48_ra_equal(raid1: &GprsRaId, raid2: &GprsRaId) -> bool {
    raid1 == raid2
}

/// `osmo_rai_to_gprs`.
pub fn osmo_rai_to_gprs(dest: &mut GprsRaId, src: &OsmoRoutingAreaId) {
    *dest = GprsRaId {
        mcc: src.lac.plmn.mcc,
        mnc: src.lac.plmn.mnc,
        mnc_3_digits: src.lac.plmn.mnc_3_digits,
        lac: src.lac.lac,
        rac: src.rac,
    };
}

/// `gprs_rai_to_osmo`.
pub fn gprs_rai_to_osmo(dest: &mut OsmoRoutingAreaId, src: &GprsRaId) {
    *dest = OsmoRoutingAreaId {
        lac: lai_from_gprs(src),
        rac: src.rac,
    };
}

/// `osmo_rai_name`, and its `_c` variant: `MCC-MNC-LAC-RAC`.
pub fn osmo_rai_name(rai: &GprsRaId) -> String {
    format!(
        "{}-{}-{}-{}",
        osmo_mcc_name(rai.mcc),
        osmo_mnc_name(rai.mnc, rai.mnc_3_digits),
        rai.lac,
        rai.rac
    )
}

/// `osmo_rai_name_buf`: the name, cut to fit the buffer with its NUL.
pub fn osmo_rai_name_buf<'a>(buf: &'a mut [u8], rai: &GprsRaId) -> &'a mut [u8] {
    write_truncated(buf, &osmo_rai_name(rai));
    buf
}

/// `gsm48_mi_type_name`: the type's name, or `unknown 0x` and the value in hex.
pub fn gsm48_mi_type_name(mi: u8) -> String {
    match mi {
        GSM_MI_TYPE_NONE => "NONE".to_owned(),
        GSM_MI_TYPE_IMSI => "IMSI".to_owned(),
        GSM_MI_TYPE_IMEI => "IMEI".to_owned(),
        GSM_MI_TYPE_IMEISV => "IMEI-SV".to_owned(),
        GSM_MI_TYPE_TMSI => "TMSI".to_owned(),
        _ => format!("unknown 0x{mi:x}"),
    }
}

/// The legacy type of encoded bytes: the low three bits, or NONE for none.
fn legacy_type(mi: Option<&[u8]>) -> (u8, &[u8]) {
    match mi {
        Some(mi @ [first, ..]) => (first & GSM_MI_TYPE_MASK, mi),
        _ => (GSM_MI_TYPE_NONE, &[]),
    }
}

/// A legacy TMSI: exactly five bytes, the first F4.
fn legacy_tmsi(mi: &[u8]) -> Option<u32> {
    match mi {
        [0xf4, tmsi @ ..] => Some(u32::from_be_bytes(tmsi.try_into().ok()?)),
        _ => None,
    }
}

/// The digit nibbles the legacy functions read: from the first byte's high nibble to the
/// end, less a last nibble without the odd flag. They aren't checked.
fn legacy_digits_end(mi: &[u8]) -> usize {
    mi.len() * 2 - usize::from(mi[0] & GSM_MI_ODD == 0)
}

/// `osmo_mi_name`, and its `_c` variant: `TYPE-digits`, `TMSI-0x` and eight hex digits,
/// `TMSI-invalid` or `unknown`. Digits are cut at 31.
pub fn osmo_mi_name(mi: Option<&[u8]>) -> String {
    let (mi_type, mi) = legacy_type(mi);
    match mi_type {
        GSM_MI_TYPE_TMSI => match legacy_tmsi(mi) {
            Some(tmsi) => format!("TMSI-0x{tmsi:08X}"),
            None => "TMSI-invalid".to_owned(),
        },
        GSM_MI_TYPE_IMSI | GSM_MI_TYPE_IMEI | GSM_MI_TYPE_IMEISV => {
            let mut digits = [0; GSM48_MI_SIZE];
            let _ = osmo_bcd2str(&mut digits, mi, 1, legacy_digits_end(mi), true);
            let digits = String::from_utf8_lossy(c_text(&digits)).into_owned();
            format!("{}-{digits}", gsm48_mi_type_name(mi_type))
        }
        _ => "unknown".to_owned(),
    }
}

/// `osmo_mi_name_buf`: the name, cut to fit the buffer with its NUL.
pub fn osmo_mi_name_buf<'a>(buf: &'a mut [u8], mi: Option<&[u8]>) -> &'a mut [u8] {
    write_truncated(buf, &osmo_mi_name(mi));
    buf
}

/// `gsm48_mi_to_string`: the digits, or a TMSI in decimal, into `string`. Returns the bytes
/// written with the NUL for digits, the TMSI's full length as `snprintf` does, 1 for an
/// empty string, and 0 for an empty buffer.
pub fn gsm48_mi_to_string(string: &mut [u8], mi: Option<&[u8]>) -> i32 {
    let (mi_type, mi) = legacy_type(mi);
    match mi_type {
        GSM_MI_TYPE_TMSI => {
            if let Some(tmsi) = legacy_tmsi(mi) {
                let text = tmsi.to_string();
                write_truncated(string, &text);
                return text.len() as i32;
            }
        }
        GSM_MI_TYPE_IMSI | GSM_MI_TYPE_IMEI | GSM_MI_TYPE_IMEISV => {
            return match osmo_bcd2str(string, mi, 1, legacy_digits_end(mi), true) {
                Err(_) => 0,
                Ok(len) if len < string.len() => len as i32 + 1,
                Ok(_) => c_text(string).len() as i32 + 1,
            };
        }
        _ => {}
    }
    match string.first_mut() {
        None => 0,
        Some(first) => {
            *first = 0;
            1
        }
    }
}

/// `gsm48_generate_mid`: the tag, length and value of an identity of `mi_type` from up to
/// 16 characters of `id`, each a hex digit or else 0. Returns the bytes written.
pub fn gsm48_generate_mid(buf: &mut [u8; GSM48_MID_MAX_SIZE], id: &[u8], mi_type: u8) -> u8 {
    let id = &c_text(id)[..c_text(id).len().min(16)];
    let length = id.len();
    let odd = length % 2 == 1;
    let digit = |index: usize| osmo_char2bcd(id.get(index).copied().unwrap_or(0));
    let value_len = (length + if odd { 1 } else { 2 }) / 2;
    buf[0] = GSM48_IE_MOBILE_ID;
    buf[1] = value_len as u8;
    buf[2] = digit(0) << 4 | (mi_type & GSM_MI_TYPE_MASK) | if odd { GSM_MI_ODD } else { 0 };
    for i in 1..value_len {
        let low = digit(2 * i - 1);
        let high = if !odd && 2 * i == length {
            0x0f
        } else {
            digit(2 * i) & 0x0f
        };
        buf[2 + i] = high << 4 | low;
    }
    2 + value_len as u8
}

/// `gsm48_generate_mid_from_imsi`.
pub fn gsm48_generate_mid_from_imsi(buf: &mut [u8; GSM48_MID_MAX_SIZE], imsi: &[u8]) -> i32 {
    i32::from(gsm48_generate_mid(buf, imsi, GSM_MI_TYPE_IMSI))
}

/// `gsm48_generate_mid_from_tmsi`: the tag, length 5, F4 and the TMSI. Returns 7.
pub fn gsm48_generate_mid_from_tmsi(buf: &mut [u8; GSM48_MID_TMSI_LEN], tmsi: u32) -> i32 {
    buf[..3].copy_from_slice(&[
        GSM48_IE_MOBILE_ID,
        GSM48_TMSI_LEN as u8,
        0xf0 | GSM_MI_TYPE_TMSI,
    ]);
    buf[3..].copy_from_slice(&tmsi.to_be_bytes());
    GSM48_MID_TMSI_LEN as i32
}

/// The digit lengths each type decodes and encodes, and its member's size in the union.
fn digit_type(mi_type: u8) -> Option<(std::ops::RangeInclusive<usize>, usize)> {
    match mi_type {
        GSM_MI_TYPE_IMSI => Some((6..=15, 16)),
        GSM_MI_TYPE_IMEI => Some((14..=15, 16)),
        GSM_MI_TYPE_IMEISV => Some((16..=16, 17)),
        _ => None,
    }
}

/// `osmo_mobile_identity_decode`: 0, `-EBADMSG` for a wrong length, filler or digit, or
/// `-EINVAL` for an unknown type. On failure the identity is NONE with a zero union, so
/// its TMSI is 0.
pub fn osmo_mobile_identity_decode(
    mi: &mut OsmoMobileIdentity,
    mi_data: Option<&[u8]>,
    allow_hex: bool,
) -> i32 {
    let rc = decode_into(mi, mi_data, allow_hex);
    if rc != 0 {
        *mi = OsmoMobileIdentity::default();
    }
    rc
}

fn decode_into(mi: &mut OsmoMobileIdentity, mi_data: Option<&[u8]>, allow_hex: bool) -> i32 {
    let Some(data @ [first, rest @ ..]) = mi_data else {
        return EBADMSG;
    };
    let odd = first & GSM_MI_ODD != 0;
    let count = rest.len() * 2 + usize::from(odd);
    *mi = OsmoMobileIdentity {
        mi_type: first & GSM_MI_TYPE_MASK,
        ..OsmoMobileIdentity::default()
    };
    if mi.mi_type == GSM_MI_TYPE_TMSI {
        mi.set_tmsi(GSM_RESERVED_TMSI);
        if count != GSM23003_TMSI_NUM_BYTES * 2 || first >> 4 != 0x0f {
            return EBADMSG;
        }
        mi.set_tmsi(u32::from_be_bytes(rest.try_into().expect("four bytes")));
        return 0;
    }
    let Some((lengths, size)) = digit_type(mi.mi_type) else {
        return EINVAL;
    };
    if !lengths.contains(&count) || (!odd && rest.last().is_some_and(|last| last >> 4 != 0x0f)) {
        return EBADMSG;
    }
    match osmo_bcd2str(&mut mi.union[..size], data, 1, 1 + count, allow_hex) {
        Ok(len) if len >= 1 && len < size => 0,
        _ => EBADMSG,
    }
}

/// `osmo_mobile_identity_encoded_len`: the bytes the encoding takes, writing the digit count
/// or 8 for a TMSI; `-EINVAL` for a NULL identity or a wrong digit count, or `-ENOTSUP` for
/// a type without an encoding.
pub fn osmo_mobile_identity_encoded_len(
    mi: Option<&OsmoMobileIdentity>,
    mi_digits: Option<&mut i32>,
) -> i32 {
    let Some(mi) = mi else {
        return EINVAL;
    };
    let count = if mi.mi_type == GSM_MI_TYPE_TMSI {
        GSM23003_TMSI_NUM_BYTES * 2
    } else {
        match digit_type(mi.mi_type) {
            Some((lengths, _)) if lengths.contains(&mi.digits().len()) => mi.digits().len(),
            Some(_) => return EINVAL,
            None => return ENOTSUP,
        }
    };
    if let Some(mi_digits) = mi_digits {
        *mi_digits = count as i32;
    }
    (count as i32 + 2) / 2
}

/// `osmo_mobile_identity_encode_buf`: the bytes written, or `-EIO` for no buffer, `-ENOSPC`
/// when it doesn't fit, `-ENOTSUP` for a type without an encoding, and `-EINVAL` for a
/// wrong digit count or an invalid digit, after the bytes before it are written.
pub fn osmo_mobile_identity_encode_buf(
    buf: Option<&mut [u8]>,
    mi: Option<&OsmoMobileIdentity>,
    allow_hex: bool,
) -> i32 {
    let Some(buf) = buf.filter(|buf| !buf.is_empty()) else {
        return EIO;
    };
    let mut count = 0;
    let octets = osmo_mobile_identity_encoded_len(mi, Some(&mut count));
    if octets < 0 {
        return octets;
    }
    if octets as usize > buf.len() {
        return ENOSPC;
    }
    let mi = mi.expect("a NULL identity has no length");
    buf[0] = (mi.mi_type & GSM_MI_TYPE_MASK) | if count % 2 == 1 { GSM_MI_ODD } else { 0 };
    if mi.mi_type == GSM_MI_TYPE_TMSI {
        buf[0] |= 0xf0;
        buf[1..5].copy_from_slice(&mi.tmsi().to_be_bytes());
        return octets;
    }
    match osmo_str2bcd(buf, mi.digits(), 1, None, allow_hex) {
        Ok(used) if used as i32 == octets => octets,
        _ => EINVAL,
    }
}

/// `osmo_mobile_identity_decode_from_l3_buf`: the identity of a Complete Layer 3 message.
/// The identity starts as NONE with `GSM_RESERVED_TMSI` and stays so if the message is too
/// short (`-EBADMSG`) or carries no identity (`-ENOTSUP`); otherwise
/// [`osmo_mobile_identity_decode`]'s result.
pub fn osmo_mobile_identity_decode_from_l3_buf(
    mi: &mut OsmoMobileIdentity,
    l3_data: &[u8],
    allow_hex: bool,
) -> i32 {
    *mi = OsmoMobileIdentity::default();
    mi.set_tmsi(GSM_RESERVED_TMSI);
    match l3_identity(l3_data) {
        Ok(identity) => osmo_mobile_identity_decode(mi, Some(identity), allow_hex),
        Err(rc) => rc,
    }
}

/// The identity bytes of a Complete Layer 3 message, as C finds them.
fn l3_identity(l3: &[u8]) -> Result<&[u8], i32> {
    let [pdisc, message_type, body @ ..] = l3 else {
        return Err(EBADMSG);
    };
    let pdisc = if pdisc & 0x0f == 0x0e {
        *pdisc
    } else {
        pdisc & 0x0f
    };
    // The index in the body of the identity's length byte, or of the classmark 2's.
    let (length_at, classmark, minimum) = match (pdisc, *message_type) {
        (0x05, mm) => match mm & 0x3f {
            0x08 => (7, false, 8),
            0x24 | 0x28 => (1, true, 2),
            0x01 => (1, false, 2),
            0x19 => (0, false, 2),
            _ => return Err(ENOTSUP),
        },
        (0x06, 0x27) => (1, true, 6),
        (0x06, 0x11) => {
            if body.len() < 6 || body[0] != 3 {
                return Err(EBADMSG);
            }
            (0, true, 6)
        }
        _ => return Err(ENOTSUP),
    };
    if body.len() < minimum {
        return Err(EBADMSG);
    }
    let length_at = if classmark {
        length_at + 1 + usize::from(body[length_at])
    } else {
        length_at
    };
    let length = usize::from(*body.get(length_at).ok_or(EBADMSG)?);
    body.get(length_at + 1..length_at + 1 + length)
        .ok_or(EBADMSG)
}

fn identity_suffix(mi: &OsmoMobileIdentity) -> String {
    match mi.mi_type {
        GSM_MI_TYPE_TMSI => format!("-0x{:08X}", mi.tmsi()),
        GSM_MI_TYPE_IMSI | GSM_MI_TYPE_IMEI | GSM_MI_TYPE_IMEISV => {
            format!("-{}", String::from_utf8_lossy(mi.digits()))
        }
        _ => String::new(),
    }
}

/// `osmo_mobile_identity_to_str_c`: `TYPE-digits`, `TMSI-0x` and eight hex digits, the bare
/// type name, or `NULL`.
pub fn osmo_mobile_identity_to_str_c(mi: Option<&OsmoMobileIdentity>) -> String {
    match mi {
        None => "NULL".to_owned(),
        Some(mi) => format!("{}{}", gsm48_mi_type_name(mi.mi_type), identity_suffix(mi)),
    }
}

/// `osmo_mobile_identity_to_str_buf`: the text, cut to fit the buffer with its NUL. Returns
/// the full text's length, as `snprintf` does.
pub fn osmo_mobile_identity_to_str_buf(buf: &mut [u8], mi: Option<&OsmoMobileIdentity>) -> i32 {
    let text = osmo_mobile_identity_to_str_c(mi);
    write_truncated(buf, &text);
    text.len() as i32
}

fn sign(ordering: Ordering) -> i32 {
    ordering as i32
}

/// `strncmp` over `n` bytes, as glibc returns it: the difference of the first differing
/// bytes, or 0.
fn strncmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    for (&x, &y) in a.iter().zip(b).take(n) {
        if x != y || x == 0 {
            return i32::from(x) - i32::from(y);
        }
    }
    0
}

/// `osmo_mobile_identity_cmp`: NULL first, then by type, then by TMSI as -1, 0 or 1, or by
/// digits as `strncmp` returns, which glibc gives as the difference of the first differing
/// bytes. Identities of an unknown type are equal.
pub fn osmo_mobile_identity_cmp(
    a: Option<&OsmoMobileIdentity>,
    b: Option<&OsmoMobileIdentity>,
) -> i32 {
    let (a, b) = match (a, b) {
        (None, None) => return 0,
        (None, Some(_)) => return -1,
        (Some(_), None) => return 1,
        (Some(a), Some(b)) => (a, b),
    };
    match a.mi_type.cmp(&b.mi_type) {
        Ordering::Equal => {}
        ordering => return sign(ordering),
    }
    match a.mi_type {
        GSM_MI_TYPE_TMSI => sign(a.tmsi().cmp(&b.tmsi())),
        _ => match digit_type(a.mi_type) {
            Some((_, size)) => strncmp(&a.union, &b.union, size),
            None => 0,
        },
    }
}
