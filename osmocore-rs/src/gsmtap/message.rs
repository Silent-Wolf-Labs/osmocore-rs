// SPDX-License-Identifier: GPL-3.0-or-later

use super::GSMTAP_TYPE_UM;
use super::encode;
use crate::msgb::{Msgb, MsgbError};

/// Inputs corresponding to the historical `gsmtap_makemsg_ex()` constructor.
///
/// `antenna_number` and the reserved header byte are not represented because
/// the C constructor does not accept them; both are always emitted as zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GsmtapMessage<'a> {
    pub message_type: u8,
    pub arfcn: u16,
    pub timeslot: u8,
    pub channel_type: u8,
    pub sub_slot: u8,
    pub frame_number: u32,
    pub signal_dbm: i8,
    pub snr_db: i8,
    pub payload: &'a [u8],
}

impl GsmtapMessage<'_> {
    /// Encode as owned GSMTAP v2 bytes, without the `Msgb` capacity limit.
    pub fn encode(self) -> Vec<u8> {
        encode::to_vec(self)
    }

    /// Encode directly into an owned message buffer, without an intermediate Vec.
    ///
    /// The 16-byte header leaves room for at most 65,519 payload bytes in a
    /// `Msgb`. Larger payloads return [`MsgbError::CapacityTooLarge`]. This limit
    /// describes buffer capacity, not whether a packet can be sent over UDP.
    /// [`Self::encode`] retains its existing Vec allocation behavior.
    pub fn encode_msgb(self) -> Result<Msgb, MsgbError> {
        encode::to_msgb(self)
    }

    /// Prepare a generic runtime datagram containing an owned GSMTAP packet.
    ///
    /// Requires the `runtime` feature. `None` means no explicit destination;
    /// `Some(address)` supplies one. The generic runtime validates compatibility
    /// with the connected/unconnected UDP resource when enqueuing. This method
    /// does not validate socket state, enqueue data, or guarantee UDP delivery.
    ///
    /// Uses the same payload capacity limit and errors as [`Self::encode_msgb`].
    /// Enqueue errors remain [`crate::runtime::EnqueueError`] values and return
    /// ownership of the datagram to the caller.
    #[cfg(feature = "runtime")]
    pub fn into_datagram(
        self,
        destination: Option<std::net::SocketAddr>,
    ) -> Result<crate::runtime::Datagram, MsgbError> {
        let message = self.encode_msgb()?;
        crate::diagnostics::event!(
            trace,
            "osmocore_rs::gsmtap",
            operation = "prepare_datagram",
            message_type = self.message_type,
            arfcn = self.arfcn,
            timeslot = self.timeslot,
            channel_type = self.channel_type,
            frame_number = self.frame_number,
            payload_len = self.payload.len(),
            explicit_destination = destination.is_some()
        );
        Ok(crate::runtime::Datagram {
            message,
            destination,
        })
    }
}

impl<'a> GsmtapMessage<'a> {
    /// Create a UM message with zero-valued header fields for the caller to fill.
    pub fn um(payload: &'a [u8]) -> Self {
        Self {
            message_type: GSMTAP_TYPE_UM,
            arfcn: 0,
            timeslot: 0,
            channel_type: 0,
            sub_slot: 0,
            frame_number: 0,
            signal_dbm: 0,
            snr_db: 0,
            payload,
        }
    }
}
