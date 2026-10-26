// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("README.md")]

macro_rules! event {
    ($level:ident, $target:literal, $($fields:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::$level!(target: $target, $($fields)*);
    };
}
pub(crate) use event;

// Keep the context owned: callbacks may remove or rename the observed instance.
// Disabled instrumentation evaluates neither identifiers nor diagnostic fields.
macro_rules! observe {
    ($runtime:expr, $id:expr, $operation:literal, $context:ident, $body:block) => {{
        #[cfg(feature = "tracing")]
        let $context = $runtime.diagnostic_context($id);
        #[cfg(feature = "tracing")]
        let _diagnostic_span = $context.span($operation);
        #[cfg(feature = "tracing")]
        let _diagnostic_guard = _diagnostic_span.enter();
        let result = (|| $body)();
        #[cfg(feature = "tracing")]
        if let Err(error) = &result {
            $context.reject($operation, error);
        }
        result
    }};
}
pub(crate) use observe;

#[cfg(feature = "tracing")]
mod enabled {
    use crate::{fsm::FsmError, timer::TimerError};

    pub(crate) fn timer_category(error: &TimerError) -> &'static str {
        match error {
            TimerError::Time(_) => "time",
            TimerError::InvalidHandle => "invalid_handle",
            TimerError::NestedUpdate => "nested_update",
            TimerError::GenerationExhausted => "generation_exhausted",
            TimerError::MillisecondsOverflow => "milliseconds_overflow",
        }
    }

    fn fsm_category(error: &FsmError) -> &'static str {
        match error {
            FsmError::InvalidHandle => "invalid_handle",
            FsmError::InvalidDefinition => "invalid_definition",
            FsmError::InvalidDescriptor => "invalid_descriptor",
            FsmError::InvalidIdentifier => "invalid_identifier",
            FsmError::DuplicateName => "duplicate_name",
            FsmError::NotRegistered => "not_registered",
            FsmError::InvalidState(_) => "invalid_state",
            FsmError::InvalidEvent(_) => "invalid_event",
            FsmError::Terminating => "terminating",
            FsmError::TransitionNotPermitted => "transition_not_permitted",
            FsmError::EventNotPermitted => "event_not_permitted",
            FsmError::HierarchyRequiresTermination => "hierarchy_requires_termination",
            FsmError::ParentCycle => "parent_cycle",
            FsmError::StuckChild => "stuck_child",
            FsmError::Timer(error) => timer_category(error),
            FsmError::TDef(_) => "tdef",
            FsmError::TimeoutOverflow => "timeout_overflow",
            FsmError::GenerationExhausted => "generation_exhausted",
        }
    }

    pub(crate) struct FsmContext {
        pub(crate) fsm_name: Option<String>,
        pub(crate) instance_slot: Option<usize>,
        pub(crate) instance_generation: Option<u64>,
        pub(crate) current_state: Option<u32>,
    }
    impl FsmContext {
        pub(crate) fn span(&self, operation: &'static str) -> tracing::Span {
            tracing::trace_span!(target: "osmocore_rs::fsm", "fsm_execution",
                operation, fsm_name = self.fsm_name.as_deref(),
                instance_slot = self.instance_slot,
                instance_generation = self.instance_generation,
                current_state = self.current_state)
        }
        pub(crate) fn reject(&self, operation: &'static str, error: &FsmError) {
            tracing::warn!(target: "osmocore_rs::fsm", operation,
                fsm_name = self.fsm_name.as_deref(), instance_slot = self.instance_slot,
                instance_generation = self.instance_generation, current_state = self.current_state,
                error_category = fsm_category(error));
        }
        pub(crate) fn emit(&self, operation: &'static str) {
            tracing::debug!(target: "osmocore_rs::fsm", operation,
                fsm_name = self.fsm_name.as_deref(), instance_slot = self.instance_slot,
                instance_generation = self.instance_generation, current_state = self.current_state);
        }
    }

    pub(crate) struct TimerContext {
        // Only validated handles have local identity. A foreign slot could alias
        // a live local timer, so do not label it as a local timer in diagnostics.
        pub(crate) timer_slot: Option<usize>,
        pub(crate) timer_generation: Option<u64>,
    }
    impl TimerContext {
        pub(crate) fn span(&self, operation: &'static str) -> tracing::Span {
            tracing::trace_span!(target: "osmocore_rs::timer", "timer_operation", operation,
                timer_slot = self.timer_slot, timer_generation = self.timer_generation)
        }
        pub(crate) fn reject(&self, operation: &'static str, error: &TimerError) {
            tracing::warn!(target: "osmocore_rs::timer", operation,
                timer_slot = self.timer_slot, timer_generation = self.timer_generation,
                error_category = timer_category(error));
        }
    }
}
#[cfg(feature = "tracing")]
pub(crate) use enabled::{FsmContext, TimerContext};
