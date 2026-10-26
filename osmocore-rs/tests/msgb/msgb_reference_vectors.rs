use crate::msgb_fixture as fixture;
use fixture::{Record, bytes, number, words};
use osmocore_rs::compat::msgb::{msgb_data, msgb_headlen};
use osmocore_rs::msgb::*;

pub(super) fn snapshot(msg: &Msgb, r: &Record, prefix: &str) {
    let n = |key| number(r, &format!("{prefix}_{key}"));
    assert_eq!(msg.backing(), bytes(&r[&format!("{prefix}_hex")]));
    assert_eq!(msg.data_offset() as i64, n("data"));
    assert_eq!(msg.tail_offset() as i64, n("tail"));
    assert_eq!(i64::from(msg.len()), n("len"));
    assert_eq!(i64::from(msg.len()), n("length"));
    assert_eq!(msgb_data(msg) as i64, n("data_return"));
    assert_eq!(i64::from(msgb_headlen(msg)), n("headlen"));
    assert_eq!(i64::from(msg.headroom()), n("headroom"));
    assert_eq!(i64::from(msg.tailroom()), n("tailroom"));
    assert_eq!(i64::from(msg.check_invariants()), n("invariant"));
    assert_eq!(
        msg.layer_offsets()
            .map(|o| o.map_or(-1, |v| v as i64))
            .to_vec(),
        words(&r[&format!("{prefix}_layers")], 4)
    );
    assert_eq!(
        msg.cb.map(|v| v as i64).to_vec(),
        words(&r[&format!("{prefix}_cb")], 5)
    );
    assert_eq!(
        msg.data(),
        &msg.backing()[msg.data_offset()..msg.tail_offset()]
    );
}
pub(super) fn initial(r: &Record) -> Msgb {
    let mut msg = Msgb::with_headroom(
        number(r, "capacity") as usize,
        number(r, "before_data") as usize,
    )
    .unwrap();
    msg.put(number(r, "before_len") as usize).unwrap();
    msg.backing_mut().copy_from_slice(&bytes(&r["before_hex"]));
    for (i, offset) in words(&r["before_layers"], 4).into_iter().enumerate() {
        msg.set_layer_offset(
            i + 1,
            if offset < 0 {
                None
            } else {
                Some(offset as usize)
            },
        )
        .unwrap();
    }
    msg.cb = words(&r["before_cb"], 5)
        .into_iter()
        .map(|v| v as u64)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    msg
}

#[test]
fn primitives_match_all_c_observations() {
    let records =
        fixture::validated_records(include_str!("../compatibility/vectors/msgb_vectors.json"));
    let mut current: Option<Msgb> = None;
    for r in &records {
        if number(r, "step") == 0 {
            current = Some(initial(r));
        }
        let msg = current.as_mut().unwrap();
        snapshot(msg, r, "before");
        assert_eq!(i64::from(msg.capacity()), number(r, "capacity"));
        let arg = number(r, "arg");
        let outcome: Result<i64, MsgbError> = match r["api"].as_str() {
            "put" => msg.put(arg as usize).map(|o| o as i64),
            "get" => msg.get(arg as usize).map(|o| o as i64),
            "push" => msg.push(arg as usize).map(|o| o as i64),
            "pull" => msg.pull(arg as usize).map(|o| o as i64),
            "trim" => msg.trim(arg as i32).map(|()| 0),
            "reserve" => msg.reserve(arg as usize).map(|()| -1),
            "reset" => {
                msg.reset();
                Ok(-1)
            }
            "alloc" | "invariant" => Ok(-1),
            api => panic!("unknown API {api}"),
        };
        let expected_error =
            number(r, "panic") == 1 || (r["api"] == "trim" && number(r, "return_code") == -1);
        assert_eq!(outcome.is_err(), expected_error, "case {}", r["case"]);
        if let Ok(value) = outcome {
            assert_eq!(value, number(r, "return_code"), "case {}", r["case"]);
        }
        snapshot(msg, r, "after");
    }
}

#[test]
fn rust_safety_errors_leave_bytes_and_metadata_unchanged() {
    assert_eq!(Msgb::new(65536).unwrap_err(), MsgbError::CapacityTooLarge);
    assert_eq!(
        Msgb::new(usize::MAX).unwrap_err(),
        MsgbError::CapacityTooLarge
    );
    assert_eq!(
        Msgb::with_headroom(8, 9).unwrap_err(),
        MsgbError::NotEnoughTailroom
    );
    let mut msg = Msgb::with_headroom(8, 4).unwrap();
    msg.backing_mut().fill(0xa5);
    msg.set_layer_offset(1, Some(8)).unwrap();
    msg.cb = [1, 2, 3, 4, 5];
    let before = format!("{msg:?}");
    assert_eq!(msg.trim(5), Err(MsgbError::NotEnoughTailroom));
    assert_eq!(msg.trim(-1), Err(MsgbError::NegativeLength));
    assert_eq!(msg.put(usize::MAX), Err(MsgbError::NotEnoughTailroom));
    assert_eq!(msg.push(usize::MAX), Err(MsgbError::NotEnoughHeadroom));
    assert_eq!(msg.get(usize::MAX), Err(MsgbError::MessageTooShort));
    assert_eq!(msg.pull(usize::MAX), Err(MsgbError::MessageTooShort));
    assert_eq!(msg.reserve(usize::MAX), Err(MsgbError::NotEnoughTailroom));
    assert_eq!(
        msg.set_layer_offset(0, Some(0)),
        Err(MsgbError::InvalidLayer)
    );
    assert_eq!(msg.set_layer_offset(5, None), Err(MsgbError::InvalidLayer));
    assert_eq!(
        msg.set_layer_offset(1, Some(usize::MAX)),
        Err(MsgbError::OffsetOutOfRange)
    );
    assert_eq!(format!("{msg:?}"), before);
    msg.put(1).unwrap();
    let before = format!("{msg:?}");
    assert_eq!(msg.reserve(0), Err(MsgbError::ReserveOnNonempty));
    assert_eq!(msg.reserve(1), Err(MsgbError::ReserveOnNonempty));
    assert_eq!(format!("{msg:?}"), before);
}

#[test]
fn owned_storage_initialization_and_byte_access() {
    let mut msg = Msgb::new(8).unwrap();
    assert_eq!(msg.backing(), [0; 8]);
    msg.reserve(2).unwrap();
    let offset = msg.put(3).unwrap();
    msg.backing_mut()[offset..offset + 3].copy_from_slice(&[1, 2, 3]);
    assert_eq!(msg.data(), [1, 2, 3]);
    msg.data_mut()[1] = 9;
    assert_eq!(msg.get(1), Ok(4));
    assert_eq!(msg.pull(1), Ok(3));
    assert_eq!(msg.data(), [9]);
    assert_eq!(msg.backing(), [0, 0, 1, 9, 3, 0, 0, 0]);
    msg.reset();
    assert!(msg.data().is_empty());
    assert_eq!(msg.backing(), [0, 0, 1, 9, 3, 0, 0, 0]);
}

#[test]
fn rejects_corrupt_schema_provenance_and_observations() {
    fixture::check_corruptions(include_str!("../compatibility/vectors/msgb_vectors.json"));
}
