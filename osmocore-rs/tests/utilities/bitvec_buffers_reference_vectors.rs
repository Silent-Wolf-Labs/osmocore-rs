use osmocore_rs::bitvec::*;

#[path = "../support/bitvec_buffers_fixture.rs"]
mod fixture;
const FIXTURE: &str = include_str!("../compatibility/vectors/bitvec_buffers_vectors.json");

fn compare(fixture_text: &str) {
    for record in fixture::validated_records(fixture_text) {
        let case = &record["case"];
        let mut data = fixture::bytes(&record["before_hex"]);
        let mut io = fixture::bytes(&record["io_before_hex"]);
        let len = fixture::number(&record, "data_len");
        let n = fixture::number(&record, "num_bits") as u32;
        let word = u64::from_str_radix(&record["word_hex"], 16).unwrap();
        let bit = [BitValue::Zero, BitValue::One, BitValue::L, BitValue::H]
            [fixture::number(&record, "value")];
        let mut bv = BitVec::new(&mut data[..len]);
        bv.cur_bit = fixture::number(&record, "cursor_before");
        let result: Result<i32, BitVecError> = match record["api"].as_str() {
            "bitvec_set_u64" => bv
                .write_u64(word, n as u8, fixture::number(&record, "mode") != 0)
                .map(|()| 0),
            "bitvec_set_uint" => bv.write_uint(word as u32, n).map(|()| 0),
            "bitvec_get_uint" => bv.read_uint(n),
            "bitvec_get_int16_msb" => bv.read_int16_msb(n).map(i32::from),
            "bitvec_fill" => bv.fill(n, bit).map(|()| 0),
            "bitvec_spare_padding" => bv.spare_padding(n).map(|()| 0),
            "bitvec_get_bytes" => bv.read_bytes(&mut io[1..1 + n as usize]).map(|()| 0),
            "bitvec_set_bytes" => bv.write_bytes(&io[1..1 + n as usize]).map(|()| 0),
            api => panic!("unexpected API {api}"),
        };
        let rc = result.unwrap_or_else(|error| match error {
            BitVecError::OutOfRange => -22,
            BitVecError::TooManyBits => -7,
            e => panic!("case {case}: unexpected {e:?}"),
        });
        assert_eq!(
            rc,
            record["return_code"].parse::<i32>().unwrap(),
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
            "{case} all vector bytes"
        );
        assert_eq!(
            io,
            fixture::bytes(&record["io_after_hex"]),
            "{case} all transfer bytes"
        );
    }
}

#[test]
fn integer_fill_padding_and_bytes_match_c() {
    compare(FIXTURE);
}

#[test]
fn rejects_corrupt_schema_provenance_and_observations() {
    for corrupt in [
        FIXTURE.replacen("\"case\":0", "\"case\":1", 1),
        FIXTURE.replacen("789ef706", "00000000", 1),
        FIXTURE.replacen("\"num_bits\":0", "\"num_bits\":\"0\"", 1),
        FIXTURE.replacen(
            "\"word_hex\":\"0000000000000000\"",
            "\"word_hex\":\"0000000000000001\"",
            1,
        ),
    ] {
        assert!(std::panic::catch_unwind(|| fixture::validated_records(&corrupt)).is_err());
    }
    // A validly typed but incorrect C outcome must also fail comparison.
    let corrupt = FIXTURE.replacen("\"return_code\":0", "\"return_code\":1", 1);
    assert!(std::panic::catch_unwind(|| compare(&corrupt)).is_err());
}
