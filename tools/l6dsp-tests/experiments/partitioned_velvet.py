#!/usr/bin/env python3
"""Host-only full-density convolution experiment; not production DSP.

Requires NumPy. This checks mathematical output and a proposed order of work,
not embedded timing, memory ownership, or target-side implementation.
"""
import argparse
from pathlib import Path
import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--tap-file', type=Path, help='optional channel, index, delay, gain text from the Rust generator')
parser.add_argument('--partition', type=int, choices=(256, 512), default=256,
                    help='tail partition size in frames (default: 256)')
parser.add_argument('--build-ffts', type=int, choices=(1, 2), default=2,
                    help='maximum bank-rebuild FFTs per callback (default: 2)')
args = parser.parse_args()

B = args.partition
HEAD = 2 * B
TAIL = 4560
TAPS = 285
N = B * 40
rng = np.random.default_rng(0x16ef)
signals = rng.uniform(-0.25, 0.25, (2, N)).astype(np.float32)
# Each bank has independent stereo channels. Preserve the real generator's
# zero-delay impulse and strictly increasing delays.
def make_bank():
    bank = np.zeros((2, TAIL + 1), dtype=np.float32)
    for ch in range(2):
        delays = np.r_[0, np.sort(rng.choice(np.arange(1, TAIL + 1), TAPS - 1, replace=False))]
        gains = rng.choice(np.array([-1, 1], np.float32), len(delays)) * np.exp(-3 * delays / TAIL).astype(np.float32)
        gains[0] = 1.0
        gains /= np.linalg.norm(gains)
        bank[ch, delays] = gains
    return bank

kernels = np.stack([make_bank(), make_bank()])

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

# The second edit interrupts the first. Both are queued while bank 0 is
# active; the experiment does not model editing an already audible bank.
old_bank1 = kernels[1].copy()
replacements = np.stack([make_bank(), make_bank()])

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
tail_epoch = np.zeros((2, 2, 2, 2), np.int32)
edit_points = {(9, 1): 0, (9, 3): 1}
epoch = 0
build_job = None
completed_at = None
published_at = None
skipped_pending_outputs = 0
for block in range(N // B):
    base = block * B
    # Publish only at a block boundary, after all four pending outputs for
    # this block were calculated against the complete latest spectrum.
    if completed_at is not None and published_at is None and np.all(tail_epoch[block % 2, 1] == epoch):
        published_at = base
    slots = B // 64
    for slot in range(slots):
        begin = base + slot * 64
        end = begin + 64
        if (block, slot) in edit_points:
            candidate = edit_points[block, slot]
            if candidate == 1:
                assert build_job is not None and published_at is None
            epoch += 1
            build_job = 0
            completed_at = None
        if build_job is not None:
            # An interrupted build is restarted and may not publish partial spectra.
            for _ in range(args.build_ffts):
                ch = build_job % 2
                part = 2 + build_job // 2
                window = np.zeros(2 * B, np.float32)
                taps = replacements[candidate, ch, part*B:(part+1)*B]
                window[:len(taps)] = taps
                freq[1, ch, part-2] = np.fft.rfft(window)
                build_job += 1
                if build_job == 2*(P-2):
                    kernels[1] = replacements[candidate]
                    build_job = None
                    completed_at = begin
                    break
        for bank in range(2):
            for ch in range(2):
                head = kernels[bank, ch, :HEAD]
                for bus in range(2):
                    past = signals[bus, max(0, begin-HEAD+1):end]
                    head_full = np.convolve(past, head)
                    offset = begin - max(0, begin-HEAD+1)
                    actual[bank, ch, bus, begin:end] = tail_ready[block % 2, bank, ch, bus, slot*64:(slot+1)*64] + head_full[offset:offset+64]
        # Spread all eight bank/channel/bus outputs across the callbacks. Input
        # spectra for block+1 are already available before this block starts.
        future = block + 1
        for output_index in range(slot*8//slots, (slot+1)*8//slots):
            bank = output_index // 4
            ch = output_index // 2 % 2
            bus = output_index % 2
            # An incomplete pending bank is inaudible and must not consume
            # a transform slot or leave an apparently usable tail behind.
            if bank == 1 and build_job is not None:
                tail_epoch[future % 2, bank, ch, bus] = -1
                skipped_pending_outputs += 1
                continue
            accum = np.zeros(B + 1, dtype=np.complex64)
            for part in range(2, min(future, P-1) + 1):
                accum += freq[bank, ch, part-2] * xhist[bus, (future-part) % (P-2)]
            tail_ready[future % 2, bank, ch, bus] = np.fft.irfft(accum, n=2*B)[B:]
            tail_epoch[future % 2, bank, ch, bus] = epoch if bank == 1 else 0
    for bus in range(2):
        xhist[bus, block % (P-2)] = np.fft.rfft(np.r_[previous[bus], signals[bus, base:base+B]])
        previous[bus] = signals[bus, base:base+B]

assert epoch == 2 and completed_at is not None and published_at is not None
assert skipped_pending_outputs > 0
assert published_at > completed_at >= 9*B + 3*64
reference = np.empty_like(actual)
for bank in range(2):
    for ch in range(2):
        for bus in range(2):
            reference[bank,ch,bus] = np.convolve(signals[bus], kernels[bank,ch])[:N]
            if bank == 1:
                first_edit = 9*B + 64
                reference[bank,ch,bus,:first_edit] = np.convolve(signals[bus], old_bank1[ch])[:first_edit]
# A rebuilding pending bank can contain mixed generations. It is inaudible
# until publication; bank 0 must remain correct over the entire interval.
assert np.max(np.abs(actual[0] - reference[0])) < 2e-6
mask = np.ones(N, bool)
mask[first_edit:published_at] = False
error = np.abs(actual[...,mask] - reference[...,mask])
assert np.max(error) < 2e-6, (np.max(error), np.unravel_index(np.argmax(error), error.shape))
weights = np.zeros(N, np.float32)
start = published_at + 37
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
print('complex_MAC_per_64', (8//(B//64))*(P-2)*(B+1))
print('max_rebuild_forward_FFTs_per_64', args.build_ffts)
print('skipped_pending_outputs', skipped_pending_outputs)
print('replacement_build_completed_frame', completed_at, 'published_frame', published_at)
print('prototype_spectra_bytes', freq.nbytes, 'input_spectra_bytes', xhist.nbytes, 'tail_ready_bytes', tail_ready.nbytes)
