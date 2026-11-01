// SPDX-License-Identifier: GPL-3.0-or-later

//! Process-wide identity for the schedulers and runtimes that issue handles.
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Never reused, so a handle can't match a later owner after its own is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnerId(u64);

impl OwnerId {
    /// Panics if the 64-bit counter runs out, rather than reusing an identity.
    pub(crate) fn next() -> Self {
        let id = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("owner identities exhausted");
        Self(id)
    }
}
