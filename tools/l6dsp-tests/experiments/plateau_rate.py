#!/usr/bin/env python3
"""Host-only 48/32 kHz streaming SRC experiment; not firmware code.

The filter follows the supplied Multistomp prototype's 64-tap, 14 kHz
Kaiser-window design. At the 3:2 ratio only two downsampling and three
upsampling phases are needed. This model inserts an identity function where
the 32 kHz reverb would run.
"""

import unittest

import numpy as np

TAPS = 64


def phase_table(rate, phases):
    offsets = np.arange(TAPS)[None, :] - (31 + np.asarray(phases)[:, None])
    cutoff = 14_000 / rate
    table = 2 * cutoff * np.sinc(2 * cutoff * offsets) * np.kaiser(TAPS, 7.8)
    return (table / table.sum(axis=1, keepdims=True)).astype(np.float32)


DOWN = phase_table(48_000, (0, 0.5))
UP = phase_table(32_000, (1 / 3, 2 / 3, 1))


class IdentityRoundTrip:
    """Stereo streaming model with separate input and output histories."""

    def __init__(self):
        self.phase = 0
        self.input = np.zeros((2, TAPS), dtype=np.float32)
        self.output = np.zeros((2, TAPS), dtype=np.float32)
        self.internal_frames = 0

    def process(self, frames):
        result = np.empty_like(frames, dtype=np.float32)
        for i, frame in enumerate(frames):
            self.input[:, 1:] = self.input[:, :-1]
            self.input[:, 0] = frame
            self.phase += 2
            if self.phase >= 3:
                self.phase -= 3
                self.output[:, 1:] = self.output[:, :-1]
                self.output[:, 0] = self.input @ DOWN[self.phase]
                self.internal_frames += 1
            result[i] = self.output @ UP[2 - self.phase]
        return result


class PlateauRateExperiment(unittest.TestCase):
    def test_rate_dc_and_chunk_boundaries(self):
        source = np.full((48_000, 2), (0.25, -0.125), dtype=np.float32)
        one = IdentityRoundTrip()
        expected = one.process(source)
        split = IdentityRoundTrip()
        actual = np.concatenate([split.process(source[:37]),
                                 split.process(source[37:10_001]),
                                 split.process(source[10_001:])])
        self.assertEqual(one.internal_frames, 32_000)
        self.assertEqual(split.internal_frames, 32_000)
        np.testing.assert_array_equal(actual, expected)
        np.testing.assert_allclose(actual[1_000:], source[1_000:], atol=1e-6)

    def test_band_gain_and_channel_isolation(self):
        time = np.arange(24_000) / 48_000
        for frequency in (100, 1_000, 6_000, 12_000, 16_000, 17_000, 19_000, 23_000):
            with self.subTest(frequency=frequency):
                signal = np.sin(2 * np.pi * frequency * time).astype(np.float32)
                source = np.column_stack((signal, np.zeros_like(signal)))
                output = IdentityRoundTrip().process(source)
                ratio = np.sqrt(np.mean(output[2_400:, 0] ** 2) /
                                np.mean(signal[2_400:] ** 2))
                decibels = 20 * np.log10(max(ratio, 1e-12))
                if frequency <= 12_000:
                    self.assertLess(abs(decibels), 0.03)
                else:
                    self.assertLess(decibels, -65)
                np.testing.assert_array_equal(output[:, 1], 0)

    def test_downsample_filter_stopband_for_both_phases(self):
        frequencies = np.fft.rfftfreq(131_072, 1 / 48_000)
        for coefficients in DOWN:
            response = np.abs(np.fft.rfft(coefficients, 131_072))
            self.assertLess(float(response[frequencies >= 16_000].max()), 10 ** (-70 / 20))
            self.assertAlmostEqual(float(coefficients.sum()), 1, places=6)

    def test_phase_cycle_and_impulse_alignment(self):
        for phase in range(3):
            source = np.zeros((300, 2), dtype=np.float32)
            source[phase, 0] = 1.0
            output = IdentityRoundTrip().process(source)[:, 0]
            self.assertEqual(int(np.argmax(np.abs(output))) - phase, 79)
            self.assertAlmostEqual(float(output.sum()), 1, places=4)

        time = np.arange(24_000) / 48_000
        signal = np.sin(2 * np.pi * 12_000 * time).astype(np.float32)
        output = IdentityRoundTrip().process(np.column_stack((signal, np.zeros_like(signal))))[2_400:, 0]
        time = time[2_400:]
        sine, cosine = np.sin(2 * np.pi * 12_000 * time), np.cos(2 * np.pi * 12_000 * time)
        fitted = 2 * (sine * np.mean(output * sine) + cosine * np.mean(output * cosine))
        residual = np.sqrt(np.mean((output - fitted) ** 2) / np.mean(output ** 2))
        self.assertLess(residual, 10 ** (-80 / 20))


if __name__ == "__main__":
    unittest.main()
