# Host-only DSP experiments

`python3 tools/l6dsp-tests/experiments/partitioned_velvet.py` requires NumPy
and compares a proposed full-density, partitioned convolution schedule with
direct convolution. It covers two inputs, stereo filters, two banks, an
interrupted incremental bank rebuild and a crossfade after the replacement
tail is ready. An optional `--tap-file PATH` loads channel,
index, delay and gain columns from a locally generated 285-tap Rust bank.

This is a mathematical experiment, not a firmware implementation or a
qualification test. It spreads construction across callbacks but does not
bound individual FFT execution, model edits to an audible bank, measure ARM
deadlines, establish firmware memory ownership or test controls and audio on
hardware. The printed buffer sizes exclude additional DSP and AINR state and
workspace.
