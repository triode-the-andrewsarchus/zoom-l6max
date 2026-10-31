# Host-only DSP experiments

`python3 tools/l6dsp-tests/experiments/partitioned_velvet.py` requires NumPy
and compares a proposed full-density, partitioned convolution schedule with
direct convolution. It covers two inputs, stereo filters, two banks, a bank
replacement and a crossfade. An optional `--tap-file PATH` loads channel,
index, delay and gain columns from a locally generated 285-tap Rust bank.

This is a mathematical experiment, not a firmware implementation or a
qualification test. It assumes both spectra exist before they are needed;
it does not schedule FFT construction, measure ARM deadlines, establish
firmware memory ownership or test button and audio behavior on hardware. The
printed buffer sizes exclude additional DSP and AINR state and workspace.
