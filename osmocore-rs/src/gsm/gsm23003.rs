// SPDX-License-Identifier: GPL-3.0-or-later

//! Subscriber and network identities of 3GPP TS 23.003: checks for IMSI, MSISDN and IMEI
//! digits, the PLMN and the area, cell and core network identities built on it, and the
//! home network and MME domain names.
//!
//! An [`Mnc`] keeps whether it has three digits, since `01` and `001` are different MNCs.
//! Identities print in libosmocore's form, MCC and MNC first, and order field by field.
//!
//! ```
//! use osmocore_rs::gsm::gsm23003::{Cgi, Lai, Mcc, Mnc, Plmn};
//! let plmn = Plmn::new("262".parse()?, "42".parse()?);
//! let cgi = Cgi { lai: Lai { plmn, lac: 1 }, ci: 2 };
//! assert_eq!(cgi.to_string(), "262-42-1-2");
//! assert_eq!(plmn.home_network_domain(), "epc.mnc042.mcc262.3gppnetwork.org");
//!
//! let three = Plmn::new(Mcc::new(1).unwrap(), Mnc::new(1, true).unwrap());
//! assert_eq!(three.to_string(), "001-001");
//! assert!(three > Plmn::new(Mcc::new(1).unwrap(), Mnc::new(1, false).unwrap()));
//! # Ok::<(), osmocore_rs::gsm::gsm23003::IdentityError>(())
//! ```

use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// The fewest digits an IMSI check accepts: an MCC, a two-digit MNC and one MSIN digit.
pub const IMSI_MIN_DIGITS: usize = 6;
/// The most digits in an IMSI (3GPP TS 23.003 section 2.2).
pub const IMSI_MAX_DIGITS: usize = 15;
/// The fewest digits an MSISDN check accepts.
pub const MSISDN_MIN_DIGITS: usize = 1;
/// The most digits in an MSISDN (ITU-T E.164 section 6.1, by 3GPP TS 23.003 section 3.3).
pub const MSISDN_MAX_DIGITS: usize = 15;
/// The digits of an IMEI: an 8-digit TAC, a 6-digit SNR and a check digit (3GPP TS 23.003
/// section 6.2.1).
pub const IMEI_DIGITS: usize = 15;
/// The length of a home network domain name (3GPP TS 23.003 section 19.2).
pub const HOME_NETWORK_DOMAIN_LEN: usize = 33;
/// The length of an MME domain name (3GPP TS 23.003 section 19.4.2.4).
pub const MME_DOMAIN_LEN: usize = 54;

/// The suffix of every domain name under the 3GPP network domain (3GPP TS 23.003 section
/// 19.2).
const NETWORK_DOMAIN: &str = "3gppnetwork.org";

/// Errors from parsing an identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// MCC or MNC text that isn't one to three decimal digits.
    Digits,
    /// A domain name that isn't in the form 3GPP TS 23.003 gives.
    Domain,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Digits => "MCC or MNC is not one to three decimal digits",
            Self::Domain => "not a 3GPP TS 23.003 domain name",
        })
    }
}

impl Error for IdentityError {}

fn all_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}

fn digits_in(text: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&text.len()) && all_digits(text)
}

/// Whether text is an IMSI: six to fifteen decimal digits. C's `osmo_imsi_str_valid`.
pub fn is_valid_imsi(digits: &str) -> bool {
    digits_in(digits, IMSI_MIN_DIGITS, IMSI_MAX_DIGITS)
}

/// Whether text is an MSISDN: one to fifteen decimal digits. C's `osmo_msisdn_str_valid`.
pub fn is_valid_msisdn(digits: &str) -> bool {
    digits_in(digits, MSISDN_MIN_DIGITS, MSISDN_MAX_DIGITS)
}

/// Whether text is an IMEI with its check digit: fifteen decimal digits, the last of which
/// is the Luhn check digit of the first fourteen (3GPP TS 23.003 section 6.2.1 and Annex
/// B). C's `osmo_imei_str_valid` with `with_15th_digit`.
pub fn is_valid_imei(digits: &str) -> bool {
    match digits.as_bytes().split_last() {
        Some((&check, body)) if digits.len() == IMEI_DIGITS && all_digits(digits) => {
            luhn_check_digit(body) == check - b'0'
        }
        _ => false,
    }
}

/// Whether text is an IMEI without its check digit: fourteen decimal digits, as it's
/// transmitted (3GPP TS 23.003 section 6.2.1). C's `osmo_imei_str_valid` without
/// `with_15th_digit`.
pub fn is_valid_imei_without_check_digit(digits: &str) -> bool {
    digits_in(digits, IMEI_DIGITS - 1, IMEI_DIGITS - 1)
}

/// The Luhn check digit of decimal digits: double every second digit from the right, sum
/// the digits of every result, and take what brings the sum to a multiple of ten.
fn luhn_check_digit(digits: &[u8]) -> u8 {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(position, &digit)| {
            let value = u32::from(digit - b'0');
            match position % 2 {
                0 if value > 4 => value * 2 - 9,
                0 => value * 2,
                _ => value,
            }
        })
        .sum();
    ((10 - sum % 10) % 10) as u8
}

/// Parses one to three decimal digits, returning the value and how many digits there were.
fn three_digits(text: &str) -> Result<(u16, usize), IdentityError> {
    if text.is_empty() || text.len() > 3 || !all_digits(text) {
        return Err(IdentityError::Digits);
    }
    let value = text
        .bytes()
        .fold(0, |value, digit| value * 10 + u16::from(digit - b'0'));
    Ok((value, text.len()))
}

/// A Mobile Country Code, from 0 to 999. It prints as three digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mcc(u16);

impl Mcc {
    /// The MCC, or `None` past 999.
    pub const fn new(value: u16) -> Option<Self> {
        if value <= 999 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Mcc {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:03}", self.0)
    }
}

/// Parses one to three decimal digits, with or without leading zeros. C's
/// `osmo_mcc_from_str`.
impl FromStr for Mcc {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Self, IdentityError> {
        three_digits(text).map(|(value, _)| Self(value))
    }
}

/// A Mobile Network Code, from 0 to 999, and whether it has three digits.
///
/// An MNC under 100 can have two digits or three, and the two are different MNCs: `01`
/// isn't `001`. An MNC from 100 on always has three. MNCs order by value, then two digits
/// before three, as C's `osmo_mnc_cmp` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mnc {
    value: u16,
    three_digits: bool,
}

impl Mnc {
    /// The MNC, or `None` past 999. `three_digits` matters only under 100.
    pub const fn new(value: u16, three_digits: bool) -> Option<Self> {
        if value <= 999 {
            Some(Self {
                value,
                three_digits: three_digits || value > 99,
            })
        } else {
            None
        }
    }

    pub const fn value(self) -> u16 {
        self.value
    }

    pub const fn is_three_digits(self) -> bool {
        self.three_digits
    }
}

impl fmt::Display for Mnc {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.three_digits {
            write!(formatter, "{:03}", self.value)
        } else {
            write!(formatter, "{:02}", self.value)
        }
    }
}

/// Parses one to three decimal digits. Three digits give a three-digit MNC, so `"001"`
/// isn't `"01"`. C's `osmo_mnc_from_str`.
impl FromStr for Mnc {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Self, IdentityError> {
        let (value, digits) = three_digits(text)?;
        Ok(Self {
            value,
            three_digits: digits == 3,
        })
    }
}

/// A Public Land Mobile Network identity: MCC and MNC (3GPP TS 23.003 section 12.1). It
/// prints as `MCC-MNC`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Plmn {
    pub mcc: Mcc,
    pub mnc: Mnc,
}

impl Plmn {
    pub const fn new(mcc: Mcc, mnc: Mnc) -> Self {
        Self { mcc, mnc }
    }

    /// The home network domain, `epc.mncMNC.mccMCC.3gppnetwork.org` (3GPP TS 23.003 section
    /// 19.2). A two-digit MNC gets a leading zero, so `01` and `001` give the same domain.
    pub fn home_network_domain(&self) -> String {
        format!(
            "epc.mnc{:03}.mcc{:03}.{NETWORK_DOMAIN}",
            self.mnc.value, self.mcc.0
        )
    }

    /// Parses a home network domain, which must be exactly in the form
    /// [`home_network_domain`](Self::home_network_domain) writes. The MNC has two digits
    /// under 100, since the domain can't tell. C's `osmo_parse_home_network_domain`.
    pub fn from_home_network_domain(domain: &str) -> Result<Self, IdentityError> {
        let rest = domain.strip_prefix("epc.").ok_or(IdentityError::Domain)?;
        let (plmn, rest) = domain_plmn(rest)?;
        if rest == NETWORK_DOMAIN {
            Ok(plmn)
        } else {
            Err(IdentityError::Domain)
        }
    }

    /// The domain of an MME group in this network,
    /// `mmegiMMEGI.mme.epc.mncMNC.mccMCC.3gppnetwork.org` (3GPP TS 23.003 section
    /// 19.4.2.4). C's `osmo_gen_mme_group_domain`.
    pub fn mme_group_domain(&self, group_id: u16) -> String {
        format!("mmegi{group_id:04x}.mme.{}", self.home_network_domain())
    }
}

impl fmt::Display for Plmn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.mcc, self.mnc)
    }
}

/// Takes `LABELddd.` from the start of a domain, with exactly three decimal digits.
fn domain_field<'a>(text: &'a str, label: &str) -> Result<(u16, &'a str), IdentityError> {
    let rest = text.strip_prefix(label).ok_or(IdentityError::Domain)?;
    let (digits, rest) = rest.split_at_checked(3).ok_or(IdentityError::Domain)?;
    match (three_digits(digits), rest.strip_prefix('.')) {
        (Ok((value, 3)), Some(rest)) => Ok((value, rest)),
        _ => Err(IdentityError::Domain),
    }
}

/// Takes `mncMNC.mccMCC.` from the start of a domain.
fn domain_plmn(text: &str) -> Result<(Plmn, &str), IdentityError> {
    let (mnc, rest) = domain_field(text, "mnc")?;
    let (mcc, rest) = domain_field(rest, "mcc")?;
    let mnc = Mnc::new(mnc, false).ok_or(IdentityError::Domain)?;
    Ok((Plmn::new(Mcc(mcc), mnc), rest))
}

/// Takes exactly `digits` hex digits of either case.
fn hex_field(text: &str, digits: usize) -> Result<u16, IdentityError> {
    if text.len() != digits || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(IdentityError::Domain);
    }
    u16::from_str_radix(text, 16).map_err(|_| IdentityError::Domain)
}

/// A Location Area Identity: PLMN and LAC (3GPP TS 23.003 section 4.1). It prints as
/// `MCC-MNC-LAC`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lai {
    pub plmn: Plmn,
    pub lac: u16,
}

impl fmt::Display for Lai {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.plmn, self.lac)
    }
}

/// A Routing Area Identity: LAI and RAC (3GPP TS 23.003 section 4.2). It prints as
/// `MCC-MNC-LAC-RAC`, C's `osmo_rai_name2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rai {
    pub lai: Lai,
    pub rac: u8,
}

impl fmt::Display for Rai {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.lai, self.rac)
    }
}

/// A Cell Global Identity: LAI and cell identity (3GPP TS 23.003 section 4.3.1). It prints
/// as `MCC-MNC-LAC-CI`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cgi {
    pub lai: Lai,
    pub ci: u16,
}

impl fmt::Display for Cgi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.lai, self.ci)
    }
}

/// A cell's identity in the packet domain: RAI and cell identity (3GPP TS 48.018 section
/// 11.3.9). It prints as `MCC-MNC-LAC-RAC-CI`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CgiPs {
    pub rai: Rai,
    pub ci: u16,
}

impl fmt::Display for CgiPs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.rai, self.ci)
    }
}

/// A Service Area Identity: LAI and SAC (3GPP TS 23.003 section 12.5). It prints as
/// `MCC-MNC-LAC-SAC`. libosmocore has no order for it, so it has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sai {
    pub lai: Lai,
    pub sac: u16,
}

impl fmt::Display for Sai {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.lai, self.sac)
    }
}

/// A Radio Network Controller's identity: PLMN and RNC ID (3GPP TS 23.003 section 12.4).
/// It prints as `MCC-MNC-RNCID`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RncId {
    pub plmn: Plmn,
    pub rnc_id: u16,
}

impl fmt::Display for RncId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.plmn, self.rnc_id)
    }
}

/// An MME's identity within its PLMN: MME group ID and MME code (3GPP TS 23.003 section
/// 2.8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MmeId {
    pub group_id: u16,
    pub code: u8,
}

/// A Globally Unique MME Identifier: PLMN and MME ID (3GPP TS 23.003 section 2.8.1). It
/// prints as `MCC-MNC-MMEGI-MMEC`, with the MME group ID and code in four and two
/// lower-case hex digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Gummei {
    pub plmn: Plmn,
    pub mme: MmeId,
}

impl Gummei {
    /// The MME's domain, `mmecMMEC.mmegiMMEGI.mme.epc.mncMNC.mccMCC.3gppnetwork.org` (3GPP
    /// TS 23.003 section 19.4.2.4). C's `osmo_gen_mme_domain`.
    pub fn mme_domain(&self) -> String {
        format!(
            "mmec{:02x}.{}",
            self.mme.code,
            self.plmn.mme_group_domain(self.mme.group_id)
        )
    }

    /// Parses an MME domain, which must be in the form [`mme_domain`](Self::mme_domain)
    /// writes, though its hex digits may be upper case. The MNC has two digits under 100,
    /// since the domain can't tell. C's `osmo_parse_mme_domain`.
    pub fn from_mme_domain(domain: &str) -> Result<Self, IdentityError> {
        let rest = domain.strip_prefix("mmec").ok_or(IdentityError::Domain)?;
        let (code, rest) = rest.split_at_checked(2).ok_or(IdentityError::Domain)?;
        let rest = rest.strip_prefix(".mmegi").ok_or(IdentityError::Domain)?;
        let (group_id, rest) = rest.split_at_checked(4).ok_or(IdentityError::Domain)?;
        let rest = rest.strip_prefix(".mme.").ok_or(IdentityError::Domain)?;
        let plmn = Plmn::from_home_network_domain(rest)?;
        Ok(Self {
            plmn,
            mme: MmeId {
                group_id: hex_field(group_id, 4)?,
                code: hex_field(code, 2)? as u8,
            },
        })
    }
}

impl fmt::Display for Gummei {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}-{:04x}-{:02x}",
            self.plmn, self.mme.group_id, self.mme.code
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnc_from_100_always_has_three_digits() {
        assert_eq!(Mnc::new(100, false), Mnc::new(100, true));
        assert_eq!(Mnc::new(100, false).unwrap().to_string(), "100");
        assert_eq!(Mnc::new(1000, true), None);
    }
}
