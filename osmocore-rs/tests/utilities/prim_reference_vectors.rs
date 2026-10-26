use osmocore_rs::compat::prim::OSMO_NO_EVENT;
use osmocore_rs::compat::prim::{osmo_event_for_prim, osmo_prim_init};
#[path = "../support/prim_fixture.rs"]
mod fixture;

use fixture::{Record, number};
use osmocore_rs::msgb::Msgb;
use osmocore_rs::prim::*;

const FIXTURE: &str = include_str!("../compatibility/vectors/prim_vectors.json");

fn message() -> Msgb {
    let mut msg = Msgb::new(16).unwrap();
    for (i, byte) in msg.backing_mut().iter_mut().enumerate() {
        *byte = 0xa5 ^ i as u8;
    }
    msg.reserve(4).unwrap();
    msg.put(4).unwrap();
    for (layer, offset) in [4, 5, 6, 8].into_iter().enumerate() {
        msg.set_layer_offset(layer + 1, Some(offset)).unwrap();
    }
    msg.cb = [1, 2, 3, 4, 5];
    msg
}
fn snapshot(msg: &Msgb) -> String {
    let mut state = msg
        .backing()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    state.push_str(&format!(
        "/{}/{}/{}",
        msg.data_offset(),
        msg.tail_offset(),
        msg.data().len()
    ));
    for layer in msg.layer_offsets() {
        state.push_str(&format!("/{}", layer.unwrap()));
    }
    for word in msg.cb {
        state.push_str(&format!("/{word}"));
    }
    state
}
fn fields(header: &PrimHeader<'_>, record: &Record) {
    assert_eq!(header.sap, number(record, "sap_out"));
    assert_eq!(header.primitive, number(record, "primitive_out"));
    assert_eq!(header.operation.0, number(record, "operation_out"));
}
fn callback(header: &mut PrimHeader<'_>, context: &mut u32) -> i32 {
    *context += 1;
    header.sap = 42;
    header.msg.as_deref_mut().unwrap().data_mut()[0] ^= 0xff;
    -23
}

#[test]
fn direct_c_primitive_observations() {
    let records = fixture::validated_records(FIXTURE);
    assert_eq!(SAP_GSM_SHIFT, 24);
    assert_eq!(
        [SAP_GSM_BASE, SAP_TETRA_BASE, SAP_SS7_BASE],
        [0x1000000, 0x2000000, 0x3000000]
    );
    assert_eq!(
        [
            PRIM_OP_REQUEST.0,
            PRIM_OP_RESPONSE.0,
            PRIM_OP_INDICATION.0,
            PRIM_OP_CONFIRM.0
        ],
        [0, 1, 2, 3]
    );
    assert_eq!(OSMO_NO_EVENT, u32::MAX);
    for pair in records[..fixture::INIT_COUNT].as_chunks::<2>().0 {
        for preferred in [false, true] {
            let mut msg = message();
            let identity = &msg as *const Msgb;
            let mut available = Some(&mut msg);
            let mut header = PrimHeader::new(17, 23, PrimOperation(42), None);
            for record in pair {
                available = header.msg.take().or(available);
                assert_eq!(snapshot(available.as_deref().unwrap()), record["before"]);
                let attached = number(record, "attached") == 1;
                let message = if attached { available.take() } else { None };
                if preferred {
                    header = PrimHeader::new(
                        number(record, "sap"),
                        number(record, "primitive"),
                        PrimOperation(number(record, "operation")),
                        message,
                    );
                } else {
                    osmo_prim_init(
                        &mut header,
                        number(record, "sap"),
                        number(record, "primitive"),
                        PrimOperation(number(record, "operation")),
                        message,
                    );
                }
                fields(&header, record);
                let association = if attached {
                    std::ptr::eq(header.msg.as_deref().unwrap(), identity)
                } else {
                    header.msg.is_none()
                };
                assert_eq!(u32::from(association), number(record, "association"));
                assert_eq!(
                    packed_id(header.primitive, header.operation),
                    number(record, "packed")
                );
                assert_eq!(header.packed_id(), number(record, "header_packed"));
                assert_eq!(operation_name(header.operation), record["name"]);
                assert_eq!(
                    snapshot(header.msg.as_deref().or(available.as_deref()).unwrap()),
                    record["after"]
                );
            }
        }
    }
    for record in &records[fixture::INIT_COUNT..fixture::INIT_COUNT + fixture::EVENT_COUNT] {
        let mut msg = message();
        assert_eq!(snapshot(&msg), record["before"]);
        let header = PrimHeader::new(
            number(record, "sap"),
            number(record, "primitive"),
            PrimOperation(number(record, "operation")),
            Some(&mut msg),
        );
        let maps = fixture::maps(
            header.sap,
            header.primitive,
            header.operation.0,
            number(record, "scenario"),
        )
        .into_iter()
        .map(|r| PrimEventMap {
            sap: r[0],
            primitive: r[1],
            operation: PrimOperation(r[2]),
            event: r[3],
        })
        .collect::<Vec<_>>();
        assert_eq!(osmo_event_for_prim(&header, &maps), number(record, "event"));
        // Ordinary Rust maps represent the active prefix, excluding C's sentinel.
        let active = maps
            .iter()
            .position(|map| map.event == OSMO_NO_EVENT)
            .unwrap_or(maps.len());
        let event = number(record, "event");
        assert_eq!(
            event_for_primitive(&header, &maps[..active]),
            (event != OSMO_NO_EVENT).then_some(event)
        );
        assert_eq!(snapshot(header.msg.as_deref().unwrap()), record["after"]);
    }
    let record = &records[fixture::COUNT - 1];
    let mut msg = message();
    let identity = &msg as *const Msgb;
    assert_eq!(snapshot(&msg), record["before"]);
    let mut header = PrimHeader::new(17, 23, PRIM_OP_RESPONSE, Some(&mut msg));
    let cb: PrimCallback<u32> = callback;
    let mut context = 7;
    assert_eq!(
        cb(&mut header, &mut context),
        record["return_code"].parse::<i32>().unwrap()
    );
    assert_eq!(context, number(record, "context"));
    fields(&header, record);
    assert_eq!(
        u32::from(std::ptr::eq(header.msg.as_deref().unwrap(), identity)),
        number(record, "association")
    );
    assert_eq!(snapshot(header.msg.as_deref().unwrap()), record["after"]);
}

#[test]
fn corrupt_primitive_fixtures_are_rejected() {
    fixture::check_corruptions(FIXTURE);
}
