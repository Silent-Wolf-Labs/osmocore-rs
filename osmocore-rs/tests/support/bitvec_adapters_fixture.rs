//! Frozen adapter inputs and strict C fixture schema. No outcomes generated here.
use crate::bitvec_fixture as primitive;
pub use primitive::{Record, number};
pub const COUNT: usize = 6_757;
const SEEDS: [u8; 8] = [0, 0xff, 0x2b, 0x80, 0xfe, 0x7f, 0x55, 0xa5];
const TEXTS: &[(&str, &[u8])] = &[
    ("empty", b"\0"),
    ("whitespace", b"\t \n\r\0"),
    ("lower", b"a1b2\0"),
    ("upper", b"A1B2\0"),
    ("mixed", b"aB01Cf\0"),
    ("spaced", b"a1 b2\tc3\r\n\0"),
    ("odd_first", b"a\0"),
    ("odd_prefix", b"12a\0"),
    ("invalid_first", b"x\0"),
    ("invalid_prefix", b"12x\0"),
    ("invalid_half", b"1x\0"),
    ("embedded_nul", b"12\0ffff\0"),
    ("high_bit", b"\x80\0"),
    ("high_after", b"12\x80\0"),
    ("eight_bytes", b"0001020304050607\0"),
    ("nine_bytes", b"000102030405060708\0"),
    ("sixteen_bytes", b"000102030405060708090a0b0c0d0e0f\0"),
    ("trailing_spaces", b"12 \t\n\r\0"),
    ("full_with_spaces", b"0001020304050607 \t\0"),
];
const NUMERIC: &[&str] = &[
    "case",
    "c_unsigned_bits",
    "data_len",
    "pattern",
    "cursor_before",
    "io_len",
    "return_code",
    "cursor_after",
];
const STRINGS: &[&str] = &[
    "api",
    "libosmocore_commit",
    "source_sha256",
    "header_sha256",
    "test_sha256",
    "hex_source_sha256",
    "hex_header_sha256",
    "hex_test_sha256",
    "input",
    "src_hex",
    "before_hex",
    "after_hex",
    "io_before_hex",
    "io_after_hex",
];

pub fn bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

#[derive(Debug)]
struct Input {
    api: &'static str,
    len: usize,
    pattern: usize,
    cursor: usize,
    io_len: usize,
    name: String,
    src: Vec<u8>,
}

fn expected_inputs() -> Vec<Input> {
    let mut cases = Vec::with_capacity(COUNT);
    for len in 0..=8 {
        for pattern in 0..3 {
            for cursor in [0, len * 8, u32::MAX as usize] {
                for extra in [0, 2] {
                    for api in ["bitvec_pack", "bitvec_unpack"] {
                        cases.push(Input {
                            api,
                            len,
                            pattern,
                            cursor,
                            io_len: len + extra,
                            name: "none".into(),
                            src: vec![],
                        });
                    }
                }
            }
        }
    }
    for len in [0, 1, 2, 4, 8] {
        for pattern in 0..3 {
            for cursor in [0, 1, 7, len * 8, u32::MAX as usize] {
                for &(name, src) in TEXTS {
                    cases.push(Input {
                        api: "bitvec_unhex",
                        len,
                        pattern,
                        cursor,
                        io_len: 0,
                        name: name.into(),
                        src: src.to_vec(),
                    });
                }
            }
        }
    }
    for byte in 0..=255u8 {
        cases.push(Input {
            api: "bitvec_unhex",
            len: 2,
            pattern: 2,
            cursor: 7,
            io_len: 0,
            name: format!("byte_{byte:02x}"),
            src: vec![byte, 0],
        });
    }
    for len in 0..=8 {
        for pattern in 0..8 {
            for cursor in 0..=len * 8 {
                let required = cursor + cursor / 8 + usize::from(cursor % 8 != 0) + 1;
                for extra in [0, 3] {
                    cases.push(Input {
                        api: "bitvec_to_string_r",
                        len,
                        pattern,
                        cursor,
                        io_len: required + extra,
                        name: "none".into(),
                        src: vec![],
                    });
                }
            }
        }
    }
    assert_eq!(cases.len(), COUNT);
    cases
}

pub fn validated_records(fixture: &str) -> Vec<Record> {
    let body = fixture
        .trim()
        .strip_prefix('[')
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let lines: Vec<_> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), COUNT);
    let expected = expected_inputs();
    let mut records = Vec::with_capacity(COUNT);
    let mut initial_io: Vec<u8> = (0..80).map(|i| 0xd3 ^ i).collect();
    initial_io[0] = 0x5a;
    initial_io[79] = 0xa5;
    for (i, line) in lines.iter().enumerate() {
        let line = if i + 1 == COUNT {
            *line
        } else {
            line.strip_suffix(',').unwrap()
        };
        let record = primitive::parse_record(line, NUMERIC, STRINGS);
        assert_eq!(number(&record, "case"), i);
        assert_eq!(
            record["libosmocore_commit"],
            "950430e829a3dc1d162aa241bc0505745c5a7311"
        );
        for (key, hash) in [
            (
                "source_sha256",
                "789ef706bc0c71f497400f831e920c9efad324dfaea99b13292fca717b4f480e",
            ),
            (
                "header_sha256",
                "db895f6eb8bfada1d6764248d122128b0a0bb530ec5f4a198953f56f223215b6",
            ),
            (
                "test_sha256",
                "5b4a1a3f8c5bafd4113f7b4634f3c80061c274356e05c328d270fb40dfb007f5",
            ),
            (
                "hex_source_sha256",
                "a5d9d5dc5e4ba23a746257d4b751a6d2dc721a245442be8161d37ceb952fdce4",
            ),
            (
                "hex_header_sha256",
                "3a66898c8a085693541404cf47a5d18588432fceae668609edcc0ae24219c70d",
            ),
            (
                "hex_test_sha256",
                "7666972f9755fc95c69b2740beb6292a9ca28d00dc7f4cc1f55b83c9615bc60d",
            ),
        ] {
            assert_eq!(record[key], hash);
        }
        assert_eq!(number(&record, "c_unsigned_bits"), 32);
        let input = &expected[i];
        assert_eq!(record["api"], input.api);
        assert_eq!(record["input"], input.name);
        assert_eq!(bytes(&record["src_hex"]), input.src);
        for (key, value) in [
            ("data_len", input.len),
            ("pattern", input.pattern),
            ("cursor_before", input.cursor),
            ("io_len", input.io_len),
        ] {
            assert_eq!(number(&record, key), value, "case {i}: {key}");
        }
        let seed = SEEDS[input.pattern];
        let before = bytes(&record["before_hex"]);
        let after = bytes(&record["after_hex"]);
        assert_eq!(before, [seed, seed, seed ^ 0xff, 0x2b, 0xa5, 0, 0xff, 0x80]);
        assert_eq!(after.len(), 8);
        assert_eq!(after[input.len..], before[input.len..]);
        let io_before = bytes(&record["io_before_hex"]);
        let io_after = bytes(&record["io_after_hex"]);
        assert_eq!(io_before, initial_io);
        assert_eq!(io_after.len(), 80);
        assert_eq!(io_after[0], io_before[0]);
        assert_eq!(io_after[1 + input.io_len..], io_before[1 + input.io_len..]);
        let rc = record["return_code"].parse::<i32>().unwrap();
        let cursor = number(&record, "cursor_after");
        assert!(u32::try_from(cursor).is_ok());
        match input.api {
            "bitvec_pack" => {
                assert_eq!(after, before);
                assert_eq!(rc, input.len as i32);
                assert_eq!(cursor, input.cursor);
            }
            "bitvec_unpack" => {
                assert_eq!(io_after, io_before);
                assert_eq!(rc, input.len as i32);
                assert_eq!(cursor, input.cursor);
            }
            "bitvec_unhex" => {
                assert_eq!(io_after, io_before);
                assert!([0, 1].contains(&rc));
                assert_eq!(input.src.last(), Some(&0));
                if rc == 1 {
                    assert_eq!(cursor, input.cursor);
                } else {
                    assert_eq!(cursor % 8, 0);
                    assert!(cursor <= input.len * 8);
                }
            }
            "bitvec_to_string_r" => {
                assert_eq!(after, before);
                assert_eq!(rc, 0);
                assert_eq!(cursor, input.cursor);
                let chars =
                    input.cursor + input.cursor / 8 + usize::from(!input.cursor.is_multiple_of(8));
                assert_eq!(io_after[1 + chars], 0);
                assert!(
                    io_after[1..1 + chars]
                        .iter()
                        .all(|b| matches!(b, b' ' | b'0' | b'1'))
                );
                assert_eq!(io_after[2 + chars..], io_before[2 + chars..]);
            }
            _ => unreachable!(),
        }
        records.push(record);
    }
    records
}
