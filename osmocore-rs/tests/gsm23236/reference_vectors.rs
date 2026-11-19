use crate::gsm23236_fixture::{self as fixture, Body, GUARD, Outcome, Ranges, Record, SENTINEL};
use osmocore_rs::compat::gsm23236::*;
use osmocore_rs::gsm::gsm23236::{
    NriBitlen, NriError, NriRange, NriRanges, parse_nri, set_tmsi_nri, tmsi_nri,
};

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm23236/gsm23236_vectors.ndjson");

fn records() -> Vec<Record> {
    fixture::validate(FIXTURE)
}

fn bodies() -> Vec<Body> {
    records().into_iter().map(|record| record.body).collect()
}

/// The recorded list, built as C built it.
fn c_list(ranges: &Ranges) -> OsmoNriRanges {
    let mut list = OsmoNriRanges::new();
    for &(first, last) in ranges {
        assert_eq!(
            osmo_nri_ranges_add(&mut list, &OsmoNriRange { first, last }),
            0
        );
    }
    assert_eq!(&entries(&list), ranges);
    list
}

fn entries(list: &OsmoNriRanges) -> Ranges {
    list.entries().iter().map(|r| (r.first, r.last)).collect()
}

fn preferred(ranges: &Ranges) -> NriRanges {
    ranges
        .iter()
        .map(|&(first, last)| range(first, last).expect("C keeps valid ranges"))
        .collect()
}

/// The preferred range of C's values, where they are valid.
fn range(first: i16, last: i16) -> Option<NriRange> {
    NriRange::new(u16::try_from(first).ok()?, u16::try_from(last).ok()?).ok()
}

fn nri(value: i16) -> Option<u16> {
    u16::try_from(value).ok()
}

#[test]
fn fixture_is_well_formed() {
    let total: usize = fixture::SECTIONS.iter().map(|(_, count)| count).sum();
    assert_eq!(records().len(), total);
    fixture::check_corruptions(FIXTURE);
}

#[test]
fn tmsi_nri_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        match body {
            Body::TmsiGet {
                tmsi,
                nri_bitlen,
                rc,
                nri_v,
            } => {
                let mut out = SENTINEL;
                assert_eq!(osmo_tmsi_nri_v_get(&mut out, tmsi, nri_bitlen), rc);
                assert_eq!(out, nri_v);
                match NriBitlen::new(nri_bitlen) {
                    Some(bitlen) => assert_eq!(Some(tmsi_nri(tmsi, bitlen)), nri(nri_v)),
                    None => assert_eq!(rc, -1),
                }
            }
            Body::TmsiSet {
                tmsi,
                nri_v,
                nri_bitlen,
                rc,
                out,
            } => {
                let mut changed = tmsi;
                assert_eq!(osmo_tmsi_nri_v_set(&mut changed, nri_v, nri_bitlen), rc);
                assert_eq!(changed, out);
                match (NriBitlen::new(nri_bitlen), nri(nri_v)) {
                    (Some(bitlen), Some(value)) if bitlen.fits(value) => {
                        assert_eq!(set_tmsi_nri(tmsi, value, bitlen), Ok(out));
                    }
                    // C keeps the bits that fit.
                    (Some(bitlen), Some(value)) => {
                        assert_eq!(set_tmsi_nri(tmsi, value, bitlen), Err(NriError::TooLarge));
                        assert_eq!(
                            set_tmsi_nri(tmsi, value & bitlen.max_nri(), bitlen),
                            Ok(out)
                        );
                    }
                    _ => assert_eq!(rc, -1),
                }
            }
            _ => continue,
        }
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[0].1 + fixture::SECTIONS[1].1);
}

#[test]
fn validation_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        match body {
            Body::Validate {
                nri_v,
                nri_bitlen,
                rc,
            } => {
                assert_eq!(osmo_nri_v_validate(nri_v, nri_bitlen), rc);
                if let (Some(bitlen), Some(value)) = (NriBitlen::new(nri_bitlen), nri(nri_v)) {
                    assert_eq!(bitlen.fits(value), rc == 0);
                }
            }
            Body::RangeValidate {
                first,
                last,
                nri_bitlen,
                rc,
            } => {
                assert_eq!(
                    osmo_nri_range_validate(&OsmoNriRange { first, last }, nri_bitlen),
                    rc
                );
                let preferred = range(first, last);
                match NriBitlen::new(nri_bitlen) {
                    Some(bitlen) if nri_bitlen < 15 => {
                        assert_eq!(preferred.is_some_and(|r| r.fits(bitlen)), rc == 0);
                    }
                    // From 15 bits on, C checks only the signs and the order.
                    _ if nri_bitlen >= 15 => assert_eq!(preferred.is_some(), rc == 0),
                    _ => assert_ne!(rc, 0),
                }
            }
            _ => continue,
        }
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[2].1 + fixture::SECTIONS[3].1);
}

#[test]
fn list_changes_match_c() {
    let mut checked = 0;
    for body in bodies() {
        let (add, ranges, first, last, rc, out) = match body {
            Body::Add {
                ranges,
                first,
                last,
                rc,
                out,
            } => (true, ranges, first, last, rc, out),
            Body::Del {
                ranges,
                first,
                last,
                rc,
                out,
            } => (false, ranges, first, last, rc, out),
            _ => continue,
        };
        let mut list = c_list(&ranges);
        let change = OsmoNriRange { first, last };
        let result = if add {
            osmo_nri_ranges_add(&mut list, &change)
        } else {
            osmo_nri_ranges_del(&mut list, &change)
        };
        assert_eq!(result, rc, "{ranges:?} {first}..{last}");
        assert_eq!(entries(&list), out, "{ranges:?} {first}..{last}");

        let mut set = preferred(&ranges);
        match range(first, last) {
            Some(change) if add => set.add(change),
            Some(change) => set.remove(change),
            None => assert_eq!(rc, -1),
        }
        assert_eq!(set, preferred(&out));
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[4].1 + fixture::SECTIONS[5].1);
}

#[test]
fn queries_match_c() {
    let mut checked = 0;
    for body in bodies() {
        match body {
            Body::Matches {
                ranges,
                nri_v,
                matches,
            } => {
                assert_eq!(osmo_nri_v_matches_ranges(nri_v, &c_list(&ranges)), matches);
                if let Some(value) = nri(nri_v) {
                    assert_eq!(preferred(&ranges).contains(value), matches);
                }
            }
            Body::Overlaps {
                ranges,
                first,
                last,
                overlaps,
            } => {
                let query = OsmoNriRange { first, last };
                assert_eq!(
                    osmo_nri_range_overlaps_ranges(&query, &c_list(&ranges)),
                    overlaps
                );
                if let Some(query) = range(first, last) {
                    assert_eq!(preferred(&ranges).overlaps(query), overlaps);
                }
            }
            _ => continue,
        }
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[6].1 + fixture::SECTIONS[7].1);
}

#[test]
fn limits_match_c() {
    let mut checked = 0;
    for record in records() {
        match record.body {
            Body::Limit {
                ranges,
                nri_v,
                nri_bitlen,
                outcome,
            } => {
                let mut value = nri_v;
                let rc = osmo_nri_v_limit_by_ranges(&mut value, &c_list(&ranges), nri_bitlen);
                // Where C divides by zero, compat fails and keeps the value.
                let expected = match outcome {
                    Outcome::Signal(_) => Outcome::Returned { rc: -1, out: nri_v },
                    returned => returned,
                };
                assert_eq!(Outcome::Returned { rc, out: value }, expected);
                let bitlen = u8::try_from(nri_bitlen).ok().and_then(NriBitlen::new);
                if let (Some(bitlen), Some(index)) = (bitlen, nri(nri_v)) {
                    let limited = preferred(&ranges).limit(index, bitlen);
                    match outcome {
                        Outcome::Signal(_) => assert_eq!(limited, None),
                        Outcome::Returned { rc, out } => {
                            assert_eq!(rc, 0);
                            assert_eq!(limited, nri(out));
                        }
                    }
                }
            }
            Body::TmsiLimit {
                ranges,
                tmsi,
                nri_bitlen,
                outcome,
            } => {
                let mut value = tmsi;
                let rc = osmo_tmsi_nri_v_limit_by_ranges(&mut value, &c_list(&ranges), nri_bitlen);
                let expected = match outcome {
                    Outcome::Signal(_) => Outcome::Returned { rc: -1, out: tmsi },
                    returned => returned,
                };
                assert_eq!(Outcome::Returned { rc, out: value }, expected);
                if let Some(bitlen) = NriBitlen::new(nri_bitlen) {
                    let limited = preferred(&ranges).limit_tmsi(tmsi, bitlen);
                    match outcome {
                        Outcome::Signal(_) => assert_eq!(limited, None),
                        Outcome::Returned { out, .. } => assert_eq!(limited, Some(out)),
                    }
                }
            }
            _ => continue,
        }
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[8].1 + fixture::SECTIONS[9].1);
}

#[test]
fn text_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        match body {
            Body::ToStr {
                ranges,
                buflen,
                rc,
                out,
            } => {
                let mut buf = vec![GUARD; buflen + 1];
                assert_eq!(
                    osmo_nri_ranges_to_str_buf(&mut buf[..buflen], &c_list(&ranges)),
                    rc
                );
                assert_eq!(buf, out);
            }
            Body::ToStrC { ranges, out } => {
                let text = osmo_nri_ranges_to_str_c(&c_list(&ranges));
                assert_eq!(text.as_bytes(), out);
                // The preferred text has the commas C's leaves out.
                let preferred = preferred(&ranges).to_string();
                assert_eq!(preferred.replace(',', ""), text);
                assert_eq!(
                    preferred.matches(',').count(),
                    ranges.len().saturating_sub(1)
                );
            }
            _ => continue,
        }
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[10].1 + fixture::SECTIONS[11].1);
}

/// Text C's `strtoll` parser takes that the preferred one doesn't: leading whitespace and a
/// sign.
fn only_c_takes(text: &[u8]) -> bool {
    text.first()
        .is_some_and(|byte| b" \t\n\x0b\x0c\r+-".contains(byte))
}

#[test]
fn vty_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        let (bitlen, ranges, argv, rc, message, parsed, out) = match body {
            Body::VtyAdd {
                ranges,
                argv,
                nri_bitlen,
                rc,
                message,
                parsed,
                out,
            } => (Some(nri_bitlen), ranges, argv, rc, message, parsed, out),
            Body::VtyDel {
                ranges,
                argv,
                rc,
                message,
                parsed,
                out,
            } => (None, ranges, argv, rc, message, parsed, out),
            _ => continue,
        };
        let args: Vec<Option<&[u8]>> = argv.iter().map(Option::as_deref).collect();
        let mut list = c_list(&ranges);
        let mut got_message = Some("untouched");
        let mut got_parsed = OsmoNriRange {
            first: SENTINEL,
            last: SENTINEL,
        };
        let got_rc = match bitlen {
            Some(bitlen) => osmo_nri_ranges_vty_add(
                &mut got_message,
                Some(&mut got_parsed),
                &mut list,
                &args,
                bitlen,
            ),
            None => {
                osmo_nri_ranges_vty_del(&mut got_message, Some(&mut got_parsed), &mut list, &args)
            }
        };
        assert_eq!(got_rc, rc, "{argv:?}");
        assert_eq!(got_message, message.as_deref(), "{argv:?}");
        assert_eq!((got_parsed.first, got_parsed.last), parsed, "{argv:?}");
        assert_eq!(entries(&list), out, "{argv:?}");

        // The preferred parser takes what C parses, except whitespace and signs.
        let texts: Vec<_> = args
            .iter()
            .flatten()
            .map(|arg| std::str::from_utf8(arg))
            .collect();
        let c_parsed = parsed != (SENTINEL, SENTINEL);
        let preferred = match texts.as_slice() {
            [Ok(first)] => Some(NriRange::parse(first, None)),
            [Ok(first), Ok(last)] => Some(NriRange::parse(first, Some(last))),
            _ => None,
        };
        match preferred {
            Some(Ok(r)) => assert_eq!((r.first() as i16, r.last() as i16), parsed),
            Some(Err(NriError::Reversed)) => assert!(c_parsed && parsed.0 > parsed.1),
            _ => assert!(
                !c_parsed || args.iter().flatten().any(|arg| only_c_takes(arg)),
                "{argv:?}"
            ),
        }
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[12].1 + fixture::SECTIONS[13].1);
}

#[test]
fn every_parsed_vty_value_parses_alone() {
    for body in bodies() {
        let (Body::VtyAdd { argv, parsed, .. } | Body::VtyDel { argv, parsed, .. }) = body else {
            continue;
        };
        if let [Some(arg)] = argv.as_slice()
            && let Ok(text) = std::str::from_utf8(arg)
            && let Ok(value) = parse_nri(text)
        {
            assert_eq!((value as i16, value as i16), parsed);
        }
    }
}
