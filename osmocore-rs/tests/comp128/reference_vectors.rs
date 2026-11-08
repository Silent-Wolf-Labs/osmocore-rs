use crate::comp128_fixture::{self, Record};
use osmocore_rs::gsm::auth::{GsmAlgorithm, GsmVector, Ki, xor_2g};
use osmocore_rs::gsm::comp128::{comp128v1, comp128v2, comp128v3};

const FIXTURE: &str = include_str!("../compatibility/vectors/comp128/comp128_vectors.ndjson");

fn records() -> Vec<Record> {
    comp128_fixture::validate(FIXTURE)
}

fn expected(sres: [u8; 4], kc: [u8; 8]) -> GsmVector {
    GsmVector {
        sres,
        kc: kc.into(),
    }
}

#[test]
fn fixture_is_well_formed() {
    comp128_fixture::check_corruptions(FIXTURE);
}

#[test]
fn comp128_matches_c() {
    let mut seen = [0; 4];
    for record in records() {
        let Record::Comp128 {
            version,
            ki,
            rand,
            sres,
            kc,
            ..
        } = record
        else {
            continue;
        };
        seen[usize::from(version)] += 1;
        let ki = Ki::new(ki);
        let (direct, algorithm) = match version {
            1 => (comp128v1(&ki, &rand), GsmAlgorithm::Comp128v1),
            2 => (comp128v2(&ki, &rand), GsmAlgorithm::Comp128v2),
            3 => (comp128v3(&ki, &rand), GsmAlgorithm::Comp128v3),
            _ => unreachable!("the reader rejects other versions"),
        };
        let want = expected(sres, kc);
        assert_eq!(direct, want, "v{version} {ki:?} {rand:02x?}");
        assert_eq!(algorithm.vector(&ki, &rand), want, "v{version} {rand:02x?}");
    }
    assert!(seen[1..].iter().all(|&count| count > 0));
}

#[test]
fn xor_2g_matches_c() {
    let mut seen = 0;
    for record in records() {
        if let Record::Xor2g { ki, rand, sres, kc } = record {
            seen += 1;
            let ki = Ki::new(ki);
            let want = expected(sres, kc);
            assert_eq!(xor_2g(&ki, &rand), want, "{rand:02x?}");
            assert_eq!(GsmAlgorithm::Xor2g.vector(&ki, &rand), want);
        }
    }
    assert!(seen > 0);
}
