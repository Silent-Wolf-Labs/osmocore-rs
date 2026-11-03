use crate::tlv_fixture::{
    self, DefEntry, FILL, IS_NULL, MSG_PREFIX, MSG_SIZE, Output, PUSH_HEADROOM, PUT_HEADROOM,
    Record, UNTOUCHED, value,
};
use osmocore_rs::compat::tlv as compat;
use osmocore_rs::msgb::Msgb;
use osmocore_rs::tlv::*;
use std::collections::HashMap;

const FIXTURE: &str = include_str!("../compatibility/vectors/tlv/tlv_vectors.ndjson");

fn records() -> Vec<Record> {
    tlv_fixture::validate(FIXTURE)
}

fn kind(name: &str) -> TlvType {
    match name {
        "none" => TlvType::None,
        "fixed" => TlvType::Fixed,
        "t" => TlvType::T,
        "tv" => TlvType::Tv,
        "tlv" => TlvType::Tlv,
        "tl16v" => TlvType::Tl16v,
        "tvlv" => TlvType::Tvlv,
        "single_tv" => TlvType::SingleTv,
        "vtvlv_gan" => TlvType::VtvlvGan,
        other => panic!("unknown format {other}"),
    }
}

fn definition(entries: &[DefEntry]) -> TlvDefinition {
    let mut definition = TlvDefinition::new();
    for (tag, name, fixed_len) in entries {
        definition.set(
            *tag,
            TlvDef {
                kind: kind(name),
                fixed_len: *fixed_len,
            },
        );
    }
    definition
}

fn definitions() -> HashMap<String, TlvDefinition> {
    records()
        .iter()
        .filter_map(|record| match record {
            Record::Def { name, entries } => Some((name.clone(), definition(entries))),
            _ => None,
        })
        .collect()
}

/// The offset of a slice within the buffer it borrows from.
fn offset(buf: &[u8], value: &[u8]) -> usize {
    value.as_ptr() as usize - buf.as_ptr() as usize
}

#[test]
fn shared_definitions_match_c() {
    let definitions = definitions();
    assert_eq!(definitions.len(), 4);
    assert_eq!(definitions["empty"], TlvDefinition::new());
    assert_eq!(definitions["tvlv_att"], TlvDefinition::all(TlvType::Tvlv));
    assert_eq!(
        definitions["vtvlv_gan_att"],
        TlvDefinition::all(TlvType::VtvlvGan)
    );
    assert_eq!(definitions["mixed"].get(3), TlvDef::fixed(3));
}

#[test]
fn parse_one_matches_c_including_partly_written_outputs() {
    let definitions = definitions();
    let mut checked = 0;
    for record in records() {
        let Record::ParseOne {
            def,
            input,
            rc,
            tag,
            len,
            val,
        } = record
        else {
            continue;
        };
        let definition = &definitions[&def];
        let context = format!("{def} {input:02x?}");

        let sentinel = [0u8; 1];
        let (mut o_tag, mut o_len, mut o_val) = (FILL, 0xa5a5, Some(&sentinel[..]));
        let actual = compat::tlv_parse_one(&mut o_tag, &mut o_len, &mut o_val, definition, &input);
        assert_eq!(actual, rc, "{context}");
        assert_eq!((o_tag, o_len), (tag, len), "{context}");
        match (val, o_val) {
            (UNTOUCHED, Some(value)) => assert_eq!(value.as_ptr(), sentinel.as_ptr(), "{context}"),
            (IS_NULL, None) => {}
            (val, Some(value)) => {
                assert_eq!(offset(&input, value) as i64, val, "{context}");
                assert_eq!(value.len(), usize::from(len), "{context}");
            }
            (val, None) => panic!("{context}: expected {val}, got NULL"),
        }

        match parse_one(definition, &input) {
            Ok((ie, used)) => {
                assert_eq!(used as i32, rc, "{context}");
                assert_eq!(ie.tag, tag, "{context}");
                assert_eq!(offset(&input, ie.value) as i64, val, "{context}");
                assert_eq!(ie.value.len(), usize::from(len), "{context}");
            }
            Err(error) => assert_eq!(compat::tlv_error_code(error), rc, "{context}"),
        }
        checked += 1;
    }
    assert_eq!(checked, 5532);
}

fn entries<'a>(decoded: &[TlvParsed<'a>], buf: &[u8]) -> Vec<(usize, u8, usize, usize)> {
    decoded
        .iter()
        .enumerate()
        .flat_map(|(index, parsed)| {
            parsed
                .iter()
                .map(|ie| (index, ie.tag, offset(buf, ie.value), ie.value.len()))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn parse_matches_c_for_every_occurrence_and_lv_tag() {
    let definitions = definitions();
    let (mut successes, mut failures) = (0, 0);
    for record in records() {
        let Record::Parse {
            def,
            input,
            lv1,
            lv2,
            multiples,
            rc,
            dec,
        } = record
        else {
            continue;
        };
        let definition = &definitions[&def];
        let context = format!("{def} {input:02x?} lv {lv1} {lv2} x{multiples}");
        let mut decoded = vec![TlvParsed::new(); multiples];
        assert_eq!(
            compat::tlv_parse2(&mut decoded, definition, &input, lv1, lv2),
            rc,
            "{context}"
        );
        // C leaves what it found before an error.
        assert_eq!(entries(&decoded, &input), dec, "{context}");

        let tags = [lv1, lv2].map(|tag| (tag != 0).then_some(tag));
        let mut preferred = vec![TlvParsed::new(); multiples];
        match parse_multiple(&mut preferred, definition, &input, tags) {
            Ok(count) => {
                assert_eq!(count as i32, rc, "{context}");
                successes += 1;
            }
            Err(error) => {
                assert_eq!(compat::tlv_error_code(error), rc, "{context}");
                failures += 1;
            }
        }
        assert_eq!(entries(&preferred, &input), dec, "{context}");

        if multiples == 1 {
            let mut single = TlvParsed::new();
            assert_eq!(
                compat::tlv_parse(&mut single, definition, &input, lv1, lv2),
                rc,
                "{context}"
            );
            match parse(definition, &input, tags) {
                Ok((parsed, count)) => {
                    assert_eq!(count as i32, rc, "{context}");
                    assert_eq!(parsed, single, "{context}");
                }
                Err(error) => assert_eq!(compat::tlv_error_code(error), rc, "{context}"),
            }
        }
    }
    assert!(successes > 100 && failures > 100, "{successes} {failures}");
}

fn message(headroom: usize) -> Msgb {
    Msgb::with_headroom(MSG_SIZE, headroom).unwrap()
}

#[test]
fn encode_one_matches_c_where_lengths_fit_and_truncates_in_compat() {
    let (mut rejected, mut accepted) = (0, 0);
    for record in records() {
        let Record::EncodeOne {
            kind: name,
            tag,
            len,
            rc,
            out,
        } = record
        else {
            continue;
        };
        let context = format!("{name} tag {tag} len {len}");
        let kind = kind(&name);
        // The formats that read one value byte do so whatever the length.
        let bytes = value((len as usize).max(1));

        let mut msg = message(PUT_HEADROOM);
        let actual = compat::tlv_encode_one(&mut msg, kind, tag, len, &bytes).unwrap();
        assert_eq!(actual, rc, "{context}");
        assert!(out.matches(msg.data()), "{context}");

        let fits = match kind {
            TlvType::Tlv => len <= 0xff,
            TlvType::Tl16v => len <= 0xffff,
            TlvType::Tvlv | TlvType::VtvlvGan => len as usize <= VARIABLE_MAX,
            _ => true,
        };
        let mut msg = message(PUT_HEADROOM);
        let result = encode_one(&mut msg, kind, tag, &bytes[..len as usize]);
        if matches!(kind, TlvType::Tv | TlvType::SingleTv) && len == 0 {
            assert_eq!(result, Err(TlvError::ValueMissing), "{context}");
        } else if fits {
            result.unwrap_or_else(|error| panic!("{context}: {error}"));
            assert!(out.matches(msg.data()), "{context}");
            accepted += 1;
        } else {
            assert_eq!(result, Err(TlvError::ValueTooLong), "{context}");
            assert_eq!(msg.data(), &[] as &[u8], "{context}");
            rejected += 1;
        }
    }
    assert!(accepted > 100 && rejected > 50, "{accepted} {rejected}");
}

#[test]
fn encode_matches_c_in_tag_and_given_order() {
    let definitions = definitions();
    let mut checked = 0;
    for record in records() {
        let Record::Encode {
            def,
            encode_def,
            input,
            ordered,
            order,
            rc,
            out,
        } = record
        else {
            continue;
        };
        let context = format!("{def} {encode_def} {order:?}");
        let (parsed, _) = parse(&definitions[&def], &input, [None, None]).unwrap();
        let target = &definitions[&encode_def];

        let mut msg = message(PUT_HEADROOM);
        let written = if ordered {
            encode_ordered(&mut msg, target, &parsed, &order)
        } else {
            encode(&mut msg, target, &parsed)
        };
        assert_eq!(written, Ok(rc as usize), "{context}");
        assert!(out.matches(msg.data()), "{context}");

        let mut msg = message(PUT_HEADROOM);
        let written = if ordered {
            compat::tlv_encode_ordered(&mut msg, target, &parsed, &order)
        } else {
            compat::tlv_encode(&mut msg, target, &parsed)
        };
        assert_eq!(written, Ok(rc), "{context}");
        assert!(out.matches(msg.data()), "{context}");
        checked += 1;
    }
    assert_eq!(checked, 10);
}

/// The same value bytes as C's, as host-order 16-bit values.
fn values16(len: usize) -> Vec<u16> {
    value(2 * len)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_ne_bytes(*pair))
        .collect()
}

fn prepared(op: &str) -> Msgb {
    let headroom = if op.ends_with("_push") {
        PUSH_HEADROOM
    } else {
        PUT_HEADROOM
    };
    let mut msg = message(headroom);
    msg.put_u8(MSG_PREFIX[0]).unwrap();
    msg.put_u8(MSG_PREFIX[1]).unwrap();
    let tail = msg.tail_offset();
    msg.backing_mut()[tail..].fill(FILL);
    msg
}

/// Runs one preferred method. Returns the offset the method returned, if it has one.
fn preferred_op(
    msg: &mut Msgb,
    op: &str,
    tag: u16,
    len: u16,
    num: u32,
) -> Result<Option<usize>, TlvError> {
    let bytes = value(usize::from(len));
    let tag8 = tag as u8;
    match op {
        "lv" => msg.put_lv(&bytes)?,
        "tlv" => msg.put_tlv(tag8, &bytes)?,
        "tlv_zeroed" => msg.put_tlv_zeroed(tag8, len as u8)?,
        "tl" => return msg.put_tl(tag8).map(Some),
        "tlv16" => msg.put_tlv16(tag8, &values16(usize::from(len)))?,
        "tl16v" => msg.put_tl16v(tag8, &bytes)?,
        "t16lv" => msg.put_t16lv(tag, &bytes)?,
        "tvl" => return msg.put_tvl(tag8, len).map(Some),
        "tvlv" => msg.put_tvlv(tag8, &bytes)?,
        "tvlv_16be" => msg.put_tvlv_u16(tag8, num as u16)?,
        "tvlv_32be" => msg.put_tvlv_u32(tag8, num)?,
        "vtvlv_gan" => msg.put_vtvlv_gan(tag, &bytes)?,
        "l16tv" => msg.put_l16tv(tag8, &bytes)?,
        "tv" => msg.put_tv(tag8, num as u8)?,
        "tv_fixed" => msg.put_tv_fixed(tag8, &bytes)?,
        "v" => msg.put_v(num as u8)?,
        "tv16" => msg.put_tv16(tag8, num as u16)?,
        "tv32" => msg.put_tv32(tag8, num)?,
        "tlv_push" => msg.push_tlv(tag8, &bytes)?,
        "tlv1_push" => msg.push_tlv1(tag8, num as u8)?,
        "tv_push" => msg.push_tv(tag8, num as u8)?,
        "tv16_push" => msg.push_tv16(tag8, num as u16)?,
        "tv32_push" => msg.push_tv32(tag8, num)?,
        "tvlv_push" => msg.push_tvlv(tag8, &bytes)?,
        "vtvl_gan_push" => msg.push_vtvl_gan(tag, len)?,
        "vtvlv_gan_push" => msg.push_vtvlv_gan(tag, &bytes)?,
        other => panic!("unknown operation {other}"),
    }
    Ok(None)
}

fn compat_op(
    msg: &mut Msgb,
    op: &str,
    tag: u16,
    len: u16,
) -> Option<Result<Option<usize>, TlvError>> {
    let bytes = value(usize::from(len));
    Some(match op {
        "tvl" => compat::msgb_tvl_put(msg, tag as u8, len).map(Some),
        "tvlv" => compat::msgb_tvlv_put(msg, tag as u8, len, &bytes).map(|()| None),
        "tvlv_push" => compat::msgb_tvlv_push(msg, tag as u8, len, &bytes).map(|()| None),
        "vtvlv_gan" => compat::msgb_vtvlv_gan_put(msg, tag, len, &bytes).map(|()| None),
        "vtvl_gan_push" => compat::msgb_vtvl_gan_push(msg, tag, len).map(|()| None),
        "vtvlv_gan_push" => compat::msgb_vtvlv_gan_push(msg, tag, len, &bytes).map(|()| None),
        _ => return None,
    })
}

fn check_msgb(
    msg: &Msgb,
    returned: Option<usize>,
    ret: i64,
    headroom: usize,
    out: &Output,
    context: &str,
) {
    assert!(out.matches(msg.data()), "{context}");
    assert_eq!(msg.data_offset(), headroom, "{context}");
    match returned {
        Some(offset) => assert_eq!((offset - msg.data_offset()) as i64, ret, "{context}"),
        None => assert_eq!(ret, UNTOUCHED, "{context}"),
    }
}

#[test]
fn msgb_methods_match_c_and_reject_what_c_cannot_represent() {
    let (mut matched, mut rejected, mut wide) = (0, 0, 0);
    for record in records() {
        let Record::Msgb {
            op,
            tag,
            len,
            num,
            ret,
            headroom,
            out,
        } = record
        else {
            continue;
        };
        let context = format!("{op} tag {tag} len {len} num {num}");

        let mut msg = prepared(&op);
        if let Some(result) = compat_op(&mut msg, &op, tag, len) {
            let returned = result.unwrap_or_else(|error| panic!("{context}: {error}"));
            check_msgb(&msg, returned, ret, headroom, &out, &context);
            wide += 1;
        }

        let mut msg = prepared(&op);
        let before = msg.data().to_vec();
        match preferred_op(&mut msg, &op, tag, len, num) {
            Ok(returned) => {
                check_msgb(&msg, returned, ret, headroom, &out, &context);
                matched += 1;
            }
            Err(TlvError::ValueTooLong) => {
                assert!(usize::from(len) > VARIABLE_MAX, "{context}");
                assert_eq!(msg.data(), before, "{context}");
                rejected += 1;
            }
            Err(TlvError::TagTooLarge) => {
                assert!(usize::from(tag) > VARIABLE_MAX, "{context}");
                assert_eq!(msg.data(), before, "{context}");
                rejected += 1;
            }
            Err(error) => panic!("{context}: {error}"),
        }
    }
    assert!(
        matched > 500 && rejected > 30 && wide > 100,
        "{matched} {rejected} {wide}"
    );
}

#[test]
fn gross_lengths_match_c() {
    for record in records() {
        match record {
            Record::Gross {
                len,
                lv,
                tlv,
                tlv16,
                tl16v,
                l16tv,
                t16lv,
                tvlv,
            } => {
                let wide = len as usize;
                assert_eq!(lv_gross_len(wide), lv as usize);
                assert_eq!(tlv_gross_len(wide), tlv as usize);
                assert_eq!(tlv16_gross_len(wide), tlv16 as usize);
                assert_eq!(tl16v_gross_len(wide), tl16v as usize);
                assert_eq!(tl16v_gross_len(wide), l16tv as usize);
                assert_eq!(tl16v_gross_len(wide), t16lv as usize);
                assert_eq!(compat::tvlv_gross_len(len as u16), tvlv, "{len}");
                // C's result wraps at 16 bits, and this one doesn't.
                assert_eq!(tvlv_gross_len(wide) as u16, tvlv, "{len}");
            }
            Record::GrossGan {
                tag,
                len,
                vtvl,
                vtvlv,
            } => {
                let (wide_tag, wide_len) = (usize::from(tag), usize::from(len));
                assert_eq!(compat::vtvl_gan_gross_len(tag, len), vtvl);
                assert_eq!(compat::vtvlv_gan_gross_len(tag, len), vtvlv);
                assert_eq!(vtvl_gan_gross_len(wide_tag, wide_len), usize::from(vtvl));
                assert_eq!(vtvlv_gan_gross_len(wide_tag, wide_len) as u16, vtvlv);
            }
            _ => {}
        }
    }
}

#[test]
fn shifting_matches_c() {
    let mut checked = 0;
    for record in records() {
        let Record::Shift {
            op,
            arg,
            input,
            rc,
            consumed,
            remaining,
            tag,
            val,
            val_len,
        } = record
        else {
            continue;
        };
        let context = format!("{op} {arg} {input:02x?}");
        let sentinel = [0u8; 1];
        let mut data = &input[..];
        let mut value = &sentinel[..];
        let mut found_tag = FILL;
        let actual = match op.as_str() {
            "v_fixed" => compat::osmo_shift_v_fixed(&mut data, arg as usize, Some(&mut value)),
            "match_tv_fixed" => compat::osmo_match_shift_tv_fixed(
                &mut data,
                arg as u8,
                (arg >> 8) as usize,
                Some(&mut value),
            ),
            "tlv" => compat::osmo_shift_tlv(&mut data, Some(&mut found_tag), Some(&mut value)),
            "match_tlv" => compat::osmo_match_shift_tlv(&mut data, arg as u8, Some(&mut value)),
            "lv" => compat::osmo_shift_lv(&mut data, Some(&mut value)),
            other => panic!("unknown operation {other}"),
        };
        assert_eq!(actual, rc, "{context}");
        assert_eq!(data.len(), remaining, "{context}");
        assert_eq!(input.len() - data.len(), consumed, "{context}");
        assert_eq!(found_tag, tag, "{context}");
        if val == UNTOUCHED {
            assert_eq!(value.as_ptr(), sentinel.as_ptr(), "{context}");
        } else {
            assert_eq!(offset(&input, value) as i64, val, "{context}");
            // The fixed forms don't write a length, so C's output stays at its fill.
            if !op.ends_with("fixed") {
                assert_eq!(value.len(), val_len, "{context}");
            }
        }

        let mut data = &input[..];
        let preferred: Result<Option<(&[u8], usize)>, TlvError> = match op.as_str() {
            "v_fixed" => shift_v_fixed(&mut data, arg as usize).map(|v| Some((v, v.len()))),
            "match_tv_fixed" => match_shift_tv_fixed(&mut data, arg as u8, (arg >> 8) as usize)
                .map(|v| v.map(|v| (v, v.len() + 1))),
            "tlv" => shift_tlv(&mut data).map(|(found, v)| {
                assert_eq!(found, tag, "{context}");
                Some((v, v.len() + 2))
            }),
            "match_tlv" => {
                match_shift_tlv(&mut data, arg as u8).map(|v| v.map(|v| (v, v.len() + 2)))
            }
            "lv" => shift_lv(&mut data).map(|v| Some((v, v.len() + 1))),
            other => panic!("unknown operation {other}"),
        };
        match preferred {
            Ok(Some((found, used))) => {
                assert_eq!(used as i32, rc, "{context}");
                assert_eq!(offset(&input, found) as i64, val, "{context}");
            }
            Ok(None) => assert_eq!(rc, 0, "{context}"),
            Err(_) => assert_eq!(rc, -1, "{context}"),
        }
        assert_eq!(data.len(), remaining, "{context}");
        checked += 1;
    }
    assert_eq!(checked, 1170);
}

#[test]
fn merge_copy_and_patch_match_c() {
    let definitions = definitions();
    let mixed = &definitions["mixed"];
    let mut checked = 0;
    for record in records() {
        match record {
            Record::Merge {
                dst_input,
                src_input,
                rc,
                result,
            } => {
                let (mut compat_dst, _) = parse(mixed, &dst_input, [None, None]).unwrap();
                let (mut preferred, _) = parse(mixed, &dst_input, [None, None]).unwrap();
                let (src, _) = parse(mixed, &src_input, [None, None]).unwrap();
                assert_eq!(compat::osmo_tlvp_merge(&mut compat_dst, &src), rc);
                preferred.merge(&src);
                let tags: Vec<u8> = compat_dst.iter().map(|ie| ie.tag).collect();
                let expected: Vec<u8> = result.iter().map(|(tag, ..)| *tag).collect();
                assert_eq!(tags, expected);
                // An empty value in the source adds nothing, in either API.
                let tags: Vec<u8> = preferred.iter().map(|ie| ie.tag).collect();
                assert_eq!(tags, expected);
                for (tag, len, bytes) in result {
                    // C keeps length zero for what it merged, where this keeps the bytes.
                    assert_eq!(
                        compat_dst.get(tag).unwrap().len(),
                        usize::from(len),
                        "{tag}"
                    );
                    if len == 0 && !bytes.is_empty() {
                        assert_eq!(preferred.get(tag).unwrap(), bytes, "{tag}");
                    } else {
                        assert_eq!(preferred.get(tag).unwrap().len(), usize::from(len), "{tag}");
                    }
                }
                checked += 1;
            }
            Record::Copy {
                input,
                lens,
                present,
            } => {
                let (parsed, _) = parse(mixed, &input, [None, None]).unwrap();
                let lengths = compat::osmo_tlvp_copy(Some(&parsed));
                let actual: Vec<(u8, u16)> = (0..=255)
                    .filter(|tag| lengths[usize::from(*tag)] != 0)
                    .map(|tag| (tag, lengths[usize::from(tag)]))
                    .collect();
                assert_eq!(actual, lens);
                // C's copy has no values at all, which `clone` does not share.
                assert!(present.is_empty());
                assert_eq!(parsed.clone(), parsed);
                assert_eq!(compat::osmo_tlvp_copy(None), [0; 256]);
                checked += 1;
            }
            Record::Patch { dst, src, result } => {
                let mut patched = definition(&dst);
                patched.patch(&definition(&src));
                assert_eq!(patched, definition(&result));
                checked += 1;
            }
            _ => {}
        }
    }
    assert_eq!(checked, 3);
}

#[test]
fn accessors_match_c() {
    let mixed = &definitions()["mixed"].clone();
    let input = [
        0x01, 0x02, 0xaa, 0x04, 0x00, 0x20, 0x01, 0x99, 0x05, 0x00, 0x02, 0x12, 0x34, 0x81, 0x00,
        0x04, 0x89, 0xab, 0xcd, 0xef, 0xff, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05,
    ];
    let (parsed, _) = parse(mixed, &input, [None, None]).unwrap();
    let mut checked = 0;
    for record in records() {
        let Record::Access {
            tag,
            present,
            len,
            val8,
            val16be,
            val32be,
            val16_unal,
            val32_unal,
            min_len,
        } = record
        else {
            continue;
        };
        assert_eq!(parsed.is_present(tag), present, "{tag}");
        assert_eq!(
            parsed.get(tag).map_or(-1, |value| value.len() as i32),
            len,
            "{tag}"
        );
        assert_eq!(parsed.val8(tag, 0x5a), val8, "{tag}");
        assert_eq!(parsed.val16be(tag).map_or(-1, i32::from), val16be, "{tag}");
        assert_eq!(parsed.val32be(tag).map_or(-1, i64::from), val32be, "{tag}");
        assert_eq!(
            compat::tlvp_val16_unal(&parsed, tag).map_or(-1, i32::from),
            val16_unal,
            "{tag}"
        );
        assert_eq!(
            compat::tlvp_val32_unal(&parsed, tag).map_or(-1, i64::from),
            val32_unal,
            "{tag}"
        );
        for (min, expected) in min_len.iter().enumerate() {
            assert_eq!(
                parsed.get_min_len(tag, min).is_some(),
                *expected,
                "{tag} {min}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 8);
}

const ATTACH_IES: [u8; 2] = [0x04, 0x05];
const DETACH_IES: [u8; 1] = [0x04];
const MESSAGE_NAMES: [(u8, &str); 2] = [(1, "One"), (7, "Seven")];

/// The protocol the harness defines.
fn protocol(definition: &TlvDefinition) -> TlvProtocol<'_> {
    let mut protocol = TlvProtocol::new("plain", definition);
    protocol.set_message(
        1,
        TlvMessageDef {
            name: Some("Attach"),
            mandatory: &ATTACH_IES,
            flags: 0x11,
        },
    );
    protocol.set_message(
        2,
        TlvMessageDef {
            name: None,
            mandatory: &DETACH_IES,
            flags: 0x02,
        },
    );
    protocol.set_message(
        3,
        TlvMessageDef {
            name: None,
            mandatory: &[],
            flags: 0x04,
        },
    );
    protocol.set_ie(
        0x04,
        TlvIeDef {
            min_len: 2,
            name: Some("Four"),
        },
    );
    protocol.set_ie(
        0x05,
        TlvIeDef {
            min_len: 3,
            name: None,
        },
    );
    protocol
}

#[test]
fn protocol_rules_and_names_match_c() {
    let definitions = definitions();
    let plain = protocol(&definitions["mixed"]);
    let mut named = plain.clone();
    named.message_names = Some(&MESSAGE_NAMES);
    let mut codes = std::collections::HashSet::new();
    for record in records() {
        match record {
            Record::Prot {
                msg_type,
                input,
                multiples,
                parse: parse_rc,
                validate,
                ..
            } => {
                let context = format!("{msg_type} {input:02x?} x{multiples}");
                codes.insert(parse_rc);
                let mut decoded = vec![TlvParsed::new(); multiples];
                assert_eq!(
                    compat::osmo_tlv_prot_parse(&plain, &mut decoded, msg_type, &input, 0, 0),
                    parse_rc,
                    "{context}"
                );
                if validate != UNTOUCHED as i32 {
                    assert_eq!(
                        compat::osmo_tlv_prot_validate_tp(&plain, msg_type, &decoded[0]),
                        validate,
                        "{context}"
                    );
                }
                let mut decoded = vec![TlvParsed::new(); multiples];
                let result = plain.parse(&mut decoded, msg_type, &input, [None, None]);
                assert_eq!(
                    result.map_or_else(compat::tlv_error_code, |()| 0),
                    parse_rc,
                    "{context}"
                );
            }
            Record::ProtName {
                msg_type,
                plain: plain_name,
                named: named_name,
                flags,
            } => {
                assert_eq!(plain.message_name(msg_type), plain_name);
                assert_eq!(named.message_name(msg_type), named_name);
                assert_eq!(plain.flags(msg_type), flags);
            }
            Record::ProtIe { iei, name } => assert_eq!(plain.ie_name(iei), name),
            _ => {}
        }
    }
    for code in [0, -2, -3, -50, -51] {
        assert!(codes.contains(&code), "status {code} never occurred");
    }
}
