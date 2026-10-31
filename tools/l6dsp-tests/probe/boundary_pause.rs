//! QEMU-only request/acknowledgment protocol. It does not establish that all
//! firmware users of the captured state pass through the observed audio hook.
use core::sync::atomic::{AtomicU32, Ordering};

const PHASE: u32 = 3;
const REQUESTED: u32 = 1;
const PAUSED: u32 = 2;
const EPOCH: u32 = !PHASE;

pub struct BoundaryPause<'a>(pub &'a AtomicU32);

impl BoundaryPause<'_> {
    pub fn request(&self) -> Option<u32> {
        let state = self.0.load(Ordering::Acquire);
        if state & PHASE != 0 || state & EPOCH == EPOCH { return None; }
        let request = state | REQUESTED;
        self.0.compare_exchange(state, request, Ordering::AcqRel, Ordering::Acquire).ok()?;
        Some(request)
    }

    /// Called at the observed AINR hook; index zero acknowledges a request.
    pub fn bypass(&self, index: u32) -> bool {
        let state = self.0.load(Ordering::Acquire);
        if !matches!(state & PHASE, REQUESTED | PAUSED) { return false; }
        if index >= 2 {
            // An unobserved stock index invalidates the pause, so a snapshot
            // made concurrently with it cannot be published as complete.
            self.finish((state & EPOCH) | REQUESTED);
            return false;
        }
        if index == 0 && state & PHASE == REQUESTED {
            let _ = self.0.compare_exchange(state, (state & EPOCH) | PAUSED, Ordering::AcqRel, Ordering::Acquire);
        }
        true
    }

    pub fn acknowledged(&self, request: u32) -> bool {
        request & PHASE == REQUESTED && self.0.load(Ordering::Acquire) == (request & EPOCH) | PAUSED
    }

    /// Ends only this request; late acknowledgments cannot affect its successor.
    pub fn finish(&self, request: u32) -> bool {
        if request & PHASE != REQUESTED { return false; }
        let epoch = request & EPOCH;
        if epoch == EPOCH { return false; }
        let mut state = self.0.load(Ordering::Acquire);
        loop {
            if state & EPOCH != epoch || !matches!(state & PHASE, REQUESTED | PAUSED) { return false; }
            let next = epoch + 4;
            match self.0.compare_exchange_weak(state, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return true,
                Err(actual) => state = actual,
            }
        }
    }
}
