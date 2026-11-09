use crate::kasumi_fixture::{self, Record};
use osmocore_rs::compat::kasumi as compat;
use osmocore_rs::gsm::a5::{a5_3_count, a5_4_count};
use osmocore_rs::gsm::kasumi::*;

const FIXTURE: &str = include_str!("../compatibility/vectors/kasumi/kasumi_vectors.ndjson");

fn records() -> Vec<Record> {
    kasumi_fixture::validate(FIXTURE)
}

#[test]
fn fixture_is_well_formed() {
    kasumi_fixture::check_corruptions(FIXTURE);
}

fn arrays(subkeys: &Subkeys) -> [[u16; 8]; 8] {
    [
        subkeys.kl1,
        subkeys.kl2,
        subkeys.ko1,
        subkeys.ko2,
        subkeys.ko3,
        subkeys.ki1,
        subkeys.ki2,
        subkeys.ki3,
    ]
}

#[test]
fn key_expansion_matches_c() {
    for record in records() {
        let Record::KeyExpand { key, subkeys, .. } = record else {
            continue;
        };
        assert_eq!(arrays(&Subkeys::new(&key)), subkeys, "key {key:02x?}");

        let mut c = [[0x5555; 8]; 8];
        let [kl1, kl2, ko1, ko2, ko3, ki1, ki2, ki3] = &mut c;
        compat::_kasumi_key_expand(&key, kl1, kl2, ko1, ko2, ko3, ki1, ki2, ki3);
        assert_eq!(c, subkeys);
    }
}

#[test]
fn blocks_match_c() {
    for record in records() {
        let Record::Block {
            key, input, output, ..
        } = record
        else {
            continue;
        };
        let kasumi = Kasumi::new(&key);
        assert_eq!(kasumi.encrypt(input), output, "key {key:02x?} {input:016x}");

        let [kl1, kl2, ko1, ko2, ko3, ki1, ki2, ki3] = arrays(kasumi.subkeys());
        assert_eq!(
            compat::_kasumi(input, &kl1, &kl2, &ko1, &ko2, &ko3, &ki1, &ki2, &ki3),
            output
        );
    }
}

#[test]
fn kgcore_matches_c() {
    let mut preferred = 0;
    for record in records() {
        let Record::Kgcore {
            ca,
            cb,
            cc,
            cd,
            key,
            cl,
            out,
            ..
        } = record
        else {
            continue;
        };
        let mut c = vec![kasumi_fixture::GUARD; out.len()];
        compat::_kasumi_kgcore(ca, cb, cc, cd, &key, &mut c, cl);
        assert_eq!(c, out, "CA {ca} CB {cb} CC {cc} CD {cd} CL {cl}");

        // The preferred API takes only a 5-bit bearer and a 1-bit direction.
        if let (Some(bearer), Some(direction)) =
            (Bearer::new(cb), [false, true].get(usize::from(cd)))
        {
            let input = KgcoreInput {
                ca,
                bearer,
                count: cc,
                direction: *direction,
            };
            let written = out.len() - kasumi_fixture::GUARD_BYTES;
            let mut keystream = vec![0; written];
            kgcore(&key, &input, &mut keystream);
            assert_eq!(keystream, out[..written]);
            preferred += 1;
        }
    }
    assert!(preferred > 0);
}

#[test]
fn count_based_a5_streams_match_c() {
    let mut seen = [0; 2];
    for record in records() {
        let Record::A5Count {
            n,
            key,
            count,
            downlink,
            uplink,
            ..
        } = record
        else {
            continue;
        };
        let stream = match n {
            3 => a5_3_count(key[..].try_into().unwrap(), count),
            4 => a5_4_count(key[..].try_into().unwrap(), count),
            _ => unreachable!("the reader rejects other ciphers"),
        };
        seen[usize::from(n - 3)] += 1;
        assert_eq!(stream.downlink[..], downlink, "A5/{n} {key:02x?} {count}");
        assert_eq!(stream.uplink[..], uplink, "A5/{n} {key:02x?} {count}");
    }
    assert!(seen.iter().all(|&count| count > 0));
}
