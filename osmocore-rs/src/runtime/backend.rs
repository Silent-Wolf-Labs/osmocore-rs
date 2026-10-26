// SPDX-License-Identifier: GPL-3.0-or-later
//! Private platform boundary; sockets remain Mio sources on every I/O path.
use mio::{Events, Poll, Registry, Waker};
use std::{io, time::Duration};
pub(crate) trait Wake: Send + Sync {
    fn wake(&self) -> io::Result<()>;
}
impl Wake for Waker {
    fn wake(&self) -> io::Result<()> {
        Waker::wake(self)
    }
}
pub(crate) struct Backend {
    poll: Poll,
    #[cfg(test)]
    pub script: std::collections::VecDeque<io::Result<()>>,
}
impl Backend {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            poll: Poll::new()?,
            #[cfg(test)]
            script: Default::default(),
        })
    }
    pub fn registry(&self) -> &Registry {
        self.poll.registry()
    }
    pub fn poll(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        #[cfg(test)]
        if let Some(result) = self.script.pop_front() {
            events.clear();
            return result;
        }
        self.poll.poll(events, timeout)
    }
}
