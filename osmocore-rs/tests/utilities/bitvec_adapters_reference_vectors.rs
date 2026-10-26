use osmocore_rs::bitvec::*;
use osmocore_rs::compat::bitvec::{bitvec_to_string_r, bitvec_unhex};
#[path = "../support/bitvec_adapters_fixture.rs"]
mod fixture;
const FIXTURE: &str = include_str!("../compatibility/vectors/bitvec_adapters_vectors.json");

fn compare(text: &str) {
    for record in fixture::validated_records(text) {
        let case = &record["case"];
        let mut data = fixture::bytes(&record["before_hex"]);
        let mut io = fixture::bytes(&record["io_before_hex"]);
        let len = fixture::number(&record, "data_len");
        let io_len = fixture::number(&record, "io_len");
        let mut bv = BitVec::new(&mut data[..len]);
        bv.cur_bit = fixture::number(&record, "cursor_before");
        let rc = match record["api"].as_str() {
            "bitvec_pack" => bv.pack(&mut io[1..1 + io_len]).unwrap() as i32,
            "bitvec_unpack" => bv.unpack(&io[1..1 + io_len]).unwrap() as i32,
            "bitvec_unhex" => bitvec_unhex(&mut bv, &fixture::bytes(&record["src_hex"])),
            "bitvec_to_string_r" => {
                bitvec_to_string_r(&bv, &mut io[1..1 + io_len]).unwrap();
                0
            }
            api => panic!("unexpected {api}"),
        };
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
            "{case} complete backing array"
        );
        assert_eq!(
            io,
            fixture::bytes(&record["io_after_hex"]),
            "{case} complete output/transfer array"
        );
    }
}

#[test]
fn copies_hex_and_formatting_match_c() {
    compare(FIXTURE);
}

#[test]
fn rejects_corrupt_inputs_provenance_and_outputs() {
    for corrupt in [
        FIXTURE.replacen("\"case\":0", "\"case\":1", 1),
        FIXTURE.replacen("a5d9d5dc", "00000000", 1),
        FIXTURE.replacen("\"io_len\":0", "\"io_len\":\"0\"", 1),
        FIXTURE.replacen("\"src_hex\":\"\"", "\"src_hex\":\"00\"", 1),
        FIXTURE.replacen("\"input\":\"none\"", "\"input\":\"unexpected\"", 1),
    ] {
        assert!(std::panic::catch_unwind(|| fixture::validated_records(&corrupt)).is_err());
    }
    // Corrupt an output bit while retaining valid schema, text length, and guards.
    let corrupt = FIXTURE.replacen(
        "\"io_after_hex\":\"5a203000",
        "\"io_after_hex\":\"5a203100",
        1,
    );
    assert_ne!(corrupt, FIXTURE, "fixture includes formatting output 0");
    assert!(std::panic::catch_unwind(|| compare(&corrupt)).is_err());
}
