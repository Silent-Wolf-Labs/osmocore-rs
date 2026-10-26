//! Shared frozen stage-2 inputs and strict evidence validation. No C outcomes generated.
use crate::msgb_fixture as primitive;
use primitive::{HASHES, STATES, parse, schema, state};
pub use primitive::{Record, bytes, number, words};

pub const COUNT: usize = 7_572;
const EXTRA_HASHES: &[(&str, &str)] = &[
    (
        "endian16_sha256",
        "927d65fa34c9cc59cf777fadabea4a7d370f934eebf85f24c0c25a540a8df895",
    ),
    (
        "endian32_sha256",
        "dac4631b3a38d3fba61af5c739cf1823d2a5cd2f460a511aeef04412601a4cf0",
    ),
    (
        "endian_template_sha256",
        "35109704fc81fd83d2a75072f1efe1df8fafbc1775c1a5114d2fb892441f02a1",
    ),
    (
        "endian_rule_sha256",
        "3474af2d135857e1da1814fbae33aff8b24292d7d740041d4454cdf72c49e83e",
    ),
    (
        "bits_header_sha256",
        "922f568b46a84f36a2c6208308a71ceb505088b217e7105f254725f029c297f7",
    ),
    (
        "utils_header_sha256",
        "3a66898c8a085693541404cf47a5d18588432fceae668609edcc0ae24219c70d",
    ),
];
const TYPED: &[&str] = &[
    "put_u8", "put_u16", "put_u32", "push_u8", "push_u16", "push_u32", "get_u8", "get_u16",
    "get_u32", "pull_u8", "pull_u16", "pull_u32",
];
const VALUES: &[i64] = &[
    0, 1, 0x7f, 0x80, 0xff, 0x100, 0x7fff, 0x8000, 0xffff, 0x10000, 0x12345678, 0x80000000,
    0xffffffff,
];
#[derive(Debug)]
struct Input {
    api: &'static str,
    arg: i64,
    scenario: usize,
    step: usize,
    cap: usize,
    data: usize,
    len: usize,
    layer: usize,
    offset: i64,
    seed: i64,
}
fn inputs() -> Vec<Input> {
    let mut v = Vec::new();
    let mut scenario = 0;
    let mut emit = |api, arg, cap, data, len, layer, offset, seed| {
        v.push(Input {
            api,
            arg,
            scenario,
            step: 0,
            cap,
            data,
            len,
            layer,
            offset,
            seed,
        });
        scenario += 1;
    };
    for cap in [0, 1, 2, 3, 4, 5, 8] {
        for data in 0..=cap {
            for len in 0..=cap - data {
                if len != 0 && len != (cap - data) / 2 && len != cap - data {
                    continue;
                }
                for (ai, api) in TYPED.iter().enumerate() {
                    let width = 1 << (ai % 3);
                    if (3..6).contains(&ai) && data >= width && data % width != 0 {
                        continue;
                    }
                    if ai >= 6 {
                        emit(*api, 0, cap, data, len, 0, 0, -1);
                    } else {
                        for value in VALUES {
                            if width == 1 && *value > 255 {
                                continue;
                            }
                            if width == 2 && *value > 65535 {
                                continue;
                            }
                            emit(*api, *value, cap, data, len, 0, 0, -1);
                        }
                    }
                }
            }
        }
    }
    for byte in 0..=255 {
        emit("put_u8", byte, 8, 4, 0, 0, 0, -1);
        emit("push_u8", byte, 8, 4, 0, 0, 0, -1);
        emit("get_u8", 0, 1, 0, 1, 0, 0, byte);
        emit("pull_u8", 0, 1, 0, 1, 0, 0, byte);
    }
    let access = [
        "l1",
        "l2",
        "l3",
        "l4",
        "sms",
        "l1len",
        "l2len",
        "l3len",
        "l4len",
        "pull_to_l2",
        "pull_to_l3",
    ];
    for data in (0..=8).step_by(2) {
        for len in 0..=8 - data {
            if len != 0 && len != (8 - data) / 2 && len != 8 - data {
                continue;
            }
            for (ai, api) in access.into_iter().enumerate() {
                let layer = if ai < 4 {
                    ai + 1
                } else if ai == 4 {
                    4
                } else if ai < 9 {
                    ai - 4
                } else if ai == 9 {
                    2
                } else {
                    3
                };
                for offset in -1..=8 {
                    if ai >= 9 && offset < 0 {
                        continue;
                    }
                    emit(api, 0, 8, data, len, layer, offset, -1);
                }
            }
        }
    }
    for data in (0..=8).step_by(2) {
        for offset in 0..=8 {
            for arg in [
                -10,
                -3,
                -1,
                0,
                1,
                2,
                4,
                6,
                8,
                9,
                10,
                i64::from(i32::MIN),
                i64::from(i32::MAX),
            ] {
                let total = offset - data as i64 + arg;
                if i32::try_from(total).is_err() {
                    continue;
                }
                if total > 8 - data as i64 && total <= 8 {
                    continue;
                }
                emit("l3trim", arg, 8, data, (8 - data) / 2, 3, offset, -1);
            }
        }
    }
    for cap in [1, 2, 4, 260, 512] {
        for data in 0..=4.min(cap) {
            for len in [0, 1, 254, 255, 256, 257, cap - data] {
                if len > cap - data {
                    continue;
                }
                for tag in [0, 1, 0x7f, 0x80, 0xff] {
                    emit("push_tl", tag, cap, data, len, 0, 0, -1);
                }
            }
        }
    }
    for tag in 0..=255 {
        emit("push_tl", tag, 260, 2, 256, 0, 0, -1);
    }
    emit("push_tl", 0xff, 65535, 2, 65533, 0, 0, -1);
    for (step, (api, arg)) in [
        ("put_u32", 0x01020304),
        ("put_u16", 0xff80),
        ("push_u32", 0x89abcdef),
        ("get_u16", 0),
        ("pull_u32", 0),
        ("push_tl", 7),
        ("pull_to_l3", 0),
        ("get_u16", 0),
        ("l3trim", 0),
        ("reset", 0),
        ("put_u8", 0xa5),
        ("get_u8", 0),
        ("l3", 0),
        ("l3len", 0),
    ]
    .into_iter()
    .enumerate()
    {
        v.push(Input {
            api,
            arg,
            scenario,
            step,
            cap: 16,
            data: 8,
            len: 0,
            layer: 0,
            offset: 0,
            seed: -1,
        });
    }
    assert_eq!(v.len(), COUNT);
    v
}
fn equal_states(r: &Record) {
    for key in STATES {
        assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
    }
}
fn unchanged(r: &Record, keys: &[&str]) {
    for key in keys {
        assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
    }
}
fn mutation(r: &Record, data: i64, tail: i64) {
    assert_eq!(number(r, "after_data"), data);
    assert_eq!(number(r, "after_tail"), tail);
}
fn write_span(r: &Record, start: usize, expected: &[u8]) {
    let before = bytes(&r["before_hex"]);
    let after = bytes(&r["after_hex"]);
    assert_eq!(&after[..start], &before[..start]);
    assert_eq!(&after[start..start + expected.len()], expected);
    assert_eq!(
        &after[start + expected.len()..],
        &before[start + expected.len()..]
    );
}
fn validate_outcome(r: &Record, cap: i64) {
    let api = r["api"].as_str();
    let arg = number(r, "arg");
    let data = number(r, "before_data");
    let tail = number(r, "before_tail");
    let len = tail - data;
    let layers = words(&r["before_layers"], 4);
    let rc = number(r, "return_code");
    let panic = number(r, "panic");
    if let Some(ai) = TYPED.iter().position(|a| *a == api) {
        let width = 1i64 << (ai % 3);
        let fails = match ai / 3 {
            0 => cap - tail < width,
            1 => data < width,
            _ => len < width,
        };
        assert_eq!(panic, i64::from(fails));
        if fails {
            assert_eq!(rc, -1);
            equal_states(r);
            return;
        }
        unchanged(r, &["layers", "cb", "dst", "lchan"]);
        match ai / 3 {
            0 | 1 => {
                let start = if ai < 3 { tail } else { data - width };
                if ai >= 3 {
                    assert_eq!(start % width, 0, "aligned C typed push");
                }
                let be = (arg as u32).to_be_bytes();
                write_span(r, start as usize, &be[4 - width as usize..]);
                mutation(
                    r,
                    if ai < 3 { data } else { start },
                    if ai < 3 { tail + width } else { tail },
                );
                assert_eq!(rc, -1);
            }
            _ => {
                unchanged(r, &["hex"]);
                let start = if ai < 9 { tail - width } else { data };
                let bytes = bytes(&r["before_hex"]);
                let value = bytes[start as usize..(start + width) as usize]
                    .iter()
                    .fold(0i64, |v, b| v * 256 + i64::from(*b));
                assert_eq!(rc, value);
                mutation(
                    r,
                    if ai < 9 { data } else { data + width },
                    if ai < 9 { tail - width } else { tail },
                );
            }
        }
    } else if matches!(api, "l1" | "l2" | "l3" | "l4" | "sms") {
        assert_eq!(panic, 0);
        equal_states(r);
        let index = if api == "sms" {
            3
        } else {
            api.as_bytes()[1] as usize - b'1' as usize
        };
        assert_eq!(rc, layers[index]);
    } else if matches!(api, "l1len" | "l2len" | "l3len" | "l4len") {
        let index = api.as_bytes()[1] as usize - b'1' as usize;
        assert_eq!(panic, i64::from(layers[index] < 0));
        equal_states(r);
        assert_eq!(
            rc,
            if layers[index] < 0 {
                -1
            } else {
                i64::from((tail as u32).wrapping_sub(layers[index] as u32))
            }
        );
    } else if matches!(api, "pull_to_l2" | "pull_to_l3") {
        let index = if api == "pull_to_l2" { 1 } else { 2 };
        let offset = layers[index];
        assert!(offset >= 0, "no undefined NULL pointer subtraction");
        let fails = offset < data || offset > tail;
        assert_eq!(panic, i64::from(fails));
        if fails {
            assert_eq!(rc, -1);
            equal_states(r);
        } else {
            assert_eq!(rc, offset);
            mutation(r, offset, tail);
            let mut expected = layers;
            expected[..index].fill(-1);
            assert_eq!(words(&r["after_layers"], 4), expected);
            unchanged(r, &["hex", "cb", "dst", "lchan"]);
        }
    } else if api == "l3trim" {
        assert!(layers[2] >= 0);
        let total = layers[2] - data + arg;
        assert!(i32::try_from(total).is_ok());
        assert!(
            total <= cap - data || total > cap,
            "no unsafe headroom trim"
        );
        assert_eq!(panic, i64::from(total < 0));
        assert_eq!(rc, if total < 0 || total > cap { -1 } else { 0 });
        if rc == -1 {
            equal_states(r);
        } else {
            mutation(r, data, data + total);
            unchanged(r, &["hex", "layers", "cb", "dst", "lchan"]);
        }
    } else if api == "push_tl" {
        assert_eq!(panic, i64::from(data < 2));
        if data < 2 {
            assert_eq!(rc, -1);
            equal_states(r);
        } else {
            assert_eq!(rc, data - 2);
            mutation(r, data - 2, tail);
            write_span(r, (data - 2) as usize, &[arg as u8, len as u8]);
            unchanged(r, &["layers", "cb", "dst", "lchan"]);
        }
    } else if api == "reset" {
        assert_eq!(panic, 0);
        assert_eq!(rc, -1);
        mutation(r, 0, 0);
        unchanged(r, &["hex"]);
        assert_eq!(words(&r["after_layers"], 4), vec![layers[0], -1, -1, -1]);
        assert_eq!(r["after_cb"], "0/0/0/0/0");
        assert_eq!(number(r, "after_dst"), 0);
        assert_eq!(number(r, "after_lchan"), 0);
    } else {
        panic!("unknown API {api}");
    }
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
    let (mut numeric, mut strings) = schema();
    numeric.insert("seed".into());
    for (key, _) in EXTRA_HASHES {
        strings.insert((*key).into());
    }
    let manifest = inputs();
    let mut records: Vec<Record> = Vec::with_capacity(COUNT);
    for (i, (line, input)) in lines.iter().zip(manifest).enumerate() {
        let line = if i + 1 == COUNT {
            *line
        } else {
            line.strip_suffix(',').unwrap()
        };
        let r = parse(line, &numeric, &strings);
        for (key, value) in HASHES.iter().chain(EXTRA_HASHES) {
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
        for (key, value) in [
            ("arg", input.arg),
            ("seed", input.seed),
            ("scenario", input.scenario as i64),
            ("step", input.step as i64),
            ("capacity", input.cap as i64),
        ] {
            assert_eq!(number(&r, key), value);
        }
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
            assert_eq!(
                bytes(&r["before_hex"]),
                (0..input.cap)
                    .map(|i| if input.seed < 0 {
                        0xa5 ^ i as u8
                    } else {
                        input.seed as u8
                    })
                    .collect::<Vec<_>>()
            );
            let c = input.cap as i64;
            let mut layers = vec![c / 3, c / 2, c * 2 / 3, c];
            if input.layer > 0 {
                layers[input.layer - 1] = input.offset;
            }
            assert_eq!(words(&r["before_layers"], 4), layers);
            assert_eq!(r["before_cb"], "1/2/3/4/5");
            assert_eq!(number(&r, "before_dst"), 1);
            assert_eq!(number(&r, "before_lchan"), 1);
        }
        validate_outcome(&r, input.cap as i64);
        records.push(r);
    }
    records
}

#[cfg(test)]
pub fn check_corruptions(fixture: &str) {
    for (from, to) in [
        ("\"case\":0", "\"case\":1"),
        ("\"arg\":0", "\"arg\":1"),
        ("\"seed\":-1", "\"seed\":256"),
        ("\"c_uint_bits\":32", "\"c_uint_bits\":64"),
        (EXTRA_HASHES[0].1, EXTRA_HASHES[1].1),
        ("\"api\":\"put_u8\"", "\"api\":\"unknown\""),
        ("\"panic\":1", "\"panic\":0"),
        (
            "\"before_layers\":\"0/0/0/0\"",
            "\"before_layers\":\"-1/0/0/0\"",
        ),
        ("\"case\":0", "\"case\":0,\"unknown\":0"),
        ("\"case\":0", "\"case\":0,\"case\":0"),
        ("\"case\":0,", ""),
    ] {
        assert!(fixture.contains(from));
        let corrupt = fixture.replacen(from, to, 1);
        assert!(
            std::panic::catch_unwind(|| validated_records(&corrupt)).is_err(),
            "accepted {from}"
        );
    }
    let records = validated_records(fixture);
    let lines: Vec<_> = fixture.lines().collect();
    for index in [
        records
            .iter()
            .position(|r| r["api"] == "put_u32" && number(r, "panic") == 0)
            .unwrap(),
        records
            .iter()
            .position(|r| r["api"] == "l3len" && number(r, "return_code") > 65535)
            .unwrap(),
        records
            .iter()
            .position(|r| {
                r["api"] == "pull_to_l3"
                    && number(r, "panic") == 0
                    && number(r, "before_len") > 0
                    && number(r, "return_code") > 0
            })
            .unwrap(),
        records
            .iter()
            .position(|r| {
                r["api"] == "push_tl" && number(r, "before_len") == 256 && number(r, "panic") == 0
            })
            .unwrap(),
    ] {
        let line = lines[index + 1];
        let r = &records[index];
        let key = if matches!(r["api"].as_str(), "put_u32" | "push_tl") {
            "after_hex"
        } else {
            "return_code"
        };
        let corrupt = if key == "after_hex" {
            let mut hex = r[key].clone().into_bytes();
            let start = number(r, "after_data") as usize * 2;
            hex[start] = if hex[start] == b'0' { b'1' } else { b'0' };
            line.replacen(
                &format!("\"{key}\":\"{}\"", r[key]),
                &format!("\"{key}\":\"{}\"", String::from_utf8(hex).unwrap()),
                1,
            )
        } else {
            line.replacen(&format!("\"{key}\":{}", r[key]), &format!("\"{key}\":0"), 1)
        };
        assert_ne!(corrupt, line);
        let corrupt = fixture.replacen(line, &corrupt, 1);
        assert!(std::panic::catch_unwind(|| validated_records(&corrupt)).is_err());
    }
    let missing = fixture.replacen(lines[1], "", 1);
    assert!(std::panic::catch_unwind(|| validated_records(&missing)).is_err());
}
