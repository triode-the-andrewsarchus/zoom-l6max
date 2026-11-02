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

    /// Rejects a closed gate or either occupied path. Atomic retries are not bounded;
    /// the caller must define safe real-time behavior for rejection.
    pub fn try_enter(&self, path: CallbackPath) -> Option<CallbackLease<'_>> {
        let bit = match path { CallbackPath::Live => LIVE, CallbackPath::FileMix => FILE_MIX };
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            if state & (CLOSED | LIVE | FILE_MIX) != 0 { return None; }
            match self.state.compare_exchange_weak(state, state | bit, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(CallbackLease { gate: self, bit }),
                Err(actual) => state = actual,
            }
        }
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
