// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
/// Runtime identity with generation checks; slot reuse never revives old handles.
#[derive(Clone, Debug)]
pub struct FsmId {
    pub(super) owner: Rc<()>,
    pub(super) slot: usize,
    pub(super) generation: u64,
}
/// Owned definition identity, retained after unregistration for live instances.
#[derive(Clone, Debug)]
pub struct FsmDefinitionId {
    pub(super) owner: Rc<()>,
    pub(super) slot: usize,
}
impl PartialEq for FsmId {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner)
            && self.slot == other.slot
            && self.generation == other.generation
    }
}
impl Eq for FsmId {}
impl PartialEq for FsmDefinitionId {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner) && self.slot == other.slot
    }
}
impl Eq for FsmDefinitionId {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum TermCause {
    Parent = 0,
    Request = 1,
    Regular = 2,
    Error = 3,
    Timeout = 4,
}
/// Borrowed application data or the timer number captured before timeout dispatch.
pub enum EventData<'a, E> {
    None,
    User(&'a E),
    Timeout(i32),
}
pub type Action<C, D, E> = fn(
    &mut TimerScheduler<C, FsmRuntime<C, D, E>>,
    &mut FsmRuntime<C, D, E>,
    FsmId,
    u32,
    &EventData<'_, E>,
);
pub type StateCallback<C, D, E> =
    fn(&mut TimerScheduler<C, FsmRuntime<C, D, E>>, &mut FsmRuntime<C, D, E>, FsmId, u32);
pub type TermCallback<C, D, E> =
    fn(&mut TimerScheduler<C, FsmRuntime<C, D, E>>, &mut FsmRuntime<C, D, E>, FsmId, TermCause);
pub type TimeoutCallback<C, D, E> =
    fn(&mut TimerScheduler<C, FsmRuntime<C, D, E>>, &mut FsmRuntime<C, D, E>, FsmId) -> i32;
/// State rules with optional names and synchronous handle-based callbacks.
pub struct FsmState<C, D, E> {
    pub name: Option<String>,
    pub in_event_mask: u32,
    pub out_state_mask: u32,
    pub action: Option<Action<C, D, E>>,
    pub onenter: Option<StateCallback<C, D, E>>,
    pub onleave: Option<StateCallback<C, D, E>>,
}
/// Immutable definition; registration owns its tables and callback pointers.
pub struct Fsm<C, D, E> {
    pub name: String,
    pub states: Vec<FsmState<C, D, E>>,
    pub allstate_event_mask: u32,
    pub allstate_action: Option<Action<C, D, E>>,
    pub pre_term: Option<TermCallback<C, D, E>>,
    pub cleanup: Option<TermCallback<C, D, E>>,
    pub timer_cb: Option<TimeoutCallback<C, D, E>>,
    pub event_names: Option<Vec<(u32, Option<String>)>>,
}
/// Read-only inspection; private application data has typed downcast access.
/// No API exposes a mutable instance borrow across callback execution.
pub struct FsmInstance {
    pub state: u32,
    pub timer_number: i32,
    pub terminating: bool,
    pub id: Option<String>,
    pub name: String,
    pub(super) definition: FsmDefinitionId,
    pub(super) timer: TimerId,
    pub(super) private: Box<dyn Any>,
    pub(super) parent: Option<FsmId>,
    pub(super) parent_term_event: u32,
    pub(super) children: Vec<FsmId>,
}
impl FsmInstance {
    pub fn parent(&self) -> Option<&FsmId> {
        self.parent.as_ref()
    }
    pub fn children(&self) -> &[FsmId] {
        &self.children
    }
    pub fn parent_term_event(&self) -> u32 {
        self.parent_term_event
    }
    pub fn timer(&self) -> &TimerId {
        &self.timer
    }
    pub fn definition(&self) -> &FsmDefinitionId {
        &self.definition
    }
    pub fn private<T: Any>(&self) -> Option<&T> {
        self.private.downcast_ref()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmError {
    InvalidHandle,
    InvalidDefinition,
    InvalidDescriptor,
    InvalidIdentifier,
    DuplicateName,
    NotRegistered,
    InvalidState(u32),
    InvalidEvent(u32),
    Terminating,
    TransitionNotPermitted,
    EventNotPermitted,
    HierarchyRequiresTermination,
    ParentCycle,
    StuckChild,
    Timer(TimerError),
    TDef(crate::tdef::TDefError),
    TimeoutOverflow,
    GenerationExhausted,
}
impl FsmError {
    pub(crate) fn compatibility_status(&self) -> i32 {
        match self {
            Self::InvalidHandle => -19,
            Self::DuplicateName => -17,
            Self::TransitionNotPermitted | Self::EventNotPermitted => -1,
            _ => -22,
        }
    }
}
impl From<TimerError> for FsmError {
    fn from(e: TimerError) -> Self {
        Self::Timer(e)
    }
}
impl fmt::Display for FsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FsmError {}
