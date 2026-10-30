// QEMU-only diagnostic payload. This is not safe firmware and must never be installed.
#![no_std]
#![no_main]
#![allow(dead_code)]
#[path = "../../../patches/dual-efx/src/math.rs"]
mod math;
#[path = "../../../patches/dual-efx/src/plateau.rs"]
mod plateau;
#[path = "../../../patches/dual-efx/src/routing.rs"]
mod routing;
#[path = "../../../patches/dual-efx/src/velvet.rs"]
mod velvet;
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.probe")]
pub unsafe extern "C" fn probe(entry: *mut u32) {
    let clear: unsafe extern "C" fn() = unsafe { core::mem::transmute(0x80024679usize) };
    unsafe { clear() };
    if entry as usize == 0x2028b8dc || entry as usize == 0x2028b8f8 {
        let storage =
            unsafe { &mut *(0x80d00000 as *mut core::mem::MaybeUninit<routing::DualEfxProcessor>) };
        routing::DualEfxProcessor::initialize_in_place(storage);
        unsafe {
            core::ptr::write_volatile(0x83e00000 as *mut u32, 0x494e4954);
            core::ptr::write_volatile(0x83e00004 as *mut u32, entry as u32);
            core::ptr::write_volatile(0x83e00008 as *mut u32, 0);
            // Prevent the copied stock effect-specific post-callback routine from
            // touching the diagnostic state after callback publication.
            core::ptr::write_volatile(0x202a6f28 as *mut u32, 0);
            core::ptr::write_volatile(entry, diagnostic_audio as *const () as usize as u32);
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostic_audio(_context: u32, descriptor: u32) {
    let buffer = unsafe { core::ptr::read_volatile((descriptor + 4) as *const u32) } as *mut f32;
    let control_link = unsafe { core::ptr::read_volatile((descriptor + 28) as *const u32) };
    let control_target =
        unsafe { core::ptr::read_volatile(control_link as *const u32) } as *mut u32;
    let control_source =
        unsafe { core::ptr::read_volatile((descriptor + 32) as *const u32) } as *const u32;
    let state = unsafe { &mut *(0x80d00000 as *mut routing::DualEfxProcessor) };
    let counter = 0x83e00008 as *mut u32;
    unsafe {
        core::ptr::write_volatile(counter, core::ptr::read_volatile(counter).wrapping_add(1));
    }
    for i in 0..64 {
        unsafe {
            core::ptr::write_volatile(
                control_target,
                core::ptr::read_volatile(control_source.add(i)),
            );
        }
        let l = unsafe { core::ptr::read_volatile(buffer.add(i)) };
        let r = unsafe { core::ptr::read_volatile(buffer.add(i + 64)) };
        let (vl, vr) = state.velvet.process_sample(l, r);
        let (pl, pr) = state.plateau.process_sample(l, r);
        unsafe {
            core::ptr::write_volatile(buffer.add(i), math::soft_clip(vl + pl));
            core::ptr::write_volatile(buffer.add(i + 64), math::soft_clip(vr + pr));
        }
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
