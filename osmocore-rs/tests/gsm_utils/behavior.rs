//! Behavior the C recording can't cover, because the C calls abort, overflow or have
//! no equivalent.

use osmocore_rs::compat::gsm_utils as compat;
use osmocore_rs::gsm::gsm_utils::*;
use osmocore_rs::gsm::gsm0502::TDMA_HYPERFRAME;

#[test]
fn an_rfn_that_aborts_in_c_is_none() {
    assert_eq!(rfn_to_frame(RFN_MODULUS, 0), None);
    assert_eq!(rfn_to_frame(u16::MAX, 0), None);
    assert_eq!(
        rfn_to_frame(RFN_MODULUS - 1, RFN_MODULUS.into()),
        Some(42431)
    );
}

#[test]
fn dbm_near_the_top_of_i32_stays_in_range() {
    // Adding 110 overflows in C here.
    assert_eq!(dbm_to_rxlev(i32::MAX), 63);
    assert_eq!(dbm_to_rxlev(i32::MAX - 109), 63);
}

#[test]
fn band_parsing_stops_at_a_nul_like_a_c_string() {
    assert_eq!(Band::parse("900\0 1800"), Some(Band::Gsm900));
    assert_eq!(Band::parse("GSM\0900"), None);
    assert_eq!(Band::parse("\0"), None);
}

#[test]
fn band_parsing_handles_text_that_is_not_ascii() {
    assert_eq!(Band::parse("é900é"), Some(Band::Gsm900));
    // A digit outside ASCII isn't a digit to C's isdigit in the C locale.
    assert_eq!(Band::parse("٩٠٠"), None);
}

#[test]
fn every_band_code_round_trips() {
    for band in Band::ALL {
        assert_eq!(Band::from_code(band.code()), Some(band));
    }
}

#[test]
fn sampled_frames_round_trip_through_t1_t2_t3() {
    for frame in (0..TDMA_HYPERFRAME).step_by(97) {
        let time = GsmTime::from_frame(frame).unwrap();
        assert_eq!(
            GsmTime::from_parts(time.t1(), time.t2(), time.t3()),
            Some(time)
        );
    }
}

#[test]
fn upstream_frame_recovery_check_holds() {
    // The same loop as upstream's test_gsm_gsmtime2fn, whose expected value is the
    // input frame number.
    for frame in 0..u32::from(RFN_MODULUS) {
        let fields = compat::GsmTimeFields {
            t1: ((frame / 1326) % 32) as u16,
            t2: (frame % 26) as u8,
            t3: (frame % 51) as u8,
            ..Default::default()
        };
        assert_eq!(compat::gsm_gsmtime2fn(&fields), frame);
    }
}

#[test]
fn frames_past_the_hyperframe_and_the_unset_value_have_no_time() {
    assert_eq!(GsmTime::from_frame(TDMA_HYPERFRAME), None);
    assert_eq!(GsmTime::from_frame(FN_UNSET), None);
    assert!(GsmTime::from_frame(TDMA_HYPERFRAME - 1).is_some());
}

#[test]
fn from_parts_rejects_each_field_on_its_own() {
    assert!(GsmTime::from_parts(2047, 25, 50).is_some());
    assert_eq!(GsmTime::from_parts(2048, 0, 0), None);
    assert_eq!(GsmTime::from_parts(0, 26, 0), None);
    assert_eq!(GsmTime::from_parts(0, 0, 51), None);
}

#[test]
fn a_tlli_of_all_ones_has_a_tmsi_where_c_returns_its_error_value() {
    assert_eq!(tlli_to_tmsi(u32::MAX), Some(u32::MAX));
    assert_eq!(compat::gprs_tlli2tmsi(u32::MAX), u32::MAX);
    assert_eq!(tlli_to_tmsi(0), None);
    assert_eq!(compat::gprs_tlli2tmsi(0), u32::MAX);
}

#[test]
fn an_uplink_arfcn_must_lose_its_flag_to_convert_back() {
    let arfcn = freq10_to_arfcn(8900, Direction::Uplink).unwrap();
    assert_eq!(arfcn, ARFCN_UPLINK);
    assert_eq!(arfcn_to_freq10(arfcn, Direction::Uplink), None);
    assert_eq!(
        arfcn_to_freq10(arfcn & !ARFCN_UPLINK, Direction::Uplink),
        Some(8900)
    );
}
