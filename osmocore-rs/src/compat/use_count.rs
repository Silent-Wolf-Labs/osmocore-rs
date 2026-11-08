// SPDX-License-Identifier: GPL-3.0-or-later

use crate::base::use_count::{UseChange, UseCount};

const EINVAL_RESULT: i32 = -22;
const ERANGE_RESULT: i32 = -34;

/// C's `osmo_use_count_cb_t`, without its source file and line. It runs after a change
/// and can adjust or remove entries through the counters it receives, and its result
/// becomes the call's. A change it reports as an error isn't undone.
pub type UseCallback<'a> = dyn FnMut(&mut UseCount, UseChange) -> i32 + 'a;

/// Adds `change` to a token's count and runs `use_cb` if the count changed. Returns -22
/// with no counters, -34 if the count would leave the range of `i32`, otherwise the
/// callback's result or 0.
pub fn osmo_use_count_get_put(
    uc: Option<&mut UseCount>,
    use_token: Option<&str>,
    change: i32,
    use_cb: Option<&mut UseCallback<'_>>,
) -> i32 {
    let Some(uc) = uc else {
        return EINVAL_RESULT;
    };
    if change == 0 {
        return 0;
    }
    match (uc.change(use_token, change), use_cb) {
        (Err(_), _) => ERANGE_RESULT,
        (Ok(changed), Some(use_cb)) => use_cb(uc, changed),
        (Ok(_), None) => 0,
    }
}

/// Returns the sum of all counts, stopping at the limits of `i32`, or 0 with no counters.
pub fn osmo_use_count_total(uc: Option<&UseCount>) -> i32 {
    uc.map_or(0, UseCount::total)
}

/// Returns a token's count, or 0 if there is no entry or no counters.
pub fn osmo_use_count_by(uc: Option<&UseCount>, use_token: Option<&str>) -> i32 {
    uc.map_or(0, |uc| uc.count_of(use_token))
}

/// Writes the listing to `buf`, whose length is C's `buf_len`, cutting it short and adding
/// a NUL if `buf` isn't empty. Returns the listing's full length, like `snprintf`.
pub fn osmo_use_count_to_str_buf(buf: &mut [u8], uc: &UseCount) -> i32 {
    let text = uc.to_string();
    if let Some(room) = buf.len().checked_sub(1) {
        let copied = text.len().min(room);
        buf[..copied].copy_from_slice(&text.as_bytes()[..copied]);
        buf[copied] = 0;
    }
    text.len() as i32
}

/// Writes the listing like [`osmo_use_count_to_str_buf`] and returns `buf`.
pub fn osmo_use_count_name_buf<'a>(buf: &'a mut [u8], uc: &UseCount) -> &'a [u8] {
    osmo_use_count_to_str_buf(buf, uc);
    buf
}

/// Returns the listing as a new string.
pub fn osmo_use_count_to_str_c(uc: &UseCount) -> String {
    uc.to_string()
}

/// Adds idle entries with no token. Unlike C's static entries, these can be removed.
pub fn osmo_use_count_make_static_entries(uc: &mut UseCount, buf_n_entries: usize) {
    uc.add_idle_entries(buf_n_entries);
}

/// Removes a token's entry, as C's `osmo_use_count_find` and `osmo_use_count_free` do
/// together. Does nothing if there is no entry.
pub fn osmo_use_count_free(uc: &mut UseCount, use_token: Option<&str>) {
    uc.remove(use_token);
}
