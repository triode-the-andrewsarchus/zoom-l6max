//! Callback admission model. This is not connected to firmware hooks.
//!
//! A lease must be acquired before loading a callback pointer or borrowing
//! its state or shared scratch, and held until all accesses are complete.
//! Live and file-mix leases are mutually exclusive because the stock paths
//! share scratch buffers. Closing this gate does not stop DMA or code that
//! does not participate in it.
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

    /// Stops new leases. Only one selector can hold the close guard; a caller
    /// must wait for `is_drained` before touching callback-owned state.
    pub fn try_close(&self) -> Option<CloseGuard<'_>> {
        let previous = self.state.fetch_or(CLOSED, Ordering::AcqRel);
        (previous & CLOSED == 0).then(|| CloseGuard { gate: self })
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
    }
}
