//! Results the C recording can't hold, and where the preferred API differs from C.

use osmocore_rs::compat::gsm48::*;
use osmocore_rs::compat::gsm23003::{OsmoPlmnId, osmo_plmn_from_bcd};
use osmocore_rs::gsm::gsm48::{
    Imei, Imeisv, Imsi, MOBILE_IDENTITY_MAX_BYTES, MobileIdentity, MobileIdentityError,
};
use osmocore_rs::gsm::gsm23003::{IdentityError, Lai, Mcc, Mnc, Plmn, Rai, is_valid_imei};

fn plmn(mcc: u16, mnc: u16, three_digits: bool) -> Plmn {
    Plmn::new(Mcc::new(mcc).unwrap(), Mnc::new(mnc, three_digits).unwrap())
}

#[test]
fn every_plmn_round_trips_through_bcd() {
    for mcc in 0..=999 {
        for (mnc, three) in [
            (0, false),
            (0, true),
            (1, false),
            (1, true),
            (99, false),
            (99, true),
            (100, true),
            (999, true),
        ] {
            let p = plmn(mcc, mnc, three);
            assert_eq!(Plmn::from_bcd(&p.to_bcd()), Ok(p));
        }
    }
}

#[test]
fn hex_bcd_digits_are_values_only_in_c() {
    let mut c = OsmoPlmnId::default();
    osmo_plmn_from_bcd(&[0xaa, 0xaa, 0xaa], &mut c);
    assert_eq!((c.mcc, c.mnc, c.mnc_3_digits), (1110, 1110, true));
    assert_eq!(Plmn::from_bcd(&[0xaa, 0xaa, 0xaa]), Err(IdentityError::Bcd));
    // An F is the two-digit marker only as the MNC's third digit.
    assert_eq!(Plmn::from_bcd(&[0x21, 0xf3, 0xf4]), Err(IdentityError::Bcd));
    assert_eq!(Plmn::from_bcd(&[0x2f, 0xf3, 0x54]), Err(IdentityError::Bcd));
    assert_eq!(
        Lai::from_bytes(&[0xaa, 0xaa, 0xaa, 0, 1]),
        Err(IdentityError::Bcd)
    );
    assert_eq!(
        Rai::from_bytes(&[0xaa, 0xaa, 0xaa, 0, 1, 2]),
        Err(IdentityError::Bcd)
    );
}

#[test]
fn every_identity_length_round_trips() {
    let digits = "9876543210987654";
    for len in 6..=15 {
        let imsi = MobileIdentity::Imsi(digits[..len].parse().unwrap());
        assert_eq!(imsi.encoded_len(), imsi.encode().len());
        assert_eq!(MobileIdentity::decode(&imsi.encode()), Ok(imsi));
    }
    for len in 14..=15 {
        let imei = MobileIdentity::Imei(digits[..len].parse().unwrap());
        assert_eq!(MobileIdentity::decode(&imei.encode()), Ok(imei));
    }
    let imeisv = MobileIdentity::Imeisv(digits.parse().unwrap());
    assert_eq!(imeisv.encode().len(), MOBILE_IDENTITY_MAX_BYTES);
    assert_eq!(MobileIdentity::decode(&imeisv.encode()), Ok(imeisv));
    for tmsi in [0, 1, 0xc0ffee, u32::MAX] {
        let identity = MobileIdentity::Tmsi(tmsi);
        assert_eq!(identity.encoded_len(), 5);
        assert_eq!(MobileIdentity::decode(&identity.encode()), Ok(identity));
    }
}

#[test]
fn digit_strings_take_only_their_lengths_and_decimal_digits() {
    assert_eq!("12345".parse::<Imsi>(), Err(MobileIdentityError::Digits));
    assert_eq!(
        "1234567890123456".parse::<Imsi>(),
        Err(MobileIdentityError::Digits)
    );
    assert_eq!("12345a".parse::<Imsi>(), Err(MobileIdentityError::Digits));
    assert_eq!(
        "1234567890123".parse::<Imei>(),
        Err(MobileIdentityError::Digits)
    );
    assert_eq!(
        "123456789012345".parse::<Imeisv>(),
        Err(MobileIdentityError::Digits)
    );
    assert_eq!("".parse::<Imsi>(), Err(MobileIdentityError::Digits));
}

#[test]
fn an_imei_check_digit_is_not_checked() {
    // 24.008 sends the check digit as a spare digit, so any value decodes.
    assert!(!is_valid_imei("357613004448480"));
    let imei: Imei = "357613004448480".parse().unwrap();
    let identity = MobileIdentity::Imei(imei);
    assert_eq!(MobileIdentity::decode(&identity.encode()), Ok(identity));
}

#[test]
fn hex_digits_decode_only_in_compat() {
    // An even IMSI: the type with A as its first digit, then 1 to 5 and the filler.
    let bytes = [0xa1, 0x21, 0x43, 0xf5];
    let mut mi = OsmoMobileIdentity::default();
    assert_eq!(osmo_mobile_identity_decode(&mut mi, Some(&bytes), true), 0);
    assert_eq!(mi.digits(), b"A12345");
    assert_eq!(
        osmo_mobile_identity_decode(&mut mi, Some(&bytes), false),
        -74
    );
    assert_eq!(
        MobileIdentity::decode(&bytes),
        Err(MobileIdentityError::Malformed)
    );
}

#[test]
fn no_identity_has_no_preferred_value() {
    // C's NONE type neither decodes nor encodes; the preferred API has no such identity.
    assert_eq!(
        MobileIdentity::decode(&[0xf0]),
        Err(MobileIdentityError::UnknownType)
    );
    let none = OsmoMobileIdentity::with_digits(GSM_MI_TYPE_NONE, b"123");
    assert_eq!(
        osmo_mobile_identity_encode_buf(Some(&mut [0; 9]), Some(&none), false),
        -95
    );
    assert_eq!(osmo_mobile_identity_to_str_c(Some(&none)), "NONE");
}

#[test]
fn identities_order_by_type_then_value() {
    let imsi = |digits: &str| MobileIdentity::Imsi(digits.parse().unwrap());
    assert!(imsi("923456") > imsi("1234567"));
    assert!(imsi("1234567") > imsi("123456"));
    assert!(imsi("999999") < MobileIdentity::Imei("12345678901234".parse().unwrap()));
    assert!(MobileIdentity::Imeisv("1234567890123456".parse().unwrap()) < MobileIdentity::Tmsi(0));
    assert!(MobileIdentity::Tmsi(1) < MobileIdentity::Tmsi(0x1000_0000));
}

#[test]
fn compat_union_text_ends_with_the_union() {
    // C's strlen would run past a union with no NUL.
    let mi = OsmoMobileIdentity {
        mi_type: GSM_MI_TYPE_IMSI,
        union: [b'1'; 17],
    };
    assert_eq!(mi.digits().len(), 17);
    let mut digits = 0;
    assert_eq!(
        osmo_mobile_identity_encoded_len(Some(&mi), Some(&mut digits)),
        -22
    );
}

#[test]
fn a_failed_decode_zeroes_the_tmsi_that_c_documents_as_reserved() {
    let mut mi = OsmoMobileIdentity::with_tmsi(7);
    assert_eq!(
        osmo_mobile_identity_decode(&mut mi, Some(&[0xf4, 1, 2]), false),
        -74
    );
    assert_eq!((mi.mi_type, mi.tmsi()), (GSM_MI_TYPE_NONE, 0));
    // The Complete Layer 3 decoder keeps the reserved TMSI when its own checks fail.
    assert_eq!(
        osmo_mobile_identity_decode_from_l3_buf(&mut mi, &[0x05], false),
        -74
    );
    assert_eq!(mi.tmsi(), GSM_RESERVED_TMSI);
}

#[test]
fn compat_cmp_returns_byte_differences_for_digits() {
    let a = OsmoMobileIdentity::with_digits(GSM_MI_TYPE_IMSI, b"123456");
    let b = OsmoMobileIdentity::with_digits(GSM_MI_TYPE_IMSI, b"923456");
    assert_eq!(osmo_mobile_identity_cmp(Some(&a), Some(&b)), -8);
    assert_eq!(osmo_mobile_identity_cmp(Some(&b), Some(&a)), 8);
    assert_eq!(osmo_mobile_identity_cmp(None, None), 0);
}

#[test]
fn messages_without_an_identity_are_not_carried() {
    // An MM Status message, and a GMM message.
    assert_eq!(
        MobileIdentity::from_l3(&[0x05, 0x31, 0x00]),
        Err(MobileIdentityError::NotCarried)
    );
    assert_eq!(
        MobileIdentity::from_l3(&[0x08, 0x01, 0x00]),
        Err(MobileIdentityError::NotCarried)
    );
    assert_eq!(
        MobileIdentity::from_l3(&[0x05]),
        Err(MobileIdentityError::Malformed)
    );
}

#[test]
fn errors_render_their_meaning() {
    assert_eq!(
        MobileIdentityError::NotCarried.to_string(),
        "message does not carry a Mobile Identity"
    );
    assert_eq!(
        IdentityError::Bcd.to_string(),
        "encoded PLMN digit is not decimal"
    );
}
