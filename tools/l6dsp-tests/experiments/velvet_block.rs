//! Experimental tap-outer FIR, not firmware integration.
//!
//! The mirrored-history approach is informed by the MIT-licensed Multistomp
//! Velvet port at ZoomMultistompZDL commit 7ea50ae6e30705989d0b8414ad5a83c97dbef0db.
//! This experiment covers two independent mono sends and two filter channels.

pub const MAX_DELAY: usize = 4560; // 95 ms at 48 kHz
pub const BLOCK: usize = 16;
pub const RING: usize = 4802;
// Prewriting a block must not overwrite history needed by its first frame.
const _: () = assert!(MAX_DELAY + BLOCK <= RING);

#[derive(Clone, Copy)]
pub struct Tap {
    pub delay: usize,
    pub gain: f32,
}

pub struct Histories {
    pub samples: [[f32; RING * 2]; 2],
    pub position: usize,
}

impl Histories {
    pub fn new() -> Self {
        Self {
            samples: [[0.0; RING * 2]; 2],
            position: 0,
        }
    }

    /// Append at most one block before calling `convolve` with its start.
    pub fn append(&mut self, input: &[[f32; BLOCK]; 2], frames: usize) -> usize {
        assert!(frames <= BLOCK);
        let start = self.position;
        for frame in 0..frames {
            for bus in 0..2 {
                let sample = input[bus][frame];
                self.samples[bus][self.position] = sample;
                self.samples[bus][self.position + RING] = sample;
            }
            self.position += 1;
            if self.position == RING {
                self.position = 0;
            }
        }
        start
    }

    /// One filter channel, applied to both independent histories.
    pub fn convolve(&self, taps: &[Tap], start: usize, frames: usize, out: &mut [[f32; BLOCK]; 2]) {
        assert!(start < RING && frames <= BLOCK);
        out[0][..frames].fill(0.0);
        out[1][..frames].fill(0.0);
        let (left, right) = out.split_at_mut(1);
        for tap in taps {
            assert!(tap.delay <= MAX_DELAY);
            let read = if start >= tap.delay {
                start - tap.delay
            } else {
                start + RING - tap.delay
            };
            let first = &self.samples[0][read..read + frames];
            let second = &self.samples[1][read..read + frames];
            for (((a, b), x), y) in left[0][..frames]
                .iter_mut()
                .zip(right[0][..frames].iter_mut())
                .zip(first)
                .zip(second)
            {
                *a += tap.gain * *x;
                *b += tap.gain * *y;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_sample(taps: &[Tap], input: &[f32], index: usize) -> f32 {
        let mut result = 0.0;
        for tap in taps {
            if index >= tap.delay {
                result += tap.gain * input[index - tap.delay];
            }
        }
        result
    }

    #[test]
    fn dense_stereo_banks_match_sample_reference_with_split_events_and_wrap() {
        let mut banks = [[Vec::new(), Vec::new()], [Vec::new(), Vec::new()]];
        for bank in 0..2 {
            for channel in 0..2 {
                for i in 0..285 {
                    let sign = if (i + channel + bank) % 2 == 0 {
                        1.0
                    } else {
                        -1.0
                    };
                    banks[bank][channel].push(Tap {
                        delay: i * MAX_DELAY / 284,
                        gain: sign * (1.0 / (i + 1) as f32),
                    });
                }
            }
        }
        let mut histories = Box::new(Histories::new());
        let total = 10_000;
        let mut source = [vec![0.0f32; total], vec![0.0f32; total]];
        for i in 0..total {
            source[0][i] = ((i * 37 % 101) as f32 - 50.0) / 200.0;
            source[1][i] = ((i * 71 % 103) as f32 - 51.0) / 210.0;
        }
        // Switch bank on a non-block boundary. Split there, as a real caller
        // must also do for any edit that changes the filter inside a block.
        let switch_at = 4979;
        let mut at = 0;
        while at < total {
            let mut count = BLOCK.min(total - at);
            if at < switch_at {
                count = count.min(switch_at - at);
            }
            let mut input = [[0.0; BLOCK]; 2];
            for bus in 0..2 {
                input[bus][..count].copy_from_slice(&source[bus][at..at + count]);
            }
            let start = histories.append(&input, count);
            for channel in 0..2 {
                let mut actual = [[[0.0; BLOCK]; 2]; 2];
                for bank in 0..2 {
                    histories.convolve(&banks[bank][channel], start, count, &mut actual[bank]);
                }
                for bus in 0..2 {
                    for frame in 0..count {
                        let index = at + frame;
                        let expected = [
                            reference_sample(&banks[0][channel], &source[bus], index),
                            reference_sample(&banks[1][channel], &source[bus], index),
                        ];
                        // Exercise a per-frame crossfade as well as a bank
                        // change at a non-aligned boundary.
                        let weight = ((index.saturating_sub(4800)).min(179) as f32) / 179.0;
                        let actual =
                            actual[0][bus][frame] * (1.0 - weight) + actual[1][bus][frame] * weight;
                        let expected = expected[0] * (1.0 - weight) + expected[1] * weight;
                        assert!(
                            (actual - expected).abs() < 1e-6,
                            "frame={index} bus={bus} channel={channel}"
                        );
                    }
                }
            }
            at += count;
        }
    }
}
