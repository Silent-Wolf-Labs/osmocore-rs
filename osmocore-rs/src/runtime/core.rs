// SPDX-License-Identifier: GPL-3.0-or-later
use super::backend::Backend;
use super::{
    control::{self, Admission, Envelope},
    effects::{Effect, View},
    resource::{Resource, Socket, registration_io},
    *,
};
use mio::{Events, Token, Waker};
use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Write},
    marker::PhantomData,
    net,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
const WAKE: Token = Token(0);
pub struct IoRuntime<C: Send + 'static> {
    core: RuntimeCore,
    receiver: mpsc::Receiver<Envelope<C>>,
    control: ControlHandle<C>,
    _local: PhantomData<Rc<()>>,
}
pub(crate) struct RuntimeCore {
    config: RuntimeConfig,
    owner: u64,
    poll: Backend,
    events: Events,
    resources: Vec<Option<Resource>>,
    generations: Vec<u64>,
    tokens: HashMap<Token, Key>,
    next_token: usize,
    next_id: u64,
    state: Lifecycle,
    admission: Arc<Mutex<Admission>>,
    pending: VecDeque<OwnedEvent>,
    credits: HashMap<Key, (usize, usize)>,
    bytes: usize,
    messages: usize,
    cursor: usize,
    scratch: Vec<u8>,
    report: ShutdownReport,
    queue_saturated: bool,
}
enum OwnedEvent {
    Eof(TcpHandle),
    Oversized(UdpHandle),
    Transport(ResourceHandle, &'static str, io::Error),
    Write(WriteOutcome),
    Registered(RegistrationId, Result<ResourceHandle, RegistrationError>),
    EffectFailed(ResourceHandle, io::Error),
    Closed(ResourceHandle),
    Driver(&'static str, bool),
}
impl OwnedEvent {
    fn event(self) -> RuntimeEvent<'static> {
        match self {
            Self::Eof(h) => RuntimeEvent::PeerEof(h),
            Self::Oversized(h) => RuntimeEvent::OversizedDatagram(h),
            Self::Transport(resource, operation, source) => RuntimeEvent::TransportError {
                resource,
                operation,
                source,
            },
            Self::Write(w) => RuntimeEvent::Write(w),
            Self::Registered(request, result) => RuntimeEvent::Registered { request, result },
            Self::EffectFailed(resource, source) => RuntimeEvent::EffectFailed { resource, source },
            Self::Closed(h) => RuntimeEvent::Closed(h),
            Self::Driver(label, false) => RuntimeEvent::DriverProgress { label },
            Self::Driver(label, true) => RuntimeEvent::DriverDisabled { label },
        }
    }
}
impl<C: Send + 'static> IoRuntime<C> {
    pub fn new(config: RuntimeConfig) -> Result<Self, InitError> {
        config.validate()?;
        let owner = NEXT_OWNER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| InitError::IdentityExhausted)?;
        let poll = Backend::new().map_err(InitError::Io)?;
        let waker = Arc::new(Waker::new(poll.registry(), WAKE).map_err(InitError::Io)?);
        let admission = Arc::new(Mutex::new(Admission {
            state: Lifecycle::Running,
            shutdown: None,
            waker: Some(waker),
        }));
        let (sender, receiver) = mpsc::sync_channel(config.command_capacity);
        let control = ControlHandle {
            sender,
            admission: admission.clone(),
        };
        let core = RuntimeCore {
            events: Events::with_capacity(config.poll_capacity),
            scratch: vec![
                0;
                config
                    .max_datagram_size
                    .max(config.bytes_per_resource)
                    .min(65536)
                    + 1
            ],
            config,
            owner,
            poll,
            resources: Vec::new(),
            generations: Vec::new(),
            tokens: HashMap::new(),
            next_token: 1,
            next_id: 1,
            state: Lifecycle::Running,
            admission,
            pending: VecDeque::new(),
            credits: HashMap::new(),
            bytes: 0,
            messages: 0,
            cursor: 0,
            report: ShutdownReport::default(),
            queue_saturated: false,
        };
        Ok(Self {
            core,
            receiver,
            control,
            _local: PhantomData,
        })
    }
    pub fn control_handle(&self) -> ControlHandle<C> {
        self.control.clone()
    }
    pub fn lifecycle(&self) -> Lifecycle {
        self.core.state
    }
    /// Completion events retain message credits but no payload backing storage.
    pub fn queue_usage(&self) -> QueueUsage {
        QueueUsage {
            retained_bytes: self.core.bytes,
            messages: self.core.messages,
            pending_wire_bytes: self
                .core
                .resources
                .iter()
                .flatten()
                .flat_map(|r| &r.output)
                .map(|o| o.message.data().len() - o.offset)
                .sum(),
        }
    }

    pub fn register_tcp(
        &mut self,
        socket: net::TcpStream,
        options: TcpResourceOptions,
    ) -> Result<TcpHandle, RegistrationError> {
        self.core.register_tcp(socket, options)
    }
    pub fn register_udp(
        &mut self,
        socket: net::UdpSocket,
        options: UdpResourceOptions,
    ) -> Result<UdpHandle, RegistrationError> {
        self.core.register_udp(socket, options)
    }
    pub fn register_listener(
        &mut self,
        socket: net::TcpListener,
        options: ListenerOptions,
    ) -> Result<ListenerHandle, RegistrationError> {
        self.core.register_listener(socket, options)
    }
    pub fn run<A: Application<Command = C>>(
        &mut self,
        app: &mut A,
        drivers: &mut DriverSet<'_, A>,
    ) -> Result<ShutdownReport, RunFailure<A::Error>> {
        while self.step(app, drivers, None)? != StepOutcome::Stopped {}
        Ok(std::mem::take(&mut self.core.report))
    }
    pub fn take_shutdown_report(&mut self) -> Option<ShutdownReport> {
        (self.core.state == Lifecycle::Stopped).then(|| std::mem::take(&mut self.core.report))
    }
    pub fn step<A: Application<Command = C>>(
        &mut self,
        app: &mut A,
        drivers: &mut DriverSet<'_, A>,
        max_wait: Option<Duration>,
    ) -> Result<StepOutcome, RunFailure<A::Error>> {
        let result = catch_unwind(AssertUnwindSafe(|| self.step_inner(app, drivers, max_wait)));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(cause)) => {
                crate::diagnostics::event!(
                    error,
                    "osmocore_rs::runtime",
                    operation = "runtime_failure"
                );
                self.stop();
                Err(RunFailure {
                    cause,
                    report: std::mem::take(&mut self.core.report),
                })
            }
            Err(panic) => {
                self.stop();
                resume_unwind(panic)
            }
        }
    }
    fn stop(&mut self) {
        self.core.stop();
        while let Ok(e) = self.receiver.try_recv() {
            let _ = e.receipt.try_send(CommandStatus::Rejected);
        }
    }
    fn lifecycle_checkpoint<A: Application<Command = C>>(
        &mut self,
        app: &mut A,
    ) -> Result<(), FailureCause<A::Error>> {
        let requested = control::lock(&self.core.admission).shutdown;
        if let Some(deadline) = requested {
            if self.core.state == Lifecycle::Running {
                crate::diagnostics::event!(debug, "osmocore_rs::runtime", operation = "draining");
                self.core.state = Lifecycle::Draining;
                control::lock(&self.core.admission).state = Lifecycle::Draining;
                while let Ok(e) = self.receiver.try_recv() {
                    let _ = e.receipt.try_send(CommandStatus::Rejected);
                }
                self.core
                    .callback(true, |cx| app.on_shutdown(cx))
                    .map_err(FailureCause::Application)?;
                self.core.refresh_interests();
            }
            // Read the latch again: the hook or another thread may escalate.
            let deadline = control::lock(&self.core.admission)
                .shutdown
                .unwrap_or(deadline);
            crate::diagnostics::event!(
                trace,
                "osmocore_rs::runtime",
                operation = "drain_check",
                remaining_drain_ms = deadline
                    .map(|d| d.saturating_duration_since(Instant::now()).as_millis() as u64)
            );
            if deadline.is_none()
                || self.core.queued_outputs() == 0
                || deadline.is_some_and(|d| Instant::now() >= d)
            {
                self.core.report.timed_out = deadline.is_some_and(|d| Instant::now() >= d)
                    && self.core.queued_outputs() != 0;
                if self.core.report.timed_out {
                    crate::diagnostics::event!(
                        warn,
                        "osmocore_rs::runtime",
                        operation = "drain_timeout"
                    );
                }
                self.stop();
            }
        }
        Ok(())
    }
    fn commands<A: Application<Command = C>>(
        &mut self,
        app: &mut A,
    ) -> Result<(), FailureCause<A::Error>> {
        for _ in 0..self.core.config.commands_per_step {
            self.lifecycle_checkpoint(app)?;
            if self.core.state != Lifecycle::Running || self.core.normal_room() == 0 {
                break;
            }
            let Ok(envelope) = self.receiver.try_recv() else {
                break;
            };
            let receipt = control::ReceiptGuard::new(envelope.receipt);
            let result = envelope.context.scope(|| {
                self.core
                    .callback(false, |cx| app.on_command(cx, envelope.command))
            });
            receipt.resolve(if result.is_ok() {
                CommandStatus::Processed
            } else {
                CommandStatus::Failed
            });
            result.map_err(FailureCause::Application)?;
        }
        Ok(())
    }
    fn notifications<A: Application<Command = C>>(
        &mut self,
        app: &mut A,
        remaining: &mut usize,
    ) -> Result<(), FailureCause<A::Error>> {
        while *remaining != 0 {
            self.lifecycle_checkpoint(app)?;
            if self.core.state == Lifecycle::Stopped {
                break;
            }
            let Some(event) = self.core.pending.pop_front() else {
                break;
            };
            *remaining -= 1;
            if let OwnedEvent::Write(ref outcome) = event {
                self.core.release_message(outcome.resource.key());
            }
            let context = match &event {
                OwnedEvent::Write(w) => w.context.clone(),
                _ => TraceContext::capture(),
            };
            context
                .scope(|| {
                    self.core
                        .callback(false, |cx| app.on_event(cx, event.event()))
                })
                .map_err(FailureCause::Application)?;
        }
        Ok(())
    }
    fn step_inner<A: Application<Command = C>>(
        &mut self,
        app: &mut A,
        drivers: &mut DriverSet<'_, A>,
        max_wait: Option<Duration>,
    ) -> Result<StepOutcome, FailureCause<A::Error>> {
        let mut notifications_left = self.core.config.notifications_per_step;
        self.lifecycle_checkpoint(app)?;
        if self.core.state == Lifecycle::Stopped {
            return Ok(StepOutcome::Stopped);
        }
        self.notifications(app, &mut notifications_left)?;
        self.commands(app)?;
        let mut wait = self
            .core
            .config
            .max_poll_interval
            .min(max_wait.unwrap_or(Duration::MAX));
        if self.core.state == Lifecycle::Running {
            let driver_wait = self.core.drive(app, drivers)?;
            wait = wait.min(driver_wait);
        }
        self.lifecycle_checkpoint(app)?;
        if self.core.state == Lifecycle::Stopped {
            return Ok(StepOutcome::Stopped);
        }
        if !self.core.pending.is_empty()
            || self.core.resources.iter().flatten().any(|r| {
                r.read_ready && !r.paused && !r.eof && self.core.state == Lifecycle::Running
                    || r.write_ready && !r.output.is_empty()
            })
        {
            wait = Duration::ZERO;
        }
        // A full command quantum may leave queued work; never block behind it.
        if let Ok(e) = self.receiver.try_recv() {
            if self.core.state == Lifecycle::Running {
                let receipt = control::ReceiptGuard::new(e.receipt);
                let result = e.context.scope(|| {
                    self.core
                        .callback(false, |cx| app.on_command(cx, e.command))
                });
                receipt.resolve(if result.is_ok() {
                    CommandStatus::Processed
                } else {
                    CommandStatus::Failed
                });
                result.map_err(FailureCause::Application)?;
            } else {
                let _ = e.receipt.try_send(CommandStatus::Rejected);
            }
            wait = Duration::ZERO;
        }
        if let Some(Some(deadline)) = control::lock(&self.core.admission).shutdown {
            wait = wait.min(deadline.saturating_duration_since(Instant::now()));
        }
        self.core.refresh_interests();
        match self.core.poll.poll(&mut self.core.events, Some(wait)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(StepOutcome::Progress),
            Err(e) => return Err(FailureCause::Poll(e)),
        }
        for event in &self.core.events {
            crate::diagnostics::event!(trace, "osmocore_rs::runtime", operation = "readiness");
            if let Some(key) = self.core.tokens.get(&event.token()).copied()
                && let Some(r) = self
                    .core
                    .resources
                    .get_mut(key.slot)
                    .and_then(Option::as_mut)
                    .filter(|r| r.key == key)
            {
                r.read_ready |= event.is_readable() || event.is_read_closed() || event.is_error();
                r.write_ready |= event.is_writable() || event.is_write_closed() || event.is_error();
            }
        }
        self.commands(app)?;
        let count = self.core.resources.len();
        for offset in 0..count {
            self.lifecycle_checkpoint(app)?;
            if self.core.state == Lifecycle::Stopped {
                break;
            }
            let slot = (self.core.cursor + offset) % count;
            let key = self.core.resources[slot].as_ref().map(|r| r.key);
            if let Some(key) = key {
                self.core.service(key, app)?;
                self.notifications(app, &mut notifications_left)?;
            }
        }
        if count != 0 {
            self.core.cursor = (self.core.cursor + 1) % count;
        }
        self.notifications(app, &mut notifications_left)?;
        self.lifecycle_checkpoint(app)?;
        Ok(if self.core.state == Lifecycle::Stopped {
            StepOutcome::Stopped
        } else {
            StepOutcome::Progress
        })
    }
}
impl<C: Send + 'static> Drop for IoRuntime<C> {
    fn drop(&mut self) {
        self.stop();
    }
}

impl RuntimeCore {
    fn normal_room(&self) -> usize {
        let normal = self
            .pending
            .iter()
            .filter(|e| !matches!(e, OwnedEvent::Write(_)))
            .count();
        self.config.notification_capacity.saturating_sub(normal)
    }
    fn get(&self, key: Key) -> Option<&Resource> {
        self.resources
            .get(key.slot)?
            .as_ref()
            .filter(|r| r.key == key)
    }
    fn get_mut(&mut self, key: Key) -> Option<&mut Resource> {
        self.resources
            .get_mut(key.slot)?
            .as_mut()
            .filter(|r| r.key == key)
    }
    fn allocate_token(&mut self) -> io::Result<Token> {
        let token = self.next_token;
        self.next_token = token
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Mio token space exhausted"))?;
        Ok(Token(token))
    }
    fn install(
        &mut self,
        socket: Socket,
        paused: bool,
    ) -> Result<ResourceHandle, RegistrationError> {
        if self.state != Lifecycle::Running || control::lock(&self.admission).shutdown.is_some() {
            return Err(RegistrationError::Rejected(Rejection::Lifecycle));
        }
        let slot = self
            .resources
            .iter()
            .enumerate()
            .find_map(|(i, r)| (r.is_none() && self.generations[i] < u64::MAX).then_some(i))
            .unwrap_or(self.resources.len());
        if slot >= self.config.max_resources {
            return Err(RegistrationError::Rejected(
                if self.resources.iter().flatten().count() >= self.config.max_resources {
                    Rejection::Capacity
                } else {
                    Rejection::Exhausted
                },
            ));
        }
        let generation = if slot == self.resources.len() {
            1
        } else {
            self.generations[slot] + 1
        };
        let key = Key {
            owner: self.owner,
            slot,
            generation,
        };
        let mut resource = Resource {
            key,
            socket,
            token: None,
            interest: None,
            paused,
            eof: false,
            close_after_flush: false,
            read_ready: !paused,
            write_ready: false,
            output: VecDeque::new(),
        };
        if let Some(interest) = resource.desired_interest(true) {
            let token = self
                .allocate_token()
                .map_err(|e| registration_io("token", e))?;
            self.poll
                .registry()
                .register(resource.source(), token, interest)
                .map_err(|e| registration_io("register", e))?;
            resource.token = Some(token);
            resource.interest = Some(interest);
            self.tokens.insert(token, key);
        }
        let handle = resource.handle();
        if slot == self.resources.len() {
            self.resources.push(Some(resource));
            self.generations.push(generation);
        } else {
            self.resources[slot] = Some(resource);
            self.generations[slot] = generation;
        }
        crate::diagnostics::event!(
            debug,
            "osmocore_rs::runtime",
            operation = "register",
            slot,
            generation
        );
        Ok(handle)
    }
    fn register_tcp(
        &mut self,
        socket: net::TcpStream,
        options: TcpResourceOptions,
    ) -> Result<TcpHandle, RegistrationError> {
        socket
            .set_nonblocking(true)
            .map_err(|e| registration_io("nonblocking", e))?;
        socket
            .set_nodelay(options.nodelay)
            .map_err(|e| registration_io("nodelay", e))?;
        match self.install(
            Socket::Tcp(mio::net::TcpStream::from_std(socket), options),
            options.read_paused,
        )? {
            ResourceHandle::Tcp(h) => Ok(h),
            _ => unreachable!(),
        }
    }
    fn register_udp(
        &mut self,
        socket: net::UdpSocket,
        options: UdpResourceOptions,
    ) -> Result<UdpHandle, RegistrationError> {
        socket
            .set_nonblocking(true)
            .map_err(|e| registration_io("nonblocking", e))?;
        let peer = match socket.peer_addr() {
            Ok(peer) => Some(peer),
            Err(e) if e.kind() == io::ErrorKind::NotConnected => None,
            Err(e) => return Err(registration_io("peer", e)),
        };
        match self.install(
            Socket::Udp(mio::net::UdpSocket::from_std(socket), peer),
            options.read_paused,
        )? {
            ResourceHandle::Udp(h) => Ok(h),
            _ => unreachable!(),
        }
    }
    fn register_listener(
        &mut self,
        socket: net::TcpListener,
        options: ListenerOptions,
    ) -> Result<ListenerHandle, RegistrationError> {
        socket
            .set_nonblocking(true)
            .map_err(|e| registration_io("nonblocking", e))?;
        match self.install(
            Socket::Listener(mio::net::TcpListener::from_std(socket), options),
            false,
        )? {
            ResourceHandle::Listener(h) => Ok(h),
            _ => unreachable!(),
        }
    }
    fn interest(&mut self, key: Key) -> io::Result<()> {
        let Some(resource) = self.get(key) else {
            return Ok(());
        };
        let listener_full = matches!(resource.socket, Socket::Listener(..))
            && self.resources.iter().flatten().count() >= self.config.max_resources;
        let desired = resource.desired_interest(self.state == Lifecycle::Running && !listener_full);
        if desired == resource.interest {
            return Ok(());
        }
        let new_token = if desired.is_some() && resource.token.is_none() {
            Some(self.allocate_token()?)
        } else {
            resource.token
        };
        let resource = self.resources[key.slot].as_mut().expect("live key");
        match (resource.token, desired) {
            (Some(token), Some(interest)) => {
                self.poll
                    .registry()
                    .reregister(resource.source(), token, interest)?
            }
            (None, Some(interest)) => {
                let token = new_token.expect("allocated token");
                self.poll
                    .registry()
                    .register(resource.source(), token, interest)?;
                self.tokens.insert(token, key);
                resource.token = Some(token);
            }
            (Some(token), None) => {
                self.poll.registry().deregister(resource.source())?;
                self.tokens.remove(&token);
                resource.token = None;
            }
            _ => {}
        }
        crate::diagnostics::event!(
            debug,
            "osmocore_rs::runtime",
            operation = "interest",
            slot = key.slot,
            generation = key.generation
        );
        resource.interest = desired;
        Ok(())
    }
    fn refresh_interests(&mut self) {
        for slot in 0..self.resources.len() {
            if self.normal_room() < 3 {
                break;
            }
            if let Some(key) = self.resources[slot].as_ref().map(|r| r.key)
                && let Err(source) = self.interest(key)
            {
                let handle = self.get(key).expect("live interest resource").handle();
                self.pending
                    .push_back(OwnedEvent::EffectFailed(handle, source));
                self.close(key, true);
            }
        }
    }
    fn callback<E>(
        &mut self,
        final_output: bool,
        callback: impl FnOnce(&mut RuntimeContext<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        let view = self
            .resources
            .iter()
            .flatten()
            .map(|r| {
                let (bytes, messages) = self.credits.get(&r.key).copied().unwrap_or_default();
                (
                    r.key,
                    View {
                        handle: r.handle(),
                        bytes,
                        messages,
                        closed: false,
                        connected: matches!(r.socket, Socket::Udp(_, Some(_))),
                    },
                )
            })
            .collect();
        let mut config = self.config.clone();
        config.effects_per_callback = if final_output {
            config.shutdown_effects
        } else {
            config
                .effects_per_callback
                .min(self.normal_room().saturating_sub(1) / 2)
        };
        let accepting = final_output
            || self.state == Lifecycle::Running
                && control::lock(&self.admission).shutdown.is_none();
        let mut cx = RuntimeContext {
            config: &config,
            owner: self.owner,
            view,
            effects: Vec::new(),
            bytes: self.bytes,
            messages: self.messages,
            next_id: &mut self.next_id,
            accepting,
            registering: self.state == Lifecycle::Running && accepting,
        };
        crate::diagnostics::event!(
            trace,
            "osmocore_rs::runtime",
            operation = "callback",
            queue_bytes = self.bytes,
            queue_messages = self.messages
        );
        let result = catch_unwind(AssertUnwindSafe(|| callback(&mut cx)));
        let effects = std::mem::take(&mut cx.effects);
        drop(cx);
        match result {
            Ok(Ok(())) => {
                self.apply(effects, final_output);
                Ok(())
            }
            Ok(Err(e)) => {
                self.cancel_effects(effects);
                Err(e)
            }
            Err(panic) => {
                self.cancel_effects(effects);
                resume_unwind(panic)
            }
        }
    }
    fn cancel_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Send { resource, output } => {
                    self.messages += 1;
                    self.credits.entry(resource.key()).or_default().1 += 1;
                    self.pending.push_back(OwnedEvent::Write(WriteOutcome {
                        context: output.context,
                        id: output.id,
                        resource,
                        bytes_written: 0,
                        status: WriteStatus::Cancelled,
                    }));
                }
                Effect::Tcp(id, _, _) | Effect::Udp(id, _, _) | Effect::Listener(id, _, _) => {
                    self.pending.push_back(OwnedEvent::Registered(
                        id,
                        Err(RegistrationError::Rejected(Rejection::Cancelled)),
                    ));
                }
                _ => {}
            }
        }
    }
    fn apply(&mut self, effects: Vec<Effect>, final_output: bool) {
        for effect in effects {
            crate::diagnostics::event!(trace, "osmocore_rs::runtime", operation = "apply_effect");
            let latched = control::lock(&self.admission).shutdown.is_some();
            match effect {
                Effect::Send { resource, output } => {
                    let bytes = usize::from(output.message.capacity());
                    if (!final_output && latched) || self.get(resource.key()).is_none() {
                        self.messages += 1;
                        self.credits.entry(resource.key()).or_default().1 += 1;
                        self.pending.push_back(OwnedEvent::Write(WriteOutcome {
                            context: output.context,
                            id: output.id,
                            resource,
                            bytes_written: 0,
                            status: WriteStatus::Cancelled,
                        }));
                    } else {
                        self.bytes += bytes;
                        self.messages += 1;
                        let credit = self.credits.entry(resource.key()).or_default();
                        credit.0 += bytes;
                        credit.1 += 1;
                        let r = self.get_mut(resource.key()).expect("validated key");
                        r.output.push_back(output);
                        r.write_ready = true;
                    }
                }
                Effect::Close(handle) => self.close(handle.key(), true),
                Effect::Pause(handle, paused) => {
                    if let Some(r) = self.get_mut(handle.key()) {
                        r.paused = paused;
                        r.read_ready = !paused;
                    }
                }
                Effect::Tcp(id, socket, options) => {
                    let result = self.register_tcp(socket, options).map(ResourceHandle::Tcp);
                    self.pending.push_back(OwnedEvent::Registered(id, result));
                }
                Effect::Udp(id, socket, options) => {
                    let result = self.register_udp(socket, options).map(ResourceHandle::Udp);
                    self.pending.push_back(OwnedEvent::Registered(id, result));
                }
                Effect::Listener(id, socket, options) => {
                    let result = self
                        .register_listener(socket, options)
                        .map(ResourceHandle::Listener);
                    self.pending.push_back(OwnedEvent::Registered(id, result));
                }
                Effect::Shutdown(deadline) => {
                    let mut admission = control::lock(&self.admission);
                    admission.shutdown = Some(match admission.shutdown {
                        Some(None) => None,
                        Some(Some(old)) => deadline.map(|d| d.min(old)),
                        None => deadline,
                    });
                }
            }
        }
        self.saturation_transition();
    }
    fn saturation_transition(&mut self) {
        let saturated = self.bytes >= self.config.total_bytes
            || self.messages >= self.config.total_messages
            || self.credits.values().any(|(bytes, messages)| {
                *bytes >= self.config.per_resource_bytes
                    || *messages >= self.config.per_resource_messages
            });
        if saturated && !self.queue_saturated {
            crate::diagnostics::event!(
                warn,
                "osmocore_rs::runtime",
                operation = "queue_saturated",
                queue_bytes = self.bytes,
                queue_messages = self.messages
            );
        }
        self.queue_saturated = saturated;
    }
    fn release_message(&mut self, key: Key) {
        self.messages -= 1;
        if let Some(credit) = self.credits.get_mut(&key) {
            credit.1 -= 1;
            if credit.1 == 0 {
                self.credits.remove(&key);
            }
        }
        self.saturation_transition();
    }
    fn complete(&mut self, key: Key, output: super::resource::Output, status: WriteStatus) {
        let resource = self.get(key).expect("live output resource").handle();
        let bytes = usize::from(output.message.capacity());
        self.bytes -= bytes;
        self.credits.get_mut(&key).expect("output credit").0 -= bytes;
        output.context.scope(|| {
            crate::diagnostics::event!(
                debug,
                "osmocore_rs::runtime",
                operation = "write_complete",
                slot = key.slot,
                generation = key.generation,
                bytes_written = output.offset
            );
        });
        self.pending.push_back(OwnedEvent::Write(WriteOutcome {
            context: output.context,
            id: output.id,
            resource,
            bytes_written: output.offset,
            status,
        }));
    }
    fn close(&mut self, key: Key, notify: bool) {
        if self.get(key).is_none() {
            return;
        }
        while let Some(output) = self.get_mut(key).and_then(|r| r.output.pop_front()) {
            self.complete(key, output, WriteStatus::Cancelled);
        }
        let mut resource = self.resources[key.slot].take().expect("live resource");
        if let Some(token) = resource.token {
            self.tokens.remove(&token);
            if let Err(e) = self.poll.registry().deregister(resource.source())
                && notify
            {
                self.pending
                    .push_back(OwnedEvent::EffectFailed(resource.handle(), e));
            }
        }
        if notify {
            self.pending
                .push_back(OwnedEvent::Closed(resource.handle()));
        }
        for listener in self
            .resources
            .iter_mut()
            .flatten()
            .filter(|r| matches!(r.socket, Socket::Listener(..)) && !r.paused)
        {
            listener.read_ready = true;
        }
        crate::diagnostics::event!(
            debug,
            "osmocore_rs::runtime",
            operation = "close",
            slot = key.slot,
            generation = key.generation
        );
    }
    fn queued_outputs(&self) -> usize {
        self.resources
            .iter()
            .flatten()
            .map(|r| r.output.len())
            .sum()
    }
    fn stop(&mut self) {
        if self.state == Lifecycle::Stopped {
            return;
        }
        self.state = Lifecycle::Stopped;
        {
            let mut admission = control::lock(&self.admission);
            admission.state = Lifecycle::Stopped;
            admission.waker = None;
        }
        for slot in 0..self.resources.len() {
            if let Some(key) = self.resources[slot].as_ref().map(|r| r.key) {
                self.close(key, false);
            }
        }
        while let Some(event) = self.pending.pop_front() {
            match event {
                OwnedEvent::Write(outcome) => self.report.writes.push(outcome),
                OwnedEvent::Registered(id, outcome) => {
                    self.report.registrations.push((id, outcome))
                }
                _ => {}
            }
        }
        self.credits.clear();
        self.bytes = 0;
        self.messages = 0;
        crate::diagnostics::event!(
            debug,
            "osmocore_rs::runtime",
            operation = "stopped",
            timed_out = self.report.timed_out
        );
    }
    fn service<A: Application>(
        &mut self,
        key: Key,
        app: &mut A,
    ) -> Result<(), FailureCause<A::Error>> {
        let Some(r) = self.get(key) else {
            return Ok(());
        };
        if r.read_ready
            && !r.paused
            && !r.eof
            && self.state == Lifecycle::Running
            && control::lock(&self.admission).shutdown.is_none()
        {
            let mut scratch = std::mem::take(&mut self.scratch);
            let result = self.read(key, app, &mut scratch);
            self.scratch = scratch;
            result?;
        }
        if self.normal_room() < 3 {
            return Ok(());
        }
        self.write(key);
        if self
            .get(key)
            .is_some_and(|r| r.close_after_flush && r.output.is_empty())
            && self.normal_room() >= 2
        {
            self.close(key, true);
        }
        if let Err(source) = self.interest(key) {
            if let Some(r) = self.get(key) {
                self.pending
                    .push_back(OwnedEvent::EffectFailed(r.handle(), source));
            }
            self.close(key, true);
        }
        Ok(())
    }
    fn read<A: Application>(
        &mut self,
        key: Key,
        app: &mut A,
        scratch: &mut [u8],
    ) -> Result<(), FailureCause<A::Error>> {
        let mut bytes = 0;
        let mut accepts = 0;
        for _ in 0..self.config.io_calls_per_resource {
            if control::lock(&self.admission).shutdown.is_some() || self.normal_room() < 3 {
                break;
            }
            let limit = self
                .config
                .bytes_per_resource
                .saturating_sub(bytes)
                .min(scratch.len());
            if limit == 0 {
                break;
            }
            let max_datagram = self.config.max_datagram_size;
            let resource_count = self.resources.iter().flatten().count();
            let resource_cap = self.config.max_resources;
            let accept_cap = self.config.accepts_per_resource;
            let Some(r) = self
                .get_mut(key)
                .filter(|r| !r.paused && !r.eof && r.read_ready)
            else {
                break;
            };
            enum Input {
                Tcp(usize),
                Udp(usize, net::SocketAddr),
                Accepted(mio::net::TcpStream, net::SocketAddr, ListenerOptions),
            }
            let handle = r.handle();
            let input = match &mut r.socket {
                Socket::Tcp(socket, _) => socket.read(&mut scratch[..limit]).map(Input::Tcp),
                Socket::Udp(socket, _) => socket
                    .recv_from(&mut scratch[..max_datagram + 1])
                    .map(|(n, a)| Input::Udp(n, a)),
                Socket::Listener(socket, options) => {
                    if resource_count >= resource_cap {
                        r.read_ready = false;
                        break;
                    }
                    if accepts >= accept_cap {
                        break;
                    }
                    socket
                        .accept()
                        .map(|(s, a)| Input::Accepted(s, a, *options))
                }
            };
            match input {
                Ok(Input::Tcp(0)) => {
                    let r = self.get_mut(key).expect("read resource");
                    r.eof = true;
                    r.read_ready = false;
                    if let Socket::Tcp(_, options) = r.socket {
                        r.close_after_flush = !options.retain_write_half;
                    }
                    self.pending.push_back(OwnedEvent::Eof(TcpHandle(key)));
                    break;
                }
                Ok(Input::Tcp(n)) => {
                    crate::diagnostics::event!(
                        trace,
                        "osmocore_rs::net",
                        operation = "read",
                        resource_kind = "tcp",
                        slot = key.slot,
                        generation = key.generation,
                        bytes = n
                    );
                    bytes += n;
                    self.callback(false, |cx| {
                        app.on_event(
                            cx,
                            RuntimeEvent::TcpBytes {
                                stream: TcpHandle(key),
                                bytes: &scratch[..n],
                            },
                        )
                    })
                    .map_err(FailureCause::Application)?;
                }
                Ok(Input::Udp(n, source)) => {
                    crate::diagnostics::event!(
                        trace,
                        "osmocore_rs::net",
                        operation = "receive",
                        resource_kind = "udp",
                        slot = key.slot,
                        generation = key.generation,
                        bytes = n
                    );
                    bytes += n;
                    if n > max_datagram {
                        self.pending
                            .push_back(OwnedEvent::Oversized(UdpHandle(key)));
                    } else {
                        self.callback(false, |cx| {
                            app.on_event(
                                cx,
                                RuntimeEvent::UdpDatagram {
                                    socket: UdpHandle(key),
                                    bytes: &scratch[..n],
                                    source,
                                },
                            )
                        })
                        .map_err(FailureCause::Application)?;
                    }
                }
                Ok(Input::Accepted(socket, peer, options)) => {
                    accepts += 1;
                    // Mio's accept creates a nonblocking source on every platform.
                    // Set the accepted stream option explicitly before installation.
                    let result = socket
                        .set_nodelay(options.accepted.nodelay)
                        .map_err(|e| registration_io("accepted_nodelay", e))
                        .and_then(|()| self.install(Socket::Tcp(socket, options.accepted), true));
                    match result {
                        Ok(ResourceHandle::Tcp(stream)) => {
                            self.callback(false, |cx| {
                                app.on_event(
                                    cx,
                                    RuntimeEvent::Accepted {
                                        listener: ListenerHandle(key),
                                        stream,
                                        peer,
                                    },
                                )
                            })
                            .map_err(FailureCause::Application)?;
                            // Setup callback may close or explicitly pause the stream.
                            // Accepted streams start paused; app resumes when configured.
                        }
                        Err(RegistrationError::Io { source, .. }) => self
                            .pending
                            .push_back(OwnedEvent::Transport(handle, "accept_setup", source)),
                        _ => {}
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(r) = self.get_mut(key) {
                        r.read_ready = false;
                    }
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    crate::diagnostics::event!(warn, "osmocore_rs::net", operation = "read_failure", slot = key.slot, generation = key.generation, error_category = ?e.kind());
                    self.pending
                        .push_back(OwnedEvent::Transport(handle, "read", e));
                    self.close(key, true);
                    break;
                }
            }
        }
        Ok(())
    }
    fn write(&mut self, key: Key) {
        let mut bytes = 0;
        for _ in 0..self.config.io_calls_per_resource {
            let limit = self.config.bytes_per_resource.saturating_sub(bytes);
            if limit == 0 {
                break;
            }
            let Some(r) = self.get_mut(key).filter(|r| r.write_ready) else {
                break;
            };
            let Some(front) = r.output.front_mut() else {
                break;
            };
            let is_tcp = matches!(r.socket, Socket::Tcp(..));
            let result = super::output::attempt(front, is_tcp, limit, |data, destination| {
                match &mut r.socket {
                    Socket::Tcp(socket, _) => socket.write(data),
                    Socket::Udp(socket, _) => match destination {
                        Some(destination) => socket.send_to(data, destination),
                        None => socket.send(data),
                    },
                    Socket::Listener(..) => unreachable!("listener has no output"),
                }
            });
            match result {
                super::output::Attempt::Progress(n) => bytes += n,
                super::output::Attempt::Complete(n) => {
                    bytes += n;
                    let output = self
                        .get_mut(key)
                        .expect("write resource")
                        .output
                        .pop_front()
                        .expect("write entry");
                    self.complete(key, output, WriteStatus::Complete);
                }
                super::output::Attempt::Blocked => {
                    self.get_mut(key).expect("write resource").write_ready = false;
                    break;
                }
                super::output::Attempt::Interrupted => continue,
                super::output::Attempt::Failed(e) => {
                    let output = self
                        .get_mut(key)
                        .expect("write resource")
                        .output
                        .pop_front()
                        .expect("write entry");
                    self.complete(key, output, WriteStatus::Failed(e));
                    if is_tcp {
                        self.close(key, true);
                        break;
                    }
                }
            }
        }
    }
    fn drive<A: Application>(
        &mut self,
        app: &mut A,
        drivers: &mut DriverSet<'_, A>,
    ) -> Result<Duration, FailureCause<A::Error>> {
        let count = drivers.entries.len();
        for offset in 0..count.min(self.config.driver_invocations_per_step) {
            if control::lock(&self.admission).shutdown.is_some() || self.normal_room() < 3 {
                break;
            }
            let index = (drivers.cursor + offset) % count;
            let entry = &mut drivers.entries[index];
            if entry.disabled {
                continue;
            }
            crate::diagnostics::event!(
                trace,
                "osmocore_rs::runtime",
                operation = "driver_check",
                driver_label = entry.label
            );
            let result = entry.driver.prepare(app).and_then(|_| {
                let mut progress = DriverProgress::Idle;
                self.callback(false, |cx| {
                    progress = entry.driver.drive(app, cx)?;
                    Ok(())
                })
                .map(|()| {
                    if progress == DriverProgress::Progress {
                        self.pending
                            .push_back(OwnedEvent::Driver(entry.label, false));
                    }
                })
            });
            if let Err(failure) = result {
                crate::diagnostics::event!(
                    warn,
                    "osmocore_rs::runtime",
                    operation = "driver_failure",
                    driver_label = entry.label
                );
                if entry.policy == DriverFailurePolicy::Optional {
                    entry.disabled = true;
                    entry.failure = Some(failure);
                    self.pending
                        .push_back(OwnedEvent::Driver(entry.label, true));
                } else {
                    return Err(FailureCause::Driver {
                        label: entry.label,
                        failure,
                    });
                }
            }
        }
        if count != 0 {
            drivers.cursor =
                (drivers.cursor + self.config.driver_invocations_per_step.min(count)) % count;
        }
        let mut wait = self.config.max_poll_interval;
        for entry in &mut drivers.entries {
            if entry.disabled {
                continue;
            }
            match entry.driver.prepare(app) {
                Ok(WakeHint::After(d)) => wait = wait.min(d),
                Ok(_) => {}
                Err(failure) => {
                    if entry.policy == DriverFailurePolicy::Optional {
                        entry.disabled = true;
                        entry.failure = Some(failure);
                        if self.normal_room() != 0 {
                            self.pending
                                .push_back(OwnedEvent::Driver(entry.label, true));
                        }
                    } else {
                        return Err(FailureCause::Driver {
                            label: entry.label,
                            failure,
                        });
                    }
                }
            }
        }
        Ok(wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msgb::Msgb;
    fn message(capacity: usize, text: &[u8]) -> Msgb {
        let mut m = Msgb::new(capacity).unwrap();
        m.put(text.len()).unwrap();
        m.data_mut().copy_from_slice(text);
        m
    }
    fn runtime() -> IoRuntime<()> {
        IoRuntime::new(RuntimeConfig::default()).unwrap()
    }
    fn udp(rt: &mut IoRuntime<()>) -> UdpHandle {
        rt.register_udp(
            net::UdpSocket::bind("127.0.0.1:0").unwrap(),
            UdpResourceOptions::default(),
        )
        .unwrap()
    }
    #[allow(clippy::result_large_err)]
    fn send(
        cx: &mut RuntimeContext<'_>,
        h: UdpHandle,
        capacity: usize,
    ) -> Result<WriteId, EnqueueError<Datagram>> {
        cx.try_send_udp(
            h,
            Datagram {
                message: message(capacity, b"a"),
                destination: Some("127.0.0.1:9".parse().unwrap()),
            },
        )
    }
    #[test]
    fn quota_reservations_charge_backing_and_rejections_return_ownership() {
        let mut rt = IoRuntime::<()>::new(RuntimeConfig {
            per_resource_bytes: 16,
            ..RuntimeConfig::default()
        })
        .unwrap();
        let h = udp(&mut rt);
        rt.core
            .callback(false, |cx| {
                send(cx, h, 16).unwrap();
                let e = send(cx, h, 16).err().unwrap();
                assert_eq!(e.reason, Rejection::Capacity);
                assert_eq!(e.value.message.data(), b"a");
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(rt.core.bytes, 16);
        assert_eq!(rt.core.messages, 1);
        assert_eq!(rt.core.get(h.0).unwrap().output.len(), 1);
    }
    #[test]
    fn fifo_send_close_and_close_send_have_distinct_outcomes() {
        let mut rt = runtime();
        let h = udp(&mut rt);
        rt.core
            .callback(false, |cx| {
                let id = send(cx, h, 16).unwrap();
                cx.close(h).unwrap();
                assert_eq!(send(cx, h, 16).err().unwrap().reason, Rejection::Closed);
                assert!(
                    cx.effects
                        .iter()
                        .any(|e| matches!(e, Effect::Send { output, .. } if output.id == id))
                );
                Ok::<_, ()>(())
            })
            .unwrap();
        assert!(rt.core.get(h.0).is_none());
        assert_eq!(rt.core.bytes, 0);
        assert!(matches!(
            rt.core.pending.pop_front(),
            Some(OwnedEvent::Write(WriteOutcome {
                status: WriteStatus::Cancelled,
                ..
            }))
        ));
        assert!(matches!(
            rt.core.pending.pop_front(),
            Some(OwnedEvent::Closed(_))
        ));
    }
    #[test]
    fn callback_error_and_panic_cancel_tickets_and_close_pending_sockets() {
        let mut rt = runtime();
        let h = udp(&mut rt);
        let result = rt.core.callback(false, |cx| {
            send(cx, h, 16).unwrap();
            cx.close(h).unwrap();
            Err("callback")
        });
        assert_eq!(result, Err("callback"));
        assert!(rt.core.get(h.0).is_some());
        assert_eq!(rt.core.bytes, 0);
        assert_eq!(rt.core.messages, 1);
        assert!(matches!(
            rt.core.pending.front(),
            Some(OwnedEvent::Write(WriteOutcome {
                status: WriteStatus::Cancelled,
                ..
            }))
        ));
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _: Result<(), ()> = rt.core.callback(false, |cx| {
                send(cx, h, 16).unwrap();
                panic!("callback panic")
            });
        }));
        assert!(result.is_err());
        assert_eq!(rt.core.bytes, 0);
        assert_eq!(rt.core.messages, 2);
    }
    #[test]
    fn slot_reuse_never_reuses_kernel_token_or_checked_key() {
        let mut rt = runtime();
        let first = udp(&mut rt);
        let token = rt.core.get(first.0).unwrap().token.unwrap();
        rt.core.close(first.0, false);
        let next = udp(&mut rt);
        assert_eq!(first.0.slot, next.0.slot);
        assert_ne!(first, next);
        assert!(!rt.core.tokens.contains_key(&token));
        assert_ne!(rt.core.get(next.0).unwrap().token, Some(token));
        rt.core
            .callback(false, |cx| {
                assert_eq!(cx.close(first), Err(Rejection::StaleHandle));
                Ok::<_, ()>(())
            })
            .unwrap();
        let mut other = runtime();
        let foreign = udp(&mut other);
        rt.core
            .callback(false, |cx| {
                assert_eq!(cx.close(foreign), Err(Rejection::ForeignHandle));
                Ok::<_, ()>(())
            })
            .unwrap();
    }
    #[test]
    fn pause_resume_gets_fresh_registration_and_software_readiness() {
        let mut rt = runtime();
        let h = udp(&mut rt);
        let old = rt.core.get(h.0).unwrap().token;
        rt.core
            .callback(false, |cx| {
                cx.set_read_paused(h, true).unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
        rt.core.refresh_interests();
        assert!(rt.core.get(h.0).unwrap().token.is_none());
        rt.core
            .callback(false, |cx| {
                cx.set_read_paused(h, false).unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
        rt.core.refresh_interests();
        let r = rt.core.get(h.0).unwrap();
        assert!(r.read_ready);
        assert_ne!(r.token, old);
    }
    #[test]
    fn exhausted_ids_tokens_and_generations_never_wrap() {
        let mut rt = runtime();
        let h = udp(&mut rt);
        rt.core.next_id = u64::MAX;
        rt.core
            .callback(false, |cx| {
                assert_eq!(send(cx, h, 16).err().unwrap().reason, Rejection::Exhausted);
                Ok::<_, ()>(())
            })
            .unwrap();
        rt.core.next_token = usize::MAX;
        assert!(rt.core.allocate_token().is_err());
        let mut rt = runtime();
        let h = udp(&mut rt);
        rt.core.close(h.0, false);
        rt.core.generations[h.0.slot] = u64::MAX;
        let next = udp(&mut rt);
        assert_ne!(next.0.slot, h.0.slot);
    }
    #[test]
    fn external_shutdown_cancels_unapplied_output_and_closes_registration_admission() {
        let mut rt = runtime();
        let h = udp(&mut rt);
        let control = rt.control_handle();
        rt.core
            .callback(false, |cx| {
                send(cx, h, 16).unwrap();
                control.request_shutdown(ShutdownMode::Immediate).unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(rt.core.queued_outputs(), 0);
        assert_eq!(rt.core.messages, 1);
        assert!(matches!(
            rt.register_udp(
                net::UdpSocket::bind("127.0.0.1:0").unwrap(),
                UdpResourceOptions::default()
            ),
            Err(RegistrationError::Rejected(Rejection::Lifecycle))
        ));
    }
    struct App {
        count: usize,
        fail: bool,
        panic: bool,
    }
    impl Application for App {
        type Command = ();
        type Error = &'static str;
        fn on_event(
            &mut self,
            _: &mut RuntimeContext<'_>,
            _: RuntimeEvent<'_>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
        fn on_command(&mut self, _: &mut RuntimeContext<'_>, _: ()) -> Result<(), Self::Error> {
            self.count += 1;
            assert!(!self.panic, "app panic");
            if self.fail { Err("failed") } else { Ok(()) }
        }
    }
    #[test]
    fn full_mailbox_cannot_block_shutdown_and_stopped_rejections_keep_values() {
        let mut rt = IoRuntime::<()>::new(RuntimeConfig {
            command_capacity: 1,
            ..RuntimeConfig::default()
        })
        .unwrap();
        let control = rt.control_handle();
        let receipt = control.try_submit(()).ok().unwrap().receipt;
        assert!(matches!(control.try_submit(()), Err(SubmitError::Full(()))));
        control.request_shutdown(ShutdownMode::Immediate).unwrap();
        let mut app = App {
            count: 0,
            fail: false,
            panic: false,
        };
        assert_eq!(
            rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
                .unwrap(),
            StepOutcome::Stopped
        );
        assert_eq!(receipt.recv().unwrap(), CommandStatus::Rejected);
        assert_eq!(app.count, 0);
        assert!(matches!(
            control.try_submit(()),
            Err(SubmitError::Stopped(()))
        ));
    }
    #[test]
    fn application_errors_and_unwind_stop_runtime_without_resetting_app() {
        for panic in [false, true] {
            let mut rt = runtime();
            let c = rt.control_handle();
            let _receipt = c.try_submit(()).ok().unwrap();
            let mut app = App {
                count: 0,
                fail: true,
                panic,
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(matches!(
                    result,
                    Ok(Err(RunFailure {
                        cause: FailureCause::Application("failed"),
                        ..
                    }))
                ));
            }
            assert_eq!(app.count, 1);
            assert_eq!(rt.lifecycle(), Lifecycle::Stopped);
        }
    }
    #[test]
    fn shutdown_deadline_shortens_and_immediate_is_irreversible() {
        let rt = runtime();
        let c = rt.control_handle();
        c.request_shutdown(ShutdownMode::Drain {
            timeout: Duration::from_secs(1),
        })
        .unwrap();
        let first = control::lock(&rt.core.admission).shutdown;
        c.request_shutdown(ShutdownMode::Drain {
            timeout: Duration::from_secs(10),
        })
        .unwrap();
        assert_eq!(first, control::lock(&rt.core.admission).shutdown);
        c.request_shutdown(ShutdownMode::Immediate).unwrap();
        c.request_shutdown(ShutdownMode::Drain {
            timeout: Duration::from_secs(10),
        })
        .unwrap();
        assert_eq!(control::lock(&rt.core.admission).shutdown, Some(None));
    }
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    struct FailWake;
    impl super::super::backend::Wake for FailWake {
        fn wake(&self) -> io::Result<()> {
            Err(io::Error::other("scripted wake error"))
        }
    }
    #[derive(Default)]
    struct App {
        commands: usize,
        hooks: usize,
    }
    impl Application for App {
        type Command = ();
        type Error = ();
        fn on_event(&mut self, _: &mut RuntimeContext<'_>, _: RuntimeEvent<'_>) -> Result<(), ()> {
            Ok(())
        }
        fn on_command(&mut self, _: &mut RuntimeContext<'_>, _: ()) -> Result<(), ()> {
            self.commands += 1;
            Ok(())
        }
        fn on_shutdown(&mut self, _: &mut RuntimeContext<'_>) -> Result<(), ()> {
            self.hooks += 1;
            Ok(())
        }
    }
    #[test]
    fn queued_wake_failure_returns_receipt_and_fallback_services_command() {
        let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
        control::lock(&rt.core.admission).waker = Some(Arc::new(FailWake));
        let result = rt.control_handle().try_submit(()).ok().unwrap();
        assert!(result.wake_error.is_some());
        let mut app = App::default();
        rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
            .unwrap();
        assert_eq!(app.commands, 1);
        assert_eq!(result.receipt.recv().unwrap(), CommandStatus::Processed);
    }
    #[test]
    fn interrupted_poll_revisits_turn_and_fatal_poll_stops_and_preserves_source() {
        let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
        let mut app = App::default();
        let mut drivers = DriverSet::default();
        rt.core
            .poll
            .script
            .push_back(Err(io::ErrorKind::Interrupted.into()));
        assert_eq!(
            rt.step(&mut app, &mut drivers, None).unwrap(),
            StepOutcome::Progress
        );
        rt.core
            .poll
            .script
            .push_back(Err(io::Error::from_raw_os_error(5)));
        let failure = rt.step(&mut app, &mut drivers, None).unwrap_err();
        assert!(matches!(failure.cause, FailureCause::Poll(e) if e.raw_os_error() == Some(5)));
        assert_eq!(rt.lifecycle(), Lifecycle::Stopped);
    }
    #[test]
    fn drain_timeout_reports_partial_writes_and_hook_runs_once() {
        let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
        let _peer = net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
        let h = rt
            .register_tcp(socket, TcpResourceOptions::default())
            .unwrap();
        rt.core
            .callback(false, |cx| {
                let mut m = crate::msgb::Msgb::new(16).unwrap();
                m.put(8).unwrap();
                cx.try_send_tcp(h, m).unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
        rt.core
            .get_mut(h.0)
            .unwrap()
            .output
            .front_mut()
            .unwrap()
            .offset = 3;
        rt.control_handle()
            .request_shutdown(ShutdownMode::Drain {
                timeout: Duration::ZERO,
            })
            .unwrap();
        let mut app = App::default();
        assert_eq!(
            rt.step(&mut app, &mut DriverSet::default(), None).unwrap(),
            StepOutcome::Stopped
        );
        assert_eq!(app.hooks, 1);
        let report = rt.take_shutdown_report().unwrap();
        assert!(report.timed_out);
        assert_eq!(report.writes.len(), 1);
        assert_eq!(report.writes[0].bytes_written, 3);
        assert!(matches!(report.writes[0].status, WriteStatus::Cancelled));
        rt.step(&mut app, &mut DriverSet::default(), None).unwrap();
        assert_eq!(app.hooks, 1);
    }
    #[test]
    fn drop_rejects_pending_receipts_and_disables_control_without_callbacks() {
        let rt = IoRuntime::<()>::new(RuntimeConfig::default()).unwrap();
        let control = rt.control_handle();
        let receipt = control.try_submit(()).ok().unwrap().receipt;
        drop(rt);
        assert_eq!(receipt.recv().unwrap(), CommandStatus::Rejected);
        assert!(matches!(
            control.try_submit(()),
            Err(SubmitError::Stopped(()))
        ));
    }
}

#[cfg(test)]
mod boundedness_tests {
    use super::*;
    #[test]
    fn empty_messages_hold_completion_credits_until_notification_delivery() {
        let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
        let _peer = net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let mut rt = IoRuntime::<()>::new(RuntimeConfig {
            total_messages: 1,
            ..RuntimeConfig::default()
        })
        .unwrap();
        let h = rt
            .register_tcp(socket, TcpResourceOptions::default())
            .unwrap();
        rt.core
            .callback(false, |cx| {
                cx.try_send_tcp(h, crate::msgb::Msgb::new(0).unwrap())
                    .unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
        rt.core.write(h.0);
        assert_eq!(rt.core.bytes, 0);
        assert_eq!(rt.core.messages, 1);
        assert_eq!(rt.core.queued_outputs(), 0);
        rt.core
            .callback(false, |cx| {
                assert_eq!(
                    cx.try_send_tcp(h, crate::msgb::Msgb::new(0).unwrap())
                        .unwrap_err()
                        .reason,
                    Rejection::Capacity
                );
                Ok::<_, ()>(())
            })
            .unwrap();
        let Some(OwnedEvent::Write(w)) = rt.core.pending.pop_front() else {
            panic!("missing outcome")
        };
        rt.core.release_message(w.resource.key());
        rt.core
            .callback(false, |cx| {
                cx.try_send_tcp(h, crate::msgb::Msgb::new(0).unwrap())
                    .unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
    }
    #[test]
    fn deferred_registration_has_no_live_handle_until_applied_and_errors_close_socket() {
        let mut rt = IoRuntime::<()>::new(RuntimeConfig::default()).unwrap();
        let socket = net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        let mut ticket = None;
        let result = rt.core.callback(false, |cx| {
            ticket = Some(
                cx.register_udp(socket, UdpResourceOptions::default())
                    .unwrap(),
            );
            assert!(cx.view.is_empty());
            Err(())
        });
        assert!(result.is_err());
        assert!(rt.core.resources.is_empty());
        let _replacement = net::UdpSocket::bind(address).unwrap();
        assert!(
            matches!(rt.core.pending.pop_front(), Some(OwnedEvent::Registered(id, Err(RegistrationError::Rejected(Rejection::Cancelled)))) if Some(id) == ticket)
        );
    }
    #[test]
    fn resource_cap_suspends_listener_and_closure_reschedules_accept() {
        struct App {
            accepted: usize,
        }
        impl Application for App {
            type Command = ();
            type Error = ();
            fn on_command(&mut self, _: &mut RuntimeContext<'_>, _: ()) -> Result<(), ()> {
                Ok(())
            }
            fn on_event(
                &mut self,
                _: &mut RuntimeContext<'_>,
                e: RuntimeEvent<'_>,
            ) -> Result<(), ()> {
                if matches!(e, RuntimeEvent::Accepted { .. }) {
                    self.accepted += 1;
                }
                Ok(())
            }
        }
        let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut rt = IoRuntime::new(RuntimeConfig {
            max_resources: 2,
            ..RuntimeConfig::default()
        })
        .unwrap();
        let h = rt
            .register_listener(listener, ListenerOptions::default())
            .unwrap();
        let _first = net::TcpStream::connect(address).unwrap();
        let _second = net::TcpStream::connect(address).unwrap();
        let mut app = App { accepted: 0 };
        let mut drivers = DriverSet::default();
        rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
            .unwrap();
        rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
            .unwrap();
        assert_eq!(app.accepted, 1);
        assert!(rt.core.get(h.0).unwrap().token.is_none());
        assert!(!rt.core.get(h.0).unwrap().read_ready);
        let key = rt
            .core
            .resources
            .iter()
            .flatten()
            .find(|r| matches!(r.socket, Socket::Tcp(..)))
            .unwrap()
            .key;
        rt.core.close(key, false);
        assert!(rt.core.get(h.0).unwrap().read_ready);
        rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
            .unwrap();
        assert_eq!(app.accepted, 2);
    }
    #[test]
    fn callback_removes_self_and_other_without_retaining_resource_borrows() {
        struct App {
            other: TcpHandle,
            calls: usize,
        }
        impl Application for App {
            type Command = ();
            type Error = ();
            fn on_command(&mut self, _: &mut RuntimeContext<'_>, _: ()) -> Result<(), ()> {
                Ok(())
            }
            fn on_event(
                &mut self,
                cx: &mut RuntimeContext<'_>,
                e: RuntimeEvent<'_>,
            ) -> Result<(), ()> {
                if let RuntimeEvent::TcpBytes { stream, .. } = e {
                    self.calls += 1;
                    cx.close(stream).unwrap();
                    cx.close(self.other).unwrap();
                }
                Ok(())
            }
        }
        let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
        let mut peers = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..2 {
            let mut p = net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (s, _) = listener.accept().unwrap();
            handles.push(rt.register_tcp(s, TcpResourceOptions::default()).unwrap());
            p.write_all(b"x").unwrap();
            peers.push(p);
        }
        let mut app = App {
            other: handles[1],
            calls: 0,
        };
        rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
            .unwrap();
        assert_eq!(app.calls, 1);
        assert_eq!(rt.core.resources.iter().flatten().count(), 0);
    }
}

#[cfg(test)]
mod notification_tests {
    use super::*;
    struct App {
        events: usize,
        socket: Option<UdpHandle>,
        final_writes: usize,
    }
    impl Application for App {
        type Command = ();
        type Error = ();
        fn on_event(&mut self, _: &mut RuntimeContext<'_>, _: RuntimeEvent<'_>) -> Result<(), ()> {
            self.events += 1;
            Ok(())
        }
        fn on_command(&mut self, _: &mut RuntimeContext<'_>, _: ()) -> Result<(), ()> {
            Ok(())
        }
        fn on_shutdown(&mut self, cx: &mut RuntimeContext<'_>) -> Result<(), ()> {
            if let Some(handle) = self.socket {
                cx.try_send_udp(
                    handle,
                    Datagram {
                        message: crate::msgb::Msgb::new(0).unwrap(),
                        destination: Some("127.0.0.1:9".parse().unwrap()),
                    },
                )
                .unwrap();
                self.final_writes += 1;
            }
            Ok(())
        }
    }
    #[test]
    fn notification_budget_is_shared_across_entire_step() {
        let mut rt = IoRuntime::new(RuntimeConfig {
            notifications_per_step: 1,
            ..RuntimeConfig::default()
        })
        .unwrap();
        for _ in 0..5 {
            rt.core.pending.push_back(OwnedEvent::Driver("test", false));
        }
        let mut app = App {
            events: 0,
            socket: None,
            final_writes: 0,
        };
        rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
            .unwrap();
        assert_eq!(app.events, 1);
        assert_eq!(rt.core.pending.len(), 4);
    }
    #[test]
    fn shutdown_hook_has_effect_allowance_even_when_normal_notifications_are_full() {
        let mut rt = IoRuntime::new(RuntimeConfig {
            notification_capacity: 4,
            ..RuntimeConfig::default()
        })
        .unwrap();
        let h = rt
            .register_udp(
                net::UdpSocket::bind("127.0.0.1:0").unwrap(),
                UdpResourceOptions::default(),
            )
            .unwrap();
        for _ in 0..4 {
            rt.core.pending.push_back(OwnedEvent::Driver("test", false));
        }
        rt.control_handle()
            .request_shutdown(ShutdownMode::Drain {
                timeout: Duration::from_secs(1),
            })
            .unwrap();
        let mut app = App {
            events: 0,
            socket: Some(h),
            final_writes: 0,
        };
        for _ in 0..3 {
            rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
                .unwrap();
            if rt.lifecycle() == Lifecycle::Stopped {
                break;
            }
        }
        assert_eq!(app.final_writes, 1);
        assert_eq!(rt.lifecycle(), Lifecycle::Stopped);
        let report = rt.take_shutdown_report().unwrap();
        assert!(!report.timed_out);
        assert_eq!(report.writes.len(), 1);
        assert!(matches!(report.writes[0].status, WriteStatus::Complete));
    }
}
