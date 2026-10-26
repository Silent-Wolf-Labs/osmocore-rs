use super::rate_ctr_group_fixture::{self as fixture, Counter, Group};
use osmocore_rs::rate_ctr::*;
use osmocore_rs::time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone};
const TEXT: &str =
    include_str!("../compatibility/vectors/rate_ctr_group/rate_ctr_group_vectors.tsv");
const REFERENCE: &str =
    include_str!("../compatibility/vectors/rate_ctr_group/rate_ctr_group_provenance.json");
const GENERATOR: &str = include_str!(
    "../compatibility/vectors/rate_ctr_group/rate_ctr_group_generator_provenance.json"
);
const DIAGNOSTICS: &str =
    include_str!("../compatibility/vectors/rate_ctr_group/rate_ctr_group_diagnostics.json");
const NATIVE: &str =
    include_str!("../compatibility/vectors/rate_ctr_group/rate_ctr_group_native.tsv");
fn counter(name: &str, description: &str) -> RateCtrDesc {
    RateCtrDesc {
        name: name.into(),
        description: Some(description.into()),
    }
}
fn desc(which: u64) -> RateCtrGroupDesc {
    let counters = || {
        vec![
            counter("a", "A"),
            counter("a", "duplicate"),
            counter("b", "B"),
        ]
    };
    let dots = || vec![counter("a.b", "A"), counter("plain", "P")];
    let (prefix, description, class_id, counters) = match which {
        0 => ("g", "Group", i32::MIN, counters()),
        1 => ("g.dot", "Dot", i32::MAX, dots()),
        2 => ("bad name", "Invalid", -1, counters()),
        3 => ("empty", "Empty", 0, vec![]),
        4 => ("other", "Other", 42, counters()),
        5 => ("g:dot", "Colon", 1, dots()),
        _ => panic!(),
    };
    RateCtrGroupDesc {
        group_name_prefix: prefix.into(),
        group_description: Some(description.into()),
        class_id,
        counters,
    }
}
fn logical_id(nodes: &[Option<RateCtrGroupId>], id: &RateCtrGroupId) -> usize {
    nodes.iter().position(|n| n.as_ref() == Some(id)).unwrap()
}
fn snapshot(r: &RateCtrRuntime, nodes: &[Option<RateCtrGroupId>]) -> Vec<Group> {
    r.registered_groups()
        .into_iter()
        .map(|id| {
            let g = r.group(&id).unwrap();
            let d = g.descriptor();
            Group {
                id: logical_id(nodes, &id),
                index: g.index(),
                prefix: d.group_name_prefix.as_bytes().to_vec(),
                description: d.group_description.as_ref().map(|s| s.as_bytes().to_vec()),
                class_id: d.class_id,
                name: g.name().map(|s| s.as_bytes().to_vec()),
                counters: g
                    .counters()
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Counter {
                        name: d.counters[i].name.as_bytes().to_vec(),
                        description: d.counters[i]
                            .description
                            .as_ref()
                            .map(|s| s.as_bytes().to_vec()),
                        words: [
                            c.current,
                            c.previous,
                            c.intv[0].last,
                            c.intv[0].rate,
                            c.intv[1].last,
                            c.intv[1].rate,
                            c.intv[2].last,
                            c.intv[2].rate,
                            c.intv[3].last,
                            c.intv[3].rate,
                        ],
                    })
                    .collect(),
            }
        })
        .collect()
}
#[test]
fn stage7_evidence_integrity_and_corruptions() {
    fixture::integrity(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS, NATIVE);
    fixture::corruptions(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS, NATIVE);
}
#[test]
fn stage7_complete_direct_c_replay() {
    let rows = fixture::validate_structure(TEXT);
    let mut scenario = 0;
    let mut r = RateCtrRuntime::new();
    let mut nodes = vec![None; 8];
    let mut replayed = 0;
    for row in rows {
        if scenario != row.scenario {
            scenario = row.scenario;
            r = RateCtrRuntime::new();
            nodes.fill(None);
        }
        // Ownership records characterize C aliasing; Rust intentionally copies.
        if matches!(row.op.as_str(), "alias" | "alias-report") {
            continue;
        }
        assert_eq!(r.ticks(), row.before_tick);
        assert_eq!(
            snapshot(&r, &nodes),
            row.before,
            "before {scenario}/{} {}",
            row.step,
            row.op
        );
        let mut rc = 0i64;
        let mut trace = vec![];
        let id = nodes[row.slot].clone();
        let text = || String::from_utf8_lossy(row.text.as_deref().unwrap()).into_owned();
        match row.op.as_str() {
            "alloc" => match r.allocate(desc(row.a), row.b as u32) {
                Ok(id) => {
                    nodes[row.slot] = Some(id);
                    rc = 1;
                }
                Err(e) => assert_eq!(e, RateCtrError::InvalidDescriptor),
            },
            "identifier" | "identifier-counter" => {
                let name = text();
                let d = if row.op == "identifier" {
                    RateCtrGroupDesc {
                        group_name_prefix: name,
                        group_description: Some("byte".into()),
                        class_id: 0,
                        counters: vec![],
                    }
                } else {
                    RateCtrGroupDesc {
                        group_name_prefix: "identifier".into(),
                        group_description: Some("counter-byte".into()),
                        class_id: -1,
                        counters: vec![counter(&name, "byte")],
                    }
                };
                match r.allocate(d, 0) {
                    Ok(id) => {
                        nodes[0] = Some(id);
                        rc = 1;
                    }
                    Err(e) => assert_eq!(e, RateCtrError::InvalidDescriptor),
                }
            }
            "invalid" => {
                let mut d = desc(0);
                d.counters = if row.a < 2 {
                    vec![]
                } else {
                    vec![counter(if row.a == 4 { "bad name" } else { "" }, "D")]
                };
                d.group_name_prefix = if row.a < 2 { "".into() } else { "valid".into() };
                assert_eq!(r.allocate(d, 1), Err(RateCtrError::InvalidDescriptor));
            }
            "free" => {
                if let Some(id) = id {
                    r.free(&id).unwrap();
                }
                nodes[row.slot] = None;
            }
            "index" => r.set_index(id.as_ref().unwrap(), row.a as u32).unwrap(),
            "name" => r
                .set_name(
                    id.as_ref().unwrap(),
                    row.text.as_deref().map(|s| std::str::from_utf8(s).unwrap()),
                )
                .unwrap(),
            "seed" => {
                let c = r
                    .group_mut(id.as_ref().unwrap())
                    .unwrap()
                    .counter_mut(row.a as u32)
                    .unwrap();
                let v = row.b as u64;
                c.current = v;
                c.previous = v.wrapping_add(13);
                for j in 0..4 {
                    c.intv[j].last = v.wrapping_add(j as u64 + 1);
                    c.intv[j].rate = 100 + j as u64;
                }
            }
            "add" => r
                .add(id.as_ref().unwrap(), row.a as u32, row.b as i32)
                .unwrap(),
            "inc" => r.inc(id.as_ref().unwrap(), row.a as u32).unwrap(),
            "difference" => {
                rc = r
                    .group_mut(id.as_ref().unwrap())
                    .unwrap()
                    .counter_mut(row.a as u32)
                    .unwrap()
                    .difference()
            }
            "reset" => r.reset(id.as_ref().unwrap()).unwrap(),
            "counter-reset" => r
                .group_mut(id.as_ref().unwrap())
                .unwrap()
                .counter_mut(row.a as u32)
                .unwrap()
                .reset(),
            "lookup" => {
                rc = r
                    .find_group(&text(), row.a as u32)
                    .map_or(-1, |id| logical_id(&nodes, &id) as i64)
            }
            "counter-lookup" => {
                rc = r
                    .group(id.as_ref().unwrap())
                    .unwrap()
                    .counter_by_name(&text())
                    .map_or(-1, i64::from)
            }
            "phase" => {
                assert!(r.registered_groups().is_empty());
                r = RateCtrRuntime::with_tick_phase(row.a);
            }
            "expire" => r.process_expirations(row.a).unwrap(),
            "groups" => {
                rc = i64::from(
                    r.for_each_group(|r, id| {
                        trace.push(logical_id(&nodes, id) as i32);
                        match row.a {
                            1 => {
                                let index = r.group(id).unwrap().index();
                                r.set_index(id, index.wrapping_add(10)).unwrap();
                                r.set_name(id, Some("visited")).unwrap();
                            }
                            2 | 5 => {
                                let order = r.registered_groups();
                                let pos = order.iter().position(|i| i == id).unwrap();
                                if let Some(next) = order.get(pos + 1) {
                                    if row.a == 2 {
                                        let n = logical_id(&nodes, next);
                                        r.free(next).unwrap();
                                        nodes[n] = None;
                                    } else {
                                        r.unregister(next).unwrap();
                                    }
                                }
                            }
                            3 => {
                                if nodes[7].is_none() {
                                    nodes[7] = Some(r.allocate(desc(4), 77).unwrap());
                                }
                            }
                            4 => {
                                let g = r.group(id).unwrap();
                                let found = r
                                    .find_group(&g.descriptor().group_name_prefix, g.index())
                                    .unwrap();
                                trace.push(logical_id(&nodes, &found) as i32);
                                r.for_each_group(|_, id| {
                                    trace.push(100 + logical_id(&nodes, id) as i32);
                                    0
                                })
                                .unwrap();
                            }
                            6 => {
                                let n = logical_id(&nodes, id);
                                r.free(id).unwrap();
                                nodes[n] = None;
                            }
                            0 => {}
                            _ => panic!(),
                        }
                        row.b as i32
                    })
                    .unwrap(),
                );
            }
            "counters" => {
                rc = i64::from(
                    r.for_each_counter(id.as_ref().unwrap(), |r, id, i| {
                        trace.push(i as i32);
                        if row.a == 1 && i == 0 {
                            r.reset(id).unwrap();
                        }
                        if row.a == 2 {
                            let g = r.group(id).unwrap();
                            let name = &g.descriptor().counters[i as usize].name;
                            trace.push(g.counter_by_name(name).unwrap() as i32);
                            r.for_each_counter(id, |_, _, i| {
                                trace.push(100 + i as i32);
                                0
                            })
                            .unwrap();
                        }
                        row.b as i32
                    })
                    .unwrap(),
                );
            }
            _ => panic!("unknown op {}", row.op),
        }
        assert_eq!(rc, row.result, "return {scenario}/{} {}", row.step, row.op);
        assert_eq!(trace, row.trace, "callback trace {scenario}/{}", row.step);
        assert_eq!(
            r.ticks(),
            row.after_tick,
            "tick phase {scenario}/{}",
            row.step
        );
        assert_eq!(
            snapshot(&r, &nodes),
            row.after,
            "after {scenario}/{} {}",
            row.step,
            row.op
        );
        replayed += 1;
    }
    assert_eq!(replayed, 1911);
}
#[test]
fn stage7_ownership_handles_and_safe_restrictions() {
    let mut r = RateCtrRuntime::new();
    let mut d = desc(0);
    let id = r.allocate(d.clone(), 2).unwrap();
    d.group_name_prefix = "edited".into();
    d.counters[0].name = "changed".into();
    d.class_id = 17;
    assert_eq!(r.group(&id).unwrap().descriptor(), &desc(0));
    let mut name = "report".to_owned();
    r.set_name(&id, Some(&name)).unwrap();
    name.clear();
    assert_eq!(r.group(&id).unwrap().name(), Some("report"));
    r.set_index(&id, 99).unwrap();
    assert_eq!(r.find_group("g", 99), Some(id.clone()));
    assert_eq!(
        RateCtrRuntime::new().group(&id).unwrap_err(),
        RateCtrError::InvalidHandle
    );
    let before = snapshot(&r, &[Some(id.clone())]);
    assert_eq!(r.process_expirations(0), Err(RateCtrError::ZeroExpirations));
    assert_eq!(r.ticks(), 0);
    assert_eq!(snapshot(&r, &[Some(id.clone())]), before);
    assert_eq!(r.add(&id, 3, 1), Err(RateCtrError::CounterOutOfRange));
    assert_eq!(snapshot(&r, &[Some(id.clone())]), before);
    assert_eq!(
        r.set_name(&id, Some("x\0y")),
        Err(RateCtrError::EmbeddedNul)
    );
    assert_eq!(snapshot(&r, &[Some(id.clone())]), before);
    let mut bad = desc(0);
    bad.group_description = Some("x\0y".into());
    assert_eq!(r.allocate(bad, 1), Err(RateCtrError::EmbeddedNul));
    assert_eq!(snapshot(&r, &[Some(id.clone())]), before);
    assert_eq!(
        r.for_each_group(|r, id| {
            r.free(id).unwrap();
            0
        }),
        Err(RateCtrError::CurrentGroupRemoved)
    );
    assert_eq!(r.group(&id).unwrap_err(), RateCtrError::InvalidHandle);
    let reused = r.allocate(desc(0), 99).unwrap();
    assert_ne!(id, reused);
    assert!(r.group(&id).is_err());
    assert!(r.group(&reused).is_ok());
    assert_eq!(
        r.for_each_counter(&reused, |r, id, _| {
            r.free(id).unwrap();
            0
        }),
        Err(RateCtrError::CurrentGroupRemoved)
    );
    let id = r.allocate(desc(0), 0).unwrap();
    assert_eq!(
        r.for_each_group(|r, id| {
            r.unregister(id).unwrap();
            0
        }),
        Err(RateCtrError::CurrentGroupRemoved)
    );
    assert!(r.group(&id).is_ok());
    assert!(r.find_group("g", 0).is_none());
    r.free(&id).unwrap();
    let id = r.allocate(desc(0), 0).unwrap();
    assert_eq!(
        r.for_each_counter(&id, |r, id, _| {
            r.free(id).unwrap();
            -9
        }),
        Ok(-9)
    );
}
#[derive(Clone)]
struct FakeClock {
    now: TimeSpec,
    fail: bool,
}
impl Clock for FakeClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        panic!("rollup driver must use monotonic time")
    }
    fn clock_gettime(&mut self, id: ClockId) -> Result<TimeSpec, TimeError> {
        assert_eq!(id, ClockId::MONOTONIC);
        if self.fail {
            Err(TimeError::SystemError(5))
        } else {
            Ok(self.now)
        }
    }
}
#[test]
fn stage7_periodic_monotonic_driver_and_retained_failure_state() {
    let mut r = RateCtrRuntime::new();
    let id = r.allocate(desc(0), 0).unwrap();
    let mut d = RateCtrPeriodicDriver::new(FakeClock {
        now: TimeSpec::new(10, 250_000_000),
        fail: false,
    });
    assert_eq!(d.update(&mut r), Ok(0));
    d.start().unwrap();
    d.clock.now = TimeSpec::new(11, 249_999_999);
    assert_eq!(d.update(&mut r), Ok(0));
    d.start().unwrap();
    r.add(&id, 0, 17).unwrap();
    d.clock.now = TimeSpec::new(11, 250_000_000);
    assert_eq!(d.update(&mut r), Ok(1));
    assert_eq!(r.group(&id).unwrap().counter(0).unwrap().intv[0].rate, 17);
    r.add(&id, 0, 3).unwrap();
    d.clock.now = TimeSpec::new(14, 750_000_000);
    assert_eq!(d.update(&mut r), Ok(3));
    assert_eq!(r.ticks(), 4);
    assert_eq!(r.group(&id).unwrap().counter(0).unwrap().intv[0].rate, 0);
    d.clock.fail = true;
    assert_eq!(
        d.update(&mut r),
        Err(RateCtrError::Clock(TimeError::SystemError(5)))
    );
    assert_eq!(r.ticks(), 4);
    d.clock.fail = false;
    d.clock.now = TimeSpec::new(14, 0);
    assert_eq!(d.update(&mut r), Err(RateCtrError::ClockWentBackwards));
    assert_eq!(r.ticks(), 4);
    d.clock.now = TimeSpec::new(15, 250_000_000);
    assert_eq!(d.update(&mut r), Ok(1));
    assert_eq!(r.ticks(), 5);
    d.stop();
    assert!(!d.is_started());
    d.clock.now = TimeSpec::new(100, 0);
    d.start().unwrap();
    assert_eq!(d.update(&mut r), Ok(0));
    assert_eq!(r.ticks(), 5);
    d.stop();
    d.clock.fail = true;
    assert!(d.start().is_err());
    assert!(!d.is_started());
    d.clock.fail = false;
    d.clock.now = TimeSpec::new(i64::MAX, 0);
    assert!(d.start().is_err());
    assert!(!d.is_started());
    d.clock.now = TimeSpec::new(i64::MAX - 1, 0);
    d.start().unwrap();
    d.clock.now = TimeSpec::new(i64::MAX, 0);
    assert!(d.update(&mut r).is_err());
    assert_eq!(r.ticks(), 5);
}
