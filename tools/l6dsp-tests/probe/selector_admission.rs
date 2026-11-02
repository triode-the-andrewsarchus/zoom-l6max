//! Selector admission experiment; unowned QEMU scratch, never install.
use core::ptr::{read_volatile as read, write_volatile as write};
use crate::admission::{CallbackAdmission, CloseGuard};

const GATE: usize = 0x83e0b000;
const COUNTS: usize = 0x83e0b010;
const GUARD: usize = 0x83e0b020;
const ACTIVE: usize = 0x83e0b024;

unsafe fn increment(index: usize) {
    unsafe {
        let value = (COUNTS as *mut u32).add(index);
        write(value, read(value).wrapping_add(1));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn selector_handoff_begin() -> u32 {
    unsafe {
        increment(0);
        if !crate::file_mix_handoff::ensure_gate() {
            increment(2);
            return 0;
        }
        let gate: &'static CallbackAdmission = &*(GATE as *const CallbackAdmission);
        #[cfg(selector_overlap)]
        let overlap = read(0x81fff004 as *const u32) == 0x4f564c50;
        #[cfg(selector_overlap)]
        if overlap {
            write(0x81fff004 as *mut u32, 0);
            write(0x83e0b100 as *mut u32, 1);
            let give: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = core::mem::transmute(0x80090ad1usize);
            let delay: unsafe extern "C" fn(u32) = core::mem::transmute(0x8008e771usize);
            write(0x83e0b104 as *mut u32, give(read(0x20007cf8 as *const u32), 0, 0, 0));
            delay(1);
            write(0x83e0b108 as *mut u32, read(0x83e0b100 as *const u32));
        }
        let Some(guard) = gate.try_close() else {
            increment(2);
            return 0;
        };
        #[cfg(selector_overlap)]
        if overlap {
            use core::sync::atomic::{AtomicU32, Ordering};
            write(0x83e0b10c as *mut u32, (&*(GATE as *const AtomicU32)).load(Ordering::Acquire));
        }
        let delay: unsafe extern "C" fn(u32) = core::mem::transmute(0x8008e771usize);
        for _ in 0..20 {
            if guard.is_drained() {
                #[cfg(selector_overlap)]
                if overlap { write(0x83e0b114 as *mut u32, 4); }
                (GUARD as *mut CloseGuard<'static>).write(guard);
                write(ACTIVE as *mut u32, 1);
                increment(1);
                return 1;
            }
            #[cfg(selector_overlap)]
            if overlap {
                let waits = 0x83e0b110 as *mut u32;
                write(waits, read(waits).wrapping_add(1));
            }
            delay(1);
        }
        drop(guard);
        increment(2);
        2
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn selector_handoff_end() {
    unsafe {
        if read(ACTIVE as *const u32) == 1 {
            write(ACTIVE as *mut u32, 0);
            core::ptr::drop_in_place(GUARD as *mut CloseGuard<'static>);
            increment(3);
        }
    }
}

// These veneers reproduce the overwritten L6max 1.10 instructions. Acquire
// before the stock call, which resets control structures. Failure skips that
// call and the requested selection; the timeout path only attempts fade and
// flag cleanup. These paths are diagnostic only.
core::arch::global_asm!(r#"
.syntax unified
.thumb
.section .text.selector_handoff,"ax",%progbits
.global selector_handoff_entry
.thumb_func
selector_handoff_entry:
    push.w {r0, r1, r2, r3, r12, lr}
    bl selector_handoff_begin
    cmp r0, #0
    beq .Labort
    cmp r0, #2
    beq .Ltimeout
    pop.w {r0, r1, r2, r3, r12, lr}
    mov r1, r4
    ldr.w r12, .Lstock_call
    blx r12
    cmp r6, #4
    ldr.w pc, .Lcontinue
.Ltimeout:
    pop.w {r0, r1, r2, r3, r12, lr}
    movs r0, #0
    ldr.w r12, .Lfade_argument
    blx r12
    ldr.w r12, .Lfade_call
    blx r12
    cmp.w r10, #0
    bne .Ltimeout_cleanup
    ldr.w r12, .Lflag_clear
    blx r12
.Ltimeout_cleanup:
    ldr.w r0, [r9]
    movs r1, #0
    movs r2, #0
    movs r3, #0
    ldr.w pc, .Lexit_continue
.Labort:
    pop.w {r0, r1, r2, r3, r12, lr}
    ldr.w r0, [r9]
    movs r1, #0
    movs r2, #0
    movs r3, #0
    ldr.w pc, .Lexit_continue
.global selector_handoff_fade
.thumb_func
selector_handoff_fade:
    push.w {r0, r1, r2, r3, r12, lr}
    bl selector_handoff_end
    pop.w {r0, r1, r2, r3, r12, lr}
    ldr.w r12, .Lfade_argument
    blx r12
    ldr.w r12, .Lfade_call
    blx r12
    ldr.w pc, .Lfade_continue
.global selector_handoff_exit
.thumb_func
selector_handoff_exit:
    push.w {r0, r1, r2, r3, r12, lr}
    bl selector_handoff_end
    pop.w {r0, r1, r2, r3, r12, lr}
    ldr.w r0, [r9]
    movs r1, #0
    movs r2, #0
    movs r3, #0
    ldr.w pc, .Lexit_continue
.align 2
.Lstock_call: .word 0x20282f6d
.Lcontinue: .word 0x202805d5
.Lexit_continue: .word 0x20280709
.Lfade_argument: .word 0x80049a21
.Lfade_call: .word 0x8002c011
.Lflag_clear: .word 0x8002e749
.Lfade_continue: .word 0x202806f5
"#, options(raw));
