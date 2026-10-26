use super::fsm_fixture;
use osmocore_rs::compat::fsm::{
    osmo_fsm_inst_find_by_name, osmo_fsm_inst_update_id_f, osmo_fsm_inst_update_id_f_sanitize,
    osmo_fsm_term_cause_name,
};
use osmocore_rs::{
    fsm::*,
    time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::TimerScheduler,
};
const TEXT: &str = include_str!("../compatibility/vectors/fsm_registry/fsm_registry_vectors.tsv");
const REFERENCE: &str =
    include_str!("../compatibility/vectors/fsm_registry/fsm_registry_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/fsm_registry/fsm_registry_generator_provenance.json");
const DIAGNOSTICS: &str =
    include_str!("../compatibility/vectors/fsm_registry/fsm_registry_diagnostics.json");
const SPEC: fsm_fixture::Evidence = fsm_fixture::Evidence {
    text_hash: 0xba325624830ca2d7,
    reference_hash: 0xcab6c5c42bd14581,
    reference_sha: "f572b2e98589d209217fdd17b117b79f1ad113b499a96898fd6c7c251d3d7fe0",
    generator_hash: 0x4c714d55107bb4ca,
    generator_sha: "cd36fd426f7c6982e0f76d5c6e6f164200b2cdacdd0a1eacde3919ea86d1eefe",
    diagnostics_hash: 0x3519a2dc2f5d25a5,
    records: 1524,
    scenarios: 127,
    fields: 32,
    probes: 2,
};
#[test]
fn fsm_registry_evidence() {
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
    step: usize,
    definition: Option<FsmDefinitionId>,
    nodes: [Option<FsmId>; 3],
    trace: Vec<String>,
}
type R = FsmRuntime<Source, Data>;
type S = TimerScheduler<Source, R>;
fn definition(name: &str, names: bool) -> Fsm<Source, Data, ()> {
    Fsm {
        name: name.into(),
        states: ["zero", "one"]
            .iter()
            .map(|n| FsmState {
                name: Some((*n).into()),
                in_event_mask: 0,
                out_state_mask: 0,
                action: None,
                onenter: None,
                onleave: None,
            })
            .collect(),
        allstate_event_mask: 0,
        allstate_action: None,
        pre_term: None,
        cleanup: None,
        timer_cb: None,
        event_names: if names {
            Some(vec![
                (0, Some("E0".into())),
                (2, Some("E2".into())),
                (2, Some("duplicate".into())),
                (0, None),
            ])
        } else {
            None
        },
    }
}
fn hex(s: Option<&str>) -> String {
    match s {
        None => "s:-".into(),
        Some(s) => {
            let mut t = String::from("s:");
            for b in s.bytes() {
                t.push_str(&format!("{b:02x}"));
            }
            t
        }
    }
}
fn index(r: &R, id: Option<&FsmId>) -> i32 {
    match id {
        None => -1,
        Some(id) => r
            .data
            .nodes
            .iter()
            .position(|node| node.as_ref() == Some(id))
            .map(|n| n as i32)
            .unwrap_or(-2),
    }
}
// Preserve the parameter list of the C API or reference record.
#[allow(clippy::too_many_arguments)]
fn record(
    r: &mut R,
    op: &str,
    arg: i64,
    rc: i32,
    qid: Option<&str>,
    qname: Option<&str>,
    event: u32,
    state: u32,
    cause: usize,
) {
    let d = r.data.definition.as_ref().unwrap();
    let instances = r.instances(d).unwrap();
    let safe = instances
        .iter()
        .all(|id| r.instance(id).unwrap().id.is_some());
    let idfound = if let Some(query) = qid.filter(|_| safe) {
        index(r, r.find_by_id(d, query).unwrap().as_ref())
    } else {
        -2
    };
    let namefound = index(r, osmo_fsm_inst_find_by_name(r, d, qname).unwrap().as_ref());
    let causes = [
        TermCause::Parent,
        TermCause::Request,
        TermCause::Regular,
        TermCause::Error,
        TermCause::Timeout,
    ];
    let registered = i32::from(r.find_fsm_by_name("Identity").as_ref() == Some(d));
    let order = instances
        .iter()
        .fold(0, |order, id| order * 4 + index(r, Some(id)) + 1);
    let mut line = format!(
        "{}\t{}\t{op}\t{arg}\t{rc}\t{}\t{idfound}\t{}\t{namefound}\t{event}\t{}\t{state}\t{}\t{cause}\t{}\t{registered}\t{order}",
        r.data.scenario,
        r.data.step,
        hex(qid),
        hex(qname),
        hex(Some(&r.event_name(d, event).unwrap())),
        hex(r.state_name(d, state).unwrap().as_deref()),
        hex(Some(osmo_fsm_term_cause_name(causes[cause])))
    );
    for node in &r.data.nodes {
        if let Some(id) = node.as_ref().filter(|id| r.observable(id)) {
            let i = r.instance(id).unwrap();
            line.push_str(&format!(
                "\t{}\t{}\t1\t{}\t{}",
                i.state,
                i.timer_number,
                hex(i.id.as_deref()),
                hex(Some(&i.name))
            ));
        } else {
            line.push_str("\t-1\t0\t0\ts:-\ts:-");
        }
    }
    r.data.trace.push(line);
    r.data.step += 1;
}
fn status<T>(result: Result<T, FsmError>) -> i32 {
    result
        .err()
        .map(|e| osmocore_rs::compat::fsm::status(&e))
        .unwrap_or(0)
}
#[test]
fn fsm_registry_complete_c_replay() {
    fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut all = Vec::new();
    for c in 1u8..128 {
        let mut r = R::new(Data {
            scenario: c as usize,
            ..Data::default()
        });
        let mut s = S::new(Source);
        let names = c % 2 != 0;
        let mut descriptor = definition("Identity", names);
        if c == 1 {
            descriptor.states[1].name = None;
        }
        let d = r.register(descriptor).unwrap();
        r.data.definition = Some(d.clone());
        let rec = |r: &mut R, op, arg, rc, qid, qname| {
            record(
                r,
                op,
                arg,
                rc,
                qid,
                qname,
                c as u32,
                c as u32,
                c as usize % 5,
            )
        };
        rec(&mut r, "init", c as i64, 0, None, None);
        let rc = status(r.register(definition("Identity", names)));
        rec(&mut r, "duplicate-register", 0, rc, None, None);
        for i in 0..2 {
            r.data.nodes[i] = Some(r.alloc(&mut s, &d, Some("base"), ()).unwrap());
        }
        rec(
            &mut r,
            "allocated",
            0,
            0,
            Some("base"),
            Some("Identity(base)"),
        );
        let id = format!("x{}z", char::from(c));
        let node = r.data.nodes[0].clone().unwrap();
        let rc = status(r.update_id(&node, Some(&id)));
        rec(
            &mut r,
            "update-id",
            c as i64,
            rc,
            Some(&id),
            Some("Identity(base)"),
        );
        let rc = status(osmo_fsm_inst_update_id_f_sanitize(
            &mut r,
            &node,
            b'-',
            Some(format_args!("{id}")),
        ));
        rec(
            &mut r,
            "sanitize-id",
            c as i64,
            rc,
            Some("base"),
            Some("Identity(base)"),
        );
        let second = r.data.nodes[1].take().unwrap();
        r.free(&mut s, &second).unwrap();
        rec(
            &mut r,
            "free-second",
            0,
            0,
            Some("base"),
            Some("Identity(base)"),
        );
        r.data.nodes[2] = Some(r.alloc(&mut s, &d, Some("base"), ()).unwrap());
        rec(&mut r, "reuse", 0, 0, Some("base"), Some("Identity(base)"));
        r.unregister(&d).unwrap();
        rec(
            &mut r,
            "unregistered",
            0,
            0,
            Some("base"),
            Some("Identity(base)"),
        );
        r.data.nodes[1] = Some(r.alloc(&mut s, &d, Some("after"), ()).unwrap());
        rec(
            &mut r,
            "alloc-unregistered",
            0,
            1,
            Some("after"),
            Some("Identity(after)"),
        );
        let rc = status(osmo_fsm_inst_update_id_f(&mut r, &node, None));
        rec(&mut r, "clear-id", 0, rc, None, Some("Identity"));
        for i in 0..3 {
            if let Some(id) = r.data.nodes[i].take() {
                r.free(&mut s, &id).unwrap();
            }
        }
        rec(&mut r, "removed", 0, 0, None, Some("missing"));
        let other = r.register(definition(&id, names));
        let rc = match other {
            Ok(other) => {
                r.unregister(&other).unwrap();
                0
            }
            Err(e) => osmocore_rs::compat::fsm::status(&e),
        };
        rec(&mut r, "identifier-register", c as i64, rc, None, None);
        all.extend(r.data.trace);
    }
    let expected = TEXT.lines().skip(1).collect::<Vec<_>>();
    assert_eq!(all.len(), expected.len());
    for (a, b) in all.iter().zip(expected) {
        assert_eq!(a, b);
    }
}
