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
