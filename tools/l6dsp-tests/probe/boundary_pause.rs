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

    /// Close this pause before publishing the snapshot. An unexpected index
    /// may invalidate it after the caller's last validity check.
    pub fn publish_capture(&self, request: u32, capture: &crate::handoff_protocol::CaptureProtocol<'_>, claim: u32) -> bool {
        let epoch = request & EPOCH;
        let closed = request & PHASE == REQUESTED && epoch != EPOCH
            && self.0.compare_exchange(epoch | PAUSED, epoch + 4, Ordering::AcqRel, Ordering::Acquire).is_ok();
        if closed {
            capture.finish(claim, 15, true)
        } else {
            capture.finish(claim, 0, false);
            false
        }
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
