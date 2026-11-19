// SPDX-License-Identifier: GPL-3.0-or-later

//! The Network Resource Indicators (NRI) of 3GPP TS 23.236, for MSC and SGSN pooling.
//!
//! An NRI says which core network node of a pool gave out a TMSI or P-TMSI. It is the
//! TMSI's bits from bit 23 down, as many as the pool's NRI length. 3GPP TS 23.003 section 2.4
//! gives it 0 to 10 bits, 0 meaning no pool; libosmocore, and so [`NriBitlen`], takes 1 to
//! 15. Each node owns lists of NRI values, and makes its TMSIs with an NRI from its list.
//!
//! ```
//! use osmocore_rs::gsm::gsm23236::{NriBitlen, NriRange, NriRanges, set_tmsi_nri, tmsi_nri};
//!
//! let bitlen = NriBitlen::new(10).unwrap();
//! assert_eq!(tmsi_nri(0x0123_4567, bitlen), 0x8d);
//! assert_eq!(set_tmsi_nri(0x0123_4567, 0x3ff, bitlen)?, 0x01ff_c567);
//!
//! let mut ranges = NriRanges::new();
//! ranges.add(NriRange::new(10, 10)?);
//! ranges.add(NriRange::parse("20", Some("0x15"))?);
//! assert_eq!(ranges.to_string(), "10..10,20..21");
//!
//! // A random TMSI, given an NRI of this node.
//! let tmsi = ranges.limit_tmsi(0x1234_5678, bitlen).unwrap();
//! assert!(ranges.contains(tmsi_nri(tmsi, bitlen)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::error::Error;
use std::fmt;

/// The largest NRI a range may hold, which C keeps in an `int16_t`.
pub const NRI_MAX: u16 = 0x7fff;

/// The most significant bit of the NRI in a TMSI (3GPP TS 23.003 section 2.4).
const TMSI_NRI_TOP_BIT: u8 = 23;

/// Errors from building or parsing NRI values and ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NriError {
    /// A value past [`NRI_MAX`], or past the bit length it is for.
    TooLarge,
    /// A range whose first value is past its last.
    Reversed,
    /// Text that is not a decimal number or `0x` and a hexadecimal one.
    Syntax,
}

impl fmt::Display for NriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooLarge => "NRI value too large",
            Self::Reversed => "NRI range ends before it starts",
            Self::Syntax => "not an NRI value",
        })
    }
}

impl Error for NriError {}

/// The number of NRI bits in a TMSI: 1 to 15, as libosmocore takes them. 3GPP TS 23.003
/// section 2.4 allows up to 10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NriBitlen(u8);

impl NriBitlen {
    /// The shortest NRI, C's `OSMO_NRI_BITLEN_MIN`.
    pub const MIN: Self = Self(1);
    /// The longest NRI, C's `OSMO_NRI_BITLEN_MAX`.
    pub const MAX: Self = Self(15);
    /// The usual length, C's `OSMO_NRI_BITLEN_DEFAULT`.
    pub const DEFAULT: Self = Self(10);

    /// A bit length, if `bits` is from 1 to 15.
    pub const fn new(bits: u8) -> Option<Self> {
        if bits >= Self::MIN.0 && bits <= Self::MAX.0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    /// The number of bits.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The largest NRI of this many bits.
    pub const fn max_nri(self) -> u16 {
        (1 << self.0) - 1
    }

    /// Whether `nri` fits in this many bits. C's `osmo_nri_v_validate` returning 0.
    pub const fn fits(self, nri: u16) -> bool {
        nri <= self.max_nri()
    }

    /// The lowest bit of the NRI in a TMSI.
    const fn shift(self) -> u8 {
        TMSI_NRI_TOP_BIT + 1 - self.0
    }
}

impl Default for NriBitlen {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The NRI in `tmsi`. C's `osmo_tmsi_nri_v_get`.
pub const fn tmsi_nri(tmsi: u32, bitlen: NriBitlen) -> u16 {
    ((tmsi >> bitlen.shift()) & bitlen.max_nri() as u32) as u16
}

/// `tmsi` with its NRI bits replaced by `nri`. C's `osmo_tmsi_nri_v_set`, which keeps only
/// the bits of `nri` that fit.
pub fn set_tmsi_nri(tmsi: u32, nri: u16, bitlen: NriBitlen) -> Result<u32, NriError> {
    if !bitlen.fits(nri) {
        return Err(NriError::TooLarge);
    }
    let mask = u32::from(bitlen.max_nri()) << bitlen.shift();
    Ok(tmsi & !mask | u32::from(nri) << bitlen.shift())
}

/// An NRI as the VTY takes it: decimal like `511`, or `0x` and hexadecimal like `0x1ff`, no
/// larger than [`NRI_MAX`]. C's parser, which takes `strtoll` input, also skips leading
/// whitespace and takes a sign.
pub fn parse_nri(text: &str) -> Result<u16, NriError> {
    let (digits, radix) = match text.strip_prefix("0x") {
        Some(hex) => (hex, 16),
        None => (text, 10),
    };
    if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
        return Err(NriError::Syntax);
    }
    match u16::from_str_radix(digits, radix) {
        Ok(nri) if nri <= NRI_MAX => Ok(nri),
        _ => Err(NriError::TooLarge),
    }
}

/// The NRI values from `first` to `last`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NriRange {
    first: u16,
    last: u16,
}

impl NriRange {
    /// The values from `first` to `last`, which may be equal, up to [`NRI_MAX`]. C's
    /// `osmo_nri_range_validate` with no bit length.
    pub const fn new(first: u16, last: u16) -> Result<Self, NriError> {
        if last > NRI_MAX {
            Err(NriError::TooLarge)
        } else if first > last {
            Err(NriError::Reversed)
        } else {
            Ok(Self { first, last })
        }
    }

    /// The range of only `nri`.
    pub const fn single(nri: u16) -> Result<Self, NriError> {
        Self::new(nri, nri)
    }

    /// The range of the VTY's `nri add` and `nri del` arguments: `first`, and `last` if given,
    /// each as [`parse_nri`] takes them.
    pub fn parse(first: &str, last: Option<&str>) -> Result<Self, NriError> {
        let first = parse_nri(first)?;
        Self::new(first, last.map_or(Ok(first), parse_nri)?)
    }

    /// The first value.
    pub const fn first(self) -> u16 {
        self.first
    }

    /// The last value.
    pub const fn last(self) -> u16 {
        self.last
    }

    /// Whether the range holds `nri`.
    pub const fn contains(self, nri: u16) -> bool {
        self.first <= nri && nri <= self.last
    }

    /// Whether the ranges share a value.
    pub const fn overlaps(self, other: Self) -> bool {
        self.first <= other.last && other.first <= self.last
    }

    /// Whether every value fits in `bitlen` bits. C's `osmo_nri_range_validate` returning 0.
    pub const fn fits(self, bitlen: NriBitlen) -> bool {
        bitlen.fits(self.last)
    }

    /// Whether the ranges share a value or one starts right after the other.
    const fn touches(self, other: Self) -> bool {
        self.first <= other.last + 1 && other.first <= self.last + 1
    }
}

impl fmt::Display for NriRange {
    /// `first..last`, as C writes each range of a list.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}..{}", self.first, self.last)
    }
}

/// A set of NRI values, as sorted ranges that neither overlap nor touch. C's `struct
/// osmo_nri_ranges`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NriRanges(Vec<NriRange>);

impl NriRanges {
    /// No values.
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Whether there are no values.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The ranges, sorted.
    pub fn ranges(&self) -> &[NriRange] {
        &self.0
    }

    /// Adds the values of `range`, joining the ranges it overlaps or touches. C's
    /// `osmo_nri_ranges_add`.
    pub fn add(&mut self, range: NriRange) {
        let start = self.0.partition_point(|r| r.last + 1 < range.first);
        let end = start + self.0[start..].partition_point(|r| r.touches(range));
        let joined = self.0[start..end].iter().fold(range, |joined, r| NriRange {
            first: joined.first.min(r.first),
            last: joined.last.max(r.last),
        });
        self.0.splice(start..end, [joined]);
    }

    /// Removes the values of `range`, cutting or splitting the ranges it overlaps. C's
    /// `osmo_nri_ranges_del`.
    pub fn remove(&mut self, range: NriRange) {
        let start = self.0.partition_point(|r| r.last < range.first);
        let end = start + self.0[start..].partition_point(|r| r.overlaps(range));
        let (Some(&head), Some(&tail)) = (self.0[start..end].first(), self.0[start..end].last())
        else {
            return;
        };
        let head = (head.first < range.first).then(|| NriRange {
            first: head.first,
            last: range.first - 1,
        });
        let tail = (range.last < tail.last).then(|| NriRange {
            first: range.last + 1,
            last: tail.last,
        });
        self.0.splice(start..end, head.into_iter().chain(tail));
    }

    /// Whether `nri` is one of the values. C's `osmo_nri_v_matches_ranges`.
    pub fn contains(&self, nri: u16) -> bool {
        self.0.iter().any(|range| range.contains(nri))
    }

    /// Whether `range` shares a value with these. C's `osmo_nri_range_overlaps_ranges`.
    pub fn overlaps(&self, range: NriRange) -> bool {
        self.0.iter().any(|r| r.overlaps(range))
    }

    /// One of the values that fit in `bitlen`, picked by `nri`: with N such values in order,
    /// the one at `nri` modulo N. `None` if no value fits, where C divides by zero. C's
    /// `osmo_nri_v_limit_by_ranges`, which turns a random or counted value into an NRI of
    /// this node.
    pub fn limit(&self, nri: u16, bitlen: NriBitlen) -> Option<u16> {
        self.pick(u32::from(nri), Some(bitlen.max_nri()))
    }

    /// `tmsi` with its NRI replaced by [`limit`](Self::limit) of it. C's
    /// `osmo_tmsi_nri_v_limit_by_ranges`.
    pub fn limit_tmsi(&self, tmsi: u32, bitlen: NriBitlen) -> Option<u32> {
        let nri = self.limit(tmsi_nri(tmsi, bitlen), bitlen)?;
        set_tmsi_nri(tmsi, nri, bitlen).ok()
    }

    /// The value at `index` modulo the number of values up to `max`, of which `None` has none.
    pub(crate) fn pick(&self, index: u32, max: Option<u16>) -> Option<u16> {
        let max = max?;
        let within = || {
            self.0
                .iter()
                .take_while(move |r| r.first <= max)
                .map(move |r| (r.first, r.last.min(max)))
        };
        let count: u32 = within()
            .map(|(first, last)| u32::from(last - first) + 1)
            .sum();
        let mut index = index.checked_rem(count)?;
        for (first, last) in within() {
            let len = u32::from(last - first) + 1;
            if index < len {
                return Some(first + index as u16);
            }
            index -= len;
        }
        unreachable!("the index is within the count")
    }
}

impl FromIterator<NriRange> for NriRanges {
    fn from_iter<I: IntoIterator<Item = NriRange>>(ranges: I) -> Self {
        let mut set = Self::new();
        ranges.into_iter().for_each(|range| set.add(range));
        set
    }
}

impl fmt::Display for NriRanges {
    /// `empty`, or the ranges separated by commas, like `23..42,123..142`. C's
    /// `osmo_nri_ranges_to_str_buf` says it writes that, but leaves out the commas.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some((first, rest)) = self.0.split_first() else {
            return formatter.write_str("empty");
        };
        write!(formatter, "{first}")?;
        rest.iter()
            .try_for_each(|range| write!(formatter, ",{range}"))
    }
}
