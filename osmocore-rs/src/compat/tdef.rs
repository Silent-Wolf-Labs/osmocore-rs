// SPDX-License-Identifier: GPL-3.0-or-later

/// Return a C status only for errors corresponding to defined C returns.
pub fn status(error: &crate::tdef::TDefError) -> Option<i32> {
    error.c_status()
}
/// Recorded C metadata.
pub const OSMO_TDEF_UNIT_NAMES: [(crate::tdef::TDefUnit, Option<&str>); 6] =
    crate::tdef::OSMO_TDEF_UNIT_NAMES;

pub use crate::tdef::legacy::{
    osmo_tdef_for_each, osmo_tdef_for_each_mut, osmo_tdef_get, osmo_tdef_get_entry,
    osmo_tdef_get_entry_mut, osmo_tdef_get_state_timeout, osmo_tdef_groups_for_each,
    osmo_tdef_range_str_buf, osmo_tdef_set, osmo_tdef_set_status, osmo_tdef_unit_name,
    osmo_tdef_val_in_range, osmo_tdefs_reset,
};
