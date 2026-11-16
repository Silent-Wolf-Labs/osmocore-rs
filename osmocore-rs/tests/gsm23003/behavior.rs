//! Results the C recording can't hold, and where the preferred API differs from C.

use osmocore_rs::compat::gsm23003 as compat;
use osmocore_rs::gsm::gsm23003::*;
use std::collections::HashSet;

fn plmn(mcc: u16, mnc: u16, three_digits: bool) -> Plmn {
    Plmn::new(Mcc::new(mcc).unwrap(), Mnc::new(mnc, three_digits).unwrap())
}

#[test]
fn compat_text_ends_at_nul_but_preferred_rejects_it() {
    assert!(compat::osmo_imsi_str_valid(Some(b"123456\0abc")));
    assert!(!is_valid_imsi("123456\0abc"));

    let mut mnc = 0;
    assert_eq!(
        compat::osmo_mnc_from_str(Some(b"01\0x"), Some(&mut mnc), None),
        0
    );
    assert_eq!(mnc, 1);
    assert_eq!("01\0x".parse::<Mnc>(), Err(IdentityError::Digits));
}

#[test]
fn compat_null_text_is_invalid() {
    assert!(!compat::osmo_imsi_str_valid(None));
    assert!(!compat::osmo_msisdn_str_valid(None));
    assert!(!compat::osmo_imei_str_valid(None, true));
    let mut mcc = 7;
    assert_eq!(compat::osmo_mcc_from_str(None, Some(&mut mcc)), -22);
    assert_eq!(mcc, 7);
}

#[test]
fn values_past_three_digits_have_no_preferred_identity() {
    assert_eq!(Mcc::new(1000), None);
    assert_eq!(Mnc::new(1000, true), None);
    // C names and compares them, and its domain generators reject them.
    let c = compat::OsmoPlmnId {
        mcc: 1000,
        mnc: 65535,
        mnc_3_digits: false,
    };
    assert_eq!(compat::osmo_plmn_name(&c), "1000-65535");
    let mut out = [0x55; compat::GSM23003_HOME_NETWORK_DOMAIN_LEN + 1];
    assert_eq!(compat::osmo_gen_home_network_domain(&mut out, &c), -22);
}

#[test]
fn mnc_flag_counts_only_under_100() {
    assert_eq!(Mnc::new(100, false), Mnc::new(100, true));
    assert_ne!(Mnc::new(99, false), Mnc::new(99, true));
    let set: HashSet<Plmn> = [plmn(1, 100, false), plmn(1, 100, true)].into();
    assert_eq!(set.len(), 1);
    assert!(plmn(1, 1, false) < plmn(1, 1, true));
    assert!(plmn(1, 1, true) < plmn(1, 2, false));
}

#[test]
fn home_network_domains_round_trip_except_for_the_mnc_flag() {
    for mcc in [0, 1, 262, 901, 999] {
        for mnc in [0, 1, 42, 99, 100, 999] {
            for three in [false, true] {
                let p = plmn(mcc, mnc, three);
                let parsed = Plmn::from_home_network_domain(&p.home_network_domain()).unwrap();
                // The domain always has three MNC digits, so a two-digit MNC comes back.
                assert_eq!(parsed, plmn(mcc, mnc, false));
                let gummei = Gummei {
                    plmn: p,
                    mme: MmeId {
                        group_id: 0xa123,
                        code: 0xb1,
                    },
                };
                let domain = gummei.mme_domain();
                assert_eq!(domain.len(), MME_DOMAIN_LEN);
                assert_eq!(
                    Gummei::from_mme_domain(&domain).unwrap(),
                    Gummei {
                        plmn: parsed,
                        ..gummei
                    }
                );
            }
        }
    }
}

#[test]
fn preferred_parsers_reject_what_c_accepts_loosely() {
    for domain in [
        "epc.mnc070.mcc901.example.com",
        "epc.mnc070.mcc901",
        "epc.mnc70.mcc901.3gppnetwork.org",
        "epc.mnc-01.mcc901.3gppnetwork.org",
        "epc.mnc 70.mcc901.3gppnetwork.org",
        "epc.mnc070.mcc9010.3gppnetwork.org",
    ] {
        let mut c = compat::OsmoPlmnId::default();
        assert_eq!(
            compat::osmo_parse_home_network_domain(&mut c, domain.as_bytes()),
            0
        );
        assert_eq!(
            Plmn::from_home_network_domain(domain),
            Err(IdentityError::Domain),
            "{domain}"
        );
    }
    let mut c = compat::OsmoGummei::default();
    let domain = "mmec0x.mmegi0001.mme.epc.mnc070.mcc901.3gppnetwork.org";
    assert_eq!(compat::osmo_parse_mme_domain(&mut c, domain.as_bytes()), 0);
    assert_eq!(Gummei::from_mme_domain(domain), Err(IdentityError::Domain));
    assert_eq!(
        Plmn::from_home_network_domain(""),
        Err(IdentityError::Domain)
    );
    assert_eq!(
        compat::osmo_parse_home_network_domain(&mut compat::OsmoPlmnId::default(), b""),
        -1
    );
}

#[test]
fn compat_home_domain_buffer_holds_the_nul_c_documents_no_room_for() {
    // C asks for 33 bytes and writes 34; the compat buffer is sized for what it writes.
    let mut out = [0x55; compat::GSM23003_HOME_NETWORK_DOMAIN_LEN + 1];
    let c = compat::OsmoPlmnId {
        mcc: 901,
        mnc: 70,
        mnc_3_digits: false,
    };
    assert_eq!(compat::osmo_gen_home_network_domain(&mut out, &c), 33);
    assert_eq!(out[33], 0);
}

#[test]
fn imei_check_digit_is_checked_only_with_it() {
    assert!(is_valid_imei("357613004448485"));
    assert!(!is_valid_imei("357613004448480"));
    assert!(!is_valid_imei("35761300444848"));
    assert!(is_valid_imei_without_check_digit("35761300444848"));
    assert!(!is_valid_imei_without_check_digit("357613004448485"));
}
