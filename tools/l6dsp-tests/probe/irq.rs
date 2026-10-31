//! Disposable QEMU-only UART interrupt trace. No target ownership is established.
use core::ptr::{read_volatile as read, write_volatile as write};
#[cfg(irq_timing)]
use core::sync::atomic::{AtomicU32, Ordering};

const HOOK: *mut u32 = 0x3934 as *mut u32;
const VENEER: *mut u32 = 0xff000 as *mut u32;
const TRACE: *mut u32 = 0x83e00900 as *mut u32;
const DECODER: *mut crate::controls::PanelEfxDecoder = 0x83e00e00 as *mut _;
#[cfg(irq_timing)]
const QUEUE: usize = 0x83e00b00;
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
        // Re-selection must not discard edges already queued by this hook.
        // An installed hook is accepted only if its veneer still matches.
        if expected {
            for i in 0..69 { write(TRACE.add(i), 0); }
            core::ptr::write(DECODER, crate::controls::PanelEfxDecoder::default());
            #[cfg(ring_timing)]
            crate::ring_timing::initialize();
            #[cfg(irq_timing)]
            {
                core::ptr::write(QUEUE as *mut AtomicU32, AtomicU32::new(0));
                core::ptr::write((QUEUE + 4) as *mut AtomicU32, AtomicU32::new(0));
                core::ptr::write((QUEUE + 8) as *mut AtomicU32, AtomicU32::new(0));
            }
            write(VENEER, 0xf000f8df); // LDR.W PC, [PC, #0]
            write(VENEER.add(1), inspector_irq_trampoline as *const () as usize as u32 | 1);
            write(HOOK, PATCH);
            write(HOOK.add(1), 0x8000f3af); // NOP.W
            core::arch::asm!("dsb sy", "isb sy", options(nostack));
            write(TRACE, 0x49525154);
        } else if !installed || read(VENEER) != 0xf000f8df
            || read(VENEER.add(1)) != (inspector_irq_trampoline as *const () as usize as u32 | 1) {
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
        #[cfg(ring_timing)]
        crate::ring_timing::observe_byte();
        if let Some(pressed) = (*DECODER).feed(raw as u8) {
            let tick = read(0x8020ab4c as *const u32);
            #[cfg(ring_timing)]
            crate::ring_timing::observe_edge(pressed, tick);
            let count = read(TRACE.add(4));
            if count < 32 {
                write(TRACE.add(5 + count as usize * 2), pressed as u32);
                write(TRACE.add(6 + count as usize * 2), tick);
                write(TRACE.add(4), count + 1);
            }
            #[cfg(irq_timing)]
            {
                let head = (&*(QUEUE as *const AtomicU32)).load(Ordering::Relaxed);
                let tail = (&*((QUEUE + 4) as *const AtomicU32)).load(Ordering::Acquire);
                if head.wrapping_sub(tail) < 64 {
                    let entry = (QUEUE + 16 + (head % 64) as usize * 8) as *mut u32;
                    write(entry, pressed as u32);
                    write(entry.add(1), tick);
                    (&*(QUEUE as *const AtomicU32)).store(head.wrapping_add(1), Ordering::Release);
                } else {
                    (&*((QUEUE + 8) as *const AtomicU32)).store(1, Ordering::Release);
                }
            }
        }
    }
}

/// Match by polarity only. An empty queue or visible mismatch cancels the
/// gesture, but an entire lost press/release pair leaves matching stale edges.
/// This diagnostic cannot establish event identity or safe gesture timing.
#[cfg(irq_timing)]
pub unsafe fn take(pressed: bool) -> Option<u32> {
    unsafe {
        if read(TRACE) != 0x49525154 || (&*((QUEUE + 8) as *const AtomicU32)).load(Ordering::Acquire) != 0 {
            return None;
        }
        let head = (&*(QUEUE as *const AtomicU32)).load(Ordering::Acquire);
        let tail = (&*((QUEUE + 4) as *const AtomicU32)).load(Ordering::Relaxed);
        if head == tail { return None; }
        let entry = (QUEUE + 16 + (tail % 64) as usize * 8) as *const u32;
        if read(entry) != pressed as u32 {
            (&*((QUEUE + 4) as *const AtomicU32)).store(head, Ordering::Release);
            return None;
        }
        let tick = read(entry.add(1));
        (&*((QUEUE + 4) as *const AtomicU32)).store(tail.wrapping_add(1), Ordering::Release);
        Some(tick)
    }
}
