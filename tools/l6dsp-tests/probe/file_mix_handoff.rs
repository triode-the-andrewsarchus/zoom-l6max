//! Forced file-mix handoff experiment. QEMU only: never install this image.
use core::ptr::{read_volatile as read, write_volatile as write};
#[cfg(file_mix_atomic)]
use core::sync::atomic::{AtomicU32, Ordering};
#[cfg(file_mix_atomic)]
use crate::admission::{CallbackAdmission, CallbackPath};

#[cfg(file_mix_atomic)]
const GATE: usize = 0x83e0b000;
#[cfg(file_mix_atomic)]
const READY: usize = 0x83e0b008;
#[cfg(file_mix_atomic)]
const READY_MAGIC: u32 = 0x47415445;
#[cfg(file_mix_atomic)]
const INITIALIZING: u32 = 0x494e4954;

#[cfg(file_mix_atomic)]
pub unsafe fn ensure_gate() -> bool {
    unsafe {
        // Only disposable, initially zeroed emulator scratch is assumed.
        // A competing initializer or an unexpected value fails closed.
        let ready = &*(READY as *const AtomicU32);
        if ready.load(Ordering::Acquire) == READY_MAGIC {
            return true;
        }
        match ready.compare_exchange(0, INITIALIZING, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                (GATE as *mut CallbackAdmission).write(CallbackAdmission::new());
                ready.store(READY_MAGIC, Ordering::Release);
                true
            }
            Err(READY_MAGIC) => true,
            Err(_) => false,
        }
    }
}

pub unsafe fn queue_if_requested() {
    unsafe {
        let trigger = 0x81fff000 as *mut u32;
        if read(trigger) == 0x54524947 {
            write(trigger, 0);
            #[cfg(file_mix_atomic)]
            if !ensure_gate() { return; }
            let enqueue: unsafe extern "C" fn() = core::mem::transmute(0x800475b9usize);
            enqueue();
        }
    }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.mix_handoff_observer")]
pub unsafe extern "C" fn mix_handoff_observer(a: u32, b: u32, c: u32, d: u32) -> u32 {
    unsafe {
        let out = 0x83e0a000 as *mut u32;
        write(out, 0x4d495831);
        write(out.add(1), read(0x83e00008 as *const u32));
        write(out.add(2), read(0x40384014 as *const u32));
        write(out.add(3), read(0x40384094 as *const u32));
        write(out.add(4), read(0x40384008 as *const u32));
        write(out.add(5), read(0x40384088 as *const u32));
        #[cfg(file_mix_atomic)]
        if !ensure_gate() { return 9; }
        #[cfg(file_mix_atomic)]
        let lease = match (&*(GATE as *const CallbackAdmission)).try_enter(CallbackPath::FileMix) {
            Some(lease) => lease,
            None => return 9, // Diagnostic failure, not a production fallback.
        };
        #[cfg(file_mix_atomic)]
        write(0x83e0b004 as *mut u32, 0);
        #[cfg(file_mix_bypass)]
        {
            write(0x83e0b004 as *mut u32, 0);
            write(0x83e0b000 as *mut u32, 1);
        }
        let give: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = core::mem::transmute(0x80090ad1usize);
        let semaphore = read(0x20007cf8 as *const u32);
        write(out.add(9), give(semaphore, 0, 0, 0));
        let delay: unsafe extern "C" fn(u32) = core::mem::transmute(0x8008e771usize);
        delay(50);
        write(out.add(6), read(0x83e00008 as *const u32));
        #[cfg(file_mix_bypass)]
        write(out.add(10), read(0x83e0b004 as *const u32));
        #[cfg(file_mix_atomic)]
        write(out.add(10), read(0x83e0b004 as *const u32));
        let mix: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = core::mem::transmute(0x800015b9usize);
        let result = mix(a, b, c, d);
        #[cfg(file_mix_atomic)]
        drop(lease);
        #[cfg(file_mix_bypass)]
        write(0x83e0b000 as *mut u32, 0);
        write(out.add(7), result);
        write(out.add(8), 0x444f4e45);
        result
    }
}

#[cfg(file_mix_atomic)]
unsafe extern "C" {
    fn mix_handoff_original();
    fn mix_handoff_silence();
}

// Own the lease across the complete stock wrapper, including deferred copies
// before its callback load and shared-scratch accesses after its return.
#[cfg(file_mix_atomic)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mix_handoff_atomic_live() {
    unsafe {
        let lease = if ensure_gate() {
            (&*(GATE as *const CallbackAdmission)).try_enter(CallbackPath::Live)
        } else {
            None
        };
        if let Some(_lease) = lease {
            #[cfg(selector_overlap)]
            if read(0x83e0b100 as *const u32) == 1 {
                write(0x83e0b100 as *mut u32, 2);
                let delay: unsafe extern "C" fn(u32) = core::mem::transmute(0x8008e771usize);
                #[cfg(selector_timeout)]
                delay(40);
                #[cfg(not(selector_timeout))]
                delay(8);
            }
            mix_handoff_original();
        } else {
            let counter = 0x83e0b004 as *mut u32;
            write(counter, read(counter).wrapping_add(1));
            // Experimental only: test whether a stock ramp can advance while
            // effect processing is excluded. Memory ownership is unproven.
            #[cfg(selector_admission)]
            if read(0x83e0b024 as *const u32) == 1 {
                let ramp: unsafe extern "C" fn() = core::mem::transmute(0x80066181usize);
                ramp();
                // Selected stock callbacks propagate the float ramp to this
                // indirect destination. This models only its last sample in
                // disposable QEMU, not the intervening per-frame behavior.
                let float_target = read(0x8020a3ec as *const u32) as *mut u32;
                write(float_target, read(0x8046310c as *const u32));
            }
            mix_handoff_silence();
        }
    }
}

// QEMU-only veneer for the two observed in-place stock AINR calls. Returning
// without processing leaves those buffers unchanged; other callers and the
// return-value contract remain unqualified for real firmware.
#[cfg(ainr_admission)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.ainr_admission")]
pub unsafe extern "C" fn ainr_admission(input: *mut f32, output: *mut f32, index: u32) -> u32 {
    unsafe {
        let counters = 0x83e0b200 as *const AtomicU32;
        let valid = input == output && index < 2;
        if !valid { (&*counters.add(6)).fetch_add(1, Ordering::Relaxed); }
        let lease = if ensure_gate() {
            (&*(GATE as *const CallbackAdmission)).try_enter(CallbackPath::Live)
        } else {
            None
        };
        if let Some(_lease) = lease {
            if valid { (&*counters.add(index as usize)).fetch_add(1, Ordering::Relaxed); }
            let stock: unsafe extern "C" fn(*mut f32, *mut f32, u32) -> u32 = core::mem::transmute(0x781usize);
            stock(input, output, index)
        } else {
            if valid {
                (&*counters.add(2 + index as usize)).fetch_add(1, Ordering::Relaxed);
                if read(0x83e0b030 as *const u32) == 1 && read(0x83e0b02c as *const u32) == 5
                    && read(0x2028ba4c as *const u32) == 1 && read(0x2028bad0 as *const u32) == 1 {
                    (&*counters.add(4 + index as usize)).fetch_add(1, Ordering::Relaxed);
                }
            }
            0
        }
    }
}

// Entry bypass precedes deferred stock copies as well as the callback load.
// The flag and counter are unowned, volatile QEMU scratch; this is not a lock.
#[cfg(file_mix_bypass)]
core::arch::global_asm!(r#"
.syntax unified
.thumb
.section .text.mix_handoff_bypass,"ax",%progbits
.global mix_handoff_bypass
.thumb_func
mix_handoff_bypass:
    push {r4, r5, r6, r7, lr}
    sub sp, #4
    push {r0, r1}
    ldr r0, .Lgate
    ldr r1, [r0]
    cmp r1, #0
    beq .Lgo
    ldr r1, [r0, #4]
    adds r1, #1
    str r1, [r0, #4]
    pop {r0, r1}
    ldr.w pc, .Lnull
.Lgo:
    pop {r0, r1}
    ldr.w r12, .Lprefix
    blx r12
    ldr.w pc, .Lresume
.align 2
.Lgate: .word 0x83e0b000
.Lnull: .word 0x20280331
.Lprefix: .word 0x8002bb19
.Lresume: .word 0x202800f1
"#, options(raw));

#[cfg(file_mix_atomic)]
core::arch::global_asm!(r#"
.syntax unified
.thumb
.section .text.mix_handoff_atomic,"ax",%progbits
.global mix_handoff_bypass
.thumb_func
mix_handoff_bypass:
    b.w mix_handoff_atomic_live
.global mix_handoff_original
.thumb_func
mix_handoff_original:
    push {r4, r5, r6, r7, lr}
    sub sp, #4
    ldr.w r12, .Latomic_prefix
    blx r12
    ldr.w pc, .Latomic_resume
.global mix_handoff_silence
.thumb_func
mix_handoff_silence:
    push {r4, r5, r6, r7, lr}
    sub sp, #4
    ldr.w pc, .Latomic_null
.align 2
.Latomic_prefix: .word 0x8002bb19
.Latomic_resume: .word 0x202800f1
.Latomic_null: .word 0x20280331
"#, options(raw));
