use super::fsm_fixture;
use osmocore_rs::{
    fsm::*,
    time::{Clock, ClockId, ClockOverrides, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::TimerScheduler,
};
const TEXT: &str = include_str!("../compatibility/vectors/fsm_core/fsm_core_vectors.tsv");
const REFERENCE: &str = include_str!("../compatibility/vectors/fsm_core/fsm_core_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/fsm_core/fsm_core_generator_provenance.json");
const DIAGNOSTICS: &str =
    include_str!("../compatibility/vectors/fsm_core/fsm_core_diagnostics.json");
const SPEC: fsm_fixture::Evidence = fsm_fixture::Evidence {
    text_hash: 0xbbde5a5c6b3dc6c2,
    reference_hash: 0xcab6c5c42bd14581,
    reference_sha: "f572b2e98589d209217fdd17b117b79f1ad113b499a96898fd6c7c251d3d7fe0",
    generator_hash: 0x2d92181512c078c7,
    generator_sha: "9f5c41d6e60b9999ee511de720b6abd66eacf5c70a9b09a1f22436325bce38bd",
    diagnostics_hash: 0xb7a8b2b8e70d7d35,
    records: 1506,
    scenarios: 64,
    fields: 14,
    probes: 7,
};
#[test]
fn fsm_core_evidence() {
    fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    fsm_fixture::corruptions(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}
struct Source;
impl Clock for Source {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        Ok(TimeVal::new(100, 0))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        Err(TimeError::SystemError(5))
    }
}
#[derive(Default)]
struct Data {
    scenario: usize,
    mode: usize,
    step: usize,
    nested: bool,
    definition: Option<FsmDefinitionId>,
    trace: Vec<String>,
}
type R = FsmRuntime<ClockOverrides<Source>, Data>;
type S = TimerScheduler<ClockOverrides<Source>, R>;
fn record(s: &S, r: &mut R, op: &str, arg: i64, rc: i32) {
    let d = r.data.definition.as_ref().unwrap();
    let found = r.find_by_id(d, "node").unwrap();
    let name = r.find_by_name(d, "Evidence(node)").unwrap();
    let (state, t, active, ds, dus, term) = if let Some(id) = found.as_ref() {
        let i = r.instance(id).unwrap();
        let deadline = s.deadline(i.timer()).unwrap();
        (
            i.state as i32,
            i.timer_number,
            i32::from(s.pending(i.timer()).unwrap()),
            deadline.seconds,
            deadline.microseconds,
            i32::from(i.terminating),
        )
    } else {
        (-1, 0, 0, 0, 0, 0)
    };
    r.data.trace.push(format!(
        "{}\t{}\t{op}\t{arg}\t{rc}\t{state}\t{t}\t{active}\t{ds}\t{dus}\t{term}\t{}\t{}\t{}",
        r.data.scenario,
        r.data.step,
        i32::from(found.is_some()),
        i32::from(name.is_some()),
        r.instances(d).unwrap().len()
    ));
    r.data.step += 1;
}
fn status(r: Result<(), FsmError>) -> i32 {
    r.err()
        .map(|e| osmocore_rs::compat::fsm::status(&e))
        .unwrap_or(0)
}
fn cause(n: usize) -> TermCause {
    [
        TermCause::Parent,
        TermCause::Request,
        TermCause::Regular,
        TermCause::Error,
        TermCause::Timeout,
    ][n % 5]
}
fn action(s: &mut S, r: &mut R, id: FsmId, event: u32, data: &EventData<'_, ()>) {
    record(s, r, "action", event as i64, 0);
    if r.data.mode == 1 {
        let rc = status(r.state_change(s, &id, 1, 0, 7));
        record(s, r, "nested-chg", 1, rc);
    }
    if r.data.mode == 4 {
        r.term(s, &id, TermCause::Regular, data).unwrap();
        record(s, r, "after-action-term", 0, 0);
    }
}
fn allstate(s: &mut S, r: &mut R, id: FsmId, event: u32, data: &EventData<'_, ()>) {
    record(s, r, "allstate", event as i64, 0);
    if r.data.mode == 2 && !r.data.nested {
        r.data.nested = true;
        let rc = status(r.dispatch(s, Some(&id), 0, data));
        r.data.nested = false;
        record(s, r, "nested-dispatch", 0, rc);
    }
}
fn leave(s: &mut S, r: &mut R, id: FsmId, next: u32) {
    record(s, r, "leave", next as i64, 0);
    if r.data.mode == 7 && !r.data.nested {
        r.data.nested = true;
        let rc = status(r.state_change(s, &id, 2, 0, 9));
        r.data.nested = false;
        record(s, r, "leave-nested-chg", 2, rc);
    }
}
fn enter(s: &mut S, r: &mut R, id: FsmId, prev: u32) {
    record(s, r, "enter", prev as i64, 0);
    if r.data.mode == 3 {
        r.term(s, &id, TermCause::Request, &EventData::None)
            .unwrap();
        record(s, r, "after-enter-term", 0, 0);
    }
}
fn terminating(s: &mut S, r: &mut R, id: &FsmId) {
    let rc = status(r.dispatch(s, Some(id), 0, &EventData::None));
    record(s, r, "terminating-dispatch", 0, rc);
    let rc = status(r.state_change(s, id, 1, 0, 0));
    record(s, r, "terminating-chg", 1, rc);
    r.term(s, id, TermCause::Error, &EventData::None).unwrap();
    record(s, r, "terminating-term", 0, 0);
}
fn preterm(s: &mut S, r: &mut R, id: FsmId, cause: TermCause) {
    record(s, r, "preterm", cause as i64, 0);
    if r.data.mode == 5 {
        terminating(s, r, &id)
    }
}
fn cleanup(s: &mut S, r: &mut R, id: FsmId, cause: TermCause) {
    record(s, r, "cleanup", cause as i64, 0);
    if r.data.mode == 6 {
        terminating(s, r, &id)
    }
}
fn definition(callbacks: usize, permit: usize) -> Fsm<ClockOverrides<Source>, Data, ()> {
    Fsm {
        name: "Evidence".into(),
        states: (0..3)
            .map(|_| FsmState {
                name: Some("state".into()),
                in_event_mask: 3,
                out_state_mask: if permit == 1 { 7 } else { 5 },
                action: if callbacks & 1 != 0 {
                    Some(action)
                } else {
                    None
                },
                onenter: Some(enter),
                onleave: Some(leave),
            })
            .collect(),
        allstate_event_mask: 2,
        allstate_action: if callbacks & 2 != 0 {
            Some(allstate)
        } else {
            None
        },
        pre_term: Some(preterm),
        cleanup: Some(cleanup),
        timer_cb: None,
        event_names: None,
    }
}
#[test]
fn fsm_core_complete_c_replay() {
    fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut all = Vec::new();
    let mut scenario = 0;
    for mode in 0..8 {
        for callbacks in 0..4 {
            for permit in 0..2 {
                scenario += 1;
                let mut r = R::new(Data {
                    scenario,
                    mode,
                    ..Data::default()
                });
                let mut s = S::new(ClockOverrides::new(Source));
                let d = r.register(definition(callbacks, permit)).unwrap();
                r.data.definition = Some(d.clone());
                r.alloc(&mut s, &d, Some("node"), ()).unwrap();
                record(
                    &s,
                    &mut r,
                    "init",
                    (mode * 8 + callbacks * 2 + permit) as i64,
                    0,
                );
                let rc = status(r.dispatch(&mut s, None, 0, &EventData::None));
                record(&s, &mut r, "null-dispatch", 0, rc);
                for event in [0, 1, 2, 30] {
                    if let Some(id) = r.find_by_id(&d, "node").unwrap() {
                        let rc = status(r.dispatch(&mut s, Some(&id), event, &EventData::None));
                        record(&s, &mut r, "dispatch", event as i64, rc);
                    } else {
                        break;
                    }
                }
                for target in [1, 2, 2, 0, 30] {
                    if let Some(id) = r.find_by_id(&d, "node").unwrap() {
                        let rc = status(r.state_change(&mut s, &id, target, 0, 13));
                        record(&s, &mut r, "chg", target as i64, rc);
                    } else {
                        break;
                    }
                }
                if let Some(id) = r.find_by_id(&d, "node").unwrap() {
                    r.term(&mut s, &id, cause(scenario), &EventData::None)
                        .unwrap();
                    record(&s, &mut r, "term", (scenario % 5) as i64, 0);
                }
                record(&s, &mut r, "removed", 0, 0);
                all.extend(r.data.trace);
            }
        }
    }
    let expected = TEXT.lines().skip(1).collect::<Vec<_>>();
    assert_eq!(all, expected);
}

#[test]
fn fsm_handles_and_unsafe_leave_termination() {
    fn terminate_from_leave(s: &mut S, r: &mut R, id: FsmId, _: u32) {
        r.term(s, &id, TermCause::Request, &EventData::None)
            .unwrap();
    }
    let mut r = R::new(Data::default());
    let mut f = definition(0, 1);
    f.states[0].onleave = Some(terminate_from_leave);
    let d = r.register(f).unwrap();
    r.data.definition = Some(d.clone());
    let mut s = S::new(ClockOverrides::new(Source));
    let id = r.alloc(&mut s, &d, Some("node"), 17u64).unwrap();
    let mut other = R::new(Data::default());
    let other_d = other.register(definition(0, 1)).unwrap();
    let mut other_s = S::new(ClockOverrides::new(Source));
    let other_id = other
        .alloc(&mut other_s, &other_d, Some("node"), ())
        .unwrap();
    assert_ne!(id, other_id);
    assert_ne!(d, other_d);
    assert!(matches!(
        r.state_change(&mut other_s, &id, 1, 0, 0),
        Err(FsmError::Timer(_))
    ));
    assert_eq!(r.instance(&other_id).err(), Some(FsmError::InvalidHandle));
    assert!(matches!(
        r.alloc(&mut s, &other_d, Some("node"), ()),
        Err(FsmError::InvalidDefinition)
    ));
    assert_eq!(r.instance(&id).unwrap().private::<u64>(), Some(&17));
    *r.private_mut::<u64>(&id).unwrap().unwrap() = 18;
    assert_eq!(
        r.state_change(&mut s, &id, 1, 0, 0),
        Err(FsmError::InvalidHandle)
    );
    assert!(!r.observable(&id));
    assert_eq!(s.check(), 0);
    let replacement = r.alloc(&mut s, &d, Some("node"), ()).unwrap();
    assert_ne!(id, replacement);
    assert!(matches!(
        r.dispatch(&mut s, Some(&id), 0, &EventData::None),
        Err(FsmError::InvalidHandle)
    ));
    assert_eq!(
        r.dispatch(&mut s, Some(&replacement), 32, &EventData::None),
        Err(FsmError::InvalidEvent(32))
    );
    assert_eq!(
        r.state_change(&mut s, &replacement, 32, 0, 0),
        Err(FsmError::InvalidState(32))
    );
    r.free(&mut s, &replacement).unwrap();
}
