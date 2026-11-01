# Host-only DSP experiments

`python3 tools/l6dsp-tests/experiments/partitioned_velvet.py` requires NumPy
and compares a proposed full-density, partitioned convolution schedule with
direct convolution. It covers two inputs, stereo filters, two banks, an
interrupted incremental bank rebuild and a crossfade after the replacement
tail is ready. An optional `--tap-file PATH` loads channel,
index, delay and gain columns from a locally generated 285-tap Rust bank.
Use `--partition 512` to compare the larger partition with the default 256.
Use `--build-ffts 1` to spread each stereo bank rebuild over more callbacks.
The pending bank's tail transforms are skipped while its spectra are incomplete.

This is a mathematical experiment, not a firmware implementation or a
qualification test. It spreads construction across callbacks but does not
bound individual FFT execution, model edits to an audible bank, measure ARM
deadlines, establish firmware memory ownership or test controls and audio on
hardware. The printed buffer sizes exclude additional DSP and AINR state and
workspace.

`rustc --edition=2024 --test -O tools/l6dsp-tests/experiments/velvet_block.rs -o /tmp/velvet-block-test && /tmp/velvet-block-test`
checks a separate tap-outer, mirrored-history FIR against a direct sample
reference. It exercises 285 taps, two filter banks, two channels, independent
send histories, a per-frame crossfade, a non-aligned split and wraparound.
It does not implement live parameter delivery, bank generation, output
clipping, firmware integration or memory ownership. The extra mirrored history
is included in neither the existing processor size nor the heap estimate. The
two mirrored sample arrays take 76,832 bytes; the current processor's three
unmirrored sample arrays take 57,624 bytes, excluding other state in both cases.
The block size and implementation are informed by the MIT-licensed
Multistomp Velvet port at commit `7ea50ae6e30705989d0b8414ad5a83c97dbef0db`;
that pedal's tests do not qualify the L6max implementation.
