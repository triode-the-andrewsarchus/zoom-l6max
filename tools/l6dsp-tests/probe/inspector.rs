//! QEMU-only inspector experiment. Scratch addresses and hooks are unqualified.
use core::sync::atomic::{AtomicU32, Ordering};
use crate::{controls, parameters, routing};

const QUEUE: usize = 0x83e00200;
const INSPECTOR: usize = 0x83e00340;
const SUMMARY: usize = 0x83e00400;
const CAPACITY: u32 = 64;
const MAGIC: u32 = 0x47535431;

#[repr(C)]
struct UiState {
    magic: u32,
    gesture: controls::EfxSelectGesture,
    arg0: u32,
    arg1: u32,
    active: bool,
}

unsafe fn atomic(address: usize) -> &'static AtomicU32 {
    unsafe { &*(address as *const AtomicU32) }
}

/// Called while the stock effect callback is disabled during selection.
pub unsafe fn initialize() {
    unsafe {
        core::ptr::write(QUEUE as *mut AtomicU32, AtomicU32::new(0));
        core::ptr::write((QUEUE + 4) as *mut AtomicU32, AtomicU32::new(0));
        core::ptr::write_volatile((QUEUE + 8) as *mut u32, 0);
        core::ptr::write_volatile(SUMMARY as *mut u32, u32::MAX);
        core::ptr::write_volatile((SUMMARY + 24) as *mut u32, 0);
        core::ptr::write_volatile((SUMMARY + 28) as *mut u32, 0);
        // A selection closes the previous inspector, but preserves gesture
        // state until the selector event returns to its caller.
        let ui = INSPECTOR as *mut UiState;
        if core::ptr::read_volatile(core::ptr::addr_of!((*ui).magic)) == MAGIC {
            (*ui).active = false;
        }
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
            core::ptr::write(ui, UiState { magic: MAGIC, gesture: controls::EfxSelectGesture::default(), arg0: 0, arg1: 0, active: false });
        }
        let s = &mut *ui;
        if event > 1 || !custom_active() || !recorder_active() {
            s.gesture.cancel();
            s.active = false;
            core::ptr::write_volatile((SUMMARY + 28) as *mut u32, 0);
            return stock(arg0, arg1, event);
        }
        if event == 1 { s.arg0 = arg0; s.arg1 = arg1; }
        let tick = core::ptr::read_volatile(0x8020ab4c as *const u32);
        match s.gesture.edge(event == 1, tick, 750) {
            controls::EfxSelectAction::None => 0,
            controls::EfxSelectAction::SelectNext => {
                s.active = false;
                core::ptr::write_volatile((SUMMARY + 28) as *mut u32, 0);
                stock(s.arg0, s.arg1, 1)
            }
            controls::EfxSelectAction::ToggleInspector => {
                s.active = !s.active;
                core::ptr::write_volatile((SUMMARY + 28) as *mut u32, s.active as u32);
                0
            }
        }
    }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.inspector_knob")]
pub unsafe extern "C" fn inspector_knob(channel: u32, mode: u32, absolute: u32, delta: i32, value: u32) {
    let stock: unsafe extern "C" fn(u32, u32, u32, i32, u32) = unsafe { core::mem::transmute(0x80053b29usize) };
    unsafe {
        let ui = INSPECTOR as *const UiState;
        if channel >= 8 || mode != 6 || delta == 0 || !recorder_active() || !custom_active()
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
        core::ptr::write_volatile((QUEUE + 16 + ((head % CAPACITY) as usize) * 4) as *mut u32, (signed << 8) | channel);
        atomic(QUEUE).store(head.wrapping_add(1), Ordering::Release);
    }
}

/// Apply at most eight queued events per callback; only the audio task touches DSP state.
pub unsafe fn consume(processor: &mut routing::DualEfxProcessor) {
    unsafe {
        let head = atomic(QUEUE).load(Ordering::Acquire);
        let mut tail = atomic(QUEUE + 4).load(Ordering::Relaxed);
        let bank = if core::ptr::read_volatile(0x83e00004 as *const u32) == 0x2028b8dc {
            parameters::ParameterBank::Plateau
        } else {
            parameters::ParameterBank::Velvet
        };
        for _ in 0..8 {
            if tail == head { break; }
            let item = core::ptr::read_volatile((QUEUE + 16 + ((tail % CAPACITY) as usize) * 4) as *const u32);
            let delta = (item as i32) >> 8;
            if let Some(id) = parameters::edit_encoder(processor, bank, (item & 0xff) as usize, delta) {
                let value = parameters::format_parameter(id, processor);
                core::ptr::write_volatile(SUMMARY as *mut u32, id as u32);
                core::ptr::write_volatile((SUMMARY + 4) as *mut u32, value.value_len as u32);
                for i in 0..16 { core::ptr::write_volatile((SUMMARY + 8 + i) as *mut u8, value.value_str[i]); }
                let count = (SUMMARY + 24) as *mut u32;
                core::ptr::write_volatile(count, core::ptr::read_volatile(count).wrapping_add(1));
            }
            tail = tail.wrapping_add(1);
            atomic(QUEUE + 4).store(tail, Ordering::Release);
        }
    }
}
