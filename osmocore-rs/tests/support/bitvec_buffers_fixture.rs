//! Frozen input construction and schema for the second bit-vector stage.
//! Only C-generated fixtures supply outcomes.
use crate::bitvec_fixture as primitive;
pub use primitive::{Record, number};

pub const COUNT: usize = 38_837;
const WIDTHS: &[u32] = &[0, 1, 7, 8, 9, 15, 16, 31, 32, 33, 63, 64, 65, 255];
const WIDE_COUNTS: &[u32] = &[256, 257, 288, 320, 321, 511, u32::MAX];
const WORDS: [u64; 4] = [0, 1, 0x8000_0000_8000_0000, u64::MAX];
const NUMERIC: &[&str] = &[
    "case",
    "c_unsigned_bits",
    "data_len",
    "pattern",
    "cursor_before",
    "num_bits",
    "value",
    "mode",
    "return_code",
    "cursor_after",
];
const STRINGS: &[&str] = &[
    "api",
    "libosmocore_commit",
    "source_sha256",
    "header_sha256",
    "test_sha256",
    "word_hex",
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

#[derive(Debug, PartialEq)]
struct Input {
    api: &'static str,
    len: usize,
    pattern: usize,
    cursor: usize,
    n: u32,
    word: u64,
    value: usize,
    mode: usize,
}

fn initial(pattern: usize) -> [u8; 8] {
    let seed = [0, 0xff, 0x2b][pattern];
    [seed, seed ^ 0xff, 0x2b, 0xa5, 0xff, 0x80, 0x01, 0x55]
}

// This excludes C calls with undefined shifts; it constructs inputs only.
fn safe_uint_read(len: usize, pattern: usize, mut cursor: usize, n: u32) -> bool {
    let data = initial(pattern);
    for i in 0..n {
        if cursor / 8 >= len {
            return true;
        }
        if data[cursor / 8] & (1 << (7 - cursor % 8)) != 0 && n - i > 32 {
            return false;
        }
        cursor += 1;
    }
    true
}

fn expected_inputs() -> Vec<Input> {
    let mut cases = Vec::with_capacity(COUNT);
    for len in [0, 1, 2, 4, 8] {
        for pattern in 0..3 {
            for cursor in [0, 1, 7, 8, 15, len * 8, len * 8 + 1, u32::MAX as usize] {
                let mut push = |api, n, word, value, mode| {
                    cases.push(Input {
                        api,
                        len,
                        pattern,
                        cursor,
                        n,
                        word,
                        value,
                        mode,
                    })
                };
                for &n in WIDTHS {
                    for word in WORDS {
                        for mode in 0..2 {
                            push("bitvec_set_u64", n, word, 0, mode);
                        }
                        push("bitvec_set_uint", n, word, 0, 0);
                    }
                    if safe_uint_read(len, pattern, cursor, n) {
                        push("bitvec_get_uint", n, 0, 0, 0);
                    }
                    if n > 15 || cursor < n as usize || n == 0 || len >= (n as usize).div_ceil(8) {
                        push("bitvec_get_int16_msb", n, 0, 0, 0);
                    }
                    for value in 0..4 {
                        push("bitvec_fill", n, 0, value, 0);
                    }
                    push("bitvec_spare_padding", n, 0, 0, 0);
                }
                for &n in WIDE_COUNTS {
                    push("bitvec_set_uint", n, u64::MAX, 0, 0);
                    for value in 0..4 {
                        push("bitvec_fill", n, 0, value, 0);
                    }
                    push("bitvec_spare_padding", n, 0, 0, 0);
                }
                for n in 0..=4 {
                    push("bitvec_get_bytes", n, 0, 0, 0);
                    push("bitvec_set_bytes", n, 0, 0, 0);
                }
            }
        }
    }
    for pattern in 0..3 {
        for cursor in 0..32 {
            for n in 0..=4 {
                for api in ["bitvec_get_bytes", "bitvec_set_bytes"] {
                    cases.push(Input {
                        api,
                        len: 8,
                        pattern,
                        cursor,
                        n,
                        word: 0,
                        value: 0,
                        mode: 0,
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
        assert_eq!(
            record["source_sha256"],
            "789ef706bc0c71f497400f831e920c9efad324dfaea99b13292fca717b4f480e"
        );
        assert_eq!(
            record["header_sha256"],
            "db895f6eb8bfada1d6764248d122128b0a0bb530ec5f4a198953f56f223215b6"
        );
        assert_eq!(
            record["test_sha256"],
            "5b4a1a3f8c5bafd4113f7b4634f3c80061c274356e05c328d270fb40dfb007f5"
        );
        assert_eq!(number(&record, "c_unsigned_bits"), 32);
        let input = &expected[i];
        assert_eq!(record["api"], input.api);
        for (key, value) in [
            ("data_len", input.len),
            ("pattern", input.pattern),
            ("cursor_before", input.cursor),
            ("num_bits", input.n as usize),
            ("value", input.value),
            ("mode", input.mode),
        ] {
            assert_eq!(number(&record, key), value, "case {i}: {key}");
        }
        assert_eq!(record["word_hex"], format!("{:016x}", input.word));
        let before = bytes(&record["before_hex"]);
        let after = bytes(&record["after_hex"]);
        assert_eq!(before, initial(input.pattern));
        assert_eq!(after.len(), 8);
        assert_eq!(after[input.len..], before[input.len..]);
        assert_eq!(record["io_before_hex"], "a500ff2b805a");
        let io_after = bytes(&record["io_after_hex"]);
        assert_eq!(io_after.len(), 6);
        assert_eq!(io_after[0], 0xa5);
        if input.api == "bitvec_get_bytes" {
            assert_eq!(
                io_after[1 + input.n as usize..],
                bytes(&record["io_before_hex"])[1 + input.n as usize..]
            );
        } else {
            assert_eq!(record["io_after_hex"], record["io_before_hex"]);
        }
        record["return_code"]
            .parse::<i32>()
            .expect("C signed return");
        assert!(u32::try_from(number(&record, "cursor_after")).is_ok());
        records.push(record);
    }
    records
}
