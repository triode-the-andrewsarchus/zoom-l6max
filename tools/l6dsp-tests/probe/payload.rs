// QEMU-only diagnostic payload. This is not safe firmware and must never be installed.
#![no_std]
#![no_main]
#![allow(dead_code)]
#[path = "../../../patches/dual-efx/src/math.rs"] mod math;
#[path = "../../../patches/dual-efx/src/velvet.rs"] mod velvet;
#[path = "../../../patches/dual-efx/src/plateau.rs"] mod plateau;
#[path = "../../../patches/dual-efx/src/routing.rs"] mod routing;
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.probe")]
pub unsafe extern "C" fn probe() {
    let clear: unsafe extern "C" fn() = unsafe { core::mem::transmute(0x80024679usize) };
    unsafe { clear() };
    let storage = unsafe { &mut *(0x80d00000 as *mut core::mem::MaybeUninit<routing::DualEfxProcessor>) };
    let processor = routing::DualEfxProcessor::initialize_in_place(storage);
    let mut v_peak = 0.0f32;
    let mut p_peak = 0.0f32;
    for i in 0..2048 {
        let input = if i == 0 { 0.5 } else { 0.0 };
        let (vl, vr) = processor.velvet.process_sample(input, input);
        let (pl, pr) = processor.plateau.process_sample(input, input);
        v_peak = v_peak.max(vl.abs()).max(vr.abs());
        p_peak = p_peak.max(pl.abs()).max(pr.abs());
    }
    // This scratch address is only a diagnostic sink in the QEMU model.
    unsafe {
        core::ptr::write_volatile(0x83e00000 as *mut u32, v_peak.to_bits());
        core::ptr::write_volatile(0x83e00004 as *mut u32, p_peak.to_bits());
        core::ptr::write_volatile(0x83e00008 as *mut u32, 0x46585031);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop { core::hint::spin_loop(); } }
