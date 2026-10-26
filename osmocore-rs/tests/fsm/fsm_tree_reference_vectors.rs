use super::fsm_fixture;
use osmocore_rs::{
    fsm::*,
    time::{Clock, ClockId, ClockOverrides, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::TimerScheduler,
};
const TEXT: &str = include_str!("../compatibility/vectors/fsm_tree/fsm_tree_vectors.tsv");
const REFERENCE: &str = include_str!("../compatibility/vectors/fsm_tree/fsm_tree_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/fsm_tree/fsm_tree_generator_provenance.json");
const DIAGNOSTICS: &str =
    include_str!("../compatibility/vectors/fsm_tree/fsm_tree_diagnostics.json");
const SPEC: fsm_fixture::Evidence = fsm_fixture::Evidence {
    text_hash: 0xd919a390c9a14184,
    reference_hash: 0xcab6c5c42bd14581,
    reference_sha: "f572b2e98589d209217fdd17b117b79f1ad113b499a96898fd6c7c251d3d7fe0",
    generator_hash: 0xcd1e26364862a26a,
    generator_sha: "20b6ca3d2c09276476f69a22ca05893e75e96eee0d1888f39ea1c2dceb26bb5d",
    diagnostics_hash: 0xc4722a66858594e7,
    records: 2010,
    scenarios: 134,
    fields: 43,
    probes: 5,
};
#[test]
fn fsm_tree_evidence() {
    fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    fsm_fixture::corruptions(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}
struct Source;
impl Clock for Source {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        Err(TimeError::SystemError(5))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        Err(TimeError::SystemError(5))
    }
}
#[derive(Default)]
struct Data {
    scenario: usize,
    step: usize,
    mode: usize,
    definition: Option<FsmDefinitionId>,
    trace: Vec<String>,
}
type R = FsmRuntime<ClockOverrides<Source>, Data, i64>;
type S = TimerScheduler<ClockOverrides<Source>, R>;
const IDS: [&str; 4] = ["p", "a", "b", "g"];
fn get(r: &R, n: usize) -> Option<FsmId> {
    r.find_by_id(r.data.definition.as_ref().unwrap(), IDS[n])
        .unwrap()
}
fn number(r: &R, id: Option<&FsmId>) -> i32 {
    match id {
        None => -1,
        Some(id) => (0..4)
            .find(|&n| get(r, n).as_ref() == Some(id))
            .map(|n| n as i32)
            .unwrap_or(-2),
    }
}
fn record(s: &S, r: &mut R, op: &str, target: i32, arg: i64, rc: i32) {
    let mut line = format!(
        "{}\t{}\t{op}\t{target}\t{arg}\t{rc}",
        r.data.scenario, r.data.step
    );
    for n in 0..4 {
        if let Some(id) = get(r, n) {
            let i = r.instance(&id).unwrap();
            let childbits = i.children().iter().fold(0u32, |bits, c| {
                let n = number(r, Some(c));
                if n >= 0 { bits | (1 << n) } else { bits }
            });
            let time = s.deadline(i.timer()).unwrap();
            line.push_str(&format!(
                "\t{}\t{}\t{}\t1\t{}\t{}\t{}\t{}\t{}",
                i.state,
                i.timer_number,
                i32::from(i.terminating),
                number(r, i.parent()),
                childbits,
                i32::from(s.pending(i.timer()).unwrap()),
                time.seconds,
                time.microseconds
            ));
        } else {
            line.push_str("\t-1\t0\t0\t0\t-1\t0\t0\t0\t0");
        }
    }
    let order = r
        .instances(r.data.definition.as_ref().unwrap())
        .unwrap()
        .iter()
        .fold(0, |order, id| order * 5 + number(r, Some(id)) + 1);
    line.push_str(&format!("\t{order}"));
    r.data.trace.push(line);
    r.data.step += 1;
}
fn action(s: &mut S, r: &mut R, id: FsmId, event: u32, data: &EventData<'_, i64>) {
    let n = number(r, Some(&id));
    if event >= 10 {
        let payload = match data {
            EventData::None => 0,
            EventData::User(n) => **n as i32,
            EventData::Timeout(n) => *n,
        };
        record(s, r, "notify", n, event as i64, payload);
        if r.data.mode == 5 {
            r.term(s, &id, TermCause::Regular, &EventData::None)
                .unwrap();
            record(s, r, "notify-term", n, 0, 0);
        }
        return;
    }
    record(s, r, "action", n, event as i64, 0);
    if event == 0 {
        r.term(s, &id, TermCause::Regular, data).unwrap();
        record(s, r, "action-term", n, 0, 0);
    }
}
fn preterm(s: &mut S, r: &mut R, id: FsmId, cause: TermCause) {
    let n = number(r, Some(&id));
    record(s, r, "preterm", n, cause as i64, 0);
    if r.data.mode == 1 && r.instance(&id).unwrap().parent().is_some() {
        r.unlink_parent(s, &id).unwrap();
        record(s, r, "pre-unlink", n, 0, 0);
    }
    if r.data.mode == 2 && n == 1 {
        let p = get(r, 2).unwrap();
        r.change_parent(s, &id, Some(&p), 13).unwrap();
        record(s, r, "pre-reparent", n, 2, 0);
    }
    if r.data.mode == 7 {
        r.term(s, &id, TermCause::Error, &EventData::None).unwrap();
        record(s, r, "pre-repeat", n, 0, 0);
        let rc = osmocore_rs::compat::fsm::status(
            &r.dispatch(s, Some(&id), 2, &EventData::None).unwrap_err(),
        );
        record(s, r, "pre-dispatch", n, 2, rc);
    }
}
fn cleanup(s: &mut S, r: &mut R, id: FsmId, cause: TermCause) {
    let n = number(r, Some(&id));
    record(s, r, "cleanup", n, cause as i64, 0);
    if r.data.mode == 3
        && n == 1
        && let Some(parent) = get(r, 0)
    {
        r.term(s, &parent, TermCause::Request, &EventData::None)
            .unwrap();
        record(s, r, "cleanup-parent-term", n, 0, 0);
        let term = i64::from(r.instance(&id).unwrap().terminating);
        record(s, r, "cleanup-still-readable", n, term, 0);
    }
}
fn timeout(s: &mut S, r: &mut R, id: FsmId) -> i32 {
    let n = number(r, Some(&id));
    let t = r.instance(&id).unwrap().timer_number;
    record(s, r, "timeout", n, t as i64, 1);
    r.set_timer_number(s, &id, 77).unwrap();
    1
}
fn definition() -> Fsm<ClockOverrides<Source>, Data, i64> {
    Fsm {
        name: "Tree".into(),
        states: vec![FsmState {
            name: Some("alive".into()),
            in_event_mask: 0xffff,
            out_state_mask: 1,
            action: Some(action),
            onenter: None,
            onleave: None,
        }],
        allstate_event_mask: 0,
        allstate_action: None,
        pre_term: Some(preterm),
        cleanup: Some(cleanup),
        timer_cb: Some(timeout),
        event_names: None,
    }
}
fn scene(scenario: usize, mode: usize, safe: bool) -> (R, S) {
    let mut r = R::new(Data {
        scenario,
        mode,
        ..Data::default()
    });
    r.term_safely(safe);
    let d = r.register(definition()).unwrap();
    r.data.definition = Some(d.clone());
    let mut clock = ClockOverrides::new(Source);
    clock.gettimeofday_override = true;
    clock.gettimeofday_override_time = TimeVal::new(100, 0);
    let mut s = S::new(clock);
    let p = r.alloc(&mut s, &d, Some("p"), ()).unwrap();
    let a = r.alloc_child(&mut s, &d, &p, 10, ()).unwrap();
    r.update_id(&a, Some("a")).unwrap();
    let b = r.alloc_child(&mut s, &d, &p, 11, ()).unwrap();
    r.update_id(&b, Some("b")).unwrap();
    let g = r.alloc_child(&mut s, &d, &a, 12, ()).unwrap();
    r.update_id(&g, Some("g")).unwrap();
    (r, s)
}
fn execute(r: &mut R, s: &mut S, op: usize) -> Result<i32, FsmError> {
    let data = EventData::User(&55);
    let p = get(r, 0);
    let a = get(r, 1);
    let b = get(r, 2);
    let g = get(r, 3);
    let rc = match op {
        0 => {
            r.term(s, p.as_ref().unwrap(), TermCause::Request, &data)?;
            0
        }
        1 => {
            r.term(s, a.as_ref().unwrap(), TermCause::Regular, &data)?;
            0
        }
        2 => {
            r.term(s, g.as_ref().unwrap(), TermCause::Error, &data)?;
            0
        }
        3 => {
            r.term_children(s, p.as_ref().unwrap(), TermCause::Parent, &data)?;
            0
        }
        4 => {
            r.term_children(s, p.as_ref().unwrap(), TermCause::Regular, &data)?;
            0
        }
        5 => {
            r.broadcast_children(s, p.as_ref().unwrap(), 0, &data)?;
            0
        }
        6 => {
            r.change_parent(s, g.as_ref().unwrap(), b.as_ref(), 13)?;
            0
        }
        7 => {
            r.unlink_parent(s, g.as_ref().unwrap())?;
            0
        }
        8 => {
            r.dispatch(s, a.as_ref(), 0, &data)?;
            0
        }
        9 => {
            let id = a.as_ref().unwrap();
            r.set_timer_number(s, id, 23)?;
            let timer = r.instance(id)?.timer().clone();
            s.schedule(&timer, 1, 0)?;
            record(s, r, "armed", 1, 0, 0);
            s.clock.gettimeofday_override_time.seconds = 101;
            r.update(s)?
        }
        _ => {
            let id = g.as_ref().unwrap();
            r.unlink_parent(s, id)?;
            record(s, r, "unlink-before-free", 3, 0, 0);
            r.free(s, id)?;
            0
        }
    };
    let mode = r.data.mode;
    record(s, r, "operation", op as i32, mode as i64, rc);
    Ok(rc)
}
#[test]
fn fsm_tree_complete_c_replay() {
    let rows = fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut all = Vec::new();
    for row in rows.iter().filter(|r| r[2] == "init") {
        let scenario = row[0].parse().unwrap();
        let op = row[3].parse().unwrap();
        let mode = row[4].parse().unwrap();
        let safe = row[5] == "1";
        let (mut r, mut s) = scene(scenario, mode, safe);
        record(&s, &mut r, "init", op as i32, mode as i64, i32::from(safe));
        execute(&mut r, &mut s, op).unwrap();
        for n in 0..4 {
            if let Some(id) = get(&r, n) {
                r.term(&mut s, &id, TermCause::Request, &EventData::None)
                    .unwrap();
                record(&s, &mut r, "drain", n as i32, 0, 0);
            }
        }
        record(&s, &mut r, "removed", 0, 0, 0);
        all.extend(r.data.trace);
    }
    let expected = TEXT.lines().skip(1).collect::<Vec<_>>();
    assert_eq!(all.len(), expected.len());
    for (a, b) in all.iter().zip(expected) {
        assert_eq!(a, b);
    }
}
#[test]
fn fsm_tree_unsafe_c_cases_are_safe_errors() {
    for safe in [false, true] {
        for op in [4, 5] {
            let (mut r, mut s) = scene(1, 5, safe);
            assert_eq!(execute(&mut r, &mut s, op), Err(FsmError::InvalidHandle));
        }
    }
    let (mut r, mut s) = scene(1, 0, false);
    let p = get(&r, 0).unwrap();
    let g = get(&r, 3).unwrap();
    assert_eq!(
        r.free(&mut s, &g),
        Err(FsmError::HierarchyRequiresTermination)
    );
    assert_eq!(
        r.change_parent(&s, &p, Some(&g), 10),
        Err(FsmError::ParentCycle)
    );
}
