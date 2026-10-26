// SPDX-License-Identifier: GPL-3.0-or-later

//! Owned FSM execution against the local C reference.
//!
//! Callbacks receive handles, never mutable instance references. They may make
//! synchronous nested calls; lookups remain visible while termination callbacks
//! run. Registration belongs to this runtime, not a process-global registry.
use crate::time::Clock;
use crate::timer::{TimerError, TimerId, TimerScheduler};
use std::{any::Any, borrow::Cow, fmt, rc::Rc};

mod descriptor;
mod hierarchy;
mod runtime;
pub use descriptor::{
    Action, EventData, Fsm, FsmDefinitionId, FsmError, FsmId, FsmInstance, FsmState, StateCallback,
    TermCallback, TermCause, TimeoutCallback,
};
pub use runtime::FsmRuntime;
pub(crate) mod legacy {
    use super::*;
    pub fn osmo_fsm_term_cause_name(cause: TermCause) -> &'static str {
        match cause {
            TermCause::Parent => "OSMO_FSM_TERM_PARENT",
            TermCause::Request => "OSMO_FSM_TERM_REQUEST",
            TermCause::Regular => "OSMO_FSM_TERM_REGULAR",
            TermCause::Error => "OSMO_FSM_TERM_ERROR",
            TermCause::Timeout => "OSMO_FSM_TERM_TIMEOUT",
        }
    }

    pub fn osmo_fsm_inst_find_by_name<C, D, E>(
        r: &FsmRuntime<C, D, E>,
        definition: &FsmDefinitionId,
        name: Option<&str>,
    ) -> Result<Option<FsmId>, FsmError> {
        match name {
            Some(n) => r.find_by_name(definition, n),
            None => {
                r.definition_index(definition)?;
                Ok(None)
            }
        }
    }

    pub fn osmo_fsm_inst_update_id_f<C, D, E>(
        r: &mut FsmRuntime<C, D, E>,
        id: &FsmId,
        args: Option<fmt::Arguments<'_>>,
    ) -> Result<(), FsmError> {
        let text = args.map(|a| a.to_string());
        r.update_id(id, text.as_deref())
    }
    pub fn osmo_fsm_inst_update_id_f_sanitize<C, D, E>(
        r: &mut FsmRuntime<C, D, E>,
        id: &FsmId,
        replacement: u8,
        args: Option<fmt::Arguments<'_>>,
    ) -> Result<(), FsmError> {
        let text = args.map(|a| a.to_string());
        r.update_id_sanitize(id, replacement, text.as_deref())
    }

    pub fn osmo_fsm_inst_name<'a, C, D, E>(
        r: &'a FsmRuntime<C, D, E>,
        id: Option<&FsmId>,
    ) -> Result<&'a str, FsmError> {
        match id {
            Some(id) => Ok(&r.instance(id)?.name),
            None => Ok("NULL"),
        }
    }
    pub fn osmo_fsm_inst_state_name<'a, C, D, E>(
        r: &'a FsmRuntime<C, D, E>,
        id: Option<&FsmId>,
    ) -> Result<Option<Cow<'a, str>>, FsmError> {
        match id {
            Some(id) => {
                let i = r.instance(id)?;
                r.state_name(&i.definition, i.state)
            }
            None => Ok(Some(Cow::Borrowed("NULL"))),
        }
    }
}
