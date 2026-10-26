// SPDX-License-Identifier: GPL-3.0-or-later

/// Map a typed FSM error to its recorded C integer status.
pub fn status(error: &crate::fsm::FsmError) -> i32 {
    error.compatibility_status()
}
pub use crate::fsm::legacy::{
    osmo_fsm_inst_find_by_name, osmo_fsm_inst_name, osmo_fsm_inst_state_name,
    osmo_fsm_inst_update_id_f, osmo_fsm_inst_update_id_f_sanitize, osmo_fsm_term_cause_name,
};
