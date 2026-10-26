// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(all(feature = "runtime", feature = "tracing"))]
use osmocore_rs::{msgb::Msgb, runtime::*};
use std::{net::UdpSocket, time::Duration};
fn message(bytes: &[u8]) -> Msgb {
    let mut m = Msgb::new(bytes.len()).unwrap();
    m.put(bytes.len()).unwrap();
    m.data_mut().copy_from_slice(bytes);
    m
}
#[path = "../../support/diagnostics_capture.rs"]
mod capture;
#[test]
fn command_and_write_spans_propagate_and_payloads_stay_private() {
    let capture = capture::Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
        let h = rt
            .register_udp(socket, UdpResourceOptions::default())
            .unwrap();
        struct Secret {
            handle: UdpHandle,
            done: bool,
        }
        impl Application for Secret {
            type Command = String;
            type Error = ();
            fn on_command(
                &mut self,
                cx: &mut RuntimeContext<'_>,
                command: String,
            ) -> Result<(), ()> {
                assert_eq!(command, "PRIVATE_COMMAND");
                cx.try_send_udp(
                    self.handle,
                    Datagram {
                        message: message(b"PRIVATE_PAYLOAD"),
                        destination: Some("127.0.0.1:9".parse().unwrap()),
                    },
                )
                .unwrap();
                Ok(())
            }
            fn on_event(
                &mut self,
                _: &mut RuntimeContext<'_>,
                event: RuntimeEvent<'_>,
            ) -> Result<(), ()> {
                if let RuntimeEvent::Write(_) = event {
                    tracing::info!(target: "test_runtime_application", operation = "outcome");
                    self.done = true;
                }
                Ok(())
            }
        }
        {
            let span = tracing::info_span!("submitter", correlation = 17u64);
            let _guard = span.enter();
            rt.control_handle()
                .try_submit("PRIVATE_COMMAND".into())
                .ok()
                .unwrap();
        }
        let mut app = Secret {
            handle: h,
            done: false,
        };
        for _ in 0..4 {
            if app.done {
                break;
            }
            rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
                .unwrap();
        }
        assert!(app.done);
    });
    let events = capture.events();
    for name in ["write_complete", "outcome"] {
        let event = events
            .iter()
            .find(|e| e.fields.text("operation") == Some(name))
            .unwrap();
        assert!(
            event.scope.iter().any(|s| s.name == "submitter"
                && s.fields.0["correlation"] == capture::Value::Unsigned(17))
        );
    }
    let text = format!("{events:?}");
    assert!(!text.contains("PRIVATE_COMMAND"));
    assert!(!text.contains("PRIVATE_PAYLOAD"));
    assert!(
        events
            .iter()
            .any(|e| e.target == "osmocore_rs::runtime" && e.level == tracing::Level::TRACE)
    );
}
