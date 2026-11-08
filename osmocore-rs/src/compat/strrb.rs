// SPDX-License-Identifier: GPL-3.0-or-later

use crate::base::strrb::StringRing;

/// C's limit on a stored string, including its NUL.
pub const RB_MAX_MESSAGE_SIZE: usize = crate::base::strrb::MAX_STRING_BYTES + 1;

const EINVAL_RESULT: i32 = -22;

/// C's `struct osmo_strrb`: a ring with `size` slots, which holds one string fewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsmoStrrb {
    ring: StringRing,
    start: u16,
    end: u16,
    size: u16,
}

impl OsmoStrrb {
    /// Returns the slot of the oldest string.
    pub const fn start(&self) -> u16 {
        self.start
    }

    /// Returns the slot the next string goes in.
    pub const fn end(&self) -> u16 {
        self.end
    }

    /// Returns the number of slots.
    pub const fn size(&self) -> u16 {
        self.size
    }
}

/// Returns an empty ring. As in C, `rb_size` is cut to 16 bits, and the ring holds one string
/// fewer than its size.
pub fn osmo_strrb_create(rb_size: usize) -> OsmoStrrb {
    let size = rb_size as u16;
    OsmoStrrb {
        ring: StringRing::new(usize::from(size).saturating_sub(1)),
        start: 0,
        end: 0,
        size,
    }
}

/// Returns whether the ring holds no strings.
pub const fn osmo_strrb_is_empty(rb: &OsmoStrrb) -> bool {
    rb.end == rb.start
}

/// Returns whether a slot holds a string.
pub const fn _osmo_strrb_is_bufindex_valid(rb: &OsmoStrrb, bufi: u32) -> bool {
    if osmo_strrb_is_empty(rb) || bufi >= rb.size as u32 {
        false
    } else if rb.start < rb.end {
        bufi >= rb.start as u32 && bufi < rb.end as u32
    } else {
        bufi < rb.end as u32 || bufi >= rb.start as u32
    }
}

/// Returns the number of strings held.
pub const fn osmo_strrb_elements(rb: &OsmoStrrb) -> usize {
    if rb.end < rb.start {
        rb.end as usize + (rb.size - rb.start) as usize
    } else {
        (rb.end - rb.start) as usize
    }
}

/// Returns the string at `string_index`, where 0 is the oldest. As in C, an index past the
/// newest can still return a string once the ring has wrapped: the index is added to the
/// start with 32-bit wrapping, one size is taken off, and any slot holding a string counts.
pub fn osmo_strrb_get_nth(rb: &OsmoStrrb, string_index: u32) -> Option<&[u8]> {
    let (start, end, size) = (u32::from(rb.start), u32::from(rb.end), u32::from(rb.size));
    let mut slot = string_index.wrapping_add(start);
    if slot >= size && start > end {
        slot -= size;
    }
    if !_osmo_strrb_is_bufindex_valid(rb, slot) {
        return None;
    }
    rb.ring.get(((slot + size - start) % size) as usize)
}

/// Adds `data` up to its first NUL, forgetting the oldest string if the ring is full. Returns
/// 1 if it was cut short, otherwise 0. Returns -22 with no slots, where C crashes.
pub fn osmo_strrb_add(rb: &mut OsmoStrrb, data: &[u8]) -> i32 {
    if rb.size == 0 {
        return EINVAL_RESULT;
    }
    let text = data.split(|&byte| byte == 0).next().unwrap_or(data);
    let truncated = rb.ring.push(text);
    rb.end = (rb.end + 1) % rb.size;
    if rb.end == rb.start {
        rb.start = (rb.start + 1) % rb.size;
    }
    i32::from(truncated)
}
