use osmocore_rs::bitvec::*;
use osmocore_rs::compat::bitvec::bitvec_tailroom_bits;

use crate::bitvec_fixture as fixture;
const FIXTURE: &str = include_str!("../compatibility/vectors/bitvec_vectors.json");
const VALUES: [BitValue; 4] = [BitValue::Zero, BitValue::One, BitValue::L, BitValue::H];
const SEQUENCE: [BitValue; 6] = [
    BitValue::Zero,
    BitValue::One,
    BitValue::L,
    BitValue::H,
    BitValue::One,
    BitValue::Zero,
];

#[test]
fn bitvec_matches_c_bytes_returns_and_cursor_state() {
    for record in fixture::validated_records(FIXTURE) {
        let case = &record["case"];
        let mut data = fixture::bytes(&record["before_hex"]);
        let len = fixture::number(&record, "data_len");
        let cursor = fixture::number(&record, "cursor_before");
        let position = fixture::number(&record, "position");
        let value = VALUES[fixture::number(&record, "value")];
        let count = fixture::number(&record, "count");
        let mut bv = BitVec::new(&mut data[..len]);
        bv.cur_bit = cursor;
        let result: Result<i64, BitVecError> = match record["api"].as_str() {
            "bitvec_get_bit_pos" => bv.get(position).map(|v| v as i64),
            "bitvec_get_bit_pos_high" => bv.get_padding_bit(position).map(|v| v as i64),
            "bitvec_set_bit_pos" => bv.set(position, value).map(|()| 0),
            "bitvec_set_bit" => bv.write_bit(value).map(|()| 0),
            "bitvec_get_bit_high" => bv.read_padding_bit().map(|v| v as i64),
            "bitvec_set_bits" => bv.write_bits(&SEQUENCE[..count]).map(|()| 0),
            "bitvec_zero" => {
                bv.zero();
                Ok(0)
            }
            "bitvec_used_bytes" => Ok(bv.used_bytes() as i64),
            "bitvec_tailroom_bits" => {
                let remaining = bv.remaining_bits();
                if cursor <= len * 8 {
                    assert_eq!(remaining, Ok(len * 8 - cursor));
                } else {
                    assert_eq!(remaining, Err(BitVecError::OutOfRange));
                }
                Ok(i64::from(bitvec_tailroom_bits(&bv)))
            }
            "bit_value_to_char" => Ok(bit_value_to_char(value) as i64),
            api => panic!("unexpected API {api}"),
        };
        let expected: i64 = record["return_code"].parse().unwrap();
        assert_eq!(
            result
                .map_err(|e| {
                    assert_eq!(e, BitVecError::OutOfRange);
                    -22
                })
                .unwrap_or_else(|rc| rc),
            expected,
            "{case} return"
        );
        assert_eq!(
            bv.cur_bit,
            fixture::number(&record, "cursor_after"),
            "{case} cursor"
        );
        assert_eq!(
            data,
            fixture::bytes(&record["after_hex"]),
            "{case} complete backing storage"
        );
    }
}

#[test]
fn bitvec_fixture_rejects_corrupt_inputs_and_provenance() {
    for corrupt in [
        FIXTURE.replacen("\"case\":0", "\"case\":1", 1),
        FIXTURE.replacen("\"position\":0", "\"position\":\"0\"", 1),
        FIXTURE.replacen("789ef706", "00000000", 1),
        FIXTURE.replacen(
            "\"before_hex\":\"00ff2ba5\"",
            "\"before_hex\":\"00ff2ba4\"",
            1,
        ),
        FIXTURE.replacen("\"api\":", "\"unexpected\":", 1),
    ] {
        assert!(std::panic::catch_unwind(|| fixture::validated_records(&corrupt)).is_err());
    }
}
