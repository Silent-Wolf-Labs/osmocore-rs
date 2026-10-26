// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::rc::Rc;

struct Slot {
    generation: u64,
    group: Option<RateCtrGroup>,
}
/// An independent registry and global tick phase. This does not implement C's
/// process-global `rate_ctr_init`, select loop or timerfd API.
///
/// ```
/// use osmocore_rs::rate_ctr::{RateCtrDesc, RateCtrGroupDesc, RateCtrRuntime};
/// let mut runtime = RateCtrRuntime::new();
/// let group = runtime.allocate(RateCtrGroupDesc {
///     group_name_prefix: "example:traffic".into(),
///     group_description: Some("Traffic counters".into()),
///     class_id: 0,
///     counters: vec![RateCtrDesc { name: "packets".into(), description: None }],
/// }, 0)?;
/// runtime.add(&group, 0, 7)?;
/// runtime.process_expirations(1)?;
/// assert_eq!(runtime.group(&group)?.counter(0)?.intv[0].rate, 7);
/// # Ok::<(), osmocore_rs::rate_ctr::RateCtrError>(())
/// ```
pub struct RateCtrRuntime {
    owner: Rc<()>,
    slots: Vec<Slot>,
    registry: Vec<usize>,
    ticks: u64,
}
impl Default for RateCtrRuntime {
    fn default() -> Self {
        Self::new()
    }
}
impl RateCtrRuntime {
    pub fn new() -> Self {
        Self::with_tick_phase(0)
    }
    /// Restore an explicit phase for replay or runtime restoration. The first
    /// expiration increments this phase before checking interval boundaries.
    pub fn with_tick_phase(ticks: u64) -> Self {
        Self {
            owner: Rc::new(()),
            slots: vec![],
            registry: vec![],
            ticks,
        }
    }
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
    fn handle(&self, slot: usize) -> RateCtrGroupId {
        RateCtrGroupId {
            owner: self.owner.clone(),
            slot,
            generation: self.slots[slot].generation,
        }
    }
    fn slot(&self, id: &RateCtrGroupId) -> Result<usize, RateCtrError> {
        if Rc::ptr_eq(&self.owner, &id.owner)
            && let Some(s) = self.slots.get(id.slot)
            && s.generation == id.generation
            && s.group.is_some()
        {
            return Ok(id.slot);
        }
        Err(RateCtrError::InvalidHandle)
    }
    pub fn group(&self, id: &RateCtrGroupId) -> Result<&RateCtrGroup, RateCtrError> {
        Ok(self.slots[self.slot(id)?].group.as_ref().unwrap())
    }
    pub fn group_mut(&mut self, id: &RateCtrGroupId) -> Result<&mut RateCtrGroup, RateCtrError> {
        let slot = self.slot(id)?;
        Ok(self.slots[slot].group.as_mut().unwrap())
    }
    /// Duplicate selection uses the caller's original prefix before mangling,
    /// and the C ordered, wrapping scan rather than a mathematical maximum.
    pub fn allocate(
        &mut self,
        descriptor: RateCtrGroupDesc,
        mut index: u32,
    ) -> Result<RateCtrGroupId, RateCtrError> {
        if self
            .find_group(&descriptor.group_name_prefix, index)
            .is_some()
        {
            let mut unused = 0u32;
            for &s in &self.registry {
                let g = self.slots[s].group.as_ref().unwrap();
                if g.descriptor.group_name_prefix == descriptor.group_name_prefix
                    && unused <= g.index
                {
                    unused = g.index.wrapping_add(1);
                }
            }
            index = unused;
        }
        let descriptor = descriptor.prepare()?;
        let counters = vec![RateCtr::default(); descriptor.counters.len()];
        let slot = self
            .slots
            .iter()
            .position(|s| s.group.is_none() && s.generation < u64::MAX)
            .unwrap_or(self.slots.len());
        if slot == self.slots.len() {
            self.slots.push(Slot {
                generation: 0,
                group: None,
            });
        }
        self.slots[slot].group = Some(RateCtrGroup {
            descriptor,
            index,
            name: None,
            counters,
        });
        self.registry.insert(0, slot);
        Ok(self.handle(slot))
    }
    pub fn free(&mut self, id: &RateCtrGroupId) -> Result<(), RateCtrError> {
        let slot = self.slot(id)?;
        let next = self.slots[slot]
            .generation
            .checked_add(1)
            .ok_or(RateCtrError::GenerationExhausted)?;
        self.registry.retain(|&s| s != slot);
        self.slots[slot].group = None;
        self.slots[slot].generation = next;
        Ok(())
    }
    /// Safe equivalent of explicitly unlinking a C group list entry. The owned
    /// group/handle survives, but lookup, iteration and rollups no longer see it.
    pub fn unregister(&mut self, id: &RateCtrGroupId) -> Result<(), RateCtrError> {
        let slot = self.slot(id)?;
        self.registry.retain(|&s| s != slot);
        Ok(())
    }
    pub fn registered_groups(&self) -> Vec<RateCtrGroupId> {
        self.registry.iter().map(|&s| self.handle(s)).collect()
    }
    pub fn find_group(&self, prefix: &str, index: u32) -> Option<RateCtrGroupId> {
        self.registry.iter().find_map(|&s| {
            let g = self.slots[s].group.as_ref().unwrap();
            (g.descriptor.group_name_prefix == prefix && g.index == index).then(|| self.handle(s))
        })
    }
    pub fn set_index(&mut self, id: &RateCtrGroupId, index: u32) -> Result<(), RateCtrError> {
        self.group_mut(id)?.index = index;
        Ok(())
    }
    pub fn set_name(
        &mut self,
        id: &RateCtrGroupId,
        name: Option<&str>,
    ) -> Result<(), RateCtrError> {
        self.slot(id)?;
        if name.is_some_and(|s| s.contains('\0')) {
            return Err(RateCtrError::EmbeddedNul);
        }
        self.group_mut(id)?.name = name.map(str::to_owned);
        Ok(())
    }
    pub fn add(
        &mut self,
        id: &RateCtrGroupId,
        index: u32,
        increment: i32,
    ) -> Result<(), RateCtrError> {
        self.group_mut(id)?.counter_mut(index)?.add(increment);
        Ok(())
    }
    pub fn inc(&mut self, id: &RateCtrGroupId, index: u32) -> Result<(), RateCtrError> {
        self.add(id, index, 1)
    }
    pub fn reset(&mut self, id: &RateCtrGroupId) -> Result<(), RateCtrError> {
        self.group_mut(id)?.reset();
        Ok(())
    }
    /// Callback receives runtime access, enabling nested lookup/iteration and
    /// removal of the next group. Allocation at the head does not join this
    /// traversal. A negative callback returns immediately, even if it freed
    /// the current group; otherwise current removal returns a typed error.
    pub fn for_each_group<F>(&mut self, mut callback: F) -> Result<i32, RateCtrError>
    where
        F: FnMut(&mut Self, &RateCtrGroupId) -> i32,
    {
        let mut current = self.registry.first().copied().map(|s| self.handle(s));
        let mut rc = 0;
        while let Some(id) = current {
            rc = callback(self, &id);
            if rc < 0 {
                return Ok(rc);
            }
            self.slot(&id)
                .map_err(|_| RateCtrError::CurrentGroupRemoved)?;
            let pos = self
                .registry
                .iter()
                .position(|&s| s == id.slot)
                .ok_or(RateCtrError::CurrentGroupRemoved)?;
            current = self.registry.get(pos + 1).copied().map(|s| self.handle(s));
        }
        Ok(rc)
    }
    /// Counter indices visit descriptor order. Owned descriptors are immutable;
    /// callbacks can reset counters or perform nested registry/counter visits.
    pub fn for_each_counter<F>(
        &mut self,
        id: &RateCtrGroupId,
        mut callback: F,
    ) -> Result<i32, RateCtrError>
    where
        F: FnMut(&mut Self, &RateCtrGroupId, u32) -> i32,
    {
        let count = self.group(id)?.num_ctr();
        let mut rc = 0;
        for index in 0..count {
            rc = callback(self, id, index);
            if rc < 0 {
                return Ok(rc);
            }
            self.slot(id)
                .map_err(|_| RateCtrError::CurrentGroupRemoved)?;
        }
        Ok(rc)
    }
    /// Process every delivered expiration independently. `previous` belongs to
    /// difference/reporting and remains untouched. No missed ticks are merged.
    pub fn process_expirations(&mut self, count: u64) -> Result<(), RateCtrError> {
        if count == 0 {
            return Err(RateCtrError::ZeroExpirations);
        }
        for _ in 0..count {
            self.ticks = self.ticks.wrapping_add(1);
            for &slot in &self.registry {
                for c in &mut self.slots[slot].group.as_mut().unwrap().counters {
                    for (interval, period) in [
                        (RATE_CTR_INTV_SEC, 1),
                        (RATE_CTR_INTV_MIN, 60),
                        (RATE_CTR_INTV_HOUR, 3600),
                        (RATE_CTR_INTV_DAY, 86400),
                    ] {
                        if self.ticks.is_multiple_of(period) {
                            let i = &mut c.intv[interval];
                            i.rate = c.current.wrapping_sub(i.last);
                            i.last = c.current;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
