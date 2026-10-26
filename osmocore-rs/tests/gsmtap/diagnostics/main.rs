// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(all(feature = "runtime", feature = "tracing"))]

use osmocore_rs::{gsmtap::GsmtapMessage, msgb::MsgbError};
#[path = "../../support/diagnostics_capture.rs"]
mod capture;

#[test]
fn preparation_is_trace_metadata_only_and_inherits_application_context() {
    let capture = capture::Capture::default();
    let destination = "127.0.0.1:4729".parse().unwrap();
    let mut message = GsmtapMessage::um(b"PRIVATE_GSMTAP_PAYLOAD");
    message.arfcn = 512;
    message.timeslot = 3;
    message.channel_type = 8;
    message.frame_number = 42;
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let span = tracing::info_span!("application", request = 19u64);
        let _guard = span.enter();
        message.into_datagram(Some(destination)).unwrap();
        message.into_datagram(None).unwrap();
        // Plain encoders and failed preparation do not emit transport events.
        message.encode();
        message.encode_msgb().unwrap();
        let oversized = vec![0; 65_520];
        assert_eq!(
            GsmtapMessage::um(&oversized)
                .into_datagram(None)
                .err()
                .unwrap(),
            MsgbError::CapacityTooLarge
        );
    });
    let events = capture.events();
    assert_eq!(events.len(), 2, "{events:?}");
    for (event, explicit) in events.iter().zip([true, false]) {
        assert_eq!(event.target, "osmocore_rs::gsmtap");
        assert_eq!(event.level, tracing::Level::TRACE);
        assert_eq!(event.fields.text("operation"), Some("prepare_datagram"));
        let expected = [
            ("message_type", capture::Value::Unsigned(1)),
            ("arfcn", capture::Value::Unsigned(512)),
            ("timeslot", capture::Value::Unsigned(3)),
            ("channel_type", capture::Value::Unsigned(8)),
            ("frame_number", capture::Value::Unsigned(42)),
            (
                "payload_len",
                capture::Value::Unsigned(message.payload.len() as u64),
            ),
            ("explicit_destination", capture::Value::Bool(explicit)),
        ];
        assert_eq!(event.fields.0.len(), expected.len() + 1);
        for (name, value) in expected {
            assert_eq!(event.fields.0[name], value);
        }
        assert_eq!(event.scope.len(), 1);
        assert_eq!(event.scope[0].name, "application");
        assert_eq!(
            event.scope[0].fields.0["request"],
            capture::Value::Unsigned(19)
        );
    }
    let rendered = format!("{events:?}");
    assert!(!rendered.contains("PRIVATE_GSMTAP_PAYLOAD"));
    assert!(!rendered.contains("127.0.0.1"));
    assert!(!rendered.contains("4729"));
    tracing::subscriber::with_default(capture.subscriber("debug"), || {
        message.into_datagram(None).unwrap();
    });
    assert_eq!(
        capture.events().len(),
        2,
        "preparation must be filtered at debug"
    );
}
