//! QEMU-only capture publication protocol; it does not provide quiescence.
use core::sync::atomic::{AtomicU32, Ordering};

const PHASE: u32 = 3;
const WAIT: u32 = 0;
const COPYING: u32 = 1;
const DONE: u32 = 2;
const CANCELLED: u32 = 3;
const STEP_MASK: u32 = 0x3c;
const EPOCH_MASK: u32 = !0x3f;
const EPOCH_INCREMENT: u32 = 0x40;

/// The control word and all copied memory must outlive both tasks.
pub struct CaptureProtocol<'a>(pub &'a AtomicU32);

impl CaptureProtocol<'_> {
    pub fn claim(&self) -> Option<(u32, usize)> {
        let state = self.0.load(Ordering::Acquire);
        if state & PHASE != WAIT { return None; }
        let claim = state | COPYING;
        self.0.compare_exchange(state, claim, Ordering::AcqRel, Ordering::Acquire).ok()?;
        Some((claim, ((state & STEP_MASK) >> 2) as usize))
    }

    pub fn finish(&self, claim: u32, next_step: usize, complete: bool) -> bool {
        if next_step > 15 || claim & PHASE != COPYING { return false; }
        let next = (claim & EPOCH_MASK) | ((next_step as u32) << 2)
            | if complete { DONE } else { WAIT };
        self.0.compare_exchange(claim, next, Ordering::Release, Ordering::Relaxed).is_ok()
    }

    /// Invalidates an incomplete claim; a completed snapshot may be retained.
    pub fn cancel(&self, preserve_done: bool) -> bool {
        let mut state = self.0.load(Ordering::Acquire);
        loop {
            if preserve_done && state & PHASE == DONE { return false; }
            let epoch = state & EPOCH_MASK;
            let next_epoch = epoch.checked_add(EPOCH_INCREMENT).unwrap_or(EPOCH_MASK);
            let next = next_epoch | CANCELLED;
            match self.0.compare_exchange_weak(state, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return true,
                Err(actual) => state = actual,
            }
        }
    }

    /// Rearms only after a selector explicitly enters AINR. Epoch exhaustion
    /// remains cancelled until reset, preventing an old claim from matching.
    pub fn arm(&self) {
        let state = self.0.load(Ordering::Acquire);
        if state & PHASE == CANCELLED && state & EPOCH_MASK != EPOCH_MASK {
            let _ = self.0.compare_exchange(state, state & EPOCH_MASK, Ordering::AcqRel, Ordering::Acquire);
        }
    }

    pub fn done(&self) -> bool { self.0.load(Ordering::Acquire) & PHASE == DONE }

    pub fn needs_reset(&self) -> bool {
        self.0.load(Ordering::Acquire) & (PHASE | STEP_MASK) != WAIT
    }
}
