// SPDX-License-Identifier: GPL-3.0-or-later

//! C's 3GPP TS 23.236 NRI functions, with C's `int16_t` values and status codes, its
//! truncating TMSI writes, its bit lengths of 0 and past 15, its overlap rule for invalid
//! ranges, its list text without commas, its `strtoll` VTY parser and its VTY messages.
//!
//! The preferred API is [`crate::gsm::gsm23236`], whose values and ranges are valid by
//! construction.
//!
//! `osmo_nri_ranges_alloc` and `osmo_nri_ranges_free` have no counterpart: an
//! [`OsmoNriRanges`](crate::compat::gsm23236::OsmoNriRanges) is an owned value. C's
//! functions take a NULL list as an empty one, which a reference can't be.

use crate::gsm::gsm23236::{NRI_MAX, NriRange, NriRanges};

/// C's `OSMO_NRI_BITLEN_MIN`.
pub const OSMO_NRI_BITLEN_MIN: u8 = 1;
/// C's `OSMO_NRI_BITLEN_MAX`.
pub const OSMO_NRI_BITLEN_MAX: u8 = 15;
/// C's `OSMO_NRI_BITLEN_DEFAULT`.
pub const OSMO_NRI_BITLEN_DEFAULT: u8 = 10;

const TMSI_NRI_TOP_BIT: u32 = 23;
const MESSAGE_PARSE: &str = "Error: cannot parse NRI range";
const MESSAGE_ADD: &str = "Error: failed to add NRI range";
const MESSAGE_DEL: &str = "Error: failed to remove NRI range";
const MESSAGE_BITLEN: &str = "Warning: NRI range surpasses current NRI bitlen";

/// C's `struct osmo_nri_range`: NRI values from `first` to `last`, both included. Any
/// values may be set; the functions check them as C does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct OsmoNriRange {
    pub first: i16,
    pub last: i16,
}

impl OsmoNriRange {
    /// The range if it is valid, which `osmo_nri_range_validate` with a bit length of 255
    /// returning 0 says.
    fn checked(self) -> Option<NriRange> {
        let first = u16::try_from(self.first).ok()?;
        let last = u16::try_from(self.last).ok()?;
        NriRange::new(first, last).ok()
    }

    /// C's rule: a value of either range lies within the other. A range whose first value is
    /// past its last holds no values, but still overlaps a range that holds one of its ends.
    fn overlaps(self, other: Self) -> bool {
        let holds = |range: Self, value: i16| range.first <= value && value <= range.last;
        holds(other, self.first)
            || holds(other, self.last)
            || holds(self, other.first)
            || holds(self, other.last)
    }
}

impl From<NriRange> for OsmoNriRange {
    fn from(range: NriRange) -> Self {
        // Ranges hold values up to NRI_MAX, which fit an i16.
        Self {
            first: range.first() as i16,
            last: range.last() as i16,
        }
    }
}

/// C's `struct osmo_nri_ranges`: a list that [`osmo_nri_ranges_add`] and
/// [`osmo_nri_ranges_del`] keep sorted, with ranges that neither overlap nor touch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct OsmoNriRanges(NriRanges);

impl OsmoNriRanges {
    /// An empty list.
    pub const fn new() -> Self {
        Self(NriRanges::new())
    }

    /// The entries in list order.
    pub fn entries(&self) -> Vec<OsmoNriRange> {
        self.0.ranges().iter().copied().map(Into::into).collect()
    }
}

impl From<NriRanges> for OsmoNriRanges {
    fn from(ranges: NriRanges) -> Self {
        Self(ranges)
    }
}

/// The largest value of `nri_bitlen` bits, as C computes it in an `int16_t`: none past 15
/// bits, where the value wraps to -1. Past 30 bits, C's shift overflows an `int`; this
/// takes those as it does 16 to 30.
fn range_max(nri_bitlen: u32) -> Option<u16> {
    (nri_bitlen <= u32::from(OSMO_NRI_BITLEN_MAX)).then(|| ((1u32 << nri_bitlen) - 1) as u16)
}

/// `osmo_nri_v_validate`: -1 for a negative value, 1 for one past `nri_bitlen` bits or for
/// a bit length of 0, and 0 otherwise. From 15 bits on only the sign is checked.
pub fn osmo_nri_v_validate(nri_v: i16, nri_bitlen: u8) -> i32 {
    if nri_v < 0 {
        -1
    } else if nri_bitlen < OSMO_NRI_BITLEN_MIN
        || (nri_bitlen < OSMO_NRI_BITLEN_MAX && nri_v >> nri_bitlen != 0)
    {
        1
    } else {
        0
    }
}

/// `osmo_nri_v_matches_ranges`: whether a range of the list holds `nri_v`.
pub fn osmo_nri_v_matches_ranges(nri_v: i16, nri_ranges: &OsmoNriRanges) -> bool {
    u16::try_from(nri_v).is_ok_and(|nri| nri_ranges.0.contains(nri))
}

/// `osmo_nri_v_limit_by_ranges`: replaces `nri_v` with one of the list's values that fit in
/// `nri_bitlen` bits, the one at `nri_v` modulo their number. A bit length of 0 leaves only
/// the value 0, and past 15 none. Returns -1 for a negative `nri_v`, which is kept.
///
/// Where no value fits, C divides by zero and the process ends by `SIGFPE`; this returns -1
/// with `nri_v` kept.
pub fn osmo_nri_v_limit_by_ranges(
    nri_v: &mut i16,
    nri_ranges: &OsmoNriRanges,
    nri_bitlen: u32,
) -> i32 {
    let Ok(index) = u32::try_from(*nri_v) else {
        return -1;
    };
    match nri_ranges.0.pick(index, range_max(nri_bitlen)) {
        Some(nri) => {
            *nri_v = nri as i16;
            0
        }
        None => -1,
    }
}

/// The lowest bit of the NRI in a TMSI, for a valid bit length.
fn nri_shift(nri_bitlen: u8) -> Option<u32> {
    (OSMO_NRI_BITLEN_MIN..=OSMO_NRI_BITLEN_MAX)
        .contains(&nri_bitlen)
        .then(|| TMSI_NRI_TOP_BIT + 1 - u32::from(nri_bitlen))
}

/// `osmo_tmsi_nri_v_get`: writes the NRI of `nri_bitlen` bits in `tmsi` to `nri_v`. Writes
/// -1 and returns -1 for a bit length other than 1 to 15.
pub fn osmo_tmsi_nri_v_get(nri_v: &mut i16, tmsi: u32, nri_bitlen: u8) -> i32 {
    *nri_v = -1;
    let Some(shift) = nri_shift(nri_bitlen) else {
        return -1;
    };
    *nri_v = ((tmsi >> shift) & ((1 << nri_bitlen) - 1)) as i16;
    0
}

/// `osmo_tmsi_nri_v_set`: replaces the NRI bits of `tmsi` with as many of the low bits of
/// `nri_v` as fit. Returns -1 with `tmsi` kept for a bit length other than 1 to 15 or a
/// negative `nri_v`.
pub fn osmo_tmsi_nri_v_set(tmsi: &mut u32, nri_v: i16, nri_bitlen: u8) -> i32 {
    let Some(shift) = nri_shift(nri_bitlen) else {
        return -1;
    };
    if nri_v < 0 {
        return -1;
    }
    let mask = ((1u32 << nri_bitlen) - 1) << shift;
    *tmsi = *tmsi & !mask | (nri_v as u32) << shift & mask;
    0
}

/// `osmo_tmsi_nri_v_limit_by_ranges`: [`osmo_nri_v_limit_by_ranges`] on the NRI in `tmsi`,
/// written back. Returns -1 with `tmsi` kept for a bit length other than 1 to 15, or where
/// no value of the list fits, where C divides by zero.
pub fn osmo_tmsi_nri_v_limit_by_ranges(
    tmsi: &mut u32,
    nri_ranges: &OsmoNriRanges,
    nri_bitlen: u8,
) -> i32 {
    let mut nri_v = 0;
    let rc = osmo_tmsi_nri_v_get(&mut nri_v, *tmsi, nri_bitlen);
    if rc != 0 {
        return rc;
    }
    let rc = osmo_nri_v_limit_by_ranges(&mut nri_v, nri_ranges, u32::from(nri_bitlen));
    if rc != 0 {
        return rc;
    }
    osmo_tmsi_nri_v_set(tmsi, nri_v, nri_bitlen)
}

/// `osmo_nri_range_validate`: [`osmo_nri_v_validate`] of `first`, then of `last` doubled,
/// then -3 if `first` is past `last`.
pub fn osmo_nri_range_validate(range: &OsmoNriRange, nri_bitlen: u8) -> i32 {
    match osmo_nri_v_validate(range.first, nri_bitlen) {
        0 => {}
        rc => return rc,
    }
    match osmo_nri_v_validate(range.last, nri_bitlen) {
        0 => {}
        rc => return 2 * rc,
    }
    if range.first > range.last { -3 } else { 0 }
}

/// `osmo_nri_range_overlaps_ranges`: whether `range` and a range of the list overlap, by
/// C's rule for ranges whose first value is past their last.
pub fn osmo_nri_range_overlaps_ranges(range: &OsmoNriRange, nri_ranges: &OsmoNriRanges) -> bool {
    nri_ranges
        .entries()
        .into_iter()
        .any(|entry| entry.overlaps(*range))
}

/// `osmo_nri_ranges_add`: adds the values of `add`, joining the entries it overlaps or
/// touches. Returns -1 with the list kept for a negative value or `first` past `last`.
pub fn osmo_nri_ranges_add(nri_ranges: &mut OsmoNriRanges, add: &OsmoNriRange) -> i32 {
    match add.checked() {
        Some(range) => {
            nri_ranges.0.add(range);
            0
        }
        None => -1,
    }
}

/// `osmo_nri_ranges_del`: removes the values of `del`, cutting or splitting the entries it
/// overlaps. Returns -1 with the list kept for a negative value or `first` past `last`.
pub fn osmo_nri_ranges_del(nri_ranges: &mut OsmoNriRanges, del: &OsmoNriRange) -> i32 {
    match del.checked() {
        Some(range) => {
            nri_ranges.0.remove(range);
            0
        }
        None => -1,
    }
}

/// C's VTY parser of one NRI: `strtoll` of a decimal number, or of a hexadecimal one after
/// `0x`, with nothing after it, from 0 to `INT16_MAX`. `strtoll` skips leading whitespace and
/// takes a sign, which only a decimal number can have here.
fn parse_nri(text: &[u8]) -> Option<i16> {
    let text = text.split(|&byte| byte == 0).next().unwrap_or_default();
    let (digits, radix, negative) = match text.strip_prefix(b"0x") {
        Some(hex) => (hex, 16, false),
        None => {
            let start = text
                .iter()
                .position(|byte| !b" \t\n\x0b\x0c\r".contains(byte))
                .unwrap_or(text.len());
            match &text[start..] {
                [b'-', rest @ ..] => (rest, 10, true),
                [b'+', rest @ ..] => (rest, 10, false),
                rest => (rest, 10, false),
            }
        }
    };
    if digits.is_empty()
        || !digits
            .iter()
            .all(|&digit| char::from(digit).is_digit(radix))
    {
        return None;
    }
    let digits = std::str::from_utf8(digits).ok()?;
    let value = u16::from_str_radix(digits, radix).ok()?;
    match (negative, value) {
        (true, 0) => Some(0),
        (false, value) if value <= NRI_MAX => Some(value as i16),
        _ => None,
    }
}

/// C's VTY parser of a range: `first`, or `last` alone when there is no `first`, then `last`
/// if both are given.
fn parse_range(first: Option<&[u8]>, last: Option<&[u8]>) -> Option<OsmoNriRange> {
    let (first, last) = match (first, last) {
        (Some(first), last) => (first, last),
        (None, Some(last)) => (last, None),
        (None, None) => return None,
    };
    let first = parse_nri(first)?;
    let last = last.map_or(Some(first), parse_nri)?;
    Some(OsmoNriRange { first, last })
}

/// The arguments C's VTY functions read: `argv[0]`, and `argv[1]` for an `argc` past 1.
/// With no arguments C reads past `argv`; this takes it as a missing first argument.
fn vty_args<'a>(argv: &[Option<&'a [u8]>]) -> (Option<&'a [u8]>, Option<&'a [u8]>) {
    (
        argv.first().copied().flatten(),
        argv.get(1).copied().flatten(),
    )
}

/// `osmo_nri_ranges_vty_add`: parses `argv` (C's `argc` is its length) into a range, writes
/// it to `added_range`, and adds it to the list. Each value is decimal, or hexadecimal after
/// `0x`. Returns -1 with a message for text it can't parse, leaving `added_range` alone, or
/// for a range it can't add. Returns 1 with a warning, after adding the range, for one past
/// `nri_bitlen` bits when that is at most 15. Otherwise sets `message` to `None` and
/// returns 0.
pub fn osmo_nri_ranges_vty_add(
    message: &mut Option<&'static str>,
    added_range: Option<&mut OsmoNriRange>,
    nri_ranges: &mut OsmoNriRanges,
    argv: &[Option<&[u8]>],
    nri_bitlen: u8,
) -> i32 {
    let (first, last) = vty_args(argv);
    let Some(range) = parse_range(first, last) else {
        *message = Some(MESSAGE_PARSE);
        return -1;
    };
    if let Some(added_range) = added_range {
        *added_range = range;
    }
    if osmo_nri_ranges_add(nri_ranges, &range) != 0 {
        *message = Some(MESSAGE_ADD);
        return -1;
    }
    if nri_bitlen <= OSMO_NRI_BITLEN_MAX && osmo_nri_range_validate(&range, nri_bitlen) != 0 {
        *message = Some(MESSAGE_BITLEN);
        return 1;
    }
    *message = None;
    0
}

/// `osmo_nri_ranges_vty_del`: as [`osmo_nri_ranges_vty_add`], removing the range from the
/// list, with no bit length to warn about.
pub fn osmo_nri_ranges_vty_del(
    message: &mut Option<&'static str>,
    removed_range: Option<&mut OsmoNriRange>,
    nri_ranges: &mut OsmoNriRanges,
    argv: &[Option<&[u8]>],
) -> i32 {
    let (first, last) = vty_args(argv);
    let Some(range) = parse_range(first, last) else {
        *message = Some(MESSAGE_PARSE);
        return -1;
    };
    if let Some(removed_range) = removed_range {
        *removed_range = range;
    }
    if osmo_nri_ranges_del(nri_ranges, &range) != 0 {
        *message = Some(MESSAGE_DEL);
        return -1;
    }
    *message = None;
    0
}

/// The text C writes for the list: `empty`, or each entry as `first..last`, with no
/// separator.
fn c_text(nri_ranges: &OsmoNriRanges) -> String {
    if nri_ranges.0.is_empty() {
        return "empty".to_owned();
    }
    nri_ranges
        .0
        .ranges()
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// `osmo_nri_ranges_to_str_buf`: writes C's text of the list into `buf` as `snprintf` does,
/// as much as fits before a NUL, and returns its full length. C's documentation gives the
/// text as `23..42,123..142`, but no commas are written: that list gives `23..42123..142`.
/// [`NriRanges`]'s `Display` writes the commas.
pub fn osmo_nri_ranges_to_str_buf(buf: &mut [u8], nri_ranges: &OsmoNriRanges) -> i32 {
    let text = c_text(nri_ranges);
    if let Some(room) = buf.len().checked_sub(1) {
        let len = text.len().min(room);
        buf[..len].copy_from_slice(&text.as_bytes()[..len]);
        buf[len] = 0;
    }
    text.len() as i32
}

/// `osmo_nri_ranges_to_str_c`: C's text of the list, as [`osmo_nri_ranges_to_str_buf`]
/// writes it, with no commas.
pub fn osmo_nri_ranges_to_str_c(nri_ranges: &OsmoNriRanges) -> String {
    c_text(nri_ranges)
}
