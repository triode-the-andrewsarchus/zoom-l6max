//! Synthetic, QEMU-only inspection of a stock mix routine; never install.
use core::{arch::asm, ptr::{read_volatile as read, write_volatile as write}};
const OUT: usize = 0x83e07000;

// QEMU-only veneer for the stock file-mix callback. Its extra argument is
// supplied only to this probe's observer; ordinary callbacks keep r2 intact.
core::arch::global_asm!(r#"
    .syntax unified
    .thumb
    .section .text.offline_mix_veneer,"ax",%progbits
    .global offline_mix_veneer
    .thumb_func
offline_mix_veneer:
    ldrd r0, r1, [r5]
    str r6, [r1, #4]
    push {r2, r3, r4, lr}
    mrs r4, APSR
    ldr r3, .Loffline_observer
    cmp r12, r3
    bne .Lordinary
    mov r2, r9
    b .Lcall
.Lordinary:
    ldr r2, [sp]
.Lcall:
    msr APSR_nzcvq, r4
    ldr r3, [sp, #4]
    ldr r4, [sp, #8]
    blx r12
    ldr.w lr, [sp, #12]
    add sp, #16
    bx lr
    .align 2
.Loffline_observer: .word offline_observer
"#, options(raw));

#[unsafe(no_mangle)]
unsafe extern "C" fn offline_observer(_context: u32, descriptor: *const u32, offset: u32) {
    unsafe {
        let n = read(OUT as *const u32) as usize;
        if n < 2 && offset <= 64 && offset % 64 == 0 {
            write((OUT + 216 + n * 4) as *mut u32, offset);
            let block = read(descriptor.add(1)) as *mut f32;
            let processor = &mut *(0x80d00000 as *mut crate::routing::DualEfxProcessor);
            for frame in 0..64 {
                let edge = if frame == 0 { Some(0) } else if frame == 63 { Some(1) } else { None };
                if let Some(edge) = edge {
                    write((OUT + 8 + (n * 2 + edge) * 4) as *mut f32, read(block.add(frame)));
                }
                let mut sends = [0.0f32; 3];
                for slot in 0..12 {
                    let sample = (0x81570400 + slot * 960000 + (offset as usize + frame) * 4) as *const f32;
                    let sample = read(sample);
                    if let Some(edge) = edge {
                        write((OUT + 24 + (n * 24 + slot * 2 + edge) * 4) as *mut f32, sample);
                    }
                    let gain = read((0x20016170 + slot * 8) as *const f32);
                    let smooth = read((0x20016140 + slot * 4) as *const f32);
                    sends[slot % 3] += (sample * (1.0 / 2147483648.0) * gain) * smooth;
                }
                let (left, right) = processor.process_bus_sample(sends[0], sends[1], sends[2]);
                if let Some(edge) = edge {
                    for bus in 0..3 {
                        write((OUT + 224 + ((n * 2 + edge) * 3 + bus) * 4) as *mut f32, sends[bus]);
                    }
                    write((OUT + 272 + (n * 2 + edge) * 8) as *mut f32, left);
                    write((OUT + 276 + (n * 2 + edge) * 8) as *mut f32, right);
                }
                write(block.add(frame), left);
                write(block.add(frame + 64), right);
            }
        }
        write(OUT as *mut u32, (n+1) as u32);
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn ordinary_observer(_context: u32, _descriptor: *const u32, argument: u32) {
    unsafe {
        let count = read((OUT + 304) as *const u32) as usize;
        if count < 2 { write((OUT + 308 + count * 4) as *mut u32, argument); }
        write((OUT + 304) as *mut u32, (count + 1) as u32);
    }
}

pub unsafe fn run() {
    unsafe {
        let saved_mask: u32;
        asm!("mrs {}, PRIMASK", "cpsid i", out(reg) saved_mask, options(nostack));
        let old_context = read(0x202a6f18 as *const u32);
        let old_descriptor = read(0x202a6f1c as *const u32);
        let old_callback = read(0x202a6f20 as *const u32);
        let desc = 0x83e07300 as *mut u32;
        for i in 0..16 { write(desc.add(i), 0); }
        write(OUT as *mut u32, 0);
        write(0x20015110 as *mut u32, 0);
        write(0x20015114 as *mut u32, 0);
        write(0x20016138 as *mut f32, 0.0);
        write(0x2001613c as *mut f32, 1.0);
        for slot in 0usize..14 {
            for frame in 0..128 {
                let v = if slot < 12 { (slot as f32+1.0)*1000000.0 + frame as f32*1000.0 } else { 0.0 };
                write((0x81570400 + slot*960000 + frame*4) as *mut f32, v);
            }
            if slot < 12 {
                write((0x20015584 + slot*0x18) as *mut u32, 0);
                write((0x200156a4 + slot*0x28) as *mut u32, 0);
                write((0x20015884 + slot*0x18) as *mut u32, 0);
                write((0x20016140 + slot*4) as *mut f32, 1.0);
                write((0x20016170 + slot*8) as *mut f32, (slot as f32 + 1.0) / 16.0);
                write((0x20016174 + slot*8) as *mut f32, 1.0);
            }
        }
        write(0x202a6f18 as *mut u32, 0);
        write(0x202a6f1c as *mut u32, desc as u32);
        write(0x202a6f20 as *mut u32, offline_observer as *const () as u32);
        type Mix = unsafe extern "C" fn(u32,u32,u32,u32,u32,u32,u32,u32,u32,u32) -> u32;
        let mix: Mix = core::mem::transmute(0x80017501usize);
        mix(0x82241800,0x81570400,2,0,128,0,0,0,1,0x83e07380);
        write((OUT + 304) as *mut u32, 0);
        write(0x202a6f20 as *mut u32, ordinary_observer as *const () as u32);
        mix(0x82241800,0x81570400,2,0,128,0,0,0,1,0x83e07380);
        write(0x202a6f18 as *mut u32, old_context);
        write(0x202a6f1c as *mut u32, old_descriptor);
        write(0x202a6f20 as *mut u32, old_callback);
        write((OUT + 4) as *mut u32, 0x4f46464c);
        asm!("msr PRIMASK, {}", in(reg) saved_mask, options(nostack));
    }
}
