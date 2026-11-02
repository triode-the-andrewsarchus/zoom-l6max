//! QEMU-only experiment: route file-mix track chunks to the replacement DSP.
use core::ptr::{read_volatile as read, write_volatile as write};

// Synthetic QEMU-only request. Do not overwrite an already-pending copy.
#[cfg(file_mix_project_deferred_copy)]
#[inline(always)]
unsafe fn stage_deferred_copy() {
    unsafe {
        let observation = 0x83e0b900 as *mut u32;
        let target = 0x83e0b940 as *mut u32;
        let destination = 0x80464fc8 as *mut u32;
        let count = 0x80464fcc as *mut u32;
        write(observation.add(1), read(count));
        if read(count) == 0 && read(destination) == 0 {
            write(target, 0xa55aa55a);
            write(0x80464e20 as *mut u32, 0x5a3cc35a);
            write(destination, target as u32);
            write(count, 1);
            write(observation, 1);
        } else {
            write(observation, 2);
        }
    }
}

// Interrupt one file-mix callback with a forced audio wake. Both guard
// variants remain QEMU-only diagnostics.
#[cfg(file_mix_project_pending)]
#[inline(always)]
unsafe fn force_pending_wake() {
    use core::sync::atomic::{AtomicU32, Ordering};
    unsafe {
        // This generated card yielded exactly 6,144 callbacks in the earlier
        // experiment. The checker rejects a run with a different total.
        #[cfg(file_mix_project_deferred_last)]
        if read(0x83e0a060 as *const u32) != 6144 { return; }
        let pending = 0x83e0b800 as *mut u32;
        if read(pending) != 0 { return; }
        write(pending, 1);
        #[cfg(file_mix_project_deferred_copy)]
        stage_deferred_copy();
        let gate = &*(0x83e0b000 as *const AtomicU32);
        let rejected = &*(0x83e0b004 as *const AtomicU32);
        write(pending.add(1), gate.load(Ordering::Acquire));
        write(pending.add(2), rejected.load(Ordering::Relaxed));
        let give: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = core::mem::transmute(0x80090ad1usize);
        let semaphore = read(0x20007cf8 as *const u32);
        write(pending.add(3), give(semaphore, 0, 0, 0));
        let delay: unsafe extern "C" fn(u32) = core::mem::transmute(0x8008e771usize);
        delay(5);
        #[cfg(file_mix_project_deferred_copy)]
        {
            let observation = 0x83e0b900 as *mut u32;
            write(observation.add(2), read(0x80464fcc as *const u32));
            write(observation.add(3), read(0x83e0b940 as *const u32));
        }
        write(pending.add(4), rejected.load(Ordering::Relaxed));
        write(pending.add(5), gate.load(Ordering::Acquire));
        write(pending, 2);
    }
}

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
        #[cfg(file_mix_project_pending)]
        force_pending_wake();
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
        // Accumulate output observations once per block, not per volatile
        // sample. This instrumentation perturbs timing and is QEMU-only.
        let stats = 0x83e0a078 as *mut u32;
        let mut nonzero = read(stats);
        let mut nonfinite = read(stats.add(1));
        let mut peaks = [
            f32::from_bits(read(stats.add(2))),
            f32::from_bits(read(stats.add(3))),
        ];
        for i in 0..64 {
            write(control_target, read(control_source.add(i)));
            let (l, r) = state.process_bus_sample(sends[0][i], sends[1][i], sends[2][i]);
            for (channel, value) in [l, r].into_iter().enumerate() {
                if !value.is_finite() {
                    nonfinite = nonfinite.saturating_add(1);
                } else {
                    nonzero = nonzero.saturating_add((value != 0.0) as u32);
                    peaks[channel] = peaks[channel].max(value.abs());
                }
            }
            write(buffer.add(i), l);
            write(buffer.add(i + 64), r);
        }
        write(stats, nonzero);
        write(stats.add(1), nonfinite);
        write(stats.add(2), peaks[0].to_bits());
        write(stats.add(3), peaks[1].to_bits());
    }
}
