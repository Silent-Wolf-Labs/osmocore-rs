//! Fsm API comparisons against committed C observations.

#[path = "../support/fsm_fixture.rs"]
mod fsm_fixture;

mod fsm_core_reference_vectors;
mod fsm_registry_reference_vectors;
mod fsm_shared_reference_vectors;
mod fsm_timer_reference_vectors;
mod fsm_tree_reference_vectors;

#[cfg(feature = "tracing")]
mod diagnostics;
