//! Velvet Noise Decorrelator (VND) DSP Engine
//!
//! Ported and adapted from 115dkk/Velvet-Noise-Decorrelator-VST3 for bare-metal
//! ARM Cortex-M7 target for Zoom LiveTrak L6max. The physical processor
//! model has not been identified.
//!
//! Generates a sparse, unit-energy FIR sequence of pseudo-random impulses
//! to decorrelate stereo audio channels without comb filtering or transient smearing.

use super::math::*;

pub const SAMPLE_RATE: f32 = 48000.0;
pub const MAX_DELAY_SAMPLES: usize = 4802; // Up to 100 ms at 48 kHz
pub const MAX_TAPS: usize = 64; // Plenty for 30–60 taps
const AMOUNT_FADE_SAMPLES: u32 = 64;

#[cold]
#[inline(never)]
fn wrap_out_of_range_delay(delay: usize) -> usize {
    delay % MAX_DELAY_SAMPLES
}

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

/// A separate mono send history using the decorrelator's filter banks.
pub(crate) struct MonoHistory {
    delay_line: [f32; MAX_DELAY_SAMPLES],
    write_position: usize,
    nonzero_samples: usize,
}

impl MonoHistory {
    pub(crate) const fn new() -> Self {
        Self { delay_line: [0.0; MAX_DELAY_SAMPLES], write_position: 0, nonzero_samples: 0 }
    }

    pub(crate) fn clear(&mut self) {
        self.delay_line.fill(0.0);
        self.write_position = 0;
        self.nonzero_samples = 0;
    }

    fn write(&mut self, sample: f32) {
        if self.delay_line[self.write_position] != 0.0 {
            self.nonzero_samples -= 1;
        }
        if sample != 0.0 {
            self.nonzero_samples += 1;
        }
        self.delay_line[self.write_position] = sample;
    }

    fn advance(&mut self) {
        self.write_position += 1;
        if self.write_position == MAX_DELAY_SAMPLES {
            self.write_position = 0;
        }
    }

    pub(crate) fn silence(&mut self) {
        self.write(0.0);
        self.advance();
    }
}

pub struct Channel {
    // Only process_sample/reset may change history, so nonzero_samples stays exact.
    delay_line: [f32; MAX_DELAY_SAMPLES],
    write_position: usize,
    nonzero_samples: usize,
    pub banks: [Bank; 2],
}

// One step writes or normalizes one tap per channel. The pending bank is never
// exposed to convolution until both channels have been completely built.
struct BankBuild {
    bank: usize,
    count: usize,
    index: usize,
    tail: usize,
    phase: u8, // 0 = ready, 1 = generate, 2 = normalize
    random: [u64; 2],
    previous: [u32; 2],
    energy: [f32; 2],
    scale: [f32; 2],
    grid: f32,
    decay_db: f32,
}

impl BankBuild {
    const fn new() -> Self {
        Self {
            bank: 0, count: 0, index: 0, tail: 0, phase: 0,
            random: [0; 2], previous: [0; 2], energy: [0.0; 2],
            scale: [0.0; 2], grid: 0.0, decay_db: 0.0,
        }
    }
}

impl Channel {
    pub const fn new() -> Self {
        Self {
            delay_line: [0.0; MAX_DELAY_SAMPLES],
            write_position: 0,
            nonzero_samples: 0,
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
    // The centered integer is exact in f64. Round it once to f32 and scale
    // by a power of two, avoiding software double arithmetic on Cortex-M7.
    ((bits as i64 - (1i64 << 52)) as f32) * (1.0 / 4503599627370496.0)
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
    // A filter edit during a crossfade is applied after that crossfade ends.
    pub filter_update_pending: bool,
    pub active_transition_samples: usize,
    amount_current: f32,
    amount_step: f32,
    amount_remaining: u32,
    build: BankBuild,
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
            filter_update_pending: false,
            active_transition_samples: 12000,
            amount_current: 1.0,
            amount_step: 0.0,
            amount_remaining: 0,
            build: BankBuild::new(),
        }
    }

    /// Initializes and computes the first set of sparse taps for both channels.
    pub fn init(&mut self, params: Parameters) {
        // Do not crossfade against banks from a previous initialization.
        self.channels[0].banks[0].count = 0;
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

        let filter_changed = params.length_ms != self.current.length_ms
            || params.density != self.current.density
            || params.decay_db != self.current.decay_db
            || params.seed != self.current.seed;
        if params.amount != self.current.amount {
            self.amount_step = (params.amount - self.amount_current) / AMOUNT_FADE_SAMPLES as f32;
            self.amount_remaining = AMOUNT_FADE_SAMPLES;
        }
        let initialized = self.channels[0].banks[0].count != 0;
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

        self.samples_until_transition = self.samples_until_transition.min(self.refresh_samples);
        if initialized && filter_changed {
            if self.transitioning {
                self.filter_update_pending = true;
            } else {
                self.start_build(self.pending_bank, self.generation);
                self.generation = self.generation.wrapping_add(1);
                self.filter_update_pending = true;
            }
        }
    }

    /// Clears history and rebuilds both banks for the current parameters.
    /// Bank generation is not suitable for a real-time audio callback.
    pub fn reset(&mut self) {
        self.amount_current = self.current.amount;
        self.amount_step = 0.0;
        self.amount_remaining = 0;
        self.start_build(0, 0);
        while self.build.phase != 0 {
            self.build_step();
        }
        self.start_build(1, 1);
        while self.build.phase != 0 {
            self.build_step();
        }
        self.generation = 2;
        for ch in &mut self.channels {
            ch.delay_line.fill(0.0);
            ch.write_position = 0;
            ch.nonzero_samples = 0;
        }
        self.transitioning = false;
        self.transition_position = 0;
        self.samples_until_transition = self.refresh_samples;
        self.active_bank = 0;
        self.pending_bank = 1;
        self.filter_update_pending = false;
        self.active_transition_samples = self.transition_samples;
    }

    fn start_build(&mut self, bank: usize, generation: u64) {
        let count = clamp_usize(
            (self.current.length_ms / 1000.0 * self.current.density + 0.5) as usize,
            2,
            MAX_TAPS.min(self.current_tail_samples + 1),
        );
        self.build = BankBuild {
            bank, count, index: 0, tail: self.current_tail_samples, phase: 1,
            random: [mixed_seed(self.current.seed, 0, generation),
                     mixed_seed(self.current.seed, 1, generation)],
            previous: [0; 2], energy: [0.0; 2], scale: [0.0; 2],
            grid: self.current_tail_samples as f32 / (count - 1) as f32,
            decay_db: self.current.decay_db,
        };
        for channel in &mut self.channels {
            channel.banks[bank].count = count;
        }
    }

    // Bounded work: at most one tap per channel, including normalization.
    fn build_step(&mut self) {
        let build = &mut self.build;
        let i = build.index;
        if build.phase == 1 {
            for c in 0..2 {
                // The reference starts with a positive impulse at zero delay.
                // It consumes no random numbers for that impulse.
                let delay = if i == 0 {
                    0
                } else {
                    let centre = build.grid * i as f32;
                    let jitter = uniform_signed(&mut build.random[c]) * build.grid * 0.45;
                    let lower = build.previous[c] + 1;
                    let upper = (build.tail - (build.count - 1 - i)) as u32;
                    let proposed = (centre + jitter + 0.5) as i32;
                    if proposed < lower as i32 { lower }
                    else if proposed > upper as i32 { upper }
                    else { proposed as u32 }
                };
                build.previous[c] = delay;
                let progress = delay as f32 / build.tail as f32;
                let envelope = pow10(build.decay_db * progress / 20.0);
                let sign = if i == 0 || splitmix64(&mut build.random[c]) & 1 != 0 {
                    1.0
                } else {
                    -1.0
                };
                let gain = sign * envelope;
                self.channels[c].banks[build.bank].storage[i] = Tap { delay, gain };
                build.energy[c] += gain * gain;
            }
            build.index += 1;
            if build.index == build.count {
                for c in 0..2 {
                    build.scale[c] = if build.energy[c] > 1e-12 {
                        1.0 / sqrt(build.energy[c])
                    } else {
                        1.0
                    };
                }
                build.index = 0;
                build.phase = 2;
            }
        } else if build.phase == 2 {
            for c in 0..2 {
                self.channels[c].banks[build.bank].storage[i].gain *= build.scale[c];
            }
            build.index += 1;
            if build.index == build.count {
                build.phase = 0;
            }
        }
    }

    #[inline(always)]
    fn convolve(bank: &Bank, delay_line: &[f32; MAX_DELAY_SAMPLES], wp: usize) -> f32 {
        let mut result = 0.0f32;
        let size = MAX_DELAY_SAMPLES;
        for tap in &bank.storage[..bank.count] {
            let delay = tap.delay as usize;
            let d = if delay < size { delay } else { wrap_out_of_range_delay(delay) };
            let read = if wp >= d { wp - d } else { wp + size - d };
            result += tap.gain * delay_line[read];
        }
        result
    }

    #[inline(always)]
    fn transition_weights(&self) -> (f32, f32) {
        if self.transitioning {
            let phase = if self.active_transition_samples <= 1 {
                PI_2
            } else {
                PI_2 * self.transition_position as f32 / (self.active_transition_samples - 1) as f32
            };
            (cos(phase), sin(phase))
        } else {
            (1.0, 0.0)
        }
    }

    #[inline(always)]
    fn convolve_pair(ca: &Channel, history: &MonoHistory, bank: usize) -> (f32, f32) {
        let wp = ca.write_position;
        let mut sum_a = 0.0f32;
        let mut sum_b = 0.0f32;
        let taps = &ca.banks[bank].storage[..ca.banks[bank].count];
        if ca.nonzero_samples != 0 && history.nonzero_samples != 0 && wp == history.write_position {
            for tap in taps {
                let delay = tap.delay as usize;
                let d = if delay < MAX_DELAY_SAMPLES { delay } else { wrap_out_of_range_delay(delay) };
                let read = if wp >= d { wp - d } else { wp + MAX_DELAY_SAMPLES - d };
                sum_a += tap.gain * ca.delay_line[read];
                sum_b += tap.gain * history.delay_line[read];
            }
        } else if ca.nonzero_samples != 0 && history.nonzero_samples != 0 {
            for tap in taps {
                let delay = tap.delay as usize;
                let d = if delay < MAX_DELAY_SAMPLES { delay } else { wrap_out_of_range_delay(delay) };
                let ra = if wp >= d { wp - d } else { wp + MAX_DELAY_SAMPLES - d };
                let rb = if history.write_position >= d { history.write_position - d } else { history.write_position + MAX_DELAY_SAMPLES - d };
                sum_a += tap.gain * ca.delay_line[ra];
                sum_b += tap.gain * history.delay_line[rb];
            }
        } else if ca.nonzero_samples != 0 {
            sum_a = Self::convolve(&ca.banks[bank], &ca.delay_line, wp);
        } else if history.nonzero_samples != 0 {
            sum_b = Self::convolve(&ca.banks[bank], &history.delay_line, history.write_position);
        }
        (sum_a, sum_b)
    }

    /// Processes a single stereo frame (in_l, in_r) -> (out_l, out_r).
    #[inline(always)]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let amount = self.advance_amount();
        let (old_weight, new_weight) = self.transition_weights();

        let inputs = [
            audio_input(in_l),
            audio_input(in_r),
        ];
        let mut outputs = [0.0f32; 2];

        for c in 0..2 {
            let dry = inputs[c];
            let wp = self.channels[c].write_position;
            if self.channels[c].delay_line[wp] != 0.0 {
                self.channels[c].nonzero_samples -= 1;
            }
            if dry != 0.0 {
                self.channels[c].nonzero_samples += 1;
            }
            self.channels[c].delay_line[wp] = dry;

            let wet = if self.channels[c].nonzero_samples == 0 {
                0.0
            } else {
                let mut wet = Self::convolve(&self.channels[c].banks[self.active_bank], &self.channels[c].delay_line, self.channels[c].write_position);
                if self.transitioning {
                    wet = old_weight * wet + new_weight * Self::convolve(&self.channels[c].banks[self.pending_bank], &self.channels[c].delay_line, self.channels[c].write_position);
                }
                wet
            };

            // In an Aux Send bus effect, return pure wet scaled by amount, soft-clipped to [-1.0, 1.0].
            outputs[c] = soft_clip(wet * amount);
        }

        for c in 0..2 {
            let mut wp = self.channels[c].write_position + 1;
            if wp >= MAX_DELAY_SAMPLES {
                wp = 0;
            }
            self.channels[c].write_position = wp;
        }

        self.advance_filter();
        (outputs[0], outputs[1])
    }

    /// Processes two independent mono histories using the same filter timeline.
    pub(crate) fn process_shared_mono(
        &mut self,
        history: &mut MonoHistory,
        in_a: f32,
        in_b: f32,
    ) -> ((f32, f32), (f32, f32)) {
        let amount = self.advance_amount();
        let (old_weight, new_weight) = self.transition_weights();
        let input_a = audio_input(in_a);
        let input_b = audio_input(in_b);
        history.write(input_b);
        let mut out_a = [0.0; 2];
        let mut out_b = [0.0; 2];
        for c in 0..2 {
            let ca = &mut self.channels[c];
            let wp = ca.write_position;
            if ca.delay_line[wp] != 0.0 {
                ca.nonzero_samples -= 1;
            }
            if input_a != 0.0 {
                ca.nonzero_samples += 1;
            }
            ca.delay_line[wp] = input_a;
            let (mut wa, mut wb) = Self::convolve_pair(ca, history, self.active_bank);
            if self.transitioning && (ca.nonzero_samples != 0 || history.nonzero_samples != 0) {
                let (na, nb) = Self::convolve_pair(ca, history, self.pending_bank);
                wa = old_weight * wa + new_weight * na;
                wb = old_weight * wb + new_weight * nb;
            }
            out_a[c] = soft_clip(wa * amount);
            out_b[c] = soft_clip(wb * amount);
            ca.write_position = if wp + 1 >= MAX_DELAY_SAMPLES { 0 } else { wp + 1 };
        }
        history.advance();
        self.advance_filter();
        ((out_a[0], out_a[1]), (out_b[0], out_b[1]))
    }

    #[inline(always)]
    fn advance_amount(&mut self) -> f32 {
        if self.amount_remaining != 0 {
            self.amount_remaining -= 1;
            if self.amount_remaining == 0 {
                self.amount_current = self.current.amount;
            } else {
                self.amount_current += self.amount_step;
            }
        }
        self.amount_current
    }

    // Progress a fade and prepare future taps without exposing a partial bank.
    fn advance_filter(&mut self) {
        if self.transitioning {
            self.transition_position += 1;
            if self.transition_position >= self.active_transition_samples {
                self.finish_transition();
            }
        } else {
            if self.current.dynamic && self.samples_until_transition > 0 {
                self.samples_until_transition -= 1;
            }
            if self.build.phase == 0 && (self.filter_update_pending
                || (self.current.dynamic && self.samples_until_transition == 0)) {
                self.filter_update_pending = false;
                self.transitioning = true;
                self.transition_position = 0;
                self.active_transition_samples = self.transition_samples;
            }
        }

        if self.build.phase != 0 {
            self.build_step();
        }
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

        self.start_build(self.pending_bank, self.generation);
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
