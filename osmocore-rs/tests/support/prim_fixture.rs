//! Shared strict fixture schema, frozen input coverage and direct-C evidence.
//! This validator generates no expected observations.
use std::collections::{BTreeMap, BTreeSet};
pub type Record = BTreeMap<String, String>;
pub const INIT_COUNT: usize = 2_112;
pub const EVENT_COUNT: usize = 4_752;
pub const COUNT: usize = INIT_COUNT + EVENT_COUNT + 1;
pub const SAPS: &[u32] = &[0, 1, 0x01000000, 0x02000000, 0x03000000, u32::MAX];
pub const PRIMITIVES: &[u32] = &[0, 1, 255, 256, 0xffffff, 0x1000000, 0x80000000, u32::MAX];
pub const OPERATIONS: &[u32] = &[
    0,
    1,
    2,
    3,
    4,
    255,
    256,
    257,
    0x7fffffff,
    0x80000000,
    u32::MAX,
];
pub fn number(r: &Record, key: &str) -> u32 {
    r[key].parse().unwrap()
}
fn parse(line: &str, numbers: &[&str], strings: &[&str]) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut record = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').unwrap();
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        let value = if strings.contains(&key) {
            let value = value.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(!value.contains(['"', '\\']) && !value.chars().any(char::is_control));
            value
        } else {
            assert!(numbers.contains(&key), "unknown field {key}");
            let number: i64 = value.parse().unwrap();
            assert_eq!(number.to_string(), value);
            assert!((-23..=i64::from(u32::MAX)).contains(&number));
            value
        };
        assert!(
            record.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate {key}"
        );
    }
    let expected: BTreeSet<_> = numbers.iter().chain(strings.iter()).copied().collect();
    assert_eq!(
        record.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        expected
    );
    record
}
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
pub fn maps(sap: u32, primitive: u32, operation: u32, scenario: u32) -> Vec<[u32; 4]> {
    let exact = [sap, primitive, operation, 17];
    let sentinel = [0, 0, 0, u32::MAX];
    let mut map = vec![exact, sentinel];
    match scenario {
        0 => map = vec![[sap, primitive, operation, u32::MAX]],
        1 => {}
        2 => map[0][0] ^= 1,
        3 => map[0][1] ^= 1,
        4 => map[0][2] ^= 256,
        5 => map = vec![exact, [sap, primitive, operation, 23], sentinel],
        6 => map = vec![[sap, primitive, operation, u32::MAX], exact, sentinel],
        7 => {
            map = vec![
                [sap ^ 1, primitive, operation, 17],
                [sap, primitive, operation, 0],
                sentinel,
            ]
        }
        8 => map[0][3] = u32::MAX - 1,
        _ => panic!("scenario"),
    }
    map
}
fn map_text(map: &[[u32; 4]]) -> String {
    map.iter()
        .map(|r| format!("{}/{}/{}/{}", r[0], r[1], r[2], r[3]))
        .collect::<Vec<_>>()
        .join(";")
}
pub fn validated_records(fixture: &str) -> Vec<Record> {
    assert!(fixture.ends_with('\n'));
    let mut lines = fixture.lines();
    let meta = parse(
        lines.next().unwrap(),
        &[
            "uint_bits",
            "enum_bits",
            "ulong_bits",
            "pointer_bits",
            "enum_unsigned",
            "sap_shift",
            "gsm_base",
            "tetra_base",
            "ss7_base",
            "no_event",
        ],
        &[
            "kind",
            "revision",
            "prim_source",
            "prim_header",
            "utils_source",
            "utils_header",
            "msgb_source",
            "msgb_header",
        ],
    );
    assert_eq!(meta["kind"], "metadata");
    assert_eq!(meta["revision"], "950430e829a3dc1d162aa241bc0505745c5a7311");
    assert_eq!(
        meta["prim_source"],
        "40359a64f0963d34e344482dc5d6f68c9229eb44c93318a04ac517fe41e85d35"
    );
    assert_eq!(
        meta["prim_header"],
        "4c1f9789777d22e5d646ab9ae515571eedd1c7b7487a491d897e2be982a3da58"
    );
    assert_eq!(
        meta["utils_source"],
        "a5d9d5dc5e4ba23a746257d4b751a6d2dc721a245442be8161d37ceb952fdce4"
    );
    assert_eq!(
        meta["utils_header"],
        "3a66898c8a085693541404cf47a5d18588432fceae668609edcc0ae24219c70d"
    );
    assert_eq!(
        meta["msgb_source"],
        "eaf5f2fe4f3cfa3cf44de33d5c47804990e120e5ab8f8d17e797bd08af437ee1"
    );
    assert_eq!(
        meta["msgb_header"],
        "26ab5bf78406a29d60d8079ae5516b553234b433aec100c958cdc50bb80f5f4a"
    );
    assert_eq!(meta["uint_bits"], "32");
    assert_eq!(meta["enum_bits"], "32");
    assert_eq!(meta["ulong_bits"], "64");
    assert_eq!(meta["pointer_bits"], "64");
    assert_eq!(meta["enum_unsigned"], "1");
    assert_eq!(meta["sap_shift"], "24");
    assert_eq!(meta["gsm_base"], "16777216");
    assert_eq!(meta["tetra_base"], "33554432");
    assert_eq!(meta["ss7_base"], "50331648");
    assert_eq!(meta["no_event"], "4294967295");
    let mut records = Vec::new();
    for &sap in SAPS {
        for &primitive in PRIMITIVES {
            for &operation in OPERATIONS {
                for attached in 0..2 {
                    for phase in 0..2 {
                        let record = parse(
                            lines.next().unwrap(),
                            &[
                                "case",
                                "sap",
                                "primitive",
                                "operation",
                                "attached",
                                "phase",
                                "sap_out",
                                "primitive_out",
                                "operation_out",
                                "association",
                                "packed",
                                "header_packed",
                            ],
                            &["kind", "name", "before", "after"],
                        );
                        assert_eq!(record["kind"], "init");
                        assert_eq!(number(&record, "case"), records.len() as u32);
                        let mask = if phase == 0 { 0 } else { u32::MAX };
                        for (key, value) in [
                            ("sap", sap ^ mask),
                            ("primitive", primitive ^ mask),
                            ("operation", operation ^ mask),
                            ("attached", attached ^ phase),
                            ("phase", phase),
                        ] {
                            assert_eq!(number(&record, key), value);
                        }
                        records.push(record);
                    }
                }
            }
        }
    }
    for &sap in SAPS {
        for &primitive in PRIMITIVES {
            for &operation in OPERATIONS {
                for scenario in 0..9 {
                    let record = parse(
                        lines.next().unwrap(),
                        &["case", "sap", "primitive", "operation", "scenario", "event"],
                        &["kind", "map", "before", "after"],
                    );
                    assert_eq!(record["kind"], "event");
                    assert_eq!(number(&record, "case"), records.len() as u32);
                    for (key, value) in [
                        ("sap", sap),
                        ("primitive", primitive),
                        ("operation", operation),
                        ("scenario", scenario),
                    ] {
                        assert_eq!(number(&record, key), value);
                    }
                    assert_eq!(
                        record["map"],
                        map_text(&maps(sap, primitive, operation, scenario))
                    );
                    records.push(record);
                }
            }
        }
    }
    let callback = parse(
        lines.next().unwrap(),
        &[
            "case",
            "return_code",
            "context",
            "sap_out",
            "primitive_out",
            "operation_out",
            "association",
        ],
        &["kind", "before", "after"],
    );
    assert_eq!(callback["kind"], "callback");
    assert_eq!(number(&callback, "case"), records.len() as u32);
    records.push(callback);
    assert_eq!(records.len(), COUNT);
    assert!(lines.next().is_none());
    assert_eq!(fingerprint(fixture), 0x720c5d7e5075cdb8);
    records
}
#[cfg(test)]
pub fn check_corruptions(fixture: &str) {
    for (from, to) in [
        ("\"case\":0", "\"case\":1"),
        ("\"case\":0", "\"case\":0,\"case\":0"),
        ("\"case\":0", "\"case\":0,\"unexpected\":0"),
        ("\"case\":0,", ""),
        ("\"uint_bits\":32", "\"uint_bits\":64"),
        ("\"enum_unsigned\":1", "\"enum_unsigned\":0"),
        (
            "950430e829a3dc1d162aa241bc0505745c5a7311",
            "0000000000000000000000000000000000000000",
        ),
        (
            "40359a64f0963d34e344482dc5d6f68c9229eb44c93318a04ac517fe41e85d35",
            "invalid",
        ),
        ("\"kind\":\"init\"", "\"kind\":\"bad\""),
        ("\"sap\":0", "\"sap\":4294967296"),
        ("\"sap\":0", "\"sap\":00"),
        ("\"association\":1", "\"association\":0"),
        ("\"packed\":0", "\"packed\":1"),
        ("\"name\":\"request\"", "\"name\":\"REQUEST\""),
        ("\"event\":17", "\"event\":23"),
        ("\"map\":\"0/0/0/17", "\"map\":\"1/0/0/17"),
        ("\"after\":\"a5", "\"after\":\"a4"),
        ("\"return_code\":-23", "\"return_code\":0"),
    ] {
        assert!(fixture.contains(from), "missing corruption target {from}");
        assert!(
            std::panic::catch_unwind(|| validated_records(&fixture.replacen(from, to, 1))).is_err(),
            "accepted {from}"
        );
    }
    let line = fixture.lines().nth(1).unwrap();
    for changed in [
        fixture.replacen(&format!("{line}\n"), "", 1),
        format!("{fixture}{line}\n"),
        fixture.trim_end().to_owned(),
    ] {
        assert!(std::panic::catch_unwind(|| validated_records(&changed)).is_err());
    }
}
