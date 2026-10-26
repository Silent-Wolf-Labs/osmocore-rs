// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::diagnostics::{event, observe};
pub(super) struct Slot {
    pub(super) generation: u64,
    pub(super) instance: Option<FsmInstance>,
    pub(super) visible: bool,
    pub(super) order: u64,
}
pub(super) struct Definition<C, D, E> {
    pub(super) fsm: Rc<Fsm<C, D, E>>,
    pub(super) registered: bool,
}
/// Pair with `TimerScheduler<C, FsmRuntime<C, D, E>>`. `D` is caller-owned
/// runtime context and `E` is the application event-data type. Instance-private
/// values are owned independently. Native globals and talloc contexts are
/// replaced by runtime ownership. Optional tracing observes execution without
/// owning a logger. Safe termination ordering defaults off,
/// as in C; both modes use checked handles and safe storage reclamation.
pub struct FsmRuntime<C, D, E = ()> {
    pub data: D,
    pub(super) owner: Rc<()>,
    pub(super) definitions: Vec<Definition<C, D, E>>,
    pub(super) slots: Vec<Slot>,
    pub(super) callback_error: Option<FsmError>,
    pub(super) safe_termination: bool,
    pub(super) termination_depth: usize,
    pub(super) allocation_order: u64,
}
const ILLEGAL: &[u8] = b"., {}[]()<>|~\\^`'\"?=;/+*&%$#!";
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| (0x20..=0x7e).contains(&b) && !ILLEGAL.contains(&b))
}
impl<C, D, E> FsmRuntime<C, D, E> {
    #[cfg(feature = "tracing")]
    pub(super) fn diagnostic_context(&self, id: Option<&FsmId>) -> crate::diagnostics::FsmContext {
        let valid = id.and_then(|id| self.index(id).ok().map(|i| (id, i)));
        let instance = valid.map(|(_, i)| self.slots[i].instance.as_ref().unwrap());
        crate::diagnostics::FsmContext {
            fsm_name: instance.map(|i| self.definitions[i.definition.slot].fsm.name.clone()),
            instance_slot: valid.map(|(id, _)| id.slot),
            instance_generation: valid.map(|(id, _)| id.generation),
            current_state: instance.map(|i| i.state),
        }
    }
    pub fn new(data: D) -> Self {
        Self {
            data,
            owner: Rc::new(()),
            definitions: Vec::new(),
            slots: Vec::new(),
            callback_error: None,
            safe_termination: false,
            termination_depth: 0,
            allocation_order: 0,
        }
    }
    pub(super) fn definition_index(&self, id: &FsmDefinitionId) -> Result<usize, FsmError> {
        if Rc::ptr_eq(&self.owner, &id.owner) && id.slot < self.definitions.len() {
            Ok(id.slot)
        } else {
            Err(FsmError::InvalidDefinition)
        }
    }
    pub(super) fn index(&self, id: &FsmId) -> Result<usize, FsmError> {
        if Rc::ptr_eq(&self.owner, &id.owner)
            && let Some(s) = self.slots.get(id.slot)
            && s.generation == id.generation
            && s.instance.is_some()
        {
            return Ok(id.slot);
        }
        Err(FsmError::InvalidHandle)
    }
    pub fn instance(&self, id: &FsmId) -> Result<&FsmInstance, FsmError> {
        Ok(self.slots[self.index(id)?].instance.as_ref().unwrap())
    }
    pub fn private_mut<T: Any>(&mut self, id: &FsmId) -> Result<Option<&mut T>, FsmError> {
        let i = self.index(id)?;
        Ok(self.slots[i]
            .instance
            .as_mut()
            .unwrap()
            .private
            .downcast_mut())
    }
    pub fn instances(&self, definition: &FsmDefinitionId) -> Result<Vec<FsmId>, FsmError> {
        self.definition_index(definition)?;
        let mut slots = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.visible
                    && s.instance
                        .as_ref()
                        .map(|i| i.definition == *definition)
                        .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        slots.sort_by_key(|(_, s)| std::cmp::Reverse(s.order));
        Ok(slots
            .into_iter()
            .map(|(slot, s)| FsmId {
                owner: self.owner.clone(),
                slot,
                generation: s.generation,
            })
            .collect())
    }
    pub fn find_by_id(
        &self,
        definition: &FsmDefinitionId,
        id: &str,
    ) -> Result<Option<FsmId>, FsmError> {
        Ok(self
            .instances(definition)?
            .into_iter()
            .find(|h| self.instance(h).unwrap().id.as_deref() == Some(id)))
    }
    pub fn find_by_name(
        &self,
        definition: &FsmDefinitionId,
        name: &str,
    ) -> Result<Option<FsmId>, FsmError> {
        Ok(self
            .instances(definition)?
            .into_iter()
            .find(|h| self.instance(h).unwrap().name == name))
    }
    pub fn register(&mut self, fsm: Fsm<C, D, E>) -> Result<FsmDefinitionId, FsmError> {
        if !identifier(&fsm.name) {
            return Err(FsmError::InvalidIdentifier);
        }
        if self
            .definitions
            .iter()
            .any(|d| d.registered && d.fsm.name == fsm.name)
        {
            return Err(FsmError::DuplicateName);
        }
        if fsm.states.is_empty() || fsm.states.len() > 32 {
            return Err(FsmError::InvalidDescriptor);
        }
        let id = FsmDefinitionId {
            owner: self.owner.clone(),
            slot: self.definitions.len(),
        };
        self.definitions.push(Definition {
            fsm: Rc::new(fsm),
            registered: true,
        });
        Ok(id)
    }
    /// Select C-compatible callback/removal ordering; storage reclamation remains safe.
    pub fn term_safely(&mut self, enabled: bool) {
        self.safe_termination = enabled;
    }
    pub fn observable(&self, id: &FsmId) -> bool {
        self.index(id)
            .map(|i| self.slots[i].visible)
            .unwrap_or(false)
    }
    pub fn update_id(&mut self, id: &FsmId, name: Option<&str>) -> Result<(), FsmError> {
        let i = self.index(id)?;
        if name.map(|s| !identifier(s)).unwrap_or(false) {
            return Err(FsmError::InvalidIdentifier);
        }
        let inst = self.slots[i].instance.as_mut().unwrap();
        let fsm = &self.definitions[inst.definition.slot].fsm;
        inst.id = name.map(str::to_owned);
        inst.name = match name {
            Some(n) => format!("{}({n})", fsm.name),
            None => fsm.name.clone(),
        };
        Ok(())
    }
    pub fn unregister(&mut self, id: &FsmDefinitionId) -> Result<(), FsmError> {
        let i = self.definition_index(id)?;
        if !self.definitions[i].registered {
            return Err(FsmError::NotRegistered);
        }
        self.definitions[i].registered = false;
        Ok(())
    }
    pub fn find_fsm_by_name(&self, name: &str) -> Option<FsmDefinitionId> {
        self.definitions
            .iter()
            .position(|d| d.registered && d.fsm.name == name)
            .map(|slot| FsmDefinitionId {
                owner: self.owner.clone(),
                slot,
            })
    }
    pub fn descriptor(&self, id: &FsmDefinitionId) -> Result<&Fsm<C, D, E>, FsmError> {
        Ok(&self.definitions[self.definition_index(id)?].fsm)
    }
    pub fn update_id_sanitize(
        &mut self,
        id: &FsmId,
        replacement: u8,
        text: Option<&str>,
    ) -> Result<(), FsmError> {
        let text = match text {
            Some(t) => t,
            None => return self.update_id(id, None),
        };
        // Rust strings are complete values; embedded NUL is rejected rather
        // than silently accepting only the prefix. A NUL replacement, however,
        // truncates the generated C-style destination just as the C helper does.
        if text.contains('\0') {
            return Err(FsmError::InvalidIdentifier);
        }
        let bytes = text
            .bytes()
            .map(|b| if ILLEGAL.contains(&b) { replacement } else { b })
            .take_while(|b| *b != 0)
            .collect::<Vec<_>>();
        let sanitized = std::str::from_utf8(&bytes).map_err(|_| FsmError::InvalidIdentifier)?;
        self.update_id(id, Some(sanitized))
    }
    pub fn event_name(&self, id: &FsmDefinitionId, event: u32) -> Result<Cow<'_, str>, FsmError> {
        let fsm = self.descriptor(id)?;
        match &fsm.event_names {
            None => Ok(Cow::Owned(event.to_string())),
            Some(names) => {
                for (value, name) in names {
                    match name {
                        None => break,
                        Some(name) if *value == event => return Ok(Cow::Borrowed(name)),
                        _ => {}
                    }
                }
                Ok(Cow::Owned(format!("unknown 0x{event:x}")))
            }
        }
    }
    pub fn state_name(
        &self,
        id: &FsmDefinitionId,
        state: u32,
    ) -> Result<Option<Cow<'_, str>>, FsmError> {
        let fsm = self.descriptor(id)?;
        match fsm.states.get(state as usize) {
            Some(s) => Ok(s.name.as_deref().map(Cow::Borrowed)),
            None => Ok(Some(Cow::Owned(format!("unknown {state}")))),
        }
    }
    /// Consume an internal timeout-callback error after raw scheduler dispatch.
    /// `update()` consumes and returns the same error automatically.
    pub fn take_callback_error(&mut self) -> Option<FsmError> {
        self.callback_error.take()
    }
}
impl<C: Clock, D, E> FsmRuntime<C, D, E> {
    pub fn alloc<P: Any>(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        definition: &FsmDefinitionId,
        id: Option<&str>,
        private: P,
    ) -> Result<FsmId, FsmError> {
        self.definition_index(definition)?;
        let d = definition.slot;
        if id.map(|s| !identifier(s)).unwrap_or(false) {
            return Err(FsmError::InvalidIdentifier);
        }
        let name = match id {
            Some(id) => format!("{}({id})", self.definitions[d].fsm.name),
            None => self.definitions[d].fsm.name.clone(),
        };
        let slot = self
            .slots
            .iter()
            .position(|s| s.instance.is_none() && s.generation < u64::MAX)
            .unwrap_or(self.slots.len());
        if slot == self.slots.len() {
            self.slots.push(Slot {
                generation: 0,
                instance: None,
                visible: false,
                order: 0,
            })
        }
        self.allocation_order = self
            .allocation_order
            .checked_add(1)
            .ok_or(FsmError::GenerationExhausted)?;
        self.slots[slot].order = self.allocation_order;
        self.slots[slot].visible = true;
        let timer = s.create(Some(Self::timeout));
        self.slots[slot].instance = Some(FsmInstance {
            state: 0,
            timer_number: 0,
            terminating: false,
            id: id.map(str::to_owned),
            name,
            definition: definition.clone(),
            timer,
            private: Box::new(private),
            parent: None,
            parent_term_event: 0,
            children: Vec::new(),
        });
        event!(
            debug,
            "osmocore_rs::fsm",
            operation = "alloc",
            fsm_name = self.definitions[d].fsm.name.as_str(),
            instance_slot = slot,
            instance_generation = self.slots[slot].generation,
            current_state = 0u32
        );
        Ok(FsmId {
            owner: self.owner.clone(),
            slot,
            generation: self.slots[slot].generation,
        })
    }
    pub(super) fn checked(
        &self,
        s: &TimerScheduler<C, Self>,
        id: &FsmId,
    ) -> Result<usize, FsmError> {
        let i = self.index(id)?;
        s.deadline(&self.slots[i].instance.as_ref().unwrap().timer)?;
        Ok(i)
    }
    pub fn dispatch(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: Option<&FsmId>,
        event: u32,
        data: &EventData<'_, E>,
    ) -> Result<(), FsmError> {
        observe!(self, id, "dispatch", diagnostics, {
            event!(
                trace,
                "osmocore_rs::fsm",
                operation = "dispatch",
                event,
                fsm_name = diagnostics.fsm_name.as_deref(),
                instance_slot = diagnostics.instance_slot,
                instance_generation = diagnostics.instance_generation,
                current_state = diagnostics.current_state
            );
            let id = id.ok_or(FsmError::InvalidHandle)?;
            if self.instance(id)?.terminating {
                return Err(FsmError::Terminating);
            }
            let i = self.checked(s, id)?;
            let instance = self.slots[i].instance.as_ref().unwrap();
            if instance.terminating {
                return Err(FsmError::Terminating);
            }
            if event >= 32 {
                return Err(FsmError::InvalidEvent(event));
            }
            let fsm = self.definitions[instance.definition.slot].fsm.clone();
            let st = fsm
                .states
                .get(instance.state as usize)
                .ok_or(FsmError::InvalidState(instance.state))?;
            let cb = if fsm.allstate_event_mask & (1u32 << event) != 0
                && fsm.allstate_action.is_some()
            {
                fsm.allstate_action
            } else {
                if st.in_event_mask & (1u32 << event) == 0 {
                    return Err(FsmError::EventNotPermitted);
                }
                st.action
            };
            if let Some(cb) = cb {
                cb(s, self, id.clone(), event, data)
            }
            Ok(())
        })
    }
    pub fn state_change(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        new_state: u32,
        timeout_secs: u64,
        t: i32,
    ) -> Result<(), FsmError> {
        self.change(s, id, new_state, false, timeout_secs.wrapping_mul(1000), t)
    }
    pub fn state_change_ms(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        new_state: u32,
        timeout_ms: u64,
        t: i32,
    ) -> Result<(), FsmError> {
        self.change(s, id, new_state, false, timeout_ms, t)
    }
    pub fn state_change_keep_timer(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        new_state: u32,
    ) -> Result<(), FsmError> {
        self.change(s, id, new_state, true, 0, 0)
    }
    pub fn state_change_keep_or_start_timer(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        new_state: u32,
        timeout_secs: u64,
        t: i32,
    ) -> Result<(), FsmError> {
        self.change(s, id, new_state, true, timeout_secs.wrapping_mul(1000), t)
    }
    pub fn state_change_keep_or_start_timer_ms(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        new_state: u32,
        timeout_ms: u64,
        t: i32,
    ) -> Result<(), FsmError> {
        self.change(s, id, new_state, true, timeout_ms, t)
    }
    pub fn set_timer_number(
        &mut self,
        s: &TimerScheduler<C, Self>,
        id: &FsmId,
        t: i32,
    ) -> Result<(), FsmError> {
        observe!(self, Some(id), "set_timer_number", diagnostics, {
            let i = self.checked(s, id)?;
            self.slots[i].instance.as_mut().unwrap().timer_number = t;
            Ok(())
        })
    }
    fn timeout(s: &mut TimerScheduler<C, Self>, timer: TimerId, r: &mut Self) {
        let id = r.slots.iter().enumerate().find_map(|(slot, entry)| {
            entry
                .instance
                .as_ref()
                .filter(|i| i.timer == timer)
                .map(|_| FsmId {
                    owner: r.owner.clone(),
                    slot,
                    generation: entry.generation,
                })
        });
        if let Some(id) = id {
            #[cfg(feature = "tracing")]
            let span = r.diagnostic_context(Some(&id)).span("timeout");
            #[cfg(feature = "tracing")]
            let _guard = span.enter();
            let i = r.instance(&id).unwrap();
            let t = i.timer_number;
            let cb = r.definitions[i.definition.slot].fsm.timer_cb;
            if let Some(cb) = cb
                && cb(s, r, id.clone()) != 1
            {
                return;
            }
            if let Err(e) = r.term(s, &id, TermCause::Timeout, &EventData::Timeout(t))
                && r.callback_error.is_none()
            {
                event!(
                    warn,
                    "osmocore_rs::fsm",
                    operation = "timeout",
                    error_category = "callback_error"
                );
                r.callback_error = Some(e)
            }
        } else if r.callback_error.is_none() {
            event!(
                warn,
                "osmocore_rs::fsm",
                operation = "timeout",
                error_category = "invalid_handle"
            );
            r.callback_error = Some(FsmError::InvalidHandle)
        }
    }
    fn change(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        new_state: u32,
        keep: bool,
        timeout_ms: u64,
        t: i32,
    ) -> Result<(), FsmError> {
        observe!(self, Some(id), "state_change", diagnostics, {
            if self.instance(id)?.terminating {
                return Err(FsmError::Terminating);
            }
            let i = self.checked(s, id)?;
            let inst = self.slots[i].instance.as_ref().unwrap();
            if inst.terminating {
                return Err(FsmError::Terminating);
            }
            if new_state >= 32 {
                return Err(FsmError::InvalidState(new_state));
            }
            let fsm = self.definitions[inst.definition.slot].fsm.clone();
            let old = inst.state;
            let st = fsm
                .states
                .get(old as usize)
                .ok_or(FsmError::InvalidState(old))?;
            if st.out_state_mask & (1u32 << new_state) == 0 {
                return Err(FsmError::TransitionNotPermitted);
            }
            if new_state as usize >= fsm.states.len() {
                return Err(FsmError::InvalidState(new_state));
            }
            let timer = inst.timer.clone();
            let leave = st.onleave;
            if !keep {
                s.delete(&timer)?
            }
            if let Some(cb) = leave {
                cb(s, self, id.clone(), new_state)
            }
            // C may use freed memory if onleave terminates this instance. Rust reports
            // a stale handle after preserving the completed cancellation/callback prefix.
            if !self.observable(id) {
                return Err(FsmError::InvalidHandle);
            }
            let i = self.index(id)?;
            self.slots[i].instance.as_mut().unwrap().state = new_state;
            // This mutation is committed even if subsequent scheduling fails.
            event!(
                debug,
                "osmocore_rs::fsm",
                operation = "state_commit",
                fsm_name = fsm.name.as_str(),
                instance_slot = id.slot,
                instance_generation = id.generation,
                old_state = old,
                new_state
            );
            if !keep || !s.pending(&timer)? {
                self.slots[i].instance.as_mut().unwrap().timer_number = t;
                if timeout_ms != 0 {
                    s.schedule(
                        &timer,
                        (timeout_ms / 1000) as i32,
                        ((timeout_ms % 1000) * 1000) as i32,
                    )?
                }
            }
            if let Some(cb) = fsm.states[new_state as usize].onenter {
                cb(s, self, id.clone(), old)
            }
            event!(
                debug,
                "osmocore_rs::fsm",
                operation = "state_change",
                fsm_name = fsm.name.as_str(),
                instance_slot = id.slot,
                instance_generation = id.generation,
                old_state = old,
                new_state
            );
            Ok(())
        })
    }
}
impl<C: Clock, D, E> FsmRuntime<C, D, E> {
    pub fn update(&mut self, s: &mut TimerScheduler<C, Self>) -> Result<i32, FsmError> {
        let rc = s.update(self)?;
        if let Some(e) = self.take_callback_error() {
            return Err(e);
        }
        Ok(rc)
    }
}

impl<C: Clock, D, E> FsmRuntime<C, D, E> {
    /// Change state with the bounded timeout table, preserving single-use,
    /// keep-timer and fallback-in-seconds behavior.
    pub fn state_change_with_tdef(
        &mut self,
        scheduler: &mut crate::timer::TimerScheduler<C, crate::fsm::FsmRuntime<C, D, E>>,
        instance: &crate::fsm::FsmId,
        state: u32,
        timeouts: &[crate::tdef::StateTimeout; 32],
        table: Option<&[crate::tdef::TDef<'_>]>,
        default_timeout: i64,
    ) -> Result<(), crate::fsm::FsmError> {
        observe!(
            self,
            Some(instance),
            "state_change_with_tdef",
            diagnostics,
            {
                use crate::compat::tdef::{osmo_tdef_get_entry, osmo_tdef_get_state_timeout};
                use crate::tdef::{TDefError, TDefUnit, convert};
                let mapping =
                    osmo_tdef_get_state_timeout(state, timeouts).map_err(FsmError::TDef)?;
                let mapping = match mapping {
                    Some(m) => m,
                    None => return self.state_change(scheduler, instance, state, 0, 0),
                };
                let mut value = 0;
                if mapping.id != 0 {
                    match osmo_tdef_get_entry(table, mapping.id).map_err(FsmError::TDef)? {
                        Some(t) => {
                            value = convert(t.val, t.unit, TDefUnit::MS).map_err(FsmError::TDef)?;
                            if t.unit == TDefUnit::CUSTOM {
                                value = value.wrapping_mul(1000)
                            }
                        }
                        None => {
                            if default_timeout < 0 {
                                return Err(FsmError::TDef(TDefError::NegativeFallback {
                                    id: mapping.id,
                                    fallback: default_timeout,
                                }));
                            }
                            value = default_timeout
                                .checked_mul(1000)
                                .ok_or(FsmError::TimeoutOverflow)?
                                as u64;
                        }
                    }
                }
                if mapping.keep_timer {
                    if mapping.id != 0 {
                        self.state_change_keep_or_start_timer_ms(
                            scheduler, instance, state, value, mapping.id,
                        )
                    } else {
                        self.state_change_keep_timer(scheduler, instance, state)
                    }
                } else {
                    self.state_change_ms(scheduler, instance, state, value, mapping.id)
                }
            }
        )
    }
}
