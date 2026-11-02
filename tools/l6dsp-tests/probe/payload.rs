// QEMU-only diagnostic payload. This is not safe firmware and must never be installed.
#![no_std]
#![no_main]
#![allow(dead_code)]
#[path = "../../../patches/dual-efx/src/math.rs"] mod math;
#[path = "../../../patches/dual-efx/src/velvet.rs"] mod velvet;
#[path = "../../../patches/dual-efx/src/plateau.rs"] mod plateau;
#[cfg(experimental_plateau_32k)]
#[path = "../../../patches/dual-efx/src/plateau_rate.rs"] mod plateau_rate;
#[path = "../../../patches/dual-efx/src/routing.rs"] mod routing;
#[cfg(admission_probe)]
#[path = "../../../patches/dual-efx/src/admission.rs"] mod admission;
core::arch::global_asm!(include_str!("clear_dispatch.s"));
#[cfg(not(admission_probe))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn probe_selection() {
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

#[cfg(admission_probe)]
fn check_live_bypass(gate: &admission::CallbackAdmission) -> u32 {
    use admission::CallbackPath;
    let mut passed = 0;
    let file = gate.try_enter(CallbackPath::FileMix);
    let bypass = gate.try_enter_live_bypass();
    if file.is_some() && bypass.is_some() { passed |= 0x800; }
    if gate.try_enter_live_bypass().is_none() && gate.try_enter(CallbackPath::Live).is_none() { passed |= 0x1000; }
    let close = gate.try_close();
    if close.as_ref().is_some_and(|guard| !guard.is_drained()) { passed |= 0x2000; }
    if gate.try_enter_live_bypass().is_none() { passed |= 0x4000; }
    drop(file);
    if close.as_ref().is_some_and(|guard| !guard.is_drained()) { passed |= 0x8000; }
    drop(bypass);
    if close.as_ref().is_some_and(|guard| guard.is_drained()) { passed |= 0x10000; }
    drop(close);
    if gate.try_enter(CallbackPath::Live).is_some() { passed |= 0x20000; }
    passed
}

#[cfg(admission_probe)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn probe_selection() {
    use admission::{CallbackAdmission, CallbackPath};
    let gate = CallbackAdmission::new();
    let mut passed = 0u32;
    let live = gate.try_enter(CallbackPath::Live);
    if live.is_some() { passed |= 1; }
    if gate.try_enter(CallbackPath::FileMix).is_none() { passed |= 2; }
    if gate.try_enter(CallbackPath::Live).is_none() { passed |= 4; }
    let close = gate.try_close();
    if close.is_some() { passed |= 8; }
    if gate.try_close().is_none() { passed |= 16; }
    if gate.try_enter(CallbackPath::Live).is_none() { passed |= 32; }
    if close.as_ref().is_some_and(|guard| !guard.is_drained()) { passed |= 64; }
    drop(live);
    if close.as_ref().is_some_and(|guard| guard.is_drained()) { passed |= 128; }
    if gate.try_enter(CallbackPath::FileMix).is_none() { passed |= 256; }
    drop(close);
    if gate.try_enter(CallbackPath::Live).is_some() { passed |= 512; }
    let file = gate.try_enter(CallbackPath::FileMix);
    let timed_out = gate.try_close();
    drop(timed_out);
    if file.is_some() && gate.try_enter(CallbackPath::Live).is_none() { passed |= 1024; }
    drop(file);
    passed |= check_live_bypass(&gate);
    unsafe {
        core::ptr::write_volatile(0x83e00000 as *mut u32, passed);
        core::ptr::write_volatile(0x83e00004 as *mut u32, 0x41444d31);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop { core::hint::spin_loop(); } }
