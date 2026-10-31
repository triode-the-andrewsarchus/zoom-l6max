//! QEMU-only AINR handoff. Scratch control words and timing are unqualified.
use core::sync::atomic::{AtomicU32, Ordering};
#[cfg(all(hybrid_ainr_probe, not(compact_ainr_probe)))]
compile_error!("the hybrid diagnostic requires the channel-0 prefix");
#[cfg(all(hybrid_ainr_probe, boundary_ainr_probe))]
compile_error!("hybrid and boundary handoffs are separate diagnostics");
#[cfg(all(boundary_ainr_probe, not(compact_ainr_probe)))]
compile_error!("the boundary diagnostic requires the channel-0 prefix");
#[cfg(all(boundary_noise_probe, not(boundary_ainr_probe)))]
compile_error!("boundary noise requires the boundary diagnostic");

const POINTER: usize = 0x83e03100;
const STATUS: usize = 0x83e03104; // 0 waiting, 1 captured, 2 allocation failed, 3 boundary candidate
const START: usize = 0x83e03118;
const READY: usize = 0x83e00144;
const STATE_STRIDE: usize = 0x20000;
#[cfg(hybrid_ainr_probe)]
const PROGRESS: usize = 0x83e03130;
#[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
const CONTROL: usize = 0x83e03134;
#[cfg(boundary_ainr_probe)]
const PAUSE: usize = 0x83e03138;
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
        #[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
        if crate::handoff_protocol::CaptureProtocol(word(CONTROL)).cancel(true) {
            #[cfg(hybrid_ainr_probe)]
            word(PROGRESS).store(0, Ordering::Release);
            word(START).store(0, Ordering::Release);
        }
    }
}

/// Rearm after the selector enters the stock AINR path.
#[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
pub unsafe fn arm() {
    unsafe { crate::handoff_protocol::CaptureProtocol(word(CONTROL)).arm(); }
}

pub fn ready() -> bool {
    unsafe {
        let token = word(READY).load(Ordering::Acquire);
        #[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
        { crate::handoff_protocol::CaptureProtocol(word(CONTROL)).current_done(token) }
        #[cfg(not(any(hybrid_ainr_probe, boundary_ainr_probe)))]
        { token == 0x41494e52 }
    }
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

// Keep the optional diagnostic injection and pause separate from stock capture.
#[cfg(boundary_ainr_probe)]
#[inline(always)]
unsafe fn boundary_audio(input: *mut f32, output: *mut f32, index: u32) -> bool {
    unsafe {
        if crate::boundary_pause::BoundaryPause(word(PAUSE)).bypass(index) {
            word(0x83e03144 + index as usize * 4).fetch_add(1, Ordering::Relaxed);
            if input != output { core::ptr::copy(input, output, 64); }
            return true;
        }
        #[cfg(boundary_noise_probe)]
        if index == 0 {
            let marker = 0x83e03400 as *mut u32;
            let mut random = core::ptr::read_volatile(marker);
            if random == 0 { random = 7; }
            for i in 0..64 {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                let sample = (((random >> 16) as i32 - 32768) as f32) * (0.125 / 32768.0);
                core::ptr::write_volatile(input.add(i), sample);
            }
            core::ptr::write_volatile(marker, random);
        }
        false
    }
}

/// The guarded stock veneer calls here instead of directly calling `0x781`.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.ainr_capture")]
pub unsafe extern "C" fn ainr_capture(input: *mut f32, output: *mut f32, index: u32) -> u32 {
    unsafe {
        let stock: unsafe extern "C" fn(*mut f32, *mut f32, u32) -> u32 = core::mem::transmute(0x781usize);
        #[cfg(boundary_ainr_probe)]
        if boundary_audio(input, output, index) { return 0; }
        let result = stock(input, output, index);
        if index != 0 || core::ptr::read_volatile(0x202a6f20 as *const u32) != 0x20280021 {
            return result;
        }
        let status = word(STATUS);
        let learned = core::ptr::read_volatile(0x8046300c as *const u32) == 1;
        if !learned {
            let observed = status.load(Ordering::Acquire);
            if observed == 1 || (cfg!(boundary_ainr_probe) && observed == 3) { status.store(0, Ordering::Release); }
            word(START).store(0, Ordering::Release);
            #[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
            {
                let protocol = crate::handoff_protocol::CaptureProtocol(word(CONTROL));
                if protocol.needs_reset() {
                    protocol.cancel(false);
                    protocol.arm();
                    #[cfg(hybrid_ainr_probe)]
                    word(PROGRESS).store(0, Ordering::Release);
                }
            }
            return result;
        }
        let ptr = word(POINTER).load(Ordering::Acquire) as *mut u8;
        let observed_status = status.load(Ordering::Acquire);
        // Hybrid completion is governed by the versioned claim. A late
        // status write from an old capture must not block a rearmed claim.
        #[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
        let status_allows_capture = matches!(observed_status, 0 | 1);
        #[cfg(not(any(hybrid_ainr_probe, boundary_ainr_probe)))]
        let status_allows_capture = observed_status == 0;
        if ptr.is_null() || !status_allows_capture
            || core::ptr::read_volatile(0x2028ba4c as *const u32) != 1 { return result; }
        let now = core::ptr::read_volatile(0x8020ab4c as *const u32);
        let start = word(START).load(Ordering::Acquire);
        if start == 0 { word(START).store(now, Ordering::Release); return result; }
        // This observation delay is experimental; it is not a learning deadline.
        if now.wrapping_sub(start) < 10000 { return result; }
        #[cfg(boundary_ainr_probe)]
        {
            if !crate::handoff_protocol::CaptureProtocol(word(CONTROL)).done() {
                status.store(3, Ordering::Release);
            }
            return result;
        }
        #[cfg(hybrid_ainr_probe)]
        let complete = capture_hybrid(ptr, now);
        #[cfg(all(not(hybrid_ainr_probe), not(boundary_ainr_probe)))]
        let complete = {
            core::ptr::write_volatile(0x83e03108 as *mut u32, now);
            core::ptr::copy_nonoverlapping(0x20280000 as *const u8, ptr, CAPTURE_BYTES);
            true
        };
        #[cfg(not(boundary_ainr_probe))]
        if complete {
            core::ptr::write_volatile(0x83e0310c as *mut u32, core::ptr::read_volatile(0x8020ab4c as *const u32));
            status.store(1, Ordering::Release);
        }
        result
    }
}

#[cfg(boundary_ainr_probe)]
unsafe fn boundary_selection_valid(status: &AtomicU32, pause: &crate::boundary_pause::BoundaryPause<'_>, request: u32) -> bool {
    unsafe {
        pause.acknowledged(request) && status.load(Ordering::Acquire) == 3
            && core::ptr::read_volatile(0x8046300c as *const u32) == 1
            && core::ptr::read_volatile(0x202a6f20 as *const u32) == 0x20280021
            && core::ptr::read_volatile(0x2028ba4c as *const u32) == 1
    }
}

/// QEMU-only: pause the observed AINR calls at a boundary before snapshotting.
/// Other state users, the bypass ABI, timing and audible behavior are unproven.
#[cfg(boundary_ainr_probe)]
pub unsafe fn capture_boundary() {
    unsafe {
        let status = word(STATUS);
        let ptr = word(POINTER).load(Ordering::Acquire) as *mut u8;
        if ptr.is_null() || status.load(Ordering::Acquire) != 3 { return; }
        let capture = crate::handoff_protocol::CaptureProtocol(word(CONTROL));
        let Some((claim, step)) = capture.claim() else { return; };
        if step != 0 {
            capture.cancel(false);
            capture.finish(claim, 0, false);
            return;
        }
        let pause = crate::boundary_pause::BoundaryPause(word(PAUSE));
        let Some(request) = pause.request() else {
            capture.finish(claim, 0, false);
            return;
        };
        let delay: unsafe extern "C" fn(u32) = core::mem::transmute(0x8008e771usize);
        let mut waits = 0u32;
        while !pause.acknowledged(request) && waits < 20 {
            delay(1);
            waits += 1;
        }
        word(0x83e0313c).store(waits, Ordering::Release);
        let valid = boundary_selection_valid(status, &pause, request);
        let completed = if valid {
            core::ptr::write_volatile(0x83e03108 as *mut u32, core::ptr::read_volatile(0x8020ab4c as *const u32));
            core::ptr::copy_nonoverlapping(0x20280000 as *const u8, ptr, CAPTURE_BYTES);
            core::ptr::write_volatile(0x83e0310c as *mut u32, core::ptr::read_volatile(0x8020ab4c as *const u32));
            let still_valid = boundary_selection_valid(status, &pause, request);
            if still_valid { capture.finish(claim, 15, true) }
            else { capture.finish(claim, 0, false); false }
        } else {
            capture.finish(claim, 0, false);
            false
        };
        if completed {
            status.store(1, Ordering::Release);
            word(0x83e03140).store(1, Ordering::Release);
        } else {
            let _ = status.compare_exchange(3, 0, Ordering::AcqRel, Ordering::Acquire);
            word(START).store(0, Ordering::Release);
            word(0x83e03140).store(2, Ordering::Release);
        }
        pause.finish(request);
    }
}

// Experimental only: the copied ranges are inferred from limited QEMU runs.
#[cfg(hybrid_ainr_probe)]
unsafe fn capture_hybrid(ptr: *mut u8, now: u32) -> bool {
    unsafe {
        let protocol = crate::handoff_protocol::CaptureProtocol(word(CONTROL));
        let Some((claim, step)) = protocol.claim() else { return false; };
        if step >= 15 {
            protocol.cancel(false);
            protocol.finish(claim, 0, false);
            return false;
        }
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
        // Versioned handoffs publish DONE before STATUS; the claim, not the
        // later telemetry word, authorizes installation.
        #[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
        let Some(claim) = crate::handoff_protocol::CaptureProtocol(word(CONTROL)).claim_install() else { return; };
        #[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
        word(READY).store(1, Ordering::Release);
        #[cfg(not(any(hybrid_ainr_probe, boundary_ainr_probe)))]
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
        #[cfg(any(hybrid_ainr_probe, boundary_ainr_probe))]
        {
            let protocol = crate::handoff_protocol::CaptureProtocol(word(CONTROL));
            if protocol.finish(claim, 15, true) {
                // Restore the completed phase in the claim's generation token.
                let _ = word(READY).compare_exchange(1, (claim & !3) | 2, Ordering::AcqRel, Ordering::Acquire);
            }
        }
        #[cfg(not(any(hybrid_ainr_probe, boundary_ainr_probe)))]
        word(READY).store(0x41494e52, Ordering::Release);
    }
}
