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
        #[cfg(selector_feedback)]
        write(0x83e0b028 as *mut u32, 1);
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
#[cfg(selector_feedback)]
core::arch::global_asm!(".equ selector_feedback_enabled, 1");
#[cfg(not(selector_feedback))]
core::arch::global_asm!(".equ selector_feedback_enabled, 0");
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
    .if selector_feedback_enabled
    ldr.w r12, .Ltimeout_old_type
    ldr.w r12, [r12]
    cmp.w r12, #5
    beq .Ltimeout_flag
    .endif
    movs r0, #0
    ldr.w r12, .Lfade_argument
    blx r12
    ldr.w r12, .Lfade_call
    blx r12
.Ltimeout_flag:
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
.Ltimeout_old_type: .word 0x83e0b02c
.Lfade_continue: .word 0x202806f5
"#, options(raw));

// QEMU-only panel handler experiment. The original handler's stack argument
// is at caller SP+4; only the forced timeout is reported through scratch.
#[cfg(selector_feedback)]
core::arch::global_asm!(r#"
.syntax unified
.thumb
.section .text.selector_handoff_feedback,"ax",%progbits
.global selector_handoff_feedback
.thumb_func
selector_handoff_feedback:
    push {r4, r6, r7, lr}
    push {r0, r1, r2, r3}
    ldr.w r4, .Lfeedback_status
    movs r2, #0
    str r2, [r4]
    movs r2, #1
    str r2, [r4, #8]
    movs r0, #0
    ldr.w r12, .Lfeedback_get_active_type
    blx r12
    str r0, [r4, #4]
    pop {r0, r1, r2, r3}
    ldr.w r12, .Lfeedback_select
    blx r12
    ldr r2, [r4]
    movs r3, #0
    str r3, [r4]
    str r3, [r4, #4]
    str r3, [r4, #8]
    cmp r2, #1
    bne .Lfeedback_send
    movs r0, #0
    ldr.w r12, .Lfeedback_get_type
    blx r12
    str r0, [sp, #20]
.Lfeedback_send:
    cmp r5, #0
    bne .Lfeedback_done
    ldr r0, [sp, #20]
    uxtb r0, r0
    ldr.w r12, .Lfeedback_midi
    blx r12
.Lfeedback_done:
    pop {r4, r6, r7, pc}
.align 2
.Lfeedback_status: .word 0x83e0b028
.Lfeedback_select: .word 0x80054341
.Lfeedback_get_type: .word 0x8000b271
.Lfeedback_get_active_type: .word 0x8000b379
.Lfeedback_midi: .word 0x8000ccf9

.global selector_handoff_feedback_pre
.thumb_func
selector_handoff_feedback_pre:
    push {r1, r2}
    ldr.w r1, .Lfeedback_status
    ldr r1, [r1, #8]
    cmp r1, #1
    pop {r1, r2}
    beq .Lpre_skip
    cmp r0, #5
    beq .Lpre_disable
.Lpre_skip:
    ldr.w pc, .Lpre_continue
.Lpre_disable:
    ldr.w pc, .Lpre_stock
.align 2
.Lpre_continue: .word 0x80054381
.Lpre_stock: .word 0x8005435d

.global selector_handoff_feedback_inner
.thumb_func
selector_handoff_feedback_inner:
    push.w {r0, r1, r2, r3, r12, lr}
    ldr.w r0, .Lfeedback_status
    ldr r1, [r0, #8]
    cmp r1, #1
    bne .Linner_compare
    ldr r0, [r0]
    cmp r0, #1
    beq .Linner_abort
    ldr.w r0, .Lfeedback_status
    ldr r0, [r0, #4]
    cmp r0, #5
    bne .Linner_compare
    ldr.w r12, .Ldisable_status
    blx r12
    movs r0, #1
    movs r1, #1
    ldr.w r12, .Ldisable_mute
    blx r12
    ldr.w r12, .Ldisable_delay
    blx r12
    movs r0, #0
    ldr.w r12, .Ldisable_route
    blx r12
    movs r0, #0
    ldr.w r12, .Ldisable_ainr
    blx r12
    movs r0, #0
    movs r1, #1
    ldr.w r12, .Ldisable_mute
    blx r12
.Linner_compare:
    pop.w {r0, r1, r2, r3, r12, lr}
    cmp r4, #5
    beq .Linner_ainr
.Linner_done:
    ldr.w pc, .Linner_return
.Linner_abort:
    pop.w {r0, r1, r2, r3, r12, lr}
    b .Linner_done
.Linner_ainr:
    ldr.w pc, .Linner_enable
.align 2
.Linner_return: .word 0x800543d3
.Linner_enable: .word 0x800543ab
.Ldisable_status: .word 0x8000b2e1
.Ldisable_mute: .word 0x80003fc9
.Ldisable_delay: .word 0x80004ca1
.Ldisable_route: .word 0x80003bb1
.Ldisable_ainr: .word 0x80003f99
"#, options(raw));
