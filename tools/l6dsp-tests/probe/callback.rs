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
#[cfg(irq_trace)]
mod irq;
#[cfg(auto_handoff_probe)]
mod auto_handoff;
#[cfg(effect_buffer_probe)]
const _: () = assert!(core::mem::size_of::<routing::DualEfxProcessor>() <= 0x50000);

// QEMU-only selector veneer. The aligned redirect replaces `mov r1, r4`,
// the stock call and `cmp r6, #4`; the original conditional branch remains.
// Replay the call, close the inspector on the special AINR path, then restore
// the comparison flags before returning to that branch.
#[cfg(inspector_probe)]
core::arch::global_asm!(r#"
    .syntax unified
    .thumb
    .section .text.ainr_selector_veneer,"ax",%progbits
    .global ainr_selector_veneer
    .thumb_func
ainr_selector_veneer:
    mov r1, r4
    ldr.w r12, .Loriginal_select_call
    blx r12
    cmp r6, #4
    bls .Lresume_selector
    push.w {r0, r1, r2, r3, r12, lr}
    bl ainr_selection_close
    pop.w {r0, r1, r2, r3, r12, lr}
.Lresume_selector:
    cmp r6, #4
    ldr.w pc, .Lselector_branch
    .align 2
.Loriginal_select_call: .word 0x20282f6d
.Lselector_branch: .word 0x202805d5
"#, options(raw));

#[cfg(inspector_probe)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ainr_selection_close() {
    unsafe { inspector::close_for_selection(); }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.probe")]
pub unsafe extern "C" fn probe(entry: *mut u32) {
    let custom = entry as usize == 0x2028b8dc || entry as usize == 0x2028b8f8;
    #[cfg(inspector_probe)]
    if !custom { unsafe { inspector::close_for_selection(); } }
    // This unowned marker is only for disposable QEMU experiments. It does
    // not establish buffer ownership or synchronization on the device.
    let reuse = cfg!(retain_custom_state) && custom
        && unsafe { core::ptr::read_volatile(0x83e00148 as *const u32) } == 0x52455553;
    if !reuse {
        #[cfg(retain_custom_state)]
        unsafe { core::ptr::write_volatile(0x83e00148 as *mut u32, 0); }
        #[cfg(auto_handoff_probe)]
        unsafe { auto_handoff::disable(); }
        let clear: unsafe extern "C" fn() = unsafe { core::mem::transmute(0x80024679usize) };
        unsafe { clear() };
    }
    if custom {
        #[cfg(auto_handoff_probe)]
        unsafe { auto_handoff::allocate(); }
        let storage = 0x80d00000 as *mut core::mem::MaybeUninit<routing::DualEfxProcessor>;
        if !reuse {
            routing::DualEfxProcessor::initialize_in_place(unsafe { &mut *storage });
        }
        unsafe {
            // The route mailbox is outside the stock effect buffer. Keep it
            // across selections within this disposable emulator session.
            let routes_initialized = core::ptr::read_volatile(0x83e00000 as *const u32) == 0x494e4954;
            #[cfg(inspector_probe)]
            inspector::initialize(entry as u32, storage.cast(), routes_initialized, reuse);
            #[cfg(irq_trace)]
            irq::install();
            core::ptr::write_volatile(0x83e00000 as *mut u32, 0x494e4954);
            core::ptr::write_volatile(0x83e00004 as *mut u32, entry as u32);
            core::ptr::write_volatile(0x83e00008 as *mut u32, 0);
            // In the combined diagnostic, retain functional routes in the
            // heap sidecar. Null sidecar falls back to this effect's default.
            #[cfg(knob_inspector)]
            if !reuse {
                inspector::initialize_routes(&mut *storage.cast(), entry as usize);
            }
            #[cfg(not(knob_inspector))]
            if !routes_initialized {
                let route = if entry as usize == 0x2028b8dc { 2u32 } else { 1u32 };
                for slot in 0..12 {
                    core::ptr::write_volatile((0x83e00100 as *mut u32).add(slot), route);
                }
            }
            core::ptr::write_volatile(0x83e00140 as *mut u32, 0);
            #[cfg(effect_buffer_probe)]
            core::ptr::write_volatile(0x83e0301c as *mut u32, core::mem::size_of::<routing::DualEfxProcessor>() as u32);
            #[cfg(active_sends)]
            {
                core::ptr::write_volatile(0x83e00800 as *mut u32, 0x5349474e);
                core::ptr::write_volatile(0x83e00804 as *mut u32, 0);
                core::ptr::write_volatile(0x83e00808 as *mut u32, 0);
                core::ptr::write_volatile(0x83e0080c as *mut u32, 0);
            }
            #[cfg(auto_handoff_probe)]
            if !reuse { auto_handoff::install(); }
            // Prevent the copied stock effect-specific post-callback routine from
            // touching the diagnostic state after callback publication.
            core::ptr::write_volatile(0x202a6f28 as *mut u32, 0);
            #[cfg(retain_custom_state)]
            core::ptr::write_volatile(0x83e00148 as *mut u32, 0x52455553);
            core::ptr::write_volatile(entry, diagnostic_audio as *const () as usize as u32);
        }
    }
}
// QEMU-only helpers. The observer sets this marker only after copying all
// three guarded code and state regions into disposable emulator scratch.
#[cfg(pre_ainr_probe)]
#[inline(always)]
fn pre_ainr_ready() -> bool {
    #[cfg(auto_handoff_probe)]
    { auto_handoff::ready() }
    #[cfg(not(auto_handoff_probe))]
    { unsafe { core::ptr::read_volatile(0x83e00144 as *const u32) == 0x41494e52 } }
}

#[cfg(pre_ainr_probe)]
fn synthetic_pre_ainr_sends(sends: &mut [[f32; 64]; 3]) {
    let active = cfg!(unready_passthrough_probe) || pre_ainr_ready();
    const WAVE: [f32; 16] = [0.0, 0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.25,
                              0.0, -0.25, -0.5, -0.75, -1.0, -0.75, -0.5, -0.25];
    for i in 0..64 {
        sends[0][i] = if active { 0.125 * WAVE[i % 16] } else { 0.0 };
        sends[1][i] = if active { 0.0625 * WAVE[i % 16] } else { 0.0 };
        sends[2][i] = if active { -0.09375 * WAVE[(i + 4) % 16] } else { 0.0 };
    }
}

#[cfg(pre_ainr_probe)]
fn process_pre_ainr_sends(sends: &mut [[f32; 64]; 3]) {
    if !pre_ainr_ready() { return; }
    for bus in 0..3 {
        let before = sends[bus][63];
        #[cfg(not(effect_buffer_probe))]
        let entry = 0x80f00781usize + bus * 0x10000;
        #[cfg(effect_buffer_probe)]
        let entry = 0x80130781usize + bus * 0x6000;
        let ainr: unsafe extern "C" fn(*mut f32, *mut f32, u32) = unsafe { core::mem::transmute(entry) };
        let samples = sends[bus].as_mut_ptr();
        unsafe {
            ainr(samples, samples, 0);
            core::ptr::write_volatile((0x83e03004usize + bus * 8) as *mut f32, before);
            core::ptr::write_volatile((0x83e03008usize + bus * 8) as *mut f32, sends[bus][63]);
        }
    }
    unsafe {
        let count = 0x83e03000 as *mut u32;
        core::ptr::write_volatile(count, core::ptr::read_volatile(count).wrapping_add(1));
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

    // Timing control for QEMU investigation only: preserve the callback's
    // control stream and buffer contract while bypassing all effect DSP.
    #[cfg(timing_control)]
    {
        for i in 0..64 {
            unsafe {
                core::ptr::write_volatile(control_target, core::ptr::read_volatile(control_source.add(i)));
                core::ptr::write_volatile(buffer.add(i), 0.0);
                core::ptr::write_volatile(buffer.add(i + 64), 0.0);
            }
        }
    }
    #[cfg(not(timing_control))]
    {
        // Reconstruct the three mono sends from the observed mixer slots. This is
        // a diagnostic hypothesis, not an ownership or lifetime guarantee.
        let mut sends = [[0.0f32; 64]; 3];
        #[cfg(knob_inspector)]
        let routes = unsafe { inspector::routes_ptr() };
        for slot in 0..12 {
            #[cfg(knob_inspector)]
            let route = unsafe { inspector::route_read(routes, slot, state.channel_routing[slot] as u32) };
            #[cfg(not(knob_inspector))]
            let route = unsafe { core::ptr::read_volatile((0x83e00100 as *const u32).add(slot)) };
            if !(1..=3).contains(&route) { continue; }
            let input = (0x2000e410usize + 256 * slot) as *const f32;
            let smoothed = unsafe { core::ptr::read_volatile((0x20016140usize + 4 * slot) as *const f32) };
            let gain = unsafe { core::ptr::read_volatile((0x20016170usize + 8 * slot) as *const f32) };
            let ramp_desc = (0x200161d0usize + 16 * slot) as *const u32;
            let ramped = unsafe { core::ptr::read_volatile(ramp_desc) } != 0;
            // A fixed zero contributes silence even for corrupt input; an
            // active ramp can still contain nonzero per-frame gains.
            if !ramped && gain == 0.0 { continue; }
            let ramp = if ramped {
                unsafe { core::ptr::read_volatile(ramp_desc.add(1)) }.wrapping_sub(256) as *const f32
            } else {
                core::ptr::null()
            };
            for i in 0..64 {
                let sample = unsafe { core::ptr::read_volatile(input.add(i)) } * (1.0 / 2147483648.0);
                let factor = if ramped { unsafe { core::ptr::read_volatile(ramp.add(i)) } } else { gain };
                sends[(route - 1) as usize][i] += math::audio_input((sample * factor) * smoothed);
            }
        }
        // QEMU-only worst-case control: keep all three effect buses active.
        #[cfg(active_sends)]
        {
            #[cfg(not(pre_ainr_probe))]
            {
                sends[0].fill(0.125);
                sends[1].fill(0.0625);
                sends[2].fill(-0.09375);
            }
            #[cfg(pre_ainr_probe)]
            synthetic_pre_ainr_sends(&mut sends);
        }
        #[cfg(pre_ainr_probe)]
        process_pre_ainr_sends(&mut sends);
        if unsafe { core::ptr::read_volatile(0x83e00140 as *const u32) } != 0 {
            for bus in 0..3 {
                for i in 0..64 {
                    unsafe { core::ptr::write_volatile((0x83e01000usize + bus * 256 + i * 4) as *mut f32, sends[bus][i]); }
                }
            }
        }
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
        #[cfg(active_sends)]
        unsafe {
            core::ptr::write_volatile(0x83e00804 as *mut u32, core::ptr::read_volatile(counter));
            core::ptr::write_volatile(0x83e00808 as *mut u32, core::ptr::read_volatile(buffer.add(63)).to_bits());
            core::ptr::write_volatile(0x83e0080c as *mut u32, core::ptr::read_volatile(buffer.add(127)).to_bits());
        }
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
