//! Rust checks that C can't record: running out of room, rejected inputs and what an
//! error leaves behind.

use osmocore_rs::compat::tlv as compat;
use osmocore_rs::msgb::{Msgb, MsgbError};
use osmocore_rs::tlv::*;

type Call<'a> = (
    &'static str,
    Box<dyn Fn(&mut Msgb) -> Result<(), TlvError> + 'a>,
);

fn tvlv_message(len: usize) -> Vec<u8> {
    let mut msg = Msgb::new(len + 3).unwrap();
    msg.put_tvlv(0x10, &vec![0xab; len]).unwrap();
    msg.data().to_vec()
}

#[test]
fn every_tvlv_length_round_trips_up_to_the_limit() {
    let definition = TlvDefinition::all(TlvType::Tvlv);
    for len in [0, 1, 0x7f, 0x80, 0xff, 0x100, VARIABLE_MAX] {
        let message = tvlv_message(len);
        assert_eq!(message.len(), tvlv_gross_len(len));
        let (ie, used) = parse_one(&definition, &message).unwrap();
        assert_eq!((ie.tag, ie.value.len(), used), (0x10, len, message.len()));
    }
    let mut msg = Msgb::new(0xffff).unwrap();
    assert_eq!(
        msg.put_tvlv(0x10, &vec![0; VARIABLE_MAX + 1]),
        Err(TlvError::ValueTooLong)
    );
}

#[test]
fn every_gan_length_round_trips_up_to_the_limit() {
    let definition = TlvDefinition::all(TlvType::VtvlvGan);
    for len in [0, 1, 0x7f, 0x80, 0xff, 0x100, VARIABLE_MAX] {
        let mut msg = Msgb::new(len + 3).unwrap();
        let value = vec![0xcd; len];
        msg.put_vtvlv_gan(0x7f, &value).unwrap();
        assert_eq!(msg.data().len(), vtvlv_gan_gross_len(0x7f, len));
        let (ie, used) = parse_one(&definition, msg.data()).unwrap();
        assert_eq!(
            (ie.tag, ie.value, used),
            (0x7f, &value[..], msg.data().len())
        );
    }
}

#[test]
fn compat_lengths_from_0x8000_are_misread_by_the_parser() {
    let mut msg = Msgb::new(0xffff).unwrap();
    compat::msgb_tvlv_put(&mut msg, 0x10, 0x8000, &vec![0xab; 0x8000]).unwrap();
    // The first length byte has its high bit set, so it reads as a one-byte zero.
    let (ie, used) = parse_one(&TlvDefinition::all(TlvType::Tvlv), msg.data()).unwrap();
    assert_eq!((ie.value.len(), used), (0, 2));
}

#[test]
fn a_message_with_no_room_is_left_as_it_was() {
    let value = [0xaa; 8];
    let calls: Vec<Call> = vec![
        ("lv", Box::new(|m| m.put_lv(&value))),
        ("tlv", Box::new(|m| m.put_tlv(1, &value))),
        ("tlv_zeroed", Box::new(|m| m.put_tlv_zeroed(1, 8))),
        ("tl", Box::new(|m| m.put_tl(1).map(drop))),
        ("tlv16", Box::new(|m| m.put_tlv16(1, &[1, 2, 3, 4]))),
        ("tl16v", Box::new(|m| m.put_tl16v(1, &value))),
        ("t16lv", Box::new(|m| m.put_t16lv(1, &value))),
        ("l16tv", Box::new(|m| m.put_l16tv(1, &value))),
        ("tvl", Box::new(|m| m.put_tvl(1, 8).map(drop))),
        ("tvlv", Box::new(|m| m.put_tvlv(1, &value))),
        ("tvlv_u16", Box::new(|m| m.put_tvlv_u16(1, 2))),
        ("tvlv_u32", Box::new(|m| m.put_tvlv_u32(1, 2))),
        ("gan", Box::new(|m| m.put_vtvlv_gan(1, &value))),
        ("v", Box::new(|m| m.put_v(1))),
        ("tv", Box::new(|m| m.put_tv(1, 2))),
        ("tv_fixed", Box::new(|m| m.put_tv_fixed(1, &value))),
        ("tv16", Box::new(|m| m.put_tv16(1, 2))),
        ("tv32", Box::new(|m| m.put_tv32(1, 2))),
    ];
    for (name, call) in calls {
        let mut msg = Msgb::new(1).unwrap();
        msg.put_u8(0xee).unwrap();
        assert_eq!(
            call(&mut msg),
            Err(TlvError::Msgb(MsgbError::NotEnoughTailroom)),
            "{name}"
        );
        assert_eq!(msg.data(), [0xee], "{name}");
    }
    let pushes: Vec<Call> = vec![
        ("tlv", Box::new(|m| m.push_tlv(1, &value))),
        ("tlv1", Box::new(|m| m.push_tlv1(1, 2))),
        ("tv", Box::new(|m| m.push_tv(1, 2))),
        ("tv16", Box::new(|m| m.push_tv16(1, 2))),
        ("tv32", Box::new(|m| m.push_tv32(1, 2))),
        ("tvlv", Box::new(|m| m.push_tvlv(1, &value))),
        ("vtvl", Box::new(|m| m.push_vtvl_gan(1, 8))),
        ("vtvlv", Box::new(|m| m.push_vtvlv_gan(1, &value))),
    ];
    for (name, call) in pushes {
        let mut msg = Msgb::new(16).unwrap();
        msg.put_u8(0xee).unwrap();
        assert_eq!(
            call(&mut msg),
            Err(TlvError::Msgb(MsgbError::NotEnoughHeadroom)),
            "{name}"
        );
        assert_eq!(msg.data(), [0xee], "{name}");
    }
}

#[test]
fn encode_leaves_a_message_unchanged_on_any_failure() {
    let mut definition = TlvDefinition::new();
    definition.set(1, TlvDef::new(TlvType::Tlv));
    definition.set(2, TlvDef::new(TlvType::Tlv));
    definition.set(3, TlvDef::new(TlvType::Tv));
    let long = [0u8; 300];
    let mut parsed = TlvParsed::new();
    parsed.set(1, Some(&[1, 2]));
    parsed.set(2, Some(&long));
    let mut msg = Msgb::new(64).unwrap();
    msg.put_u8(0xee).unwrap();
    assert_eq!(
        encode(&mut msg, &definition, &parsed),
        Err(TlvError::ValueTooLong)
    );
    assert_eq!(msg.data(), [0xee]);

    parsed.set(2, None);
    parsed.set(3, Some(&[]));
    assert_eq!(
        encode(&mut msg, &definition, &parsed),
        Err(TlvError::ValueMissing)
    );
    assert_eq!(msg.data(), [0xee]);

    let mut full = Msgb::new(3).unwrap();
    parsed.set(3, Some(&[9]));
    assert_eq!(
        encode_ordered(&mut full, &definition, &parsed, &[1, 3]),
        Err(TlvError::Msgb(MsgbError::NotEnoughTailroom))
    );
    assert_eq!(full.data(), &[] as &[u8]);
}

#[test]
fn compat_encode_keeps_earlier_elements_when_a_later_one_has_no_room() {
    let mut definition = TlvDefinition::new();
    definition.set(1, TlvDef::new(TlvType::Tv));
    definition.set(2, TlvDef::new(TlvType::Tv));
    let mut parsed = TlvParsed::new();
    parsed.set(1, Some(&[7]));
    parsed.set(2, Some(&[8]));
    let mut msg = Msgb::new(3).unwrap();
    assert!(compat::tlv_encode(&mut msg, &definition, &parsed).is_err());
    assert_eq!(msg.data(), [1, 7]);
}

#[test]
fn encode_one_rejects_what_it_cannot_write() {
    let mut msg = Msgb::new(16).unwrap();
    for kind in [TlvType::Tv, TlvType::SingleTv] {
        assert_eq!(
            encode_one(&mut msg, kind, 1, &[]),
            Err(TlvError::ValueMissing)
        );
        assert_eq!(
            compat::tlv_encode_one(&mut msg, kind, 1, 0, &[]),
            Err(TlvError::ValueMissing)
        );
    }
    assert_eq!(
        compat::tlv_encode_one(&mut msg, TlvType::Tlv, 1, 4, &[0; 2]),
        Err(TlvError::ValueMissing)
    );
    assert_eq!(msg.data(), &[] as &[u8]);
}

#[test]
fn gan_tags_above_the_limit_are_rejected_only_by_the_preferred_api() {
    let mut msg = Msgb::new(16).unwrap();
    assert_eq!(msg.put_vtvlv_gan(0x8000, &[]), Err(TlvError::TagTooLarge));
    assert_eq!(msg.push_vtvl_gan(0x8000, 0), Err(TlvError::TagTooLarge));
    compat::msgb_vtvlv_gan_put(&mut msg, 0x8000, 0, &[]).unwrap();
    assert_eq!(msg.data(), [0x80, 0x00, 0x00]);
}

#[test]
fn zeroed_values_overwrite_what_was_in_the_buffer() {
    let mut msg = Msgb::new(8).unwrap();
    msg.backing_mut().fill(0xff);
    msg.put_tlv_zeroed(1, 3).unwrap();
    assert_eq!(msg.data(), [1, 3, 0, 0, 0]);
    assert_eq!(msg.backing()[5], 0xff);
}

#[test]
fn reserved_tvl_value_is_written_through_the_returned_offset() {
    let mut msg = Msgb::new(8).unwrap();
    let offset = msg.put_tvl(1, 2).unwrap();
    msg.backing_mut()[offset..offset + 2].copy_from_slice(&[7, 8]);
    assert_eq!(msg.data(), [1, 0x82, 7, 8]);
}

#[test]
fn length_byte_is_set_through_the_offset_from_put_tl() {
    let mut msg = Msgb::new(8).unwrap();
    let offset = msg.put_tl(1).unwrap();
    msg.put_v(5).unwrap();
    msg.backing_mut()[offset] = 1;
    assert_eq!(msg.data(), [1, 1, 5]);
}

#[test]
fn validation_reports_the_element_and_empty_decoded_is_safe() {
    let definition = TlvDefinition::all(TlvType::Tlv);
    let mut protocol = TlvProtocol::new("p", &definition);
    protocol.set_message(
        1,
        TlvMessageDef {
            mandatory: &[9, 8],
            ..TlvMessageDef::default()
        },
    );
    protocol.set_ie(
        2,
        TlvIeDef {
            min_len: 4,
            name: None,
        },
    );
    let (parsed, _) = parse(&definition, &[2, 1, 0xaa], [None, None]).unwrap();
    assert_eq!(
        protocol.validate(1, &parsed),
        Err(TlvError::MandatoryIeMissing { iei: 9 })
    );
    assert_eq!(
        protocol.validate(0, &parsed),
        Err(TlvError::IeTooShort {
            iei: 2,
            len: 1,
            min_len: 4
        })
    );
    assert_eq!(
        protocol.parse(&mut [], 1, &[], [None, None]),
        Err(TlvError::MandatoryIeMissing { iei: 9 })
    );
    assert_eq!(protocol.parse(&mut [], 0, &[], [None, None]), Ok(()));
}

#[test]
fn errors_render_their_details() {
    for (error, text) in [
        (TlvError::OffsetBeyondBuffer, "TLV header"),
        (TlvError::LengthBeyondBuffer, "TLV length"),
        (TlvError::UnknownType, "no TLV format"),
        (TlvError::MandatoryIeMissing { iei: 0x0a }, "0x0a"),
        (
            TlvError::IeTooShort {
                iei: 1,
                len: 2,
                min_len: 3,
            },
            "2 bytes, needs 3",
        ),
        (TlvError::ValueTooLong, "length field"),
        (TlvError::ValueMissing, "value byte"),
        (TlvError::TagTooLarge, "tag field"),
        (
            TlvError::Msgb(MsgbError::NotEnoughTailroom),
            "message buffer",
        ),
    ] {
        assert!(error.to_string().contains(text), "{error}");
    }
}

/// Any bytes either parse or fail, and a value is always inside the input.
#[test]
fn arbitrary_input_never_panics() {
    let mut definition = TlvDefinition::new();
    for (tag, kind) in [
        (1, TlvType::T),
        (2, TlvType::Tv),
        (4, TlvType::Tlv),
        (5, TlvType::Tl16v),
        (6, TlvType::Tvlv),
        (7, TlvType::VtvlvGan),
        (0x0a, TlvType::SingleTv),
    ] {
        definition.set(tag, TlvDef::new(kind));
    }
    definition.set(3, TlvDef::fixed(200));
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state as u8
    };
    for round in 0..20_000 {
        let input: Vec<u8> = (0..round % 40).map(|_| next() & 0x0f).collect();
        let (mut outer, mut decoded) = (&input[..], [TlvParsed::new(), TlvParsed::new()]);
        let _ = parse_multiple(&mut decoded, &definition, &input, [Some(0x30), None]);
        let _ = shift_lv(&mut outer);
        let _ = shift_tlv(&mut outer);
        if let Ok((ie, used)) = parse_one(&definition, &input) {
            assert!(used <= input.len() && ie.value.len() <= used);
        }
    }
}
