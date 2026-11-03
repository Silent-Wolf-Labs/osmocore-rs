use crate::a5_fixture::{self, Record};
use osmocore_rs::a5::*;
use osmocore_rs::compat::a5 as compat;

const FIXTURE: &str = include_str!("../compatibility/vectors/a5/a5_vectors.ndjson");

fn records() -> Vec<Record> {
    a5_fixture::validate(FIXTURE)
}

#[test]
fn fixture_is_well_formed() {
    a5_fixture::check_corruptions(FIXTURE);
}

#[test]
fn stream_size_matches_the_recording() {
    assert_eq!(STREAM_BITS, a5_fixture::STREAM_BITS);
}

#[test]
fn frame_counts_match_c() {
    for record in records() {
        if let Record::FnCount { frame, count } = record {
            assert_eq!(frame_count(frame), count, "frame {frame}");
            assert_eq!(compat::osmo_a5_fn_count(frame), count, "frame {frame}");
        }
    }
}

#[test]
fn streams_match_c() {
    let mut seen = [0; 3];
    for record in records() {
        let Record::Stream {
            n,
            key,
            frame,
            downlink,
            uplink,
            ..
        } = record
        else {
            continue;
        };
        seen[usize::from(n)] += 1;
        let stream = match n {
            0 => a5_0(),
            1 => a5_1(&key, frame),
            2 => a5_2(&key, frame),
            _ => unreachable!("the reader rejects other ciphers"),
        };
        assert_eq!(stream.downlink[..], downlink, "A5/{n} {key:02x?} {frame}");
        assert_eq!(stream.uplink[..], uplink, "A5/{n} {key:02x?} {frame}");

        let (mut dl, mut ul) = ([2; STREAM_BITS], [2; STREAM_BITS]);
        let rc = compat::osmo_a5(
            i32::from(n),
            Some(&key),
            frame,
            Some(&mut dl),
            Some(&mut ul),
        );
        assert_eq!(rc, 0);
        assert_eq!(dl[..], downlink);
        assert_eq!(ul[..], uplink);
    }
    assert!(seen.iter().all(|&count| count > 0));
}

#[test]
fn results_match_c() {
    let key = [0; KEY_BYTES];
    for record in records() {
        if let Record::Result { n, rc } = record {
            let mut dl = [2; STREAM_BITS];
            assert_eq!(compat::osmo_a5(n, Some(&key), 0, Some(&mut dl), None), rc);
            if rc != 0 {
                assert_eq!(dl, [2; STREAM_BITS], "an error leaves the output alone");
            }
        }
    }
}
