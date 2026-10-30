# Dual-EFX safety status

The patch is disabled in the native and browser patch builders. Do not install
an image produced by earlier versions of this branch. The firmware integration
is not qualified for physical hardware.

The blocking findings for L6max MAIN 1.10
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461` are:

- The supposed effect input and output audio addresses identify stock control
  structures. The output loop overwrites a structure containing code pointers.
- The firmware obtains a typed reference to `NOLOAD` state without first
  constructing valid Rust values. The DSP now provides a tested in-place
  initializer for caller-owned storage, but the firmware does not use it,
  establish ownership or ensure the magic value is invalidated on every reset. In one settled
  emulator run the largest stock-heap free block was 160,008 bytes, less than
  the then-current payload state of 319,216 bytes; see the
  [callback investigation](firmware/l6max-effect-callback.md).
- The hook at `0x8000277c` is in a routine containing file-output and card-error
  handling. The branch has not established an audio-buffer ABI or a live
  audio callback at that call site.
- The active firmware signal path ignores per-channel routing and has no
  pre-AINR implementation. The standalone routing processor separates A-only
  and cascaded sends, but is not connected to firmware. Its additional DSP
  state and execution time have not been qualified on the target. The extended
  UI and patch combinations are incomplete. An earlier experimental QEMU
  callback stopped EFX SEL cycling because it did not advance the stock
  control stream. The diagnostic now advances it and the tested six-press
  sequence progresses, but the production firmware integration is unfinished.
- Shared mutable state and interrupt masking have not been proven safe. There
  are no measured target deadline, stack, memory-ownership, or output bounds.
  A forced emulator interleaving has shown that audio can reach a newly
  published callback before the selecting task continues, so callback state
  must be ready before that publication.

Some standalone DSP defects have been fixed and tested on the host. Those
tests do not exercise the firmware bindings or establish hardware safety.
Generic emulator boot, panel, and audio-callback checks do not qualify effects
audio, controls, or behavior on a physical device.

Before re-enabling the builder, trace the relevant firmware interfaces against
the exact supported MAIN image; reserve and initialize state with a documented
ownership and synchronization model; validate routing, UI and audio behavior;
and record identities for the tested firmware, fixtures, and executable.
