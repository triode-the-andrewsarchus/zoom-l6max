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
#[cfg(inspector_probe)]
#[path = "../../../patches/dual-efx/src/controls.rs"]
mod controls;
#[cfg(inspector_probe)]
#[path = "../../../patches/dual-efx/src/parameter_view.rs"]
mod parameter_view;
#[cfg(inspector_probe)]
#[path = "../../../patches/dual-efx/src/parameters.rs"]
mod parameters;
#[cfg(inspector_probe)]
mod inspector;
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
            #[cfg(inspector_probe)]
            inspector::initialize();
            core::ptr::write_volatile(0x83e00000 as *mut u32, 0x494e4954);
            core::ptr::write_volatile(0x83e00004 as *mut u32, entry as u32);
            core::ptr::write_volatile(0x83e00008 as *mut u32, 0);
            // QEMU-only test mailbox. Room starts on B; Spring starts on A.
            let route = if entry as usize == 0x2028b8dc { 2u32 } else { 1u32 };
            for slot in 0..12 {
                core::ptr::write_volatile((0x83e00100 as *mut u32).add(slot), route);
            }
            core::ptr::write_volatile(0x83e00140 as *mut u32, 0);
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
    #[cfg(inspector_probe)]
    unsafe { inspector::consume(state); }

    // Reconstruct the three mono sends from the observed mixer slots. This is
    // a diagnostic hypothesis, not an ownership or lifetime guarantee.
    let mut sends = [[0.0f32; 64]; 3];
    for slot in 0..12 {
        let route = unsafe { core::ptr::read_volatile((0x83e00100 as *const u32).add(slot)) };
        if !(1..=3).contains(&route) { continue; }
        let input = (0x2000e410usize + 256 * slot) as *const f32;
        let smoothed = unsafe { core::ptr::read_volatile((0x20016140usize + 4 * slot) as *const f32) };
        let gain = unsafe { core::ptr::read_volatile((0x20016170usize + 8 * slot) as *const f32) };
        let ramp_desc = (0x200161d0usize + 16 * slot) as *const u32;
        let ramped = unsafe { core::ptr::read_volatile(ramp_desc) } != 0;
        let ramp = if ramped {
            unsafe { core::ptr::read_volatile(ramp_desc.add(1)) }.wrapping_sub(256) as *const f32
        } else {
            core::ptr::null()
        };
        for i in 0..64 {
            let sample = unsafe { core::ptr::read_volatile(input.add(i)) } * (1.0 / 2147483648.0);
            let factor = if ramped { unsafe { core::ptr::read_volatile(ramp.add(i)) } } else { gain };
            sends[(route - 1) as usize][i] += (sample * factor) * smoothed;
        }
    }
    if unsafe { core::ptr::read_volatile(0x83e00140 as *const u32) } != 0 {
        for bus in 0..3 {
            for i in 0..64 {
                unsafe { core::ptr::write_volatile((0x83e01000usize + bus * 256 + i * 4) as *mut f32, sends[bus][i]); }
            }
        }
    }
    state.prepare_bus_block();
    for i in 0..64 {
        unsafe {
            core::ptr::write_volatile(
                control_target,
                core::ptr::read_volatile(control_source.add(i)),
            );
        }
        let (l, r) = state.process_bus_sample(sends[0][i], sends[1][i], sends[2][i]);
        unsafe {
            core::ptr::write_volatile(buffer.add(i), l);
            core::ptr::write_volatile(buffer.add(i + 64), r);
        }
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
