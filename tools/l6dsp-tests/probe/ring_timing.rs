//! Disposable QEMU-only selector timing experiment. None of these addresses,
//! event fields, interrupt timings or memory ownership are hardware-qualified.
use core::ptr::{read_volatile as read, write_volatile as write};

const STATE: *mut u32 = 0x83e02800 as *mut u32;
const PORT0: *const u32 = 0x20007b9c as *const u32;
const MAGIC: u32 = 0x52494e47;
const CAPACITY: u32 = 64;
const RX_CAPACITY: u32 = 8192;
const RECORDS: usize = 16;

// Called once with interrupts masked, before installing the diagnostic ISR.
pub unsafe fn initialize() {
    unsafe {
        for i in 0..(RECORDS + CAPACITY as usize * 4) { write(STATE.add(i), 0); }
        #[cfg(any(ring_drop_pair, ring_drop_release))]
        write(0x83e02c80 as *mut u32, 0);
        let ring = read(PORT0) as *const u32;
        if ring.is_null() || read(ring.add(3)) != RX_CAPACITY {
            write(STATE.add(1), 1);
        } else {
            write(STATE.add(2), ring as u32);
        }
        write(STATE, MAGIC);
    }
}

// The ISR runs before stock writes the byte. Stop using tags if the ring is
// replaced, corrupt, or about to fill; there is no automatic recovery here.
pub unsafe fn observe_byte() {
    unsafe {
        if read(STATE) != MAGIC || read(STATE.add(1)) != 0 { return; }
        let ring = read(PORT0) as *const u32;
        if ring.is_null() || ring as u32 != read(STATE.add(2)) {
            write(STATE.add(1), 2); return;
        }
        let head = read(ring.add(1)); let tail = read(ring.add(2)); let cap = read(ring.add(3));
        if cap != RX_CAPACITY || head >= cap || tail >= cap {
            write(STATE.add(1), 3); return;
        }
        let next = if head + 1 == cap { 0 } else { head + 1 };
        if next == tail { write(STATE.add(1), 4); return; }
        let pending_head = read(STATE.add(3)); let pending_tail = read(STATE.add(4));
        if pending_head != pending_tail {
            let oldest = STATE.add(RECORDS + (pending_tail % CAPACITY) as usize * 4);
            // A slot is recorded before its original byte is written. This
            // check runs before subsequent writes, so seeing it again means
            // it would be overwritten before the tracked consumer matched it.
            if read(oldest) == head { write(STATE.add(1), 8); }
        }
    }
}

pub unsafe fn observe_edge(pressed: bool, tick: u32) {
    unsafe {
        if read(STATE) != MAGIC || read(STATE.add(1)) != 0 { return; }
        let head = read(STATE.add(3)); let tail = read(STATE.add(4));
        if head.wrapping_sub(tail) >= CAPACITY { write(STATE.add(1), 5); return; }
        let ring = read(PORT0) as *const u32;
        let idx = read(ring.add(1));
        let next = read(STATE.add(9)).wrapping_add(1);
        let sequence = if next == 0 { 1 } else { next };
        let rec = STATE.add(RECORDS + (head % CAPACITY) as usize * 4);
        write(rec, idx); write(rec.add(1), tick); write(rec.add(2), pressed as u32); write(rec.add(3), sequence);
        write(STATE.add(9), sequence);
        write(STATE.add(3), head.wrapping_add(1));
    }
}

/// Revision-guarded replacement for an observed one-byte panel read call.
/// IRQ masking makes the stock consume and slot lookup one short operation.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.inspector_rx_read")]
pub unsafe extern "C" fn inspector_rx_read(out: *mut u8, port: u32, count: u32, actual: *mut u32) {
    unsafe {
        let mask: u32;
        core::arch::asm!("mrs {mask}, primask", "cpsid i", mask = out(reg) mask, options(nostack));
        let ring = read(PORT0) as *const u32;
        let before = if ring.is_null() { u32::MAX } else { read(ring.add(2)) };
        let stock: unsafe extern "C" fn(*mut u8, u32, u32, *mut u32) = core::mem::transmute(0x80092049usize);
        stock(out, port, count, actual);
        if read(STATE) == MAGIC {
            write(STATE.add(5), 0);
            if port == 0 && count == 1 && read(actual) == 1 && read(STATE.add(1)) == 0 {
                if ring.is_null() || ring as u32 != read(STATE.add(2)) {
                    write(STATE.add(1), 2);
                } else {
                    let head = read(STATE.add(3)); let tail = read(STATE.add(4));
                    if head != tail {
                        let rec = STATE.add(RECORDS + (tail % CAPACITY) as usize * 4);
                        if read(rec) == before {
                            write(STATE.add(6), read(rec.add(1)));
                            write(STATE.add(7), read(rec.add(2)));
                            write(STATE.add(8), read(rec.add(3)));
                            write(STATE.add(5), 1);
                            write(STATE.add(4), tail.wrapping_add(1));
                        } else {
                            // An unmatched record must still point into the
                            // unread portion of this same ring. Otherwise a
                            // different consumer may have skipped it; do not
                            // wait for the slot number to wrap and match again.
                            let slot = read(rec); let producer = read(ring.add(1));
                            let consumer = read(ring.add(2)); let cap = read(ring.add(3));
                            if cap != RX_CAPACITY || slot >= cap || producer >= cap || consumer >= cap
                                || (slot + cap - consumer) % cap >= (producer + cap - consumer) % cap {
                                write(STATE.add(1), 7);
                            }
                        }
                    }
                }
            }
        }
        if mask == 0 { core::arch::asm!("cpsie i", options(nostack)); }
    }
}

/// Return a tick and sequence only when the immediately consumed byte was the
/// matching selector edge. A zero sequence deliberately cancels the gesture.
pub unsafe fn tag(pressed: bool, original: u32) -> (u32, u32) {
    unsafe {
        if read(STATE) != MAGIC { return (original, 0); }
        let valid = read(STATE.add(1)) == 0 && read(STATE.add(5)) == 1
            && read(STATE.add(7)) == pressed as u32;
        write(STATE.add(5), 0);
        if !valid {
            (original, 0)
        } else {
            (read(STATE.add(6)), read(STATE.add(8)))
        }
    }
}

pub unsafe fn usable() -> bool {
    unsafe { read(STATE) == MAGIC && read(STATE.add(1)) == 0 && read(0x83e00900 as *const u32) == 0x49525154 }
}

pub unsafe fn reject_unexpected_event() {
    unsafe { if read(STATE) == MAGIC { write(STATE.add(1), 6); } }
}
