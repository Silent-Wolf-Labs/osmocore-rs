// SPDX-License-Identifier: GPL-3.0-or-later

use osmocore_rs::{gsmtap::GsmtapMessage, msgb::MsgbError};

#[test]
fn msgb_payload_ceiling_is_65519_while_vec_accepts_65520() {
    let payload = vec![0xa5; 65_520];
    let limit = GsmtapMessage::um(&payload[..65_519]);
    let packet = limit.encode_msgb().unwrap();
    assert_eq!(packet.len(), u16::MAX);
    assert_eq!(packet.capacity(), u16::MAX);
    assert_eq!(packet.data(), limit.encode());
    assert!(packet.check_invariants());

    let over_limit = GsmtapMessage::um(&payload);
    assert_eq!(
        over_limit.encode_msgb().unwrap_err(),
        MsgbError::CapacityTooLarge
    );
    let bytes = over_limit.encode();
    assert_eq!(bytes.len(), 65_536);
    assert_eq!(&bytes[16..], payload);
}

#[test]
fn encoded_msgb_owns_bytes_independently_of_semantic_payload() {
    let mut payload = vec![0, 1, 0xff, 3];
    let message = GsmtapMessage::um(&payload);
    let expected = message.encode();
    let packet = message.encode_msgb().unwrap();
    payload.fill(0xa5);
    drop(payload);
    assert_eq!(packet.data(), expected);
    assert_eq!(usize::from(packet.capacity()), expected.len());
    assert_eq!(packet.headroom(), 0);
    assert_eq!(packet.tailroom(), 0);
}
