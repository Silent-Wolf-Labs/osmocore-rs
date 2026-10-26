// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use mio::{Interest, Token, event::Source};
use std::{collections::VecDeque, io};
pub(crate) enum Socket {
    Tcp(mio::net::TcpStream, TcpResourceOptions),
    Udp(mio::net::UdpSocket, Option<std::net::SocketAddr>),
    Listener(mio::net::TcpListener, ListenerOptions),
}
pub(crate) struct Output {
    pub message: crate::msgb::Msgb,
    pub destination: Option<std::net::SocketAddr>,
    pub offset: usize,
    pub id: WriteId,
    pub context: super::TraceContext,
}
pub(crate) struct Resource {
    pub key: Key,
    pub socket: Socket,
    pub token: Option<Token>,
    pub interest: Option<Interest>,
    pub paused: bool,
    pub eof: bool,
    pub close_after_flush: bool,
    pub read_ready: bool,
    pub write_ready: bool,
    pub output: VecDeque<Output>,
}
impl Resource {
    pub fn handle(&self) -> ResourceHandle {
        match self.socket {
            Socket::Tcp(..) => ResourceHandle::Tcp(TcpHandle(self.key)),
            Socket::Udp(..) => ResourceHandle::Udp(UdpHandle(self.key)),
            Socket::Listener(..) => ResourceHandle::Listener(ListenerHandle(self.key)),
        }
    }
    pub fn source(&mut self) -> &mut dyn Source {
        match &mut self.socket {
            Socket::Tcp(s, _) => s,
            Socket::Udp(s, _) => s,
            Socket::Listener(s, _) => s,
        }
    }
    pub fn desired_interest(&self, running: bool) -> Option<Interest> {
        let read = running && !self.paused && !self.eof;
        let write = !self.output.is_empty();
        match (read, write) {
            (true, true) => Some(Interest::READABLE | Interest::WRITABLE),
            (true, false) => Some(Interest::READABLE),
            (false, true) => Some(Interest::WRITABLE),
            _ => None,
        }
    }
}
pub(crate) fn registration_io(phase: &'static str, source: io::Error) -> RegistrationError {
    RegistrationError::Io { phase, source }
}
