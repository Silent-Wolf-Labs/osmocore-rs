use crate::gsm0502_fixture::{self, Record};
use osmocore_rs::compat::gsm0502 as compat;
use osmocore_rs::gsm::gsm0502::*;
use std::cmp::Ordering;

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm0502/gsm0502_vectors.ndjson");

fn records() -> Vec<Record> {
    gsm0502_fixture::validate(FIXTURE)
}

fn channel(name: &str) -> RemapChannel {
    match name {
        "TCH/F" => RemapChannel::TchF,
        "TCH/H0" => RemapChannel::TchH0,
        "TCH/H1" => RemapChannel::TchH1,
        "FACCH/F" => RemapChannel::FacchF,
        "FACCH/H0" => RemapChannel::FacchH0,
        "FACCH/H1" => RemapChannel::FacchH1,
        other => panic!("unknown channel {other}"),
    }
}

fn constant(name: &str) -> u64 {
    match name {
        "GSM_TDMA_FN_DURATION_nS" => TDMA_FRAME_DURATION_NS.into(),
        "GSM_TDMA_FN_DURATION_uS" => TDMA_FRAME_DURATION_US.into(),
        "GSM_TDMA_SUPERFRAME" => TDMA_SUPERFRAME.into(),
        "GSM_TDMA_HYPERFRAME" => TDMA_HYPERFRAME.into(),
        "GSM_NBITS_NB_GMSK_TAIL" => NB_GMSK_TAIL_BITS as u64,
        "GSM_NBITS_NB_GMSK_PAYLOAD" => NB_GMSK_PAYLOAD_BITS as u64,
        "GSM_NBITS_NB_GMSK_TRAIN_SEQ" => NB_GMSK_TRAIN_SEQ_BITS as u64,
        "GSM_NBITS_NB_GMSK_BURST" => NB_GMSK_BURST_BITS as u64,
        "GSM_NBITS_NB_8PSK_TAIL" => NB_8PSK_TAIL_BITS as u64,
        "GSM_NBITS_NB_8PSK_PAYLOAD" => NB_8PSK_PAYLOAD_BITS as u64,
        "GSM_NBITS_NB_8PSK_TRAIN_SEQ" => NB_8PSK_TRAIN_SEQ_BITS as u64,
        "GSM_NBITS_NB_8PSK_BURST" => NB_8PSK_BURST_BITS as u64,
        "GSM_NBITS_SB_GMSK_TAIL" => SB_GMSK_TAIL_BITS as u64,
        "GSM_NBITS_SB_GMSK_PAYLOAD" => SB_GMSK_PAYLOAD_BITS as u64,
        "GSM_NBITS_SB_GMSK_ETRAIN_SEQ" => SB_GMSK_ETRAIN_SEQ_BITS as u64,
        "GSM_NBITS_SB_GMSK_BURST" => SB_GMSK_BURST_BITS as u64,
        "GSM_NBITS_DB_GMSK_TAIL" => DB_GMSK_TAIL_BITS as u64,
        "GSM_NBITS_DB_GMSK_MIXED" => DB_GMSK_MIXED_BITS as u64,
        "GSM_NBITS_DB_GMSK_BURST" => DB_GMSK_BURST_BITS as u64,
        "GSM_NBITS_AB_GMSK_ETAIL" => AB_GMSK_ETAIL_BITS as u64,
        "GSM_NBITS_AB_GMSK_SYNCH_SEQ" => AB_GMSK_SYNCH_SEQ_BITS as u64,
        "GSM_NBITS_AB_GMSK_PAYLOAD" => AB_GMSK_PAYLOAD_BITS as u64,
        "GSM_NBITS_AB_GMSK_TAIL" => AB_GMSK_TAIL_BITS as u64,
        "GSM_NBITS_AB_GMSK_BURST" => AB_GMSK_BURST_BITS as u64,
        other => panic!("unmapped constant {other}"),
    }
}

#[test]
fn constants_match_c() {
    for record in records() {
        if let Record::Constant { name, value } = record {
            assert_eq!(constant(&name), value, "{name}");
        }
    }
}

#[test]
fn frame_arithmetic_matches_c() {
    for record in records() {
        if let Record::Frame {
            a,
            b,
            sum,
            sub,
            diff,
            cmp,
            inc,
            dec,
        } = record
        {
            let context = format!("a={a} b={b}");
            assert_eq!(frame_add(a, b), sum, "{context}");
            assert_eq!(frame_sub(a, b), sub, "{context}");
            assert_eq!(frame_distance(a, b), diff, "{context}");
            assert_eq!(compat::gsm0502_fncmp(a, b), cmp, "{context}");
            let ordering = match cmp {
                -1 => Ordering::Less,
                0 => Ordering::Equal,
                _ => Ordering::Greater,
            };
            assert_eq!(frame_cmp(a, b), ordering, "{context}");
            assert_eq!(frame_add(a, 1), inc, "{context}");
            assert_eq!(frame_sub(a, 1), dec, "{context}");
        }
    }
}

#[test]
fn frames_half_a_hyperframe_apart_each_compare_greater() {
    let half = TDMA_HYPERFRAME / 2;
    assert_eq!(frame_cmp(0, half), Ordering::Greater);
    assert_eq!(frame_cmp(half, 0), Ordering::Greater);
}

#[test]
fn block_starts_match_c() {
    let mut remapped = 0;
    let mut unmapped = 0;
    for record in records() {
        if let Record::Remap {
            channel: name,
            frame,
            begin,
            ..
        } = record
        {
            let channel = channel(&name);
            let context = format!("{name} frame {frame}");
            assert_eq!(compat::gsm0502_fn_remap(frame, channel), begin, "{context}");
            match block_start(frame, channel) {
                Some(start) => {
                    assert_eq!(start, begin, "{context}");
                    remapped += 1;
                }
                None => {
                    assert_eq!(begin, frame, "{context}");
                    unmapped += 1;
                }
            }
        }
    }
    assert!(remapped > 0 && unmapped > 0);
}

#[test]
fn ccch_blocks_match_c() {
    for record in records() {
        if let Record::Ccch { frame, block } = record {
            assert_eq!(compat::gsm0502_fn2ccch_block(frame), block, "frame {frame}");
            let expected = u8::try_from(block).ok();
            assert_eq!(ccch_block(frame), expected, "frame {frame}");
        }
    }
}

#[test]
fn hopping_matches_c() {
    for record in records() {
        if let Record::Hop {
            case,
            start,
            step,
            hsn,
            maio,
            n,
            allocation,
            values,
        } = record
        {
            for (position, value) in values.iter().enumerate() {
                let frame = start.wrapping_add((position as u32).wrapping_mul(step));
                let context = format!("{case} frame {frame} hsn {hsn} maio {maio} n {n}");
                if allocation.is_empty() {
                    let index = hop_index(frame, hsn, maio, n).unwrap();
                    assert_eq!(index, usize::from(*value), "{context}");
                } else {
                    let arfcn = hop_arfcn(frame, hsn, maio, &allocation).unwrap();
                    assert_eq!(arfcn, *value, "{context}");
                }
            }
        }
    }
}

#[test]
fn hopping_rejects_what_c_cannot_compute() {
    assert_eq!(hop_index(0, 0, 0, 0), Err(HopError::EmptyAllocation));
    assert_eq!(hop_arfcn(0, 1, 0, &[]), Err(HopError::EmptyAllocation));
    assert_eq!(hop_index(0, 64, 0, 1), Err(HopError::InvalidHsn(64)));
    assert_eq!(hop_arfcn(0, 255, 0, &[1]), Err(HopError::InvalidHsn(255)));
    assert_eq!(hop_index(0, 63, 0, 1), Ok(0));
}

#[test]
fn paging_adapters_match_every_c_field_value() {
    for record in records() {
        if let Record::Paging {
            conf,
            ag,
            pa,
            imsi,
            blocks,
            subchannels,
            channels,
            group,
        } = record
        {
            let context = format!("conf {conf} ag {ag} pa {pa} imsi {imsi}");
            assert_eq!(
                compat::gsm0502_get_n_pag_blocks(conf, ag),
                blocks,
                "{context}"
            );
            assert_eq!(
                compat::gsm48_number_of_paging_subchannels(conf, ag, pa),
                subchannels,
                "{context}"
            );
            assert_eq!(
                compat::rsl_ccch_conf_to_bs_cc_chans(conf),
                channels,
                "{context}"
            );
            assert_eq!(
                compat::gsm0502_calc_paging_group(conf, ag, pa, imsi),
                group,
                "{context}"
            );
        }
    }
}

#[test]
fn paging_config_matches_c_and_rejects_only_invalid_fields() {
    let mut accepted = 0;
    for record in records() {
        if let Record::Paging {
            conf,
            ag,
            pa,
            imsi,
            blocks,
            subchannels,
            channels,
            group,
        } = record
        {
            let context = format!("conf {conf} ag {ag} pa {pa} imsi {imsi}");
            let Some(ccch) = CcchConf::from_code(conf) else {
                assert_eq!(channels, -1, "{context}");
                continue;
            };
            assert_eq!(ccch.code(), conf, "{context}");
            assert_eq!(ccch.channels() as i32, channels, "{context}");
            let config = PagingConfig::new(ccch, ag, pa);
            if conf == 1 && ag > 2 {
                assert_eq!(config, None, "{context}");
                continue;
            }
            let config = config.unwrap_or_else(|| panic!("{context}"));
            assert_eq!(config.conf(), ccch, "{context}");
            assert_eq!(config.paging_blocks(), blocks, "{context}");
            assert_eq!(config.subchannels() as i32, subchannels, "{context}");
            assert_eq!(config.paging_group(imsi), group, "{context}");
            assert_eq!(
                Some(config.ccch_group(imsi)),
                compat::gsm0502_get_ccch_group(imsi, ccch.channels(), config.subchannels()),
                "{context}"
            );
            accepted += 1;
        }
    }
    // Four non-combined codes with all 64 field pairs, plus 3 x 8 combined pairs,
    // at each of the four IMSIs.
    assert_eq!(accepted, (4 * 64 + 3 * 8) * 4);
    assert_eq!(PagingConfig::new(CcchConf::OneNonCombined, 8, 0), None);
    assert_eq!(PagingConfig::new(CcchConf::OneNonCombined, 0, 8), None);
}

#[test]
fn group_helpers_match_c() {
    for record in records() {
        if let Record::Group {
            imsi,
            channels,
            blocks,
            ccch,
            paging,
        } = record
        {
            let context = format!("imsi {imsi} channels {channels} blocks {blocks}");
            assert_eq!(
                compat::gsm0502_get_ccch_group(imsi, channels, blocks as u32),
                Some(ccch),
                "{context}"
            );
            assert_eq!(
                compat::gsm0502_get_paging_group(imsi, channels, blocks),
                Some(paging),
                "{context}"
            );
        }
    }
}

#[test]
fn group_helpers_reject_what_c_divides_by_zero() {
    assert_eq!(compat::gsm0502_get_ccch_group(1, 1, 0), None);
    assert_eq!(compat::gsm0502_get_ccch_group(1, 0, 1), None);
    assert_eq!(compat::gsm0502_get_paging_group(1, 1, 0), None);
    assert_eq!(compat::gsm0502_get_paging_group(1, 0, 1), None);
    // The product wraps to zero at 32 bits.
    assert_eq!(compat::gsm0502_get_ccch_group(1, 0x10000, 0x10000), None);
    assert_eq!(compat::gsm0502_get_paging_group(1, 0x10000, 0x10000), None);
}

#[test]
fn corrupted_fixtures_are_rejected() {
    gsm0502_fixture::check_corruptions(FIXTURE);
}

#[test]
fn paging_adapters_refuse_inputs_where_c_divides_by_zero() {
    // A zero count, directly or because the product wraps to zero at 32 bits.
    for (channels, blocks) in [(0, 8), (1, 0), (0, 0), (1 << 16, 1 << 16)] {
        assert_eq!(
            compat::gsm0502_get_ccch_group(123, channels, blocks),
            None,
            "{channels} {blocks}"
        );
        assert_eq!(
            compat::gsm0502_get_paging_group(123, channels, blocks as i32),
            None,
            "{channels} {blocks}"
        );
    }
}
