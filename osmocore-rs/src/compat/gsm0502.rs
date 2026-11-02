// SPDX-License-Identifier: GPL-3.0-or-later

use crate::gsm0502::{RemapChannel, block_start, ccch_block, frame_cmp};
use std::cmp::Ordering;

/// Compares two frame numbers, returning -1, 0 or 1.
pub const fn gsm0502_fncmp(fn1: u32, fn2: u32) -> i32 {
    match frame_cmp(fn1, fn2) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// Returns the frame a block began on, or `frame` itself if no block ends there.
/// C also logs an error in that case; this doesn't.
pub fn gsm0502_fn_remap(frame: u32, channel: RemapChannel) -> u32 {
    block_start(frame, channel).unwrap_or(frame)
}

/// Returns the CCCH block, or a negative value that says what the frame carries:
/// -1 for FCCH, -2 for SCH, -3 for BCCH and -4 for idle.
pub const fn gsm0502_fn2ccch_block(frame: u32) -> i32 {
    match ccch_block(frame) {
        Some(block) => block as i32,
        None => match frame % 51 {
            50 => -4,
            2..=5 => -3,
            position if position % 10 == 0 => -1,
            _ => -2,
        },
    }
}

/// Returns `BS_CC_CHANS` for a `CCCH_CONF` code, or -1 for a reserved one.
pub const fn rsl_ccch_conf_to_bs_cc_chans(ccch_conf: u8) -> i32 {
    match crate::gsm0502::CcchConf::from_code(ccch_conf) {
        Some(conf) => conf.channels() as i32,
        None => -1,
    }
}

/// Returns the paging blocks in a 51-multiframe. Wraps if `bs_ag_blks_res` exceeds
/// the blocks available.
pub const fn gsm0502_get_n_pag_blocks(ccch_conf: u8, bs_ag_blks_res: u8) -> u32 {
    let available: u32 = if ccch_conf == 1 { 3 } else { 9 };
    available.wrapping_sub(bs_ag_blks_res as u32)
}

/// Returns the paging subchannels on each CCCH, treating a combined channel with
/// no paging blocks as having one. Wraps like the C `unsigned` and `int` values.
pub const fn gsm48_number_of_paging_subchannels(
    ccch_conf: u8,
    bs_ag_blks_res: u8,
    bs_pa_mfrms: u8,
) -> i32 {
    let blocks = gsm0502_get_n_pag_blocks(ccch_conf, bs_ag_blks_res);
    let blocks = if ccch_conf == 1 && blocks == 0 {
        1
    } else {
        blocks
    };
    blocks.wrapping_mul(bs_pa_mfrms as u32 + 2) as i32
}

/// Returns the CCCH group, or `None` where C divides by zero. The product of the
/// two counts wraps at 32 bits.
pub const fn gsm0502_get_ccch_group(imsi: u64, bs_cc_chans: u32, n_pag_blocks: u32) -> Option<u32> {
    let product = bs_cc_chans.wrapping_mul(n_pag_blocks);
    if product == 0 || n_pag_blocks == 0 {
        return None;
    }
    Some(((imsi % 1000) % product as u64 / n_pag_blocks as u64) as u32)
}

/// Returns the paging group, or `None` where C divides by zero. The product of the
/// two counts wraps at 32 bits, and a negative `n_pag_blocks` sign-extends to 64.
pub const fn gsm0502_get_paging_group(
    imsi: u64,
    bs_cc_chans: u32,
    n_pag_blocks: i32,
) -> Option<u32> {
    let product = bs_cc_chans.wrapping_mul(n_pag_blocks as u32);
    if product == 0 || n_pag_blocks == 0 {
        return None;
    }
    Some(((imsi % 1000) % product as u64 % n_pag_blocks as u64) as u32)
}

/// Returns the paging group for the coded three-bit control channel fields,
/// including reserved and out-of-range values. Higher bits are ignored, as they
/// don't exist in the C bit-fields.
pub const fn gsm0502_calc_paging_group(
    ccch_conf: u8,
    bs_ag_blks_res: u8,
    bs_pa_mfrms: u8,
    imsi: u64,
) -> u32 {
    let (ccch_conf, ag, pa) = (ccch_conf & 7, bs_ag_blks_res & 7, bs_pa_mfrms & 7);
    let channels = rsl_ccch_conf_to_bs_cc_chans(ccch_conf) as u32;
    let blocks = gsm48_number_of_paging_subchannels(ccch_conf, ag, pa);
    match gsm0502_get_paging_group(imsi, channels, blocks) {
        Some(group) => group,
        // Three-bit fields can't produce a zero divisor.
        None => unreachable!(),
    }
}
