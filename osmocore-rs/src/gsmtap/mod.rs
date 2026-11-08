// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("README.md")]

mod encode;
mod message;

pub use message::GsmtapMessage;

/// GSMTAP wire-format version emitted by these constructors.
pub const GSMTAP_VERSION: u8 = 2;
/// Size of the GSMTAP v2 base header emitted by the constructors, in bytes.
pub const GSMTAP_HEADER_LENGTH: usize = 16;
/// GSMTAP type used by [`crate::compat::gsmtap::gsmtap_makemsg`].
pub const GSMTAP_TYPE_UM: u8 = 1;
/// The UDP port registered for GSMTAP.
pub const GSMTAP_UDP_PORT: u16 = 4729;

/// Values for [`GsmtapMessage::message_type`], C's `GSMTAP_TYPE_*`.
pub mod message_type {
    /// GSM Um air interface.
    pub const UM: u8 = 0x01;
    /// GSM Abis.
    pub const ABIS: u8 = 0x02;
    /// Raw Um burst bits.
    pub const UM_BURST: u8 = 0x03;
    /// ISO 7816 smart card interface.
    pub const SIM: u8 = 0x04;
    /// TETRA air interface.
    pub const TETRA_I1: u8 = 0x05;
    /// TETRA air interface bursts.
    pub const TETRA_I1_BURST: u8 = 0x06;
    /// WiMAX bursts.
    pub const WMX_BURST: u8 = 0x07;
    /// GPRS Gb interface, LLC.
    pub const GB_LLC: u8 = 0x08;
    /// GPRS Gb interface, SNDCP.
    pub const GB_SNDCP: u8 = 0x09;
    /// GMR-1 L2 packets.
    pub const GMR1_UM: u8 = 0x0a;
    /// UMTS RLC/MAC.
    pub const UMTS_RLC_MAC: u8 = 0x0b;
    /// UMTS RRC.
    pub const UMTS_RRC: u8 = 0x0c;
    /// LTE RRC.
    pub const LTE_RRC: u8 = 0x0d;
    /// LTE MAC.
    pub const LTE_MAC: u8 = 0x0e;
    /// LTE MAC with a context header.
    pub const LTE_MAC_FRAMED: u8 = 0x0f;
    /// libosmocore log messages.
    pub const OSMOCORE_LOG: u8 = 0x10;
    /// Qualcomm DIAG frames.
    pub const QC_DIAG: u8 = 0x11;
    /// LTE Non-Access Stratum.
    pub const LTE_NAS: u8 = 0x12;
    /// E1/T1 lines.
    pub const E1T1: u8 = 0x13;
    /// GSM RLP frames (3GPP TS 24.022).
    pub const GSM_RLP: u8 = 0x14;
}

/// Values for [`GsmtapMessage::channel_type`] in Um messages, C's `GSMTAP_CHANNEL_*`.
pub mod channel {
    /// An unknown channel.
    pub const UNKNOWN: u8 = 0x00;
    /// Broadcast control channel.
    pub const BCCH: u8 = 0x01;
    /// Common control channel.
    pub const CCCH: u8 = 0x02;
    /// Random access channel.
    pub const RACH: u8 = 0x03;
    /// Access grant channel.
    pub const AGCH: u8 = 0x04;
    /// Paging channel.
    pub const PCH: u8 = 0x05;
    /// Standalone dedicated control channel.
    pub const SDCCH: u8 = 0x06;
    /// SDCCH/4.
    pub const SDCCH4: u8 = 0x07;
    /// SDCCH/8.
    pub const SDCCH8: u8 = 0x08;
    /// Full-rate traffic channel signalling, FACCH/F.
    pub const FACCH_F: u8 = 0x09;
    /// Half-rate traffic channel signalling, FACCH/H.
    pub const FACCH_H: u8 = 0x0a;
    /// Packet associated control channel.
    pub const PACCH: u8 = 0x0b;
    /// Cell broadcast channel on a 52-frame multiframe.
    pub const CBCH52: u8 = 0x0c;
    /// Packet data traffic channel.
    pub const PDTCH: u8 = 0x0d;
    /// C's older name for [`PDTCH`].
    pub const PDCH: u8 = PDTCH;
    /// Packet timing advance control channel.
    pub const PTCCH: u8 = 0x0e;
    /// Cell broadcast channel on a 51-frame multiframe.
    pub const CBCH51: u8 = 0x0f;
    /// Full-rate voice codec payload.
    pub const VOICE_F: u8 = 0x10;
    /// Half-rate voice codec payload.
    pub const VOICE_H: u8 = 0x11;
    /// C's older name for [`FACCH_F`].
    pub const TCH_F: u8 = FACCH_F;
    /// C's older name for [`FACCH_H`].
    pub const TCH_H: u8 = FACCH_H;
    /// Combined with a channel to mark its associated control channel, such as
    /// `SDCCH | ACCH` for the SACCH of an SDCCH.
    pub const ACCH: u8 = 0x80;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::gsmtap::*;

    #[test]
    fn rust_owned_empty_payload_has_only_the_base_header() {
        let packet = gsmtap_makemsg_ex(0xff, 0xffff, 0xff, 0xff, 0xff, u32::MAX, -128, 127, &[]);

        assert_eq!(packet.len(), GSMTAP_HEADER_LENGTH);
        assert_eq!(packet[0], GSMTAP_VERSION);
        assert_eq!(packet[1], 4);
        assert_eq!(packet[13], 0);
        assert_eq!(packet[15], 0);
    }

    #[test]
    fn rust_owned_payload_follows_the_complete_header_unchanged() {
        let payload = [0, 1, 2, 255];
        let packet = gsmtap_makemsg_ex(1, 0, 0, 0, 0, 0, 0, 0, &payload);

        assert_eq!(&packet[GSMTAP_HEADER_LENGTH..], payload);
    }

    #[test]
    fn um_wrapper_delegates_to_the_general_constructor() {
        let wrapped = gsmtap_makemsg(42, 3, 8, 1, 0x0102_0304, -73, 19, &[1, 2]);
        let explicit = gsmtap_makemsg_ex(1, 42, 3, 8, 1, 0x0102_0304, -73, 19, &[1, 2]);

        assert_eq!(wrapped, explicit);
    }
}
