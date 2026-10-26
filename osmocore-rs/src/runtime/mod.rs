// SPDX-License-Identifier: GPL-3.0-or-later
#![doc = include_str!("README.md")]
mod backend;
mod control;
mod core;
mod driver;
mod effects;
mod output;
mod resource;
mod types;
pub use control::*;
pub use core::IoRuntime;
pub use driver::*;
pub use effects::RuntimeContext;
pub use types::*;

#[derive(Clone, Debug)]
pub(crate) struct TraceContext {
    #[cfg(feature = "tracing")]
    span: tracing::Span,
}
impl TraceContext {
    fn capture() -> Self {
        Self {
            #[cfg(feature = "tracing")]
            span: tracing::Span::current(),
        }
    }
    fn scope<T>(&self, f: impl FnOnce() -> T) -> T {
        #[cfg(feature = "tracing")]
        let _guard = self.span.enter();
        f()
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::capture()
    }
}
