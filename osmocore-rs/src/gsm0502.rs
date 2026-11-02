// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM 05.02 (3GPP TS 45.002) frame numbers, burst sizes, block mapping, frequency
//! hopping and paging groups.
//!
//! Frame numbers are `u32` values that wrap at [`TDMA_HYPERFRAME`]. They aren't
//! validated: an out-of-range input to an arithmetic function is reduced the same
//! way as in C.
//!
//! ```
//! use osmocore_rs::gsm0502::{TDMA_HYPERFRAME, ccch_block, frame_add, frame_sub};
//! assert_eq!(frame_add(TDMA_HYPERFRAME - 1, 2), 1);
//! assert_eq!(frame_sub(1, 2), TDMA_HYPERFRAME - 1);
//! assert_eq!(ccch_block(6), Some(0));
//! assert_eq!(ccch_block(0), None);
//! ```

use std::cmp::Ordering;

/// Approximate length of a TDMA frame in nanoseconds.
pub const TDMA_FRAME_DURATION_NS: u32 = 4_615_384;
/// Approximate length of a TDMA frame in microseconds.
pub const TDMA_FRAME_DURATION_US: u32 = 4_615;
/// Frames in a superframe.
pub const TDMA_SUPERFRAME: u32 = 26 * 51;
/// Frames in a hyperframe. Frame numbers wrap here.
pub const TDMA_HYPERFRAME: u32 = 2048 * TDMA_SUPERFRAME;

/// Tail bits in a GMSK normal burst.
pub const NB_GMSK_TAIL_BITS: usize = 3;
/// Payload bits in a GMSK normal burst.
pub const NB_GMSK_PAYLOAD_BITS: usize = 2 * 58;
/// Training sequence bits in a GMSK normal burst.
pub const NB_GMSK_TRAIN_SEQ_BITS: usize = 26;
/// Bits in a GMSK normal burst, without the guard period.
pub const NB_GMSK_BURST_BITS: usize = 148;

/// Tail bits in an 8-PSK normal burst.
pub const NB_8PSK_TAIL_BITS: usize = 3 * NB_GMSK_TAIL_BITS;
/// Payload bits in an 8-PSK normal burst.
pub const NB_8PSK_PAYLOAD_BITS: usize = 3 * NB_GMSK_PAYLOAD_BITS;
/// Training sequence bits in an 8-PSK normal burst.
pub const NB_8PSK_TRAIN_SEQ_BITS: usize = 3 * NB_GMSK_TRAIN_SEQ_BITS;
/// Bits in an 8-PSK normal burst, without the guard period.
pub const NB_8PSK_BURST_BITS: usize = 3 * NB_GMSK_BURST_BITS;

/// Tail bits in a synchronization burst.
pub const SB_GMSK_TAIL_BITS: usize = NB_GMSK_TAIL_BITS;
/// Payload bits in a synchronization burst.
pub const SB_GMSK_PAYLOAD_BITS: usize = 2 * 39;
/// Extended training sequence bits in a synchronization burst.
pub const SB_GMSK_ETRAIN_SEQ_BITS: usize = 64;
/// Bits in a synchronization burst, without the guard period.
pub const SB_GMSK_BURST_BITS: usize = NB_GMSK_BURST_BITS;

/// Tail bits in a dummy burst.
pub const DB_GMSK_TAIL_BITS: usize = NB_GMSK_TAIL_BITS;
/// Mixed bits in a dummy burst.
pub const DB_GMSK_MIXED_BITS: usize = 142;
/// Bits in a dummy burst, without the guard period.
pub const DB_GMSK_BURST_BITS: usize = NB_GMSK_BURST_BITS;

/// Extended tail bits in an access burst.
pub const AB_GMSK_ETAIL_BITS: usize = 8;
/// Synchronization sequence bits in an access burst.
pub const AB_GMSK_SYNCH_SEQ_BITS: usize = 41;
/// Payload bits in an access burst.
pub const AB_GMSK_PAYLOAD_BITS: usize = 36;
/// Tail bits in an access burst.
pub const AB_GMSK_TAIL_BITS: usize = NB_GMSK_TAIL_BITS;
/// Bits in an access burst, without the guard period.
pub const AB_GMSK_BURST_BITS: usize = NB_GMSK_BURST_BITS;

/// Returns `(a + b) mod hyperframe`.
pub const fn frame_add(a: u32, b: u32) -> u32 {
    a.wrapping_add(b) % TDMA_HYPERFRAME
}

/// Returns `(a - b) mod hyperframe`.
pub const fn frame_sub(a: u32, b: u32) -> u32 {
    a.wrapping_add(TDMA_HYPERFRAME).wrapping_sub(b) % TDMA_HYPERFRAME
}

/// Returns the shorter distance between two frame numbers, going either way around.
pub const fn frame_distance(a: u32, b: u32) -> u32 {
    let forward = frame_sub(a, b);
    let backward = frame_sub(b, a);
    if forward < backward {
        forward
    } else {
        backward
    }
}

/// Compares two frame numbers, taking wrapping into account.
///
/// This isn't a total order. Frames exactly half a hyperframe apart have no
/// defined order, and each compares as [`Greater`](Ordering::Greater) than the
/// other, as in C.
pub const fn frame_cmp(a: u32, b: u32) -> Ordering {
    const THRESHOLD: u32 = TDMA_HYPERFRAME / 2;
    if a == b {
        Ordering::Equal
    } else if (a < b && b - a < THRESHOLD) || (a > b && a - b > THRESHOLD) {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// A channel whose block start [`block_start`] can find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemapChannel {
    /// Full-rate traffic channel.
    TchF,
    /// Half-rate traffic channel, subchannel 0.
    TchH0,
    /// Half-rate traffic channel, subchannel 1.
    TchH1,
    /// FACCH on a full-rate traffic channel.
    FacchF,
    /// FACCH on a half-rate traffic channel, subchannel 0.
    FacchH0,
    /// FACCH on a half-rate traffic channel, subchannel 1.
    FacchH1,
}

impl RemapChannel {
    /// Returns the repeat length in frames, and for each frame in that cycle that
    /// ends a block, how many frames earlier the block began.
    const fn block_ends(self) -> (u32, &'static [(u32, u32)]) {
        match self {
            Self::TchF | Self::FacchF => (13, &[(7, 7), (11, 7), (3, 8)]),
            Self::TchH0 => (13, &[(6, 6), (10, 6), (2, 7)]),
            Self::TchH1 => (13, &[(7, 6), (11, 6), (3, 7)]),
            Self::FacchH0 => (26, &[(10, 10), (19, 11), (2, 11)]),
            Self::FacchH1 => (26, &[(11, 10), (20, 11), (3, 11)]),
        }
    }
}

/// Returns the frame number a block began on, given the frame it ended on, or
/// `None` if no block on `channel` ends on that frame.
pub fn block_start(frame: u32, channel: RemapChannel) -> Option<u32> {
    let (cycle, ends) = channel.block_ends();
    let position = frame % cycle;
    ends.iter()
        .find(|(end, _)| *end == position)
        .map(|(_, distance)| frame_sub(frame, *distance))
}

/// Returns the CCCH block (0 to 8) in a frame, or `None` if the frame can't carry
/// CCCH. Accepts any frame of the block, including the first or last burst.
pub const fn ccch_block(frame: u32) -> Option<u8> {
    match frame % 51 {
        6..=9 => Some(0),
        12..=15 => Some(1),
        16..=19 => Some(2),
        22..=25 => Some(3),
        26..=29 => Some(4),
        32..=35 => Some(5),
        36..=39 => Some(6),
        42..=45 => Some(7),
        46..=49 => Some(8),
        _ => None,
    }
}

/// Errors from a frequency hopping calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HopError {
    /// The mobile allocation has no entries. C divides by zero here.
    EmptyAllocation,
    /// The hopping sequence number is above 63. C can read past its table here.
    InvalidHsn(u8),
}

impl std::fmt::Display for HopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAllocation => formatter.write_str("mobile allocation is empty"),
            Self::InvalidHsn(hsn) => write!(formatter, "hopping sequence number {hsn} exceeds 63"),
        }
    }
}

impl std::error::Error for HopError {}

/// RNTABLE from 3GPP TS 45.002 section 6.2.3.
const RNTABLE: [u8; 114] = [
    48, 98, 63, 1, 36, 95, 78, 102, 94, 73, 0, 64, 25, 81, 76, 59, 124, 23, 104, 100, 101, 47, 118,
    85, 18, 56, 96, 86, 54, 2, 80, 34, 127, 13, 6, 89, 57, 103, 12, 74, 55, 111, 75, 38, 109, 71,
    112, 29, 11, 88, 87, 19, 3, 68, 110, 26, 33, 31, 8, 45, 82, 58, 40, 107, 32, 5, 106, 92, 62,
    67, 77, 108, 122, 37, 60, 66, 121, 42, 51, 126, 117, 114, 4, 90, 43, 52, 53, 113, 120, 72, 16,
    49, 7, 79, 119, 61, 22, 84, 9, 97, 91, 15, 21, 24, 46, 39, 93, 105, 65, 70, 125, 99, 17, 123,
];

/// Returns the mobile allocation index to use on `frame`, following 3GPP TS 45.002
/// section 6.2.3.
///
/// `allocation_len` is the size of the mobile allocation. `hsn` 0 hops cyclically
/// and 1 to 63 pseudo-randomly. As in C, `maio` isn't required to be below
/// `allocation_len`.
pub fn hop_index(frame: u32, hsn: u8, maio: u8, allocation_len: usize) -> Result<usize, HopError> {
    if allocation_len == 0 {
        return Err(HopError::EmptyAllocation);
    }
    if hsn > 63 {
        return Err(HopError::InvalidHsn(hsn));
    }
    if hsn == 0 {
        return Ok(frame.wrapping_add(u32::from(maio)) as usize % allocation_len);
    }

    let t1 = (frame / TDMA_SUPERFRAME) as usize;
    let t2 = (frame % 26) as usize;
    let t3 = (frame % 51) as usize;
    // All ones up to the allocation's top bit, for sizes up to 127.
    let mask = (0..=6).fold(0, |mask, shift| mask | allocation_len >> shift);
    let sum = t2 + usize::from(RNTABLE[(usize::from(hsn) ^ (t1 & 63)) + t3]);
    let masked = sum & mask;
    let offset = if masked < allocation_len {
        masked
    } else {
        (masked + (t3 & mask)) % allocation_len
    };
    Ok((offset + usize::from(maio)) % allocation_len)
}

/// Returns the ARFCN to use on `frame`, from a mobile allocation in ascending order.
/// See [`hop_index`].
pub fn hop_arfcn(frame: u32, hsn: u8, maio: u8, allocation: &[u16]) -> Result<u16, HopError> {
    hop_index(frame, hsn, maio, allocation.len()).map(|index| allocation[index])
}

/// The CCCH configuration of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CcchConf {
    /// One basic physical channel, not combined with SDCCH.
    OneNonCombined,
    /// One basic physical channel, combined with SDCCH.
    OneCombined,
    /// Two basic physical channels, not combined with SDCCH.
    TwoNonCombined,
    /// Three basic physical channels, not combined with SDCCH.
    ThreeNonCombined,
    /// Four basic physical channels, not combined with SDCCH.
    FourNonCombined,
}

impl CcchConf {
    /// Decodes the three-bit `CCCH_CONF` field, or returns `None` for a reserved code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::OneNonCombined),
            1 => Some(Self::OneCombined),
            2 => Some(Self::TwoNonCombined),
            4 => Some(Self::ThreeNonCombined),
            6 => Some(Self::FourNonCombined),
            _ => None,
        }
    }

    /// Returns the three-bit `CCCH_CONF` field.
    pub const fn code(self) -> u8 {
        match self {
            Self::OneNonCombined => 0,
            Self::OneCombined => 1,
            Self::TwoNonCombined => 2,
            Self::ThreeNonCombined => 4,
            Self::FourNonCombined => 6,
        }
    }

    /// Returns `BS_CC_CHANS`, the number of basic physical channels carrying CCCH.
    pub const fn channels(self) -> u32 {
        match self {
            Self::OneNonCombined | Self::OneCombined => 1,
            Self::TwoNonCombined => 2,
            Self::ThreeNonCombined => 3,
            Self::FourNonCombined => 4,
        }
    }
}

/// The control channel parameters that determine paging groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagingConfig {
    conf: CcchConf,
    paging_blocks: u32,
    subchannels: u32,
}

impl PagingConfig {
    /// Creates a configuration from the coded `BS_AG_BLKS_RES` and `BS_PA_MFRMS`
    /// fields. The coded `bs_pa_mfrms` is 2 below the number of multiframes.
    ///
    /// Returns `None` if a field doesn't fit in three bits, or if `bs_ag_blks_res`
    /// leaves no paging blocks, which means above 2 on a combined channel. C
    /// computes a group for those; [`compat::gsm0502`](crate::compat::gsm0502) has
    /// its results.
    pub const fn new(conf: CcchConf, bs_ag_blks_res: u8, bs_pa_mfrms: u8) -> Option<Self> {
        let blocks_per_multiframe = match conf {
            CcchConf::OneCombined => 3,
            _ => 9,
        };
        if bs_ag_blks_res > 7 || bs_pa_mfrms > 7 || bs_ag_blks_res >= blocks_per_multiframe {
            return None;
        }
        let paging_blocks = (blocks_per_multiframe - bs_ag_blks_res) as u32;
        Some(Self {
            conf,
            paging_blocks,
            subchannels: paging_blocks * (bs_pa_mfrms as u32 + 2),
        })
    }

    /// Returns the CCCH configuration.
    pub const fn conf(&self) -> CcchConf {
        self.conf
    }

    /// Returns the number of paging blocks in a 51-multiframe.
    pub const fn paging_blocks(&self) -> u32 {
        self.paging_blocks
    }

    /// Returns the number of paging subchannels on each CCCH.
    pub const fn subchannels(&self) -> u32 {
        self.subchannels
    }

    const fn position(&self, imsi: u64) -> u32 {
        ((imsi % 1000) % (self.conf.channels() * self.subchannels) as u64) as u32
    }

    /// Returns the CCCH that carries paging for this IMSI.
    pub const fn ccch_group(&self, imsi: u64) -> u32 {
        self.position(imsi) / self.subchannels
    }

    /// Returns the paging group for this IMSI.
    pub const fn paging_group(&self, imsi: u64) -> u32 {
        self.position(imsi) % self.subchannels
    }
}
