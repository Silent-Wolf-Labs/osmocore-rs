//! Strict schema and frozen C input manifest for fields/search/runs/shifts/arrays.
//! This constructs inputs only; expected outcomes come from direct C calls.
use crate::bitvec_fixture as primitive;
pub use primitive::{Record, number};
pub const COUNT: usize = 38_643;
const WIDTHS: &[u32] = &[
    0,
    1,
    7,
    8,
    9,
    31,
    32,
    63,
    64,
    65,
    255,
    256,
    257,
    320,
    321,
    u32::MAX,
];
const SHIFTS: &[u32] = &[0, 1, 2, 7, 8, 9, 15, 16, 24, 31, 32, 64, 65, u32::MAX];
const ARRAY_WIDTHS: &[u32] = &[0, 1, 8, 32, 64, 65, 256, 257, u32::MAX];
const WORDS: [u64; 3] = [0, 0x8000_0000_0000_0001, u64::MAX];
const SEEDS: [u8; 8] = [0, 0xff, 0x2b, 0x80, 0xfe, 0x7f, 0x55, 0xa5];
const NUMERIC: &[&str] = &[
    "case",
    "c_unsigned_bits",
    "data_len",
    "pattern",
    "cursor_before",
    "index_before",
    "argument",
    "value",
    "array_len",
    "dry_run",
    "return_code",
    "errno_after",
    "index_after",
    "cursor_after",
];
const STRINGS: &[&str] = &[
    "api",
    "libosmocore_commit",
    "source_sha256",
    "header_sha256",
    "test_sha256",
    "word_hex",
    "array_hex",
    "before_hex",
    "return_hex",
    "after_hex",
];

pub fn bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len(), 16, "eight-byte backing array");
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    (0..16)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

pub fn word(hex: &str) -> u64 {
    assert_eq!(hex.len(), 16);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    u64::from_str_radix(hex, 16).unwrap()
}

fn initial(pattern: usize) -> [u8; 8] {
    let seed = SEEDS[pattern];
    [seed, seed, seed ^ 0xff, 0x2b, 0xa5, 0, 0xff, 0x80]
}

#[derive(Debug)]
struct Input {
    api: &'static str,
    len: usize,
    pattern: usize,
    cursor: u32,
    index: u32,
    arg: u32,
    word: u64,
    value: usize,
    array_len: usize,
    dry: usize,
}

// Safety-only filter for the C input manifest. Never generates outcomes.
fn safe_read(len: usize, pattern: usize, index: u32, n: u32) -> bool {
    if n != 0 && index.wrapping_add(n).wrapping_sub(1) as usize / 8 >= len {
        return true;
    }
    if n != 0 && u64::from(index) + u64::from(n) > len as u64 * 8 {
        return false;
    }
    let data = initial(pattern);
    for i in 0..n {
        let pos = index + i;
        if data[pos as usize / 8] & (1 << (7 - pos % 8)) != 0 && n - i > 64 {
            return false;
        }
    }
    true
}

fn expected_inputs() -> Vec<Input> {
    let mut cases = Vec::with_capacity(COUNT);
    for len in [0, 1, 2, 4, 8] {
        for pattern in 0..3 {
            for cursor in [3, u32::MAX] {
                for index in [0, 1, 7, 8, len as u32 * 8, len as u32 * 8 + 1, u32::MAX] {
                    for &arg in WIDTHS {
                        if safe_read(len, pattern, index, arg) {
                            cases.push(Input {
                                api: "bitvec_read_field",
                                len,
                                pattern,
                                cursor,
                                index,
                                arg,
                                word: 0,
                                value: 0,
                                array_len: 0,
                                dry: 0,
                            });
                        }
                        for word in WORDS {
                            cases.push(Input {
                                api: "bitvec_write_field",
                                len,
                                pattern,
                                cursor,
                                index,
                                arg,
                                word,
                                value: 0,
                                array_len: 0,
                                dry: 0,
                            });
                        }
                    }
                }
            }
        }
    }
    for len in [0, 1, 2, 3, 4, 8] {
        let cap = len as u32 * 8;
        for (pattern, &seed) in SEEDS.iter().enumerate() {
            for arg in [0, 1, 2, 7, 8, 9, 32, 64, 65, u32::MAX] {
                cases.push(Input {
                    api: "bitvec_get_nth_set_bit",
                    len,
                    pattern,
                    cursor: u32::MAX,
                    index: 0,
                    arg,
                    word: 0,
                    value: 0,
                    array_len: 0,
                    dry: 0,
                });
            }
            for arg in [0, 1, 7, 8, cap.saturating_sub(1), cap, cap + 1, u32::MAX] {
                for value in 0..4 {
                    cases.push(Input {
                        api: "bitvec_find_bit_pos",
                        len,
                        pattern,
                        cursor: u32::MAX,
                        index: 0,
                        arg,
                        word: 0,
                        value,
                        array_len: 0,
                        dry: 0,
                    });
                }
            }
            for ci in 0..=cap + 3 {
                let cursor = if ci == cap + 2 {
                    u32::MAX - 7
                } else if ci == cap + 3 {
                    u32::MAX
                } else {
                    ci
                };
                for value in 0..2 {
                    if cursor <= cap || (len != 0 && seed != if value != 0 { 0xff } else { 0 }) {
                        cases.push(Input {
                            api: "bitvec_rl",
                            len,
                            pattern,
                            cursor,
                            index: 0,
                            arg: 0,
                            word: 0,
                            value,
                            array_len: 0,
                            dry: 0,
                        });
                    }
                    for arg in [0, 1, 7, 8, cap] {
                        if arg <= cap {
                            cases.push(Input {
                                api: "bitvec_rl_curbit",
                                len,
                                pattern,
                                cursor,
                                index: 0,
                                arg,
                                word: 0,
                                value,
                                array_len: 0,
                                dry: 0,
                            });
                        }
                    }
                }
            }
            for cursor in [0, 1, 7, 8, cap, cap + 1, u32::MAX] {
                for &arg in SHIFTS {
                    if arg == 0 || arg >= cursor || (len >= 2 && arg as usize / 8 <= len) {
                        cases.push(Input {
                            api: "bitvec_shiftl",
                            len,
                            pattern,
                            cursor,
                            index: 0,
                            arg,
                            word: 0,
                            value: 0,
                            array_len: 0,
                            dry: 0,
                        });
                    }
                }
            }
        }
    }
    for len in [0, 1, 2, 4, 8] {
        for pattern in 0..3 {
            for cursor in [0, 7, len as u32 * 8, u32::MAX] {
                for array_len in [0, 1, 4] {
                    for &arg in ARRAY_WIDTHS {
                        for dry in 0..2 {
                            cases.push(Input {
                                api: "bitvec_add_array",
                                len,
                                pattern,
                                cursor,
                                index: 0,
                                arg,
                                word: 0,
                                value: 0,
                                array_len,
                                dry,
                            });
                        }
                    }
                }
            }
        }
    }
    cases.push(Input {
        api: "bitvec_rl_curbit",
        len: 1,
        pattern: 0,
        cursor: 9,
        index: 0,
        arg: 16,
        word: 0,
        value: 1,
        array_len: 0,
        dry: 0,
    });
    cases.push(Input {
        api: "bitvec_rl_curbit",
        len: 1,
        pattern: 0,
        cursor: 0,
        index: 0,
        arg: u32::MAX,
        word: 0,
        value: 1,
        array_len: 0,
        dry: 0,
    });
    cases.push(Input {
        api: "bitvec_rl_curbit",
        len: 1,
        pattern: 3,
        cursor: 0,
        index: 0,
        arg: u32::MAX,
        word: 0,
        value: 0,
        array_len: 0,
        dry: 0,
    });
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
        assert_eq!(record["api"], input.api, "case {i}");
        for (key, value) in [
            ("data_len", input.len),
            ("pattern", input.pattern),
            ("cursor_before", input.cursor as usize),
            ("index_before", input.index as usize),
            ("argument", input.arg as usize),
            ("value", input.value),
            ("array_len", input.array_len),
            ("dry_run", input.dry),
        ] {
            assert_eq!(number(&record, key), value, "case {i}: {key}");
        }
        assert_eq!(word(&record["word_hex"]), input.word);
        assert_eq!(record["array_hex"], "00000000ffffffff8000000012345678");
        let before = bytes(&record["before_hex"]);
        let after = bytes(&record["after_hex"]);
        assert_eq!(before, initial(input.pattern));
        assert_eq!(after[input.len..], before[input.len..]);
        let readonly = matches!(
            input.api,
            "bitvec_read_field"
                | "bitvec_get_nth_set_bit"
                | "bitvec_find_bit_pos"
                | "bitvec_rl"
                | "bitvec_rl_curbit"
        );
        if readonly || (input.api == "bitvec_add_array" && input.dry == 1) {
            assert_eq!(after, before);
        }
        let rc = record["return_code"].parse::<i64>().unwrap();
        assert!((-22..=i64::from(u32::MAX)).contains(&rc));
        let returned = word(&record["return_hex"]);
        let errno = number(&record, "errno_after");
        assert!([0, 4, 75].contains(&errno));
        if input.api == "bitvec_read_field" {
            assert_eq!(rc, 0);
            assert!([0, 75].contains(&errno));
            if errno == 75 {
                assert_eq!(returned, 0);
            }
        } else {
            assert_eq!(returned, 0);
            if input.api != "bitvec_rl_curbit" {
                assert_eq!(errno, 4);
            }
        }
        assert!(u32::try_from(number(&record, "cursor_after")).is_ok());
        assert!(u32::try_from(number(&record, "index_after")).is_ok());
        if !matches!(input.api, "bitvec_read_field" | "bitvec_write_field") {
            assert_eq!(number(&record, "index_after"), input.index as usize);
        }
        if matches!(
            input.api,
            "bitvec_get_nth_set_bit" | "bitvec_find_bit_pos" | "bitvec_rl"
        ) || (input.api == "bitvec_add_array" && input.dry == 1)
        {
            assert_eq!(number(&record, "cursor_after"), input.cursor as usize);
        }
        records.push(record);
    }
    records
}
