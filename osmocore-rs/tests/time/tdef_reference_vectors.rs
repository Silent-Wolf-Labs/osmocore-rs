use super::tdef_fixture;
use osmocore_rs::compat::tdef::OSMO_TDEF_UNIT_NAMES;
use osmocore_rs::compat::tdef::{
    osmo_tdef_for_each, osmo_tdef_get, osmo_tdef_get_entry, osmo_tdef_get_entry_mut,
    osmo_tdef_get_state_timeout, osmo_tdef_groups_for_each, osmo_tdef_range_str_buf, osmo_tdef_set,
    osmo_tdef_set_status, osmo_tdef_unit_name, osmo_tdef_val_in_range, osmo_tdefs_reset,
};
const TEXT: &str = include_str!("../compatibility/vectors/tdef/tdef_vectors.ndjson");
const REFERENCE: &str = include_str!("../compatibility/vectors/tdef/tdef_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/tdef/tdef_generator_provenance.json");
const DIAGNOSTICS: &str = include_str!("../compatibility/vectors/tdef/tdef_diagnostics.json");
#[test]
fn tdef_evidence_integrity() {
    tdef_fixture::checks::integrity(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}
#[test]
fn tdef_evidence_corruptions() {
    tdef_fixture::checks::corruptions(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}

use osmocore_rs::tdef::*;
use tdef_fixture::{signed as s, unsigned as u};

fn lookup_table() -> Vec<TDef<'static>> {
    let ids = [0, -7, 7, 7, i32::MIN, i32::MAX, 0, 123];
    let mut table: Vec<_> = ids
        .into_iter()
        .enumerate()
        .map(|(i, id)| TDef {
            id,
            default_val: i as u64 + 5,
            val: i as u64 + 91,
            ..TDef::default()
        })
        .collect();
    table[0].desc = Some("");
    table[6] = TDef {
        unit: TDefUnit::US,
        val: 97,
        min_val: 99,
        max_val: 100,
        ..TDef::default()
    };
    table[7].default_val = 11;
    table.push(TDef::default());
    table
}
fn traversal_table() -> Vec<TDef<'static>> {
    vec![
        TDef {
            id: 1,
            ..TDef::default()
        },
        TDef {
            default_val: 1,
            ..TDef::default()
        },
        TDef {
            desc: Some(""),
            ..TDef::default()
        },
        TDef {
            unit: TDefUnit::US,
            val: 99,
            min_val: 1,
            max_val: 2,
            ..TDef::default()
        },
        TDef {
            id: 2,
            ..TDef::default()
        },
        TDef::default(),
    ]
}
fn values(table: &[TDef<'_>]) -> String {
    table
        .iter()
        .map(|t| t.val.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
fn hex(buf: &[u8]) -> String {
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn tdef_c_conformance_replay() {
    let rows = tdef_fixture::validate(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut lookup = lookup_table();
    let mut preferred_lookup = lookup.clone();
    let traversal = traversal_table();
    for (number, row) in rows.iter().enumerate() {
        let id = || s(row, "id") as i32;
        let unit = |key| TDefUnit(s(row, key) as i32);
        let bounded = || TDef {
            id: 1,
            val: 73,
            min_val: u(row, "min"),
            max_val: u(row, "max"),
            ..TDef::default()
        };
        match row["type"].as_str() {
            "meta" => {}
            "conversion" | "invalid_safe" | "zero_units" => {
                let mut source = [
                    TDef {
                        id: 7,
                        unit: unit("from"),
                        val: u(row, "value"),
                        ..TDef::default()
                    },
                    TDef::default(),
                ];
                assert_eq!(
                    osmo_tdef_get(Some(&source), 7, unit("to"), -1).unwrap(),
                    u(row, "get"),
                    "record {number}"
                );
                assert_eq!(
                    TDefTable::new(&mut source).unwrap().get(7, unit("to"), -1),
                    Ok(u(row, "get")),
                    "preferred record {number}"
                );
                let mut dest = [
                    TDef {
                        id: 7,
                        unit: unit("to"),
                        val: u(row, "before"),
                        ..TDef::default()
                    },
                    TDef::default(),
                ];
                let mut preferred_dest = dest.clone();
                let status = TDefTable::new(&mut preferred_dest)
                    .unwrap()
                    .set(7, u(row, "value"), unit("from"))
                    .map_or_else(
                        |error| osmocore_rs::compat::tdef::status(&error).unwrap(),
                        |()| 0,
                    );
                assert_eq!(i64::from(status), s(row, "ret"));
                assert_eq!(preferred_dest[0].val, u(row, "after"));
                assert_eq!(
                    osmo_tdef_set_status(Some(&mut dest), 7, u(row, "value"), unit("from")).unwrap()
                        as i64,
                    s(row, "ret")
                );
                assert_eq!(dest[0].val, u(row, "after"));
            }
            "lookup" => {
                let mut preferred = TDefTable::new(&mut preferred_lookup).unwrap();
                assert_eq!(
                    preferred.get(id(), TDefUnit::US, s(row, "fallback")),
                    Ok(u(row, "get"))
                );
                let status = preferred.set(id(), 42, TDefUnit::S).map_or_else(
                    |error| osmocore_rs::compat::tdef::status(&error).unwrap(),
                    |()| 0,
                );
                assert_eq!(i64::from(status), s(row, "ret"));
                assert_eq!(values(&preferred_lookup[..8]), row["values"]);
                let entries = osmo_tdef_for_each(Some(&lookup)).unwrap();
                let entry = osmo_tdef_get_entry(Some(&lookup), id()).unwrap();
                let index = entry
                    .map(|t| {
                        entries
                            .iter()
                            .position(|candidate| std::ptr::eq(candidate, t))
                            .unwrap() as i64
                    })
                    .unwrap_or(-1);
                assert_eq!(index, s(row, "index"));
                assert_eq!(
                    osmo_tdef_get(Some(&lookup), id(), TDefUnit::US, s(row, "fallback")).unwrap(),
                    u(row, "get")
                );
                assert_eq!(
                    osmo_tdef_set_status(Some(&mut lookup), id(), 42, TDefUnit::S).unwrap() as i64,
                    s(row, "ret")
                );
                assert_eq!(values(&lookup[..8]), row["values"]);
            }
            "reset" => {
                TDefTable::new(&mut preferred_lookup)
                    .unwrap()
                    .reset()
                    .unwrap();
                assert_eq!(values(&preferred_lookup[..8]), row["values"]);
                osmo_tdefs_reset(Some(&mut lookup)).unwrap();
                assert_eq!(values(&lookup[..8]), row["values"]);
            }
            "missing" => {
                assert_eq!(
                    osmo_tdef_get(None, 0, unit("unit"), s(row, "fallback")).unwrap(),
                    u(row, "get")
                );
                assert_eq!(
                    i64::from(osmo_tdef_get_entry(None, 0).unwrap().is_some()),
                    s(row, "entry")
                );
                assert_eq!(
                    osmo_tdef_set_status(None, 0, 1, unit("unit")).unwrap() as i64,
                    s(row, "ret")
                );
                osmo_tdefs_reset(None).unwrap();
            }
            "traversal" => {
                assert_eq!(
                    osmo_tdef_for_each(Some(&traversal)).unwrap().len() as i64,
                    s(row, "entries")
                );
                assert_eq!(
                    i64::from(osmo_tdef_get_entry(Some(&traversal), 2).unwrap().is_some()),
                    s(row, "hidden")
                );
            }
            "groups" => {
                let groups = [
                    TDefGroup {
                        tdefs: Some(&traversal),
                        ..TDefGroup::default()
                    },
                    TDefGroup {
                        name: Some(""),
                        tdefs: Some(&traversal[3..]),
                        ..TDefGroup::default()
                    },
                    TDefGroup {
                        name: Some("stop"),
                        desc: Some("nonempty"),
                        ..TDefGroup::default()
                    },
                    TDefGroup {
                        tdefs: Some(&traversal),
                        ..TDefGroup::default()
                    },
                    TDefGroup::default(),
                ];
                assert_eq!(
                    osmo_tdef_groups_for_each(Some(&groups)).unwrap().len() as i64,
                    s(row, "entries")
                );
            }
            "traversal_null" => assert_eq!(
                osmo_tdef_for_each(None).unwrap().len() as i64,
                s(row, "entries")
            ),
            "groups_null" => assert_eq!(
                osmo_tdef_groups_for_each(None).unwrap().len() as i64,
                s(row, "entries")
            ),
            "range" => {
                let mut table = [bounded(), TDef::default()];
                assert_eq!(
                    i64::from(osmo_tdef_val_in_range(&table[0], u(row, "value"))),
                    s(row, "in")
                );
                assert_eq!(
                    i64::from(table[0].value_in_range(u(row, "value"))),
                    s(row, "in")
                );
                assert_eq!(table[0].val, u(row, "before"));
                assert_eq!(
                    osmo_tdef_set_status(Some(&mut table), 1, u(row, "value"), unit("from"))
                        .unwrap() as i64,
                    s(row, "ret")
                );
                assert_eq!(table[0].val, u(row, "after"));
            }
            "reset_range" => {
                let mut table = [
                    TDef {
                        default_val: u(row, "default"),
                        ..bounded()
                    },
                    TDef::default(),
                ];
                assert_eq!(table[0].val, u(row, "before"));
                osmo_tdefs_reset(Some(&mut table)).unwrap();
                assert_eq!(table[0].val, u(row, "after"));
            }
            "format" => {
                let mut buf = [0xa5; 64];
                assert_eq!(hex(&buf), row["initial"]);
                let cap = s(row, "capacity") as usize;
                let range = bounded().range_string();
                assert_eq!(range.len() as i64, s(row, "ret"));
                // Compare C's bytes only when its buffer holds the full text and NUL.
                if cap > range.len() {
                    let frozen = row["final"].as_bytes();
                    let text_bytes: Vec<_> = frozen
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .take(range.len())
                        .map(|pair| {
                            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()
                        })
                        .collect();
                    assert_eq!(range.as_bytes(), text_bytes);
                }
                assert_eq!(
                    osmo_tdef_range_str_buf(&mut buf[..cap], &bounded()) as i64,
                    s(row, "ret")
                );
                if cap == 0 {
                    assert_eq!(buf, [0xa5; 64]);
                    assert_eq!(row["final"], format!("00{}", "a5".repeat(63)));
                } else {
                    assert_eq!(hex(&buf), row["final"]);
                }
            }
            "name" => assert_eq!(osmo_tdef_unit_name(unit("unit")), row["name"]),
            "name_table" => {
                let (unit, name) = OSMO_TDEF_UNIT_NAMES[s(row, "index") as usize];
                assert_eq!(unit.0 as i64, s(row, "unit"));
                assert_eq!(i64::from(name.is_some()), s(row, "present"));
                assert_eq!(name.unwrap_or(""), row["name"]);
            }
            "state" => {
                let state = s(row, "state") as usize;
                let mut table = [StateTimeout::default(); 32];
                table[state] = StateTimeout {
                    id: id(),
                    keep_timer: s(row, "keep") != 0,
                };
                let found = osmo_tdef_get_state_timeout(state as u32, &table).unwrap();
                assert_eq!(i64::from(found.is_some()), s(row, "present"));
                let index = found
                    .map(|t| {
                        table
                            .iter()
                            .position(|candidate| std::ptr::eq(candidate, t))
                            .unwrap() as i64
                    })
                    .unwrap_or(-1);
                assert_eq!(index, s(row, "index"));
                let out = found.copied().unwrap_or_default();
                assert_eq!(out.id as i64, s(row, "out_id"));
                assert_eq!(i64::from(out.keep_timer), s(row, "out_keep"));
            }
            kind => panic!("unreplayed record {number}: {kind}"),
        }
    }
}

// Parse only the already fingerprint-validated diagnostic records, with no
// dependencies or numerical conversion model in the shared evidence reader.
fn diagnostic_probes() -> Vec<tdef_fixture::Record> {
    tdef_fixture::validate(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut probes = Vec::new();
    let mut probe = tdef_fixture::Record::new();
    for line in DIAGNOSTICS.lines() {
        let line = line.trim().trim_end_matches(',');
        if line == "}" && !probe.is_empty() {
            probes.push(std::mem::take(&mut probe));
        } else if let Some((key, value)) = line.split_once(": ") {
            let key = key.trim_matches('"');
            if key == "probe" || !probe.is_empty() {
                probe.insert(key.to_owned(), value.trim_matches('"').to_owned());
            }
        }
    }
    probes
}

#[test]
fn tdef_diagnostic_safety_replay() {
    let probes = diagnostic_probes();
    assert_eq!(probes.len(), 95);
    for probe in probes {
        match probe["category"].as_str() {
            "invalid-default" => {
                let mut table = vec![
                    TDef {
                        id: 1,
                        default_val: 5,
                        val: 91,
                        ..TDef::default()
                    },
                    TDef {
                        id: 2,
                        default_val: 11,
                        val: 92,
                        min_val: 5,
                        max_val: 10,
                        ..TDef::default()
                    },
                    TDef {
                        id: 3,
                        default_val: 7,
                        val: 93,
                        ..TDef::default()
                    },
                    TDef::default(),
                ];
                let index = match probe["probe"].as_str() {
                    "reset-invalid" => 1,
                    "reset-low" => {
                        table[1].default_val = 7;
                        table[1].min_val = 0;
                        table[1].max_val = 0;
                        table[2].default_val = 2;
                        table[2].min_val = 3;
                        table[2].max_val = 10;
                        2
                    }
                    "reset-inverted" => {
                        table[0].min_val = 10;
                        table[0].max_val = 2;
                        table[1].default_val = 7;
                        table[1].min_val = 0;
                        table[1].max_val = 0;
                        0
                    }
                    other => panic!("unknown reset {other}"),
                };
                let t = &table[index];
                let expected = TDefError::InvalidDefault {
                    index,
                    id: t.id,
                    value: t.default_val,
                    min: t.min_val,
                    max: t.max_val,
                };
                assert_eq!(osmo_tdefs_reset(Some(&mut table)), Err(expected));
                assert_eq!(values(&table[..3]), probe["values"]);
            }
            "assertion" => match probe["probe"].as_str() {
                "missing-negative" => assert_eq!(
                    osmo_tdef_get(None, 123, TDefUnit::S, -1),
                    Err(TDefError::NegativeFallback {
                        id: 123,
                        fallback: -1
                    })
                ),
                name @ ("state-32" | "state-max") => {
                    let state = if name == "state-32" { 32 } else { u32::MAX };
                    assert_eq!(
                        osmo_tdef_get_state_timeout(state, &[StateTimeout::default(); 32]),
                        Err(TDefError::InvalidState { state })
                    );
                }
                other => panic!("unknown assertion {other}"),
            },
            "division-by-zero" => {
                let from = TDefUnit(s(&probe, "from") as i32);
                let to = TDefUnit(s(&probe, "to") as i32);
                let error = TDefError::InvalidConversion { from, to };
                assert_eq!(osmocore_rs::compat::tdef::status(&error), None);
                if s(&probe, "setting") == 0 {
                    let table = [
                        TDef {
                            id: 1,
                            val: 1,
                            unit: from,
                            ..TDef::default()
                        },
                        TDef::default(),
                    ];
                    assert_eq!(osmo_tdef_get(Some(&table), 1, to, -1), Err(error));
                } else {
                    let mut table = [
                        TDef {
                            id: 1,
                            val: 73,
                            unit: to,
                            ..TDef::default()
                        },
                        TDef::default(),
                    ];
                    let before = table.clone();
                    assert_eq!(
                        osmo_tdef_set_status(Some(&mut table), 1, 1, from),
                        Err(error)
                    );
                    assert_eq!(table, before);
                }
            }
            "null-buffer-write" => {
                assert_eq!(
                    osmo_tdef_range_str_buf(&mut [], &TDef::default()),
                    "[0 .. inf]".len()
                );
            }
            other => panic!("unreplayed diagnostic {other}"),
        }
    }
}

#[test]
fn tdef_bounded_adapters_and_typed_statuses() {
    assert_eq!(
        osmo_tdef_for_each(Some(&[])),
        Err(TDefError::MissingTerminator)
    );
    let mut table = [TDef {
        id: 1,
        val: 73,
        default_val: 5,
        ..TDef::default()
    }];
    let before = table.clone();
    assert_eq!(
        osmo_tdefs_reset(Some(&mut table)),
        Err(TDefError::MissingTerminator)
    );
    assert_eq!(table, before);
    assert_eq!(
        osmo_tdef_get_entry(Some(&table), 1),
        Err(TDefError::MissingTerminator)
    );
    assert_eq!(
        osmo_tdef_get_entry_mut(Some(&mut table), 1),
        Err(TDefError::MissingTerminator)
    );
    assert_eq!(
        osmo_tdef_set(Some(&mut table), 1, 9, TDefUnit::S),
        Err(TDefError::MissingTerminator)
    );
    assert_eq!(table, before);
    assert_eq!(
        osmo_tdef_groups_for_each(Some(&[])).unwrap_err(),
        TDefError::MissingGroupTerminator
    );
    let groups = [TDefGroup {
        tdefs: Some(&[]),
        ..TDefGroup::default()
    }];
    assert_eq!(
        osmo_tdef_groups_for_each(Some(&groups)).unwrap_err(),
        TDefError::MissingGroupTerminator
    );
    let groups = [
        TDefGroup {
            tdefs: Some(&[]),
            ..TDefGroup::default()
        },
        TDefGroup::default(),
    ];
    assert_eq!(osmo_tdef_groups_for_each(Some(&groups)).unwrap().len(), 1);
    assert_eq!(
        osmo_tdef_get(None, 99, TDefUnit(-1), i64::MIN),
        Err(TDefError::NegativeFallback {
            id: 99,
            fallback: i64::MIN
        })
    );
    assert_eq!(
        osmo_tdef_set(None, 99, 0, TDefUnit(-1)),
        Err(TDefError::MissingDefinition { id: 99 })
    );
    assert_eq!(
        osmocore_rs::compat::tdef::status(&TDefError::MissingDefinition { id: 99 }),
        Some(-17)
    );
    let mut table = [
        TDef {
            id: 1,
            min_val: 5,
            max_val: 10,
            val: 7,
            ..TDef::default()
        },
        TDef::default(),
    ];
    let error = TDefError::RejectedValue {
        id: 1,
        value: 4,
        min: 5,
        max: 10,
    };
    assert_eq!(
        osmo_tdef_set(Some(&mut table), 1, 4, TDefUnit::S),
        Err(error.clone())
    );
    assert_eq!(osmocore_rs::compat::tdef::status(&error), Some(-34));
    assert_eq!(table[0].val, 7);
    let entry = osmo_tdef_get_entry_mut(Some(&mut table), 1)
        .unwrap()
        .unwrap();
    entry.val = 9;
    assert_eq!(osmo_tdef_get(Some(&table), 1, TDefUnit::S, -1), Ok(9));
}
