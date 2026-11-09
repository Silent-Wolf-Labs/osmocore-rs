//! Results the C recording can't hold: inputs where C aborts, Rust types C doesn't have,
//! and TUAK, which isn't implemented yet.

use osmocore_rs::compat::auth::{self as compat, OsmoAuthVector, OsmoSubAuthData2};
use osmocore_rs::gsm::auth::{
    AuthError, K, KeyLengthError, Opc, OperatorKey, Res, ResLength, UmtsAlgorithm, UmtsSubscriber,
    next_sqn,
};

#[test]
fn compat_c2_rejects_what_c_aborts_on() {
    let mut sres = [7; 4];
    assert_eq!(compat::osmo_auth_c2(&mut sres, &[1; 8], 0), -22);
    assert_eq!(compat::osmo_auth_c2(&mut sres, &[1; 8], 3), -22);
    assert_eq!(compat::osmo_auth_c2(&mut sres, &[1; 17], 1), -22);
    assert_eq!(sres, [7; 4]);
}

#[test]
fn keys_and_responses_check_their_length() {
    assert_eq!(K::new(&[0; 15]), Err(KeyLengthError(15)));
    assert_eq!(Opc::new(&[0; 24]), Err(KeyLengthError(24)));
    assert!(K::new(&[0; 32]).unwrap().as_128().is_none());
    assert_eq!(Res::new(&[0; 3]), None);
    assert_eq!(Res::new(&[0; 17]), None);
    assert_eq!(Res::new(&[1; 4]).unwrap().as_bytes(), &[1; 4]);
}

#[test]
fn debug_hides_keys() {
    let k = K::new(&[0x5a; 16]).unwrap();
    assert_eq!(format!("{k:?}"), "K(16 bytes)");
    let subscriber = UmtsSubscriber::new(
        UmtsAlgorithm::Xor3g,
        k,
        OperatorKey::Opc(Opc::new(&[0x5a; 16]).unwrap()),
    );
    // 0x5a is 90.
    assert!(!format!("{subscriber:?}").contains("90"));
}

#[test]
fn next_sqn_wraps_and_checks_ind() {
    assert_eq!(next_sqn(0x21, 0, 0), Ok(0x22));
    assert_eq!(next_sqn(31, 5, 10), Ok(42));
    assert_eq!(next_sqn(u64::MAX, 0, 0), Ok(0));
    assert_eq!(next_sqn(0, 29, 0), Err(AuthError::IndBitlen(29)));
    assert_eq!(
        next_sqn(0, 5, 32),
        Err(AuthError::IndRange {
            ind: 32,
            ind_bitlen: 5
        })
    );
}

#[test]
fn xor_3g_keeps_sqn_and_ignores_the_res_length() {
    let mut subscriber = UmtsSubscriber::new(
        UmtsAlgorithm::Xor3g,
        K::new(&[1; 16]).unwrap(),
        OperatorKey::Opc(Opc::new(&[0; 32]).unwrap()),
    );
    subscriber.sqn = 41;
    let vector = subscriber.vector(&[2; 16], ResLength::Four).unwrap();
    assert_eq!(vector.res.len(), 16);
    assert_eq!(subscriber.sqn, 41);
}

#[test]
fn compat_reports_tuak_as_missing() {
    // C has TUAK; it comes in a later step.
    let mut aud = OsmoSubAuthData2 {
        auth_type: compat::OSMO_AUTH_TYPE_UMTS,
        algo: compat::OSMO_AUTH_ALG_TUAK,
        ..OsmoSubAuthData2::default()
    };
    let mut vec = OsmoAuthVector::default();
    assert_eq!(compat::osmo_auth_gen_vec2(&mut vec, &mut aud, &[0; 16]), -2);
    assert_eq!(
        compat::osmo_auth_gen_vec_auts2(&mut vec, &mut aud, &[0; 14], &[0; 16], &[0; 16]),
        -2
    );
    assert_eq!(vec, OsmoAuthVector::default());
}

#[cfg(feature = "auth")]
#[test]
fn milenage_debug_hides_keys() {
    use osmocore_rs::gsm::milenage::Milenage;
    assert_eq!(
        format!("{:?}", Milenage::new(&[0x5a; 16], &[0x5a; 16])),
        "Milenage(..)"
    );
}

#[cfg(feature = "auth")]
#[test]
fn resync_checks_ind_before_auts() {
    // C checks AUTS first and returns -1 here; Rust reports the IND it would fail on.
    let mut subscriber = UmtsSubscriber::new(
        UmtsAlgorithm::Milenage,
        K::new(&[1; 16]).unwrap(),
        OperatorKey::Opc(Opc::new(&[2; 16]).unwrap()),
    );
    subscriber.sqn = 7;
    subscriber.ind_bitlen = 5;
    subscriber.ind = 32;
    let result = subscriber.resync(&[0; 14], &[0; 16], &[0; 16], ResLength::Eight);
    assert_eq!(
        result,
        Err(AuthError::IndRange {
            ind: 32,
            ind_bitlen: 5
        })
    );
    assert_eq!(subscriber.sqn, 7);
}
