// SPDX-License-Identifier: GPL-3.0-or-later

//! Owned equivalents of C group/registry operations and periodic rollups.
use super::RateCtr;
use crate::time::TimeError;
use std::{fmt, rc::Rc};

/// Owned strings replace C's retained descriptor/name pointers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateCtrDesc {
    pub name: String,
    pub description: Option<String>,
}
/// `class_id` is C `int` (signed 32-bit). Counter count is checked separately
/// against C `unsigned int num_ctr` (unsigned 32-bit), then derived from storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateCtrGroupDesc {
    pub group_name_prefix: String,
    pub group_description: Option<String>,
    pub class_id: i32,
    pub counters: Vec<RateCtrDesc>,
}
impl RateCtrGroupDesc {
    pub(super) fn prepare(mut self) -> Result<Self, RateCtrError> {
        u32::try_from(self.counters.len()).map_err(|_| RateCtrError::CounterCountOverflow)?;
        // C retries validation after replacing only '.' with ':'.
        self.group_name_prefix = self.group_name_prefix.replace('.', ":");
        for c in &mut self.counters {
            c.name = c.name.replace('.', ":");
        }
        if !identifier_valid(&self.group_name_prefix)
            || self.counters.iter().any(|c| !identifier_valid(&c.name))
        {
            return Err(RateCtrError::InvalidDescriptor);
        }
        for description in std::iter::once(&self.group_description)
            .chain(self.counters.iter().map(|c| &c.description))
        {
            if description.as_ref().is_some_and(|s| s.contains('\0')) {
                return Err(RateCtrError::EmbeddedNul);
            }
        }
        Ok(self)
    }
}
fn identifier_valid(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| (0x20..=0x7e).contains(&b) && !b"., {}[]()<>|~\\^`'\"?=;/+*&%$#!".contains(&b))
}
/// Identity uses the owning runtime, slot and generation; the mutable C index
/// never participates in identity. A cloned handle does not keep a group alive.
#[derive(Debug, Clone)]
pub struct RateCtrGroupId {
    pub(super) owner: Rc<()>,
    pub(super) slot: usize,
    pub(super) generation: u64,
}
impl PartialEq for RateCtrGroupId {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner)
            && self.slot == other.slot
            && self.generation == other.generation
    }
}
impl Eq for RateCtrGroupId {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateCtrError {
    InvalidDescriptor,
    EmbeddedNul,
    CounterCountOverflow,
    InvalidHandle,
    CounterOutOfRange,
    GenerationExhausted,
    CurrentGroupRemoved,
    ZeroExpirations,
    Clock(TimeError),
    ClockWentBackwards,
    TimeOverflow,
}
impl fmt::Display for RateCtrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDescriptor => f.write_str("invalid rate-counter descriptor"),
            Self::EmbeddedNul => f.write_str("embedded NUL in owned string"),
            Self::CounterCountOverflow => f.write_str("counter count exceeds C unsigned int"),
            Self::InvalidHandle => f.write_str("foreign or released rate-counter group handle"),
            Self::CounterOutOfRange => f.write_str("counter index outside group"),
            Self::GenerationExhausted => f.write_str("group generation exhausted"),
            Self::CurrentGroupRemoved => {
                f.write_str("iteration cannot continue after removal of its current group")
            }
            Self::ZeroExpirations => f.write_str("expiration count must be positive"),
            Self::Clock(e) => e.fmt(f),
            Self::ClockWentBackwards => f.write_str("monotonic clock went backwards"),
            Self::TimeOverflow => f.write_str("periodic driver time overflow"),
        }
    }
}
impl std::error::Error for RateCtrError {}
impl From<TimeError> for RateCtrError {
    fn from(e: TimeError) -> Self {
        Self::Clock(e)
    }
}
#[derive(Debug)]
pub struct RateCtrGroup {
    pub(super) descriptor: RateCtrGroupDesc,
    pub(super) index: u32,
    pub(super) name: Option<String>,
    pub(super) counters: Vec<RateCtr>,
}
impl RateCtrGroup {
    pub fn descriptor(&self) -> &RateCtrGroupDesc {
        &self.descriptor
    }
    /// C `unsigned int idx`, recorded as unsigned 32-bit.
    pub fn index(&self) -> u32 {
        self.index
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// C `unsigned int num_ctr`; checked at allocation, not a pointer-array size.
    pub fn num_ctr(&self) -> u32 {
        self.counters.len() as u32
    }
    pub fn counters(&self) -> &[RateCtr] {
        &self.counters
    }
    pub fn counter(&self, index: u32) -> Result<&RateCtr, RateCtrError> {
        self.counters
            .get(index as usize)
            .ok_or(RateCtrError::CounterOutOfRange)
    }
    pub fn counter_mut(&mut self, index: u32) -> Result<&mut RateCtr, RateCtrError> {
        self.counters
            .get_mut(index as usize)
            .ok_or(RateCtrError::CounterOutOfRange)
    }
    pub fn counter_by_name(&self, name: &str) -> Option<u32> {
        self.descriptor
            .counters
            .iter()
            .position(|c| c.name == name)
            .map(|i| i as u32)
    }
    pub fn reset(&mut self) {
        self.counters.fill(RateCtr::default());
    }
}
