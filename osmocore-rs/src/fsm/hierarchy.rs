// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::diagnostics::{event, observe};

impl<C: Clock, D, E> FsmRuntime<C, D, E> {
    pub fn alloc_child<P: Any>(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        definition: &FsmDefinitionId,
        parent: &FsmId,
        event: u32,
        private: P,
    ) -> Result<FsmId, FsmError> {
        self.checked(s, parent)?;
        let name = self.instance(parent)?.id.clone();
        let id = self.alloc(s, definition, name.as_deref(), private)?;
        self.change_parent(s, &id, Some(parent), event)?;
        Ok(id)
    }
    pub fn unlink_parent(
        &mut self,
        s: &TimerScheduler<C, Self>,
        id: &FsmId,
    ) -> Result<(), FsmError> {
        observe!(self, Some(id), "unlink_parent", diagnostics, {
            self.checked(s, id)?;
            self.detach(id)?;
            let i = self.index(id)?;
            let inst = self.slots[i].instance.as_mut().unwrap();
            if inst.parent.is_some() {
                inst.parent = None;
                inst.parent_term_event = 0;
            }
            #[cfg(feature = "tracing")]
            diagnostics.emit("unlink_parent");
            Ok(())
        })
    }
    fn detach(&mut self, id: &FsmId) -> Result<(), FsmError> {
        let parent = self.instance(id)?.parent.clone();
        if let Some(parent) = parent
            && let Ok(i) = self.index(&parent)
        {
            self.slots[i]
                .instance
                .as_mut()
                .unwrap()
                .children
                .retain(|child| child != id);
        }
        Ok(())
    }
    pub fn change_parent(
        &mut self,
        s: &TimerScheduler<C, Self>,
        id: &FsmId,
        parent: Option<&FsmId>,
        event: u32,
    ) -> Result<(), FsmError> {
        observe!(self, Some(id), "change_parent", diagnostics, {
            self.checked(s, id)?;
            if let Some(parent) = parent {
                self.checked(s, parent)?;
                let mut ancestor = Some(parent.clone());
                while let Some(p) = ancestor {
                    if p == *id {
                        return Err(FsmError::ParentCycle);
                    }
                    ancestor = self.instance(&p)?.parent.clone();
                }
            }
            self.unlink_parent(s, id)?;
            if let Some(parent) = parent {
                let i = self.index(id)?;
                let inst = self.slots[i].instance.as_mut().unwrap();
                inst.parent = Some(parent.clone());
                inst.parent_term_event = event;
                let p = self.index(parent)?;
                self.slots[p]
                    .instance
                    .as_mut()
                    .unwrap()
                    .children
                    .insert(0, id.clone());
            }
            event!(
                debug,
                "osmocore_rs::fsm",
                operation = "change_parent",
                fsm_name = diagnostics.fsm_name.as_deref(),
                instance_slot = diagnostics.instance_slot,
                instance_generation = diagnostics.instance_generation,
                parent_slot = parent.map(|p| p.slot),
                parent_generation = parent.map(|p| p.generation)
            );
            Ok(())
        })
    }
    fn sweep(&mut self) {
        if self.termination_depth != 0 {
            return;
        }
        for slot in &mut self.slots {
            if !slot.visible && slot.instance.is_some() {
                slot.generation = slot.generation.saturating_add(1);
                slot.instance = None;
            }
        }
    }
    fn retire(&mut self, s: &mut TimerScheduler<C, Self>, id: &FsmId) -> Result<(), FsmError> {
        let i = self.checked(s, id)?;
        let timer = self.slots[i].instance.as_ref().unwrap().timer.clone();
        s.release(&timer)?;
        self.slots[i].visible = false;
        #[cfg(feature = "tracing")]
        self.diagnostic_context(Some(id)).emit("retire");
        self.sweep();
        Ok(())
    }
    /// Raw free is safe only for an unlinked instance without children.
    pub fn free(&mut self, s: &mut TimerScheduler<C, Self>, id: &FsmId) -> Result<(), FsmError> {
        observe!(self, Some(id), "free", diagnostics, {
            self.checked(s, id)?;
            let inst = self.instance(id)?;
            if inst.parent.is_some() || !inst.children.is_empty() {
                return Err(FsmError::HierarchyRequiresTermination);
            }
            if inst.terminating {
                return Err(FsmError::Terminating);
            }
            self.retire(s, id)
        })
    }
    pub fn term_children(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        cause: TermCause,
        data: &EventData<'_, E>,
    ) -> Result<(), FsmError> {
        observe!(self, Some(id), "term_children", diagnostics, {
            self.checked(s, id)?;
            loop {
                let Some(child) = self.instance(id)?.children.first().cloned() else {
                    return Ok(());
                };
                self.term(s, &child, cause, data)?;
                // FsmId equality includes owner and generation. Mutating siblings
                // alone is not progress while this same child remains at the head.
                if self.instance(id)?.children.first() == Some(&child) {
                    return Err(FsmError::StuckChild);
                }
            }
        })
    }
    pub fn broadcast_children(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        event: u32,
        data: &EventData<'_, E>,
    ) -> Result<(), FsmError> {
        observe!(self, Some(id), "broadcast_children", diagnostics, {
            self.checked(s, id)?;
            let mut child = self.instance(id)?.children.first().cloned();
            while let Some(current) = child {
                // Cache the next sibling before dispatch, as C's safe-list iterator does.
                let siblings = &self.instance(id)?.children;
                let position = siblings
                    .iter()
                    .position(|c| *c == current)
                    .ok_or(FsmError::InvalidHandle)?;
                child = siblings.get(position + 1).cloned();
                match self.dispatch(s, Some(&current), event, data) {
                    Ok(()) | Err(FsmError::EventNotPermitted) | Err(FsmError::Terminating) => {}
                    Err(e) => return Err(e),
                }
                // A callback may have terminated the parent or a cached sibling.
                if !self.observable(id) {
                    return Err(FsmError::InvalidHandle);
                }
            }
            Ok(())
        })
    }
    fn term_body(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        cause: TermCause,
        data: &EventData<'_, E>,
        cascading: bool,
    ) -> Result<(), FsmError> {
        let inst = self.instance(id)?;
        let fsm = self.definitions[inst.definition.slot].fsm.clone();
        // C captures the notification event before pre_term, but fetches the
        // parent again after cleanup (pre_term may reparent the instance).
        let parent_event = inst.parent_term_event;
        if let Some(cb) = fsm.pre_term {
            cb(s, self, id.clone(), cause)
        }
        self.term_children(s, id, TermCause::Parent, &EventData::None)?;
        self.detach(id)?;
        if let Some(cb) = fsm.cleanup {
            cb(s, self, id.clone(), cause)
        }
        let parent = self.instance(id)?.parent.clone();
        if !cascading {
            self.retire(s, id)?;
        }
        if let Some(parent) = parent
            && cause != TermCause::Parent
        {
            // C ignores the parent's dispatch status. Safe handle validation
            // prevents stale-pointer dereferences in undefined C cascades.
            let _ = self.dispatch(s, Some(&parent), parent_event, data);
        }
        Ok(())
    }
    pub fn term(
        &mut self,
        s: &mut TimerScheduler<C, Self>,
        id: &FsmId,
        cause: TermCause,
        data: &EventData<'_, E>,
    ) -> Result<(), FsmError> {
        observe!(self, Some(id), "term", diagnostics, {
            if self.instance(id)?.terminating {
                return Ok(());
            }
            let i = self.checked(s, id)?;
            event!(
                debug,
                "osmocore_rs::fsm",
                operation = "termination_requested",
                fsm_name = diagnostics.fsm_name.as_deref(),
                instance_slot = diagnostics.instance_slot,
                instance_generation = diagnostics.instance_generation,
                cause = cause as i32
            );
            self.slots[i].instance.as_mut().unwrap().terminating = true;
            let cascading = self.safe_termination || self.termination_depth != 0;
            if cascading {
                self.termination_depth += 1;
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.term_body(s, id, cause, data, cascading)
            }));
            if cascading {
                self.termination_depth -= 1;
            }
            match result {
                Ok(result) => {
                    if result.is_ok() && cascading {
                        self.retire(s, id)?;
                    }
                    self.sweep();
                    #[cfg(feature = "tracing")]
                    if result.is_ok() {
                        diagnostics.emit("termination_completed");
                    }
                    result
                }
                Err(panic) => {
                    self.sweep();
                    event!(
                        error,
                        "osmocore_rs::fsm",
                        operation = "term",
                        fsm_name = diagnostics.fsm_name.as_deref(),
                        instance_slot = diagnostics.instance_slot,
                        instance_generation = diagnostics.instance_generation,
                        error_category = "callback_panic"
                    );
                    std::panic::resume_unwind(panic)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::SystemClock;

    #[derive(Default)]
    struct Data {
        head: Option<FsmId>,
        nested: Option<Result<(), FsmError>>,
    }
    type Runtime = FsmRuntime<SystemClock, Data>;

    fn remove_sibling_and_retry(
        scheduler: &mut TimerScheduler<SystemClock, Runtime>,
        runtime: &mut Runtime,
        child: FsmId,
        _: TermCause,
    ) {
        if runtime.data.head.as_ref() != Some(&child) {
            return;
        }
        let parent = runtime.instance(&child).unwrap().parent().unwrap().clone();
        let sibling = runtime
            .instance(&parent)
            .unwrap()
            .children()
            .last()
            .unwrap()
            .clone();
        runtime
            .term(scheduler, &sibling, TermCause::Regular, &EventData::None)
            .unwrap();
        runtime.data.nested =
            Some(runtime.term_children(scheduler, &parent, TermCause::Parent, &EventData::None));
    }

    #[test]
    fn sibling_removal_does_not_hide_a_reentrant_stuck_head() {
        for safe in [false, true] {
            let mut scheduler = TimerScheduler::new(SystemClock);
            let mut runtime = Runtime::new(Data::default());
            runtime.term_safely(safe);
            let definition = runtime
                .register(Fsm {
                    name: "progress".into(),
                    states: vec![FsmState {
                        name: None,
                        in_event_mask: 0,
                        out_state_mask: 0,
                        action: None,
                        onenter: None,
                        onleave: None,
                    }],
                    allstate_event_mask: 0,
                    allstate_action: None,
                    pre_term: Some(remove_sibling_and_retry),
                    cleanup: None,
                    timer_cb: None,
                    event_names: None,
                })
                .unwrap();
            let parent = runtime
                .alloc(&mut scheduler, &definition, None, ())
                .unwrap();
            runtime
                .alloc_child(&mut scheduler, &definition, &parent, 0, ())
                .unwrap();
            let head = runtime
                .alloc_child(&mut scheduler, &definition, &parent, 0, ())
                .unwrap();
            runtime.data.head = Some(head);
            runtime
                .term_children(&mut scheduler, &parent, TermCause::Parent, &EventData::None)
                .unwrap();
            assert_eq!(runtime.data.nested, Some(Err(FsmError::StuckChild)));
            assert!(runtime.instance(&parent).unwrap().children().is_empty());
        }
    }
}
