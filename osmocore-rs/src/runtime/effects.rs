// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{collections::HashMap, net};
#[derive(Clone)]
pub(crate) struct View {
    pub handle: ResourceHandle,
    pub bytes: usize,
    pub messages: usize,
    pub closed: bool,
    pub connected: bool,
}
// Effects have a hard count limit; keep owned buffers inline without extra allocations.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Effect {
    Send {
        resource: ResourceHandle,
        output: super::resource::Output,
    },
    Close(ResourceHandle),
    Pause(ResourceHandle, bool),
    Tcp(RegistrationId, net::TcpStream, TcpResourceOptions),
    Udp(RegistrationId, net::UdpSocket, UdpResourceOptions),
    Listener(RegistrationId, net::TcpListener, ListenerOptions),
    Shutdown(Option<std::time::Instant>),
}
/// A bounded logical snapshot and effect outbox. No socket I/O or live resource
/// mutations occur here. Successful sends reserve tickets and quota; callbacks
/// must return successfully before effects can be committed in FIFO order.
pub struct RuntimeContext<'a> {
    pub(crate) config: &'a RuntimeConfig,
    pub(crate) owner: u64,
    pub(crate) view: HashMap<Key, View>,
    pub(crate) effects: Vec<Effect>,
    pub(crate) bytes: usize,
    pub(crate) messages: usize,
    pub(crate) next_id: &'a mut u64,
    pub(crate) accepting: bool,
    pub(crate) registering: bool,
}
// Returning the original owned buffer avoids an allocation on rejection.
#[allow(clippy::result_large_err)]
impl RuntimeContext<'_> {
    fn capacity(&self) -> Result<(), Rejection> {
        if self.effects.len() >= self.config.effects_per_callback {
            Err(Rejection::Capacity)
        } else {
            Ok(())
        }
    }
    fn validate(&self, handle: ResourceHandle) -> Result<&View, Rejection> {
        if handle.key().owner != self.owner {
            return Err(Rejection::ForeignHandle);
        }
        let view = self.view.get(&handle.key()).ok_or(Rejection::StaleHandle)?;
        if view.handle != handle {
            return Err(Rejection::WrongKind);
        }
        if view.closed {
            return Err(Rejection::Closed);
        }
        Ok(view)
    }
    fn id(&mut self) -> Result<u64, Rejection> {
        let id = *self.next_id;
        *self.next_id = id.checked_add(1).ok_or(Rejection::Exhausted)?;
        Ok(id)
    }
    fn send(
        &mut self,
        resource: ResourceHandle,
        message: crate::msgb::Msgb,
        destination: Option<net::SocketAddr>,
    ) -> Result<WriteId, EnqueueError<crate::msgb::Msgb>> {
        let result = (|| {
            if !self.accepting {
                return Err(Rejection::Lifecycle);
            }
            self.capacity()?;
            let view = self.validate(resource)?;
            if matches!(resource, ResourceHandle::Udp(_)) && view.connected == destination.is_some()
            {
                return Err(Rejection::InvalidDestination);
            }
            let bytes = usize::from(message.capacity());
            if bytes > self.config.per_resource_bytes.saturating_sub(view.bytes)
                || bytes > self.config.total_bytes.saturating_sub(self.bytes)
                || view.messages >= self.config.per_resource_messages
                || self.messages >= self.config.total_messages
            {
                return Err(Rejection::Capacity);
            }
            self.id().map(WriteId)
        })();
        match result {
            Err(reason) => Err(EnqueueError {
                reason,
                value: message,
            }),
            Ok(id) => {
                let bytes = usize::from(message.capacity());
                let view = self.view.get_mut(&resource.key()).expect("validated view");
                view.bytes += bytes;
                view.messages += 1;
                self.bytes += bytes;
                self.messages += 1;
                self.effects.push(Effect::Send {
                    resource,
                    output: super::resource::Output {
                        message,
                        destination,
                        offset: 0,
                        id,
                        context: super::TraceContext::capture(),
                    },
                });
                Ok(id)
            }
        }
    }
    pub fn try_send_tcp(
        &mut self,
        handle: TcpHandle,
        message: crate::msgb::Msgb,
    ) -> Result<WriteId, EnqueueError<crate::msgb::Msgb>> {
        self.send(handle.into(), message, None)
    }
    pub fn try_send_udp(
        &mut self,
        handle: UdpHandle,
        datagram: Datagram,
    ) -> Result<WriteId, EnqueueError<Datagram>> {
        let Datagram {
            message,
            destination,
        } = datagram;
        self.send(handle.into(), message, destination)
            .map_err(|e| EnqueueError {
                reason: e.reason,
                value: Datagram {
                    message: e.value,
                    destination,
                },
            })
    }
    pub fn close(&mut self, handle: impl Into<ResourceHandle>) -> Result<(), Rejection> {
        let handle = handle.into();
        self.capacity()?;
        self.validate(handle)?;
        self.view
            .get_mut(&handle.key())
            .expect("validated view")
            .closed = true;
        self.effects.push(Effect::Close(handle));
        Ok(())
    }
    pub fn set_read_paused(
        &mut self,
        handle: impl Into<ResourceHandle>,
        paused: bool,
    ) -> Result<(), Rejection> {
        let handle = handle.into();
        self.capacity()?;
        self.validate(handle)?;
        if !self.registering {
            return Err(Rejection::Lifecycle);
        }
        self.effects.push(Effect::Pause(handle, paused));
        Ok(())
    }
    pub fn request_shutdown(&mut self, mode: ShutdownMode) -> Result<(), Rejection> {
        self.capacity()?;
        let deadline = match mode {
            ShutdownMode::Immediate => None,
            ShutdownMode::Drain { timeout } => Some(
                std::time::Instant::now()
                    .checked_add(timeout)
                    .ok_or(Rejection::Exhausted)?,
            ),
        };
        self.effects.push(Effect::Shutdown(deadline));
        self.accepting = false;
        self.registering = false;
        Ok(())
    }
    fn registration(&mut self) -> Result<RegistrationId, Rejection> {
        if !self.registering {
            return Err(Rejection::Lifecycle);
        }
        self.capacity()?;
        let pending = self
            .effects
            .iter()
            .filter(|e| matches!(e, Effect::Tcp(..) | Effect::Udp(..) | Effect::Listener(..)))
            .count();
        if self.view.len().saturating_add(pending) >= self.config.max_resources {
            return Err(Rejection::Capacity);
        }
        self.id().map(RegistrationId)
    }
    pub fn register_tcp(
        &mut self,
        socket: net::TcpStream,
        options: TcpResourceOptions,
    ) -> Result<RegistrationId, EnqueueError<net::TcpStream>> {
        match self.registration() {
            Ok(id) => {
                self.effects.push(Effect::Tcp(id, socket, options));
                Ok(id)
            }
            Err(reason) => Err(EnqueueError {
                reason,
                value: socket,
            }),
        }
    }
    pub fn register_udp(
        &mut self,
        socket: net::UdpSocket,
        options: UdpResourceOptions,
    ) -> Result<RegistrationId, EnqueueError<net::UdpSocket>> {
        match self.registration() {
            Ok(id) => {
                self.effects.push(Effect::Udp(id, socket, options));
                Ok(id)
            }
            Err(reason) => Err(EnqueueError {
                reason,
                value: socket,
            }),
        }
    }
    pub fn register_listener(
        &mut self,
        socket: net::TcpListener,
        options: ListenerOptions,
    ) -> Result<RegistrationId, EnqueueError<net::TcpListener>> {
        match self.registration() {
            Ok(id) => {
                self.effects.push(Effect::Listener(id, socket, options));
                Ok(id)
            }
            Err(reason) => Err(EnqueueError {
                reason,
                value: socket,
            }),
        }
    }
}
