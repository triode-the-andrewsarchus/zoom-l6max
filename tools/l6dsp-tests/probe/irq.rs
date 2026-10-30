//! Disposable QEMU-only UART interrupt trace. No target ownership is established.
use core::ptr::{read_volatile as read, write_volatile as write};

const HOOK: *mut u32 = 0x3934 as *mut u32;
const VENEER: *mut u32 = 0xff000 as *mut u32;
const TRACE: *mut u32 = 0x83e00900 as *mut u32;
const PATCH: u32 = 0xfb64f0fb; // BL 0x000ff000 from 0x00003934.

core::arch::global_asm!(r#"
    .syntax unified
    .thumb
    .section .text.inspector_irq,"ax"
    .global inspector_irq_trampoline
    .thumb_func
inspector_irq_trampoline:
    push.w {r0, r1, r2, r3, r12, lr}
    mov r1, r3
    bl inspector_irq_byte
    pop.w {r0, r1, r2, r3, r12}
    and.w r4, r6, #0x12
    and.w r2, r3, #0x7f
    add.w lr, sp, #8
    pop {pc}
"#, options(raw));

unsafe extern "C" { fn inspector_irq_trampoline(); }

/// Install only in a disposable QEMU image; the low-memory veneer is unowned.
pub unsafe fn install() {
    unsafe {
        let mask: u32;
        core::arch::asm!("mrs {mask}, primask", "cpsid i", mask = out(reg) mask, options(nostack));
        let first = read(HOOK);
        let second = read(HOOK.add(1));
        let expected = first == 0x0412f006 && second == 0x027ff003;
        let installed = first == PATCH && second == 0x8000f3af;
        if expected || installed {
            for i in 0..69 { write(TRACE.add(i), 0); }
            write(VENEER, 0xf000f8df); // LDR.W PC, [PC, #0]
            write(VENEER.add(1), inspector_irq_trampoline as *const () as usize as u32 | 1);
            write(HOOK, PATCH);
            write(HOOK.add(1), 0x8000f3af); // NOP.W
            core::arch::asm!("dsb sy", "isb sy", options(nostack));
            write(TRACE, 0x49525154);
        } else {
            write(TRACE, 0x42414421);
        }
        if mask == 0 { core::arch::asm!("cpsie i", options(nostack)); }
    }
}

/// Trace only complete key frames on the observed panel port; no stock byte is consumed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inspector_irq_byte(port: u32, raw: u32) {
    if port != 0 { return; }
    unsafe {
        let byte = raw & 0xff;
        if byte & 0x80 != 0 {
            write(TRACE.add(1), if byte == 0x90 || byte == 0x91 { 1 } else { 0 });
            write(TRACE.add(2), byte & 1);
        } else {
            match read(TRACE.add(1)) {
                1 => { write(TRACE.add(3), byte); write(TRACE.add(1), 2); }
                2 => {
                    write(TRACE.add(1), 0);
                    if read(TRACE.add(3)) == 4 && byte == 6 {
                        let count = read(TRACE.add(4));
                        if count < 32 {
                            write(TRACE.add(5 + count as usize * 2), read(TRACE.add(2)));
                            write(TRACE.add(6 + count as usize * 2), read(0x8020ab4c as *const u32));
                            write(TRACE.add(4), count + 1);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
