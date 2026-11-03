// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM bands, ARFCNs, power levels, GSM time, reduced frame numbers, TLLIs,
//! classmarks, RACH parameters and the 7-bit default alphabet.
//!
//! ```
//! use osmocore_rs::gsm_utils::{Band, Direction, GsmTime, arfcn_to_freq10};
//! assert_eq!(Band::parse("DCS1800"), Some(Band::Dcs1800));
//! assert_eq!(Band::from_arfcn(512), Some(Band::Dcs1800));
//! assert_eq!(arfcn_to_freq10(0, Direction::Downlink), Some(9350));
//! assert_eq!(Band::Gsm900.ms_power_dbm(5), Some(33));
//!
//! let time = GsmTime::from_frame(1327).unwrap();
//! assert_eq!((time.t1(), time.t2(), time.t3()), (1, 1, 1));
//! assert_eq!(GsmTime::from_parts(1, 1, 1), Some(time));
//! ```
//!
//! ```
//! use osmocore_rs::gsm_utils::{decode_7bit, encode_7bit};
//! let packed = encode_7bit(b"test text", 140);
//! assert_eq!(packed.septets, 9);
//! assert_eq!(decode_7bit(&packed.octets, packed.septets).unwrap(), b"test text");
//! ```

use crate::gsm0502::{TDMA_HYPERFRAME, TDMA_SUPERFRAME, frame_add, frame_distance, frame_sub};
use std::fmt;

/// Frame number meaning "not set". It isn't a valid frame.
pub const FN_UNSET: u32 = 0xffff_ffff;
/// Reduced frame numbers wrap here.
pub const RFN_MODULUS: u16 = 42432;
/// ARFCN flag selecting the PCS 1900 band.
pub const ARFCN_PCS: u16 = 0x8000;
/// ARFCN flag marking an uplink channel.
pub const ARFCN_UPLINK: u16 = 0x4000;
/// ARFCN bits reserved for flags.
pub const ARFCN_FLAG_MASK: u16 = 0xf000;

/// A GSM frequency band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Band {
    /// GSM 850.
    Gsm850,
    /// P-GSM, E-GSM and R-GSM 900.
    Gsm900,
    /// DCS 1800.
    Dcs1800,
    /// PCS 1900.
    Pcs1900,
    /// GSM 450.
    Gsm450,
    /// GSM 480.
    Gsm480,
    /// GSM 750.
    Gsm750,
    /// GSM 810.
    Gsm810,
}

/// Whether a frequency is on the uplink or the downlink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Mobile to network.
    Uplink,
    /// Network to mobile.
    Downlink,
}

impl Band {
    /// Every band.
    pub const ALL: [Self; 8] = [
        Self::Gsm850,
        Self::Gsm900,
        Self::Dcs1800,
        Self::Pcs1900,
        Self::Gsm450,
        Self::Gsm480,
        Self::Gsm750,
        Self::Gsm810,
    ];

    /// Returns the band's bit value in C's `enum gsm_band`.
    pub const fn code(self) -> i32 {
        match self {
            Self::Gsm850 => 0x01,
            Self::Gsm900 => 0x02,
            Self::Dcs1800 => 0x04,
            Self::Pcs1900 => 0x08,
            Self::Gsm450 => 0x10,
            Self::Gsm480 => 0x20,
            Self::Gsm750 => 0x40,
            Self::Gsm810 => 0x80,
        }
    }

    /// Returns the band with a C `enum gsm_band` value.
    pub const fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0x01 => Self::Gsm850,
            0x02 => Self::Gsm900,
            0x04 => Self::Dcs1800,
            0x08 => Self::Pcs1900,
            0x10 => Self::Gsm450,
            0x20 => Self::Gsm480,
            0x40 => Self::Gsm750,
            0x80 => Self::Gsm810,
            _ => return None,
        })
    }

    /// Returns the band's name, such as `"DCS1800"`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gsm450 => "GSM450",
            Self::Gsm480 => "GSM480",
            Self::Gsm750 => "GSM750",
            Self::Gsm810 => "GSM810",
            Self::Gsm850 => "GSM850",
            Self::Gsm900 => "GSM900",
            Self::Dcs1800 => "DCS1800",
            Self::Pcs1900 => "PCS1900",
        }
    }

    /// Finds a band from the first run of digits in `text`, so `"900"`, `"GSM900"`
    /// and `"E-GSM900MHz"` all name GSM 900. Text after a NUL is ignored, as C would
    /// never see it.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.split('\0').next().unwrap_or_default();
        let start = text.find(|c: char| c.is_ascii_digit())?;
        let digits = &text[start..];
        let end = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        Some(match digits[..end].parse::<u64>().ok()? {
            450 => Self::Gsm450,
            480 => Self::Gsm480,
            750 => Self::Gsm750,
            810 => Self::Gsm810,
            850 => Self::Gsm850,
            900 => Self::Gsm900,
            1800 => Self::Dcs1800,
            1900 => Self::Pcs1900,
            _ => return None,
        })
    }

    /// Returns the band an ARFCN belongs to. The [`ARFCN_PCS`] flag selects PCS 1900
    /// whatever the channel number is, and other flags are ignored.
    pub const fn from_arfcn(arfcn: u16) -> Option<Self> {
        if arfcn & ARFCN_PCS != 0 {
            return Some(Self::Pcs1900);
        }
        Some(match arfcn & !ARFCN_FLAG_MASK {
            0..=124 | 940..=1023 => Self::Gsm900,
            128..=251 => Self::Gsm850,
            512..=885 => Self::Dcs1800,
            259..=293 => Self::Gsm450,
            306..=340 => Self::Gsm480,
            350..=425 => Self::Gsm810,
            438..=511 => Self::Gsm750,
            _ => return None,
        })
    }

    /// Returns the maximum GMSK power of a mobile power class in dBm, or `None` if
    /// the band has no such class.
    pub const fn ms_class_dbm(self, class: u8) -> Option<i32> {
        Some(match (self.power_table(), class) {
            (PowerTable::Low, 1) => 43,
            (PowerTable::Low, 2) => 39,
            (PowerTable::Low, 3) => 37,
            (PowerTable::Low, 4) => 33,
            (PowerTable::Low, 5) => 29,
            (PowerTable::Dcs | PowerTable::Pcs, 1) => 30,
            (PowerTable::Dcs | PowerTable::Pcs, 2) => 24,
            (PowerTable::Dcs, 3) => 36,
            (PowerTable::Pcs, 3) => 33,
            _ => return None,
        })
    }

    /// Returns the power control level for a power in dBm. Powers outside the
    /// band's range give the level for the nearest end of it.
    pub const fn ms_power_level(self, dbm: u32) -> u8 {
        let level = match self.power_table() {
            PowerTable::Low => match dbm {
                39.. => 0,
                5..=38 => 2 + (39 - dbm) / 2,
                _ => 19,
            },
            PowerTable::Dcs => match dbm {
                36.. => 29,
                34..=35 => 30,
                32..=33 => 31,
                31 => 0,
                _ => (30 - dbm) / 2,
            },
            PowerTable::Pcs => match dbm {
                33.. => 30,
                32 => 31,
                31 => 0,
                _ => (30 - dbm) / 2,
            },
        };
        level as u8
    }

    /// Returns the power in dBm of a power control level, or `None` for a level
    /// PCS 1900 reserves. Only the low five bits of `level` are used.
    pub const fn ms_power_dbm(self, level: u8) -> Option<i32> {
        let level = (level & 0x1f) as i32;
        Some(match self.power_table() {
            PowerTable::Low => match level {
                0..=1 => 39,
                2..=19 => 39 - (level - 2) * 2,
                _ => 5,
            },
            PowerTable::Dcs => match level {
                0..=15 => 30 - level * 2,
                16..=28 => 0,
                _ => 36 - (level - 29) * 2,
            },
            PowerTable::Pcs => match level {
                0..=15 => 30 - level * 2,
                16..=29 => return None,
                _ => 33 - (level - 30),
            },
        })
    }

    const fn power_table(self) -> PowerTable {
        match self {
            Self::Dcs1800 => PowerTable::Dcs,
            Self::Pcs1900 => PowerTable::Pcs,
            _ => PowerTable::Low,
        }
    }
}

impl fmt::Display for Band {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy)]
enum PowerTable {
    Low,
    Dcs,
    Pcs,
}

struct ChannelRange {
    first: u16,
    last: u16,
    uplink_start: u16,
    downlink_offset: u16,
    flags: u16,
}

impl ChannelRange {
    const fn lowest(&self, direction: Direction) -> u16 {
        match direction {
            Direction::Uplink => self.uplink_start,
            Direction::Downlink => self.uplink_start + self.downlink_offset,
        }
    }
}

// Some bands overlap, and the first match wins, so the order is part of the results.
const CHANNEL_RANGES: [ChannelRange; 9] = {
    const fn range(
        first: u16,
        last: u16,
        uplink_start: u16,
        downlink_offset: u16,
        flags: u16,
    ) -> ChannelRange {
        ChannelRange {
            first,
            last,
            uplink_start,
            downlink_offset,
            flags,
        }
    }
    [
        range(512, 810, 18502, 800, ARFCN_PCS),
        range(0, 124, 8900, 450, 0),
        range(940, 1023, 8732, 450, 0),
        range(128, 251, 8242, 450, 0),
        range(512, 885, 17102, 950, 0),
        range(259, 293, 4506, 100, 0),
        range(306, 340, 4790, 100, 0),
        range(350, 425, 8060, 450, 0),
        range(438, 511, 7472, 300, 0),
    ]
};

/// Returns an ARFCN's frequency in units of 100 kHz.
///
/// The flags must be exactly those of a band: none, or [`ARFCN_PCS`] alone. An ARFCN
/// carrying [`ARFCN_UPLINK`] gives `None`, including one from [`freq10_to_arfcn`], as
/// in C. Clear that flag before converting back.
pub fn arfcn_to_freq10(arfcn: u16, direction: Direction) -> Option<u16> {
    let flags = arfcn & ARFCN_FLAG_MASK;
    let channel = arfcn & !ARFCN_FLAG_MASK;
    CHANNEL_RANGES
        .iter()
        .find(|range| flags == range.flags && (range.first..=range.last).contains(&channel))
        .map(|range| range.lowest(direction) + 2 * (channel - range.first))
}

/// Returns the ARFCN of a frequency in units of 100 kHz, rounding down to a channel.
///
/// The result has [`ARFCN_PCS`] set for PCS 1900 and [`ARFCN_UPLINK`] set for an
/// uplink frequency. Where bands overlap, the result is the first in this order: PCS
/// 1900, GSM 900, E-GSM 900, GSM 850, DCS 1800, GSM 450, GSM 480, GSM 810, GSM 750.
pub fn freq10_to_arfcn(freq10: u16, direction: Direction) -> Option<u16> {
    let range = CHANNEL_RANGES.iter().find(|range| {
        let lowest = range.lowest(direction);
        (lowest..=lowest + 2 * (range.last - range.first)).contains(&freq10)
    })?;
    let arfcn = (range.first + (freq10 - range.lowest(direction)) / 2) | range.flags;
    Some(match direction {
        Direction::Uplink => arfcn | ARFCN_UPLINK,
        Direction::Downlink => arfcn,
    })
}

/// Returns the received power in dBm of an RxLev value. Values above 63 count as 63.
pub const fn rxlev_to_dbm(rxlev: u8) -> i32 {
    let rxlev = if rxlev > 63 { 63 } else { rxlev };
    rxlev as i32 - 110
}

/// Returns the RxLev value of a received power in dBm, clamped to 0 to 63.
pub const fn dbm_to_rxlev(dbm: i32) -> u8 {
    match dbm {
        ..=-110 => 0,
        -47.. => 63,
        _ => (dbm + 110) as u8,
    }
}

/// A frame number and the T1, T2, T3 and TC values derived from it.
///
/// The fields always agree, because a value can only come from a valid frame number
/// or a valid T1, T2 and T3. Formats like `osmo_dump_gsmtime`, as
/// `frame/t1/t2/t3/x`. `x` is the frame number's low byte modulo 52, not the frame
/// number modulo 52, and differs from it from frame 256 on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GsmTime {
    frame: u32,
}

impl GsmTime {
    /// Returns the time of a frame number, or `None` from [`TDMA_HYPERFRAME`] on.
    pub const fn from_frame(frame: u32) -> Option<Self> {
        if frame < TDMA_HYPERFRAME {
            Some(Self { frame })
        } else {
            None
        }
    }

    /// Returns the time with these values, or `None` unless T1 is below 2048, T2
    /// below 26 and T3 below 51.
    pub const fn from_parts(t1: u16, t2: u8, t3: u8) -> Option<Self> {
        if t1 >= 2048 || t2 >= 26 || t3 >= 51 {
            return None;
        }
        let (t1, t2, t3) = (t1 as u32, t2 as i32, t3 as u32);
        let cycle = (t3 as i32 - t2).rem_euclid(26) as u32;
        Some(Self {
            frame: 51 * cycle + t3 + TDMA_SUPERFRAME * t1,
        })
    }

    /// Returns the frame number.
    pub const fn frame(self) -> u32 {
        self.frame
    }

    /// Returns the superframe count.
    pub const fn t1(self) -> u16 {
        (self.frame / TDMA_SUPERFRAME) as u16
    }

    /// Returns the frame number modulo 26.
    pub const fn t2(self) -> u8 {
        (self.frame % 26) as u8
    }

    /// Returns the frame number modulo 51.
    pub const fn t3(self) -> u8 {
        (self.frame % 51) as u8
    }

    /// Returns the 51-multiframe count modulo 8.
    pub const fn tc(self) -> u8 {
        ((self.frame / 51) % 8) as u8
    }
}

impl fmt::Display for GsmTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_time(f, self.frame, self.t1(), self.t2(), self.t3())
    }
}

pub(crate) fn format_time(
    f: &mut impl fmt::Write,
    frame: u32,
    t1: u16,
    t2: u8,
    t3: u8,
) -> fmt::Result {
    write!(
        f,
        "{frame:06}/{t1:02}/{t2:02}/{t3:02}/{:02}",
        (frame as u8) % 52
    )
}

/// Returns a frame number's reduced frame number.
pub const fn frame_to_rfn(frame: u32) -> u16 {
    (frame % RFN_MODULUS as u32) as u16
}

/// Returns the full frame number of a reduced frame number, given a current frame
/// near it, or `None` if `rfn` is [`RFN_MODULUS`] or more, where C aborts.
///
/// If `rfn` and the current frame's own RFN are over half the modulus apart, `rfn` is
/// taken to be from the previous cycle. That applies whichever of the two is larger,
/// as in C, so the result is never in a later cycle than `current`.
pub const fn rfn_to_frame(rfn: u16, current: u32) -> Option<u32> {
    if rfn >= RFN_MODULUS {
        return None;
    }
    let (rfn, modulus) = (rfn as u32, RFN_MODULUS as u32);
    let current_rfn = current % modulus;
    let mut cycle_start = frame_sub(current, current_rfn);
    if frame_distance(rfn, current_rfn) > modulus / 2 {
        cycle_start = frame_sub(cycle_start, modulus);
    }
    Some(frame_add(cycle_start, rfn))
}

/// The kind of a TLLI, from its top bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TlliType {
    /// Local TLLI.
    Local,
    /// Foreign TLLI.
    Foreign,
    /// Random TLLI.
    Random,
    /// Auxiliary TLLI.
    Auxiliary,
    /// Reserved.
    Reserved,
    /// G-RNTI.
    GRnti,
    /// Random G-RNTI.
    RandomGRnti,
}

impl TlliType {
    /// Returns the kind of a TLLI.
    pub const fn of(tlli: u32) -> Self {
        match tlli >> 27 {
            0b11000.. => Self::Local,
            0b10000.. => Self::Foreign,
            0b01111 => Self::Random,
            0b01110 => Self::Auxiliary,
            0b00000 | 0b00001 => Self::GRnti,
            0b00010 | 0b00011 => Self::RandomGRnti,
            _ => Self::Reserved,
        }
    }
}

/// Returns the P-TMSI in a local or foreign TLLI, or `None` for any other kind.
pub const fn tlli_to_tmsi(tlli: u32) -> Option<u32> {
    match TlliType::of(tlli) {
        TlliType::Local | TlliType::Foreign => Some(tlli | 0xc000_0000),
        _ => None,
    }
}

/// Returns the local TLLI for a P-TMSI.
pub const fn local_tlli(p_tmsi: u32) -> u32 {
    p_tmsi | 0xc000_0000
}

/// Returns the foreign TLLI for a P-TMSI.
pub const fn foreign_tlli(p_tmsi: u32) -> u32 {
    (p_tmsi & 0x3fff_ffff) | 0x8000_0000
}

/// Returns whether Mobile Station Classmark 2 says A5/`n` is supported. A5/0 always
/// is, and `n` above 3 isn't covered by this classmark, so gives `false`.
pub const fn cm2_supports_a5(classmark: &[u8; 3], n: u32) -> bool {
    match n {
        0 => true,
        1 => classmark[0] & 0x08 == 0,
        2 => classmark[2] & 0x01 != 0,
        3 => classmark[2] & 0x02 != 0,
        _ => false,
    }
}

/// Returns whether the first octet of Mobile Station Classmark 3 says A5/`n` is
/// supported. It covers only `n` from 4 to 7, and anything else gives `false`.
pub const fn cm3_supports_a5(first_octet: u8, n: u32) -> bool {
    match n {
        4..=7 => first_octet & (1 << (n - 4)) != 0,
        _ => false,
    }
}

/// Returns the two-bit RACH code for a maximum retransmission count.
pub const fn rach_max_trans_to_raw(value: u8) -> u8 {
    (value >> 1) & 3
}

/// Returns the maximum retransmission count of a RACH code. Only the low two bits of
/// `raw` are used.
pub const fn rach_max_trans_from_raw(raw: u8) -> u8 {
    match raw & 3 {
        0 => 1,
        1 => 2,
        2 => 4,
        _ => 7,
    }
}

/// Returns the slots a RACH transmission is spread over. Only the low four bits of
/// `raw` are used.
pub const fn rach_tx_integer_from_raw(raw: u8) -> u8 {
    match raw & 0x0f {
        10 => 14,
        11 => 16,
        12 => 20,
        13 => 25,
        14 => 32,
        15 => 50,
        low => low + 3,
    }
}

/// The septet that marks the next one as an extension character.
const ESCAPE_SEPTET: u8 = 0x1b;
/// What a byte with no septet maps to. Packing keeps its low seven bits, 0x7f.
const NO_SEPTET: u8 = 0xff;
/// USSD pads a message with this character.
const CR: u8 = b'\r';
/// Where the extension characters start in [`SEPTET_OF_BYTE`].
const EXTENSION_OFFSET: usize = 0x7f;

/// Bytes with a septet other than their own value, from the default alphabet.
const RECODED_BYTES: [(u8, u8); 29] = [
    (b'$', 0x02),
    (b'@', 0x00),
    (b'_', 0x11),
    (0xa1, 0x40),
    (0xa3, 0x01),
    (0xa5, 0x03),
    (0xc4, 0x5b),
    (0xc5, 0x0e),
    (0xc6, 0x1c),
    (0xc7, 0x09),
    (0xc9, 0x1f),
    (0xd1, 0x5d),
    (0xd6, 0x5c),
    (0xd8, 0x0b),
    (0xdc, 0x5e),
    (0xdf, 0x1e),
    (0xe0, 0x7f),
    (0xe4, 0x7b),
    (0xe5, 0x0f),
    (0xe6, 0x1d),
    (0xe8, 0x04),
    (0xe9, 0x05),
    (0xec, 0x07),
    (0xf1, 0x7d),
    (0xf2, 0x08),
    (0xf6, 0x7c),
    (0xf8, 0x0c),
    (0xf9, 0x06),
    (0xfc, 0x7e),
];

/// Extension characters as (septet after the escape, byte), and the septet each
/// byte encodes to where that isn't the same. Form feed has no septet of its own.
const EXTENSION_CHARACTERS: [(u8, u8, u8); 9] = [
    (0x0a, 0x0c, NO_SEPTET),
    (0x14, b'^', 0x14),
    (0x28, b'{', 0x28),
    (0x29, b'}', 0x29),
    (0x2f, b'\\', 0x2f),
    (0x3c, b'[', 0x3c),
    (0x3d, b'~', 0x3d),
    (0x3e, b']', 0x3e),
    (0x40, b'|', 0x40),
];

const fn identity_byte(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r' | b' '..=b'#' | b'%'..=b'?' | b'A'..=b'Z' | b'a'..=b'z')
}

/// The septet of each byte. As in C, one table also holds the extension characters'
/// bytes from [`EXTENSION_OFFSET`], so those positions replace the septets of the
/// bytes that land there. That is why 0xa7 encodes to 0x7b, not to the 0x5f of `§`.
const SEPTET_OF_BYTE: [u8; 256] = {
    let mut table = [NO_SEPTET; 256];
    let mut byte = 0;
    while byte < 256 {
        if identity_byte(byte as u8) {
            table[byte] = byte as u8;
        }
        byte += 1;
    }
    let mut index = 0;
    while index < RECODED_BYTES.len() {
        let (byte, septet) = RECODED_BYTES[index];
        table[byte as usize] = septet;
        index += 1;
    }
    let mut index = 0;
    while index < EXTENSION_CHARACTERS.len() {
        let (extension, byte, septet) = EXTENSION_CHARACTERS[index];
        table[byte as usize] = septet;
        table[EXTENSION_OFFSET + extension as usize] = byte;
        index += 1;
    }
    table
};

/// The byte of each septet: the lowest byte with that entry in [`SEPTET_OF_BYTE`], or
/// 0xff. The shared table means that for some septets this is an extension position.
const BYTE_OF_SEPTET: [u8; 128] = {
    let mut table = [0xff; 128];
    let mut byte = 256;
    while byte > 0 {
        byte -= 1;
        let septet = SEPTET_OF_BYTE[byte];
        if septet < 128 {
            table[septet as usize] = byte as u8;
        }
    }
    table
};

const NEEDS_ESCAPE: [bool; 256] = {
    let mut table = [false; 256];
    let mut index = 0;
    while index < EXTENSION_CHARACTERS.len() {
        table[EXTENSION_CHARACTERS[index].1 as usize] = true;
        index += 1;
    }
    table
};

/// Text packed into the 7-bit default alphabet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packed7bit {
    /// The packed octets.
    pub octets: Vec<u8>,
    /// The septets they hold. The octet count alone can't give this: 7 and 8 septets
    /// both take 7 octets.
    pub septets: usize,
}

/// Returns the octets needed to pack `septets` septets.
pub const fn septets_to_octets(septets: usize) -> usize {
    septets / 8 * 7 + (septets % 8 * 7).div_ceil(8)
}

/// Converts bytes to unpacked septets, putting an escape before each extension
/// character. A byte with no septet, including NUL, gives 0xff.
pub fn encode_septets(text: &[u8]) -> Vec<u8> {
    let mut septets = Vec::with_capacity(text.len());
    for &byte in text {
        if NEEDS_ESCAPE[usize::from(byte)] {
            septets.push(ESCAPE_SEPTET);
        }
        septets.push(SEPTET_OF_BYTE[usize::from(byte)]);
    }
    septets
}

/// Packs septets into octets after `padding` zero bits, using each one's low seven
/// bits. Returns `None` if `padding` is above 7.
pub fn pack_septets(septets: &[u8], padding: u8) -> Option<Vec<u8>> {
    if padding > 7 {
        return None;
    }
    let mut octets = Vec::with_capacity(septets_to_octets(septets.len()) + 1);
    let mut pending = 0u16;
    let mut bits = u32::from(padding);
    for septet in septets {
        pending |= u16::from(septet & 0x7f) << bits;
        bits += 7;
        if bits >= 8 {
            octets.push(pending as u8);
            pending >>= 8;
            bits -= 8;
        }
    }
    if bits > 0 {
        octets.push(pending as u8);
    }
    Some(octets)
}

/// Encodes and packs text, keeping as many septets as fit in `max_octets`.
///
/// Text is bytes, not UTF-8: the alphabet maps Latin-1 values. A limit can leave an
/// escape as the last septet. A byte with no septet packs as 0x7f.
pub fn encode_7bit(text: &[u8], max_octets: usize) -> Packed7bit {
    let mut septets = encode_septets(text);
    septets.truncate(max_octets.saturating_mul(8) / 7);
    let octets = pack_septets(&septets, 0).expect("zero padding is valid");
    Packed7bit {
        octets,
        septets: septets.len(),
    }
}

/// Encodes text for USSD. Like [`encode_7bit`], then fills a last octet with one spare
/// septet with `<CR>`, so that it isn't read as an `@`. A last septet of `<CR>` that
/// ends exactly on an octet gets a second `<CR>`, unless that would use the last octet
/// allowed, even though it would fit.
pub fn encode_7bit_ussd(text: &[u8], max_octets: usize) -> Packed7bit {
    let mut packed = encode_7bit(text, max_octets);
    let room_for_more = packed.octets.len() + 1 < max_octets;
    match (packed.septets % 8, packed.octets.last_mut()) {
        (7, Some(last)) => *last |= CR << 1,
        (0, Some(last)) if *last >> 1 == CR && room_for_more => {
            packed.octets.push(CR);
            packed.septets += 1;
        }
        _ => {}
    }
    packed
}

/// Reads `count` septets starting `first` septets in, stopping after `limit` bytes of
/// output. Octets at or past `octet_limit` don't add to a septet that straddles them,
/// and octets past the end of `packed` read as zero.
pub(crate) fn unpack_text(
    packed: &[u8],
    first: usize,
    count: usize,
    octet_limit: usize,
    limit: usize,
) -> Vec<u8> {
    let octet = |index: usize| packed.get(index).copied().unwrap_or(0);
    let mut text = Vec::with_capacity(count.min(limit));
    let mut escaped = false;
    for index in 0..count {
        if text.len() == limit {
            break;
        }
        let bit = (first + index) * 7;
        let (low, shift) = (bit / 8, bit % 8);
        let high = if shift > 1 && low + 1 < octet_limit {
            octet(low + 1) << (8 - shift)
        } else {
            0
        };
        let septet = ((octet(low) >> shift) | high) & 0x7f;
        if escaped {
            escaped = false;
            text.push(SEPTET_OF_BYTE[EXTENSION_OFFSET + usize::from(septet)]);
        } else if septet == ESCAPE_SEPTET && index + 1 < count {
            escaped = true;
        } else {
            text.push(BYTE_OF_SEPTET[usize::from(septet)]);
        }
    }
    text
}

/// Returns the septets taken by a user data header whose length octet is `length`.
pub(crate) const fn header_septets(length: u8) -> usize {
    ((length as usize + 1) * 8).div_ceil(7)
}

/// Decodes `septets` packed septets to bytes. Returns `None` if `packed` is too short
/// to hold them.
///
/// The output is bytes, not UTF-8. Septets with no byte give 0xff, and some accented
/// letters give C's values rather than their Latin-1 ones; see the source guide.
pub fn decode_7bit(packed: &[u8], septets: usize) -> Option<Vec<u8>> {
    let octets = septets_to_octets(septets);
    (packed.len() >= octets).then(|| unpack_text(packed, 0, septets, octets, usize::MAX))
}

/// Decodes a message that starts with a user data header, returning the text after
/// it. `septets` counts the header too. Returns `None` if `packed` is too short or the
/// header doesn't fit in `septets`.
pub fn decode_7bit_with_header(packed: &[u8], septets: usize) -> Option<Vec<u8>> {
    let octets = septets_to_octets(septets);
    let first = header_septets(*packed.first()?);
    (packed.len() >= octets && first <= septets)
        .then(|| unpack_text(packed, first, septets - first, octets, usize::MAX))
}

/// Decodes a USSD message: like [`decode_7bit`], then drops the last byte if the top
/// seven bits of the last octet hold `<CR>`, wherever the last septet ends.
pub fn decode_7bit_ussd(packed: &[u8], septets: usize) -> Option<Vec<u8>> {
    let mut text = decode_7bit(packed, septets)?;
    let octets = septets_to_octets(septets);
    if !text.is_empty() && packed[octets - 1] >> 1 == CR {
        text.pop();
    }
    Some(text)
}
