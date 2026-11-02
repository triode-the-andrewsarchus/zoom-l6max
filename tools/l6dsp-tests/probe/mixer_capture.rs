//! Intrusive QEMU-only snapshot of one modeled mixer and effects block.
// The addresses are diagnostic scratch, not owned physical-device memory.

pub unsafe fn reset() {
    unsafe {
        core::ptr::write_volatile(0x83e00800 as *mut u32, 0);
        for i in 0..6 { core::ptr::write_volatile((0x83e00820 as *mut u32).add(i), 0); }
    }
}

#[inline(always)]
pub unsafe fn begin(sends: &[[f32; 64]; 3], counter: *const u32) -> bool {
    unsafe {
        if core::ptr::read_volatile(counter) < 100
            || !sends.iter().flatten().any(|x| *x > 1e-7 || *x < -1e-7)
            || core::ptr::read_volatile(0x83e00800 as *const u32) != 0
        {
            return false;
        }
        for i in 0..768 {
            core::ptr::write_volatile((0x83e02000 as *mut u32).add(i),
                core::ptr::read_volatile((0x2000e410 as *const u32).add(i)));
        }
        for i in 0..128 {
            core::ptr::write_volatile((0x83e02c00 as *mut u32).add(i),
                core::ptr::read_volatile((0x20016138 as *const u32).add(i)));
        }
        for bus in 0..3 {
            for i in 0..64 {
                core::ptr::write_volatile((0x83e01000usize + bus * 256 + i * 4) as *mut f32, sends[bus][i]);
            }
        }
        true
    }
}

#[inline(always)]
pub unsafe fn finish(buffer: *const f32, counter: *const u32) {
    unsafe {
        for i in 0..128 {
            core::ptr::write_volatile((0x83e01400 as *mut f32).add(i), core::ptr::read_volatile(buffer.add(i)));
        }
        core::ptr::write_volatile(0x83e00804 as *mut u32, core::ptr::read_volatile(counter));
        core::ptr::write_volatile(0x83e00800 as *mut u32, 0x5349474e);
    }
}

// Accumulate a longer signal-level observation than the one-shot snapshot.
// This diagnostic reads unowned scratch and adds work to every callback.
pub unsafe fn observe(sends: &[[f32; 64]; 3], buffer: *const f32) {
    unsafe {
        let stats = 0x83e00820 as *mut u32;
        let mut peak_in = f32::from_bits(core::ptr::read_volatile(stats.add(3)));
        let mut peak_l = f32::from_bits(core::ptr::read_volatile(stats.add(4)));
        let mut peak_r = f32::from_bits(core::ptr::read_volatile(stats.add(5)));
        let mut active = false;
        let mut invalid = 0u32;
        for i in 0..64 {
            for bus in sends {
                let sample = bus[i];
                if !sample.is_finite() { invalid += 1; }
                else { peak_in = peak_in.max(sample.abs()); active |= sample.abs() > 1e-7; }
            }
            for (sample, peak) in [(core::ptr::read_volatile(buffer.add(i)), &mut peak_l),
                                   (core::ptr::read_volatile(buffer.add(i + 64)), &mut peak_r)] {
                if !sample.is_finite() || sample.abs() > 1.0 { invalid += 1; }
                else { *peak = peak.max(sample.abs()); }
            }
        }
        core::ptr::write_volatile(stats, core::ptr::read_volatile(stats).saturating_add(1));
        core::ptr::write_volatile(stats.add(1), core::ptr::read_volatile(stats.add(1)).saturating_add(active as u32));
        core::ptr::write_volatile(stats.add(2), core::ptr::read_volatile(stats.add(2)).saturating_add(invalid));
        for (i, peak) in [peak_in, peak_l, peak_r].into_iter().enumerate() {
            core::ptr::write_volatile(stats.add(3 + i), peak.to_bits());
        }
    }
}
