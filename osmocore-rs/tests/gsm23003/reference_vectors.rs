use crate::gsm23003_fixture::{self as fixture, Body, Check, GenInput, Id, Parsed, Record};
use osmocore_rs::compat::gsm23003::*;
use osmocore_rs::gsm::gsm23003::{self as preferred, *};

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm23003/gsm23003_vectors.ndjson");
const GUARD: u8 = 0x55;

fn records() -> Vec<Record> {
    fixture::validate(FIXTURE)
}

fn bodies() -> impl Iterator<Item = Body> {
    records().into_iter().map(|record| record.body)
}

// ---- The fixture's identities as compat structs ----

fn c_plmn(p: &fixture::Plmn) -> OsmoPlmnId {
    OsmoPlmnId {
        mcc: p.mcc,
        mnc: p.mnc,
        mnc_3_digits: p.mnc_3_digits,
    }
}

fn c_lai(l: &fixture::Lai) -> OsmoLocationAreaId {
    OsmoLocationAreaId {
        plmn: c_plmn(&l.plmn),
        lac: l.lac,
    }
}

fn c_rai(r: &fixture::Rai) -> OsmoRoutingAreaId {
    OsmoRoutingAreaId {
        lac: c_lai(&r.lai),
        rac: r.rac,
    }
}

fn c_mme(m: &fixture::MmeId) -> OsmoMmeId {
    OsmoMmeId {
        group_id: m.group_id,
        code: m.code,
    }
}

fn c_gummei(g: &fixture::Gummei) -> OsmoGummei {
    OsmoGummei {
        plmn: c_plmn(&g.plmn),
        mme: c_mme(&g.mme),
    }
}

// ---- The fixture's identities as preferred types, where they're valid ----

fn plmn(p: &fixture::Plmn) -> Option<Plmn> {
    Some(Plmn::new(
        Mcc::new(p.mcc)?,
        Mnc::new(p.mnc, p.mnc_3_digits)?,
    ))
}

fn lai(l: &fixture::Lai) -> Option<Lai> {
    Some(Lai {
        plmn: plmn(&l.plmn)?,
        lac: l.lac,
    })
}

fn rai(r: &fixture::Rai) -> Option<Rai> {
    Some(Rai {
        lai: lai(&r.lai)?,
        rac: r.rac,
    })
}

fn mme(m: &fixture::MmeId) -> MmeId {
    MmeId {
        group_id: m.group_id,
        code: m.code,
    }
}

fn gummei(g: &fixture::Gummei) -> Option<Gummei> {
    Some(Gummei {
        plmn: plmn(&g.plmn)?,
        mme: mme(&g.mme),
    })
}

fn in_range(p: &fixture::Plmn) -> bool {
    p.mcc <= 999 && p.mnc <= 999
}

/// The compat name of any identity.
fn compat_name(id: &Id) -> String {
    match id {
        Id::Mcc(mcc) => osmo_mcc_name(*mcc),
        Id::Mnc { mnc, mnc_3_digits } => osmo_mnc_name(*mnc, *mnc_3_digits),
        Id::Plmn(p) => osmo_plmn_name(&c_plmn(p)),
        Id::Lai(l) => osmo_lai_name(&c_lai(l)),
        Id::Rai(r) => osmo_rai_name2(&c_rai(r)),
        Id::Cgi(c) => osmo_cgi_name(&OsmoCellGlobalId {
            lai: c_lai(&c.lai),
            cell_identity: c.ci,
        }),
        Id::CgiPs(c) => osmo_cgi_ps_name(&OsmoCellGlobalIdPs {
            rai: c_rai(&c.rai),
            cell_identity: c.ci,
        }),
        Id::Sai(s) => osmo_sai_name(&OsmoServiceAreaId {
            lai: c_lai(&s.lai),
            sac: s.sac,
        }),
        Id::RncId(r) => osmo_rnc_id_name(&OsmoRncId {
            plmn: c_plmn(&r.plmn),
            rnc_id: r.rnc_id,
        }),
        Id::Gummei(g) => osmo_gummei_name(&c_gummei(g)),
        Id::MmeId(_) => unreachable!("C has no MME ID name"),
    }
}

/// The preferred name, or `None` where the MCC or MNC is past 999.
fn preferred_name(id: &Id) -> Option<String> {
    Some(match id {
        Id::Mcc(mcc) => Mcc::new(*mcc)?.to_string(),
        Id::Mnc { mnc, mnc_3_digits } => Mnc::new(*mnc, *mnc_3_digits)?.to_string(),
        Id::Plmn(p) => plmn(p)?.to_string(),
        Id::Lai(l) => lai(l)?.to_string(),
        Id::Rai(r) => rai(r)?.to_string(),
        Id::Cgi(c) => Cgi {
            lai: lai(&c.lai)?,
            ci: c.ci,
        }
        .to_string(),
        Id::CgiPs(c) => CgiPs {
            rai: rai(&c.rai)?,
            ci: c.ci,
        }
        .to_string(),
        Id::Sai(s) => Sai {
            lai: lai(&s.lai)?,
            sac: s.sac,
        }
        .to_string(),
        Id::RncId(r) => RncId {
            plmn: plmn(&r.plmn)?,
            rnc_id: r.rnc_id,
        }
        .to_string(),
        Id::Gummei(g) => gummei(g)?.to_string(),
        Id::MmeId(_) => unreachable!("C has no MME ID name"),
    })
}

fn compat_cmp(a: &Id, b: &Id) -> i32 {
    match (a, b) {
        (
            Id::Mnc {
                mnc: a,
                mnc_3_digits: a3,
            },
            Id::Mnc {
                mnc: b,
                mnc_3_digits: b3,
            },
        ) => osmo_mnc_cmp(*a, *a3, *b, *b3),
        (Id::Plmn(a), Id::Plmn(b)) => osmo_plmn_cmp(&c_plmn(a), &c_plmn(b)),
        (Id::Lai(a), Id::Lai(b)) => osmo_lai_cmp(&c_lai(a), &c_lai(b)),
        (Id::Rai(a), Id::Rai(b)) => osmo_rai_cmp(&c_rai(a), &c_rai(b)),
        (Id::Cgi(a), Id::Cgi(b)) => osmo_cgi_cmp(
            &OsmoCellGlobalId {
                lai: c_lai(&a.lai),
                cell_identity: a.ci,
            },
            &OsmoCellGlobalId {
                lai: c_lai(&b.lai),
                cell_identity: b.ci,
            },
        ),
        (Id::CgiPs(a), Id::CgiPs(b)) => osmo_cgi_ps_cmp(
            &OsmoCellGlobalIdPs {
                rai: c_rai(&a.rai),
                cell_identity: a.ci,
            },
            &OsmoCellGlobalIdPs {
                rai: c_rai(&b.rai),
                cell_identity: b.ci,
            },
        ),
        (Id::RncId(a), Id::RncId(b)) => osmo_rnc_id_cmp(
            &OsmoRncId {
                plmn: c_plmn(&a.plmn),
                rnc_id: a.rnc_id,
            },
            &OsmoRncId {
                plmn: c_plmn(&b.plmn),
                rnc_id: b.rnc_id,
            },
        ),
        (Id::MmeId(a), Id::MmeId(b)) => osmo_mme_id_cmp(&c_mme(a), &c_mme(b)),
        (Id::Gummei(a), Id::Gummei(b)) => osmo_gummei_cmp(&c_gummei(a), &c_gummei(b)),
        _ => unreachable!("the reader pairs only identities of one type"),
    }
}

/// The preferred order as C's sign, or `None` where an MCC or MNC is past 999.
fn preferred_cmp(a: &Id, b: &Id) -> Option<i32> {
    fn sign<T: Ord>(a: T, b: T) -> Option<i32> {
        Some(a.cmp(&b) as i32)
    }
    match (a, b) {
        (
            Id::Mnc {
                mnc: a,
                mnc_3_digits: a3,
            },
            Id::Mnc {
                mnc: b,
                mnc_3_digits: b3,
            },
        ) => sign(Mnc::new(*a, *a3)?, Mnc::new(*b, *b3)?),
        (Id::Plmn(a), Id::Plmn(b)) => sign(plmn(a)?, plmn(b)?),
        (Id::Lai(a), Id::Lai(b)) => sign(lai(a)?, lai(b)?),
        (Id::Rai(a), Id::Rai(b)) => sign(rai(a)?, rai(b)?),
        (Id::Cgi(a), Id::Cgi(b)) => sign(
            Cgi {
                lai: lai(&a.lai)?,
                ci: a.ci,
            },
            Cgi {
                lai: lai(&b.lai)?,
                ci: b.ci,
            },
        ),
        (Id::CgiPs(a), Id::CgiPs(b)) => sign(
            CgiPs {
                rai: rai(&a.rai)?,
                ci: a.ci,
            },
            CgiPs {
                rai: rai(&b.rai)?,
                ci: b.ci,
            },
        ),
        (Id::RncId(a), Id::RncId(b)) => sign(
            RncId {
                plmn: plmn(&a.plmn)?,
                rnc_id: a.rnc_id,
            },
            RncId {
                plmn: plmn(&b.plmn)?,
                rnc_id: b.rnc_id,
            },
        ),
        (Id::MmeId(a), Id::MmeId(b)) => sign(mme(a), mme(b)),
        (Id::Gummei(a), Id::Gummei(b)) => sign(gummei(a)?, gummei(b)?),
        _ => unreachable!("the reader pairs only identities of one type"),
    }
}

/// The input as preferred text, where it's UTF-8 with no NUL, which C would end at.
fn as_text(input: &Option<Vec<u8>>) -> Option<&str> {
    let bytes = input.as_deref()?;
    std::str::from_utf8(bytes)
        .ok()
        .filter(|text| !text.contains('\0'))
}

#[test]
fn fixture_is_well_formed() {
    fixture::check_corruptions(FIXTURE);
}

#[test]
fn constants_match_c() {
    let constants: Vec<(String, i64)> = bodies()
        .filter_map(|body| match body {
            Body::Constant { name, value } => Some((name, value)),
            _ => None,
        })
        .collect();
    let expected = [
        ("GSM23003_IMSI_MIN_DIGITS", GSM23003_IMSI_MIN_DIGITS),
        ("GSM23003_IMSI_MAX_DIGITS", GSM23003_IMSI_MAX_DIGITS),
        ("GSM23003_MSISDN_MIN_DIGITS", GSM23003_MSISDN_MIN_DIGITS),
        ("GSM23003_MSISDN_MAX_DIGITS", GSM23003_MSISDN_MAX_DIGITS),
        ("GSM23003_IMEI_NUM_DIGITS", GSM23003_IMEI_NUM_DIGITS),
        (
            "GSM23003_IMEI_NUM_DIGITS_NO_CHK",
            GSM23003_IMEI_NUM_DIGITS_NO_CHK,
        ),
        (
            "GSM23003_HOME_NETWORK_DOMAIN_LEN",
            GSM23003_HOME_NETWORK_DOMAIN_LEN,
        ),
        ("GSM23003_MME_DOMAIN_LEN", GSM23003_MME_DOMAIN_LEN),
        // The result the compat docs give for an error.
        ("EINVAL", 22),
    ]
    .map(|(name, value)| (name.to_owned(), value as i64));
    assert_eq!(constants, expected);

    assert_eq!(preferred::IMSI_MIN_DIGITS, GSM23003_IMSI_MIN_DIGITS);
    assert_eq!(preferred::IMSI_MAX_DIGITS, GSM23003_IMSI_MAX_DIGITS);
    assert_eq!(preferred::MSISDN_MIN_DIGITS, GSM23003_MSISDN_MIN_DIGITS);
    assert_eq!(preferred::MSISDN_MAX_DIGITS, GSM23003_MSISDN_MAX_DIGITS);
    assert_eq!(preferred::IMEI_DIGITS, GSM23003_IMEI_NUM_DIGITS);
    assert_eq!(
        preferred::HOME_NETWORK_DOMAIN_LEN,
        GSM23003_HOME_NETWORK_DOMAIN_LEN
    );
    // C's MME length counts the NUL, unlike its home network length.
    assert_eq!(preferred::MME_DOMAIN_LEN + 1, GSM23003_MME_DOMAIN_LEN);
}

#[test]
fn validity_checks_match_c() {
    let mut preferred_checked = 0;
    for body in bodies() {
        let Body::Valid {
            check,
            input,
            result,
        } = body
        else {
            continue;
        };
        let c_input = input.as_deref();
        let compat = match check {
            Check::Imsi => osmo_imsi_str_valid(c_input),
            Check::Msisdn => osmo_msisdn_str_valid(c_input),
            Check::Imei => osmo_imei_str_valid(c_input, false),
            Check::ImeiWithCheckDigit => osmo_imei_str_valid(c_input, true),
        };
        assert_eq!(compat, result, "{check:?} {input:?}");
        if let Some(text) = as_text(&input) {
            let preferred = match check {
                Check::Imsi => is_valid_imsi(text),
                Check::Msisdn => is_valid_msisdn(text),
                Check::Imei => is_valid_imei_without_check_digit(text),
                Check::ImeiWithCheckDigit => is_valid_imei(text),
            };
            assert_eq!(preferred, result, "{check:?} {text:?}");
            preferred_checked += 1;
        }
    }
    assert!(preferred_checked > 0);
}

#[test]
fn mcc_and_mnc_parsing_matches_c() {
    for body in bodies() {
        let Body::FromStr {
            input,
            rc,
            mnc,
            mnc_3_digits,
            mcc_rc,
            mcc,
        } = body
        else {
            continue;
        };
        let (mut got_mnc, mut got_3_digits, mut got_mcc) =
            (fixture::JUNK_U16, true, fixture::JUNK_U16);
        assert_eq!(
            osmo_mnc_from_str(
                input.as_deref(),
                Some(&mut got_mnc),
                Some(&mut got_3_digits)
            ),
            rc,
            "{input:?}"
        );
        assert_eq!((got_mnc, got_3_digits), (mnc, mnc_3_digits), "{input:?}");
        assert_eq!(
            osmo_mcc_from_str(input.as_deref(), Some(&mut got_mcc)),
            mcc_rc
        );
        assert_eq!(got_mcc, mcc, "{input:?}");

        if let Some(text) = as_text(&input) {
            let parsed_mnc = text.parse::<Mnc>();
            let parsed_mcc = text.parse::<Mcc>();
            if rc == 0 {
                assert_eq!(parsed_mnc, Ok(Mnc::new(mnc, mnc_3_digits).unwrap()));
                assert_eq!(
                    parsed_mnc.unwrap().is_three_digits(),
                    mnc_3_digits || mnc > 99
                );
                assert_eq!(parsed_mcc, Ok(Mcc::new(mcc).unwrap()));
            } else {
                assert_eq!(parsed_mnc, Err(IdentityError::Digits), "{text:?}");
                assert_eq!(parsed_mcc, Err(IdentityError::Digits), "{text:?}");
            }
        }
    }
}

#[test]
fn names_match_c() {
    let mut out_of_range = 0;
    for body in bodies() {
        let Body::Name { id, text } = body else {
            continue;
        };
        assert_eq!(compat_name(&id), text, "{id:?}");
        match preferred_name(&id) {
            Some(name) => assert_eq!(name, text, "{id:?}"),
            None => out_of_range += 1,
        }
    }
    // MCC or MNC 1000 and 65535 have no preferred value.
    assert!(out_of_range > 0);
}

#[test]
fn short_name_buffers_match_c() {
    for body in bodies() {
        let Body::NameBuf { id, buf } = body else {
            continue;
        };
        let mut got = vec![GUARD; buf.len()];
        match id {
            Id::Plmn(p) => {
                osmo_plmn_name_buf(&mut got, &c_plmn(&p));
            }
            Id::CgiPs(c) => {
                osmo_cgi_ps_name_buf(
                    &mut got,
                    &OsmoCellGlobalIdPs {
                        rai: c_rai(&c.rai),
                        cell_identity: c.ci,
                    },
                );
            }
            Id::Gummei(g) => {
                osmo_gummei_name_buf(&mut got, &c_gummei(&g));
            }
            other => panic!("no short-buffer record for {other:?}"),
        }
        assert_eq!(got, buf, "{id:?}");
    }
}

#[test]
fn comparisons_match_c() {
    let mut preferred_checked = 0;
    for body in bodies() {
        let Body::Cmp { a, b, result } = body else {
            continue;
        };
        assert_eq!(compat_cmp(&a, &b), result, "{a:?} {b:?}");
        if let Some(sign) = preferred_cmp(&a, &b) {
            assert_eq!(sign, result, "{a:?} {b:?}");
            preferred_checked += 1;
        }
    }
    assert!(preferred_checked > 0);
}

#[test]
fn domain_names_match_c() {
    for body in bodies() {
        let Body::Gen { input, rc, out } = body else {
            continue;
        };
        let mut home = [GUARD; GSM23003_HOME_NETWORK_DOMAIN_LEN + 1];
        let mut mme_out = [GUARD; GSM23003_MME_DOMAIN_LEN];
        let (got_rc, written): (i32, &[u8]) = match &input {
            GenInput::Home(p) => (osmo_gen_home_network_domain(&mut home, &c_plmn(p)), &home),
            GenInput::Mme(g) => (osmo_gen_mme_domain(&mut mme_out, &c_gummei(g)), &mme_out),
            GenInput::MmeGroup { group_id, plmn } => (
                osmo_gen_mme_group_domain(&mut mme_out, *group_id, &c_plmn(plmn)),
                &mme_out,
            ),
        };
        assert_eq!(got_rc, rc, "{input:?}");
        match &out {
            Some(text) => {
                assert_eq!(&written[..text.len()], text.as_slice());
                assert_eq!(written[text.len()], 0);
                assert!(written[text.len() + 1..].iter().all(|&byte| byte == GUARD));
            }
            None => assert!(written.iter().all(|&byte| byte == GUARD)),
        }

        let preferred = match &input {
            GenInput::Home(p) => plmn(p).map(|p| p.home_network_domain()),
            GenInput::Mme(g) => gummei(g).map(|g| g.mme_domain()),
            GenInput::MmeGroup { group_id, plmn: p } => {
                plmn(p).map(|p| p.mme_group_domain(*group_id))
            }
        };
        assert_eq!(preferred.map(String::into_bytes), out, "{input:?}");
    }
}

#[test]
fn domain_parsing_matches_c() {
    let mut accepted_by_both = 0;
    for body in bodies() {
        let Body::Parse { input, rc, out } = body else {
            continue;
        };
        match out {
            Parsed::Home(expected) => {
                let mut got = OsmoPlmnId {
                    mcc: 1,
                    mnc: 2,
                    mnc_3_digits: true,
                };
                assert_eq!(osmo_parse_home_network_domain(&mut got, &input), rc);
                assert_eq!(got, c_plmn(&expected), "{input:?}");
            }
            Parsed::Mme(expected) => {
                let mut got = OsmoGummei::default();
                got.mme.code = 7;
                assert_eq!(osmo_parse_mme_domain(&mut got, &input), rc);
                assert_eq!(got, c_gummei(&expected), "{input:?}");
            }
        }

        // The preferred parsers take a domain only in the form the generators write, with
        // hex digits of either case, and then agree with C.
        let text = std::str::from_utf8(&input).unwrap();
        let (preferred, canonical) = match out {
            Parsed::Home(expected) => (
                Plmn::from_home_network_domain(text).map(|p| p.to_string()),
                (rc == 0 && in_range(&expected))
                    .then(|| plmn(&expected).unwrap().home_network_domain()),
            ),
            Parsed::Mme(expected) => (
                Gummei::from_mme_domain(text).map(|g| g.to_string()),
                (rc == 0 && in_range(&expected.plmn))
                    .then(|| gummei(&expected).unwrap().mme_domain()),
            ),
        };
        if canonical.as_deref() == Some(text.to_ascii_lowercase().as_str()) {
            let name = match out {
                Parsed::Home(p) => osmo_plmn_name(&c_plmn(&p)),
                Parsed::Mme(g) => osmo_gummei_name(&c_gummei(&g)),
            };
            assert_eq!(preferred, Ok(name), "{text:?}");
            accepted_by_both += 1;
        } else {
            assert_eq!(preferred, Err(IdentityError::Domain), "{text:?}");
        }
    }
    assert!(accepted_by_both > 0);
}

#[test]
fn upstream_records_are_present() {
    let upstream = records().iter().filter(|record| record.upstream).count();
    assert_eq!(upstream, fixture::UPSTREAM_COUNT);
}
