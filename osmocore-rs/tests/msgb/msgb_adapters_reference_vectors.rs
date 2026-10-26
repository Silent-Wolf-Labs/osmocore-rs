use osmocore_rs::compat::msgb::{
    msgb_dequeue, msgb_dequeue_count, msgb_enqueue, msgb_enqueue_count, msgb_eq, msgb_eq_data,
    msgb_eq_l1, msgb_eq_l1_data, msgb_eq_l2, msgb_eq_l2_data, msgb_eq_l3, msgb_eq_l3_data,
    msgb_eq_l4, msgb_eq_l4_data, msgb_eq_optional, msgb_hexdump, msgb_hexdump_buf, msgb_hexdump_l1,
    msgb_hexdump_l2, msgb_hexdump_l3, msgb_hexdump_l4, msgb_hexdump_owned, msgb_printf,
    msgb_queue_free,
};
#[path = "../support/msgb_adapters_fixture.rs"]
mod fixture;
use super::msgb_reference_vectors::{initial, snapshot};
use fixture::{bytes, number};
use osmocore_rs::msgb::*;
const FIXTURE: &str = include_str!("../compatibility/vectors/msgb_adapters_vectors.json");

#[test]
fn remaining_adapters_match_all_c_observations() {
    let records = fixture::validated_records(FIXTURE);
    let mut queue = MsgbQueue::new();
    let mut count = 0;
    for r in &records {
        let mut msg = initial(r);
        snapshot(&msg, r, "before");
        let arg = number(r, "arg") as usize;
        let rc = number(r, "return_code");
        match r["api"].as_str() {
            "eq_data" => {
                let payload = bytes(&r["payload_hex"]);
                let mode = number(r, "mode");
                let result = msgb_eq_optional(
                    if mode == 3 { None } else { Some(&msg) },
                    arg,
                    if mode == 4 { None } else { Some(&payload) },
                )
                .unwrap();
                assert_eq!(i64::from(result), rc);
                if mode < 3 {
                    let f = [
                        msgb_eq_data,
                        msgb_eq_l1_data,
                        msgb_eq_l2_data,
                        msgb_eq_l3_data,
                        msgb_eq_l4_data,
                    ][arg];
                    assert_eq!(f(&msg, &payload).unwrap(), result);
                    let preferred = [
                        Msgb::eq_data,
                        Msgb::eq_l1_data,
                        Msgb::eq_l2_data,
                        Msgb::eq_l3_data,
                        Msgb::eq_l4_data,
                    ][arg];
                    assert_eq!(preferred(&msg, &payload).unwrap(), result);
                }
            }
            "eq_msg" => {
                let payload = bytes(&r["payload_hex"]);
                let mut other = Msgb::new(payload.len()).unwrap();
                other.put(payload.len()).unwrap();
                other.data_mut().copy_from_slice(&payload);
                for i in 1..=4 {
                    other.set_layer_offset(i, Some(0)).unwrap();
                }
                let f = [msgb_eq, msgb_eq_l1, msgb_eq_l2, msgb_eq_l3, msgb_eq_l4][arg];
                assert_eq!(i64::from(f(&msg, &other).unwrap()), rc);
                let preferred = [Msgb::eq, Msgb::eq_l1, Msgb::eq_l2, Msgb::eq_l3, Msgb::eq_l4][arg];
                assert_eq!(i64::from(preferred(&msg, &other).unwrap()), rc);
            }
            "dump_buf" => {
                let mut dest = vec![0x7e; arg];
                assert_eq!(i64::from(msgb_hexdump_buf(&mut dest, &msg)), rc);
                assert_eq!(dest, bytes(&r["dest_hex"]), "case {}", r["case"]);
            }
            "dump" => {
                assert_eq!(msgb_hexdump(&msg).as_bytes(), bytes(&r["output_hex"]));
                // Preferred dumps are unlimited; frozen C dumps cap each segment at 1365 bytes.
                if msg.len() <= 1365 && bytes(&r["output_hex"]) != b"ERROR" {
                    assert_eq!(msg.hexdump().as_bytes(), bytes(&r["output_hex"]));
                }
            }
            "dump_owned" => {
                assert_eq!(msgb_hexdump_owned(&msg).as_bytes(), bytes(&r["output_hex"]));
                // Preferred dumps are unlimited; frozen C dumps cap each segment at 1365 bytes.
                if msg.len() <= 1365 && bytes(&r["output_hex"]) != b"ERROR" {
                    assert_eq!(msg.hexdump().as_bytes(), bytes(&r["output_hex"]));
                }
            }
            "dump_layer" => {
                let f = [
                    msgb_hexdump_l1,
                    msgb_hexdump_l2,
                    msgb_hexdump_l3,
                    msgb_hexdump_l4,
                ][arg - 1];
                assert_eq!(f(&msg).unwrap().as_bytes(), bytes(&r["output_hex"]));
                let preferred = [
                    Msgb::hexdump_l1,
                    Msgb::hexdump_l2,
                    Msgb::hexdump_l3,
                    Msgb::hexdump_l4,
                ][arg - 1];
                assert_eq!(preferred(&msg).unwrap().as_bytes(), bytes(&r["output_hex"]));
            }
            "printf" => {
                let result = match number(r, "mode") {
                    0 => msgb_printf(&mut msg, format_args!("")),
                    1 => msgb_printf(&mut msg, format_args!("{}", "abc")),
                    2 => msgb_printf(&mut msg, format_args!("x={:04x}/{}/%", 42, -7)),
                    3 => msgb_printf(&mut msg, format_args!("A{}B", '\0')),
                    4 => msgb_printf(&mut msg, format_args!("{}", "café")),
                    _ => unreachable!(),
                };
                assert_eq!(result.is_ok(), rc == 0, "case {}", r["case"]);
            }
            api => {
                if number(r, "step") == 0 {
                    queue.clear();
                    count = number(r, "mode") as u32;
                }
                let ids = |q: &MsgbQueue| {
                    q.iter()
                        .map(|m| format!("{}/", m.cb[0]))
                        .collect::<String>()
                };
                assert_eq!(ids(&queue), r["queue_before"]);
                assert_eq!(i64::from(count), number(r, "count_before"));
                let mut result = None;
                match api {
                    "enqueue" | "enqueue_count" => {
                        let mut item = Msgb::with_headroom(8, 2).unwrap();
                        item.put(3).unwrap();
                        item.backing_mut().copy_from_slice(msg.backing());
                        item.cb = msg.cb;
                        item.cb[0] = arg as u64;
                        for (i, offset) in msg.layer_offsets().into_iter().enumerate() {
                            item.set_layer_offset(i + 1, offset).unwrap();
                        }
                        if api == "enqueue" {
                            msgb_enqueue(&mut queue, item);
                        } else {
                            msgb_enqueue_count(&mut queue, item, &mut count);
                        }
                    }
                    "queue_free" => msgb_queue_free(&mut queue),
                    "dequeue" => result = msgb_dequeue(&mut queue),
                    "dequeue_count" => result = msgb_dequeue_count(&mut queue, &mut count),
                    _ => panic!("unknown {api}"),
                }
                assert_eq!(result.as_ref().map_or(-1, |m| m.cb[0] as i64), rc);
                assert_eq!(ids(&queue), r["queue_after"]);
                assert_eq!(i64::from(count), number(r, "count_after"));
                if let Some(item) = result {
                    msg = item;
                }
            }
        }
        snapshot(&msg, r, "after");
    }
}
#[test]
fn rejects_corrupt_adapter_evidence() {
    fixture::check_corruptions(FIXTURE);
}
#[test]
fn equality_and_layer_dumps_reject_unsafe_offsets_without_mutation() {
    let mut msg = Msgb::with_headroom(12, 3).unwrap();
    msg.put(4).unwrap();
    msg.set_layer_offset(1, Some(8)).unwrap();
    let before = format!("{msg:?}");
    assert_eq!(msg.eq_l1_data(&[]), Err(MsgbError::LayerOutsideMessage));
    assert_eq!(msg.eq_l2_data(&[]), Err(MsgbError::MissingLayer));
    assert_eq!(msg.eq_level(5, &[]), Err(MsgbError::InvalidLayer));
    assert_eq!(msg.hexdump_l1(), Err(MsgbError::LayerOutsideMessage));
    assert_eq!(msg.hexdump_layer(0), Err(MsgbError::InvalidLayer));
    assert_eq!(msg.hexdump_l2().unwrap(), "[]");
    assert_eq!(format!("{msg:?}"), before);
}
#[test]
fn short_annotation_dumps_are_safe_and_printf_failure_mutates_only_tail_bytes() {
    let mut msg = Msgb::with_headroom(8, 2).unwrap();
    msg.put(3).unwrap();
    msg.backing_mut().fill(0xa5);
    msg.set_layer_offset(1, Some(0)).unwrap();
    for size in 0..12 {
        let mut buf = vec![0x7e; size];
        assert!(!msgb_hexdump_buf(&mut buf, &msg));
        if size > 0 {
            assert_eq!(buf[size - 1], 0);
        }
    }
    let before = format!(
        "{:?}/{:?}/{:?}/{}",
        msg.layer_offsets(),
        msg.cb,
        msg.data(),
        msg.tail_offset()
    );
    assert_eq!(msg.append_str("abcd"), Err(MsgbError::NotEnoughTailroom));
    assert_eq!(&msg.backing()[5..], b"ab\0");
    assert_eq!(
        format!(
            "{:?}/{:?}/{:?}/{}",
            msg.layer_offsets(),
            msg.cb,
            msg.data(),
            msg.tail_offset()
        ),
        before
    );
    assert_eq!(msg.append_str("xy"), Ok(()));
    assert_eq!(msg.len(), 5);
    assert_eq!(msg.append_str(""), Ok(()));
    msg.put(1).unwrap();
    let before = msg.backing().to_vec();
    assert_eq!(
        msgb_printf(&mut msg, format_args!("")),
        Err(MsgbError::NotEnoughTailroom)
    );
    assert_eq!(msg.backing(), before);
}
