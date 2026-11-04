use crate::strrb_fixture::{self, Event, Record};
use osmocore_rs::compat::strrb as compat;
use osmocore_rs::strrb::{MAX_STRING_BYTES, StringRing};
use std::collections::HashMap;

const FIXTURE: &str = include_str!("../compatibility/vectors/strrb/strrb_vectors.ndjson");

fn records() -> Vec<Record> {
    strrb_fixture::validate(FIXTURE)
}

#[test]
fn fixture_is_well_formed() {
    strrb_fixture::check_corruptions(FIXTURE);
}

#[test]
fn constant_matches_c() {
    let constant = records().into_iter().find_map(|record| match record {
        Record::Constant { name, value } if name == "RB_MAX_MESSAGE_SIZE" => Some(value),
        _ => None,
    });
    assert_eq!(constant, Some(compat::RB_MAX_MESSAGE_SIZE as i64));
    assert_eq!(MAX_STRING_BYTES + 1, compat::RB_MAX_MESSAGE_SIZE);
}

#[test]
fn compat_matches_c() {
    let mut rings: HashMap<String, compat::OsmoStrrb> = HashMap::new();
    let mut aliased = 0;
    for record in records() {
        let Record::Event { object, event, .. } = record else {
            continue;
        };
        if let Event::Create { requested, size } = event {
            let ring = compat::osmo_strrb_create(requested);
            assert_eq!(ring.size(), size, "{object}");
            rings.insert(object, ring);
            continue;
        }
        let ring = rings.get_mut(&object).expect("rings start with a create");
        match event {
            Event::Create { .. } => unreachable!("handled above"),
            Event::Add { text, rc } => {
                assert_eq!(compat::osmo_strrb_add(ring, &text), rc, "{object}");
            }
            Event::State {
                start,
                end,
                elements,
                empty,
                valid,
            } => {
                assert_eq!((ring.start(), ring.end()), (start, end), "{object}");
                assert_eq!(compat::osmo_strrb_elements(ring), elements, "{object}");
                assert_eq!(compat::osmo_strrb_is_empty(ring), empty, "{object}");
                for (bufi, expected) in valid.into_iter().enumerate() {
                    let actual = compat::_osmo_strrb_is_bufindex_valid(ring, bufi as u32);
                    assert_eq!(actual, expected, "{object} slot {bufi}");
                }
            }
            Event::Nth { index, text } => {
                let actual = compat::osmo_strrb_get_nth(ring, index);
                assert_eq!(actual, text.as_deref(), "{object} index {index}");
                if actual.is_some() && index as usize >= compat::osmo_strrb_elements(ring) {
                    aliased += 1;
                }
            }
        }
    }
    assert!(aliased > 0, "the recording shows indexes past the newest");
}

#[test]
fn preferred_api_matches_c_up_to_the_newest_string() {
    let mut rings: HashMap<String, StringRing> = HashMap::new();
    for record in records() {
        let Record::Event { object, event, .. } = record else {
            continue;
        };
        if let Event::Create { size, .. } = event {
            let capacity = usize::from(size).saturating_sub(1);
            rings.insert(object, StringRing::new(capacity));
            continue;
        }
        let ring = rings.get_mut(&object).expect("rings start with a create");
        match event {
            Event::Create { .. } => unreachable!("handled above"),
            Event::Add { text, rc } => assert_eq!(ring.push(&text), rc == 1, "{object}"),
            Event::State {
                elements, empty, ..
            } => {
                assert_eq!(ring.len(), elements, "{object}");
                assert_eq!(ring.is_empty(), empty, "{object}");
                assert_eq!(ring.iter().len(), elements, "{object}");
            }
            Event::Nth { index, text } => {
                let index = index as usize;
                if index < ring.len() {
                    assert_eq!(ring.get(index), text.as_deref(), "{object} index {index}");
                } else {
                    assert_eq!(ring.get(index), None, "{object} index {index}");
                }
            }
        }
    }
}
