// SPDX-License-Identifier: GPL-3.0-or-later

use osmocore_rs::{
    base::base64, base::bcd, base::bitvec::BitVec, base::crc::Crc8Code, base::msgb::Msgb, compat,
};

#[test]
fn base64_rust_buffers_omit_terminators_and_report_empty_lengths() {
    let mut output = [0xa5; 5];
    assert_eq!(base64::encode_into(&mut output, b"foo"), Ok(4));
    assert_eq!(&output, b"Zm9v\xa5");
    let mut short = [0xa5; 3];
    assert_eq!(
        base64::encode_into(&mut short, b"foo"),
        Err(base64::Base64Error::BufferTooSmall { required: 4 })
    );
    assert_eq!(short, [0xa5; 3]);
    assert_eq!(base64::decode_into(&mut output, b" \r\n"), Ok(0));
    let before = output;
    assert_eq!(
        base64::decode_into(&mut output, b"!"),
        Err(base64::Base64Error::InvalidInput)
    );
    assert_eq!(output, before);
    assert_eq!(base64::decode(b"Zg=="), Ok(b"f".to_vec()));
    // The recorded decoder accepts incomplete quartets but writes no partial one.
    assert_eq!(base64::decode(b"Zg"), Ok(Vec::new()));
    assert_eq!(base64::encode(b""), Ok(String::new()));
}

#[test]
fn owned_bcd_excludes_c_strings_and_keeps_nibble_order() {
    assert_eq!(bcd::encode("123", false), Ok(vec![0x21, 0xf3]));
    assert_eq!(bcd::decode(&[0x21, 0xf3], 0, 3, false), Ok("123".into()));
    assert_eq!(bcd::encode("", false), Ok(Vec::new()));
    assert_eq!(bcd::encode("12\0", false), Err(bcd::BcdError::InvalidDigit));
    assert_eq!(bcd::char_to_nibble(b'!'), Err(bcd::BcdError::InvalidDigit));
    assert_eq!(compat::bcd::osmo_char2bcd(b'!'), 0);
    assert_eq!(
        bcd::decode(&[0xa1], 0, 2, false),
        Err(bcd::BcdError::InvalidDigit)
    );
    assert_eq!(
        bcd::decode(&[], 0, usize::MAX, true),
        Err(bcd::BcdError::OutOfRange)
    );
}

#[test]
fn buffer_methods_preserve_partial_bit_writes_and_unused_message_storage() {
    let mut bytes = [0u8];
    let mut bits = BitVec::new(&mut bytes);
    bits.cur_bit = 7;
    assert!(
        bits.write_bits(&[osmocore_rs::base::bitvec::BitValue::One; 2])
            .is_err()
    );
    assert_eq!(bits.cur_bit, 8);
    assert_eq!(bits.data(), &[1]);

    let mut message = Msgb::with_headroom(8, 2).unwrap();
    message.backing_mut().fill(0xa5);
    message.put_u16(0x1234).unwrap();
    assert_eq!(message.data(), &[0x12, 0x34]);
    let before = message.backing().to_vec();
    assert!(message.put(5).is_err());
    assert_eq!(message.backing(), before);
    message.pull(1).unwrap();
    assert_eq!(message.data(), &[0x34]);
    assert_eq!(message.backing(), before);
    let mut large = Msgb::new(1600).unwrap();
    large.put(1600).unwrap();
    assert_eq!(compat::msgb::msgb_hexdump(&large).len(), 1365 * 3);
    assert_eq!(large.hexdump().len(), 1600 * 3);
}

#[test]
fn crc_methods_use_whole_slices_and_return_boolean_comparisons() {
    let code = Crc8Code {
        bits: 3,
        poly: 3,
        init: 0,
        remainder: 0,
    };
    let input = [1, 0, 1, 1];
    let mut output = [0xa5; 5];
    code.set_bits(&input, &mut output).unwrap();
    assert_eq!(code.check_bits(&input, &output), Ok(true));
    output[0] ^= 1;
    assert_eq!(code.check_bits(&input, &output), Ok(false));
    assert_eq!(&output[3..], &[0xa5; 2]);
}

#[test]
fn checked_timeout_tables_keep_reset_prefix_on_invalid_defaults() {
    use osmocore_rs::base::tdef::{TDef, TDefTable, TDefUnit};
    let mut entries = [
        TDef {
            id: 1,
            default_val: 2,
            val: 9,
            unit: TDefUnit::S,
            ..Default::default()
        },
        TDef {
            id: 2,
            default_val: 4,
            val: 9,
            max_val: 3,
            ..Default::default()
        },
        TDef::default(),
    ];
    let mut table = TDefTable::new(&mut entries).unwrap();
    assert!(table.reset().is_err());
    assert_eq!(table.entries()[0].val, 2);
    assert_eq!(table.entries()[1].val, 9);
    assert_eq!(table.get(1, TDefUnit::MS, -1), Ok(2_000));
    assert!(TDefTable::new(&mut []).is_err());
}

#[test]
fn ordinary_primitive_maps_do_not_interpret_c_sentinels() {
    use osmocore_rs::base::prim::{PrimEventMap, PrimHeader, event_for_primitive};
    let header = PrimHeader::default();
    let maps = [PrimEventMap {
        sap: 0,
        primitive: 0,
        operation: header.operation,
        event: u32::MAX,
    }];
    assert_eq!(event_for_primitive(&header, &maps), Some(u32::MAX));
    assert_eq!(event_for_primitive(&header, &[]), None);
    assert_eq!(compat::prim::osmo_event_for_prim(&header, &maps), u32::MAX);
}

/// Handles can leave their owner's thread, but only their owner accepts them.
mod handles {
    use osmocore_rs::{
        base::fsm::{Fsm, FsmDefinitionId, FsmError, FsmId, FsmRuntime, FsmState},
        base::rate_ctr::{
            RateCtrDesc, RateCtrError, RateCtrGroupDesc, RateCtrGroupId, RateCtrRuntime,
        },
        base::time::SystemClock,
        base::timer::{TimerError, TimerId, TimerScheduler},
    };
    use std::thread;

    type Scheduler = TimerScheduler<SystemClock, ()>;
    type MachineRuntime = FsmRuntime<SystemClock, ()>;
    type MachineScheduler = TimerScheduler<SystemClock, MachineRuntime>;

    fn assert_handle<T: Copy + Send + Sync + 'static>() {}
    fn assert_send_sync<T: Send + Sync>() {}

    /// Hand a value to another thread and take it back.
    fn round_trip<T: Send + 'static>(value: T) -> T {
        thread::spawn(move || value).join().unwrap()
    }

    fn group_descriptor() -> RateCtrGroupDesc {
        RateCtrGroupDesc {
            group_name_prefix: "handles".into(),
            group_description: None,
            class_id: 0,
            counters: vec![RateCtrDesc {
                name: "packets".into(),
                description: None,
            }],
        }
    }

    fn machine() -> (MachineScheduler, MachineRuntime, FsmDefinitionId, FsmId) {
        let mut scheduler = MachineScheduler::new(SystemClock);
        let mut runtime = MachineRuntime::new(());
        let definition = runtime
            .register(Fsm {
                name: "handles".into(),
                states: vec![FsmState {
                    name: None,
                    in_event_mask: 0,
                    out_state_mask: 0,
                    action: None,
                    onenter: None,
                    onleave: None,
                }],
                allstate_event_mask: 0,
                allstate_action: None,
                pre_term: None,
                cleanup: None,
                timer_cb: None,
                event_names: None,
            })
            .unwrap();
        let id = runtime
            .alloc(&mut scheduler, &definition, None, ())
            .unwrap();
        (scheduler, runtime, definition, id)
    }

    #[test]
    fn handles_are_copy_send_and_sync() {
        assert_handle::<TimerId>();
        assert_handle::<FsmId>();
        assert_handle::<FsmDefinitionId>();
        assert_handle::<RateCtrGroupId>();
        #[cfg(feature = "runtime")]
        {
            use osmocore_rs::runtime::{ListenerHandle, ResourceHandle, TcpHandle, UdpHandle};
            assert_handle::<TcpHandle>();
            assert_handle::<UdpHandle>();
            assert_handle::<ListenerHandle>();
            assert_handle::<ResourceHandle>();
        }
    }

    #[test]
    fn timer_scheduler_and_rate_counters_can_change_threads() {
        assert_send_sync::<Scheduler>();
        assert_send_sync::<RateCtrRuntime>();

        let mut scheduler = Scheduler::new(SystemClock);
        let id = scheduler.create(None);
        let mut scheduler = round_trip(scheduler);
        assert_eq!(scheduler.pending(&id), Ok(false));
        assert_eq!(scheduler.release(&id), Ok(()));
    }

    #[test]
    fn handles_still_work_after_visiting_another_thread() {
        let mut scheduler = Scheduler::new(SystemClock);
        let timer = scheduler.create(None);
        let returned = round_trip(timer);
        assert_eq!(returned, timer);
        assert_eq!(scheduler.pending(&returned), Ok(false));

        let (_scheduler, runtime, definition, machine_id) = machine();
        let returned = round_trip(machine_id);
        assert_eq!(returned, machine_id);
        assert!(runtime.instance(&returned).is_ok());
        let returned = round_trip(definition);
        assert_eq!(returned, definition);
        assert!(runtime.descriptor(&returned).is_ok());

        let mut counters = RateCtrRuntime::new();
        let group = counters.allocate(group_descriptor(), 0).unwrap();
        let returned = round_trip(group);
        assert_eq!(returned, group);
        assert!(counters.group(&returned).is_ok());
    }

    #[test]
    fn foreign_handles_are_rejected() {
        let mut first = Scheduler::new(SystemClock);
        let mut second = Scheduler::new(SystemClock);
        let timer = first.create(None);
        let same_slot = second.create(None);
        assert_ne!(timer, same_slot);
        assert_eq!(second.pending(&timer), Err(TimerError::InvalidHandle));

        let (_, first, first_definition, first_id) = machine();
        let (_, second, second_definition, second_id) = machine();
        assert_ne!(first_id, second_id);
        assert_ne!(first_definition, second_definition);
        assert_eq!(
            second.instance(&first_id).err(),
            Some(FsmError::InvalidHandle)
        );
        assert_eq!(
            second.descriptor(&first_definition).err(),
            Some(FsmError::InvalidDefinition)
        );
        assert!(first.instance(&first_id).is_ok());

        let mut first = RateCtrRuntime::new();
        let mut second = RateCtrRuntime::new();
        let group = first.allocate(group_descriptor(), 0).unwrap();
        let same_slot = second.allocate(group_descriptor(), 0).unwrap();
        assert_ne!(group, same_slot);
        assert_eq!(
            second.group(&group).unwrap_err(),
            RateCtrError::InvalidHandle
        );
    }

    #[test]
    fn stale_handles_are_rejected_after_their_slot_is_reused() {
        let mut scheduler = Scheduler::new(SystemClock);
        let stale = scheduler.create(None);
        scheduler.release(&stale).unwrap();
        let reused = scheduler.create(None);
        assert_ne!(stale, reused);
        assert_eq!(scheduler.pending(&stale), Err(TimerError::InvalidHandle));
        assert_eq!(scheduler.pending(&reused), Ok(false));

        let (mut scheduler, mut runtime, definition, stale) = machine();
        runtime.free(&mut scheduler, &stale).unwrap();
        let reused = runtime
            .alloc(&mut scheduler, &definition, None, ())
            .unwrap();
        assert_ne!(stale, reused);
        assert_eq!(
            runtime.instance(&stale).err(),
            Some(FsmError::InvalidHandle)
        );
        assert!(runtime.instance(&reused).is_ok());

        let mut counters = RateCtrRuntime::new();
        let stale = counters.allocate(group_descriptor(), 0).unwrap();
        counters.free(&stale).unwrap();
        let reused = counters.allocate(group_descriptor(), 0).unwrap();
        assert_ne!(stale, reused);
        assert_eq!(
            counters.group(&stale).unwrap_err(),
            RateCtrError::InvalidHandle
        );
        assert!(counters.group(&reused).is_ok());
    }

    /// A copied handle no longer keeps its owner's identity alive, so the identity
    /// itself must not come back.
    #[test]
    fn handles_outliving_their_owner_match_no_later_owner() {
        let mut dropped = Scheduler::new(SystemClock);
        let timer = dropped.create(None);
        drop(dropped);
        let mut later = Scheduler::new(SystemClock);
        let same_slot = later.create(None);
        assert_ne!(timer, same_slot);
        assert_eq!(later.pending(&timer), Err(TimerError::InvalidHandle));
    }

    #[test]
    fn owners_created_concurrently_get_distinct_identities() {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                thread::spawn(|| {
                    (0..500)
                        .map(|_| Scheduler::new(SystemClock).create(None))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        // Every handle has slot 0 and generation 0, so only the owner can differ.
        let handles: Vec<TimerId> = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(handles.len(), 4_000);
        for (index, handle) in handles.iter().enumerate() {
            assert!(!handles[index + 1..].contains(handle));
        }
    }
}
