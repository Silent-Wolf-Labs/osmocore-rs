// SPDX-License-Identifier: GPL-3.0-or-later

use osmocore_rs::{base64, bcd, bitvec::BitVec, compat, crc::Crc8Code, msgb::Msgb};

#[test]
fn base64_rust_buffers_omit_terminators_and_report_empty_lengths() {
    let mut output = [0xa5; 5];
    assert_eq!(base64::encode_into(&mut output, b"foo"), Ok(4));
    assert_eq!(&output, b"Zm9v\xa5");
    let mut short = [0xa5; 3];
    assert_eq!(
        base64::encode_into(&mut short, b"foo"),
        Err(base64::Base64Error::BufferTooSmall { required: 4 })
    );
    assert_eq!(short, [0xa5; 3]);
    assert_eq!(base64::decode_into(&mut output, b" \r\n"), Ok(0));
    let before = output;
    assert_eq!(
        base64::decode_into(&mut output, b"!"),
        Err(base64::Base64Error::InvalidInput)
    );
    assert_eq!(output, before);
    assert_eq!(base64::decode(b"Zg=="), Ok(b"f".to_vec()));
    // The recorded decoder accepts incomplete quartets but writes no partial one.
    assert_eq!(base64::decode(b"Zg"), Ok(Vec::new()));
    assert_eq!(base64::encode(b""), Ok(String::new()));
}

#[test]
fn owned_bcd_excludes_c_strings_and_keeps_nibble_order() {
    assert_eq!(bcd::encode("123", false), Ok(vec![0x21, 0xf3]));
    assert_eq!(bcd::decode(&[0x21, 0xf3], 0, 3, false), Ok("123".into()));
    assert_eq!(bcd::encode("", false), Ok(Vec::new()));
    assert_eq!(bcd::encode("12\0", false), Err(bcd::BcdError::InvalidDigit));
    assert_eq!(bcd::char_to_nibble(b'!'), Err(bcd::BcdError::InvalidDigit));
    assert_eq!(compat::bcd::osmo_char2bcd(b'!'), 0);
    assert_eq!(
        bcd::decode(&[0xa1], 0, 2, false),
        Err(bcd::BcdError::InvalidDigit)
    );
    assert_eq!(
        bcd::decode(&[], 0, usize::MAX, true),
        Err(bcd::BcdError::OutOfRange)
    );
}

#[test]
fn buffer_methods_preserve_partial_bit_writes_and_unused_message_storage() {
    let mut bytes = [0u8];
    let mut bits = BitVec::new(&mut bytes);
    bits.cur_bit = 7;
    assert!(
        bits.write_bits(&[osmocore_rs::bitvec::BitValue::One; 2])
            .is_err()
    );
    assert_eq!(bits.cur_bit, 8);
    assert_eq!(bits.data(), &[1]);

    let mut message = Msgb::with_headroom(8, 2).unwrap();
    message.backing_mut().fill(0xa5);
    message.put_u16(0x1234).unwrap();
    assert_eq!(message.data(), &[0x12, 0x34]);
    let before = message.backing().to_vec();
    assert!(message.put(5).is_err());
    assert_eq!(message.backing(), before);
    message.pull(1).unwrap();
    assert_eq!(message.data(), &[0x34]);
    assert_eq!(message.backing(), before);
    let mut large = Msgb::new(1600).unwrap();
    large.put(1600).unwrap();
    assert_eq!(compat::msgb::msgb_hexdump(&large).len(), 1365 * 3);
    assert_eq!(large.hexdump().len(), 1600 * 3);
}

#[test]
fn crc_methods_use_whole_slices_and_return_boolean_comparisons() {
    let code = Crc8Code {
        bits: 3,
        poly: 3,
        init: 0,
        remainder: 0,
    };
    let input = [1, 0, 1, 1];
    let mut output = [0xa5; 5];
    code.set_bits(&input, &mut output).unwrap();
    assert_eq!(code.check_bits(&input, &output), Ok(true));
    output[0] ^= 1;
    assert_eq!(code.check_bits(&input, &output), Ok(false));
    assert_eq!(&output[3..], &[0xa5; 2]);
}

#[test]
fn checked_timeout_tables_keep_reset_prefix_on_invalid_defaults() {
    use osmocore_rs::tdef::{TDef, TDefTable, TDefUnit};
    let mut entries = [
        TDef {
            id: 1,
            default_val: 2,
            val: 9,
            unit: TDefUnit::S,
            ..Default::default()
        },
        TDef {
            id: 2,
            default_val: 4,
            val: 9,
            max_val: 3,
            ..Default::default()
        },
        TDef::default(),
    ];
    let mut table = TDefTable::new(&mut entries).unwrap();
    assert!(table.reset().is_err());
    assert_eq!(table.entries()[0].val, 2);
    assert_eq!(table.entries()[1].val, 9);
    assert_eq!(table.get(1, TDefUnit::MS, -1), Ok(2_000));
    assert!(TDefTable::new(&mut []).is_err());
}

#[test]
fn ordinary_primitive_maps_do_not_interpret_c_sentinels() {
    use osmocore_rs::prim::{PrimEventMap, PrimHeader, event_for_primitive};
    let header = PrimHeader::default();
    let maps = [PrimEventMap {
        sap: 0,
        primitive: 0,
        operation: header.operation,
        event: u32::MAX,
    }];
    assert_eq!(event_for_primitive(&header, &maps), Some(u32::MAX));
    assert_eq!(event_for_primitive(&header, &[]), None);
    assert_eq!(compat::prim::osmo_event_for_prim(&header, &maps), u32::MAX);
}
