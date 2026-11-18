// SPDX-License-Identifier: GPL-3.0-or-later

//! The identity encodings of 3GPP TS 24.008: the Location Area and Routing Area
//! Identifications, and the Mobile Identity with its IMSI, IMEI, IMEISV or TMSI. Also
//! finding the Mobile Identity in a Complete Layer 3 message.
//!
//! The area identities are [`gsm23003`](super::gsm23003)'s types, which gain byte forms
//! here. A [`MobileIdentity`] holds only identities that encode: its digit strings have a
//! length their type allows and only decimal digits.
//!
//! ```
//! use osmocore_rs::gsm::gsm23003::{Lai, Plmn};
//! use osmocore_rs::gsm::gsm48::MobileIdentity;
//! let lai = Lai { plmn: Plmn::new("262".parse()?, "42".parse()?), lac: 0x1234 };
//! assert_eq!(lai.to_bytes(), [0x62, 0xf2, 0x24, 0x12, 0x34]);
//!
//! let imsi = MobileIdentity::Imsi("262420123456789".parse()?);
//! let encoded = imsi.encode();
//! assert_eq!(encoded, [0x29, 0x26, 0x24, 0x10, 0x32, 0x54, 0x76, 0x98]);
//! assert_eq!(MobileIdentity::decode(&encoded)?, imsi);
//! assert_eq!(imsi.to_string(), "IMSI-262420123456789");
//! assert_eq!(MobileIdentity::Tmsi(0xc0ffee).to_string(), "TMSI-0x00C0FFEE");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use super::gsm23003::{IdentityError, Lai, PLMN_BCD_BYTES, Plmn, Rai};
use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

/// The length of an encoded Location Area Identification (3GPP TS 24.008 section 10.5.1.3).
pub const LAI_BYTES: usize = PLMN_BCD_BYTES + 2;
/// The length of an encoded Routing Area Identification (3GPP TS 24.008 section 10.5.5.15).
pub const RAI_BYTES: usize = LAI_BYTES + 1;
/// The length of an encoded TMSI identity: its first byte and the TMSI.
pub const TMSI_IDENTITY_BYTES: usize = 5;
/// The most bytes an encoded Mobile Identity takes: an IMEISV's first digit with the type,
/// and fifteen digits and a filler.
pub const MOBILE_IDENTITY_MAX_BYTES: usize = 9;

/// The type of identity, in the low three bits of the first byte (3GPP TS 24.008 table
/// 10.5.4).
mod identity_type {
    pub const IMSI: u8 = 1;
    pub const IMEI: u8 = 2;
    pub const IMEISV: u8 = 3;
    pub const TMSI: u8 = 4;
    pub const MASK: u8 = 0x07;
    /// The flag for an odd number of digits.
    pub const ODD: u8 = 0x08;
}

impl Lai {
    /// The PLMN in BCD, then the LAC, most significant byte first. C's
    /// `gsm48_generate_lai2`.
    pub fn to_bytes(&self) -> [u8; LAI_BYTES] {
        let [a, b, c] = self.plmn.to_bcd();
        let [lac_high, lac_low] = self.lac.to_be_bytes();
        [a, b, c, lac_high, lac_low]
    }

    /// Decodes [`to_bytes`](Self::to_bytes)'s form, with [`Plmn::from_bcd`]'s checks. C's
    /// `gsm48_decode_lai2`.
    pub fn from_bytes(bytes: &[u8; LAI_BYTES]) -> Result<Self, IdentityError> {
        let [a, b, c, lac_high, lac_low] = *bytes;
        Ok(Self {
            plmn: Plmn::from_bcd(&[a, b, c])?,
            lac: u16::from_be_bytes([lac_high, lac_low]),
        })
    }
}

impl Rai {
    /// The LAI's bytes, then the RAC. C's `osmo_routing_area_id_encode_buf`.
    pub fn to_bytes(&self) -> [u8; RAI_BYTES] {
        let [a, b, c, d, e] = self.lai.to_bytes();
        [a, b, c, d, e, self.rac]
    }

    /// Decodes [`to_bytes`](Self::to_bytes)'s form. C's `osmo_routing_area_id_decode`.
    pub fn from_bytes(bytes: &[u8; RAI_BYTES]) -> Result<Self, IdentityError> {
        let [a, b, c, d, e, rac] = *bytes;
        Ok(Self {
            lai: Lai::from_bytes(&[a, b, c, d, e])?,
            rac,
        })
    }
}

/// Errors from building, decoding or finding a Mobile Identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobileIdentityError {
    /// Text that isn't decimal digits of a length the identity's type allows.
    Digits,
    /// An encoded identity whose length, filler or digits are wrong for its type, or a
    /// message too short for the identity it carries. C's `-EBADMSG`.
    Malformed,
    /// An encoded identity of a type with no identity: C's `-EINVAL`.
    UnknownType,
    /// A message that isn't a Complete Layer 3 message carrying a Mobile Identity. C's
    /// `-ENOTSUP`.
    NotCarried,
}

impl fmt::Display for MobileIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Digits => "not the digits of this kind of identity",
            Self::Malformed => "malformed Mobile Identity",
            Self::UnknownType => "unknown Mobile Identity type",
            Self::NotCarried => "message does not carry a Mobile Identity",
        })
    }
}

impl Error for MobileIdentityError {}

/// Up to 16 ASCII decimal digits. They order as text does, as C's `strncmp` orders them.
#[derive(Clone, Copy)]
struct Digits {
    len: u8,
    ascii: [u8; 16],
}

impl Digits {
    fn new(digits: &[u8], lengths: std::ops::RangeInclusive<usize>) -> Option<Self> {
        if !lengths.contains(&digits.len()) || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let mut ascii = [0; 16];
        ascii[..digits.len()].copy_from_slice(digits);
        Some(Self {
            len: digits.len() as u8,
            ascii,
        })
    }

    fn as_bytes(&self) -> &[u8] {
        &self.ascii[..usize::from(self.len)]
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).expect("ASCII digits")
    }
}

impl PartialEq for Digits {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for Digits {}

impl Ord for Digits {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for Digits {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for Digits {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl fmt::Debug for Digits {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), formatter)
    }
}

macro_rules! digit_identity {
    ($(#[$doc:meta])* $name:ident, $lengths:expr) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Digits);

        impl $name {
            /// The digits.
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl FromStr for $name {
            type Err = MobileIdentityError;

            fn from_str(text: &str) -> Result<Self, MobileIdentityError> {
                Digits::new(text.as_bytes(), $lengths)
                    .map(Self)
                    .ok_or(MobileIdentityError::Digits)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

digit_identity! {
    /// An IMSI: 6 to 15 decimal digits, the lengths C's decoder takes (3GPP TS 23.003 section
    /// 2.2 allows up to 15).
    Imsi, 6..=15
}

digit_identity! {
    /// An IMEI: 14 digits, or 15 with the check digit. The check digit isn't checked, since
    /// 3GPP TS 24.008 sends it as a spare digit.
    Imei, 14..=15
}

digit_identity! {
    /// An IMEISV: 16 decimal digits (3GPP TS 23.003 section 6.2.2).
    Imeisv, 16..=16
}

/// A Mobile Identity of 3GPP TS 24.008 section 10.5.1.4. It prints as C's
/// `osmo_mobile_identity_to_str_buf` does, and orders as C's `osmo_mobile_identity_cmp`:
/// by type, IMSI first, then by digits as text or by TMSI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MobileIdentity {
    Imsi(Imsi),
    Imei(Imei),
    Imeisv(Imeisv),
    /// A TMSI, P-TMSI or M-TMSI.
    Tmsi(u32),
}

impl MobileIdentity {
    /// The type and digits, or the TMSI.
    fn digits(&self) -> Result<(u8, &[u8]), u32> {
        match self {
            Self::Imsi(imsi) => Ok((identity_type::IMSI, imsi.0.as_bytes())),
            Self::Imei(imei) => Ok((identity_type::IMEI, imei.0.as_bytes())),
            Self::Imeisv(imeisv) => Ok((identity_type::IMEISV, imeisv.0.as_bytes())),
            Self::Tmsi(tmsi) => Err(*tmsi),
        }
    }

    /// How many bytes [`encode`](Self::encode) writes. C's
    /// `osmo_mobile_identity_encoded_len`.
    pub fn encoded_len(&self) -> usize {
        match self.digits() {
            Ok((_, digits)) => (digits.len() + 2) / 2,
            Err(_) => TMSI_IDENTITY_BYTES,
        }
    }

    /// The identity's value bytes, without a tag or length. The first byte holds the first
    /// digit, the odd flag and the type, and later bytes two digits each, low nibble first,
    /// with an F filler after an even count. A TMSI is F4 and its four bytes, most
    /// significant first. C's `osmo_mobile_identity_encode_buf`.
    pub fn encode(&self) -> Vec<u8> {
        let (mi_type, digits) = match self.digits() {
            Ok(digits) => digits,
            Err(tmsi) => {
                let mut encoded = vec![0xf0 | identity_type::TMSI];
                encoded.extend_from_slice(&tmsi.to_be_bytes());
                return encoded;
            }
        };
        let value = |index: usize| digits.get(index).map_or(0x0f, |digit| digit - b'0');
        let odd = if digits.len() % 2 == 1 {
            identity_type::ODD
        } else {
            0
        };
        let mut encoded = vec![value(0) << 4 | odd | mi_type];
        encoded.extend(
            (1..digits.len())
                .step_by(2)
                .map(|i| value(i + 1) << 4 | value(i)),
        );
        encoded
    }

    /// Decodes [`encode`](Self::encode)'s form. It takes exactly what C's
    /// `osmo_mobile_identity_decode` takes without hex digits.
    pub fn decode(bytes: &[u8]) -> Result<Self, MobileIdentityError> {
        let (&first, rest) = bytes.split_first().ok_or(MobileIdentityError::Malformed)?;
        let odd = first & identity_type::ODD != 0;
        let count = rest.len() * 2 + usize::from(odd);
        let mi_type = first & identity_type::MASK;
        if mi_type == identity_type::TMSI {
            return match rest.try_into() {
                Ok(tmsi) if !odd && first >> 4 == 0x0f => Ok(Self::Tmsi(u32::from_be_bytes(tmsi))),
                _ => Err(MobileIdentityError::Malformed),
            };
        }
        let lengths = match mi_type {
            identity_type::IMSI => 6..=15,
            identity_type::IMEI => 14..=15,
            identity_type::IMEISV => 16..=16,
            _ => return Err(MobileIdentityError::UnknownType),
        };
        if !lengths.contains(&count) || (!odd && rest.last().is_some_and(|last| last >> 4 != 0x0f))
        {
            return Err(MobileIdentityError::Malformed);
        }
        let nibbles = std::iter::once(first >> 4)
            .chain(rest.iter().flat_map(|byte| [byte & 0x0f, byte >> 4]));
        let mut ascii = [0; 16];
        for (slot, nibble) in ascii.iter_mut().zip(nibbles).take(count) {
            if nibble > 9 {
                return Err(MobileIdentityError::Malformed);
            }
            *slot = b'0' + nibble;
        }
        let digits = Digits::new(&ascii[..count], lengths).expect("checked digits");
        Ok(match mi_type {
            identity_type::IMSI => Self::Imsi(Imsi(digits)),
            identity_type::IMEI => Self::Imei(Imei(digits)),
            _ => Self::Imeisv(Imeisv(digits)),
        })
    }

    /// Finds and decodes the Mobile Identity of a Complete Layer 3 message: a Location
    /// Updating Request, CM Service Request, CM Re-establishment Request, IMSI Detach
    /// Indication or Identity Response of mobility management, or a Paging Response or
    /// Talker Indication of radio resource management (3GPP TS 24.008 and 44.018). C's
    /// `osmo_mobile_identity_decode_from_l3_buf`, without hex digits.
    pub fn from_l3(l3: &[u8]) -> Result<Self, MobileIdentityError> {
        Self::decode(l3_identity(l3)?)
    }
}

/// The protocol discriminators and message types [`MobileIdentity::from_l3`] reads
/// (3GPP TS 24.007 section 11.2.3.1.1, 24.008 section 10.4 and 44.018 section 10.4).
mod l3 {
    pub const PDISC_MM: u8 = 0x05;
    pub const PDISC_RR: u8 = 0x06;
    pub const MM_IMSI_DETACH_IND: u8 = 0x01;
    pub const MM_LOC_UPD_REQUEST: u8 = 0x08;
    pub const MM_ID_RESP: u8 = 0x19;
    pub const MM_CM_SERV_REQ: u8 = 0x24;
    pub const MM_CM_REEST_REQ: u8 = 0x28;
    pub const RR_TALKER_IND: u8 = 0x11;
    pub const RR_PAG_RESP: u8 = 0x27;
}

/// The Mobile Identity's value bytes in a Complete Layer 3 message, after the header of a
/// protocol discriminator and message type.
fn l3_identity(l3: &[u8]) -> Result<&[u8], MobileIdentityError> {
    use MobileIdentityError::{Malformed, NotCarried};

    let [pdisc, message_type, body @ ..] = l3 else {
        return Err(Malformed);
    };
    // The length byte of the identity, or of the classmark 2 before it.
    let (length_at, after_classmark) = match (pdisc & 0x0f, *message_type) {
        // A mobility management message type's top two bits are the N(SD) sequence number.
        (l3::PDISC_MM, mm) => match mm & 0x3f {
            // Key sequence and LAI, then classmark 1.
            l3::MM_LOC_UPD_REQUEST if body.len() >= 8 => (7, false),
            l3::MM_CM_SERV_REQ | l3::MM_CM_REEST_REQ if body.len() >= 2 => (1, true),
            l3::MM_IMSI_DETACH_IND if body.len() >= 2 => (1, false),
            l3::MM_ID_RESP if body.len() >= 2 => (0, false),
            l3::MM_LOC_UPD_REQUEST
            | l3::MM_CM_SERV_REQ
            | l3::MM_CM_REEST_REQ
            | l3::MM_IMSI_DETACH_IND
            | l3::MM_ID_RESP => return Err(Malformed),
            _ => return Err(NotCarried),
        },
        (l3::PDISC_RR, l3::RR_PAG_RESP) if body.len() >= 6 => (1, true),
        // Classmark 2 is always three bytes here, and an identity of at least one follows.
        (l3::PDISC_RR, l3::RR_TALKER_IND) if body.len() >= 6 && body[0] == 3 => (0, true),
        (l3::PDISC_RR, l3::RR_PAG_RESP | l3::RR_TALKER_IND) => return Err(Malformed),
        _ => return Err(NotCarried),
    };
    let length_at = if after_classmark {
        // Skip the classmark 2 and its length.
        let classmark_len = usize::from(body[length_at]);
        length_at + 1 + classmark_len
    } else {
        length_at
    };
    let length = usize::from(*body.get(length_at).ok_or(Malformed)?);
    body.get(length_at + 1..length_at + 1 + length)
        .ok_or(Malformed)
}

impl fmt::Display for MobileIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Imsi(imsi) => write!(formatter, "IMSI-{imsi}"),
            Self::Imei(imei) => write!(formatter, "IMEI-{imei}"),
            Self::Imeisv(imeisv) => write!(formatter, "IMEI-SV-{imeisv}"),
            Self::Tmsi(tmsi) => write!(formatter, "TMSI-0x{tmsi:08X}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_strings_take_their_type_lengths() {
        assert!("12345".parse::<Imsi>().is_err());
        assert!("123456".parse::<Imsi>().is_ok());
        assert!("1234567890123456".parse::<Imsi>().is_err());
        assert!("1234567890123".parse::<Imei>().is_err());
        assert!("12345678901234".parse::<Imei>().is_ok());
        assert!("123456789012345".parse::<Imeisv>().is_err());
        assert!("12345a".parse::<Imsi>().is_err());
    }

    #[test]
    fn digits_order_as_text() {
        let a: Imsi = "123456".parse().unwrap();
        let b: Imsi = "1234567".parse().unwrap();
        let c: Imsi = "923456".parse().unwrap();
        assert!(a < b && b < c);
    }
}
