use crate::kdf_fixture;

const FIXTURE: &str = include_str!("../compatibility/vectors/kdf/kdf_vectors.ndjson");

#[test]
fn fixture_is_well_formed() {
    kdf_fixture::check_corruptions(FIXTURE);
}

#[cfg(feature = "auth")]
mod with_auth {
    use super::FIXTURE;
    use crate::kdf_fixture::{self, Record};
    use osmocore_rs::compat::kdf as compat;
    use osmocore_rs::gsm::auth::{Ck, Ik};
    use osmocore_rs::gsm::kdf::{self, Kasme, NasKey};

    fn of_kind(kind: &str) -> Vec<Record> {
        let records: Vec<_> = kdf_fixture::validate(FIXTURE)
            .into_iter()
            .filter(|r| r.kind == kind)
            .collect();
        assert!(!records.is_empty(), "no {kind} records");
        records
    }

    fn keys(record: &Record) -> (Ck, Ik) {
        (Ck::new(record.array("ck")), Ik::new(record.array("ik")))
    }

    fn kasme(record: &Record) -> Kasme {
        Kasme::new(record.array("kasme"))
    }

    #[test]
    fn kc128_matches_c() {
        for record in of_kind("kc128") {
            let expected: [u8; 16] = record.output();
            let mut out = [0; 16];
            compat::osmo_kdf_kc128(&record.array("ck"), &record.array("ik"), &mut out);
            assert_eq!(out, expected, "compat {:?}", record.fields);
            let (ck, ik) = keys(&record);
            assert_eq!(kdf::kc128(&ck, &ik).as_bytes(), &expected);
        }
    }

    #[test]
    fn kasme_matches_c() {
        for record in of_kind("kasme") {
            let expected: [u8; 32] = record.output();
            let (sqn, ak) = (record.array::<6>("sqn"), record.array::<6>("ak"));
            let plmn_id = record.array("plmn_id");
            let mut out = [0; 32];
            compat::osmo_kdf_kasme(
                &record.array("ck"),
                &record.array("ik"),
                &plmn_id,
                &sqn,
                &ak,
                &mut out,
            );
            assert_eq!(out, expected, "compat {:?}", record.fields);
            let (ck, ik) = keys(&record);
            let concealed = std::array::from_fn(|i| sqn[i] ^ ak[i]);
            assert_eq!(
                kdf::kasme(&ck, &ik, &plmn_id, &concealed).as_bytes(),
                &expected
            );
        }
    }

    #[test]
    fn kenb_matches_c() {
        for record in of_kind("enb") {
            let expected: [u8; 32] = record.output();
            let count = u32::try_from(record.int("ul_count")).expect("a 32-bit count");
            let mut out = [0; 32];
            compat::osmo_kdf_enb(&record.array("kasme"), count, &mut out);
            assert_eq!(out, expected, "compat {:?}", record.fields);
            assert_eq!(kdf::kenb(&kasme(&record), count).as_bytes(), &expected);
        }
    }

    #[test]
    fn nh_matches_c() {
        for record in of_kind("nh") {
            let expected: [u8; 32] = record.output();
            let sync_input = record.array("sync_input");
            let mut out = [0; 32];
            compat::osmo_kdf_nh(&record.array("kasme"), &sync_input, &mut out);
            assert_eq!(out, expected, "compat {:?}", record.fields);
            assert_eq!(kdf::nh(&kasme(&record), &sync_input).as_bytes(), &expected);
        }
    }

    #[test]
    fn knas_matches_c() {
        let mut preferred = 0;
        for record in of_kind("nas") {
            let expected: [u8; 16] = record.output();
            let algo_type = u8::try_from(record.int("algo_type")).expect("a byte");
            let algo_id = u8::try_from(record.int("algo_id")).expect("a byte");
            let mut out = [0; 16];
            compat::osmo_kdf_nas(algo_type, algo_id, &record.array("kasme"), &mut out);
            assert_eq!(out, expected, "compat {:?}", record.fields);
            // The preferred API takes only the two NAS key types.
            let key = match algo_type {
                1 => NasKey::Encryption,
                2 => NasKey::Integrity,
                _ => continue,
            };
            assert_eq!(
                kdf::knas(&kasme(&record), key, algo_id).as_bytes(),
                &expected
            );
            preferred += 1;
        }
        assert_eq!(preferred, 56);
    }
}
