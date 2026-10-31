#!/usr/bin/env python3
"""Host-only full-density convolution experiment; not production DSP.

Requires NumPy. This checks mathematical output and a proposed order of work,
not embedded timing, memory ownership, or incremental bank construction.
"""
import argparse
from pathlib import Path
import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--tap-file', type=Path, help='optional channel, index, delay, gain text from the Rust generator')
args = parser.parse_args()

B = 256
HEAD = 2 * B
TAIL = 4560
TAPS = 285
N = B * 40
rng = np.random.default_rng(0x16ef)
signals = rng.uniform(-0.25, 0.25, (2, N)).astype(np.float32)
# Two filter banks, each with left and right independent channels. Preserve
# the real generator's zero-delay impulse and strictly increasing delays.
kernels = np.zeros((2, 2, TAIL + 1), dtype=np.float32)
for bank in range(2):
    for ch in range(2):
        delays = np.r_[0, np.sort(rng.choice(np.arange(1, TAIL + 1), TAPS - 1, replace=False))]
        gains = rng.choice(np.array([-1, 1], np.float32), len(delays)) * np.exp(-3 * delays / TAIL).astype(np.float32)
        gains[0] = 1.0
        gains /= np.linalg.norm(gains)
        kernels[bank, ch, delays] = gains

# Optionally compare against a locally generated Rust bank. Firmware and
# generated test artifacts must not be committed to the repository.
if args.tap_file is not None:
    kernels[0].fill(0.0)
    for row in np.loadtxt(args.tap_file):
        ch, _, delay, gain = row
        if ch not in (0, 1) or delay != int(delay) or not 0 <= delay <= TAIL:
            raise ValueError(f'invalid tap: {row}')
        kernels[0, int(ch), int(delay)] = np.float32(gain)
    if any(np.count_nonzero(kernels[0, ch]) != TAPS for ch in range(2)):
        raise ValueError('each channel must contain 285 distinct nonzero taps')

# Edit begins mid-block 9. Its completed bank is published at block 10;
# the fade is deferred to block 11 when its new tail is ready. This models
# publication order only: spectrum construction itself is not scheduled.
old_bank1 = kernels[1].copy()
replacement = np.zeros_like(old_bank1)
for ch in range(2):
    delays = np.r_[0, np.sort(rng.choice(np.arange(1, TAIL + 1), TAPS - 1, replace=False))]
    gains = rng.choice(np.array([-1, 1], np.float32), len(delays)) * np.exp(-3 * delays / TAIL).astype(np.float32)
    gains[0] = 1.0
    replacement[ch, delays] = gains / np.linalg.norm(gains)

P = (TAIL + B) // B
freq = np.zeros((2, 2, P - 2, B + 1), dtype=np.complex64)
for bank in range(2):
    for ch in range(2):
        for part in range(2, P):
            window = np.zeros(2 * B, np.float32)
            taps = kernels[bank, ch, part*B:(part+1)*B]
            window[:len(taps)] = taps
            freq[bank, ch, part - 2] = np.fft.rfft(window)

# Overlap-save windows are prepared when each input block completes. Tail
# partition j starts at j*B. The head covers the first two partitions.
xhist = np.zeros((2, P - 2, B + 1), dtype=np.complex64)
previous = np.zeros((2, B), np.float32)
actual = np.zeros((2, 2, 2, N), np.float32)
tail_ready = np.zeros((2, 2, 2, 2, B), np.float32)
for block in range(N // B):
    base = block * B
    if block == 10:
        kernels[1] = replacement
        for ch in range(2):
            for part in range(2, P):
                window = np.zeros(2 * B, np.float32)
                taps = replacement[ch, part*B:(part+1)*B]
                window[:len(taps)] = taps
                freq[1, ch, part-2] = np.fft.rfft(window)
    for quarter in range(4):
        begin = base + quarter * 64
        end = begin + 64
        for bank in range(2):
            for ch in range(2):
                head = kernels[bank, ch, :HEAD]
                for bus in range(2):
                    past = signals[bus, max(0, begin-HEAD+1):end]
                    head_full = np.convolve(past, head)
                    offset = begin - max(0, begin-HEAD+1)
                    actual[bank, ch, bus, begin:end] = tail_ready[block % 2, bank, ch, bus, quarter*64:(quarter+1)*64] + head_full[offset:offset+64]
        # Compute two full outputs per 64-frame callback. All required input
        # spectra for block+1 are already available before this block starts.
        future = block + 1
        for output_index in range(quarter*2, quarter*2+2):
            bank = output_index // 4
            ch = output_index // 2 % 2
            bus = output_index % 2
            accum = np.zeros(B + 1, dtype=np.complex64)
            for part in range(2, min(future, P-1) + 1):
                accum += freq[bank, ch, part-2] * xhist[bus, (future-part) % (P-2)]
            tail_ready[future % 2, bank, ch, bus] = np.fft.irfft(accum, n=2*B)[B:]
    for bus in range(2):
        xhist[bus, block % (P-2)] = np.fft.rfft(np.r_[previous[bus], signals[bus, base:base+B]])
        previous[bus] = signals[bus, base:base+B]

reference = np.empty_like(actual)
for bank in range(2):
    for ch in range(2):
        for bus in range(2):
            reference[bank,ch,bus] = np.convolve(signals[bus], kernels[bank,ch])[:N]
            if bank == 1:
                reference[bank,ch,bus,:10*B] = np.convolve(signals[bus], old_bank1[ch])[:10*B]
# During block 10 the new head and old tail intentionally coexist. The
# pending bank is not audible until block 11; exclude that unpublished block.
mask = np.ones(N, bool)
mask[10*B:11*B] = False
error = np.abs(actual[...,mask] - reference[...,mask])
assert np.max(error) < 2e-6, (np.max(error), np.unravel_index(np.argmax(error), error.shape))
weights = np.zeros(N, np.float32)
start = 11*B + 37
weights[start:start+1200] = np.linspace(0, 1, 1200, dtype=np.float32)
weights[start+1200:] = 1
mixed = actual[0] * (1-weights) + actual[1] * weights
expected = reference[0] * (1-weights) + reference[1] * weights
assert np.max(np.abs(mixed-expected)) < 2e-6
# The first input sample must affect the first output sample.
assert np.allclose(actual[0, :, :, 0], reference[0, :, :, 0], atol=2e-6)
assert np.any(actual[0, :, :, 0] != 0)
print(f'block={B} partitions={P} taps={TAPS} frames={N}')
print('max_bank_error', float(np.max(error)), 'max_crossfade_error', float(np.max(np.abs(mixed-expected))))
print('head_taps', [[int(np.count_nonzero(kernels[bank,ch,:HEAD])) for ch in range(2)] for bank in range(2)])
print('complex_MAC_per_64', 2*(P-2)*(B+1))
print('prototype_spectra_bytes', freq.nbytes, 'input_spectra_bytes', xhist.nbytes, 'tail_ready_bytes', tail_ready.nbytes)
