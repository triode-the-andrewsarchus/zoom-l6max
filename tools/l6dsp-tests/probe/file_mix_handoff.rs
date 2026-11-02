//! Forced file-mix handoff experiment. QEMU only: never install this image.
use core::ptr::{read_volatile as read, write_volatile as write};

pub unsafe fn queue_if_requested() {
    unsafe {
        let trigger = 0x81fff000 as *mut u32;
        if read(trigger) == 0x54524947 {
            write(trigger, 0);
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
        let mix: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = core::mem::transmute(0x800015b9usize);
        let result = mix(a, b, c, d);
        #[cfg(file_mix_bypass)]
        write(0x83e0b000 as *mut u32, 0);
        write(out.add(7), result);
        write(out.add(8), 0x444f4e45);
        result
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
