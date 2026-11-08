use crate::gsm_utils_fixture::{self, Record};
use osmocore_rs::compat::gsm_utils as compat;
use osmocore_rs::gsm::gsm_utils::*;
use osmocore_rs::gsm::gsm0502::TDMA_HYPERFRAME;

const FIXTURE: &str =
    include_str!("../compatibility/vectors/gsm_utils/gsm_utils_numeric_vectors.ndjson");

fn records() -> Vec<Record> {
    gsm_utils_fixture::validate(FIXTURE)
}

/// Runs `check` on every record `pattern` matches, and fails if there are none.
macro_rules! for_each {
    ($pattern:pat => $check:block) => {
        let mut seen = 0;
        for record in records() {
            if let $pattern = record {
                seen += 1;
                $check
            }
        }
        assert!(seen > 0, "no records matched");
    };
}

fn band_result(code: i32) -> Option<Band> {
    let band = Band::from_code(code);
    assert_eq!(band.is_some(), code != -22, "band result {code}");
    band
}

fn optional_u16(value: u16, none: u16) -> Option<u16> {
    (value != none).then_some(value)
}

#[test]
fn fixture_is_well_formed() {
    gsm_utils_fixture::check_corruptions(FIXTURE);
}

#[test]
fn constants_match_c() {
    for_each!(Record::Constant { name, value } => {
        let band = |band: Band| i64::from(band.code());
        let tlli = |tlli: u32| i64::from(compat::gprs_tlli_type(tlli));
        let ours = match name.as_str() {
            "GSM_MAX_FN" => i64::from(TDMA_HYPERFRAME),
            "GSM_FN_UNSET" => i64::from(FN_UNSET),
            "GSM_RFN_MODULUS" => i64::from(RFN_MODULUS),
            "ARFCN_PCS" => i64::from(ARFCN_PCS),
            "ARFCN_UPLINK" => i64::from(ARFCN_UPLINK),
            "ARFCN_FLAG_MASK" => i64::from(ARFCN_FLAG_MASK),
            "GSM_BAND_850" => band(Band::Gsm850),
            "GSM_BAND_900" => band(Band::Gsm900),
            "GSM_BAND_1800" => band(Band::Dcs1800),
            "GSM_BAND_1900" => band(Band::Pcs1900),
            "GSM_BAND_450" => band(Band::Gsm450),
            "GSM_BAND_480" => band(Band::Gsm480),
            "GSM_BAND_750" => band(Band::Gsm750),
            "GSM_BAND_810" => band(Band::Gsm810),
            "TLLI_LOCAL" => tlli(0xc000_0000),
            "TLLI_FOREIGN" => tlli(0x8000_0000),
            "TLLI_RANDOM" => tlli(0x7800_0000),
            "TLLI_AUXILIARY" => tlli(0x7000_0000),
            "TLLI_RESERVED" => tlli(0x6000_0000),
            "TLLI_G_RNTI" => tlli(0),
            "TLLI_RAND_G_RNTI" => tlli(0x1000_0000),
            other => panic!("unmapped constant {other}"),
        };
        assert_eq!(ours, value, "{name}");
    });
}

#[test]
fn band_names_match_c() {
    for_each!(Record::BandName { band, name } => {
        assert_eq!(compat::gsm_band_name(band), name);
        if let Some(found) = Band::from_code(band) {
            assert_eq!(found.name(), name);
            assert_eq!(found.to_string(), name);
            assert_eq!(found.code(), band);
        } else {
            assert_eq!(name, "invalid");
        }
    });
}

#[test]
fn band_parsing_matches_c() {
    for_each!(Record::BandParse { input, band } => {
        assert_eq!(compat::gsm_band_parse(&input), band, "{input:?}");
        assert_eq!(Band::parse(&input), band_result(band), "{input:?}");
    });
}

#[test]
fn power_classes_match_c() {
    for_each!(Record::Class { band, class, dbm } => {
        assert_eq!(compat::ms_class_gmsk_dbm(band, class), dbm);
        if let (Some(found), Ok(class)) = (Band::from_code(band), u8::try_from(class)) {
            assert_eq!(found.ms_class_dbm(class), (dbm != -22).then_some(dbm));
        }
    });
}

#[test]
fn power_levels_match_c() {
    for_each!(Record::CtlLevel { band, dbm, level } => {
        assert_eq!(compat::ms_pwr_ctl_lvl(band, dbm), level);
        match Band::from_code(band) {
            Some(found) => assert_eq!(i32::from(found.ms_power_level(dbm)), level),
            None => assert_eq!(level, -22),
        }
    });
}

#[test]
fn power_level_dbm_matches_c() {
    for_each!(Record::PwrDbm { band, level, dbm } => {
        assert_eq!(compat::ms_pwr_dbm(band, level), dbm);
        match Band::from_code(band) {
            Some(found) => {
                assert_eq!(found.ms_power_dbm(level), (dbm != -22).then_some(dbm));
            }
            None => assert_eq!(dbm, -22),
        }
    });
}

#[test]
fn rxlev_conversions_match_c() {
    for_each!(Record::Rxlev { rxlev, dbm } => {
        assert_eq!(rxlev_to_dbm(rxlev), dbm);
    });
    for_each!(Record::Dbm { dbm, rxlev } => {
        assert_eq!(dbm_to_rxlev(dbm), rxlev, "{dbm}");
    });
}

#[test]
fn arfcn_results_match_c() {
    for_each!(Record::Arfcn { arfcn, rc, band, uplink, downlink } => {
        let mut written = -1;
        assert_eq!(compat::gsm_arfcn2band_rc(arfcn, &mut written), rc, "{arfcn}");
        assert_eq!(written, band, "{arfcn}");
        assert_eq!(
            Band::from_arfcn(arfcn).map_or(-1, Band::code),
            band,
            "{arfcn}"
        );
        assert_eq!(compat::gsm_arfcn2freq10(arfcn, 1), uplink, "{arfcn}");
        assert_eq!(compat::gsm_arfcn2freq10(arfcn, 7), uplink, "{arfcn}");
        assert_eq!(compat::gsm_arfcn2freq10(arfcn, 0), downlink, "{arfcn}");
        assert_eq!(
            arfcn_to_freq10(arfcn, Direction::Uplink),
            optional_u16(uplink, 0xffff),
            "{arfcn}"
        );
        assert_eq!(
            arfcn_to_freq10(arfcn, Direction::Downlink),
            optional_u16(downlink, 0xffff),
            "{arfcn}"
        );
    });
}

#[test]
fn frequency_results_match_c() {
    for_each!(Record::Freq { freq10, uplink, downlink } => {
        assert_eq!(compat::gsm_freq102arfcn(freq10, 1), uplink, "{freq10}");
        assert_eq!(compat::gsm_freq102arfcn(freq10, 2), uplink, "{freq10}");
        assert_eq!(compat::gsm_freq102arfcn(freq10, 0), downlink, "{freq10}");
        assert_eq!(
            freq10_to_arfcn(freq10, Direction::Uplink),
            optional_u16(uplink, 0xffff),
            "{freq10}"
        );
        assert_eq!(
            freq10_to_arfcn(freq10, Direction::Downlink),
            optional_u16(downlink, 0xffff),
            "{freq10}"
        );
    });
}

#[test]
fn gsm_time_from_a_frame_matches_c() {
    for_each!(Record::Time { frame, t1, t2, t3, tc, rfn, back, text } => {
        let fields = compat::gsm_fn2gsmtime(frame);
        assert_eq!(
            fields,
            compat::GsmTimeFields { frame, t1, t2, t3, tc },
            "{frame}"
        );
        assert_eq!(compat::gsm_gsmtime2fn(&fields), back, "{frame}");
        assert_eq!(compat::osmo_dump_gsmtime(&fields), text, "{frame}");
        assert_eq!(compat::gsm_fn_as_gsmtime_str(frame), text, "{frame}");
        assert_eq!(frame_to_rfn(frame), rfn, "{frame}");

        match GsmTime::from_frame(frame) {
            Some(time) => {
                assert!(frame < TDMA_HYPERFRAME);
                assert_eq!(
                    (time.frame(), time.t1(), time.t2(), time.t3(), time.tc()),
                    (frame, t1, t2, t3, tc)
                );
                assert_eq!(time.to_string(), text);
                assert_eq!(back, frame);
                assert_eq!(GsmTime::from_parts(t1, t2, t3), Some(time));
            }
            None => assert!(frame >= TDMA_HYPERFRAME),
        }
    });
}

#[test]
fn gsm_time_from_fields_matches_c() {
    for_each!(Record::TimeFields { input_frame, t1, t2, t3, frame, text } => {
        // The conversion ignores these two fields and the dump ignores `tc`.
        let fields = compat::GsmTimeFields { frame: input_frame, t1, t2, t3, tc: 7 };
        assert_eq!(compat::gsm_gsmtime2fn(&fields), frame, "{t1} {t2} {t3}");
        assert_eq!(compat::osmo_dump_gsmtime(&fields), text);

        let valid = t1 < 2048 && t2 < 26 && t3 < 51;
        match GsmTime::from_parts(t1, t2, t3) {
            Some(time) => {
                assert!(valid);
                assert_eq!(time.frame(), frame);
                assert_eq!((time.t1(), time.t2(), time.t3()), (t1, t2, t3));
            }
            None => assert!(!valid, "{t1} {t2} {t3}"),
        }
    });
}

#[test]
fn reduced_frame_numbers_match_c() {
    for_each!(Record::Rfn { rfn, current, frame, .. } => {
        assert_eq!(rfn_to_frame(rfn, current), Some(frame), "{rfn} {current}");
    });
}

#[test]
fn tlli_results_match_c() {
    for_each!(Record::Tlli { tlli, tlli_type, tmsi } => {
        assert_eq!(compat::gprs_tlli_type(tlli), tlli_type, "{tlli:#x}");
        assert_eq!(compat::gprs_tlli2tmsi(tlli), tmsi, "{tlli:#x}");
        let converts = matches!(TlliType::of(tlli), TlliType::Local | TlliType::Foreign);
        assert_eq!(tlli_to_tmsi(tlli), converts.then_some(tmsi), "{tlli:#x}");
    });
    for_each!(Record::Tmsi { tmsi, tlli_type, tlli } => {
        assert_eq!(compat::gprs_tmsi2tlli(tmsi, tlli_type), tlli);
        match tlli_type {
            0 => assert_eq!(local_tlli(tmsi), tlli),
            1 => assert_eq!(foreign_tlli(tmsi), tlli),
            _ => assert_eq!(tlli, 0),
        }
    });
}

#[test]
fn classmark_results_match_c() {
    for_each!(Record::Cm2 { first, third, n, supported } => {
        // C never reads the second octet, which the harness set to 0x55.
        let classmark = [first, 0x55, third];
        assert_eq!(compat::ms_cm2_a5n_support(&classmark, n), supported);
        assert_eq!(cm2_supports_a5(&classmark, n), supported == 1);
    });
    for_each!(Record::Cm3 { first, n, supported } => {
        assert_eq!(compat::ms_cm3_a5n_support(first, n), supported);
        assert_eq!(cm3_supports_a5(first, n), supported == 1);
    });
}

#[test]
fn rach_results_match_c() {
    for_each!(Record::RachMax { value, val2raw, raw2val } => {
        assert_eq!(compat::rach_max_trans_val2raw(value), val2raw, "{value}");
        assert_eq!(compat::rach_max_trans_raw2val(value), raw2val, "{value}");
        if let Ok(value) = u8::try_from(value) {
            assert_eq!(i32::from(rach_max_trans_to_raw(value)), val2raw);
            assert_eq!(i32::from(rach_max_trans_from_raw(value)), raw2val);
        }
    });
    for_each!(Record::RachTx { raw, value } => {
        assert_eq!(rach_tx_integer_from_raw(raw), value, "{raw}");
    });
}
