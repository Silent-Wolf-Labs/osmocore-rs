#[path = "../support/msgb_resize_fixture.rs"]
mod fixture;
use super::msgb_reference_vectors::{initial, snapshot};
use fixture::number;
use osmocore_rs::msgb::*;

#[test]
fn copies_and_area_resizes_match_all_c_observations() {
    let records = fixture::validated_records(include_str!(
        "../compatibility/vectors/msgb_resize_vectors.json"
    ));
    let mut current = None;
    for r in &records {
        if number(r, "step") == 0 {
            current = Some(initial(r));
        }
        let msg = current.as_mut().unwrap();
        snapshot(msg, r, "before");
        if r["api"] == "resize_area" {
            let result = msg.resize_area(
                number(r, "area") as usize,
                number(r, "old_size") as i32,
                number(r, "new_size") as i32,
            );
            assert_eq!(
                result.is_err(),
                number(r, "return_code") == -1,
                "case {}",
                r["case"]
            );
        } else {
            let result = if matches!(r["api"].as_str(), "copy" | "copy_c") {
                msg.copy()
            } else {
                msg.copy_resize(number(r, "arg") as usize)
            };
            assert_eq!(
                result.is_ok(),
                number(r, "copy_present") == 1,
                "case {}",
                r["case"]
            );
            if let Ok(copy) = result {
                assert_eq!(i64::from(copy.capacity()), number(r, "copy_capacity"));
                snapshot(&copy, r, "copy");
            }
        }
        snapshot(msg, r, "after");
    }
}
#[test]
fn rejects_corrupt_copy_resize_evidence() {
    fixture::check_corruptions(include_str!(
        "../compatibility/vectors/msgb_resize_vectors.json"
    ));
}
#[test]
fn copies_clear_unused_bytes_and_metadata_and_own_independent_storage() {
    let mut msg = Msgb::with_headroom(12, 3).unwrap();
    msg.put(4).unwrap();
    msg.backing_mut().fill(0xa5);
    msg.data_mut().copy_from_slice(&[1, 2, 3, 4]);
    msg.cb = [1, 2, 3, 4, 5];
    msg.set_layer_offset(2, Some(3)).unwrap();
    msg.set_layer_offset(4, Some(7)).unwrap();
    let before = format!("{msg:?}");
    let mut copy = msg.copy_resize(7).unwrap();
    assert_eq!(copy.backing(), [0, 0, 0, 1, 2, 3, 4]);
    assert_eq!(copy.cb, [0; 5]);
    assert_eq!(copy.layer_offsets(), msg.layer_offsets());
    copy.data_mut()[0] = 9;
    copy.cb[0] = 8;
    copy.set_layer_offset(2, None).unwrap();
    assert_eq!(format!("{msg:?}"), before);
    let copy = msg.copy().unwrap();
    assert_eq!(copy.backing(), [0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0, 0]);
    assert_eq!(copy.cb, [0; 5]);
    assert_eq!(format!("{msg:?}"), before);
}
#[test]
fn unsafe_c_copy_resize_and_extreme_inputs_are_atomic_errors() {
    let mut msg = Msgb::with_headroom(12, 3).unwrap();
    msg.put(4).unwrap();
    msg.backing_mut().fill(0xa5);
    msg.cb = [1, 2, 3, 4, 5];
    msg.set_layer_offset(1, Some(3)).unwrap();
    msg.set_layer_offset(4, Some(12)).unwrap();
    let before = format!("{msg:?}");
    assert_eq!(
        msg.copy_resize(65536).unwrap_err(),
        MsgbError::CapacityTooLarge
    );
    assert_eq!(
        msg.copy_resize(usize::MAX).unwrap_err(),
        MsgbError::CapacityTooLarge
    );
    assert_eq!(
        msg.copy_resize(6).unwrap_err(),
        MsgbError::NotEnoughTailroom
    );
    assert_eq!(msg.copy_resize(7).unwrap_err(), MsgbError::OffsetOutOfRange);
    // C would extend tail beyond storage despite passing its total-capacity check.
    assert_eq!(msg.resize_area(3, 0, 6), Err(MsgbError::NotEnoughTailroom));
    // Moving a stale one-past-end layer marker would form an invalid C pointer.
    assert_eq!(msg.resize_area(3, 0, 1), Err(MsgbError::OffsetOutOfRange));
    assert_eq!(
        msg.resize_area(usize::MAX, 0, 0),
        Err(MsgbError::AreaOutsideMessage)
    );
    assert_eq!(msg.resize_area(2, 0, 0), Err(MsgbError::AreaOutsideMessage));
    assert_eq!(msg.resize_area(8, 0, 0), Err(MsgbError::AreaOutsideMessage));
    assert_eq!(msg.resize_area(7, 1, 0), Err(MsgbError::AreaOutsideMessage));
    assert_eq!(
        msg.resize_area(3, i32::MAX, 0),
        Err(MsgbError::AreaOutsideMessage)
    );
    assert_eq!(
        msg.resize_area(3, 0, i32::MAX),
        Err(MsgbError::NotEnoughTailroom)
    );
    assert_eq!(msg.resize_area(0, -1, 0), Err(MsgbError::NegativeLength));
    assert_eq!(msg.resize_area(3, 0, -1), Err(MsgbError::NegativeLength));
    assert_eq!(format!("{msg:?}"), before);
}
#[test]
fn resizing_nullable_layers_preserves_unwritten_extension_and_removed_markers() {
    let mut msg = Msgb::with_headroom(12, 2).unwrap();
    msg.put(6).unwrap();
    msg.backing_mut()
        .copy_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    msg.cb = [1, 2, 3, 4, 5];
    msg.set_layer_offset(2, Some(4)).unwrap();
    msg.set_layer_offset(4, Some(6)).unwrap();
    msg.resize_area(4, 2, 3).unwrap();
    assert_eq!(msg.backing(), [0, 1, 2, 3, 4, 5, 6, 6, 7, 9, 10, 11]);
    assert_eq!(msg.layer_offsets(), [None, Some(4), None, Some(7)]);
    // Removing a whole area leaves markers inside it unchanged; only those at
    // its old end or later relocate. Shrinkage preserves the unused suffix.
    msg.set_layer_offset(3, Some(6)).unwrap();
    msg.resize_area(4, 3, 0).unwrap();
    assert_eq!(msg.backing(), [0, 1, 2, 3, 6, 7, 6, 6, 7, 9, 10, 11]);
    assert_eq!(msg.layer_offsets(), [None, Some(4), Some(6), Some(4)]);
    assert_eq!(msg.tail_offset(), 6);
    assert!(!msg.check_invariants());
    assert_eq!(msg.cb, [1, 2, 3, 4, 5]);
}
