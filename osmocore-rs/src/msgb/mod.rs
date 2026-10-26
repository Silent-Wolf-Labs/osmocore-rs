// SPDX-License-Identifier: GPL-3.0-or-later

//! Fixed-capacity owned message buffers, layer markers and owned queues.
pub(crate) mod buffer;
pub(crate) mod format;
pub(crate) mod queue;
use buffer::*;
pub use buffer::{Msgb, MsgbError};
pub use queue::MsgbQueue;
