// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM bands, ARFCNs, power levels, GSM time, reduced frame numbers, TLLIs,
//! classmarks and RACH parameters.
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
