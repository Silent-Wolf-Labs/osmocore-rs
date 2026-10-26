// SPDX-License-Identifier: GPL-3.0-or-later

use crate::gsmtap::{GSMTAP_TYPE_UM, GsmtapMessage};
/// Compatibility counterpart of `gsmtap_makemsg_ex()`.
// Preserve the parameter list of the C API or reference record.
#[allow(clippy::too_many_arguments)]
pub fn gsmtap_makemsg_ex(
    message_type: u8,
    arfcn: u16,
    timeslot: u8,
    channel_type: u8,
    sub_slot: u8,
    frame_number: u32,
    signal_dbm: i8,
    snr_db: i8,
    payload: &[u8],
) -> Vec<u8> {
    GsmtapMessage {
        message_type,
        arfcn,
        timeslot,
        channel_type,
        sub_slot,
        frame_number,
        signal_dbm,
        snr_db,
        payload,
    }
    .encode()
}

/// Compatibility counterpart of `gsmtap_makemsg()`.
///
/// This is the UM-default wrapper and delegates to [`gsmtap_makemsg_ex`].
// Preserve the parameter list of the C API or reference record.
#[allow(clippy::too_many_arguments)]
pub fn gsmtap_makemsg(
    arfcn: u16,
    timeslot: u8,
    channel_type: u8,
    sub_slot: u8,
    frame_number: u32,
    signal_dbm: i8,
    snr_db: i8,
    payload: &[u8],
) -> Vec<u8> {
    gsmtap_makemsg_ex(
        GSMTAP_TYPE_UM,
        arfcn,
        timeslot,
        channel_type,
        sub_slot,
        frame_number,
        signal_dbm,
        snr_db,
        payload,
    )
}
