//! Export the current Rust generator's full-range bank for the host experiment.
#![allow(dead_code)]

#[path = "../../../patches/dual-efx/src/math.rs"]
mod math;
#[path = "../../../patches/dual-efx/src/velvet.rs"]
mod velvet;

fn main() {
    let mut effect = Box::new(velvet::VelvetDecorrelator::new());
    effect.init(velvet::Parameters {
        length_ms: 100.0,
        density: 4000.0,
        ..velvet::Parameters::default()
    });
    for channel in 0..2 {
        let bank = &effect.channels[channel].banks[effect.active_bank];
        assert_eq!(bank.count, 400);
        for (index, tap) in bank.storage[..bank.count].iter().enumerate() {
            println!("{channel} {index} {} {:.9e}", tap.delay, tap.gain);
        }
    }
}
