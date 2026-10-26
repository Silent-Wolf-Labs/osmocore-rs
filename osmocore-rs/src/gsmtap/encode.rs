// SPDX-License-Identifier: GPL-3.0-or-later

use super::{GSMTAP_HEADER_LENGTH, GSMTAP_VERSION, GsmtapMessage};
use crate::msgb::{Msgb, MsgbError};

fn header(message: GsmtapMessage<'_>) -> [u8; GSMTAP_HEADER_LENGTH] {
    let arfcn = message.arfcn.to_be_bytes();
    let frame = message.frame_number.to_be_bytes();
    [
        GSMTAP_VERSION,
        (GSMTAP_HEADER_LENGTH / 4) as u8,
        message.message_type,
        message.timeslot,
        arfcn[0],
        arfcn[1],
        message.signal_dbm as u8,
        message.snr_db as u8,
        frame[0],
        frame[1],
        frame[2],
        frame[3],
        message.channel_type,
        0, // antenna_nr is fixed by the compatibility constructors.
        message.sub_slot,
        0, // reserved is fixed by the compatibility constructors.
    ]
}

pub(super) fn to_vec(message: GsmtapMessage<'_>) -> Vec<u8> {
    let mut packet = Vec::with_capacity(GSMTAP_HEADER_LENGTH + message.payload.len());
    packet.extend_from_slice(&header(message));
    packet.extend_from_slice(message.payload);
    packet
}

pub(super) fn to_msgb(message: GsmtapMessage<'_>) -> Result<Msgb, MsgbError> {
    let capacity = GSMTAP_HEADER_LENGTH
        .checked_add(message.payload.len())
        .ok_or(MsgbError::CapacityTooLarge)?;
    let mut packet = Msgb::new(capacity)?;
    packet.put(capacity)?;
    let bytes = packet.data_mut();
    bytes[..GSMTAP_HEADER_LENGTH].copy_from_slice(&header(message));
    bytes[GSMTAP_HEADER_LENGTH..].copy_from_slice(message.payload);
    Ok(packet)
}
