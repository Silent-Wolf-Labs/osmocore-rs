//! Shared strict schema and frozen C input manifest; generates no observations.
use std::collections::{BTreeMap, BTreeSet};

pub const COUNT: usize = 3_695;
pub type Record = BTreeMap<String, String>;
pub const STATES: &[&str] = &[
    "hex",
    "data",
    "tail",
    "len",
    "layers",
    "cb",
    "dst",
    "lchan",
    "headlen",
    "headroom",
    "tailroom",
    "invariant",
    "length",
    "data_return",
];
pub const HASHES: &[(&str, &str)] = &[
    (
        "source_sha256",
        "eaf5f2fe4f3cfa3cf44de33d5c47804990e120e5ab8f8d17e797bd08af437ee1",
    ),
    (
        "header_sha256",
        "26ab5bf78406a29d60d8079ae5516b553234b433aec100c958cdc50bb80f5f4a",
    ),
    (
        "test_sha256",
        "5eac93541a8d80107b5512100bbc9743fbc263c4145da73a36deb91d23cb4bf4",
    ),
    (
        "panic_source_sha256",
        "8686ec3d0283bba6a0d8e168662f24f7a290f88e703691619d1f7d285827eb18",
    ),
    (
        "panic_header_sha256",
        "f2f6466f274bd045200626ade442479706ece3c9c4ba75164628ae84b7cb37fd",
    ),
];

pub fn number(r: &Record, key: &str) -> i64 {
    r[key].parse().unwrap()
}
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
pub fn words(text: &str, count: usize) -> Vec<i64> {
    let values: Vec<_> = text
        .split('/')
        .map(|s| {
            let v: i64 = s.parse().unwrap();
            assert_eq!(v.to_string(), s);
            v
        })
        .collect();
    assert_eq!(values.len(), count);
    values
}

#[derive(Debug)]
struct Input {
    api: &'static str,
    arg: i64,
    scenario: usize,
    step: usize,
    cap: usize,
    data: usize,
    len: usize,
    meta: usize,
    allocated: bool,
}
fn inputs() -> Vec<Input> {
    let mut result = Vec::new();
    let mut scenario = 0;
    for cap in [0, 1, 2, 8, 16] {
        result.push(Input {
            api: "alloc",
            arg: 0,
            scenario,
            step: 0,
            cap,
            data: 0,
            len: 0,
            meta: 0,
            allocated: true,
        });
        scenario += 1;
        for data in 0..=cap {
            for len in 0..=cap - data {
                for meta in 0..3 {
                    for api in [
                        "put",
                        "get",
                        "push",
                        "pull",
                        "trim",
                        "reserve",
                        "reset",
                        "invariant",
                    ] {
                        if cap >= 8 && data != 0 && data != cap / 2 && data != cap {
                            continue;
                        }
                        if cap >= 8 && len != 0 && len != (cap - data) / 2 && len != cap - data {
                            continue;
                        }
                        for arg in 0..=cap + 1 {
                            if matches!(api, "reset" | "invariant") && arg != 0 {
                                continue;
                            }
                            if api == "reserve" && (len != 0 || arg > cap - data) {
                                continue;
                            }
                            if api == "trim" && arg > cap - data && arg <= cap {
                                continue;
                            }
                            result.push(Input {
                                api,
                                arg: arg as i64,
                                scenario,
                                step: 0,
                                cap,
                                data,
                                len,
                                meta,
                                allocated: false,
                            });
                            scenario += 1;
                        }
                        if api == "trim" {
                            result.push(Input {
                                api,
                                arg: -1,
                                scenario,
                                step: 0,
                                cap,
                                data,
                                len,
                                meta,
                                allocated: false,
                            });
                            scenario += 1;
                        }
                    }
                }
            }
        }
    }
    for meta in 3..35 {
        result.push(Input {
            api: "invariant",
            arg: 0,
            scenario,
            step: 0,
            cap: 16,
            data: 4,
            len: 4,
            meta,
            allocated: false,
        });
        scenario += 1;
    }
    for (step, (api, arg)) in [
        ("reserve", 4),
        ("put", 8),
        ("push", 2),
        ("pull", 3),
        ("get", 2),
        ("trim", 5),
        ("reset", 0),
        ("put", 16),
    ]
    .into_iter()
    .enumerate()
    {
        result.push(Input {
            api,
            arg,
            scenario,
            step,
            cap: 16,
            data: 0,
            len: 0,
            meta: 1,
            allocated: false,
        });
    }
    scenario += 1;
    for (step, (api, arg)) in [
        ("alloc", 0),
        ("put", 65535),
        ("put", 1),
        ("get", 65535),
        ("reserve", 65535),
        ("push", 65535),
        ("pull", 65535),
        ("reset", 0),
    ]
    .into_iter()
    .enumerate()
    {
        result.push(Input {
            api,
            arg,
            scenario,
            step,
            cap: 65535,
            data: 0,
            len: 0,
            meta: 0,
            allocated: true,
        });
    }
    assert_eq!(result.len(), COUNT);
    result
}
pub fn schema() -> (BTreeSet<String>, BTreeSet<String>) {
    let mut numeric: BTreeSet<String> = [
        "case",
        "arg",
        "scenario",
        "step",
        "capacity",
        "c_uint_bits",
        "c_ulong_bits",
        "c_pointer_bits",
        "panic",
        "return_code",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let mut strings: BTreeSet<String> = ["api", "libosmocore_commit"]
        .into_iter()
        .chain(HASHES.iter().map(|(k, _)| *k))
        .map(String::from)
        .collect();
    for prefix in ["before", "after"] {
        for state in STATES {
            let key = format!("{prefix}_{state}");
            if matches!(*state, "hex" | "layers" | "cb") {
                strings.insert(key);
            } else {
                numeric.insert(key);
            }
        }
    }
    (numeric, strings)
}
pub fn parse(line: &str, numeric: &BTreeSet<String>, strings: &BTreeSet<String>) -> Record {
    let body = line.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
    let mut r = Record::new();
    for field in body.split(',') {
        let (key, value) = field.split_once(':').unwrap();
        let key = key.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
        let value = if strings.contains(key) {
            let s = value.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(!s.contains(['"', '\\']) && !s.chars().any(char::is_control));
            s
        } else {
            assert!(numeric.contains(key), "unknown key {key}");
            assert_eq!(value.parse::<i64>().unwrap().to_string(), value);
            value
        };
        assert!(
            r.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate key"
        );
    }
    assert_eq!(
        r.keys().cloned().collect::<BTreeSet<_>>(),
        numeric.union(strings).cloned().collect()
    );
    r
}
pub fn state(r: &Record, prefix: &str, cap: i64) {
    let n = |key| number(r, &format!("{prefix}_{key}"));
    let data = n("data");
    let tail = n("tail");
    let len = n("len");
    assert!((0..=cap).contains(&data) && (data..=cap).contains(&tail));
    assert_eq!(len, tail - data);
    assert_eq!(n("length"), len);
    assert_eq!(n("data_return"), data);
    assert_eq!(n("headroom"), data);
    assert_eq!(n("tailroom"), cap - tail);
    assert_eq!(
        n("headlen"),
        i64::from((len as u32).wrapping_sub(cap as u32))
    );
    assert_eq!(bytes(&r[&format!("{prefix}_hex")]).len(), cap as usize);
    let layers = words(&r[&format!("{prefix}_layers")], 4);
    assert!(layers.iter().all(|o| (-1..=cap).contains(o)));
    let present: Vec<_> = layers.into_iter().filter(|o| *o >= 0).collect();
    assert_eq!(
        n("invariant"),
        i64::from(present.windows(2).all(|w| w[0] <= w[1]))
    );
    assert!(
        words(&r[&format!("{prefix}_cb")], 5)
            .iter()
            .all(|v| *v >= 0)
    );
    assert!((0..=1).contains(&n("dst")) && (0..=1).contains(&n("lchan")));
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
    let (numeric, strings) = schema();
    let manifest = inputs();
    let mut records: Vec<Record> = Vec::with_capacity(COUNT);
    for (i, (line, input)) in lines.iter().zip(manifest).enumerate() {
        let line = if i + 1 == COUNT {
            *line
        } else {
            line.strip_suffix(',').unwrap()
        };
        let r = parse(line, &numeric, &strings);
        for (key, value) in HASHES {
            assert_eq!(r[*key], *value);
        }
        assert_eq!(
            r["libosmocore_commit"],
            "950430e829a3dc1d162aa241bc0505745c5a7311"
        );
        for (key, value) in [
            ("c_uint_bits", 32),
            ("c_ulong_bits", 64),
            ("c_pointer_bits", 64),
        ] {
            assert_eq!(number(&r, key), value);
        }
        assert_eq!(number(&r, "case"), i as i64);
        assert_eq!(r["api"], input.api);
        assert_eq!(number(&r, "arg"), input.arg);
        assert_eq!(number(&r, "scenario"), input.scenario as i64);
        assert_eq!(number(&r, "step"), input.step as i64);
        assert_eq!(number(&r, "capacity"), input.cap as i64);
        state(&r, "before", input.cap as i64);
        state(&r, "after", input.cap as i64);
        if input.step > 0 {
            for key in STATES {
                assert_eq!(
                    r[&format!("before_{key}")],
                    records[i - 1][&format!("after_{key}")],
                    "sequence continuity"
                );
            }
        } else {
            assert_eq!(number(&r, "before_data"), input.data as i64);
            assert_eq!(number(&r, "before_len"), input.len as i64);
            let expected_bytes: Vec<_> = (0..input.cap)
                .map(|b| if input.allocated { 0 } else { 0xa5 ^ b as u8 })
                .collect();
            assert_eq!(bytes(&r["before_hex"]), expected_bytes);
            let c = input.cap as i64;
            let mut layers = match input.meta {
                0 => vec![-1; 4],
                1 => vec![c / 3, c / 2, c * 2 / 3, c],
                _ => vec![c, c / 2, c * 2 / 3, 0],
            };
            if input.meta >= 3 {
                if input.meta < 19 {
                    layers = vec![c / 3, c / 2, c * 2 / 3, c];
                }
                let mask = (input.meta - 3) % 16;
                for (i, layer) in layers.iter_mut().enumerate() {
                    if mask & (1 << i) == 0 {
                        *layer = -1;
                    }
                }
            }
            assert_eq!(words(&r["before_layers"], 4), layers);
            assert_eq!(
                words(&r["before_cb"], 5),
                if input.meta == 0 {
                    vec![0; 5]
                } else {
                    vec![1, 2, 3, 4, 5]
                }
            );
            assert_eq!(number(&r, "before_dst"), i64::from(input.meta != 0));
            assert_eq!(number(&r, "before_lchan"), i64::from(input.meta != 0));
        }
        // These primitives alter offsets and metadata, never the backing bytes.
        assert_eq!(r["before_hex"], r["after_hex"]);
        let panic = number(&r, "panic");
        let rc = number(&r, "return_code");
        assert!((0..=1).contains(&panic));
        let expected_panic = match input.api {
            "put" => input.arg > number(&r, "before_tailroom"),
            "get" | "pull" => input.arg > number(&r, "before_len"),
            "push" => input.arg > number(&r, "before_headroom"),
            "trim" => input.arg < 0,
            _ => false,
        };
        assert_eq!(panic, i64::from(expected_panic), "panic coverage");
        if input.api == "trim" && !expected_panic {
            assert_eq!(rc, if input.arg > input.cap as i64 { -1 } else { 0 });
        }

        if panic == 1 || (input.api == "trim" && rc == -1) {
            for key in STATES {
                assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
            }
            assert_eq!(rc, -1);
        } else {
            match input.api {
                "put" => assert_eq!(rc, number(&r, "before_tail")),
                "get" => assert_eq!(rc, number(&r, "after_tail")),
                "push" | "pull" => assert_eq!(rc, number(&r, "after_data")),
                "trim" => assert_eq!(rc, 0),
                _ => assert_eq!(rc, -1),
            }
        }
        if input.api == "reset" {
            assert_eq!(number(&r, "after_data"), 0);
            assert_eq!(number(&r, "after_len"), 0);
            assert_eq!(
                words(&r["after_layers"], 4),
                vec![words(&r["before_layers"], 4)[0], -1, -1, -1]
            );
            assert_eq!(r["after_cb"], "0/0/0/0/0");
            assert_eq!(number(&r, "after_dst"), 0);
            assert_eq!(number(&r, "after_lchan"), 0);
        } else {
            for key in ["layers", "cb", "dst", "lchan"] {
                assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
            }
        }
        records.push(r);
    }
    records
}

/// Both repositories exercise the same corruption contract.
#[cfg(test)]
pub fn check_corruptions(fixture: &str) {
    let replacements = [
        (
            "950430e829a3dc1d162aa241bc0505745c5a7311",
            "0000000000000000000000000000000000000000",
        ),
        (HASHES[0].1, HASHES[1].1),
        ("\"api\":\"alloc\"", "\"api\":\"unknown\""),
        ("\"case\":0", "\"case\":1"),
        ("\"arg\":0", "\"arg\":1"),
        ("\"c_ulong_bits\":64", "\"c_ulong_bits\":32"),
        ("\"capacity\":0", "\"capacity\":1"),
        ("\"before_len\":0", "\"before_len\":1"),
        ("\"after_tail\":0", "\"after_tail\":1"),
        ("\"after_hex\":\"\"", "\"after_hex\":\"aa\""),
        (
            "\"after_layers\":\"-1/-1/-1/-1\"",
            "\"after_layers\":\"-2/-1/-1/-1\"",
        ),
        ("\"after_cb\":\"0/0/0/0/0\"", "\"after_cb\":\"-1/0/0/0/0\""),
        ("\"panic\":0", "\"panic\":1"),
        ("\"return_code\":-1", "\"return_code\":99999"),
        ("\"case\":0", "\"case\":0,\"unknown\":0"),
        ("\"case\":0", "\"case\":0,\"case\":0"),
        ("\"case\":0,", ""),
    ];
    for (from, to) in replacements {
        assert!(fixture.contains(from));
        let corrupt = fixture.replacen(from, to, 1);
        assert!(
            std::panic::catch_unwind(|| validated_records(&corrupt)).is_err(),
            "accepted corruption {from}"
        );
    }
    let missing = fixture.replacen(fixture.lines().nth(1).unwrap(), "", 1);
    assert!(std::panic::catch_unwind(|| validated_records(&missing)).is_err());
}
