use crate::prbs_fixture::{self, FILL, GUARD, Op, Record};
use osmocore_rs::base::prbs::{PRBS7, PRBS9, PRBS11, PRBS15, Prbs, PrbsState};

const FIXTURE: &str = include_str!("../compatibility/vectors/prbs/prbs_vectors.ndjson");

fn records() -> Vec<Record> {
    prbs_fixture::validate(FIXTURE)
}

fn descriptor(record: &Record) -> Prbs {
    Prbs {
        name: String::leak(record.name.clone()),
        len: record.len,
        coeff: record.coeff,
    }
}

#[test]
fn every_call_matches_c_results_buffers_and_states() {
    let mut generator = None;
    for record in records() {
        let context = format!("{} step {}", record.case, record.step);
        if record.op == Op::Init {
            generator = Some(PrbsState::new(descriptor(&record)));
        }
        let generator = generator.as_mut().unwrap();
        match record.op {
            Op::Init => {}
            Op::Ubit => assert_eq!(i32::from(generator.next_bit()), record.ret, "{context}"),
            Op::Ubits => {
                let mut dst = vec![FILL; record.out_len + GUARD];
                generator.fill(&mut dst[..record.out_len]);
                assert_eq!(dst, record.dst, "{context}");
            }
        }
        assert_eq!(generator.state(), record.state, "{context}");
    }
}

#[test]
fn standard_sequences_match_the_c_descriptors() {
    let records = records();
    for (case, prbs) in [
        ("prbs7_two_periods", PRBS7),
        ("prbs9_two_periods", PRBS9),
        ("prbs11_two_periods", PRBS11),
        ("prbs15_two_periods", PRBS15),
    ] {
        let record = records.iter().find(|r| r.case == case).unwrap();
        assert_eq!(prbs.name, record.name);
        assert_eq!(prbs.len, record.len);
        assert_eq!(prbs.coeff, record.coeff);
    }
}

/// Checked apart from the recording: the generator returns to its start.
#[test]
fn standard_sequences_repeat_after_their_period() {
    for prbs in [PRBS7, PRBS9, PRBS11, PRBS15] {
        let period = (1usize << prbs.len) - 1;
        let mut generator = PrbsState::new(prbs);
        let start = generator.clone();
        let mut first = vec![0; period];
        generator.fill(&mut first);
        assert_eq!(generator, start, "{} period", prbs.name);
        let mut second = vec![0; period];
        generator.fill(&mut second);
        assert_eq!(first, second, "{} repeat", prbs.name);
        assert!(first.contains(&0) && first.contains(&1));
    }
}

#[test]
fn fill_matches_single_steps_and_ignores_previous_contents() {
    let mut stepped = PrbsState::new(PRBS9);
    let expected: Vec<u8> = (0..600).map(|_| stepped.next_bit()).collect();
    let mut filled = PrbsState::new(PRBS9);
    let mut out = vec![0xff; 600];
    filled.fill(&mut out);
    assert_eq!(out, expected);
    assert_eq!(filled, stepped);

    let before = filled.clone();
    filled.fill(&mut []);
    assert_eq!(filled, before);
}

#[test]
fn prbs_fixture_rejects_corruption() {
    prbs_fixture::check_corruptions(FIXTURE);
}

/// `next_bits` gives C's bits for each recorded fill, most significant first.
#[test]
fn next_bits_matches_c_fills() {
    let mut generator = None;
    let mut checked = 0;
    for record in records() {
        if record.op == Op::Init {
            generator = Some(PrbsState::new(descriptor(&record)));
        }
        let generator = generator.as_mut().unwrap();
        match record.op {
            Op::Init => {}
            Op::Ubit => {
                generator.next_bit();
            }
            Op::Ubits => {
                if record.out_len <= 64 {
                    let expected = record.dst[..record.out_len]
                        .iter()
                        .fold(0u64, |value, &bit| (value << 1) | u64::from(bit));
                    let mut copy = generator.clone();
                    assert_eq!(
                        copy.next_bits(record.out_len as u32),
                        expected,
                        "{}",
                        record.case
                    );
                    checked += 1;
                }
                let mut dst = vec![0; record.out_len];
                generator.fill(&mut dst);
            }
        }
    }
    assert!(checked > 0);
}

#[test]
fn next_bits_takes_up_to_64_bits() {
    let mut a = PrbsState::new(PRBS15);
    let mut b = a.clone();
    assert_eq!(a.next_bits(0), 0);
    let mut bits = [0u8; 64];
    b.fill(&mut bits);
    let expected = bits
        .iter()
        .fold(0u64, |value, &bit| (value << 1) | u64::from(bit));
    assert_eq!(a.next_bits(64), expected);
    assert_eq!(a, b);
}

#[test]
#[should_panic(expected = "count must be at most 64")]
fn next_bits_rejects_more_than_64() {
    PrbsState::new(PRBS7).next_bits(65);
}
