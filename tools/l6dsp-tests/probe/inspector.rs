//! QEMU-only inspector experiment. Scratch addresses and hooks are unqualified.
use core::sync::atomic::{AtomicU32, Ordering};
use crate::{controls, parameters, routing};
#[cfg(knob_inspector)]
unsafe extern "C" {
    fn inspector_notify_hook(id: u32, bits: u32);
    fn inspector_generation_hook() -> u32;
    fn inspector_edit_hook(id: u32, bits: u32, generation: u32);
    fn inspector_cancel_hook();
    fn inspector_defer_cancel_hook();
    fn inspector_handoff_hook() -> u32;
}

const QUEUE: usize = 0x83e00200;
const INSPECTOR: usize = 0x83e00340;
#[cfg(knob_inspector)]
const QUEUE_GENERATIONS: usize = 0x83e02400;
const SUMMARY: usize = 0x83e00400;
#[cfg(not(knob_inspector))]
const SAVED_PARAMETERS: usize = 0x83e03800;

#[cfg(knob_inspector)]
const _: () = assert!(core::mem::size_of::<parameters::ParameterHandoff>() == 64
    && core::mem::align_of::<parameters::ParameterHandoff>() == 4);
unsafe fn saved_parameters() -> *mut parameters::ParameterHandoff {
    #[cfg(knob_inspector)]
    { unsafe { inspector_handoff_hook() as *mut _ } }
    #[cfg(not(knob_inspector))]
    { SAVED_PARAMETERS as *mut _ }
}
const CAPACITY: u32 = 64;
const MAGIC: u32 = 0x47535431;

#[repr(C)]
struct UiState {
    magic: u32,
    gesture: controls::EfxSelectGesture,
    arg0: u32,
    arg1: u32,
    active: bool,
    routing_page: bool,
    velvet_bank: bool,
}

unsafe fn atomic(address: usize) -> &'static AtomicU32 {
    unsafe { &*(address as *const AtomicU32) }
}

/// Close a live inspector when the observed selector enters another effect.
/// Gesture state remains intact until the selector event returns.
pub unsafe fn close_for_selection() {
    unsafe {
        let ui = INSPECTOR as *mut UiState;
        if core::ptr::read_volatile(core::ptr::addr_of!((*ui).magic)) == MAGIC {
            #[cfg(knob_inspector)]
            if (*ui).active { inspector_defer_cancel_hook(); }
            (*ui).active = false;
            (*ui).routing_page = false;
        }
        core::ptr::write_volatile((SUMMARY + 28) as *mut u32, 0);
    }
}

/// Called while the stock effect callback is disabled during selection.
pub unsafe fn initialize(entry: u32, processor: *mut routing::DualEfxProcessor, initialized: bool, reuse: bool) {
    unsafe {
        if !reuse {
            let saved = saved_parameters();
            if !saved.is_null() {
                if initialized {
                    (*saved).restore(&mut *processor);
                } else {
                    core::ptr::write(saved, parameters::ParameterHandoff::new());
                }
            }
            // The queue lives outside the stock effect buffer. Pending edits
            // must survive a stock selection until a custom callback resumes.
            if !initialized {
                core::ptr::write(QUEUE as *mut AtomicU32, AtomicU32::new(0));
                core::ptr::write((QUEUE + 4) as *mut AtomicU32, AtomicU32::new(0));
                core::ptr::write_volatile((QUEUE + 8) as *mut u32, 0);
            }
            core::ptr::write_volatile(SUMMARY as *mut u32, u32::MAX);
            core::ptr::write_volatile((SUMMARY + 24) as *mut u32, 0);
            core::ptr::write_volatile((SUMMARY + 28) as *mut u32, 0);
            #[cfg(gesture_trace)]
            core::ptr::write_volatile(0x83e00500 as *mut u32, 0);
            #[cfg(enqueue_trace)]
            {
                core::ptr::write_volatile(0x83e00600 as *mut u32, 0);
                core::ptr::write_volatile(0x83e02000 as *mut u32, 0);
                core::ptr::write_volatile(0x83e02200 as *mut u32, 0);
            }
        }
        close_for_selection();
        let ui = INSPECTOR as *mut UiState;
        if core::ptr::read_volatile(core::ptr::addr_of!((*ui).magic)) == MAGIC {
            (*ui).velvet_bank = entry != 0x2028b8dc;
        }
    }
}

/// Diagnostic only: observe the panel event immediately before stock enqueue.
#[cfg(enqueue_trace)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.inspector_enqueue")]
pub unsafe extern "C" fn inspector_enqueue(a: u32, edge: u32, key: u32, d: u32, e: u32) -> u32 {
    let stock: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 = unsafe { core::mem::transmute(0x80028089usize) };
    unsafe {
        if a == 0 && edge <= 1 && key == 36 {
            let count = core::ptr::read_volatile(0x83e00600 as *const u32);
            if count < 32 {
                let record = (0x83e00604 as *mut u32).add(count as usize * 2);
                core::ptr::write_volatile(record, edge);
                core::ptr::write_volatile(record.add(1), core::ptr::read_volatile(0x8020ab4c as *const u32));
                core::ptr::write_volatile(0x83e00600 as *mut u32, count + 1);
            }
        }
        stock(a, edge, key, d, e)
    }
}

/// QEMU-only timestamps for the stock encoder enqueue and later UI dispatch.
/// Each site has its own bounded diagnostic array, reset at selection. These
/// unowned records are for disposable experiments and are not synchronized
/// with selection or suitable for a hardware build.
#[cfg(enqueue_trace)]
unsafe fn trace_encoder(base: usize, channel: u32, value: u32) {
    unsafe {
        let count = core::ptr::read_volatile(base as *const u32);
        if count < 32 {
            let entry = (base as *mut u32).add(1 + count as usize * 3);
            core::ptr::write_volatile(entry, channel);
            core::ptr::write_volatile(entry.add(1), value);
            core::ptr::write_volatile(entry.add(2), core::ptr::read_volatile(0x8020ab4c as *const u32));
            core::ptr::write_volatile(base as *mut u32, count + 1);
        }
    }
}

#[cfg(enqueue_trace)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.inspector_encoder_enqueue")]
pub unsafe extern "C" fn inspector_encoder_enqueue(a: u32, b: u32, c: u32, d: u32, e: u32) -> u32 {
    let stock: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 = unsafe { core::mem::transmute(0x80028089usize) };
    unsafe {
        trace_encoder(0x83e02000, a, e);
        stock(a, b, c, d, e)
    }
}

fn recorder_active() -> bool {
    unsafe { core::ptr::read_volatile(0x80202610 as *const u32) == 0x80200408 }
}
fn custom_active() -> bool {
    unsafe { core::ptr::read_volatile(0x202a6f20 as *const u32) == (crate::diagnostic_audio as *const () as usize as u32 | 1) }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.inspector_select")]
pub unsafe extern "C" fn inspector_select(arg0: u32, arg1: u32, event: u32) -> u32 {
    let stock: unsafe extern "C" fn(u32, u32, u32) -> u32 = unsafe { core::mem::transmute(0x800542a9usize) };
    unsafe {
        let ui = INSPECTOR as *mut UiState;
        if core::ptr::read_volatile(core::ptr::addr_of!((*ui).magic)) != MAGIC {
            core::ptr::write(ui, UiState { magic: MAGIC, gesture: controls::EfxSelectGesture::default(), arg0: 0, arg1: 0, active: false, routing_page: false, velvet_bank: core::ptr::read_volatile(0x83e00004 as *const u32) != 0x2028b8dc });
        }
        let s = &mut *ui;
        #[cfg(gesture_trace)]
        {
            let count = core::ptr::read_volatile(0x83e00500 as *const u32);
            if count < 20 {
                let record = (0x83e00504 as *mut u32).add(count as usize * 3);
                core::ptr::write_volatile(record, event);
                core::ptr::write_volatile(record.add(1), core::ptr::read_volatile(0x8020ab4c as *const u32));
                core::ptr::write_volatile(record.add(2), s.active as u32);
                core::ptr::write_volatile(0x83e00500 as *mut u32, count + 1);
            }
        }
        if event > 1 || !custom_active() || !recorder_active() || saved_parameters().is_null() {
            #[cfg(irq_timing)]
            if event <= 1 { let _ = crate::irq::take(event == 1); }
            s.gesture.cancel();
            #[cfg(knob_inspector)]
            if s.active { inspector_cancel_hook(); }
            s.active = false;
            s.routing_page = false;
            core::ptr::write_volatile((SUMMARY + 28) as *mut u32, 0);
            return stock(arg0, arg1, event);
        }
        if event == 1 { s.arg0 = arg0; s.arg1 = arg1; }
        #[cfg(not(irq_timing))]
        let tick = core::ptr::read_volatile(0x8020ab4c as *const u32);
        #[cfg(irq_timing)]
        let Some(tick) = crate::irq::take(event == 1) else {
            s.gesture.cancel();
            return 0;
        };
        match s.gesture.edge(event == 1, tick, 750) {
            controls::EfxSelectAction::None => 0,
            controls::EfxSelectAction::SelectNext => {
                if s.active {
                    #[cfg(knob_inspector)]
                    inspector_cancel_hook();
                    s.velvet_bank = !s.velvet_bank;
                    s.routing_page = false;
                    #[cfg(knob_inspector)]
                    inspector_notify_hook(24, (if s.velvet_bank { 2.0f32 } else { 0.0f32 }).to_bits());
                    return 0;
                }
                s.active = false;
                s.routing_page = false;
                core::ptr::write_volatile((SUMMARY + 28) as *mut u32, 0);
                stock(s.arg0, s.arg1, 1)
            }
            controls::EfxSelectAction::ToggleInspector => {
                #[cfg(knob_inspector)]
                if s.active { inspector_cancel_hook(); }
                s.active = !s.active;
                s.routing_page = false;
                core::ptr::write_volatile((SUMMARY + 28) as *mut u32, s.active as u32);
                #[cfg(knob_inspector)]
                if s.active { inspector_notify_hook(24, (if s.velvet_bank { 2.0f32 } else { 0.0f32 }).to_bits()); }
                0
            }
        }
    }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.inspector_mode")]
pub unsafe extern "C" fn inspector_mode(mode: u32, event: u32) {
    let stock: unsafe extern "C" fn(u32, u32) = unsafe { core::mem::transmute(0x80053bb1usize) };
    unsafe {
        stock(mode, event);
        let ui = INSPECTOR as *mut UiState;
        if mode == 6 && event == 1 && recorder_active() && custom_active() && !saved_parameters().is_null()
            && core::ptr::read_volatile(core::ptr::addr_of!((*ui).magic)) == MAGIC && (*ui).active {
            #[cfg(knob_inspector)]
            inspector_cancel_hook();
            (*ui).routing_page = !(*ui).routing_page;
            #[cfg(knob_inspector)]
            inspector_notify_hook(24, (if (*ui).routing_page { 1.0f32 } else if (*ui).velvet_bank { 2.0f32 } else { 0.0f32 }).to_bits());
        }
    }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.inspector_knob")]
pub unsafe extern "C" fn inspector_knob(channel: u32, mode: u32, absolute: u32, delta: i32, value: u32) {
    #[cfg(not(knob_inspector))]
    let stock: unsafe extern "C" fn(u32, u32, u32, i32, u32) = unsafe { core::mem::transmute(0x80053b29usize) };
    #[cfg(knob_inspector)]
    let stock: unsafe extern "C" fn(u32, u32, u32, i32, u32) = unsafe { core::mem::transmute(0x80108001usize) };
    unsafe {
        #[cfg(enqueue_trace)]
        trace_encoder(0x83e02200, channel, delta as u32);
        let ui = INSPECTOR as *const UiState;
        if channel >= 8 || mode != 6 || delta == 0 || !recorder_active() || !custom_active() || saved_parameters().is_null()
            || core::ptr::read_volatile(core::ptr::addr_of!((*ui).magic)) != MAGIC || !(*ui).active {
            stock(channel, mode, absolute, delta, value);
            return;
        }
        let head = atomic(QUEUE).load(Ordering::Relaxed);
        let tail = atomic(QUEUE + 4).load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= CAPACITY {
            let dropped = (QUEUE + 8) as *mut u32;
            core::ptr::write_volatile(dropped, core::ptr::read_volatile(dropped).wrapping_add(1));
            return;
        }
        let signed = delta.clamp(-8_388_608, 8_388_607) as u32 & 0x00ff_ffff;
        let encoded_channel = channel | if (*ui).routing_page { 0x80 } else if (*ui).velvet_bank { 0x40 } else { 0 };
        #[cfg(knob_inspector)]
        core::ptr::write_volatile((QUEUE_GENERATIONS + ((head % CAPACITY) as usize) * 4) as *mut u32, inspector_generation_hook());
        core::ptr::write_volatile((QUEUE + 16 + ((head % CAPACITY) as usize) * 4) as *mut u32, (signed << 8) | encoded_channel);
        atomic(QUEUE).store(head.wrapping_add(1), Ordering::Release);
    }
}

/// Apply at most eight queued events per callback; only the audio task touches DSP state.
pub unsafe fn consume(processor: &mut routing::DualEfxProcessor) {
    unsafe {
        let head = atomic(QUEUE).load(Ordering::Acquire);
        let mut tail = atomic(QUEUE + 4).load(Ordering::Relaxed);
        for _ in 0..8 {
            if tail == head { break; }
            let item = core::ptr::read_volatile((QUEUE + 16 + ((tail % CAPACITY) as usize) * 4) as *const u32);
            let delta = (item as i32) >> 8;
            #[cfg(knob_inspector)]
            let generation = core::ptr::read_volatile((QUEUE_GENERATIONS + ((tail % CAPACITY) as usize) * 4) as *const u32);
            if item & 0x80 != 0 {
                let strip = (item & 0x7f) as usize;
                for slot in 0..12 {
                    let route = core::ptr::read_volatile((0x83e00100 as *const u32).add(slot));
                    processor.channel_routing[slot] = routing::RoutingMode::from_u8(if route <= 3 { route as u8 } else { 0 });
                }
                if let Some(mode) = processor.cycle_strip_routing(strip, delta) {
                    for slot in 0..12 {
                        core::ptr::write_volatile((0x83e00100 as *mut u32).add(slot), processor.channel_routing[slot] as u32);
                    }
                    #[cfg(knob_inspector)]
                    inspector_edit_hook(16 + strip as u32, (mode as u8 as f32).to_bits(), generation);
                    #[cfg(not(knob_inspector))]
                    {
                        let value = crate::parameter_view::format_display(crate::parameter_view::DisplaySnapshot::from_words(16 + strip as u32, (mode as u8 as f32).to_bits()).unwrap());
                        core::ptr::write_volatile(SUMMARY as *mut u32, 16 + strip as u32);
                        core::ptr::write_volatile((SUMMARY + 4) as *mut u32, value.value_len as u32);
                        for i in 0..16 { core::ptr::write_volatile((SUMMARY + 8 + i) as *mut u8, value.value_str[i]); }
                    }
                    let count = (SUMMARY + 24) as *mut u32;
                    core::ptr::write_volatile(count, core::ptr::read_volatile(count).wrapping_add(1));
                }
            } else if let Some(id) = parameters::edit_encoder(processor,
                if item & 0x40 != 0 { parameters::ParameterBank::Velvet } else { parameters::ParameterBank::Plateau },
                (item & 0x3f) as usize, delta) {
                let saved = saved_parameters();
                if !saved.is_null() { (*saved).record(id, processor); }
                #[cfg(knob_inspector)]
                {
                    let value = parameters::snapshot_parameter(id, processor);
                    inspector_edit_hook(id as u32, value.value.to_bits(), generation);
                }
                #[cfg(not(knob_inspector))]
                {
                let value = parameters::format_parameter(id, processor);
                core::ptr::write_volatile(SUMMARY as *mut u32, id as u32);
                core::ptr::write_volatile((SUMMARY + 4) as *mut u32, value.value_len as u32);
                for i in 0..16 { core::ptr::write_volatile((SUMMARY + 8 + i) as *mut u8, value.value_str[i]); }
                }
                let count = (SUMMARY + 24) as *mut u32;
                core::ptr::write_volatile(count, core::ptr::read_volatile(count).wrapping_add(1));
            }
            tail = tail.wrapping_add(1);
            atomic(QUEUE + 4).store(tail, Ordering::Release);
        }
    }
}
