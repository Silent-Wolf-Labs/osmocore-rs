use osmocore_rs::bitvec::*;
use osmocore_rs::compat::bitvec::{bitvec_add_array, bitvec_find_bit_pos, bitvec_get_nth_set_bit};
#[path = "../support/bitvec_operations_fixture.rs"]
mod fixture;
const FIXTURE: &str = include_str!("../compatibility/vectors/bitvec_operations_vectors.json");
const ARRAY: [u32; 4] = [0, u32::MAX, 0x8000_0000, 0x1234_5678];

fn compare(text: &str) {
    for record in fixture::validated_records(text) {
        let case = &record["case"];
        let mut data = fixture::bytes(&record["before_hex"]);
        let len = fixture::number(&record, "data_len");
        let arg = fixture::number(&record, "argument") as u32;
        let value = fixture::number(&record, "value");
        let mut index = fixture::number(&record, "index_before") as u32;
        let mut bv = BitVec::new(&mut data[..len]);
        bv.cur_bit = fixture::number(&record, "cursor_before");
        let mut returned = 0u64;
        let mut error = None;
        let rc = match record["api"].as_str() {
            "bitvec_read_field" => {
                let result = bv.read_field(&mut index, arg);
                match result {
                    Ok(v) => {
                        returned = v;
                        error = Some(0);
                    }
                    Err(BitVecError::OutOfRange) => {
                        error = Some(75);
                    }
                    other => panic!("{case}: unexpected {other:?}"),
                }
                0
            }
            "bitvec_write_field" => {
                match bv.write_field(&mut index, fixture::word(&record["word_hex"]), arg) {
                    Ok(()) => 0,
                    Err(BitVecError::OutOfRange) => -22,
                    Err(BitVecError::TooManyBits) => -7,
                    other => panic!("{case}: unexpected {other:?}"),
                }
            }
            "bitvec_get_nth_set_bit" => i64::from(bitvec_get_nth_set_bit(&bv, arg)),
            "bitvec_find_bit_pos" => i64::from(bitvec_find_bit_pos(
                &bv,
                arg,
                [BitValue::Zero, BitValue::One, BitValue::L, BitValue::H][value],
            )),
            "bitvec_rl" => i64::from(bv.rl(value != 0).unwrap()),
            "bitvec_rl_curbit" => i64::from(bv.rl_curbit(value != 0, arg).unwrap()),
            "bitvec_shiftl" => {
                bv.shift_left(arg).unwrap();
                0
            }
            "bitvec_add_array" => i64::from(bitvec_add_array(
                &mut bv,
                &ARRAY[..fixture::number(&record, "array_len")],
                fixture::number(&record, "dry_run") != 0,
                arg,
            )),
            api => panic!("unexpected {api}"),
        };
        assert_eq!(
            rc,
            record["return_code"].parse::<i64>().unwrap(),
            "{case} return code"
        );
        assert_eq!(
            returned,
            fixture::word(&record["return_hex"]),
            "{case} returned field"
        );
        if let Some(errno) = error {
            assert_eq!(
                errno,
                fixture::number(&record, "errno_after"),
                "{case} structured field error"
            );
        }
        assert_eq!(
            index,
            fixture::number(&record, "index_after") as u32,
            "{case} index"
        );
        assert_eq!(
            bv.cur_bit,
            fixture::number(&record, "cursor_after"),
            "{case} cursor"
        );
        assert_eq!(
            data,
            fixture::bytes(&record["after_hex"]),
            "{case} full backing array"
        );
    }
}

#[test]
fn fields_search_runs_shifts_and_arrays_match_c() {
    compare(FIXTURE);
}

#[test]
fn rejects_corrupt_inputs_provenance_and_outcomes() {
    for corrupt in [
        FIXTURE.replacen("\"case\":0", "\"case\":1", 1),
        FIXTURE.replacen("789ef706", "00000000", 1),
        FIXTURE.replacen("\"argument\":0", "\"argument\":\"0\"", 1),
        FIXTURE.replacen(
            "\"array_hex\":\"00000000ffffffff8000000012345678\"",
            "\"array_hex\":\"00000000ffffffff8000000012345679\"",
            1,
        ),
        FIXTURE.replacen("\"errno_after\":0", "\"errno_after\":1", 1),
    ] {
        assert!(std::panic::catch_unwind(|| fixture::validated_records(&corrupt)).is_err());
    }
    for corrupt in [
        FIXTURE.replacen(
            "\"return_hex\":\"0000000000000000\"",
            "\"return_hex\":\"0000000000000001\"",
            1,
        ),
        FIXTURE.replacen("\"cursor_after\":0", "\"cursor_after\":1", 1),
        FIXTURE.replacen("\"index_after\":0", "\"index_after\":1", 1),
    ] {
        assert!(std::panic::catch_unwind(|| compare(&corrupt)).is_err());
    }
}
