// SPDX-License-Identifier: GPL-3.0-or-later
use super::backend::Wake;
use super::{Lifecycle, ShutdownMode};
use std::{
    io,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandStatus {
    Processed,
    Rejected,
    Failed,
}
/// Completion of command processing after mailbox admission.
/// This reports callback processing, not transport completion or peer delivery.
pub struct CommandReceipt {
    pub(crate) receiver: mpsc::Receiver<CommandStatus>,
}
impl CommandReceipt {
    /// Poll completion without blocking the caller.
    pub fn try_recv(&self) -> Result<CommandStatus, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
    /// Block for completion. Do not call on the thread driving this runtime.
    pub fn recv(self) -> Result<CommandStatus, mpsc::RecvError> {
        self.receiver.recv()
    }
}
/// Successful mailbox admission; a wake error does not undo ownership transfer.
pub struct SubmitOutcome {
    pub receipt: CommandReceipt,
    pub wake_error: Option<io::Error>,
}
pub enum SubmitError<C> {
    Full(C),
    Stopped(C),
}
#[derive(Debug)]
pub enum ControlError {
    Stopped,
    DeadlineOverflow,
}
#[derive(Debug)]
pub struct ShutdownRequestOutcome {
    pub wake_error: Option<io::Error>,
}
pub(crate) struct Envelope<C> {
    pub command: C,
    pub context: super::TraceContext,
    pub receipt: mpsc::SyncSender<CommandStatus>,
}
pub(crate) struct Admission {
    pub state: Lifecycle,
    pub shutdown: Option<Option<Instant>>,
    pub waker: Option<Arc<dyn Wake>>,
}
/// Cloneable cross-thread control for a single runtime; rejects admission after shutdown.
pub struct ControlHandle<C: Send + 'static> {
    pub(crate) sender: mpsc::SyncSender<Envelope<C>>,
    pub(crate) admission: Arc<Mutex<Admission>>,
}
impl<C: Send + 'static> Clone for ControlHandle<C> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            admission: self.admission.clone(),
        }
    }
}
pub(crate) fn lock(admission: &Mutex<Admission>) -> std::sync::MutexGuard<'_, Admission> {
    admission.lock().unwrap_or_else(|e| e.into_inner())
}
impl<C: Send + 'static> ControlHandle<C> {
    /// Enqueue then wake. A wake failure still transfers ownership and returns a
    /// receipt. Runtime's finite poll interval supplies fallback progress.
    pub fn try_submit(&self, command: C) -> Result<SubmitOutcome, SubmitError<C>> {
        let admission = lock(&self.admission);
        if admission.state != Lifecycle::Running || admission.shutdown.is_some() {
            return Err(SubmitError::Stopped(command));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        match self.sender.try_send(Envelope {
            context: super::TraceContext::capture(),
            command,
            receipt: sender,
        }) {
            Ok(()) => Ok(SubmitOutcome {
                receipt: CommandReceipt { receiver },
                wake_error: admission.waker.as_ref().and_then(|w| w.wake().err()),
            }),
            Err(mpsc::TrySendError::Full(e)) => Err(SubmitError::Full(e.command)),
            Err(mpsc::TrySendError::Disconnected(e)) => Err(SubmitError::Stopped(e.command)),
        }
    }
    pub fn request_shutdown(
        &self,
        mode: ShutdownMode,
    ) -> Result<ShutdownRequestOutcome, ControlError> {
        let mut admission = lock(&self.admission);
        if admission.state == Lifecycle::Stopped {
            return Err(ControlError::Stopped);
        }
        let deadline = match mode {
            ShutdownMode::Immediate => None,
            ShutdownMode::Drain { timeout } => Some(
                Instant::now()
                    .checked_add(timeout)
                    .ok_or(ControlError::DeadlineOverflow)?,
            ),
        };
        admission.shutdown = Some(match admission.shutdown {
            Some(None) => None,
            Some(Some(old)) => deadline.map(|d| old.min(d)),
            None => deadline,
        });
        Ok(ShutdownRequestOutcome {
            wake_error: admission.waker.as_ref().and_then(|w| w.wake().err()),
        })
    }
}

pub(crate) struct ReceiptGuard(Option<mpsc::SyncSender<CommandStatus>>);
impl ReceiptGuard {
    pub fn new(sender: mpsc::SyncSender<CommandStatus>) -> Self {
        Self(Some(sender))
    }
    pub fn resolve(mut self, status: CommandStatus) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(status);
        }
    }
}
impl Drop for ReceiptGuard {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(CommandStatus::Failed);
        }
    }
}
