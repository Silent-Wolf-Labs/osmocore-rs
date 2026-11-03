// SPDX-License-Identifier: GPL-3.0-or-later

//! Named use counters for one object.
//!
//! Each use of an object has a token, so a leak or a double release names its source.
//! Instead of running a callback, [`UseCount::change`] reports what changed, and the
//! caller decides what to do, such as releasing the object at a total of zero.
//!
//! ```
//! use osmocore_rs::use_count::UseCount;
//! let mut uses = UseCount::new();
//! uses.get("paging")?;
//! uses.get("sms")?;
//! uses.get("sms")?;
//! assert_eq!(uses.to_string(), "3 (paging,2*sms)");
//!
//! uses.put("paging")?;
//! let change = uses.put("sms")?;
//! assert_eq!((change.old, change.new), (2, 1));
//! assert_eq!(uses.total(), 1);
//! # Ok::<(), osmocore_rs::use_count::CountOutOfRange>(())
//! ```

use std::error::Error;
use std::fmt;

/// A change would take a count past the range of `i32`. The count stays as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountOutOfRange;

impl fmt::Display for CountOutOfRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("use count out of range")
    }
}

impl Error for CountOutOfRange {}

/// What a call to [`UseCount::change`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UseChange {
    /// The position of the entry in [`UseCount::entries`]. Later calls can move it.
    pub entry: usize,
    /// The count before the change.
    pub old: i32,
    /// The count after the change.
    pub new: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    token: Option<Box<str>>,
    count: i32,
}

/// Counts of an object's uses, by token.
///
/// A token can be absent, as a `None`. Entries stay listed at a count of zero, and a new
/// token takes over the first idle one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UseCount {
    entries: Vec<Entry>,
    /// Whether entries were ever added, which changes the text of an empty list.
    listed: bool,
}

impl UseCount {
    /// Returns counters with no entries.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            listed: false,
        }
    }

    /// Adds `count` idle entries with no token, at the end of the list.
    pub fn add_idle_entries(&mut self, count: usize) {
        self.listed = true;
        self.entries.extend((0..count).map(|_| Entry {
            token: None,
            count: 0,
        }));
    }

    fn find(&self, token: Option<&str>) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.token.as_deref() == token)
    }

    /// Adds `amount` to a token's count. A token with no entry takes over the first
    /// idle entry, or gets a new one, and an entry leaving zero moves to the end of the
    /// list. An `amount` of zero changes nothing, not even the list.
    pub fn change<'a>(
        &mut self,
        token: impl Into<Option<&'a str>>,
        amount: i32,
    ) -> Result<UseChange, CountOutOfRange> {
        let token = token.into();
        let found = self.find(token);
        if amount == 0 {
            let count = found.map_or(0, |index| self.entries[index].count);
            return Ok(UseChange {
                entry: found.unwrap_or(self.entries.len()),
                old: count,
                new: count,
            });
        }
        let index = found.unwrap_or_else(|| self.take_idle_entry(token));
        let old = self.entries[index].count;
        let new = old.checked_add(amount).ok_or(CountOutOfRange)?;
        let mut entry = index;
        if old == 0 {
            entry = self.entries.len() - 1;
            self.entries[index..].rotate_left(1);
        }
        self.entries[entry].count = new;
        Ok(UseChange { entry, old, new })
    }

    fn take_idle_entry(&mut self, token: Option<&str>) -> usize {
        let idle = self.entries.iter().position(|entry| entry.count == 0);
        let token = token.map(Box::from);
        match idle {
            Some(index) => {
                self.entries[index].token = token;
                index
            }
            None => {
                self.listed = true;
                self.entries.push(Entry { token, count: 0 });
                self.entries.len() - 1
            }
        }
    }

    /// Adds one to a token's count.
    pub fn get<'a>(
        &mut self,
        token: impl Into<Option<&'a str>>,
    ) -> Result<UseChange, CountOutOfRange> {
        self.change(token, 1)
    }

    /// Takes one from a token's count, which can go below zero.
    pub fn put<'a>(
        &mut self,
        token: impl Into<Option<&'a str>>,
    ) -> Result<UseChange, CountOutOfRange> {
        self.change(token, -1)
    }

    /// Returns the sum of all counts. It stops at the limits of `i32` as it adds, so it
    /// can depend on the order of the entries.
    pub fn total(&self) -> i32 {
        self.entries
            .iter()
            .fold(0, |total: i32, entry| total.saturating_add(entry.count))
    }

    /// Returns the count of the first entry with this token, or 0 if there is none.
    pub fn count_of<'a>(&self, token: impl Into<Option<&'a str>>) -> i32 {
        self.find(token.into())
            .map_or(0, |index| self.entries[index].count)
    }

    /// Returns each entry's token and count in list order, including idle entries.
    pub fn entries(&self) -> impl Iterator<Item = (Option<&str>, i32)> {
        self.entries
            .iter()
            .map(|entry| (entry.token.as_deref(), entry.count))
    }

    /// Sets the count of the entry at a position from [`UseChange::entry`], for example
    /// to undo a change that broke a limit. Returns `false` if there is no such entry.
    pub fn set_entry_count(&mut self, entry: usize, count: i32) -> bool {
        match self.entries.get_mut(entry) {
            Some(entry) => {
                entry.count = count;
                true
            }
            None => false,
        }
    }

    /// Removes the entry at a position from [`UseChange::entry`]. Keeping idle entries
    /// is normally better, because they get reused. Returns `false` if there is no such
    /// entry.
    pub fn remove_entry(&mut self, entry: usize) -> bool {
        let exists = entry < self.entries.len();
        if exists {
            self.entries.remove(entry);
        }
        exists
    }

    /// Removes the first entry with this token. Returns `false` if there is none.
    pub fn remove<'a>(&mut self, token: impl Into<Option<&'a str>>) -> bool {
        self.find(token.into())
            .is_some_and(|index| self.remove_entry(index))
    }
}

impl fmt::Display for UseCount {
    /// Reads like `12 (3*barring,fighting,8*kungfoo)`. A token of `None` shows as `NULL`.
    /// With no count to show, the list reads `-`, unless no entry was ever added.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} (", self.total())?;
        if self.listed {
            let mut used = self.entries.iter().filter(|entry| entry.count != 0);
            match used.next() {
                Some(first) => {
                    write_entry(formatter, first)?;
                    for entry in used {
                        formatter.write_str(",")?;
                        write_entry(formatter, entry)?;
                    }
                }
                None => formatter.write_str("-")?,
            }
        }
        formatter.write_str(")")
    }
}

fn write_entry(formatter: &mut fmt::Formatter<'_>, entry: &Entry) -> fmt::Result {
    if entry.count != 1 {
        write!(formatter, "{}*", entry.count)?;
    }
    formatter.write_str(entry.token.as_deref().unwrap_or("NULL"))
}
