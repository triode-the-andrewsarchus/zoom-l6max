//! Callback admission model. This is not connected to firmware hooks.
//!
//! A lease must be acquired before loading a callback pointer or borrowing
//! its state or shared scratch, and held until all accesses are complete.
//! Full live and file-mix leases are mutually exclusive because the stock
//! paths share effect state and scratch. A separate modeled live-bypass lease
//! permits only work proven independent of file mixing. Closing this gate does
//! not stop DMA or code that does not participate in it.
use core::sync::atomic::{AtomicU32, Ordering};

const LIVE: u32 = 1;
const FILE_MIX: u32 = 2;
const CLOSED: u32 = 4;

#[derive(Clone, Copy)]
pub enum CallbackPath {
    Live,
    FileMix,
}

pub struct CallbackAdmission {
    state: AtomicU32,
}

impl CallbackAdmission {
    pub const fn new() -> Self {
        Self { state: AtomicU32::new(0) }
    }

    /// Makes one strong atomic admission attempt. A closed, occupied, or
    /// invalid state is rejected; callers must handle rejection safely.
    /// The target's atomic implementation may retry and needs deadline qualification.
    pub fn try_enter(&self, path: CallbackPath) -> Option<CallbackLease<'_>> {
        let bit = match path { CallbackPath::Live => LIVE, CallbackPath::FileMix => FILE_MIX };
        self.state.compare_exchange(0, bit, Ordering::AcqRel, Ordering::Acquire)
            .ok().map(|_| CallbackLease { gate: self, bit })
    }

    /// Claims the live side while file mixing owns the effect. The caller may
    /// only use resources proven disjoint from file mixing and must not load
    /// or invoke the effect callback. Acquire before touching live scratch.
    /// This model does not establish those resource boundaries in firmware.
    pub fn try_enter_live_bypass(&self) -> Option<LiveBypassLease<'_>> {
        self.state.compare_exchange(FILE_MIX, FILE_MIX | LIVE, Ordering::AcqRel, Ordering::Acquire)
            .ok().map(|_| LiveBypassLease { gate: self })
    }

    /// Stops new leases. Only one selector can hold the close guard; a caller
    /// must wait for `is_drained` before touching callback-owned state.
    pub fn try_close(&self) -> Option<CloseGuard<'_>> {
        let previous = self.state.fetch_or(CLOSED, Ordering::AcqRel);
        (previous & CLOSED == 0).then(|| CloseGuard { gate: self })
    }
}

/// Owns only the modeled live bypass, not the effect or file-mix state.
#[must_use]
pub struct LiveBypassLease<'a> {
    gate: &'a CallbackAdmission,
}

impl Drop for LiveBypassLease<'_> {
    fn drop(&mut self) {
        self.gate.state.fetch_and(!LIVE, Ordering::Release);
    }
}

#[must_use]
pub struct CallbackLease<'a> {
    gate: &'a CallbackAdmission,
    bit: u32,
}

impl Drop for CallbackLease<'_> {
    fn drop(&mut self) {
        self.gate.state.fetch_and(!self.bit, Ordering::Release);
    }
}

#[must_use]
pub struct CloseGuard<'a> {
    gate: &'a CallbackAdmission,
}

impl CloseGuard<'_> {
    /// True only after all admitted paths have released their leases.
    pub fn is_drained(&self) -> bool {
        self.gate.state.load(Ordering::Acquire) == CLOSED
    }
}

impl Drop for CloseGuard<'_> {
    fn drop(&mut self) {
        self.gate.state.fetch_and(!CLOSED, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_state_rejects_both_callback_paths() {
        let gate = CallbackAdmission::new();
        gate.state.store(8, Ordering::Relaxed);
        assert!(gate.try_enter(CallbackPath::Live).is_none());
        assert!(gate.try_enter(CallbackPath::FileMix).is_none());
        assert!(gate.try_enter_live_bypass().is_none());
    }

    #[test]
    fn bypass_excludes_other_live_work_and_drains_in_either_order() {
        for file_finishes_first in [false, true] {
            let gate = CallbackAdmission::new();
            assert!(gate.try_enter_live_bypass().is_none());
            let file = gate.try_enter(CallbackPath::FileMix).unwrap();
            let bypass = gate.try_enter_live_bypass().unwrap();
            assert!(gate.try_enter_live_bypass().is_none());
            assert!(gate.try_enter(CallbackPath::Live).is_none());
            let close = gate.try_close().unwrap();
            assert!(!close.is_drained());
            assert!(gate.try_enter_live_bypass().is_none());
            if file_finishes_first {
                drop(file);
                assert!(!close.is_drained());
                drop(bypass);
            } else {
                drop(bypass);
                assert!(!close.is_drained());
                drop(file);
            }
            assert!(close.is_drained());
            drop(close);
            assert!(gate.try_enter(CallbackPath::Live).is_some());
        }
    }

    #[test]
    fn bypass_outlives_file_without_allowing_a_new_owner() {
        let gate = CallbackAdmission::new();
        let file = gate.try_enter(CallbackPath::FileMix).unwrap();
        let bypass = gate.try_enter_live_bypass().unwrap();
        drop(file);
        assert!(gate.try_enter(CallbackPath::FileMix).is_none());
        assert!(gate.try_enter(CallbackPath::Live).is_none());
        assert!(gate.try_enter_live_bypass().is_none());
        drop(bypass);
        assert!(gate.try_enter(CallbackPath::FileMix).is_some());
    }
}
