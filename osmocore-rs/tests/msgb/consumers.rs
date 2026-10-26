use osmocore_rs::{
    base64,
    bitvec::BitVec,
    crc::crc16_ccitt,
    gsmtap::GsmtapMessage,
    hexparse,
    msgb::{Msgb, MsgbQueue},
    prim::{PRIM_OP_REQUEST, PrimEventMap, PrimHeader, event_for_primitive},
};

#[test]
fn binary_packet_consumer_composes_bit_fields_crc_message_storage_and_gsmtap() {
    let payload = b"123456789";
    let mut field = [0];
    let mut bits = BitVec::new(&mut field);
    bits.write_uint(5, 3).unwrap();
    bits.write_uint(3, 5).unwrap();
    assert_eq!(field, [0xa3]);
    assert_eq!(crc16_ccitt(0, payload), 0x2189);

    let mut msg = Msgb::with_headroom(32, 4).unwrap();
    msg.put_u8(field[0]).unwrap();
    let offset = msg.put(payload.len()).unwrap();
    msg.backing_mut()[offset..offset + payload.len()].copy_from_slice(payload);
    msg.set_layer_offset(2, Some(offset)).unwrap();
    msg.put_u16(crc16_ccitt(0, payload)).unwrap();
    assert_eq!(msg.l2len(), Ok(11));
    assert!(msg.check_invariants());

    let mut tap = GsmtapMessage::um(msg.data());
    tap.arfcn = 0x1234;
    tap.frame_number = 0x0102_0304;
    tap.signal_dbm = -73;
    let packet = tap.encode();
    let mut expected = vec![
        2, 4, 1, 0, 0x12, 0x34, 0xb7, 0, 1, 2, 3, 4, 0, 0, 0, 0, 0xa3,
    ];
    expected.extend_from_slice(payload);
    expected.extend_from_slice(&[0x21, 0x89]);
    assert_eq!(packet, expected);
    let text = base64::encode(&packet).unwrap();
    assert_eq!(base64::decode(text.as_bytes()), Ok(expected));
}

#[test]
fn primitive_consumer_routes_borrowed_messages_and_queues_independent_owned_copies() {
    let mut queue = MsgbQueue::new();
    for event in 0..16 {
        let mut msg = Msgb::with_headroom(8, 2).unwrap();
        let mut parsed = [0; 2];
        assert_eq!(hexparse::parse_into(b"12 aB", &mut parsed), Ok(2));
        let offset = msg.put(2).unwrap();
        msg.backing_mut()[offset..offset + 2].copy_from_slice(&parsed);
        let header = PrimHeader::new(7, event, PRIM_OP_REQUEST, Some(&mut msg));
        let map = [PrimEventMap {
            sap: 7,
            primitive: event,
            operation: PRIM_OP_REQUEST,
            event,
        }];
        assert_eq!(event_for_primitive(&header, &map), Some(event));
        let attached = header.msg.unwrap();
        attached.cb[0] = u64::from(event);
        let mut copy = attached.copy().unwrap();
        copy.cb[0] = attached.cb[0];
        queue.push_back(copy);
        attached.data_mut().fill(0xff);
        assert_eq!(queue.back().unwrap().data(), [0x12, 0xab]);
    }
    for event in 0..16 {
        let msg = queue.pop_front().unwrap();
        assert_eq!(msg.cb[0], event);
        assert_eq!(msg.data(), [0x12, 0xab]);
        assert!(msg.check_invariants());
    }
    assert!(queue.pop_front().is_none());
}
