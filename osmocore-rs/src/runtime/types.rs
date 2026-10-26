// SPDX-License-Identifier: GPL-3.0-or-later
use crate::msgb::Msgb;
use std::{io, net::SocketAddr, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct Key {
    pub owner: u64,
    pub slot: usize,
    pub generation: u64,
}
macro_rules! handle {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        /// Checked resource identity, valid only in its owning runtime until closure.
        /// Reused slots receive a new generation; foreign and stale handles reject.
        pub struct $name(pub(crate) Key);
    };
}
handle!(TcpHandle);
handle!(UdpHandle);
handle!(ListenerHandle);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ResourceHandle {
    Tcp(TcpHandle),
    Udp(UdpHandle),
    Listener(ListenerHandle),
}
impl ResourceHandle {
    pub(crate) fn key(self) -> Key {
        match self {
            Self::Tcp(h) => h.0,
            Self::Udp(h) => h.0,
            Self::Listener(h) => h.0,
        }
    }
}
impl From<TcpHandle> for ResourceHandle {
    fn from(h: TcpHandle) -> Self {
        Self::Tcp(h)
    }
}
impl From<UdpHandle> for ResourceHandle {
    fn from(h: UdpHandle) -> Self {
        Self::Udp(h)
    }
}
impl From<ListenerHandle> for ResourceHandle {
    fn from(h: ListenerHandle) -> Self {
        Self::Listener(h)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
/// Ticket for admitted output; completion arrives separately as a write outcome.
pub struct WriteId(pub(crate) u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
/// Ticket for deferred registration; the usable handle arrives in `Registered`.
pub struct RegistrationId(pub(crate) u64);

/// All limits are nonzero. Bytes charge entire Msgb backing allocations, not
/// active lengths. Command limits bound count; opaque command heaps are app policy.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// Maximum simultaneously registered listeners, streams and UDP sockets.
    pub max_resources: usize,
    /// Capacity of the kernel readiness event buffer.
    pub poll_capacity: usize,
    /// Maximum poll wait; bounds fallback progress, not callback duration.
    pub max_poll_interval: Duration,
    /// Mailbox command count; command-owned heap memory is application policy.
    pub command_capacity: usize,
    /// Maximum commands dispatched per step.
    pub commands_per_step: usize,
    /// Maximum deferred effects admitted in an ordinary callback.
    pub effects_per_callback: usize,
    /// Separate final-output hook effect allowance; output quotas still apply.
    pub shutdown_effects: usize,
    /// Maximum normal notifications dispatched across one whole step.
    pub notifications_per_step: usize,
    /// Normal notification capacity; must be at least four.
    pub notification_capacity: usize,
    /// Maximum I/O calls per direction per resource per step.
    pub io_calls_per_resource: usize,
    /// Per-direction byte quantum; indivisible datagrams can exceed it.
    pub bytes_per_resource: usize,
    /// Maximum accepts per listener per step.
    pub accepts_per_resource: usize,
    /// Maximum incoming UDP payload; larger datagrams are discarded. At most 65,535.
    pub max_datagram_size: usize,
    /// Per-resource retained Msgb backing-byte limit, including reservations.
    pub per_resource_bytes: usize,
    /// Per-resource message credits, retained through completion notification.
    pub per_resource_messages: usize,
    /// Global retained Msgb backing-byte limit, including reservations.
    pub total_bytes: usize,
    /// Global message credits, retained through completion notification.
    pub total_messages: usize,
    /// Maximum driver invocations per step; component batches remain cooperative.
    pub driver_invocations_per_step: usize,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_resources: 1024,
            poll_capacity: 256,
            max_poll_interval: Duration::from_millis(250),
            command_capacity: 64,
            commands_per_step: 32,
            effects_per_callback: 64,
            shutdown_effects: 16,
            notifications_per_step: 128,
            notification_capacity: 256,
            io_calls_per_resource: 16,
            bytes_per_resource: 65536,
            accepts_per_resource: 16,
            max_datagram_size: 65507,
            per_resource_bytes: 262144,
            per_resource_messages: 128,
            total_bytes: 4194304,
            total_messages: 2048,
            driver_invocations_per_step: 32,
        }
    }
}
impl RuntimeConfig {
    pub(crate) fn validate(&self) -> Result<(), InitError> {
        let limits = [
            self.max_resources,
            self.poll_capacity,
            self.command_capacity,
            self.commands_per_step,
            self.effects_per_callback,
            self.shutdown_effects,
            self.notifications_per_step,
            self.notification_capacity,
            self.io_calls_per_resource,
            self.bytes_per_resource,
            self.accepts_per_resource,
            self.max_datagram_size,
            self.per_resource_bytes,
            self.per_resource_messages,
            self.total_bytes,
            self.total_messages,
            self.driver_invocations_per_step,
        ];
        if limits.contains(&0)
            || self.notification_capacity < 4
            || self.max_poll_interval.is_zero()
            || self.max_datagram_size > 65535
            || self
                .shutdown_effects
                .checked_mul(2)
                .and_then(|n| n.checked_add(self.notification_capacity))
                .is_none()
        {
            return Err(InitError::Configuration);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub enum InitError {
    Configuration,
    IdentityExhausted,
    Io(io::Error),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rejection {
    Capacity,
    ForeignHandle,
    StaleHandle,
    WrongKind,
    Closed,
    Lifecycle,
    InvalidDestination,
    Exhausted,
    Cancelled,
}
pub struct EnqueueError<T> {
    pub reason: Rejection,
    pub value: T,
}
#[derive(Debug)]
pub enum RegistrationError {
    Rejected(Rejection),
    Io {
        phase: &'static str,
        source: io::Error,
    },
}
#[derive(Clone, Copy, Debug, Default)]
/// Registration policy for a TCP stream.
pub struct TcpResourceOptions {
    /// Suppress reads until explicitly resumed.
    pub read_paused: bool,
    /// Keep the write half open after peer EOF and queued output flush.
    pub retain_write_half: bool,
    /// Request TCP_NODELAY during registration.
    pub nodelay: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct UdpResourceOptions {
    /// Suppress reads until explicitly resumed.
    pub read_paused: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ListenerOptions {
    /// Options for accepted streams; accepted reads start paused for setup.
    pub accepted: TcpResourceOptions,
}
pub struct Datagram {
    pub message: Msgb,
    pub destination: Option<SocketAddr>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    Running,
    Draining,
    Stopped,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Shutdown closes admission and quiesces drivers; callbacks remain cooperative.
pub enum ShutdownMode {
    /// Cancel outstanding output and stop.
    Immediate,
    /// Drain accepted output until empty or the monotonic deadline expires.
    /// Later requests may shorten this deadline or escalate to immediate stop.
    Drain { timeout: Duration },
}
#[derive(Debug)]
pub enum WriteStatus {
    Complete,
    Cancelled,
    Failed(io::Error),
}
#[derive(Debug)]
pub struct WriteOutcome {
    pub(crate) context: super::TraceContext,
    pub id: WriteId,
    pub resource: ResourceHandle,
    pub bytes_written: usize,
    pub status: WriteStatus,
}
#[derive(Debug, Default)]
pub struct ShutdownReport {
    pub timed_out: bool,
    pub writes: Vec<WriteOutcome>,
    pub registrations: Vec<(RegistrationId, Result<ResourceHandle, RegistrationError>)>,
}
#[derive(Debug)]
pub enum FailureCause<E> {
    Application(E),
    Driver {
        label: &'static str,
        failure: DriverFailure<E>,
    },
    Poll(io::Error),
    Internal(io::Error),
}
#[derive(Debug)]
pub struct RunFailure<E> {
    pub cause: FailureCause<E>,
    pub report: ShutdownReport,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepOutcome {
    Progress,
    Stopped,
}
#[derive(Debug)]
pub enum DriverFailure<E> {
    Application(E),
    Component(Box<dyn std::error::Error>),
    Clock(crate::time::TimeError),
}
pub enum RuntimeEvent<'a> {
    Accepted {
        listener: ListenerHandle,
        stream: TcpHandle,
        peer: SocketAddr,
    },
    TcpBytes {
        stream: TcpHandle,
        bytes: &'a [u8],
    },
    PeerEof(TcpHandle),
    UdpDatagram {
        socket: UdpHandle,
        bytes: &'a [u8],
        source: SocketAddr,
    },
    OversizedDatagram(UdpHandle),
    TransportError {
        resource: ResourceHandle,
        operation: &'static str,
        source: io::Error,
    },
    Write(WriteOutcome),
    Registered {
        request: RegistrationId,
        result: Result<ResourceHandle, RegistrationError>,
    },
    EffectFailed {
        resource: ResourceHandle,
        source: io::Error,
    },
    Closed(ResourceHandle),
    DriverProgress {
        label: &'static str,
    },
    DriverDisabled {
        label: &'static str,
    },
}
/// Application callbacks run cooperatively on the runtime thread and must not block.
/// Effects commit after successful return; application mutations are not rolled back.
pub trait Application {
    type Command: Send + 'static;
    type Error;
    fn on_event(
        &mut self,
        cx: &mut super::RuntimeContext<'_>,
        event: RuntimeEvent<'_>,
    ) -> Result<(), Self::Error>;
    fn on_command(
        &mut self,
        cx: &mut super::RuntimeContext<'_>,
        command: Self::Command,
    ) -> Result<(), Self::Error>;
    fn on_shutdown(&mut self, _cx: &mut super::RuntimeContext<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<T> std::fmt::Debug for EnqueueError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnqueueError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

/// Separate allocation admission and unsent wire backlog measurements.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueueUsage {
    pub retained_bytes: usize,
    pub messages: usize,
    pub pending_wire_bytes: usize,
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration => f.write_str("invalid runtime configuration"),
            Self::IdentityExhausted => f.write_str("runtime identity space exhausted"),
            Self::Io(source) => write!(f, "runtime initialization: {source}"),
        }
    }
}
impl std::error::Error for InitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            _ => None,
        }
    }
}
impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(reason) => write!(f, "resource registration rejected: {reason:?}"),
            Self::Io { phase, source } => write!(f, "resource registration ({phase}): {source}"),
        }
    }
}
impl std::error::Error for RegistrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
impl<E: std::fmt::Display> std::fmt::Display for DriverFailure<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Application(source) => write!(f, "application: {source}"),
            Self::Component(source) => write!(f, "component: {source}"),
            Self::Clock(source) => write!(f, "clock: {source}"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for DriverFailure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Application(source) => Some(source),
            Self::Component(source) => Some(source.as_ref()),
            Self::Clock(source) => Some(source),
        }
    }
}
impl<E: std::fmt::Display> std::fmt::Display for FailureCause<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Application(source) => write!(f, "application callback: {source}"),
            Self::Driver { label, failure } => write!(f, "driver {label}: {failure}"),
            Self::Poll(source) => write!(f, "poll: {source}"),
            Self::Internal(source) => write!(f, "runtime: {source}"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for FailureCause<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Application(source) => Some(source),
            Self::Driver { failure, .. } => Some(failure),
            Self::Poll(source) | Self::Internal(source) => Some(source),
        }
    }
}
impl<E: std::fmt::Display> std::fmt::Display for RunFailure<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runtime stopped: {}", self.cause)
    }
}
impl<E: std::error::Error + 'static> std::error::Error for RunFailure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    use std::error::Error;
    #[test]
    fn runtime_errors_preserve_sources_and_format_without_debug() {
        struct DisplayOnly;
        impl std::fmt::Display for DisplayOnly {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("application failure")
            }
        }
        let failure = RunFailure {
            cause: FailureCause::Application(DisplayOnly),
            report: ShutdownReport::default(),
        };
        assert_eq!(
            failure.to_string(),
            "runtime stopped: application callback: application failure"
        );
        let registration = RegistrationError::Io {
            phase: "register",
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        assert_eq!(
            registration
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        let init = InitError::Io(io::Error::from(io::ErrorKind::Unsupported));
        assert!(init.source().unwrap().is::<io::Error>());
        let failure = RunFailure {
            cause: FailureCause::Driver {
                label: "test",
                failure: DriverFailure::<io::Error>::Component(Box::new(io::Error::from(
                    io::ErrorKind::BrokenPipe,
                ))),
            },
            report: ShutdownReport::default(),
        };
        let original = failure
            .source()
            .unwrap()
            .source()
            .unwrap()
            .source()
            .unwrap();
        assert_eq!(
            original.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
