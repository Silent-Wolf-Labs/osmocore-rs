// SPDX-License-Identifier: GPL-3.0-or-later

//! Fixed-capacity owned message buffers with checked offsets.
//!
//! Primitive functions return backing-buffer offsets in place of C pointers.
//! Bytes outside the active message remain accessible and are never erased by
//! untyped reserve, put/get, push/pull, trim, or reset. Typed writes and
//! tag/length prepending modify only their newly added bytes. Copy and area
//! resize document their separate byte-preservation rules below. Capacity and
//! message length fit the C API's 16-bit fields. Rust queues own their messages. Allocation contexts, intrusive list links, and
//! raw destination or channel pointers have no Rust counterpart here.

/// An operation cannot be performed safely. Errors leave the buffer unchanged,
/// except formatted append, which may overwrite unused tail bytes like C.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MsgbError {
    CapacityTooLarge,
    NotEnoughHeadroom,
    NotEnoughTailroom,
    MessageTooShort,
    NegativeLength,
    ReserveOnNonempty,
    InvalidLayer,
    OffsetOutOfRange,
    /// A resized area must be fully contained in active message storage.
    AreaOutsideMessage,
    /// A helper requires a layer marker that is unset.
    MissingLayer,
    /// Pull-to-layer requires a marker between data and tail, inclusive.
    LayerOutsideMessage,
    /// Checked offset arithmetic or an L3 trim total cannot be represented.
    ArithmeticOverflow,
}

/// An owned buffer whose storage never moves or grows during primitive calls.
///
/// Layer offsets may refer to unused bytes or the one-past-end position. Their
/// order is checked by `msgb_test_invariant`, rather than silently changed by
/// primitive operations. The control buffer uses fixed 64-bit words, matching
/// the recorded C ABI; this is an explicit policy independent of Rust's host.
#[derive(Debug)]
pub struct Msgb {
    pub(super) backing: Box<[u8]>,
    pub(super) data: usize,
    pub(super) tail: usize,
    pub(super) layers: [Option<usize>; 4],
    pub cb: [u64; 5],
}

impl Msgb {
    /// Allocate zero-initialized storage. Rust ownership replaces C free/talloc.
    pub fn new(capacity: usize) -> Result<Self, MsgbError> {
        if capacity > usize::from(u16::MAX) {
            return Err(MsgbError::CapacityTooLarge);
        }
        Ok(Self {
            backing: vec![0; capacity].into_boxed_slice(),
            data: 0,
            tail: 0,
            layers: [None; 4],
            cb: [0; 5],
        })
    }

    /// Capacity includes the reserved headroom, as in C `msgb_alloc_headroom`.
    pub fn with_headroom(capacity: usize, headroom: usize) -> Result<Self, MsgbError> {
        let mut msg = Self::new(capacity)?;
        msgb_reserve(&mut msg, headroom)?;
        Ok(msg)
    }

    pub fn capacity(&self) -> u16 {
        self.backing.len() as u16
    }
    pub fn backing(&self) -> &[u8] {
        &self.backing
    }
    pub fn backing_mut(&mut self) -> &mut [u8] {
        &mut self.backing
    }
    pub fn data(&self) -> &[u8] {
        &self.backing[self.data..self.tail]
    }
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.backing[self.data..self.tail]
    }
    pub fn data_offset(&self) -> usize {
        self.data
    }
    pub fn tail_offset(&self) -> usize {
        self.tail
    }
    pub fn layer_offsets(&self) -> [Option<usize>; 4] {
        self.layers
    }

    /// Set a layer marker (layer numbers 1 through 4), within backing storage.
    /// Slice access to a marker at capacity must still respect slice bounds.
    pub fn set_layer_offset(
        &mut self,
        layer: usize,
        offset: Option<usize>,
    ) -> Result<(), MsgbError> {
        if !(1..=4).contains(&layer) {
            return Err(MsgbError::InvalidLayer);
        }
        if offset.is_some_and(|o| o > self.backing.len()) {
            return Err(MsgbError::OffsetOutOfRange);
        }
        self.layers[layer - 1] = offset;
        Ok(())
    }
}

/// Offset of the active message's start; use `Msgb::data` for its bytes.
pub fn msgb_data(msg: &Msgb) -> usize {
    msg.data
}
pub fn msgb_length(msg: &Msgb) -> u16 {
    (msg.tail - msg.data) as u16
}
pub fn msgb_headroom(msg: &Msgb) -> i32 {
    msg.data as i32
}
pub fn msgb_tailroom(msg: &Msgb) -> i32 {
    (msg.backing.len() - msg.tail) as i32
}
/// Reproduces C `len - data_len` converted to a 32-bit unsigned result.
/// This is distinct from headroom and commonly wraps for a non-full message.
pub fn msgb_headlen(msg: &Msgb) -> u32 {
    u32::from(msgb_length(msg)).wrapping_sub(u32::from(msg.capacity()))
}

/// Move data and tail forward without increasing length or changing bytes.
/// Unlike unchecked C, requires an empty message and sufficient tailroom.
pub fn msgb_reserve(msg: &mut Msgb, len: usize) -> Result<(), MsgbError> {
    if msgb_length(msg) != 0 {
        return Err(MsgbError::ReserveOnNonempty);
    }
    if len > msg.backing.len() - msg.tail {
        return Err(MsgbError::NotEnoughTailroom);
    }
    msg.data += len;
    msg.tail += len;
    Ok(())
}
/// Append space and return its start offset, preserving existing backing bytes.
pub fn msgb_put(msg: &mut Msgb, len: usize) -> Result<usize, MsgbError> {
    if len > msg.backing.len() - msg.tail {
        return Err(MsgbError::NotEnoughTailroom);
    }
    let result = msg.tail;
    msg.tail += len;
    Ok(result)
}
/// Remove bytes from the tail and return the new tail offset.
pub fn msgb_get(msg: &mut Msgb, len: usize) -> Result<usize, MsgbError> {
    if len > usize::from(msgb_length(msg)) {
        return Err(MsgbError::MessageTooShort);
    }
    msg.tail -= len;
    Ok(msg.tail)
}
/// Prepend space and return the new data offset, preserving backing bytes.
pub fn msgb_push(msg: &mut Msgb, len: usize) -> Result<usize, MsgbError> {
    if len > msg.data {
        return Err(MsgbError::NotEnoughHeadroom);
    }
    msg.data -= len;
    Ok(msg.data)
}
/// Remove bytes from the front and return the new data offset.
pub fn msgb_pull(msg: &mut Msgb, len: usize) -> Result<usize, MsgbError> {
    if len > usize::from(msgb_length(msg)) {
        return Err(MsgbError::MessageTooShort);
    }
    msg.data += len;
    Ok(msg.data)
}
/// Set the absolute message length, allowing both shrinking and extending.
///
/// C checks against total capacity, which can create a tail beyond storage
/// when data has headroom. Rust checks storage remaining after data instead.
/// A negative length becomes an error rather than invoking C's panic handler.
pub fn msgb_trim(msg: &mut Msgb, len: i32) -> Result<(), MsgbError> {
    if len < 0 {
        return Err(MsgbError::NegativeLength);
    }
    if len as usize > msg.backing.len() - msg.data {
        return Err(MsgbError::NotEnoughTailroom);
    }
    msg.tail = msg.data + len as usize;
    Ok(())
}
/// Clear the active message, headroom, L2-L4, and control words.
/// L1 and all backing bytes are preserved, matching the local C reference.
pub fn msgb_reset(msg: &mut Msgb) {
    msg.data = 0;
    msg.tail = 0;
    msg.layers[1..].fill(None);
    msg.cb.fill(0);
}
/// Match C's ordered layer-marker invariant. Layers need not be in the message.
pub fn msgb_test_invariant(msg: &Msgb) -> bool {
    let mut bound = 0;
    for offset in msg.layers.into_iter().flatten() {
        if offset < bound {
            return false;
        }
        bound = offset;
    }
    // Storage offsets and message length are valid by construction.
    bound <= msg.backing.len()
}

/// Append an 8-bit value in big-endian byte order.
pub fn msgb_put_u8(msg: &mut Msgb, value: u8) -> Result<(), MsgbError> {
    let offset = msgb_put(msg, 1)?;
    msg.backing[offset..offset + 1].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Append a 16-bit value in big-endian byte order.
pub fn msgb_put_u16(msg: &mut Msgb, value: u16) -> Result<(), MsgbError> {
    let offset = msgb_put(msg, 2)?;
    msg.backing[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Append a 32-bit value in big-endian byte order.
pub fn msgb_put_u32(msg: &mut Msgb, value: u32) -> Result<(), MsgbError> {
    let offset = msgb_put(msg, 4)?;
    msg.backing[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Prepend an 8-bit value in big-endian byte order.
pub fn msgb_push_u8(msg: &mut Msgb, value: u8) -> Result<(), MsgbError> {
    let offset = msgb_push(msg, 1)?;
    msg.backing[offset..offset + 1].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Prepend a 16-bit value in big-endian byte order.
pub fn msgb_push_u16(msg: &mut Msgb, value: u16) -> Result<(), MsgbError> {
    let offset = msgb_push(msg, 2)?;
    msg.backing[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Prepend a 32-bit value in big-endian byte order.
pub fn msgb_push_u32(msg: &mut Msgb, value: u32) -> Result<(), MsgbError> {
    let offset = msgb_push(msg, 4)?;
    msg.backing[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Remove an 8-bit big-endian value from the tail.
pub fn msgb_get_u8(msg: &mut Msgb) -> Result<u8, MsgbError> {
    let offset = msgb_get(msg, 1)?;
    Ok(u8::from_be_bytes(
        msg.backing[offset..offset + 1]
            .try_into()
            .expect("checked message span"),
    ))
}

/// Remove a 16-bit big-endian value from the tail.
pub fn msgb_get_u16(msg: &mut Msgb) -> Result<u16, MsgbError> {
    let offset = msgb_get(msg, 2)?;
    Ok(u16::from_be_bytes(
        msg.backing[offset..offset + 2]
            .try_into()
            .expect("checked message span"),
    ))
}

/// Remove a 32-bit big-endian value from the tail.
pub fn msgb_get_u32(msg: &mut Msgb) -> Result<u32, MsgbError> {
    let offset = msgb_get(msg, 4)?;
    Ok(u32::from_be_bytes(
        msg.backing[offset..offset + 4]
            .try_into()
            .expect("checked message span"),
    ))
}

/// Remove an 8-bit big-endian value from the front.
pub fn msgb_pull_u8(msg: &mut Msgb) -> Result<u8, MsgbError> {
    let offset = msgb_pull(msg, 1)? - 1;
    Ok(u8::from_be_bytes(
        msg.backing[offset..offset + 1]
            .try_into()
            .expect("checked message span"),
    ))
}

/// Remove a 16-bit big-endian value from the front.
pub fn msgb_pull_u16(msg: &mut Msgb) -> Result<u16, MsgbError> {
    let offset = msgb_pull(msg, 2)? - 2;
    Ok(u16::from_be_bytes(
        msg.backing[offset..offset + 2]
            .try_into()
            .expect("checked message span"),
    ))
}

/// Remove a 32-bit big-endian value from the front.
pub fn msgb_pull_u32(msg: &mut Msgb) -> Result<u32, MsgbError> {
    let offset = msgb_pull(msg, 4)? - 4;
    Ok(u32::from_be_bytes(
        msg.backing[offset..offset + 4]
            .try_into()
            .expect("checked message span"),
    ))
}

/// Backing offset of L1, or `None` when unset.
pub fn msgb_l1(msg: &Msgb) -> Option<usize> {
    msg.layers[0]
}

/// Backing offset of L2, or `None` when unset.
pub fn msgb_l2(msg: &Msgb) -> Option<usize> {
    msg.layers[1]
}

/// Backing offset of L3, or `None` when unset.
pub fn msgb_l3(msg: &Msgb) -> Option<usize> {
    msg.layers[2]
}

/// Backing offset of L4, or `None` when unset.
pub fn msgb_l4(msg: &Msgb) -> Option<usize> {
    msg.layers[3]
}

/// SMS header alias for L4, matching C.
pub fn msgb_sms(msg: &Msgb) -> Option<usize> {
    msgb_l4(msg)
}

fn layer_length(msg: &Msgb, index: usize) -> Result<u32, MsgbError> {
    let offset = msg.layers[index].ok_or(MsgbError::MissingLayer)?;
    Ok((msg.tail as u32).wrapping_sub(offset as u32))
}

/// Tail minus L1, with the C API's 32-bit unsigned wrapping result.
/// A marker beyond the tail is permitted; an unset marker returns an error.
pub fn msgb_l1len(msg: &Msgb) -> Result<u32, MsgbError> {
    layer_length(msg, 0)
}

/// Tail minus L2, with the C API's 32-bit unsigned wrapping result.
/// A marker beyond the tail is permitted; an unset marker returns an error.
pub fn msgb_l2len(msg: &Msgb) -> Result<u32, MsgbError> {
    layer_length(msg, 1)
}

/// Tail minus L3, with the C API's 32-bit unsigned wrapping result.
/// A marker beyond the tail is permitted; an unset marker returns an error.
pub fn msgb_l3len(msg: &Msgb) -> Result<u32, MsgbError> {
    layer_length(msg, 2)
}

/// Tail minus L4, with the C API's 32-bit unsigned wrapping result.
/// A marker beyond the tail is permitted; an unset marker returns an error.
pub fn msgb_l4len(msg: &Msgb) -> Result<u32, MsgbError> {
    layer_length(msg, 3)
}

fn pull_to_layer(msg: &mut Msgb, index: usize) -> Result<usize, MsgbError> {
    let offset = msg.layers[index].ok_or(MsgbError::MissingLayer)?;
    if offset < msg.data || offset > msg.tail {
        return Err(MsgbError::LayerOutsideMessage);
    }
    let result = msgb_pull(msg, offset - msg.data)?;
    msg.layers[..index].fill(None);
    Ok(result)
}

/// Pull to L2 and clear L1. Errors preserve all metadata and bytes.
pub fn msgb_pull_to_l2(msg: &mut Msgb) -> Result<usize, MsgbError> {
    pull_to_layer(msg, 1)
}

/// Pull to L3 and clear L1/L2. Errors preserve all metadata and bytes.
pub fn msgb_pull_to_l3(msg: &mut Msgb) -> Result<usize, MsgbError> {
    pull_to_layer(msg, 2)
}

/// Set total message length to `(L3 - data) + l3len`.
///
/// This signed calculation permits negative L3 lengths when the resulting
/// total is nonnegative, matching defined C calls. L3 may lie outside active
/// data. Missing L3, totals outside `i32`, or tails beyond storage are errors.
pub fn msgb_l3trim(msg: &mut Msgb, l3len: i32) -> Result<(), MsgbError> {
    let offset = msg.layers[2].ok_or(MsgbError::MissingLayer)?;
    let total = offset as i64 - msg.data as i64 + i64::from(l3len);
    let len = i32::try_from(total).map_err(|_| MsgbError::ArithmeticOverflow)?;
    msgb_trim(msg, len)
}

/// Prepend tag and the prior message length narrowed to one byte.
/// Lengths above 255 wrap, as in C. Return the new data offset.
pub fn msgb_push_tl(msg: &mut Msgb, tag: u8) -> Result<usize, MsgbError> {
    let length = msgb_length(msg) as u8;
    let offset = msgb_push(msg, 2)?;
    msg.backing[offset] = tag;
    msg.backing[offset + 1] = length;
    Ok(offset)
}

/// Copy active bytes and offsets into new zero-initialized storage of equal size.
/// Headroom/tailroom bytes and control words are not copied, matching C.
/// Names, talloc contexts, and raw destination/channel pointers are omitted.
pub fn msgb_copy(msg: &Msgb) -> Result<Msgb, MsgbError> {
    msgb_copy_resize(msg, usize::from(msg.capacity()))
}

/// Copy active bytes with a new total capacity, including existing headroom.
///
/// Data/tail and layer offsets are preserved. The new backing bytes outside the
/// active message and all control words are zero. Reject capacities above
/// `u16::MAX`, insufficient capacity for tail, or preserved layers beyond the
/// new allocation. C does not validate those layer offsets before relocation.
pub fn msgb_copy_resize(msg: &Msgb, capacity: usize) -> Result<Msgb, MsgbError> {
    if capacity > usize::from(u16::MAX) {
        return Err(MsgbError::CapacityTooLarge);
    }
    if msg.tail > capacity {
        return Err(MsgbError::NotEnoughTailroom);
    }
    if msg.layers.iter().flatten().any(|offset| *offset > capacity) {
        return Err(MsgbError::OffsetOutOfRange);
    }
    let mut copy = Msgb::new(capacity)?;
    copy.data = msg.data;
    copy.tail = msg.tail;
    copy.layers = msg.layers;
    copy.backing[msg.data..msg.tail].copy_from_slice(msg.data());
    Ok(copy)
}

/// Resize an area of active data, moving its suffix with memmove semantics.
///
/// `area` is a backing offset; sizes use the C signed-int domain. Layer offsets
/// at or after `area + old_size` move by the size difference, including markers
/// in unused storage. Earlier markers remain unchanged. Control words are
/// preserved. Growth does not initialize the extension, and shrinkage does not
/// erase removed bytes. All storage and relocated layers are checked before
/// mutation; Rust supports unset layers without C's NULL relational comparisons.
pub fn msgb_resize_area(
    msg: &mut Msgb,
    area: usize,
    old_size: i32,
    new_size: i32,
) -> Result<(), MsgbError> {
    if old_size < 0 || new_size < 0 {
        return Err(MsgbError::NegativeLength);
    }
    if area < msg.data || area > msg.tail {
        return Err(MsgbError::AreaOutsideMessage);
    }
    let old_size = old_size as usize;
    let new_size = new_size as usize;
    let post_start = area
        .checked_add(old_size)
        .ok_or(MsgbError::ArithmeticOverflow)?;
    if post_start > msg.tail {
        return Err(MsgbError::AreaOutsideMessage);
    }
    if old_size == new_size {
        return Ok(());
    }
    let new_len = (msg.tail - msg.data - old_size)
        .checked_add(new_size)
        .ok_or(MsgbError::ArithmeticOverflow)?;
    if new_len > msg.backing.len() - msg.data {
        return Err(MsgbError::NotEnoughTailroom);
    }
    let mut layers = msg.layers;
    for offset in layers.iter_mut().flatten() {
        if *offset >= post_start {
            let relocated = (*offset - old_size)
                .checked_add(new_size)
                .ok_or(MsgbError::ArithmeticOverflow)?;
            if relocated > msg.backing.len() {
                return Err(MsgbError::OffsetOutOfRange);
            }
            *offset = relocated;
        }
    }
    msg.backing
        .copy_within(post_start..msg.tail, area + new_size);
    msg.tail = msg.data + new_len;
    msg.layers = layers;
    Ok(())
}

/// FIFO queue that owns messages instead of embedding C intrusive list links.
pub(super) fn comparison_data(msg: &Msgb, level: usize) -> Result<&[u8], MsgbError> {
    if level == 0 {
        return Ok(msg.data());
    }
    if level > 4 {
        return Err(MsgbError::InvalidLayer);
    }
    let offset = msg.layers[level - 1].ok_or(MsgbError::MissingLayer)?;
    if offset > msg.tail {
        return Err(MsgbError::LayerOutsideMessage);
    }
    Ok(&msg.backing[offset..msg.tail])
}
/// Optional inputs reproduce C's false result for a NULL message or data.
/// An empty slice is present data, and can compare equal to an empty message.
pub fn msgb_eq_optional(
    msg: Option<&Msgb>,
    level: usize,
    data: Option<&[u8]>,
) -> Result<bool, MsgbError> {
    match (msg, data) {
        (Some(msg), Some(data)) => msgb_eq_level(msg, level, data),
        _ => Ok(false),
    }
}
/// Checked equivalent of `_msgb_eq` without source-location logging.
/// Layers may start in headroom; unset or beyond-tail layers are errors.
pub fn msgb_eq_level(msg: &Msgb, level: usize, data: &[u8]) -> Result<bool, MsgbError> {
    Ok(comparison_data(msg, level)? == data)
}

/// Compare active bytes against a slice.
pub fn msgb_eq_data(msg: &Msgb, data: &[u8]) -> Result<bool, MsgbError> {
    msgb_eq_level(msg, 0, data)
}
/// Compare active bytes of two messages.
pub fn msgb_eq(msg: &Msgb, other: &Msgb) -> Result<bool, MsgbError> {
    msgb_eq_data(msg, comparison_data(other, 0)?)
}

/// Compare L1 bytes against a slice.
pub fn msgb_eq_l1_data(msg: &Msgb, data: &[u8]) -> Result<bool, MsgbError> {
    msgb_eq_level(msg, 1, data)
}
/// Compare L1 bytes of two messages.
pub fn msgb_eq_l1(msg: &Msgb, other: &Msgb) -> Result<bool, MsgbError> {
    msgb_eq_l1_data(msg, comparison_data(other, 1)?)
}

/// Compare L2 bytes against a slice.
pub fn msgb_eq_l2_data(msg: &Msgb, data: &[u8]) -> Result<bool, MsgbError> {
    msgb_eq_level(msg, 2, data)
}
/// Compare L2 bytes of two messages.
pub fn msgb_eq_l2(msg: &Msgb, other: &Msgb) -> Result<bool, MsgbError> {
    msgb_eq_l2_data(msg, comparison_data(other, 2)?)
}

/// Compare L3 bytes against a slice.
pub fn msgb_eq_l3_data(msg: &Msgb, data: &[u8]) -> Result<bool, MsgbError> {
    msgb_eq_level(msg, 3, data)
}
/// Compare L3 bytes of two messages.
pub fn msgb_eq_l3(msg: &Msgb, other: &Msgb) -> Result<bool, MsgbError> {
    msgb_eq_l3_data(msg, comparison_data(other, 3)?)
}

/// Compare L4 bytes against a slice.
pub fn msgb_eq_l4_data(msg: &Msgb, data: &[u8]) -> Result<bool, MsgbError> {
    msgb_eq_level(msg, 4, data)
}
/// Compare L4 bytes of two messages.
pub fn msgb_eq_l4(msg: &Msgb, other: &Msgb) -> Result<bool, MsgbError> {
    msgb_eq_l4_data(msg, comparison_data(other, 4)?)
}

// osmo_hexdump uses a 4096-byte TLS buffer and emits at most 1365 bytes.

impl Msgb {
    /// See [`crate::compat::msgb::msgb_length`] for mutation and error details.
    pub fn len(&self) -> u16 {
        msgb_length(self)
    }
    /// See [`crate::compat::msgb::msgb_headroom`] for mutation and error details.
    pub fn headroom(&self) -> i32 {
        msgb_headroom(self)
    }
    /// See [`crate::compat::msgb::msgb_tailroom`] for mutation and error details.
    pub fn tailroom(&self) -> i32 {
        msgb_tailroom(self)
    }
    /// See [`crate::compat::msgb::msgb_reserve`] for mutation and error details.
    pub fn reserve(&mut self, len: usize) -> Result<(), MsgbError> {
        msgb_reserve(self, len)
    }
    /// See [`crate::compat::msgb::msgb_put`] for mutation and error details.
    pub fn put(&mut self, len: usize) -> Result<usize, MsgbError> {
        msgb_put(self, len)
    }
    /// See [`crate::compat::msgb::msgb_get`] for mutation and error details.
    pub fn get(&mut self, len: usize) -> Result<usize, MsgbError> {
        msgb_get(self, len)
    }
    /// See [`crate::compat::msgb::msgb_push`] for mutation and error details.
    pub fn push(&mut self, len: usize) -> Result<usize, MsgbError> {
        msgb_push(self, len)
    }
    /// See [`crate::compat::msgb::msgb_pull`] for mutation and error details.
    pub fn pull(&mut self, len: usize) -> Result<usize, MsgbError> {
        msgb_pull(self, len)
    }
    /// See [`crate::compat::msgb::msgb_trim`] for mutation and error details.
    pub fn trim(&mut self, len: i32) -> Result<(), MsgbError> {
        msgb_trim(self, len)
    }
    /// See [`crate::compat::msgb::msgb_reset`] for mutation and error details.
    pub fn reset(&mut self) {
        msgb_reset(self)
    }
    /// See [`crate::compat::msgb::msgb_test_invariant`] for mutation and error details.
    pub fn check_invariants(&self) -> bool {
        msgb_test_invariant(self)
    }
    /// See [`crate::compat::msgb::msgb_put_u8`] for mutation and error details.
    pub fn put_u8(&mut self, value: u8) -> Result<(), MsgbError> {
        msgb_put_u8(self, value)
    }
    /// See [`crate::compat::msgb::msgb_put_u16`] for mutation and error details.
    pub fn put_u16(&mut self, value: u16) -> Result<(), MsgbError> {
        msgb_put_u16(self, value)
    }
    /// See [`crate::compat::msgb::msgb_put_u32`] for mutation and error details.
    pub fn put_u32(&mut self, value: u32) -> Result<(), MsgbError> {
        msgb_put_u32(self, value)
    }
    /// See [`crate::compat::msgb::msgb_push_u8`] for mutation and error details.
    pub fn push_u8(&mut self, value: u8) -> Result<(), MsgbError> {
        msgb_push_u8(self, value)
    }
    /// See [`crate::compat::msgb::msgb_push_u16`] for mutation and error details.
    pub fn push_u16(&mut self, value: u16) -> Result<(), MsgbError> {
        msgb_push_u16(self, value)
    }
    /// See [`crate::compat::msgb::msgb_push_u32`] for mutation and error details.
    pub fn push_u32(&mut self, value: u32) -> Result<(), MsgbError> {
        msgb_push_u32(self, value)
    }
    /// See [`crate::compat::msgb::msgb_get_u8`] for mutation and error details.
    pub fn get_u8(&mut self) -> Result<u8, MsgbError> {
        msgb_get_u8(self)
    }
    /// See [`crate::compat::msgb::msgb_get_u16`] for mutation and error details.
    pub fn get_u16(&mut self) -> Result<u16, MsgbError> {
        msgb_get_u16(self)
    }
    /// See [`crate::compat::msgb::msgb_get_u32`] for mutation and error details.
    pub fn get_u32(&mut self) -> Result<u32, MsgbError> {
        msgb_get_u32(self)
    }
    /// See [`crate::compat::msgb::msgb_pull_u8`] for mutation and error details.
    pub fn pull_u8(&mut self) -> Result<u8, MsgbError> {
        msgb_pull_u8(self)
    }
    /// See [`crate::compat::msgb::msgb_pull_u16`] for mutation and error details.
    pub fn pull_u16(&mut self) -> Result<u16, MsgbError> {
        msgb_pull_u16(self)
    }
    /// See [`crate::compat::msgb::msgb_pull_u32`] for mutation and error details.
    pub fn pull_u32(&mut self) -> Result<u32, MsgbError> {
        msgb_pull_u32(self)
    }
    /// See [`crate::compat::msgb::msgb_l1`] for mutation and error details.
    pub fn l1(&self) -> Option<usize> {
        msgb_l1(self)
    }
    /// See [`crate::compat::msgb::msgb_l2`] for mutation and error details.
    pub fn l2(&self) -> Option<usize> {
        msgb_l2(self)
    }
    /// See [`crate::compat::msgb::msgb_l3`] for mutation and error details.
    pub fn l3(&self) -> Option<usize> {
        msgb_l3(self)
    }
    /// See [`crate::compat::msgb::msgb_l4`] for mutation and error details.
    pub fn l4(&self) -> Option<usize> {
        msgb_l4(self)
    }
    /// See [`crate::compat::msgb::msgb_l1len`] for mutation and error details.
    pub fn l1len(&self) -> Result<u32, MsgbError> {
        msgb_l1len(self)
    }
    /// See [`crate::compat::msgb::msgb_l2len`] for mutation and error details.
    pub fn l2len(&self) -> Result<u32, MsgbError> {
        msgb_l2len(self)
    }
    /// See [`crate::compat::msgb::msgb_l3len`] for mutation and error details.
    pub fn l3len(&self) -> Result<u32, MsgbError> {
        msgb_l3len(self)
    }
    /// See [`crate::compat::msgb::msgb_l4len`] for mutation and error details.
    pub fn l4len(&self) -> Result<u32, MsgbError> {
        msgb_l4len(self)
    }
    /// See [`crate::compat::msgb::msgb_pull_to_l2`] for mutation and error details.
    pub fn pull_to_l2(&mut self) -> Result<usize, MsgbError> {
        msgb_pull_to_l2(self)
    }
    /// See [`crate::compat::msgb::msgb_pull_to_l3`] for mutation and error details.
    pub fn pull_to_l3(&mut self) -> Result<usize, MsgbError> {
        msgb_pull_to_l3(self)
    }
    /// See [`crate::compat::msgb::msgb_l3trim`] for mutation and error details.
    pub fn l3trim(&mut self, l3len: i32) -> Result<(), MsgbError> {
        msgb_l3trim(self, l3len)
    }
    /// See [`crate::compat::msgb::msgb_push_tl`] for mutation and error details.
    pub fn push_tl(&mut self, tag: u8) -> Result<usize, MsgbError> {
        msgb_push_tl(self, tag)
    }
    /// See [`crate::compat::msgb::msgb_copy`] for mutation and error details.
    pub fn copy(&self) -> Result<Msgb, MsgbError> {
        msgb_copy(self)
    }
    /// See [`crate::compat::msgb::msgb_copy_resize`] for mutation and error details.
    pub fn copy_resize(&self, capacity: usize) -> Result<Msgb, MsgbError> {
        msgb_copy_resize(self, capacity)
    }
    /// See [`crate::compat::msgb::msgb_resize_area`] for mutation and error details.
    pub fn resize_area(
        &mut self,
        area: usize,
        old_size: i32,
        new_size: i32,
    ) -> Result<(), MsgbError> {
        msgb_resize_area(self, area, old_size, new_size)
    }
    /// See [`crate::compat::msgb::msgb_eq_level`] for mutation and error details.
    pub fn eq_level(&self, level: usize, data: &[u8]) -> Result<bool, MsgbError> {
        msgb_eq_level(self, level, data)
    }
    /// See [`crate::compat::msgb::msgb_eq_data`] for mutation and error details.
    pub fn eq_data(&self, data: &[u8]) -> Result<bool, MsgbError> {
        msgb_eq_data(self, data)
    }
    /// See [`crate::compat::msgb::msgb_eq`] for mutation and error details.
    pub fn eq(&self, other: &Msgb) -> Result<bool, MsgbError> {
        msgb_eq(self, other)
    }
    /// See [`crate::compat::msgb::msgb_eq_l1_data`] for mutation and error details.
    pub fn eq_l1_data(&self, data: &[u8]) -> Result<bool, MsgbError> {
        msgb_eq_l1_data(self, data)
    }
    /// See [`crate::compat::msgb::msgb_eq_l1`] for mutation and error details.
    pub fn eq_l1(&self, other: &Msgb) -> Result<bool, MsgbError> {
        msgb_eq_l1(self, other)
    }
    /// See [`crate::compat::msgb::msgb_eq_l2_data`] for mutation and error details.
    pub fn eq_l2_data(&self, data: &[u8]) -> Result<bool, MsgbError> {
        msgb_eq_l2_data(self, data)
    }
    /// See [`crate::compat::msgb::msgb_eq_l2`] for mutation and error details.
    pub fn eq_l2(&self, other: &Msgb) -> Result<bool, MsgbError> {
        msgb_eq_l2(self, other)
    }
    /// See [`crate::compat::msgb::msgb_eq_l3_data`] for mutation and error details.
    pub fn eq_l3_data(&self, data: &[u8]) -> Result<bool, MsgbError> {
        msgb_eq_l3_data(self, data)
    }
    /// See [`crate::compat::msgb::msgb_eq_l3`] for mutation and error details.
    pub fn eq_l3(&self, other: &Msgb) -> Result<bool, MsgbError> {
        msgb_eq_l3(self, other)
    }
    /// See [`crate::compat::msgb::msgb_eq_l4_data`] for mutation and error details.
    pub fn eq_l4_data(&self, data: &[u8]) -> Result<bool, MsgbError> {
        msgb_eq_l4_data(self, data)
    }
    /// See [`crate::compat::msgb::msgb_eq_l4`] for mutation and error details.
    pub fn eq_l4(&self, other: &Msgb) -> Result<bool, MsgbError> {
        msgb_eq_l4(self, other)
    }
}

impl Msgb {
    /// Whether the active message contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
