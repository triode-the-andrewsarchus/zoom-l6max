//! QEMU-only AINR handoff. Scratch control words and timing are unqualified.
use core::sync::atomic::{AtomicU32, Ordering};
#[cfg(all(hybrid_ainr_probe, not(compact_ainr_probe)))]
compile_error!("the hybrid diagnostic requires the channel-0 prefix");

const POINTER: usize = 0x83e03100;
const STATUS: usize = 0x83e03104; // 0 waiting, 1 captured, 2 allocation failed
const START: usize = 0x83e03118;
const READY: usize = 0x83e00144;
const STATE_STRIDE: usize = 0x20000;
#[cfg(hybrid_ainr_probe)]
const PROGRESS: usize = 0x83e03130;
#[cfg(hybrid_ainr_probe)]
const CONTROL: usize = 0x83e03134;
#[cfg(compact_ainr_probe)]
const CAPTURE_BYTES: usize = 0x19514;
#[cfg(not(compact_ainr_probe))]
const CAPTURE_BYTES: usize = STATE_STRIDE;

unsafe fn word(address: usize) -> &'static AtomicU32 {
    unsafe { &*(address as *const AtomicU32) }
}

/// Called before the stock effect-buffer clear, while the callback is disabled.
pub unsafe fn disable() {
    unsafe {
        word(READY).store(0, Ordering::Release);
        #[cfg(hybrid_ainr_probe)]
        if crate::handoff_protocol::CaptureProtocol(word(CONTROL)).cancel(true) {
            word(PROGRESS).store(0, Ordering::Release);
            word(START).store(0, Ordering::Release);
        }
    }
}

/// Rearm after the selector enters the stock AINR path.
#[cfg(hybrid_ainr_probe)]
pub unsafe fn arm() {
    unsafe { crate::handoff_protocol::CaptureProtocol(word(CONTROL)).arm(); }
}

pub fn ready() -> bool {
    unsafe { word(READY).load(Ordering::Acquire) == 0x41494e52 }
}

/// Diagnostic only: retain one snapshot allocation for the test session.
pub unsafe fn allocate() {
    unsafe {
        let slot = word(POINTER);
        let status = word(STATUS);
        if slot.load(Ordering::Acquire) != 0 { return; }
        // A transient failure must not disable later learning attempts for
        // the rest of this diagnostic session. Unknown states stay closed.
        if !matches!(status.load(Ordering::Acquire), 0 | 2) { return; }
        let alloc: unsafe extern "C" fn(usize) -> *mut u8 = core::mem::transmute(0x80085479usize);
        let ptr = alloc(CAPTURE_BYTES) as usize;
        if ptr >= 0x808a4148 && ptr % 8 == 0
            && ptr.checked_add(CAPTURE_BYTES).is_some_and(|end| end <= 0x80921148) {
            status.store(0, Ordering::Release);
            slot.store(ptr as u32, Ordering::Release);
        } else {
            if ptr != 0 {
                let free: unsafe extern "C" fn(*mut u8) = core::mem::transmute(0x8008e589usize);
                free(ptr as *mut u8);
            }
            status.store(2, Ordering::Release);
        }
    }
}

/// The guarded stock veneer calls here instead of directly calling `0x781`.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.ainr_capture")]
pub unsafe extern "C" fn ainr_capture(input: *mut f32, output: *mut f32, index: u32) -> u32 {
    unsafe {
        let stock: unsafe extern "C" fn(*mut f32, *mut f32, u32) -> u32 = core::mem::transmute(0x781usize);
        let result = stock(input, output, index);
        if index != 0 || core::ptr::read_volatile(0x202a6f20 as *const u32) != 0x20280021 {
            return result;
        }
        let status = word(STATUS);
        let learned = core::ptr::read_volatile(0x8046300c as *const u32) == 1;
        if !learned {
            if status.load(Ordering::Acquire) == 1 { status.store(0, Ordering::Release); }
            word(START).store(0, Ordering::Release);
            #[cfg(hybrid_ainr_probe)]
            {
                let protocol = crate::handoff_protocol::CaptureProtocol(word(CONTROL));
                if protocol.needs_reset() {
                    protocol.cancel(false);
                    protocol.arm();
                    word(PROGRESS).store(0, Ordering::Release);
                }
            }
            return result;
        }
        let ptr = word(POINTER).load(Ordering::Acquire) as *mut u8;
        if ptr.is_null() || status.load(Ordering::Acquire) != 0
            || core::ptr::read_volatile(0x2028ba4c as *const u32) != 1 { return result; }
        let now = core::ptr::read_volatile(0x8020ab4c as *const u32);
        let start = word(START).load(Ordering::Acquire);
        if start == 0 { word(START).store(now, Ordering::Release); return result; }
        // This observation delay is experimental; it is not a learning deadline.
        if now.wrapping_sub(start) < 10000 { return result; }
        #[cfg(hybrid_ainr_probe)]
        let complete = capture_hybrid(ptr, now);
        #[cfg(not(hybrid_ainr_probe))]
        let complete = {
            core::ptr::write_volatile(0x83e03108 as *mut u32, now);
            core::ptr::copy_nonoverlapping(0x20280000 as *const u8, ptr, CAPTURE_BYTES);
            true
        };
        if complete {
            core::ptr::write_volatile(0x83e0310c as *mut u32, core::ptr::read_volatile(0x8020ab4c as *const u32));
            status.store(1, Ordering::Release);
        }
        result
    }
}

// Experimental only: the copied ranges are inferred from limited QEMU runs.
#[cfg(hybrid_ainr_probe)]
unsafe fn capture_hybrid(ptr: *mut u8, now: u32) -> bool {
    unsafe {
        let protocol = crate::handoff_protocol::CaptureProtocol(word(CONTROL));
        let Some((claim, step)) = protocol.claim() else { return false; };
        if step >= 15 { protocol.cancel(false); return false; }
        let offset = if step < 11 { step * 4096 } else { 0x16000 + (step - 11) * 4096 };
        if step == 0 { core::ptr::write_volatile(0x83e03108 as *mut u32, now); }
        let next = offset + core::cmp::min(4096, CAPTURE_BYTES - offset);
        core::ptr::copy_nonoverlapping((0x20280000usize + offset) as *const u8, ptr.add(offset), next - offset);
        let complete = next == CAPTURE_BYTES;
        if complete { core::ptr::copy_nonoverlapping(0x2028b000 as *const u8, ptr.add(0xb000), 0xb000); }
        if !protocol.finish(claim, step + 1, complete) { return false; }
        word(PROGRESS).store(next as u32, Ordering::Release);
        complete
    }
}

/// Called after effect initialization and before callback publication.
pub unsafe fn install() {
    unsafe {
        let ptr = word(POINTER).load(Ordering::Acquire) as *const u8;
        if ptr.is_null() { return; }
        // In the hybrid path, DONE is the release/acquire publication of the
        // snapshot. STATUS is written later and is not an installation gate.
        #[cfg(hybrid_ainr_probe)]
        if !crate::handoff_protocol::CaptureProtocol(word(CONTROL)).done() { return; }
        #[cfg(not(hybrid_ainr_probe))]
        if word(STATUS).load(Ordering::Acquire) != 1 { return; }
        for index in 0..3 {
            let dest = (0x80d50000usize + index * STATE_STRIDE) as *mut u8;
            core::ptr::copy_nonoverlapping(ptr, dest, CAPTURE_BYTES);
            #[cfg(compact_ainr_probe)]
            core::ptr::write_bytes(dest.add(CAPTURE_BYTES), 0, STATE_STRIDE - CAPTURE_BYTES);
        }
        core::ptr::write_volatile(0x83e03110 as *mut u32, core::ptr::read_volatile(ptr.add(0xba4c) as *const u32));
        core::ptr::write_volatile(0x83e03114 as *mut u32, core::ptr::read_volatile(0x80d5ba4c as *const u32));
        core::ptr::write_volatile(0x83e00140 as *mut u32, 1);
        word(READY).store(0x41494e52, Ordering::Release);
    }
}
