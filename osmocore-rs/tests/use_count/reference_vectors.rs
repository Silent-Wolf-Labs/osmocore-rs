use crate::use_count_fixture::{self, Event, Record, State};
use osmocore_rs::base::use_count::{UseChange, UseCount};
use osmocore_rs::compat::use_count as compat;
use std::collections::HashMap;

const FIXTURE: &str = include_str!("../compatibility/vectors/use_count/use_count_vectors.ndjson");
const GUARD: u8 = 0xa5;

fn records() -> Vec<Record> {
    use_count_fixture::validate(FIXTURE)
}

fn entries_text(uc: &UseCount) -> String {
    uc.entries()
        .map(|(token, count)| format!("{}={count};", token.unwrap_or("<null>")))
        .collect()
}

fn assert_state(uc: &UseCount, state: &State, what: &str) {
    assert_eq!(uc.total(), state.total, "{what}");
    assert_eq!(uc.to_string(), state.text, "{what}");
    assert_eq!(entries_text(uc), state.entries, "{what}");
    assert_eq!(compat::osmo_use_count_to_str_c(uc), state.text, "{what}");
}

fn assert_to_str(uc: &UseCount, n: usize, rc: i32, text: &str) {
    let mut buffer = vec![GUARD; n];
    assert_eq!(compat::osmo_use_count_to_str_buf(&mut buffer, uc), rc);
    if n > 0 {
        assert_eq!(&buffer[..text.len()], text.as_bytes(), "size {n}");
        assert_eq!(buffer[text.len()], 0, "size {n}");
        assert!(buffer[text.len() + 1..].iter().all(|&byte| byte == GUARD));
    }
}

#[derive(Default)]
struct Object {
    uc: UseCount,
    policy: String,
    releasing: bool,
    released: bool,
}

/// Walks the recorded events, running the same callbacks as the harness.
struct Replay {
    events: std::vec::IntoIter<(String, Event)>,
    callbacks: usize,
}

impl Replay {
    fn next_event(&mut self, object: &str) -> Event {
        let (owner, event) = self.events.next().expect("the recording has more events");
        assert_eq!(owner, object, "events of an object stay together in a call");
        event
    }

    fn call(&mut self, name: &str, object: &mut Object, token: Option<&str>, change: i32) -> i32 {
        let mut uc = std::mem::take(&mut object.uc);
        let rc = if object.policy == "none" {
            compat::osmo_use_count_get_put(Some(&mut uc), token, change, None)
        } else {
            let mut on_change =
                |uc: &mut UseCount, changed: UseChange| self.on_callback(name, object, uc, changed);
            compat::osmo_use_count_get_put(Some(&mut uc), token, change, Some(&mut on_change))
        };
        let Event::Result {
            rc: recorded,
            by,
            state,
        } = self.next_event(name)
        else {
            panic!("{name}: expected a result");
        };
        assert_eq!(rc, recorded, "{name} {token:?} {change}");
        assert_eq!(
            compat::osmo_use_count_by(Some(&uc), token),
            by,
            "{name} {token:?}"
        );
        assert_state(&uc, &state, name);
        object.uc = uc;
        rc
    }

    fn on_callback(
        &mut self,
        name: &str,
        object: &mut Object,
        uc: &mut UseCount,
        changed: UseChange,
    ) -> i32 {
        self.callbacks += 1;
        let Event::Callback {
            token,
            old,
            new,
            state,
        } = self.next_event(name)
        else {
            panic!("{name}: expected a callback");
        };
        let current = uc.entries().nth(changed.entry).map(|(token, _)| token);
        assert_eq!(current, Some(token.as_deref()), "{name}");
        assert_eq!((changed.old, changed.new), (old, new), "{name}");
        assert_state(uc, &state, name);

        match object.policy.as_str() {
            "fail" => -5,
            "clamp" if new > 1 => {
                assert!(uc.set_entry_count(changed.entry, 1));
                -34
            }
            "free_zero" if new == 0 => {
                assert!(uc.remove_entry(changed.entry));
                0
            }
            "foo" => self.foo_callback(name, object, uc, changed, token.as_deref()),
            _ => 0,
        }
    }

    /// What upstream's test does: one each of two tokens, then a two-step release.
    fn foo_callback(
        &mut self,
        name: &str,
        object: &mut Object,
        uc: &mut UseCount,
        changed: UseChange,
        token: Option<&str>,
    ) -> i32 {
        if matches!(token, Some("barring" | "releasing")) && changed.new > 1 {
            assert!(uc.set_entry_count(changed.entry, 1));
            return -34;
        }
        if uc.total() != 0 {
            return 0;
        }
        if object.releasing {
            object.released = true;
            return 0;
        }
        object.releasing = true;
        assert_eq!(
            self.next_event(name),
            Event::Call {
                token: Some("releasing".to_owned()),
                change: 1
            }
        );
        std::mem::swap(&mut object.uc, uc);
        let rc = self.call(name, object, Some("releasing"), 1);
        std::mem::swap(&mut object.uc, uc);
        assert_eq!(rc, 0);
        0
    }
}

#[test]
fn fixture_is_well_formed() {
    use_count_fixture::check_corruptions(FIXTURE);
}

#[test]
fn events_with_callbacks_match_c() {
    let mut no_object = 0;
    let mut events = Vec::new();
    for record in records() {
        match record {
            Record::Event { object, event, .. } => events.push((object, event)),
            Record::NoObject { rc, total, by } => {
                no_object += 1;
                let result = compat::osmo_use_count_get_put(None, Some("a"), 1, None);
                assert_eq!(result, rc);
                assert_eq!(compat::osmo_use_count_total(None), total);
                assert_eq!(compat::osmo_use_count_by(None, Some("a")), by);
            }
        }
    }
    assert_eq!(no_object, 1);

    let mut objects: HashMap<String, Object> = HashMap::new();
    let mut replay = Replay {
        events: events.into_iter(),
        callbacks: 0,
    };
    while let Some((name, event)) = replay.events.next() {
        if let Event::New { policy, state } = &event {
            let object = Object {
                policy: policy.clone(),
                ..Object::default()
            };
            assert_state(&object.uc, state, &name);
            objects.insert(name, object);
            continue;
        }
        let object = objects
            .get_mut(&name)
            .expect("objects start with a new event");
        match event {
            Event::Call { token, change } => {
                replay.call(&name, object, token.as_deref(), change);
            }
            Event::Static { n, state } => {
                compat::osmo_use_count_make_static_entries(&mut object.uc, n);
                assert_state(&object.uc, &state, &name);
            }
            Event::Free { token, state } => {
                compat::osmo_use_count_free(&mut object.uc, token.as_deref());
                assert_state(&object.uc, &state, &name);
            }
            Event::ToStr { n, rc, text } => assert_to_str(&object.uc, n, rc, &text),
            other => panic!("{name}: unexpected {other:?}"),
        }
    }
    assert!(replay.callbacks > 0);
    let released = objects.values().filter(|object| object.released).count();
    assert_eq!(released, 3, "upstream's three objects finish their release");
}

#[test]
fn preferred_api_matches_c_without_callbacks() {
    let mut objects: HashMap<String, UseCount> = HashMap::new();
    let mut plain: HashMap<String, bool> = HashMap::new();
    let mut pending = None;
    let mut seen = 0;
    for record in records() {
        let Record::Event { object, event, .. } = record else {
            continue;
        };
        if let Event::New { policy, .. } = &event {
            plain.insert(object.clone(), policy == "none");
            objects.insert(object.clone(), UseCount::new());
        }
        if !plain[&object] {
            continue;
        }
        let uc = objects
            .get_mut(&object)
            .expect("objects start with a new event");
        match event {
            Event::New { state, .. } => assert_state(uc, &state, &object),
            Event::Call { token, change } => {
                let result = uc.change(token.as_deref(), change);
                pending = Some((token, change, result));
            }
            Event::Result { rc, by, state } => {
                let (token, change, result) = pending.take().expect("a call came first");
                assert_eq!(result.is_err(), rc == -34, "{object} {token:?} {change}");
                assert_eq!(rc == 0, result.is_ok(), "{object} {token:?} {change}");
                if let Ok(changed) = result {
                    assert_eq!(changed.new - changed.old, change, "{object} {token:?}");
                }
                assert_eq!(uc.count_of(token.as_deref()), by, "{object} {token:?}");
                assert_state(uc, &state, &object);
                seen += 1;
            }
            Event::Static { n, state } => {
                uc.add_idle_entries(n);
                assert_state(uc, &state, &object);
            }
            Event::Free { token, state } => {
                uc.remove(token.as_deref());
                assert_state(uc, &state, &object);
            }
            Event::ToStr { .. } => {}
            Event::Callback { .. } => panic!("{object}: a callback with none set"),
        }
    }
    assert!(seen > 0);
}
