use crate::gsm48_fixture::{
    self as fixture, Body, DecodedMi, GUARD, MiInput, OUT_RECORDED, OUT_SIZE, Record,
};
use osmocore_rs::compat::gsm48::*;
use osmocore_rs::compat::gsm23003::{
    OsmoLocationAreaId, OsmoPlmnId, OsmoRoutingAreaId, osmo_plmn_from_bcd, osmo_plmn_to_bcd,
};
use osmocore_rs::gsm::gsm48::{MobileIdentity, MobileIdentityError};
use osmocore_rs::gsm::gsm23003::{Lai, Mcc, Mnc, Plmn, Rai};

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm48/gsm48_vectors.ndjson");
const JUNK_U16: u16 = fixture::JUNK_U16;

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

fn c_gprs(r: &fixture::Rai) -> GprsRaId {
    GprsRaId {
        mcc: r.lai.plmn.mcc,
        mnc: r.lai.plmn.mnc,
        mnc_3_digits: r.lai.plmn.mnc_3_digits,
        lac: r.lai.lac,
        rac: r.rac,
    }
}

fn fixture_plmn(p: &OsmoPlmnId) -> fixture::Plmn {
    fixture::Plmn {
        mcc: p.mcc,
        mnc: p.mnc,
        mnc_3_digits: p.mnc_3_digits,
    }
}

fn fixture_rai(r: &OsmoRoutingAreaId) -> fixture::Rai {
    fixture::Rai {
        lai: fixture::Lai {
            plmn: fixture_plmn(&r.lac.plmn),
            lac: r.lac.lac,
        },
        rac: r.rac,
    }
}

fn fixture_gprs(r: &GprsRaId) -> fixture::Rai {
    fixture::Rai {
        lai: fixture::Lai {
            plmn: fixture::Plmn {
                mcc: r.mcc,
                mnc: r.mnc,
                mnc_3_digits: r.mnc_3_digits,
            },
            lac: r.lac,
        },
        rac: r.rac,
    }
}

fn c_mi(input: &Option<MiInput>) -> Option<OsmoMobileIdentity> {
    let input = input.as_ref()?;
    Some(match input.tmsi {
        Some(tmsi) => {
            let mut mi = OsmoMobileIdentity::with_tmsi(tmsi);
            mi.mi_type = input.mi_type;
            mi
        }
        None => OsmoMobileIdentity::with_digits(
            input.mi_type,
            input.digits.as_deref().expect("digits").as_bytes(),
        ),
    })
}

fn decoded(mi: &OsmoMobileIdentity) -> DecodedMi {
    DecodedMi {
        mi_type: mi.mi_type,
        tmsi: mi.tmsi(),
        digits: matches!(mi.mi_type, 1..=3)
            .then(|| String::from_utf8(mi.digits().to_vec()).expect("ASCII digits")),
    }
}

/// The identity C recorded, as the preferred type: `None` where C failed.
fn preferred_mi(rc: i32, mi: &DecodedMi) -> Option<MobileIdentity> {
    if rc != 0 {
        return None;
    }
    Some(match (mi.mi_type, mi.digits.as_deref()) {
        (1, Some(digits)) => MobileIdentity::Imsi(digits.parse().ok()?),
        (2, Some(digits)) => MobileIdentity::Imei(digits.parse().ok()?),
        (3, Some(digits)) => MobileIdentity::Imeisv(digits.parse().ok()?),
        (4, None) => MobileIdentity::Tmsi(mi.tmsi),
        _ => panic!("C decoded {mi:?}"),
    })
}

fn preferred_error(rc: i32) -> MobileIdentityError {
    match rc {
        -22 => MobileIdentityError::UnknownType,
        -74 => MobileIdentityError::Malformed,
        -95 => MobileIdentityError::NotCarried,
        _ => panic!("no preferred error for {rc}"),
    }
}

fn plmn(p: &fixture::Plmn) -> Option<Plmn> {
    Some(Plmn::new(
        Mcc::new(p.mcc)?,
        Mnc::new(p.mnc, p.mnc_3_digits)?,
    ))
}

fn guarded<const N: usize>() -> [u8; N] {
    [GUARD; N]
}

#[test]
fn fixture_is_well_formed() {
    fixture::check_corruptions(FIXTURE);
}

#[test]
fn constants_match_c() {
    let mut count = 0;
    for body in bodies() {
        let Body::Constant { name, value } = body else {
            continue;
        };
        let actual = match name.as_str() {
            "GSM_MI_TYPE_MASK" => i64::from(GSM_MI_TYPE_MASK),
            "GSM_MI_TYPE_NONE" => i64::from(GSM_MI_TYPE_NONE),
            "GSM_MI_TYPE_IMSI" => i64::from(GSM_MI_TYPE_IMSI),
            "GSM_MI_TYPE_IMEI" => i64::from(GSM_MI_TYPE_IMEI),
            "GSM_MI_TYPE_IMEISV" => i64::from(GSM_MI_TYPE_IMEISV),
            "GSM_MI_TYPE_TMSI" => i64::from(GSM_MI_TYPE_TMSI),
            "GSM_MI_ODD" => i64::from(GSM_MI_ODD),
            "GSM48_IE_MOBILE_ID" => i64::from(GSM48_IE_MOBILE_ID),
            "GSM48_TMSI_LEN" => GSM48_TMSI_LEN as i64,
            "GSM48_MID_TMSI_LEN" => GSM48_MID_TMSI_LEN as i64,
            "GSM48_MID_MAX_SIZE" => GSM48_MID_MAX_SIZE as i64,
            "GSM48_MI_SIZE" => GSM48_MI_SIZE as i64,
            "GSM_RESERVED_TMSI" => i64::from(GSM_RESERVED_TMSI),
            "GSM23003_TMSI_NUM_BYTES" => GSM23003_TMSI_NUM_BYTES as i64,
            "GSM23003_IMEI_NUM_DIGITS_NO_CHK" => {
                osmocore_rs::compat::gsm23003::GSM23003_IMEI_NUM_DIGITS_NO_CHK as i64
            }
            "GSM23003_IMEISV_NUM_DIGITS" => GSM23003_IMEISV_NUM_DIGITS as i64,
            "sizeof_gsm48_loc_area_id" => GSM48_LOC_AREA_ID_LEN as i64,
            "sizeof_gsm48_ra_id" => GSM48_RA_ID_LEN as i64,
            other => panic!("unknown constant {other}"),
        };
        assert_eq!(actual, value, "{name}");
        count += 1;
    }
    assert_eq!(count, 18);
}

#[test]
fn plmn_bcd_matches_c() {
    for body in bodies() {
        match body {
            Body::ToBcd {
                plmn: p,
                bcd,
                legacy_bcd,
            } => {
                let mut out = [GUARD; 3];
                osmo_plmn_to_bcd(&mut out, &c_plmn(&p));
                assert_eq!(out, bcd, "{p:?}");
                let mut legacy = [GUARD; 3];
                gsm48_mcc_mnc_to_bcd(&mut legacy, p.mcc, p.mnc);
                assert_eq!(legacy, legacy_bcd, "{p:?}");
                if let Some(preferred) = plmn(&p) {
                    assert_eq!(preferred.to_bcd(), bcd, "{p:?}");
                }
            }
            Body::FromBcd {
                bcd,
                plmn: p,
                legacy_mcc,
                legacy_mnc,
                parse_ra,
            } => {
                let mut out = OsmoPlmnId {
                    mcc: JUNK_U16,
                    mnc: JUNK_U16,
                    mnc_3_digits: true,
                };
                osmo_plmn_from_bcd(&bcd, &mut out);
                assert_eq!(fixture_plmn(&out), p, "{bcd:02x?}");
                let (mut mcc, mut mnc) = (JUNK_U16, JUNK_U16);
                gsm48_mcc_mnc_from_bcd(&bcd, &mut mcc, &mut mnc);
                assert_eq!((mcc, mnc), (legacy_mcc, legacy_mnc));
                let mut raid = GprsRaId::default();
                gsm48_parse_ra(&mut raid, &[bcd[0], bcd[1], bcd[2], 0, 0, 0]);
                assert_eq!(fixture_gprs(&raid).lai.plmn, parse_ra);
                // The preferred decoder takes exactly the decimal digits, and gives what C
                // gives for them.
                let decimal = (0..6).all(|i| {
                    let nibble = bcd[i / 2] >> (i % 2 * 4) & 0x0f;
                    nibble <= 9 || (i == 3 && nibble == 0x0f)
                });
                match Plmn::from_bcd(&bcd) {
                    Ok(preferred) => {
                        assert!(decimal, "{bcd:02x?}");
                        assert_eq!(Some(preferred), plmn(&p));
                        assert_eq!(preferred.to_bcd(), bcd);
                    }
                    Err(_) => assert!(!decimal, "{bcd:02x?}"),
                }
            }
            _ => {}
        }
    }
}

#[test]
fn lai_encoding_matches_c() {
    for body in bodies() {
        match body {
            Body::LaiEncode {
                lai: l,
                bytes,
                legacy_bytes,
            } => {
                let mut out = guarded();
                gsm48_generate_lai2(&mut out, &c_lai(&l));
                assert_eq!(out, bytes);
                let mut legacy = guarded();
                gsm48_generate_lai(&mut legacy, l.plmn.mcc, l.plmn.mnc, l.lac);
                assert_eq!(legacy, legacy_bytes);
                if let Some(p) = plmn(&l.plmn) {
                    assert_eq!(
                        Lai {
                            plmn: p,
                            lac: l.lac
                        }
                        .to_bytes(),
                        bytes
                    );
                }
            }
            Body::LaiDecode {
                bytes,
                lai: l,
                legacy_rc,
                legacy_mcc,
                legacy_mnc,
                legacy_lac,
            } => {
                let mut decoded = OsmoLocationAreaId {
                    plmn: OsmoPlmnId {
                        mcc: JUNK_U16,
                        mnc: JUNK_U16,
                        mnc_3_digits: true,
                    },
                    lac: JUNK_U16,
                };
                gsm48_decode_lai2(&bytes, &mut decoded);
                assert_eq!(decoded, c_lai(&l));
                let (mut mcc, mut mnc, mut lac) = (JUNK_U16, JUNK_U16, JUNK_U16);
                let rc = gsm48_decode_lai(&bytes, &mut mcc, &mut mnc, &mut lac);
                assert_eq!(
                    (rc, mcc, mnc, lac),
                    (legacy_rc, legacy_mcc, legacy_mnc, legacy_lac)
                );
                match Lai::from_bytes(&bytes) {
                    Ok(preferred) => {
                        assert_eq!(preferred.plmn, plmn(&l.plmn).unwrap());
                        assert_eq!(preferred.lac, l.lac);
                        assert_eq!(preferred.to_bytes(), bytes);
                    }
                    Err(_) => assert!(Plmn::from_bcd(&[bytes[0], bytes[1], bytes[2]]).is_err()),
                }
            }
            _ => {}
        }
    }
}

#[test]
fn routing_area_encoding_matches_c() {
    for body in bodies() {
        match body {
            Body::RaEncode {
                rai: r,
                buflen,
                rc,
                out,
                legacy_bytes,
                construct_rc,
            } => {
                let mut buf = [GUARD; OUT_SIZE];
                assert_eq!(
                    osmo_routing_area_id_encode_buf(&mut buf[..buflen], &c_rai(&r)),
                    rc
                );
                assert_eq!(buf[..OUT_RECORDED], out[..]);
                let mut legacy = guarded();
                gsm48_encode_ra(&mut legacy, &c_gprs(&r));
                assert_eq!(legacy, legacy_bytes);
                let mut constructed = guarded();
                assert_eq!(
                    gsm48_construct_ra(&mut constructed, &c_gprs(&r)),
                    construct_rc
                );
                assert_eq!(constructed, legacy_bytes);
                let mut converted = GprsRaId::default();
                osmo_rai_to_gprs(&mut converted, &c_rai(&r));
                assert_eq!(converted, c_gprs(&r));
                let mut back = OsmoRoutingAreaId::default();
                gprs_rai_to_osmo(&mut back, &converted);
                assert_eq!(back, c_rai(&r));
                if let (Some(p), 6) = (plmn(&r.lai.plmn), rc) {
                    let preferred = Rai {
                        lai: Lai {
                            plmn: p,
                            lac: r.lai.lac,
                        },
                        rac: r.rac,
                    };
                    assert_eq!(preferred.to_bytes()[..], out[..6]);
                }
            }
            Body::RaDecode {
                bytes,
                rc,
                rai: r,
                parse_ra,
            } => {
                let mut decoded = OsmoRoutingAreaId {
                    lac: OsmoLocationAreaId {
                        plmn: OsmoPlmnId {
                            mcc: JUNK_U16,
                            mnc: JUNK_U16,
                            mnc_3_digits: true,
                        },
                        lac: JUNK_U16,
                    },
                    rac: 0xa5,
                };
                assert_eq!(osmo_routing_area_id_decode(&mut decoded, &bytes), rc);
                assert_eq!(fixture_rai(&decoded), r);
                if let Some(parse_ra) = parse_ra {
                    let mut parsed = GprsRaId::default();
                    gsm48_parse_ra(&mut parsed, bytes[..6].try_into().unwrap());
                    assert_eq!(fixture_gprs(&parsed), parse_ra);
                }
                if let Ok(six) = <&[u8; 6]>::try_from(bytes.as_slice())
                    && let Ok(preferred) = Rai::from_bytes(six)
                {
                    assert_eq!(preferred.lai.plmn, plmn(&r.lai.plmn).unwrap());
                    assert_eq!((preferred.lai.lac, preferred.rac), (r.lai.lac, r.rac));
                }
            }
            Body::RaEqual { a, b, result } => {
                assert_eq!(gsm48_ra_equal(&c_gprs(&a), &c_gprs(&b)), result);
            }
            Body::RaiName { rai: r, text } => {
                assert_eq!(osmo_rai_name(&c_gprs(&r)), text);
                if let Some(p) = plmn(&r.lai.plmn) {
                    let preferred = Rai {
                        lai: Lai {
                            plmn: p,
                            lac: r.lai.lac,
                        },
                        rac: r.rac,
                    };
                    // The preferred name prints a three-digit MNC from 100 on whatever the
                    // flag, as C's name does.
                    assert_eq!(preferred.to_string(), text);
                }
            }
            Body::RaiNameBuf {
                rai: r,
                buf_len,
                out,
            } => {
                let mut buf = [GUARD; OUT_SIZE];
                osmo_rai_name_buf(&mut buf[..buf_len], &c_gprs(&r));
                assert_eq!(buf[..=buf_len], out[..]);
            }
            _ => {}
        }
    }
}

#[test]
fn legacy_identity_text_matches_c() {
    for body in bodies() {
        match body {
            Body::MiTypeName { mi_type, text } => assert_eq!(gsm48_mi_type_name(mi_type), text),
            Body::MiToString {
                bytes,
                str_len,
                rc,
                out,
            } => {
                let mut buf = [GUARD; OUT_SIZE];
                assert_eq!(gsm48_mi_to_string(&mut buf[..str_len], Some(&bytes)), rc);
                assert_eq!(buf[..=str_len], out[..], "{bytes:02x?} {str_len}");
            }
            Body::MiNameBuf {
                bytes,
                buf_len,
                out,
            } => {
                let mut buf = [GUARD; OUT_SIZE];
                osmo_mi_name_buf(&mut buf[..buf_len], Some(&bytes));
                assert_eq!(buf[..=buf_len], out[..]);
            }
            Body::GenerateMid {
                id,
                mi_type,
                rc,
                out,
            } => {
                let mut buf = guarded();
                assert_eq!(gsm48_generate_mid(&mut buf, &id, mi_type), rc);
                assert_eq!(buf[..usize::from(rc)], out[..], "{id:02x?} {mi_type}");
                assert!(buf[usize::from(rc)..].iter().all(|&byte| byte == GUARD));
                if mi_type == GSM_MI_TYPE_IMSI {
                    let mut imsi = guarded();
                    assert_eq!(gsm48_generate_mid_from_imsi(&mut imsi, &id), i32::from(rc));
                    assert_eq!(imsi, buf);
                }
            }
            Body::GenerateMidFromTmsi { tmsi, rc, out } => {
                let mut buf = guarded();
                assert_eq!(gsm48_generate_mid_from_tmsi(&mut buf, tmsi), rc);
                assert_eq!(buf[..], out[..]);
            }
            _ => {}
        }
    }
}

#[test]
fn mobile_identity_decoding_matches_c() {
    let mut decodable = 0;
    for body in bodies() {
        let Body::MiBytes {
            bytes,
            name,
            to_string_rc,
            to_string,
            rc,
            decoded: c_decoded,
            hex_rc,
            hex_decoded,
            ..
        } = body
        else {
            continue;
        };
        let input = bytes.as_deref();
        assert_eq!(osmo_mi_name(input), name);
        let mut buf = [GUARD; OUT_SIZE];
        assert_eq!(gsm48_mi_to_string(&mut buf[..64], input), to_string_rc);
        assert_eq!(&buf[..to_string.len()], &to_string[..]);
        assert_eq!(buf[to_string.len()], 0);
        for (allow_hex, rc, c_decoded) in [(false, rc, &c_decoded), (true, hex_rc, &hex_decoded)] {
            let mut mi = OsmoMobileIdentity {
                mi_type: 0xff,
                union: [0xff; 17],
            };
            assert_eq!(osmo_mobile_identity_decode(&mut mi, input, allow_hex), rc);
            assert_eq!(&decoded(&mi), c_decoded, "{bytes:02x?} {allow_hex}");
        }
        // The preferred decoder takes what C takes without hex digits, and round-trips it.
        let preferred = MobileIdentity::decode(input.unwrap_or_default());
        match preferred_mi(rc, &c_decoded) {
            Some(expected) => {
                assert_eq!(preferred, Ok(expected));
                assert_eq!(expected.encode(), bytes.unwrap());
                decodable += 1;
            }
            None => assert_eq!(preferred, Err(preferred_error(rc)), "{bytes:02x?}"),
        }
    }
    assert!(decodable > 0);
}

#[test]
fn mobile_identity_encoding_and_text_match_c() {
    for body in bodies() {
        match body {
            Body::MiEncode {
                mi,
                null_buf,
                buflen,
                allow_hex,
                len_rc,
                len_digits,
                rc,
                out,
            } => {
                let identity = c_mi(&mi);
                let mut digits = fixture::JUNK_INT;
                assert_eq!(
                    osmo_mobile_identity_encoded_len(identity.as_ref(), Some(&mut digits)),
                    len_rc
                );
                assert_eq!(digits, len_digits);
                let mut buf = [GUARD; OUT_SIZE];
                let target = (!null_buf).then_some(&mut buf[..buflen]);
                assert_eq!(
                    osmo_mobile_identity_encode_buf(target, identity.as_ref(), allow_hex),
                    rc,
                    "{mi:?} {buflen}"
                );
                assert_eq!(buf[..OUT_RECORDED], out[..], "{mi:?} {buflen}");
                // A preferred identity encodes as C does into room enough.
                if let Some(mi) = &mi
                    && rc > 0
                    && let Some(preferred) = preferred_from_input(mi)
                {
                    assert_eq!(preferred.encoded_len() as i32, rc);
                    assert_eq!(preferred.encode()[..], out[..rc as usize]);
                }
            }
            Body::MiStr {
                mi,
                buflen,
                rc,
                out,
            } => {
                let identity = c_mi(&mi);
                let mut buf = [GUARD; OUT_SIZE];
                assert_eq!(
                    osmo_mobile_identity_to_str_buf(&mut buf[..buflen], identity.as_ref()),
                    rc
                );
                if buflen == OUT_SIZE {
                    assert_eq!(buf[..out.len()], out[..]);
                    assert_eq!(
                        osmo_mobile_identity_to_str_c(identity.as_ref()).as_bytes(),
                        &out[..]
                    );
                    if let Some(preferred) = mi.as_ref().and_then(preferred_from_input) {
                        assert_eq!(preferred.to_string().as_bytes(), &out[..]);
                    }
                } else {
                    assert_eq!(buf[..=buflen], out[..]);
                }
            }
            Body::MiCmp { a, b, rc } => {
                let (ca, cb) = (c_mi(&a), c_mi(&b));
                assert_eq!(
                    osmo_mobile_identity_cmp(ca.as_ref(), cb.as_ref()),
                    rc,
                    "{a:?} {b:?}"
                );
                if let (Some(pa), Some(pb)) = (
                    a.as_ref().and_then(preferred_from_input),
                    b.as_ref().and_then(preferred_from_input),
                ) {
                    assert_eq!(pa.cmp(&pb) as i32, rc.signum(), "{a:?} {b:?}");
                }
            }
            _ => {}
        }
    }
}

/// The preferred identity of an input C encodes, where there is one.
fn preferred_from_input(mi: &MiInput) -> Option<MobileIdentity> {
    match (mi.mi_type, mi.tmsi, mi.digits.as_deref()) {
        (4, Some(tmsi), _) => Some(MobileIdentity::Tmsi(tmsi)),
        (1, _, Some(digits)) => digits.parse().ok().map(MobileIdentity::Imsi),
        (2, _, Some(digits)) => digits.parse().ok().map(MobileIdentity::Imei),
        (3, _, Some(digits)) => digits.parse().ok().map(MobileIdentity::Imeisv),
        _ => None,
    }
}

#[test]
fn complete_layer_3_decoding_matches_c() {
    for body in bodies() {
        let Body::L3 {
            l3,
            allow_hex,
            rc,
            decoded: c_decoded,
        } = body
        else {
            continue;
        };
        let mut mi = OsmoMobileIdentity {
            mi_type: 0xff,
            union: [0xff; 17],
        };
        assert_eq!(
            osmo_mobile_identity_decode_from_l3_buf(&mut mi, &l3, allow_hex),
            rc,
            "{l3:02x?}"
        );
        assert_eq!(decoded(&mi), c_decoded, "{l3:02x?}");
        if !allow_hex {
            let preferred = MobileIdentity::from_l3(&l3);
            match preferred_mi(rc, &c_decoded) {
                Some(expected) => assert_eq!(preferred, Ok(expected)),
                None => assert_eq!(preferred, Err(preferred_error(rc)), "{l3:02x?}"),
            }
        }
    }
}
