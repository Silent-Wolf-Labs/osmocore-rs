//! Appending and prepending slices, which C does with `memcpy` into `msgb_put` and
//! `msgb_push`.

use osmocore_rs::msgb::{Msgb, MsgbError};

#[test]
fn put_and_push_slices_build_a_message() {
    let mut message = Msgb::with_headroom(8, 2).unwrap();
    message.put_slice(&[3, 4, 5]).unwrap();
    message.push_slice(&[1, 2]).unwrap();
    message.put_slice(&[]).unwrap();
    assert_eq!(message.data(), &[1, 2, 3, 4, 5]);
}

#[test]
fn slices_that_do_not_fit_leave_the_message_unchanged() {
    let mut message = Msgb::with_headroom(4, 1).unwrap();
    message.put_slice(&[7, 8]).unwrap();
    let state = |m: &Msgb| (m.data().to_vec(), m.headroom(), m.tailroom());
    let before = state(&message);
    assert_eq!(
        message.put_slice(&[1, 2]),
        Err(MsgbError::NotEnoughTailroom)
    );
    assert_eq!(
        message.push_slice(&[1, 2]),
        Err(MsgbError::NotEnoughHeadroom)
    );
    assert_eq!(state(&message), before);
    message.put_slice(&[9]).unwrap();
    message.push_slice(&[6]).unwrap();
    assert_eq!(message.data(), &[6, 7, 8, 9]);
}
