//! Intrusive QEMU-only snapshot of one modeled mixer and effects block.
// The addresses are diagnostic scratch, not owned physical-device memory.

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
