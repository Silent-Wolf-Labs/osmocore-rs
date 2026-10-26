//! Shared frozen copy/resize inputs and strict integrity checks; no observations generated.
use crate::msgb_fixture as primitive;
use primitive::{HASHES, STATES, parse, schema, state};
pub use primitive::{Record, bytes, number, words};
pub const COUNT: usize = 8_888;
const EXTRA_HASHES: &[(&str, &str)] = &[
    (
        "logging_source_sha256",
        "13a0adeda85a762695f4b8c338f5b272b81774a15780c218ac4ccd13b21b2d0d",
    ),
    (
        "logging_header_sha256",
        "7454572bcd4655f9cd87d8fe64e87f10bc17c322a14669982dfb62bcff002c55",
    ),
    (
        "utils_header_sha256",
        "3a66898c8a085693541404cf47a5d18588432fceae668609edcc0ae24219c70d",
    ),
];
const COPIES: &[&str] = &["copy", "copy_c", "copy_resize", "copy_resize_c"];
#[derive(Debug)]
struct Input {
    api: &'static str,
    arg: i64,
    cap: i64,
    data: i64,
    len: i64,
    area: i64,
    old: i64,
    new: i64,
    layers: Vec<i64>,
    scenario: usize,
    step: usize,
}
fn copy_layers(cap: i64, mask: i64, reverse: bool) -> Vec<i64> {
    let mut layers = if reverse {
        vec![cap, cap / 2, cap * 2 / 3, 0]
    } else {
        vec![cap / 3, cap / 2, cap * 2 / 3, cap]
    };
    for (i, offset) in layers.iter_mut().enumerate() {
        if mask & (1 << i) == 0 {
            *offset = -1;
        }
    }
    layers
}
fn resize_layers(cap: i64, data: i64, len: i64, area: i64, old: i64, layout: usize) -> Vec<i64> {
    let post = area + old;
    match layout {
        0 => vec![data, area, post, data + len],
        1 => vec![post; 4],
        2 => vec![0, cap / 3, cap / 2, cap],
        _ => vec![data + len, post, area, 0],
    }
}
fn safe_resize(
    cap: i64,
    data: i64,
    len: i64,
    area: i64,
    old: i64,
    new: i64,
    layers: &[i64],
) -> bool {
    let post = area + old;
    if area < 0 || area > cap || post < 0 || post > cap {
        return false;
    }
    if old < 0 || new < 0 || area < data || post > data + len {
        return true;
    }
    let delta = new - old;
    if delta == 0 || len + delta > cap {
        return true;
    }
    if len + delta > cap - data {
        return false;
    }
    layers
        .iter()
        .all(|o| *o < post || (0..=cap).contains(&(*o + delta)))
}
fn inputs() -> Vec<Input> {
    let mut v = Vec::new();
    let mut scenario = 0;
    let mut emit = |api, arg, cap, data, len, area, old, new, layers| {
        v.push(Input {
            api,
            arg,
            cap,
            data,
            len,
            area,
            old,
            new,
            layers,
            scenario,
            step: 0,
        });
        scenario += 1;
    };
    for cap in [0, 1, 2, 8, 16] {
        for data in 0..=cap {
            if data != 0 && data != cap / 2 && data != cap {
                continue;
            }
            for len in 0..=cap - data {
                if len != 0 && len != (cap - data) / 2 && len != cap - data {
                    continue;
                }
                for mask in [0, 15] {
                    for (ai, api) in COPIES.iter().enumerate() {
                        for size in 0..=cap + 2 {
                            if ai < 2 && size != cap {
                                continue;
                            }
                            let layers = copy_layers(cap, mask, false);
                            if size >= data + len && layers.iter().any(|o| *o > size) {
                                continue;
                            }
                            emit(*api, size, cap, data, len, 0, 0, 0, layers);
                        }
                    }
                }
            }
        }
    }
    for meta in 3..35 {
        for api in COPIES {
            emit(
                *api,
                8,
                8,
                2,
                3,
                0,
                0,
                0,
                copy_layers(8, (meta - 3) % 16, meta >= 19),
            );
        }
    }
    for cap in [0, 1, 2, 4, 8] {
        for data in 0..=cap {
            if data != 0 && data != cap / 2 && data != cap {
                continue;
            }
            for len in 0..=cap - data {
                if len != 0 && len != (cap - data) / 2 && len != cap - data {
                    continue;
                }
                for area in 0..=cap {
                    for old in 0..=cap - area {
                        for new in 0..=cap + 1 {
                            if cap == 8
                                && new != 0
                                && new != old
                                && new != old + 1
                                && new != cap + 1
                            {
                                continue;
                            }
                            for layout in 0..4 {
                                let layers = resize_layers(cap, data, len, area, old, layout);
                                if safe_resize(cap, data, len, area, old, new, &layers) {
                                    emit("resize_area", 0, cap, data, len, area, old, new, layers);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for area in 1..=8 {
        for which in 0..2 {
            let (old, new) = if which == 0 { (-1, 0) } else { (0, -1) };
            emit(
                "resize_area",
                0,
                8,
                2,
                4,
                area,
                old,
                new,
                resize_layers(8, 2, 4, area, old, 0),
            );
        }
    }
    for (step, (api, arg, area, old, new)) in [
        ("resize_area", 0, 4, 2, 4),
        ("resize_area", 0, 8, 2, 1),
        ("copy_resize", 16, 0, 0, 0),
        ("resize_area", 0, 2, 0, 2),
        ("resize_area", 0, 6, 4, 0),
        ("resize_area", 0, 9, 0, 0),
        ("copy", 16, 0, 0, 0),
        ("resize_area", 0, 2, 7, 0),
        ("copy_resize_c", 16, 0, 0, 0),
    ]
    .into_iter()
    .enumerate()
    {
        v.push(Input {
            api,
            arg,
            cap: 16,
            data: 2,
            len: 8,
            area,
            old,
            new,
            layers: resize_layers(16, 2, 8, 4, 2, 0),
            scenario,
            step,
        });
    }
    scenario += 1;
    for (step, (api, arg, area, old, new)) in [
        ("resize_area", 0, 1, 1, 0),
        ("resize_area", 0, 1, 0, 1),
        ("copy_resize", 65535, 0, 0, 0),
    ]
    .into_iter()
    .enumerate()
    {
        v.push(Input {
            api,
            arg,
            cap: 65535,
            data: 1,
            len: 65534,
            area,
            old,
            new,
            layers: resize_layers(65535, 1, 65534, 1, 1, 0),
            scenario,
            step,
        });
    }
    scenario += 1;
    v.push(Input {
        api: "copy_resize_c",
        arg: 8,
        cap: 65535,
        data: 1,
        len: 4,
        area: 0,
        old: 0,
        new: 0,
        layers: vec![-1; 4],
        scenario,
        step: 0,
    });
    assert_eq!(v.len(), COUNT);
    v
}
fn equal_source(r: &Record) {
    for key in STATES {
        assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
    }
}
fn absent_copy(r: &Record) {
    assert_eq!(number(r, "copy_present"), 0);
    assert_eq!(number(r, "copy_capacity"), -1);
    assert_eq!(number(r, "distinct"), 0);
    for key in STATES {
        assert_eq!(
            r[&format!("copy_{key}")],
            if matches!(*key, "hex" | "layers" | "cb") {
                ""
            } else {
                "-1"
            }
        );
    }
}
fn validate_outcome(r: &Record, input: &Input) {
    let cap = input.cap;
    let api = input.api;
    let data = number(r, "before_data");
    let tail = number(r, "before_tail");
    let len = tail - data;
    let layers = words(&r["before_layers"], 4);
    let panic = number(r, "panic");
    let rc = number(r, "return_code");
    if COPIES.contains(&api) {
        equal_source(r);
        assert_eq!(panic, 0);
        assert_eq!(rc, -1);
        let size = if matches!(api, "copy" | "copy_c") {
            cap
        } else {
            input.arg
        };
        let success = size >= tail;
        if !success {
            absent_copy(r);
            return;
        }
        assert!(
            layers.iter().all(|o| *o <= size),
            "no C relocated pointer outside allocation"
        );
        assert_eq!(number(r, "copy_present"), 1);
        assert_eq!(number(r, "copy_capacity"), size);
        assert_eq!(number(r, "distinct"), 1);
        state(r, "copy", size);
        assert_eq!(number(r, "copy_data"), data);
        assert_eq!(number(r, "copy_tail"), tail);
        assert_eq!(words(&r["copy_layers"], 4), layers);
        assert_eq!(r["copy_cb"], "0/0/0/0/0");
        assert_eq!(number(r, "copy_dst"), 0);
        assert_eq!(number(r, "copy_lchan"), 0);
        let before = bytes(&r["before_hex"]);
        let after = bytes(&r["copy_hex"]);
        assert!(after[..data as usize].iter().all(|b| *b == 0));
        assert_eq!(
            after[data as usize..tail as usize],
            before[data as usize..tail as usize]
        );
        assert!(after[tail as usize..].iter().all(|b| *b == 0));
    } else {
        assert_eq!(api, "resize_area");
        absent_copy(r);
        let post = input.area + input.old;
        let delta = input.new - input.old;
        assert!((0..=cap).contains(&input.area) && (0..=cap).contains(&post));
        let abort = input.old < 0 || input.new < 0 || input.area < data || post > tail;
        assert_eq!(panic, i64::from(abort));
        let fails = abort || len + delta > cap;
        assert_eq!(rc, if fails { -1 } else { 0 });
        if fails || delta == 0 {
            equal_source(r);
            return;
        }
        assert!(len + delta <= cap - data);
        assert!(
            layers.iter().all(|o| *o >= 0),
            "exclude C NULL relational comparisons"
        );
        let expected_layers: Vec<_> = layers
            .iter()
            .map(|o| if *o >= post { *o + delta } else { *o })
            .collect();
        assert!(expected_layers.iter().all(|o| (0..=cap).contains(o)));
        assert_eq!(words(&r["after_layers"], 4), expected_layers);
        assert_eq!(number(r, "after_data"), data);
        assert_eq!(number(r, "after_tail"), tail + delta);
        for key in ["cb", "dst", "lchan"] {
            assert_eq!(r[&format!("after_{key}")], r[&format!("before_{key}")]);
        }
        // Integrity property: only the moved suffix's destination bytes can differ.
        let before = bytes(&r["before_hex"]);
        let after = bytes(&r["after_hex"]);
        let dst = (input.area + input.new) as usize;
        let count = (tail - post) as usize;
        assert_eq!(&after[..dst], &before[..dst]);
        assert_eq!(
            &after[dst..dst + count],
            &before[post as usize..tail as usize]
        );
        assert_eq!(&after[dst + count..], &before[dst + count..]);
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
    for key in [
        "area",
        "old_size",
        "new_size",
        "copy_present",
        "copy_capacity",
        "distinct",
        "c_int_bits",
    ] {
        numeric.insert(key.into());
    }
    for (key, _) in EXTRA_HASHES {
        strings.insert((*key).into());
    }
    for key in STATES {
        let name = format!("copy_{key}");
        if matches!(*key, "hex" | "layers" | "cb") {
            strings.insert(name);
        } else {
            numeric.insert(name);
        }
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
            ("c_int_bits", 32),
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
            ("capacity", input.cap),
            ("area", input.area),
            ("old_size", input.old),
            ("new_size", input.new),
            ("scenario", input.scenario as i64),
            ("step", input.step as i64),
        ] {
            assert_eq!(number(&r, key), value);
        }
        state(&r, "before", input.cap);
        state(&r, "after", input.cap);
        if input.step > 0 {
            for key in STATES {
                assert_eq!(
                    r[&format!("before_{key}")],
                    records[i - 1][&format!("after_{key}")],
                    "sequence continuity"
                );
            }
        } else {
            assert_eq!(number(&r, "before_data"), input.data);
            assert_eq!(number(&r, "before_len"), input.len);
            assert_eq!(words(&r["before_layers"], 4), input.layers);
            assert_eq!(
                bytes(&r["before_hex"]),
                (0..input.cap).map(|i| 0xa5 ^ i as u8).collect::<Vec<_>>()
            );
            assert_eq!(r["before_cb"], "1/2/3/4/5");
            assert_eq!(number(&r, "before_dst"), 1);
            assert_eq!(number(&r, "before_lchan"), 1);
        }
        validate_outcome(&r, &input);
        records.push(r);
    }
    records
}

#[cfg(test)]
pub fn check_corruptions(fixture: &str) {
    for (from, to) in [
        ("\"case\":0", "\"case\":1"),
        ("\"arg\":0", "\"arg\":1"),
        ("\"c_int_bits\":32", "\"c_int_bits\":64"),
        (EXTRA_HASHES[0].1, EXTRA_HASHES[1].1),
        ("\"api\":\"copy\"", "\"api\":\"unknown\""),
        ("\"distinct\":1", "\"distinct\":0"),
        ("\"copy_cb\":\"0/0/0/0/0\"", "\"copy_cb\":\"1/2/3/4/5\""),
        ("\"copy_present\":1", "\"copy_present\":0"),
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
    for (api, key, predicate) in [
        ("copy_resize", "copy_hex", 0),
        ("copy_resize", "copy_layers", 0),
        ("resize_area", "after_hex", 1),
        ("resize_area", "after_layers", 1),
        ("resize_area", "return_code", 2),
    ] {
        let index = records
            .iter()
            .position(|r| {
                r["api"] == api
                    && match predicate {
                        0 => {
                            number(r, "copy_present") == 1
                                && number(r, "before_data") > 0
                                && number(r, "before_len") > 0
                                && number(r, "capacity") > 0
                                && (key != "copy_layers" || r["copy_layers"] != "-1/-1/-1/-1")
                        }
                        1 => {
                            number(r, "return_code") == 0
                                && number(r, "new_size") != number(r, "old_size")
                                && r["before_hex"] != r["after_hex"]
                        }
                        _ => number(r, "panic") == 1,
                    }
            })
            .unwrap();
        let line = lines[index + 1];
        let old = &records[index][key];
        let changed = if key.ends_with("hex") {
            let mut b = old.clone().into_bytes();
            b[0] = if b[0] == b'0' { b'1' } else { b'0' };
            String::from_utf8(b).unwrap()
        } else if key.ends_with("layers") {
            "-1/-1/-1/-1".into()
        } else {
            "0".into()
        };
        assert_ne!(*old, changed);
        let (from, to) = if key == "return_code" {
            (format!("\"{key}\":{old}"), format!("\"{key}\":{changed}"))
        } else {
            (
                format!("\"{key}\":\"{old}\""),
                format!("\"{key}\":\"{changed}\""),
            )
        };
        let corrupt = fixture.replacen(line, &line.replacen(&from, &to, 1), 1);
        assert!(std::panic::catch_unwind(|| validated_records(&corrupt)).is_err());
    }
    let missing = fixture.replacen(lines[1], "", 1);
    assert!(std::panic::catch_unwind(|| validated_records(&missing)).is_err());
}
