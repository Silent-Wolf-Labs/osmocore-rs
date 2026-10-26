//! Shared CRC fixture schema, exact numeric parsing and frozen direct C evidence.
//! Generates input coverage only; expected CRC values come exclusively from C.
use std::collections::{BTreeMap, BTreeSet};
pub type Record = BTreeMap<String, String>;
pub const BYTE_COUNT: usize = 3_472;
pub const GENERIC_COUNT: usize = 22_848;
pub const STATES: &[u16] = &[0, 1, 0xffff, 0x5555, 0xaaaa];
pub const BYTE_LENGTHS: &[usize] = &[0, 1, 2, 7, 8, 16, 31, 32, 255, 256];
pub const LENGTHS: &[i32] = &[i32::MIN, -1, 0, 1, 7, 32];
pub fn number(r: &Record, key: &str) -> u64 {
    r[key].parse().unwrap()
}
pub fn signed(r: &Record, key: &str) -> i32 {
    r[key].parse().unwrap()
}
pub fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    assert!(
        text.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
fn parse(line: &str, unsigned: &[&str], signed: &[&str], strings: &[&str]) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut record = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').unwrap();
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        let value = if strings.contains(&key) {
            let value = value.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(!value.contains(['"', '\\']) && !value.chars().any(char::is_control));
            value
        } else if signed.contains(&key) {
            assert_eq!(value.parse::<i32>().unwrap().to_string(), value);
            value
        } else {
            assert!(unsigned.contains(&key), "unknown field {key}");
            assert_eq!(value.parse::<u64>().unwrap().to_string(), value);
            value
        };
        assert!(
            record.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate {key}"
        );
    }
    let keys: BTreeSet<_> = unsigned
        .iter()
        .chain(signed)
        .chain(strings)
        .copied()
        .collect();
    assert_eq!(
        record.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        keys
    );
    record
}
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
fn metadata(line: &str, stage: &str) {
    let meta = parse(
        line,
        &["int_bits"],
        &[],
        &["kind", "stage", "revision", "manifest"],
    );
    assert_eq!(meta["kind"], "metadata");
    assert_eq!(meta["stage"], stage);
    assert_eq!(meta["revision"], "950430e829a3dc1d162aa241bc0505745c5a7311");
    assert_eq!(number(&meta, "int_bits"), 32);
    assert_eq!(meta["manifest"], MANIFEST);
}
const MANIFEST: &str = "src/core/crc16.c=c59ec3c0353c18ab72903ff831829cc8775e102b33d250cf552fb59e35b127c3;include/osmocom/core/crc16.h=fc6fc2a4dba7d60bae4b5406186f2ef0f6b4626b3599d12b72953382b7bb2ce9;src/core/crcXXgen.c.tpl=003824c93a86def704dfe0d7e835b97b1590ca92b7e34d6eeaf30a0c470a6c59;include/osmocom/core/crcXXgen.h.tpl=1945d6ed8688dd366cfb715b8d57c24b309db21c5ac56d4787a7e9339d650326;include/osmocom/core/crcgen.h=0a2dbb97b3fafa7293b172cc35120228c6a278b26e440191859a0791a41af67b;include/osmocom/core/bits.h=922f568b46a84f36a2c6208308a71ceb505088b217e7105f254725f029c297f7;src/core/Makefile.am=1804d0b242711232b424a76c56692edd05025dfb6be7e996e528cad3a2da5db7;src/core/crc8gen.c=22e2a2967d8b2f7f0a3192429f7374d4ff85347d8f076879df90b3f249f3d7b3;include/osmocom/core/crc8gen.h=224db44da888a8c5b2a7de0f3291836f9352968f27960672d129d403e5fb0eb9;src/core/crc16gen.c=43bae71b01fb77bc6fe85b022278bb119d45f7abe7246aec54319788ab9e7137;include/osmocom/core/crc16gen.h=10c853bcab13db78b0ac70de66ebd20cd43abac51b88864dc51c036b7062d904;src/core/crc32gen.c=b38571941727b1aa9dd396d60cc448ac4a34e2562fe69cb6e1617f0335386f8c;include/osmocom/core/crc32gen.h=012a9893ef2527cd2e1224d49aa97140d180c997140c9e1d2720c7e097da2baf;src/core/crc64gen.c=53b2bd8694e13359490a7680475a8b5e82c21bf4fdcc073bc75cd3dfc74f9fe3;include/osmocom/core/crc64gen.h=66fb92b362c7e177099189477f20af50217bcf1775037178c58072ad87313720;include/osmocom/core/Makefile.am=3474af2d135857e1da1814fbae33aff8b24292d7d740041d4454cdf72c49e83e";
const BYTE_FINGERPRINT: u64 = 0x17f9ca55d5a4f2aa;
const GENERIC_FINGERPRINT: u64 = 0x7383f40e597de00d;

pub fn byte_input(mode: u64) -> Vec<u8> {
    (0..260)
        .map(|i| match mode {
            0 => 0,
            1 => 0xff,
            2 => {
                if i & 1 == 1 {
                    0x55
                } else {
                    0xaa
                }
            }
            3 => (i * 73 + 19) as u8,
            _ => panic!("mode"),
        })
        .collect()
}
pub fn generic_input(mode: u64) -> Vec<u8> {
    (0..36)
        .map(|i| match mode {
            0 => 0,
            1 => 1,
            2 => (i & 1) as u8,
            3 => [0, 1, 2, 3, 0x80, 0xff][i % 6],
            _ => panic!("mode"),
        })
        .collect()
}
pub fn validate_byte(fixture: &str) -> Vec<Record> {
    assert!(fixture.ends_with('\n'));
    assert_eq!(fingerprint(fixture), BYTE_FINGERPRINT);
    let mut lines = fixture.lines();
    metadata(lines.next().unwrap(), "byte");
    let mut records = Vec::new();
    for flavor in 0..2 {
        for byte in 0..256 {
            let r = parse(
                lines.next().unwrap(),
                &["case", "flavor", "byte", "result"],
                &[],
                &["kind"],
            );
            assert_eq!(r["kind"], "table");
            assert_eq!(number(&r, "case"), records.len() as u64);
            assert_eq!(number(&r, "flavor"), flavor);
            assert_eq!(number(&r, "byte"), byte);
            assert!(number(&r, "result") <= u64::from(u16::MAX));
            records.push(r);
        }
        for &init in STATES {
            for byte in 0..256 {
                let r = parse(
                    lines.next().unwrap(),
                    &["case", "flavor", "init", "byte", "result"],
                    &[],
                    &["kind"],
                );
                assert_eq!(r["kind"], "byte");
                assert_eq!(number(&r, "case"), records.len() as u64);
                for (key, value) in [
                    ("flavor", flavor),
                    ("init", u64::from(init)),
                    ("byte", byte),
                ] {
                    assert_eq!(number(&r, key), value);
                }
                assert!(number(&r, "result") <= u64::from(u16::MAX));
                records.push(r);
            }
        }
        for &init in STATES {
            for mode in 0..4 {
                for &len in BYTE_LENGTHS {
                    let r = parse(
                        lines.next().unwrap(),
                        &[
                            "case",
                            "flavor",
                            "init",
                            "mode",
                            "len",
                            "split",
                            "result",
                            "mid",
                            "continuation",
                        ],
                        &[],
                        &["kind", "before", "after"],
                    );
                    assert_eq!(r["kind"], "buffer");
                    assert_eq!(number(&r, "case"), records.len() as u64);
                    for (key, value) in [
                        ("flavor", flavor),
                        ("init", u64::from(init)),
                        ("mode", mode),
                        ("len", len as u64),
                        ("split", (len / 2) as u64),
                    ] {
                        assert_eq!(number(&r, key), value);
                    }
                    assert_eq!(bytes(&r["before"]), byte_input(mode));
                    assert_eq!(r["before"], r["after"]);
                    for key in ["result", "mid", "continuation"] {
                        assert!(number(&r, key) <= u64::from(u16::MAX));
                    }
                    records.push(r);
                }
            }
        }
    }
    assert!(lines.next().is_none());
    assert_eq!(records.len(), BYTE_COUNT);
    records
}
#[derive(Debug)]
pub struct GenericCase {
    pub width: u32,
    pub bits: i32,
    pub variant: u64,
    pub mode: u64,
    pub len: i32,
    pub capacity: usize,
    pub poly: u64,
    pub init: u64,
    pub remainder: u64,
}
pub fn generic_cases() -> Vec<GenericCase> {
    let mut cases = Vec::new();
    for width in [8, 16, 32, 64] {
        let storage = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        for bits in 1..=width {
            for variant in 0..4 {
                for mode in 0..4 {
                    for &len in LENGTHS {
                        for suffix in 0..2 {
                            if width >= 32 && bits == width && len > 0 {
                                continue;
                            }
                            let poly = match variant {
                                0 => 0,
                                1 => 3,
                                2 => storage,
                                _ => 0xaaaaaaaaaaaaaaaa & storage,
                            };
                            let init = match variant {
                                0 | 1 => variant,
                                2 => storage,
                                _ => 0x5555555555555555 & storage,
                            };
                            let remainder = match variant {
                                0 => 0,
                                2 => 1,
                                _ => storage,
                            };
                            cases.push(GenericCase {
                                width,
                                bits: bits as i32,
                                variant,
                                mode,
                                len,
                                capacity: (bits + if suffix == 1 { 3 } else { 0 }) as usize,
                                poly,
                                init,
                                remainder,
                            });
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), GENERIC_COUNT);
    cases
}
pub fn validate_generic(fixture: &str) -> Vec<Record> {
    assert!(fixture.ends_with('\n'));
    assert_eq!(fingerprint(fixture), GENERIC_FINGERPRINT);
    let mut lines = fixture.lines();
    metadata(lines.next().unwrap(), "generic");
    let mut records = Vec::new();
    for case in generic_cases() {
        let r = parse(
            lines.next().unwrap(),
            &[
                "case",
                "width",
                "bits",
                "variant",
                "mode",
                "capacity",
                "poly",
                "init",
                "remainder",
                "result",
                "match",
            ],
            &["len"],
            &[
                "kind",
                "input_before",
                "input_after",
                "before",
                "after",
                "flips",
                "nonbinary",
            ],
        );
        assert_eq!(r["kind"], "generic");
        assert_eq!(number(&r, "case"), records.len() as u64);
        for (key, value) in [
            ("width", u64::from(case.width)),
            ("bits", case.bits as u64),
            ("variant", case.variant),
            ("mode", case.mode),
            ("capacity", case.capacity as u64),
            ("poly", case.poly),
            ("init", case.init),
            ("remainder", case.remainder),
        ] {
            assert_eq!(number(&r, key), value);
        }
        assert_eq!(signed(&r, "len"), case.len);
        let storage = if case.width == 64 {
            u64::MAX
        } else {
            (1u64 << case.width) - 1
        };
        assert!(number(&r, "result") <= storage);
        assert_eq!(bytes(&r["input_before"]), generic_input(case.mode));
        assert_eq!(r["input_before"], r["input_after"]);
        assert_eq!(bytes(&r["before"]), vec![0x7e; case.capacity]);
        let after = bytes(&r["after"]);
        assert_eq!(after.len(), case.capacity);
        assert!(after[..case.bits as usize].iter().all(|b| *b <= 1));
        assert_eq!(
            &after[case.bits as usize..],
            &vec![0x7e; case.capacity - case.bits as usize]
        );
        assert_eq!(number(&r, "match"), 0);
        for key in ["flips", "nonbinary"] {
            assert_eq!(bytes(&r[key]), vec![1; case.bits as usize]);
        }
        records.push(r);
    }
    assert!(lines.next().is_none());
    assert_eq!(records.len(), GENERIC_COUNT);
    records
}
#[cfg(test)]
pub fn check_corruptions(fixture: &str, generic: bool) {
    let validate = |text: &str| {
        if generic {
            validate_generic(text);
        } else {
            validate_byte(text);
        }
    };
    let mut changes = vec![
        ("\"case\":0", "\"case\":1"),
        ("\"case\":0", "\"case\":0,\"case\":0"),
        ("\"case\":0", "\"case\":0,\"unknown\":0"),
        ("\"case\":0,", ""),
        ("\"int_bits\":32", "\"int_bits\":64"),
        (
            "950430e829a3dc1d162aa241bc0505745c5a7311",
            "0000000000000000000000000000000000000000",
        ),
        ("\"result\":0", "\"result\":1"),
        ("\"manifest\":\"", "\"manifest\":\"bad"),
    ];
    if generic {
        changes.extend([
            ("\"len\":-2147483648", "\"len\":-2147483649"),
            ("18446744073709551615", "18446744073709551616"),
            ("\"match\":0", "\"match\":1"),
            ("\"flips\":\"01", "\"flips\":\"00"),
            ("\"nonbinary\":\"01", "\"nonbinary\":\"00"),
            ("\"after\":\"00", "\"after\":\"02"),
            ("\"bits\":1", "\"bits\":0"),
        ]);
    } else {
        changes.extend([
            ("\"byte\":0", "\"byte\":256"),
            ("\"mid\":0", "\"mid\":1"),
            ("\"after\":\"00", "\"after\":\"01"),
        ]);
    }
    for (from, to) in changes {
        assert!(fixture.contains(from), "missing corruption target {from}");
        assert!(
            std::panic::catch_unwind(|| validate(&fixture.replacen(from, to, 1))).is_err(),
            "accepted {from}"
        );
    }
    let line = fixture.lines().nth(1).unwrap();
    for changed in [
        fixture.replacen(&format!("{line}\n"), "", 1),
        format!("{fixture}{line}\n"),
        fixture.trim_end().to_owned(),
    ] {
        assert!(std::panic::catch_unwind(|| validate(&changed)).is_err());
    }
    // Exercise the schema/parser itself, independently of the frozen fingerprint.
    for text in [
        r#"{"value":18446744073709551616,"len":0,"hex":"00"}"#,
        r#"{"value":-1,"len":0,"hex":"00"}"#,
        r#"{"value":00,"len":0,"hex":"00"}"#,
        r#"{"value":0,"len":2147483648,"hex":"00"}"#,
        r#"{"value":0,"len":0,"hex":"00","extra":0}"#,
        r#"{"value":0,"value":0,"len":0,"hex":"00"}"#,
        r#"{"value":0,"len":0}"#,
    ] {
        assert!(std::panic::catch_unwind(|| parse(text, &["value"], &["len"], &["hex"])).is_err());
    }
    let r = parse(
        r#"{"value":18446744073709551615,"len":-2147483648,"hex":"00"}"#,
        &["value"],
        &["len"],
        &["hex"],
    );
    assert_eq!(number(&r, "value"), u64::MAX);
    assert_eq!(signed(&r, "len"), i32::MIN);
    for hex in ["0", "0g", "FF"] {
        assert!(std::panic::catch_unwind(|| bytes(hex)).is_err());
    }
}
