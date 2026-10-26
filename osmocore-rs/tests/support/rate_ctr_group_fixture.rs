//! Independent reader for complete group/registry and interval C observations.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counter {
    pub name: Vec<u8>,
    pub description: Option<Vec<u8>>,
    /// current, previous, second/minute/hour/day last and rate.
    pub words: [u64; 10],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: usize,
    pub index: u32,
    pub prefix: Vec<u8>,
    pub description: Option<Vec<u8>>,
    pub class_id: i32,
    pub name: Option<Vec<u8>>,
    pub counters: Vec<Counter>,
}
#[derive(Debug)]
pub struct Record {
    pub scenario: usize,
    pub step: usize,
    pub op: String,
    pub slot: usize,
    pub a: u64,
    pub b: i64,
    pub text: Option<Vec<u8>>,
    pub result: i64,
    pub before_tick: u64,
    pub after_tick: u64,
    pub before: Vec<Group>,
    pub after: Vec<Group>,
    pub trace: Vec<i32>,
}
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
const REFERENCE_SHA: &str = "78d88e94f93f5340cd658716a12acfdf384b3fcdca514a48b426c27559c7226c";
const GENERATOR_SHA: &str = "1ecb801d0cf0f58133daf1fa6d875c084ad1415bab2aca01f4c26ec5ebf9174f";
const HASHES: [u64; 5] = [
    0x17fe5dd175656346,
    0xfa7230b956b7d19c,
    0x365e68264854ef6c,
    0xd13d648785982d1b,
    0x44e10c0a6c35ebe8,
];
const INPUT_HASH: u64 = 0xc325ef169ba7ff5d;
fn num<T: std::str::FromStr + ToString>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    let n = s.parse::<T>().unwrap();
    assert_eq!(n.to_string(), s, "noncanonical number");
    n
}
fn string(s: &str) -> Option<Vec<u8>> {
    if s == "s:-" {
        return None;
    }
    let hex = s.strip_prefix("s:").unwrap();
    assert_eq!(hex.len() % 2, 0);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    assert!(!bytes.contains(&0));
    Some(bytes)
}
fn state(s: &str) -> Vec<Group> {
    if s == "-" {
        return vec![];
    }
    let mut ids = BTreeSet::new();
    s.split(';')
        .map(|g| {
            let f: Vec<_> = g.split(',').collect();
            assert!(f.len() >= 7);
            let count = num::<u32>(f[6]);
            assert!(count <= 3);
            assert_eq!(f.len(), 7 + 12 * count as usize);
            let id = num::<usize>(f[0]);
            assert!(id < 8 && ids.insert(id));
            Group {
                id,
                index: num(f[1]),
                prefix: string(f[2]).unwrap(),
                description: string(f[3]),
                class_id: num(f[4]),
                name: string(f[5]),
                counters: (0..count as usize)
                    .map(|i| {
                        let k = 7 + 12 * i;
                        Counter {
                            name: string(f[k]).unwrap(),
                            description: string(f[k + 1]),
                            words: std::array::from_fn(|j| num(f[k + 2 + j])),
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}
fn header(line: &str) {
    assert_eq!(
        line.split('\t').collect::<Vec<_>>(),
        [
            "meta",
            "1",
            "exact-source",
            "unknown",
            REFERENCE_SHA,
            GENERATOR_SHA,
            "64",
            "64",
            "32"
        ]
    );
}
pub fn validate_structure(text: &str) -> Vec<Record> {
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let mut lines = text.lines();
    header(lines.next().unwrap());
    let mut rows = Vec::<Record>::new();
    let mut inputs = String::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for line in lines {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 13);
        let row = Record {
            scenario: num(f[0]),
            step: num(f[1]),
            op: f[2].into(),
            slot: num(f[3]),
            a: num(f[4]),
            b: num(f[5]),
            text: string(f[6]),
            result: num(f[7]),
            before_tick: num(f[8]),
            after_tick: num(f[9]),
            before: state(f[10]),
            after: state(f[11]),
            trace: if f[12] == "-" {
                vec![]
            } else {
                f[12].split(',').map(num).collect()
            },
        };
        assert!(row.slot < 8);
        if let Some(last) = rows.last() {
            if row.scenario == last.scenario {
                assert_eq!(row.step, last.step + 1);
                assert_eq!(row.before_tick, last.after_tick);
                if row.op != "alias-report" {
                    assert_eq!(row.before, last.after);
                }
            } else {
                assert_eq!(row.scenario, last.scenario + 1);
                assert_eq!(row.step, 0);
            }
        } else {
            assert_eq!((row.scenario, row.step), (1, 0));
        }
        if !matches!(row.op.as_str(), "phase" | "expire") {
            assert_eq!(row.before_tick, row.after_tick);
        }
        if row.op == "alias" {
            assert_eq!(row.result, if row.a == 0 { 63 } else { 56 });
        }
        if row.op == "alias-report" {
            assert_eq!(row.result, 1);
            assert_eq!(row.before, row.after);
            assert_eq!(row.after[0].name.as_deref(), Some(b"report".as_slice()));
        }
        if matches!(row.op.as_str(), "lookup" | "counter-lookup" | "difference") {
            // difference can update previous
            if row.op != "difference" {
                assert_eq!(row.before, row.after);
            }
        }
        if row.op == "invalid" {
            assert_eq!(row.result, 0);
            assert!(row.before.is_empty() && row.after.is_empty());
        }
        inputs.push_str(&f[..7].join("\t"));
        inputs.push('\n');
        *counts.entry(row.op.clone()).or_default() += 1;
        rows.push(row);
    }
    assert_eq!(rows.len(), 1915);
    assert_eq!(rows.last().unwrap().scenario, 608);
    assert_eq!(counts.values().sum::<usize>(), 1915);
    assert_eq!(counts.len(), 21);
    assert_eq!(counts["identifier"], 255);
    assert_eq!(counts["identifier-counter"], 255);
    assert_eq!(counts["alias"], 2);
    assert_eq!(counts["alias-report"], 2);
    assert_eq!(counts["expire"], 361);
    assert_eq!(
        fingerprint(&inputs),
        INPUT_HASH,
        "frozen input/coverage changed"
    );
    rows
}
pub fn integrity(text: &str, reference: &str, generator: &str, diagnostics: &str, native: &str) {
    validate_structure(text);
    for (i, s) in [text, reference, generator, diagnostics, native]
        .iter()
        .enumerate()
    {
        assert_eq!(fingerprint(s), HASHES[i], "artifact fingerprint {i}");
    }
    assert!(reference.contains("\"upstream_git_revision\": \"unknown\""));
    assert!(reference.contains("\"included_translation_unit\": \"src/core/rate_ctr.c\""));
    assert!(reference.contains("\"preprocessor_defines_sha256\""));
    assert!(reference.contains("\"compiler_flags\""));
    assert!(
        generator.contains(REFERENCE_SHA)
            && diagnostics.contains(REFERENCE_SHA)
            && diagnostics.contains(GENERATOR_SHA)
    );
    assert_eq!(diagnostics.matches("\"probe\"").count(), 7);
    let mut lines = native.lines();
    header(lines.next().unwrap());
    assert_eq!(
        lines.collect::<Vec<_>>(),
        [
            "native\tinit-ok\t0\t0\t1\t1\t2048\t1\t1",
            "native\tinit-create-fail\t-1\t-1\t0\t1\t2048\t0\t0",
            "native\tinit-schedule-fail\t0\t0\t1\t1\t2048\t1\t1"
        ]
    );
}
pub fn corruptions(text: &str, reference: &str, generator: &str, diagnostics: &str, native: &str) {
    let rejects = |s: String| assert!(std::panic::catch_unwind(|| validate_structure(&s)).is_err());
    rejects(text.replacen("exact-source", "unverified", 1));
    rejects(text.replacen("\tgroups\t0\t0\t0\t", "\tgroups\t00\t0\t0\t", 1));
    rejects(text.lines().take(1915).collect::<Vec<_>>().join("\n") + "\n");
    rejects(text.replacen("\talloc\t0\t3\t0\t", "\talloc\t0\t4\t0\t", 1));
    for i in 0..5 {
        let mut files = [
            text.to_owned(),
            reference.to_owned(),
            generator.to_owned(),
            diagnostics.to_owned(),
            native.to_owned(),
        ];
        files[i].push(' ');
        assert!(
            std::panic::catch_unwind(|| integrity(
                &files[0], &files[1], &files[2], &files[3], &files[4]
            ))
            .is_err()
        );
    }
}
