//! Synthetic, QEMU-only inspection of a stock mix routine; never install.
use core::{arch::asm, ptr::{read_volatile as read, write_volatile as write}};
const OUT: usize = 0x83e07000;
#[unsafe(no_mangle)]
unsafe extern "C" fn offline_observer(_context: u32, descriptor: *const u32) {
    unsafe {
        let n = read(OUT as *const u32) as usize;
        if n < 2 {
            let block = read(descriptor.add(1)) as *const f32;
            for (edge, frame) in [0, 63].into_iter().enumerate() {
                write((OUT + 8 + (n * 2 + edge) * 4) as *mut f32, read(block.add(frame)));
                for slot in 0..12 {
                    let sample = (0x81570400 + slot * 960000 + (n * 64 + frame) * 4) as *const f32;
                    write((OUT + 24 + (n * 24 + slot * 2 + edge) * 4) as *mut f32, read(sample));
                }
            }
        }
        write(OUT as *mut u32, (n+1) as u32);
    }
}
pub unsafe fn run() {
    unsafe {
        let saved_mask: u32;
        asm!("mrs {}, PRIMASK", "cpsid i", out(reg) saved_mask, options(nostack));
        let old_context = read(0x202a6f18 as *const u32);
        let old_descriptor = read(0x202a6f1c as *const u32);
        let old_callback = read(0x202a6f20 as *const u32);
        let desc = 0x83e07300 as *mut u32;
        for i in 0..16 { write(desc.add(i), 0); }
        write(OUT as *mut u32, 0);
        write(0x20015110 as *mut u32, 0);
        write(0x20015114 as *mut u32, 0);
        write(0x20016138 as *mut f32, 0.0);
        write(0x2001613c as *mut f32, 1.0);
        for slot in 0usize..14 {
            for frame in 0..128 {
                let v = if slot < 12 { (slot as f32+1.0)*1000000.0 + frame as f32*1000.0 } else { 0.0 };
                write((0x81570400 + slot*960000 + frame*4) as *mut f32, v);
            }
            if slot < 12 {
                write((0x20015584 + slot*0x18) as *mut u32, 0);
                write((0x200156a4 + slot*0x28) as *mut u32, 0);
                write((0x20015884 + slot*0x18) as *mut u32, 0);
                write((0x20016140 + slot*4) as *mut f32, 1.0);
                write((0x20016170 + slot*8) as *mut f32, (slot as f32 + 1.0) / 16.0);
                write((0x20016174 + slot*8) as *mut f32, 1.0);
            }
        }
        write(0x202a6f18 as *mut u32, 0);
        write(0x202a6f1c as *mut u32, desc as u32);
        write(0x202a6f20 as *mut u32, offline_observer as *const () as u32);
        type Mix = unsafe extern "C" fn(u32,u32,u32,u32,u32,u32,u32,u32,u32,u32) -> u32;
        let mix: Mix = core::mem::transmute(0x80017501usize);
        mix(0x82241800,0x81570400,2,0,128,0,0,0,1,0x83e07380);
        write(0x202a6f18 as *mut u32, old_context);
        write(0x202a6f1c as *mut u32, old_descriptor);
        write(0x202a6f20 as *mut u32, old_callback);
        write((OUT + 4) as *mut u32, 0x4f46464c);
        asm!("msr PRIMASK, {}", in(reg) saved_mask, options(nostack));
    }
}
