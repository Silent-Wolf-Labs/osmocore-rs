// SPDX-License-Identifier: GPL-3.0-or-later

//! A ring of recent log strings, which forgets the oldest when it's full.
//!
//! ```
//! use osmocore_rs::strrb::StringRing;
//! let mut ring = StringRing::new(2);
//! ring.push("first");
//! ring.push("second");
//! ring.push("third");
//! assert_eq!(ring.len(), 2);
//! assert_eq!(ring.get(0), Some(&b"second"[..]));
//! assert_eq!(ring.get(1), Some(&b"third"[..]));
//! assert_eq!(ring.get(2), None);
//! ```

use std::collections::VecDeque;

/// The bytes a stored string can hold. Longer strings are cut short here.
pub const MAX_STRING_BYTES: usize = 239;

/// A ring of up to `capacity` recent strings, oldest first.
///
/// Strings are bytes, because the length limit can cut through a UTF-8 character.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringRing {
    strings: VecDeque<Vec<u8>>,
    capacity: usize,
}

impl StringRing {
    /// Returns an empty ring that holds up to `capacity` strings. Unlike C's size, this is
    /// the number of strings it holds, with no 16-bit limit. Space is taken as it's used.
    pub const fn new(capacity: usize) -> Self {
        Self {
            strings: VecDeque::new(),
            capacity,
        }
    }

    /// Returns the most strings the ring holds.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of strings held.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Returns whether the ring holds no strings.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Adds a string, forgetting the oldest if the ring is full, and returns whether the
    /// string was cut short to [`MAX_STRING_BYTES`]. A ring with no capacity keeps nothing.
    /// A NUL is an ordinary byte here.
    pub fn push(&mut self, text: impl AsRef<[u8]>) -> bool {
        let text = text.as_ref();
        let kept = &text[..text.len().min(MAX_STRING_BYTES)];
        if self.capacity > 0 {
            let mut slot = if self.strings.len() == self.capacity {
                self.strings.pop_front().unwrap_or_default()
            } else {
                Vec::with_capacity(kept.len())
            };
            slot.clear();
            slot.extend_from_slice(kept);
            self.strings.push_back(slot);
        }
        kept.len() < text.len()
    }

    /// Returns the string at `index`, where 0 is the oldest, or `None` past the newest.
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        self.strings.get(index).map(Vec::as_slice)
    }

    /// Returns the strings, oldest first.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &[u8]> + ExactSizeIterator {
        self.strings.iter().map(Vec::as_slice)
    }
}
