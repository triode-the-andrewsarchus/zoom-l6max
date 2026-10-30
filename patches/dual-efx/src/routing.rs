//! Dual-Effect Routing and Signal Flow Matrix
//!
//! Handles per-channel send accumulation, cascading (A -> B),
//! optional Pre-AINR (noise reduction), and summing to the master return bus.

use super::plateau::{PlateauParameters, PlateauReverb};
use super::velvet::{Parameters as VelvetParameters, VelvetDecorrelator};

pub const MAX_CHANNELS: usize = 12; // 8 mono/stereo channels + USB/pads
pub const BLOCK_SIZE: usize = 64;   // Max block size processed per callback

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RoutingMode {
    Neither = 0,
    SendA = 1,     // Velvet Noise Decorrelator
    SendB = 2,     // Plateau Plate Reverb
    SendAtoB = 3,  // Velvet Noise cascaded into Plateau Reverb
}

impl RoutingMode {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::SendA,
            2 => Self::SendB,
            3 => Self::SendAtoB,
            _ => Self::Neither,
        }
    }
}

pub struct DualEfxProcessor {
    pub velvet: VelvetDecorrelator,
    // A separate history keeps A-only sends out of the A-to-B cascade.
    pub velvet_cascade: VelvetDecorrelator,
    pub plateau: PlateauReverb,

    // Per-channel routing modes (12 mixer channels)
    pub channel_routing: [RoutingMode; MAX_CHANNELS],
    // Per-channel send levels (0.0 to 1.0)
    pub channel_send: [f32; MAX_CHANNELS],

    // Global cascade amount (0.0 to 1.0) for SendAtoB channels
    pub cascade_gain: f32,
    // Effect return levels
    pub return_gain_a: f32,
    pub return_gain_b: f32,

    // Optional Pre-AINR enable flag
    pub pre_ainr_enabled: bool,
}

impl DualEfxProcessor {
    pub const fn new() -> Self {
        Self {
            velvet: VelvetDecorrelator::new(),
            velvet_cascade: VelvetDecorrelator::new(),
            plateau: PlateauReverb::new(),
            channel_routing: [RoutingMode::Neither; MAX_CHANNELS],
            channel_send: [0.0; MAX_CHANNELS],
            cascade_gain: 0.0,
            return_gain_a: 1.0,
            return_gain_b: 1.0,
            pre_ainr_enabled: false,
        }
    }

    pub fn init(&mut self) {
        self.channel_routing.fill(RoutingMode::Neither);
        self.channel_send.fill(0.0);
        self.cascade_gain = 0.0;
        self.return_gain_a = 1.0;
        self.return_gain_b = 1.0;
        self.pre_ainr_enabled = false;
        self.velvet.init(VelvetParameters::default());
        self.velvet_cascade.init(VelvetParameters::default());
        self.plateau.init(PlateauParameters::default());
    }

    pub fn set_channel_routing(&mut self, channel: usize, mode: RoutingMode) {
        if channel < MAX_CHANNELS {
            self.channel_routing[channel] = mode;
        }
    }

    pub fn set_channel_send(&mut self, channel: usize, level: f32) {
        if channel < MAX_CHANNELS {
            self.channel_send[channel] = if level < 0.0 { 0.0 } else if level > 1.0 { 1.0 } else { level };
        }
    }

    /// Processes a block of audio frames.
    ///
    /// - `channels`: slice of channel input buffers [ch][frame].
    /// - `out_l`, `out_r`: destination buffers for the wet effect return to master bus.
    pub fn process_block(
        &mut self,
        channels: &[&[f32]],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        let frames = out_l.len().min(out_r.len());
        if frames == 0 {
            return;
        }

        // Send accumulation buffers
        let mut send_a_l = [0.0f32; BLOCK_SIZE];
        let mut send_a_r = [0.0f32; BLOCK_SIZE];
        let mut send_b_l = [0.0f32; BLOCK_SIZE];
        let mut send_b_r = [0.0f32; BLOCK_SIZE];
        let mut send_cascade_l = [0.0f32; BLOCK_SIZE];
        let mut send_cascade_r = [0.0f32; BLOCK_SIZE];

        let num_channels = channels.len().min(MAX_CHANNELS);

        // 1. Accumulate channel audio into Send A and Send B buses
        for ch in 0..num_channels {
            let send_level = self.channel_send[ch];
            if send_level <= 1e-4 {
                continue;
            }

            let mode = self.channel_routing[ch];
            let ch_data = channels[ch];
            let n = frames.min(ch_data.len()).min(BLOCK_SIZE);

            match mode {
                RoutingMode::Neither => {}
                RoutingMode::SendA => {
                    for i in 0..n {
                        let sample = ch_data[i] * send_level;
                        send_a_l[i] += sample;
                        send_a_r[i] += sample;
                    }
                }
                RoutingMode::SendB => {
                    for i in 0..n {
                        let sample = ch_data[i] * send_level;
                        send_b_l[i] += sample;
                        send_b_r[i] += sample;
                    }
                }
                RoutingMode::SendAtoB => {
                    for i in 0..n {
                        let sample = ch_data[i] * send_level;
                        send_cascade_l[i] += sample;
                        send_cascade_r[i] += sample;
                    }
                }
            }
        }

        // 2. Process Effect A (Velvet Noise Decorrelator)
        let mut wet_a_l = [0.0f32; BLOCK_SIZE];
        let mut wet_a_r = [0.0f32; BLOCK_SIZE];

        let n = frames.min(BLOCK_SIZE);
        self.velvet_cascade.set_parameters(self.velvet.current);
        for i in 0..n {
            let (l, r) = self.velvet.process_sample(send_a_l[i], send_a_r[i]);
            let (cascade_l, cascade_r) = self.velvet_cascade.process_sample(
                send_cascade_l[i], send_cascade_r[i],
            );
            wet_a_l[i] = l + cascade_l;
            wet_a_r[i] = r + cascade_r;
            send_b_l[i] += cascade_l * self.cascade_gain;
            send_b_r[i] += cascade_r * self.cascade_gain;
        }

        // 3. Process Effect B (Plateau Plate Reverb)
        let mut wet_b_l = [0.0f32; BLOCK_SIZE];
        let mut wet_b_r = [0.0f32; BLOCK_SIZE];

        for i in 0..n {
            let (l, r) = self.plateau.process_sample(send_b_l[i], send_b_r[i]);
            wet_b_l[i] = l;
            wet_b_r[i] = r;
        }

        // 4. Sum to Master Return with soft clipping to protect downstream DAC
        for i in 0..n {
            out_l[i] = super::math::soft_clip(wet_a_l[i] * self.return_gain_a + wet_b_l[i] * self.return_gain_b);
            out_r[i] = super::math::soft_clip(wet_a_r[i] * self.return_gain_a + wet_b_r[i] * self.return_gain_b);
        }
    }

    /// Processes a stereo send block through the dual effect engine.
    /// Used when the stock mixer has already summed channel sends into a stereo bus.
    pub fn process_stereo_send(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        enable_a: bool,
        enable_b: bool,
    ) {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len()).min(BLOCK_SIZE);
        if n == 0 {
            return;
        }

        let mut a_out_l = [0.0f32; BLOCK_SIZE];
        let mut a_out_r = [0.0f32; BLOCK_SIZE];

        if enable_a {
            for i in 0..n {
                let (l, r) = self.velvet.process_sample(in_l[i], in_r[i]);
                a_out_l[i] = l;
                a_out_r[i] = r;
            }
        }

        let mut b_in_l = [0.0f32; BLOCK_SIZE];
        let mut b_in_r = [0.0f32; BLOCK_SIZE];

        for i in 0..n {
            let cascade_l = if enable_a && self.cascade_gain > 1e-4 { a_out_l[i] * self.cascade_gain } else { 0.0 };
            let cascade_r = if enable_a && self.cascade_gain > 1e-4 { a_out_r[i] * self.cascade_gain } else { 0.0 };
            b_in_l[i] = in_l[i] + cascade_l;
            b_in_r[i] = in_r[i] + cascade_r;
        }

        let mut b_out_l = [0.0f32; BLOCK_SIZE];
        let mut b_out_r = [0.0f32; BLOCK_SIZE];

        if enable_b {
            for i in 0..n {
                let (l, r) = self.plateau.process_sample(b_in_l[i], b_in_r[i]);
                b_out_l[i] = l;
                b_out_r[i] = r;
            }
        }

        for i in 0..n {
            let val_a_l = if enable_a { a_out_l[i] * self.return_gain_a } else { 0.0 };
            let val_a_r = if enable_a { a_out_r[i] * self.return_gain_a } else { 0.0 };
            let val_b_l = if enable_b { b_out_l[i] * self.return_gain_b } else { 0.0 };
            let val_b_r = if enable_b { b_out_r[i] * self.return_gain_b } else { 0.0 };
            out_l[i] = super::math::soft_clip(val_a_l + val_b_l);
            out_r[i] = super::math::soft_clip(val_a_r + val_b_r);
        }
    }
}
