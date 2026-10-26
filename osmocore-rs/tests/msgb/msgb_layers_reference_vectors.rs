use osmocore_rs::compat::msgb::msgb_sms;
#[path = "../support/msgb_layers_fixture.rs"]
mod fixture;
use super::msgb_reference_vectors::{initial, snapshot};
use fixture::number;
use osmocore_rs::msgb::*;

#[test]
fn typed_and_layer_helpers_match_all_c_observations() {
    let records = fixture::validated_records(include_str!(
        "../compatibility/vectors/msgb_layers_vectors.json"
    ));
    let mut current = None;
    for r in &records {
        if number(r, "step") == 0 {
            current = Some(initial(r));
        }
        let msg = current.as_mut().unwrap();
        snapshot(msg, r, "before");
        let arg = number(r, "arg");
        let outcome: Result<i64, MsgbError> = match r["api"].as_str() {
            "put_u8" => msg.put_u8(arg as u8).map(|()| -1),
            "put_u16" => msg.put_u16(arg as u16).map(|()| -1),
            "put_u32" => msg.put_u32(arg as u32).map(|()| -1),
            "push_u8" => msg.push_u8(arg as u8).map(|()| -1),
            "push_u16" => msg.push_u16(arg as u16).map(|()| -1),
            "push_u32" => msg.push_u32(arg as u32).map(|()| -1),
            "get_u8" => msg.get_u8().map(i64::from),
            "get_u16" => msg.get_u16().map(i64::from),
            "get_u32" => msg.get_u32().map(i64::from),
            "pull_u8" => msg.pull_u8().map(i64::from),
            "pull_u16" => msg.pull_u16().map(i64::from),
            "pull_u32" => msg.pull_u32().map(i64::from),
            "l1" => Ok(msg.l1().map_or(-1, |o| o as i64)),
            "l1len" => msg.l1len().map(i64::from),
            "l2" => Ok(msg.l2().map_or(-1, |o| o as i64)),
            "l2len" => msg.l2len().map(i64::from),
            "l3" => Ok(msg.l3().map_or(-1, |o| o as i64)),
            "l3len" => msg.l3len().map(i64::from),
            "l4" => Ok(msg.l4().map_or(-1, |o| o as i64)),
            "l4len" => msg.l4len().map(i64::from),
            "sms" => Ok(msgb_sms(msg).map_or(-1, |o| o as i64)),
            "pull_to_l2" => msg.pull_to_l2().map(|o| o as i64),
            "pull_to_l3" => msg.pull_to_l3().map(|o| o as i64),
            "l3trim" => msg.l3trim(arg as i32).map(|()| 0),
            "push_tl" => msg.push_tl(arg as u8).map(|o| o as i64),
            "reset" => {
                msg.reset();
                Ok(-1)
            }
            api => panic!("unknown API {api}"),
        };
        let error =
            number(r, "panic") == 1 || (r["api"] == "l3trim" && number(r, "return_code") == -1);
        assert_eq!(outcome.is_err(), error, "case {}", r["case"]);
        if let Ok(value) = outcome {
            assert_eq!(value, number(r, "return_code"), "case {}", r["case"]);
        }
        snapshot(msg, r, "after");
    }
}
#[test]
fn rejects_corrupt_typed_and_layer_evidence() {
    fixture::check_corruptions(include_str!(
        "../compatibility/vectors/msgb_layers_vectors.json"
    ));
}

#[test]
fn unsafe_c_layer_cases_are_atomic_rust_errors() {
    let mut msg = Msgb::with_headroom(8, 2).unwrap();
    msg.put(2).unwrap();
    msg.backing_mut().fill(0xa5);
    msg.cb = [1, 2, 3, 4, 5];
    let before = format!("{msg:?}");
    assert_eq!(msg.l1(), None);
    assert_eq!(msg.l2(), None);
    assert_eq!(msg.l3(), None);
    assert_eq!(msg.l4(), None);
    assert_eq!(msgb_sms(&msg), None);
    for result in [msg.l1len(), msg.l2len(), msg.l3len(), msg.l4len()] {
        assert_eq!(result, Err(MsgbError::MissingLayer));
    }
    assert_eq!(msg.pull_to_l2(), Err(MsgbError::MissingLayer));
    assert_eq!(msg.pull_to_l3(), Err(MsgbError::MissingLayer));
    assert_eq!(msg.l3trim(0), Err(MsgbError::MissingLayer));
    assert_eq!(format!("{msg:?}"), before);
    msg.set_layer_offset(1, Some(0)).unwrap();
    msg.set_layer_offset(2, Some(1)).unwrap();
    msg.set_layer_offset(3, Some(6)).unwrap();
    let before = format!("{msg:?}");
    assert_eq!(msg.pull_to_l2(), Err(MsgbError::LayerOutsideMessage));
    assert_eq!(msg.pull_to_l3(), Err(MsgbError::LayerOutsideMessage));
    // C checks total capacity and would form a tail beyond this backing buffer.
    assert_eq!(msg.l3trim(3), Err(MsgbError::NotEnoughTailroom));
    // C's narrowing from ptrdiff_t to int is not a portable signed result.
    assert_eq!(msg.l3trim(i32::MAX), Err(MsgbError::ArithmeticOverflow));
    assert_eq!(format!("{msg:?}"), before);
    msg.set_layer_offset(3, Some(0)).unwrap();
    let before = format!("{msg:?}");
    assert_eq!(msg.l3trim(i32::MIN), Err(MsgbError::ArithmeticOverflow));
    assert_eq!(format!("{msg:?}"), before);
}

#[test]
fn rust_typed_push_supports_unaligned_offsets_without_pointer_casts() {
    let mut msg = Msgb::with_headroom(8, 7).unwrap();
    msg.backing_mut().fill(0xa5);
    msg.push_u32(0x12345678).unwrap(); // starts at offset 3
    msg.push_u16(0x89ab).unwrap(); // starts at offset 1
    assert_eq!(msg.data(), [0x89, 0xab, 0x12, 0x34, 0x56, 0x78]);
    assert_eq!(msg.pull_u16(), Ok(0x89ab));
    assert_eq!(msg.get_u32(), Ok(0x12345678));
    assert_eq!(
        msg.backing(),
        [0xa5, 0x89, 0xab, 0x12, 0x34, 0x56, 0x78, 0xa5]
    );
}
