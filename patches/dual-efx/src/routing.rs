//! Dual-Effect Routing and Signal Flow Matrix
//!
//! Handles per-channel send accumulation, cascading (A -> B),
//! optional Pre-AINR (noise reduction), and summing to the master return bus.

use super::plateau::{PlateauParameters, PlateauReverb};
use super::velvet::{Parameters as VelvetParameters, VelvetDecorrelator};

pub const MAX_CHANNELS: usize = 12; // 8 mono/stereo channels + USB/pads
pub const BLOCK_SIZE: usize = 64; // Max block size processed per callback

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RoutingMode {
    Neither = 0,
    SendA = 1,    // Velvet Noise Decorrelator
    SendB = 2,    // Plateau Plate Reverb
    SendAtoB = 3, // Velvet Noise cascaded into Plateau Reverb
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

    /// Constructs and initializes the processor in caller-owned storage.
    ///
    /// The storage must be reserved exclusively for this processor for the
    /// returned borrow's lifetime. Initialization clears hundreds of KiB and
    /// generates filter banks, so it must not run in a real-time callback.
    pub fn initialize_in_place(storage: &mut core::mem::MaybeUninit<Self>) -> &mut Self {
        // All fields admit the all-zero representation: floats and integers,
        // false bools, and RoutingMode::Neither. No typed reference may be
        // created until the bytes are valid. Keep this invariant current when
        // adding fields to this type or either DSP engine.
        unsafe {
            core::ptr::write_bytes(storage.as_mut_ptr(), 0, 1);
            let processor = storage.assume_init_mut();
            processor.init();
            processor
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
            self.channel_send[channel] = if level.is_nan() || level < 0.0 {
                0.0
            } else if level > 1.0 {
                1.0
            } else {
                level
            };
        }
    }

    /// Processes a block of audio frames.
    ///
    /// - `channels`: slice of channel input buffers [ch][frame].
    /// - `out_l`, `out_r`: destination buffers for the wet effect return to master bus.
    pub fn process_block(&mut self, channels: &[&[f32]], out_l: &mut [f32], out_r: &mut [f32]) {
        let frames = out_l.len().min(out_r.len());
        let mut offset = 0;
        while offset < frames {
            // Each physical input is mono here, so accumulate it only once per bus.
            let mut send_a = [0.0f32; BLOCK_SIZE];
            let mut send_b = [0.0f32; BLOCK_SIZE];
            let mut send_cascade = [0.0f32; BLOCK_SIZE];
            let n = (frames - offset).min(BLOCK_SIZE);

            for ch in 0..channels.len().min(MAX_CHANNELS) {
                let send_level = self.channel_send[ch];
                if send_level <= 0.0 {
                    continue;
                }
                let bus = match self.channel_routing[ch] {
                    RoutingMode::Neither => continue,
                    RoutingMode::SendA => &mut send_a,
                    RoutingMode::SendB => &mut send_b,
                    RoutingMode::SendAtoB => &mut send_cascade,
                };
                let input = channels[ch].get(offset..).unwrap_or(&[]);
                for i in 0..n.min(input.len()) {
                    bus[i] += input[i] * send_level;
                }
            }

            self.velvet_cascade.set_parameters(self.velvet.current);
            for i in 0..n {
                let (a_l, a_r) = self.velvet.process_sample(send_a[i], send_a[i]);
                let (cascade_l, cascade_r) = self
                    .velvet_cascade
                    .process_sample(send_cascade[i], send_cascade[i]);
                let b_l = send_b[i] + cascade_l * self.cascade_gain;
                let b_r = send_b[i] + cascade_r * self.cascade_gain;
                let (wet_b_l, wet_b_r) = self.plateau.process_sample(b_l, b_r);
                out_l[offset + i] = super::math::soft_clip(
                    (a_l + cascade_l) * self.return_gain_a + wet_b_l * self.return_gain_b,
                );
                out_r[offset + i] = super::math::soft_clip(
                    (a_r + cascade_r) * self.return_gain_a + wet_b_r * self.return_gain_b,
                );
            }
            offset += n;
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
        let frames = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        for i in 0..frames {
            let (a_l, a_r) = if enable_a {
                self.velvet.process_sample(in_l[i], in_r[i])
            } else {
                (0.0, 0.0)
            };
            let cascade_l = if enable_a && self.cascade_gain > 1e-4 {
                a_l * self.cascade_gain
            } else {
                0.0
            };
            let cascade_r = if enable_a && self.cascade_gain > 1e-4 {
                a_r * self.cascade_gain
            } else {
                0.0
            };
            let (b_l, b_r) = if enable_b {
                self.plateau
                    .process_sample(in_l[i] + cascade_l, in_r[i] + cascade_r)
            } else {
                (0.0, 0.0)
            };
            let val_a_l = if enable_a {
                a_l * self.return_gain_a
            } else {
                0.0
            };
            let val_a_r = if enable_a {
                a_r * self.return_gain_a
            } else {
                0.0
            };
            let val_b_l = if enable_b {
                b_l * self.return_gain_b
            } else {
                0.0
            };
            let val_b_r = if enable_b {
                b_r * self.return_gain_b
            } else {
                0.0
            };
            out_l[i] = super::math::soft_clip(val_a_l + val_b_l);
            out_r[i] = super::math::soft_clip(val_a_r + val_b_r);
        }
    }
}
