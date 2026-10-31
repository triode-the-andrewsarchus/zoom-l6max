//! Plateau / Dattorro Plate Reverb DSP Engine
//!
//! Adapted from Dale Johnson's Valley Plateau / PlateauNEVersio for bare-metal
//! ARM Cortex-M7 target for Zoom LiveTrak L6max. The physical processor
//! model has not been identified.
//!
//! Based on Jon Dattorro's 1997 AES paper "Effect Design Part 1: Reverberator".
//! Provides lush, high-density plate reverberation with modulated allpass loops,
//! dual tone damping, diffusion control, and infinite sustain freeze.

use super::math::*;

pub const SAMPLE_RATE: f32 = 48000.0;
const OUTPUT_FADE_SAMPLES: u32 = 64;
const DATTORRO_SR: f32 = 29761.0;
const SR_SCALE: f32 = SAMPLE_RATE / DATTORRO_SR; // ~1.61285

// Cover the supported maximum size (1.2), 190 ms predelay, modulation,
// and output tap offsets. Rings retain their entire usable history across edits.
const PREDELAY_MAX: usize = 9121;
const IN_APF1_MAX: usize = 274;
const IN_APF2_MAX: usize = 209;
const IN_APF3_MAX: usize = 735;
const IN_APF4_MAX: usize = 538;

const L_APF1_MAX: usize = 1332;
const L_DEL1_MAX: usize = 8620;
const L_APF2_MAX: usize = 3704; // Includes the 1913-sample output tap.
const L_DEL2_MAX: usize = 7201;

const R_APF1_MAX: usize = 1790;
const R_DEL1_MAX: usize = 8163;
const R_APF2_MAX: usize = 5142;
const R_DEL2_MAX: usize = 6123;

#[derive(Clone, Copy)]
pub struct Delay<const N: usize> {
    pub buffer: [f32; N],
    pub write_pos: usize,
    // Configured delay length; the complete backing ring keeps moving.
    pub length: usize,
}

impl<const N: usize> Delay<N> {
    pub const fn new() -> Self {
        Self {
            buffer: [0.0; N],
            write_pos: 0,
            length: N,
        }
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }

    #[inline(always)]
    pub fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
        self.write_pos += 1;
        if self.write_pos >= N {
            self.write_pos = 0;
        }
    }

    #[inline(always)]
    pub fn read(&self, delay: usize) -> f32 {
        self.read_history(if delay >= self.length { self.length - 1 } else { delay })
    }

    // The whole backing ring advances even when the configured delay is short.
    // Otherwise expanding it can replay samples from before the last shrink.
    #[inline(always)]
    fn read_history(&self, delay: usize) -> f32 {
        let read_pos = if self.write_pos >= delay {
            self.write_pos - delay
        } else {
            self.write_pos + N - delay
        };
        self.buffer[read_pos]
    }

    #[inline(always)]
    pub fn read_interpolated(&self, delay: f32) -> f32 {
        let d = clamp(delay, 0.0, (self.length - 2) as f32);
        let d_int = d as usize;
        let frac = d - d_int as f32;

        let r1 = if self.write_pos >= d_int {
            self.write_pos - d_int
        } else {
            self.write_pos + N - d_int
        };
        let r2 = if r1 == 0 { N - 1 } else { r1 - 1 };

        let s1 = self.buffer[r1];
        let s2 = self.buffer[r2];
        s1 + frac * (s2 - s1)
    }

    #[inline(always)]
    pub fn tap(&self, offset: usize) -> f32 {
        self.read(offset)
    }
}

pub struct Allpass<const N: usize> {
    pub delay: Delay<N>,
    pub gain: f32,
    pub delay_time: usize,
}

impl<const N: usize> Allpass<N> {
    pub const fn new(gain: f32, delay_time: usize) -> Self {
        Self {
            delay: Delay::new(),
            gain,
            delay_time,
        }
    }

    pub fn clear(&mut self) {
        self.delay.clear();
    }

    #[inline(always)]
    pub fn process(&mut self, input: f32) -> f32 {
        let delayed = self.delay.read(self.delay_time);
        let in_sum = input + delayed * self.gain;
        let output = delayed - in_sum * self.gain;
        self.delay.write(in_sum);
        output
    }

    #[inline(always)]
    pub fn process_modulated(&mut self, input: f32, mod_delay: f32) -> f32 {
        let delayed = self.delay.read_interpolated(mod_delay);
        let in_sum = input + delayed * self.gain;
        let output = delayed - in_sum * self.gain;
        self.delay.write(in_sum);
        output
    }
}

pub struct OnePoleLP {
    pub z: f32,
    pub a: f32,
    pub b: f32,
}

impl OnePoleLP {
    pub const fn new() -> Self {
        Self { z: 0.0, a: 1.0, b: 0.0 }
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        let c = clamp(cutoff_hz, 20.0, SAMPLE_RATE * 0.49);
        // For H(z) = a / (1 - b z^-1), a = 1 - b. Solve the
        // digital -3 dB condition at c, using sin(w/2) to avoid
        // cancellation near zero and a rationalized positive root.
        let s = sin(PI * c / SAMPLE_RATE);
        let a = (2.0 * s) / (sqrt(1.0 + s * s) + s);
        self.a = a;
        self.b = 1.0 - a;
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        self.z = self.a * x + self.b * self.z;
        self.z
    }

    pub fn clear(&mut self) {
        self.z = 0.0;
    }
}

pub struct OnePoleHP {
    previous_input: f32,
    previous_output: f32,
    pole: f32,
    gain: f32,
}

impl OnePoleHP {
    pub const fn new() -> Self {
        Self { previous_input: 0.0, previous_output: 0.0, pole: 0.0, gain: 1.0 }
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        let c = clamp(cutoff_hz, 20.0, SAMPLE_RATE * 0.49);
        // Bilinear one-pole high-pass with a prewarped cutoff. This keeps
        // the Nyquist passband at unity and the cutoff at -3 dB.
        let angle = PI * c / SAMPLE_RATE;
        let s = sin(angle);
        let c = cos(angle);
        self.pole = (c - s) / (c + s);
        self.gain = (1.0 + self.pole) * 0.5;
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let output = self.gain * (x - self.previous_input) + self.pole * self.previous_output;
        self.previous_input = x;
        self.previous_output = output;
        output
    }

    pub fn clear(&mut self) {
        self.previous_input = 0.0;
        self.previous_output = 0.0;
    }
}

pub struct TriangleLFO {
    pub phase: f32,
    pub phase_inc: f32,
}

impl TriangleLFO {
    pub const fn new(freq: f32) -> Self {
        Self {
            phase: 0.0,
            phase_inc: freq / SAMPLE_RATE,
        }
    }

    #[inline(always)]
    pub fn step(&mut self) -> f32 {
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        // Triangle wave on [-1, 1]
        if self.phase < 0.5 {
            4.0 * self.phase - 1.0
        } else {
            3.0 - 4.0 * self.phase
        }
    }
}

#[derive(Clone, Copy)]
pub struct PlateauParameters {
    pub predelay_ms: f32,     // 0.0 to 190.0 ms
    pub size: f32,             // 0.2 to 1.2 (time scale)
    pub decay: f32,            // 0.1 to 0.999 (tail duration)
    pub low_cut_hz: f32,       // 20.0 to 1000.0 Hz
    pub high_cut_hz: f32,      // 1000.0 to 18000.0 Hz
    pub diffusion: f32,        // 0.0 to 1.0
    pub mod_depth: f32,        // 0.0 to 16.0 samples
    pub freeze: bool,          // infinite sustain
    pub wet: f32,              // 0.0 to 1.0
}

impl Default for PlateauParameters {
    fn default() -> Self {
        Self {
            predelay_ms: 10.0,
            size: 1.0,
            decay: 0.85,
            low_cut_hz: 80.0,
            high_cut_hz: 6500.0,
            diffusion: 0.75,
            mod_depth: 8.0,
            freeze: false,
            wet: 1.0,
        }
    }
}

pub struct PlateauReverb {
    // Predelay
    pub predelay: Delay<PREDELAY_MAX>,
    pub predelay_samples: usize,

    // Input diffusers
    pub in_apf1: Allpass<IN_APF1_MAX>,
    pub in_apf2: Allpass<IN_APF2_MAX>,
    pub in_apf3: Allpass<IN_APF3_MAX>,
    pub in_apf4: Allpass<IN_APF4_MAX>,
    pub in_lp: OnePoleLP,
    pub in_hp: OnePoleHP,

    // Left tank loop
    pub l_apf1: Allpass<L_APF1_MAX>,
    pub l_del1: Delay<L_DEL1_MAX>,
    pub l_lp: OnePoleLP,
    pub l_hp: OnePoleHP,
    pub l_apf2: Allpass<L_APF2_MAX>,
    pub l_del2: Delay<L_DEL2_MAX>,
    pub l_sum: f32,

    // Right tank loop
    pub r_apf1: Allpass<R_APF1_MAX>,
    pub r_del1: Delay<R_DEL1_MAX>,
    pub r_lp: OnePoleLP,
    pub r_hp: OnePoleHP,
    pub r_apf2: Allpass<R_APF2_MAX>,
    pub r_del2: Delay<R_DEL2_MAX>,
    pub r_sum: f32,

    // Modulation
    pub lfo1: TriangleLFO,
    pub lfo2: TriangleLFO,

    // Output DC blocks
    pub out_hp_l: OnePoleHP,
    pub out_hp_r: OnePoleHP,

    // Short output transition masks discontinuities from live controls.
    transition_remaining: u32,
    transition_anchor: [f32; 2],
    previous_output: [f32; 2],
    pub params: PlateauParameters,
}

impl PlateauReverb {
    pub const fn new() -> Self {
        Self {
            predelay: Delay::new(),
            predelay_samples: 480,
            in_apf1: Allpass::new(0.75, 227),
            in_apf2: Allpass::new(0.75, 173),
            in_apf3: Allpass::new(0.625, 611),
            in_apf4: Allpass::new(0.625, 447),
            in_lp: OnePoleLP::new(),
            in_hp: OnePoleHP::new(),

            l_apf1: Allpass::new(-0.70, 1084),
            l_del1: Delay::new(),
            l_lp: OnePoleLP::new(),
            l_hp: OnePoleHP::new(),
            l_apf2: Allpass::new(0.50, 2903),
            l_del2: Delay::new(),
            l_sum: 0.0,

            r_apf1: Allpass::new(-0.70, 1465),
            r_del1: Delay::new(),
            r_lp: OnePoleLP::new(),
            r_hp: OnePoleHP::new(),
            r_apf2: Allpass::new(0.50, 4284),
            r_del2: Delay::new(),
            r_sum: 0.0,

            lfo1: TriangleLFO::new(0.10),
            lfo2: TriangleLFO::new(0.15),

            out_hp_l: OnePoleHP::new(),
            out_hp_r: OnePoleHP::new(),

            transition_remaining: 0,
            transition_anchor: [0.0; 2],
            previous_output: [0.0; 2],
            params: PlateauParameters {
                predelay_ms: 10.0,
                size: 1.0,
                decay: 0.85,
                low_cut_hz: 80.0,
                high_cut_hz: 6500.0,
                diffusion: 0.75,
                mod_depth: 8.0,
                freeze: false,
                wet: 1.0,
            },
        }
    }

    pub fn init(&mut self, params: PlateauParameters) {
        // These lengths and tank gains normally come from `new()`. Restore
        // them explicitly so initialization also works on zeroed storage.
        self.predelay.length = PREDELAY_MAX;
        self.in_apf1.delay.length = IN_APF1_MAX;
        self.in_apf2.delay.length = IN_APF2_MAX;
        self.in_apf3.delay.length = IN_APF3_MAX;
        self.in_apf4.delay.length = IN_APF4_MAX;
        self.l_apf1.delay.length = L_APF1_MAX;
        self.l_apf2.delay.length = L_APF2_MAX;
        self.r_apf1.delay.length = R_APF1_MAX;
        self.r_apf2.delay.length = R_APF2_MAX;
        self.l_apf1.gain = -0.70;
        self.r_apf1.gain = -0.70;
        self.l_apf2.gain = 0.50;
        self.r_apf2.gain = 0.50;
        self.lfo1 = TriangleLFO::new(0.10);
        self.lfo2 = TriangleLFO::new(0.15);
        self.out_hp_l.set_cutoff(20.0);
        self.out_hp_r.set_cutoff(20.0);
        self.set_parameters(params);
        self.clear();
    }

    pub fn clear(&mut self) {
        self.predelay.clear();
        self.in_apf1.clear();
        self.in_apf2.clear();
        self.in_apf3.clear();
        self.in_apf4.clear();
        self.in_lp.clear();
        self.in_hp.clear();

        self.l_apf1.clear();
        self.l_del1.clear();
        self.l_lp.clear();
        self.l_hp.clear();
        self.l_apf2.clear();
        self.l_del2.clear();
        self.l_sum = 0.0;

        self.r_apf1.clear();
        self.r_del1.clear();
        self.r_lp.clear();
        self.r_hp.clear();
        self.r_apf2.clear();
        self.r_del2.clear();
        self.r_sum = 0.0;

        self.out_hp_l.clear();
        self.out_hp_r.clear();
        self.transition_remaining = 0;
        self.transition_anchor = [0.0; 2];
        self.previous_output = [0.0; 2];
    }

    pub fn set_parameters(&mut self, mut params: PlateauParameters) {
        params.size = clamp(params.size, 0.2, 1.2);
        params.decay = clamp(params.decay, 0.1, 0.999);
        params.diffusion = clamp(params.diffusion, 0.0, 1.0);
        params.mod_depth = clamp(params.mod_depth, 0.0, 16.0);
        params.predelay_ms = clamp(params.predelay_ms, 0.0, 190.0);
        params.wet = clamp(params.wet, 0.0, 1.0);
        // Keep the published parameter values consistent with the cutoffs
        // actually applied by the one-pole filters.
        params.low_cut_hz = clamp(params.low_cut_hz, 20.0, SAMPLE_RATE * 0.49);
        params.high_cut_hz = clamp(params.high_cut_hz, 20.0, SAMPLE_RATE * 0.49);

        if self.params.size != params.size || self.params.freeze != params.freeze {
            self.begin_output_transition();
        }
        self.params = params;
        self.predelay_samples = ((params.predelay_ms / 1000.0) * SAMPLE_RATE) as usize;

        let scale = SR_SCALE * params.size;

        // Diffusers
        self.in_apf1.delay_time = clamp_usize((141.0 * scale) as usize, 1, IN_APF1_MAX - 1);
        self.in_apf2.delay_time = clamp_usize((107.0 * scale) as usize, 1, IN_APF2_MAX - 1);
        self.in_apf3.delay_time = clamp_usize((379.0 * scale) as usize, 1, IN_APF3_MAX - 1);
        self.in_apf4.delay_time = clamp_usize((277.0 * scale) as usize, 1, IN_APF4_MAX - 1);

        self.in_apf1.gain = 0.75 * params.diffusion;
        self.in_apf2.gain = 0.75 * params.diffusion;
        self.in_apf3.gain = 0.625 * params.diffusion;
        self.in_apf4.gain = 0.625 * params.diffusion;

        // Tank delays
        self.l_apf1.delay_time = clamp_usize((672.0 * scale) as usize, 1, L_APF1_MAX - 32);
        self.l_del1.length = clamp_usize((4453.0 * scale) as usize, 10, L_DEL1_MAX - 1);
        self.l_apf2.delay_time = clamp_usize((1800.0 * scale) as usize, 1, L_APF2_MAX - 1);
        self.l_del2.length = clamp_usize((3720.0 * scale) as usize, 10, L_DEL2_MAX - 1);

        self.r_apf1.delay_time = clamp_usize((908.0 * scale) as usize, 1, R_APF1_MAX - 32);
        self.r_del1.length = clamp_usize((4217.0 * scale) as usize, 10, R_DEL1_MAX - 1);
        self.r_apf2.delay_time = clamp_usize((2656.0 * scale) as usize, 1, R_APF2_MAX - 1);
        self.r_del2.length = clamp_usize((3163.0 * scale) as usize, 10, R_DEL2_MAX - 1);

        // The input and both tank channels use the same coefficients, but
        // each filter retains its own signal history.
        self.in_lp.set_cutoff(params.high_cut_hz);
        self.in_hp.set_cutoff(params.low_cut_hz);
        self.l_lp.a = self.in_lp.a;
        self.l_lp.b = self.in_lp.b;
        self.r_lp.a = self.in_lp.a;
        self.r_lp.b = self.in_lp.b;
        self.l_hp.pole = self.in_hp.pole;
        self.l_hp.gain = self.in_hp.gain;
        self.r_hp.pole = self.in_hp.pole;
        self.r_hp.gain = self.in_hp.gain;
    }

    fn begin_output_transition(&mut self) {
        self.transition_anchor = self.previous_output;
        self.transition_remaining = OUTPUT_FADE_SAMPLES;
    }

    // The audio callback updates these controls without rebuilding delays or
    // filters; a freeze edge needs the same output transition as the full setter.
    pub(crate) fn set_live_parameters(&mut self, params: PlateauParameters) {
        if self.params.freeze != params.freeze {
            self.begin_output_transition();
        }
        self.params.decay = clamp(params.decay, 0.1, 0.999);
        self.params.mod_depth = clamp(params.mod_depth, 0.0, 16.0);
        self.params.freeze = params.freeze;
    }

    #[inline(always)]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        // Do not let corrupt samples poison the feedback tank.
        let in_l = audio_input(in_l);
        let in_r = audio_input(in_r);
        // 1. Predelay & Input Sum
        let mono_in = 0.5 * in_l + 0.5 * in_r;
        let predelayed = if self.predelay_samples == 0 {
            mono_in
        } else {
            self.predelay.read(self.predelay_samples)
        };
        // Advance the ring during freeze without retaining audio received
        // while input to the tank is disabled.
        self.predelay.write(if self.params.freeze { 0.0 } else { mono_in });

        // Stop feeding the input filters while frozen; let their history drain.
        let tank_in = if self.params.freeze { 0.0 } else { predelayed };

        // 2. Input Diffusers
        let filtered_in = self.in_hp.process(self.in_lp.process(tank_in));
        let diff1 = self.in_apf1.process(filtered_in);
        let diff2 = self.in_apf2.process(diff1);
        let diff3 = self.in_apf3.process(diff2);
        let diffused = self.in_apf4.process(diff3);
        // The diffusers may still contain pre-freeze audio. Do not inject it
        // into the frozen feedback tank while that history drains.
        let diff_in = if self.params.freeze { 0.0 } else { diffused };

        // 3. Modulations
        let lfo1_val = self.lfo1.step();
        let lfo2_val = self.lfo2.step();
        // Interpolating a moving delay dissipates energy in the feedback
        // loop. Hold it at an integer delay during freeze while advancing the
        // LFOs, so the tank sustains and modulation resumes on release.
        let depth = if self.params.freeze { 0.0 } else { self.params.mod_depth };
        let l_mod = self.l_apf1.delay_time as f32 + lfo1_val * depth;
        let r_mod = self.r_apf1.delay_time as f32 + lfo2_val * depth;

        let decay = if self.params.freeze { 1.0 } else { self.params.decay };

        // 4. Left Tank Loop
        self.l_sum += diff_in;
        let l_ap1_out = self.l_apf1.process_modulated(self.l_sum, l_mod);
        // Read the configured delay from continuously advancing history.
        let l_del1_out = self.l_del1.read_history(self.l_del1.length);
        self.l_del1.write(l_ap1_out);

        // Keep damping history current while bypassed so releasing freeze
        // does not reintroduce samples from before it was engaged.
        let l_damped = self.l_hp.process(self.l_lp.process(l_del1_out));
        let l_filtered = if self.params.freeze { l_del1_out } else { l_damped };

        let l_ap2_out = self.l_apf2.process(l_filtered * decay);
        let l_del2_out = self.l_del2.read_history(self.l_del2.length);
        self.l_del2.write(l_ap2_out);

        // 5. Right Tank Loop
        self.r_sum += diff_in;
        let r_ap1_out = self.r_apf1.process_modulated(self.r_sum, r_mod);
        let r_del1_out = self.r_del1.read_history(self.r_del1.length);
        self.r_del1.write(r_ap1_out);

        let r_damped = self.r_hp.process(self.r_lp.process(r_del1_out));
        let r_filtered = if self.params.freeze { r_del1_out } else { r_damped };

        let r_ap2_out = self.r_apf2.process(r_filtered * decay);
        let r_del2_out = self.r_del2.read_history(self.r_del2.length);
        self.r_del2.write(r_ap2_out);

        // Cross feedback
        self.l_sum = r_del2_out * decay;
        self.r_sum = l_del2_out * decay;

        // 6. Output Taps Summation (Dattorro ratios scaled)
        let scale = SR_SCALE * self.params.size;
        let tap = |val: f32| (val * scale) as usize;

        let mut out_l = l_ap1_out
            + self.l_del1.tap(tap(266.0))
            + self.l_del1.tap(tap(2974.0))
            - self.l_apf2.delay.tap(tap(1913.0))
            + self.l_del2.tap(tap(1996.0))
            - self.r_del1.tap(tap(1990.0))
            - self.r_apf2.delay.tap(tap(187.0))
            - self.r_del2.tap(tap(1066.0));

        let mut out_r = r_ap1_out
            + self.r_del1.tap(tap(353.0))
            + self.r_del1.tap(tap(3627.0))
            - self.r_apf2.delay.tap(tap(1228.0))
            + self.r_del2.tap(tap(2673.0))
            - self.l_del1.tap(tap(2111.0))
            - self.l_apf2.delay.tap(tap(335.0))
            - self.l_del2.tap(tap(121.0));

        // DC blocking highpass
        out_l = self.out_hp_l.process(out_l * 0.6);
        out_r = self.out_hp_r.process(out_r * 0.6);

        // Continuous cubic soft clipper to prevent harsh digital clipping
        out_l = soft_clip(out_l);
        out_r = soft_clip(out_r);
        if self.transition_remaining != 0 {
            let fade = (OUTPUT_FADE_SAMPLES - self.transition_remaining + 1) as f32
                / OUTPUT_FADE_SAMPLES as f32;
            out_l = self.transition_anchor[0] * (1.0 - fade) + out_l * fade;
            out_r = self.transition_anchor[1] * (1.0 - fade) + out_r * fade;
            self.transition_remaining -= 1;
        }
        self.previous_output = [out_l, out_r];
        (out_l * self.params.wet, out_r * self.params.wet)
    }

    // The caller must guarantee all histories and filter state are zero and
    // revoke that guarantee on arbitrary mutable access. Advancing cursors and
    // LFOs preserves the state needed when processing resumes.
    pub(crate) fn advance_pristine(&mut self) {
        macro_rules! advance {
            ($d:expr) => {{
                let d = &mut $d;
                d.write_pos += 1;
                if d.write_pos >= d.buffer.len() {
                    d.write_pos = 0;
                }
            }};
        }
        advance!(self.predelay);
        advance!(self.in_apf1.delay);
        advance!(self.in_apf2.delay);
        advance!(self.in_apf3.delay);
        advance!(self.in_apf4.delay);
        advance!(self.l_apf1.delay);
        advance!(self.l_del1);
        advance!(self.l_apf2.delay);
        advance!(self.l_del2);
        advance!(self.r_apf1.delay);
        advance!(self.r_del1);
        advance!(self.r_apf2.delay);
        advance!(self.r_del2);
        self.lfo1.step();
        self.lfo2.step();
        self.transition_remaining = self.transition_remaining.saturating_sub(1);
    }

    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let count = left.len().min(right.len());
        for i in 0..count {
            let (out_l, out_r) = self.process_sample(left[i], right[i]);
            left[i] = out_l;
            right[i] = out_r;
        }
    }
}
