//! QEMU-only experiment: route file-mix track chunks to the replacement DSP.
use core::ptr::{read_volatile as read, write_volatile as write};

core::arch::global_asm!(
    r#"
    .syntax unified
    .thumb
    .section .text.offline_mix_veneer,"ax",%progbits
    .global file_mix_effect_veneer
    .thumb_func
file_mix_effect_veneer:
    ldrd r0, r1, [r5]
    str r6, [r1, #4]
    push {r2, r3, r4, lr}
    mrs r4, APSR
    ldr r3, .Ldiagnostic
    cmp r12, r3
    bne .Lordinary
    mov r2, r9
    ldr r12, .Lreplacement
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
.Ldiagnostic: .word diagnostic_audio
.Lreplacement: .word file_mix_effect_audio
"#,
    options(raw)
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_mix_effect_audio(_context: u32, descriptor: *const u32, offset: u32) {
    unsafe {
        let count = 0x83e0a060 as *mut u32;
        write(count, read(count).wrapping_add(1));
        write(count.add(1), read(count.add(1)).max(offset));
        if offset > 239936 {
            write(count.add(2), read(count.add(2)).wrapping_add(1));
        }
        if offset == 0 {
            write(count.add(3), read(count.add(3)).wrapping_add(1));
        }
        write(count.add(4), offset);
        if offset % 64 != 0 {
            write(count.add(5), read(count.add(5)).wrapping_add(1));
        }
        let buffer = read(descriptor.add(1)) as *mut f32;
        // Never read beyond a 240000-frame file-mix channel in this probe.
        if offset > 239936 {
            for i in 0..128 {
                write(buffer.add(i), 0.0);
            }
            return;
        }
        let control_link = read(descriptor.add(7)) as *const u32;
        let control_target = read(control_link) as *mut u32;
        let control_source = read(descriptor.add(8)) as *const u32;
        let mut sends = [[0.0f32; 64]; 3];
        for slot in 0..12usize {
            let route = read((0x83e00100 as *const u32).add(slot));
            if !(1..=3).contains(&route) {
                continue;
            }
            let gain = read((0x20016170usize + slot * 8) as *const f32);
            let smoothed = read((0x20016140usize + slot * 4) as *const f32);
            let src = (0x81570400usize + slot * 960000 + offset as usize * 4) as *const f32;
            for i in 0..64 {
                let sample = read(src.add(i)) * (1.0 / 2147483648.0);
                sends[(route - 1) as usize][i] +=
                    crate::math::audio_input((sample * gain) * smoothed);
            }
        }
        let captured = 0x83e0c000 as *mut u32;
        if read(captured) == 0 {
            let mut active = false;
            for i in 0..64 {
                active |= read(buffer.add(i)) != 0.0;
            }
            if active {
                for i in 0..64 {
                    write(captured.add(4 + i), read(buffer.add(i)).to_bits());
                    for bus in 0..3 {
                        write(captured.add(68 + bus * 64 + i), sends[bus][i].to_bits());
                    }
                }
                write(captured, 0x43415054);
            }
        }
        let state = &mut *(0x80d00000 as *mut crate::routing::DualEfxProcessor);
        for i in 0..64 {
            write(control_target, read(control_source.add(i)));
            let (l, r) = state.process_bus_sample(sends[0][i], sends[1][i], sends[2][i]);
            write(buffer.add(i), l);
            write(buffer.add(i + 64), r);
        }
    }
}
