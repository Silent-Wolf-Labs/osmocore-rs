//! Shared stage-4 schema, input coverage, state checks, and frozen C evidence.
use crate::msgb_fixture as primitive;
use primitive::{HASHES, STATES, parse, schema, state};
pub use primitive::{Record, bytes, number, words};
pub const COUNT: usize = 4_055;
const EXTRA_HASHES: &[(&str, &str)] = &[
    (
        "utils_source_sha256",
        "a5d9d5dc5e4ba23a746257d4b751a6d2dc721a245442be8161d37ceb952fdce4",
    ),
    (
        "utils_header_sha256",
        "3a66898c8a085693541404cf47a5d18588432fceae668609edcc0ae24219c70d",
    ),
    (
        "list_header_sha256",
        "9015f71c9e30755d48f8f8597c8a5362cb98f5acccf5149b9cf6a5a6b508e39f",
    ),
];
#[derive(Debug)]
struct Input {
    api: &'static str,
    cap: i64,
    data: i64,
    len: i64,
    layout: i64,
    arg: i64,
    mode: i64,
    scenario: i64,
    step: i64,
}
fn inputs() -> Vec<Input> {
    let mut v = Vec::new();
    let mut emit = |api, cap, data, len, layout, arg, mode| {
        let scenario = v.len() as i64;
        v.push(Input {
            api,
            cap,
            data,
            len,
            layout,
            arg,
            mode,
            scenario,
            step: 0,
        });
    };
    for cap in [0, 1, 8, 16] {
        for data in 0..=cap {
            if data != 0 && data != cap / 2 && data != cap {
                continue;
            }
            for len in 0..=cap - data {
                if len != 0 && len != (cap - data) / 2 && len != cap - data {
                    continue;
                }
                for level in 0..=4 {
                    for mode in 0..5 {
                        emit("eq_data", cap, data, len, 1, level, mode);
                    }
                    for mode in 0..3 {
                        emit("eq_msg", cap, data, len, 1, level, mode);
                    }
                }
                for kind in 0..3 {
                    emit("dump", cap, data, len, kind, 0, 0);
                    emit("dump_owned", cap, data, len, kind, 0, 0);
                    for size in 0..=80 {
                        if kind == 2 && size < 80 {
                            continue;
                        }
                        emit("dump_buf", cap, data, len, kind, size, 0);
                    }
                }
                for level in 1..=4 {
                    emit("dump_layer", cap, data, len, 0, level, 0);
                    emit("dump_layer", cap, data, len, 1, level, 0);
                }
                for mode in 0..5 {
                    emit("printf", cap, data, len, 1, 0, mode);
                }
            }
        }
    }
    for cap in [1364, 1365, 1366, 3000] {
        for kind in 0..2 {
            emit("dump", cap, 0, cap, kind, 0, 0);
            emit("dump_owned", cap, 0, cap, kind, 0, 0);
            emit("dump_buf", cap, 0, cap, kind, 4100, 0);
            emit("dump_layer", cap, 0, cap, kind, 1, 0);
        }
    }
    for mode in [0, 1, 4294967295] {
        let scenario = v.len() as i64;
        for (step, api) in [
            "dequeue_count",
            "enqueue_count",
            "enqueue",
            "enqueue_count",
            "dequeue",
            "dequeue_count",
            "enqueue_count",
            "queue_free",
            "dequeue_count",
        ]
        .into_iter()
        .enumerate()
        {
            v.push(Input {
                api,
                cap: 8,
                data: 2,
                len: 3,
                layout: 0,
                arg: step as i64,
                mode,
                scenario,
                step: step as i64,
            });
        }
    }
    assert_eq!(v.len(), COUNT);
    v
}
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
fn unchanged(r: &Record) {
    for key in STATES {
        assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
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
    for key in ["layout", "mode"] {
        numeric.insert(key.into());
    }
    for (key, _) in EXTRA_HASHES {
        strings.insert((*key).into());
    }
    let mut records = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    let mut count = 0u32;
    for (i, (line, input)) in lines.iter().zip(inputs()).enumerate() {
        let queue_api = matches!(
            input.api,
            "enqueue" | "enqueue_count" | "dequeue" | "dequeue_count" | "queue_free"
        );
        let (mut n, mut s) = (numeric.clone(), strings.clone());
        if queue_api {
            for key in ["count_before", "count_after"] {
                n.insert(key.into());
            }
            for key in ["queue_before", "queue_after"] {
                s.insert(key.into());
            }
        } else {
            for key in ["payload_hex", "output_hex", "dest_hex"] {
                s.insert(key.into());
            }
        }
        let r = parse(
            if i + 1 == COUNT {
                line
            } else {
                line.strip_suffix(',').unwrap()
            },
            &n,
            &s,
        );
        for (key, val) in HASHES.iter().chain(EXTRA_HASHES) {
            assert_eq!(r[*key], *val);
        }
        assert_eq!(
            r["libosmocore_commit"],
            "950430e829a3dc1d162aa241bc0505745c5a7311"
        );
        for (key, val) in [
            ("c_uint_bits", 32),
            ("c_ulong_bits", 64),
            ("c_pointer_bits", 64),
            ("case", i as i64),
            ("capacity", input.cap),
            ("arg", input.arg),
            ("layout", input.layout),
            ("mode", input.mode),
            ("scenario", input.scenario),
            ("step", input.step),
            ("panic", 0),
        ] {
            assert_eq!(number(&r, key), val, "{key} case {i}");
        }
        assert_eq!(r["api"], input.api);
        state(&r, "before", input.cap);
        state(&r, "after", input.cap);
        assert_eq!(number(&r, "before_data"), input.data);
        assert_eq!(number(&r, "before_len"), input.len);
        assert_eq!(
            bytes(&r["before_hex"]),
            (0..input.cap).map(|i| 0xa5 ^ i as u8).collect::<Vec<_>>()
        );
        assert_eq!(r["before_cb"], "1/2/3/4/5");
        let tail = input.data + input.len;
        let layers = if queue_api {
            vec![8 / 3, 8 / 2, 8 * 2 / 3, 8]
        } else {
            match input.layout {
                0 => vec![-1; 4],
                1 => vec![
                    input.data,
                    input.data + input.len / 3,
                    input.data + input.len * 2 / 3,
                    tail,
                ],
                2 => vec![0, tail, input.data, input.cap],
                _ => panic!("layout"),
            }
        };
        assert_eq!(words(&r["before_layers"], 4), layers);
        for key in ["before_dst", "before_lchan", "after_dst", "after_lchan"] {
            assert_eq!(number(&r, key), 1);
        }
        let rc = number(&r, "return_code");
        if queue_api {
            if input.step == 0 {
                queue.clear();
                count = input.mode as u32;
            }
            let ids = |q: &std::collections::VecDeque<i64>| {
                q.iter().map(|id| format!("{id}/")).collect::<String>()
            };
            assert_eq!(r["queue_before"], ids(&queue));
            assert_eq!(number(&r, "count_before"), i64::from(count));
            let mut id = None;
            match input.api {
                "enqueue" | "enqueue_count" => {
                    queue.push_back(input.step);
                    if input.api == "enqueue_count" {
                        count = count.wrapping_add(1);
                    }
                }
                "queue_free" => queue.clear(),
                _ => {
                    id = queue.pop_front();
                    if id.is_some() && input.api == "dequeue_count" {
                        count = count.wrapping_sub(1);
                    }
                }
            }
            assert_eq!(rc, id.unwrap_or(-1));
            assert_eq!(r["queue_after"], ids(&queue));
            assert_eq!(number(&r, "count_after"), i64::from(count));
            for key in STATES {
                if *key != "cb" {
                    assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
                }
            }
            assert_eq!(words(&r["after_cb"], 5), vec![id.unwrap_or(1), 2, 3, 4, 5]);
        } else {
            let payload = bytes(&r["payload_hex"]);
            let output = bytes(&r["output_hex"]);
            let dest = bytes(&r["dest_hex"]);
            match input.api {
                "printf" => {
                    let texts: &[&[u8]] = &[b"", b"abc", b"x=002a/-7/%", b"A\0B", b"caf\xc3\xa9"];
                    assert_eq!(payload, texts[input.mode as usize]);
                    assert!(output.is_empty() && dest.is_empty());
                    let room = (input.cap - tail) as usize;
                    let success = payload.len() < room;
                    assert_eq!(rc, if success { 0 } else { -22 });
                    assert_eq!(
                        number(&r, "after_tail"),
                        tail + if success { payload.len() as i64 } else { 0 }
                    );
                    let mut expected = bytes(&r["before_hex"]);
                    if room > 0 {
                        let len = payload.len().min(room - 1);
                        expected[tail as usize..tail as usize + len]
                            .copy_from_slice(&payload[..len]);
                        expected[tail as usize + len] = 0;
                    }
                    assert_eq!(bytes(&r["after_hex"]), expected);
                    for key in ["data", "layers", "cb"] {
                        assert_eq!(r[&format!("before_{key}")], r[&format!("after_{key}")]);
                    }
                }
                "eq_data" | "eq_msg" => {
                    unchanged(&r);
                    assert!(output.is_empty() && dest.is_empty());
                    let start = if input.arg == 0 {
                        input.data
                    } else {
                        layers[input.arg as usize - 1]
                    };
                    let backing = bytes(&r["before_hex"]);
                    let mut expected = backing[start as usize..tail as usize].to_vec();
                    if input.mode == 1 && !expected.is_empty() {
                        expected[0] ^= 255;
                    }
                    if input.mode == 2 {
                        expected.push(0);
                    }
                    assert_eq!(payload, expected);
                    let equal = input.mode == 0 || (input.mode == 1 && tail == start);
                    assert_eq!(rc, i64::from(equal));
                }
                "dump" | "dump_owned" | "dump_buf" | "dump_layer" => {
                    unchanged(&r);
                    assert!(payload.is_empty());
                    assert!(output.is_ascii());
                    assert!((0..=1).contains(&rc));
                    if input.api == "dump_buf" {
                        assert_eq!(dest.len(), input.arg as usize);
                        if rc == 1 {
                            let end = dest.iter().position(|b| *b == 0).unwrap();
                            assert_eq!(output, dest[..end]);
                            assert!(dest[end + 1..].iter().all(|b| *b == 0x7e));
                        } else {
                            assert_eq!(output, b"ERROR");
                        }
                    } else {
                        assert!(dest.is_empty());
                    }
                    if rc == 0 {
                        assert_eq!(output, b"ERROR");
                    }
                }
                _ => panic!("API"),
            }
        }
        records.push(r);
    }
    // Frozen fingerprint of direct C output: catches every output byte change,
    // including static dump strings. Not a cryptographic authenticity claim.
    assert_eq!(fingerprint(fixture), FROZEN_FINGERPRINT);
    records
}
const FROZEN_FINGERPRINT: u64 = 0x37cdbc85488669dd;
#[cfg(test)]
pub fn check_corruptions(fixture: &str) {
    for (from, to) in [
        ("\"case\":0", "\"case\":1"),
        ("\"case\":0", "\"case\":0,\"case\":0"),
        ("\"case\":0", "\"case\":0,\"unknown\":0"),
        ("\"case\":0,", ""),
        ("\"c_uint_bits\":32", "\"c_uint_bits\":64"),
        ("\"api\":\"eq_data\"", "\"api\":\"unknown\""),
        ("\"return_code\":1", "\"return_code\":0"),
        ("\"output_hex\":\"", "\"output_hex\":\"00"),
        ("\"after_hex\":\"a5", "\"after_hex\":\"a4"),
        ("\"count_after\":1", "\"count_after\":2"),
        (EXTRA_HASHES[0].1, EXTRA_HASHES[1].1),
    ] {
        assert!(fixture.contains(from));
        assert!(
            std::panic::catch_unwind(|| validated_records(&fixture.replacen(from, to, 1))).is_err(),
            "accepted {from}"
        );
    }
    let line = fixture.lines().nth(1).unwrap();
    assert!(
        std::panic::catch_unwind(|| validated_records(&fixture.replacen(line, "", 1))).is_err()
    );
}
