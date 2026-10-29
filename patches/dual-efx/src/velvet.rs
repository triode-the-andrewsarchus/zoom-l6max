//! Velvet Noise Decorrelator (VND) DSP Engine
//!
//! Ported and adapted from 115dkk/Velvet-Noise-Decorrelator-VST3 for bare-metal
//! ARM Cortex-M7 (NXP i.MX RT1040/RT1041 on Zoom LiveTrak L6max).
//!
//! Generates a sparse, unit-energy FIR sequence of pseudo-random impulses
//! to decorrelate stereo audio channels without comb filtering or transient smearing.

use super::math::*;

pub const SAMPLE_RATE: f32 = 48000.0;
pub const MAX_DELAY_SAMPLES: usize = 4802; // Up to 100 ms at 48 kHz
pub const MAX_TAPS: usize = 64; // Plenty for 30–60 taps

#[derive(Clone, Copy)]
pub struct Tap {
    pub delay: u32,
    pub gain: f32,
}

impl Tap {
    pub const fn zero() -> Self {
        Self { delay: 0, gain: 0.0 }
    }
}

#[derive(Clone, Copy)]
pub struct Bank {
    pub storage: [Tap; MAX_TAPS],
    pub count: usize,
}

impl Bank {
    pub const fn new() -> Self {
        Self {
            storage: [Tap::zero(); MAX_TAPS],
            count: 0,
        }
    }
}

pub struct Channel {
    pub delay_line: [f32; MAX_DELAY_SAMPLES],
    pub write_position: usize,
    pub banks: [Bank; 2],
}

impl Channel {
    pub const fn new() -> Self {
        Self {
            delay_line: [0.0; MAX_DELAY_SAMPLES],
            write_position: 0,
            banks: [Bank::new(), Bank::new()],
        }
    }
}

#[derive(Clone, Copy)]
pub struct Parameters {
    pub dynamic: bool,
    pub amount: f32,          // 0.0 to 1.0 (wet mix)
    pub length_ms: f32,       // 5.0 to 80.0 ms (default ~27.56 ms)
    pub density: f32,         // pulses per second (default ~1088.4)
    pub refresh_seconds: f32, // time between renewals (default 5.0 s)
    pub transition_ms: f32,   // crossfade time (default 250.0 ms)
    pub decay_db: f32,        // end-of-filter decay (default -60.0 dB)
    pub seed: u64,            // deterministic base seed
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            dynamic: true,
            amount: 1.0,
            length_ms: 27.5625,
            density: 1088.435,
            refresh_seconds: 5.0,
            transition_ms: 250.0,
            decay_db: -60.0,
            seed: 2050083136,
        }
    }
}

/// SplitMix64 PRNG generator.
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// Generates a signed uniform float in [-1.0, 1.0].
pub fn uniform_signed(state: &mut u64) -> f32 {
    let bits = splitmix64(state) >> 11;
    (bits as f64 * (1.0 / 9007199254740992.0) * 2.0 - 1.0) as f32
}

/// Mixes base seed with channel index and generation count.
pub fn mixed_seed(seed: u64, channel: u64, generation: u64) -> u64 {
    let state = seed
        ^ (channel.wrapping_add(1)).wrapping_mul(0xd6e8feb86659fd93)
        ^ (generation.wrapping_add(1)).wrapping_mul(0xa5a3564e27f8862f);
    let mut s = state;
    splitmix64(&mut s)
}

pub struct VelvetDecorrelator {
    pub channels: [Channel; 2], // Stereo (Left, Right)
    pub current: Parameters,
    pub current_tail_samples: usize,
    pub refresh_samples: usize,
    pub transition_samples: usize,
    pub samples_until_transition: usize,
    pub transition_position: usize,
    pub generation: u64,
    pub active_bank: usize,
    pub pending_bank: usize,
    pub transitioning: bool,
}

impl VelvetDecorrelator {
    pub const fn new() -> Self {
        Self {
            channels: [Channel::new(), Channel::new()],
            current: Parameters {
                dynamic: true,
                amount: 1.0,
                length_ms: 27.5625,
                density: 1088.435,
                refresh_seconds: 5.0,
                transition_ms: 250.0,
                decay_db: -60.0,
                seed: 2050083136,
            },
            current_tail_samples: 1323,
            refresh_samples: 240000,
            transition_samples: 12000,
            samples_until_transition: 240000,
            transition_position: 0,
            generation: 2,
            active_bank: 0,
            pending_bank: 1,
            transitioning: false,
        }
    }

    /// Initializes and computes the first set of sparse taps for both channels.
    pub fn init(&mut self, params: Parameters) {
        self.set_parameters(params);
        self.reset();
    }

    pub fn set_parameters(&mut self, mut params: Parameters) {
        params.amount = clamp(params.amount, 0.0, 1.0);
        params.length_ms = clamp(params.length_ms, 1.0, 95.0);
        params.density = clamp(params.density, 10.0, 3000.0);
        params.refresh_seconds = clamp(params.refresh_seconds, 0.1, 60.0);
        params.transition_ms = clamp(params.transition_ms, 1.0, params.refresh_seconds * 900.0);
        params.decay_db = clamp(params.decay_db, -120.0, 0.0);

        self.current = params;
        self.current_tail_samples = clamp_usize(
            (SAMPLE_RATE * params.length_ms / 1000.0) as usize,
            1,
            MAX_DELAY_SAMPLES - 2,
        );

        self.refresh_samples = (SAMPLE_RATE * params.refresh_seconds) as usize;
        if self.refresh_samples == 0 {
            self.refresh_samples = 1;
        }

        self.transition_samples = (SAMPLE_RATE * (params.transition_ms / 1000.0)) as usize;
        if self.transition_samples == 0 {
            self.transition_samples = 1;
        }

        self.active_bank = 0;
        self.pending_bank = 1;
        self.transitioning = false;
        self.transition_position = 0;
        self.samples_until_transition = self.refresh_samples;
        self.generation = 2;

        for c in 0..2 {
            self.generate_bank(c, 0, 0);
            self.generate_bank(c, 1, 1);
        }
    }

    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.delay_line.fill(0.0);
            ch.write_position = 0;
        }
        self.transitioning = false;
        self.transition_position = 0;
        self.samples_until_transition = self.refresh_samples;
        self.active_bank = 0;
        self.pending_bank = 1;
    }

    /// Generates a sparse impulse train into `channel.banks[bank_index]`.
    fn generate_bank(&mut self, channel_index: usize, bank_index: usize, bank_gen: u64) {
        let seconds = self.current.length_ms / 1000.0;
        let mut count = (seconds * self.current.density + 0.5) as usize;
        if count < 2 {
            count = 2;
        }
        if count > MAX_TAPS {
            count = MAX_TAPS;
        }
        if count > self.current_tail_samples + 1 {
            count = self.current_tail_samples + 1;
        }

        let mut random = mixed_seed(self.current.seed, channel_index as u64, bank_gen);
        let mut previous: u32 = 0;
        let mut energy = 0.0f32;
        let grid = self.current_tail_samples as f32 / (count - 1) as f32;

        let bank = &mut self.channels[channel_index].banks[bank_index];
        bank.count = count;

        for i in 0..count {
            let centre = grid * (i as f32 + 0.5);
            let jitter = uniform_signed(&mut random) * grid * 0.45;
            let lower = previous + if i == 0 { 0 } else { 1 };
            let upper = (self.current_tail_samples - (count - 1 - i)) as u32;
            let proposed = (centre + jitter + 0.5) as i32;
            let delay = if proposed < lower as i32 {
                lower
            } else if proposed > upper as i32 {
                upper
            } else {
                proposed as u32
            };
            previous = delay;

            let progress = delay as f32 / self.current_tail_samples as f32;
            let envelope = pow10(self.current.decay_db * progress / 20.0);
            let sign = if (splitmix64(&mut random) & 1) != 0 {
                1.0
            } else {
                -1.0
            };
            let gain = sign * envelope;

            bank.storage[i] = Tap { delay, gain };
            energy += gain * gain;
        }

        let scale = if energy > 1e-12 {
            1.0 / sqrt(energy)
        } else {
            1.0
        };

        for i in 0..count {
            bank.storage[i].gain *= scale;
        }
    }

    #[inline(always)]
    fn convolve(&self, channel: &Channel, bank_index: usize) -> f32 {
        let bank = &channel.banks[bank_index];
        let mut result = 0.0f32;
        let size = MAX_DELAY_SAMPLES;
        let wp = channel.write_position;

        for i in 0..bank.count {
            let tap = &bank.storage[i];
            let d = tap.delay as usize % size;
            let read = if wp >= d { wp - d } else { wp + size - d };
            result += tap.gain * channel.delay_line[read];
        }
        result
    }

    /// Processes a single stereo frame (in_l, in_r) -> (out_l, out_r).
    #[inline(always)]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let mut old_weight = 1.0f32;
        let mut new_weight = 0.0f32;

        if self.transitioning {
            let phase = if self.transition_samples <= 1 {
                PI_2
            } else {
                PI_2 * (self.transition_position as f32) / ((self.transition_samples - 1) as f32)
            };
            old_weight = cos(phase);
            new_weight = sin(phase);
        }

        let inputs = [in_l, in_r];
        let mut outputs = [0.0f32; 2];

        for c in 0..2 {
            let dry = inputs[c];
            let wp = self.channels[c].write_position;
            self.channels[c].delay_line[wp] = dry;

            let mut wet = self.convolve(&self.channels[c], self.active_bank);
            if self.transitioning {
                wet = old_weight * wet + new_weight * self.convolve(&self.channels[c], self.pending_bank);
            }

            // In an Aux Send bus effect, return pure wet scaled by amount, soft-clipped to [-1.0, 1.0].
            outputs[c] = soft_clip(wet * self.current.amount);
        }

        for c in 0..2 {
            let mut wp = self.channels[c].write_position + 1;
            if wp >= MAX_DELAY_SAMPLES {
                wp = 0;
            }
            self.channels[c].write_position = wp;
        }

        if self.current.dynamic {
            if self.transitioning {
                self.transition_position += 1;
                if self.transition_position >= self.transition_samples {
                    self.finish_transition();
                }
            } else if self.samples_until_transition > 0 {
                self.samples_until_transition -= 1;
                if self.samples_until_transition == 0 {
                    self.transitioning = true;
                    self.transition_position = 0;
                }
            }
        }

        (outputs[0], outputs[1])
    }

    fn finish_transition(&mut self) {
        self.active_bank = self.pending_bank;
        self.pending_bank = 1 - self.active_bank;
        self.transitioning = false;
        self.transition_position = 0;
        self.samples_until_transition = if self.refresh_samples > self.transition_samples {
            self.refresh_samples - self.transition_samples
        } else {
            1
        };

        for c in 0..2 {
            self.generate_bank(c, self.pending_bank, self.generation);
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// Process a block of stereo frames in-place or from buffer.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let count = left.len().min(right.len());
        for i in 0..count {
            let (out_l, out_r) = self.process_sample(left[i], right[i]);
            left[i] = out_l;
            right[i] = out_r;
        }
    }
}
