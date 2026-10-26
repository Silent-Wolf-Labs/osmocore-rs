//! Scoped subscriber tests run separately from uninstrumented loopback threads.
#![cfg(all(feature = "net", feature = "tracing"))]
use osmocore_rs::net::{self, AddressFamily, SocketOptions, TcpOptions};
use std::net::SocketAddr;
fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
#[cfg(feature = "tracing")]
#[path = "../../support/diagnostics_capture.rs"]
mod capture;
#[cfg(feature = "tracing")]
#[test]
fn diagnostics_inherit_application_context_and_omit_endpoint_details() {
    let capture = capture::Capture::default();
    tracing::subscriber::with_default(capture.subscriber("trace"), || {
        let span = tracing::info_span!("application", request = 9u64);
        let _guard = span.enter();
        let _socket = net::bind_udp(loopback(), None, &SocketOptions::default()).unwrap();
        net::listen_tcp(loopback(), 0, &TcpOptions::default()).unwrap_err();
        net::resolve("127.0.0.1", 53, AddressFamily::Any).unwrap();
    });
    let events = capture.events();
    assert_eq!(events.len(), 3, "{events:#?}");
    assert!(
        events
            .iter()
            .all(|event| event.target == "osmocore_rs::net")
    );
    assert_eq!(events[0].level, tracing::Level::DEBUG);
    assert_eq!(events[0].fields.text("operation"), Some("udp_bind"));
    assert_eq!(events[0].fields.text("family"), Some("ipv4"));
    assert_eq!(
        events[0].fields.0["nonblocking"],
        capture::Value::Bool(false)
    );
    assert_eq!(events[1].level, tracing::Level::WARN);
    assert_eq!(events[1].fields.text("phase"), Some("validate"));
    assert_eq!(
        events[1].fields.text("error_category"),
        Some("invalid_input")
    );
    assert_eq!(events[0].scope[0].name, "application");
    assert_eq!(
        events[0].scope[0].fields.0["request"],
        capture::Value::Unsigned(9)
    );
    assert!(events.iter().all(|event| {
        event
            .scope
            .iter()
            .any(|span| span.name == "net_establishment")
    }));
    assert!(!format!("{events:?}").contains("127.0.0.1"));
}
