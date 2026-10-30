# Dual-EFX safety status

The patch is disabled in the native and browser patch builders. Do not install
an image produced by earlier versions of this branch. The firmware integration
is not qualified for physical hardware.

The blocking findings for L6max MAIN 1.10
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461` are:

- The supposed effect input and output audio addresses identify stock control
  structures. The output loop overwrites a structure containing code pointers.
- The firmware obtains a typed reference to `NOLOAD` state without first
  constructing valid Rust values. The DSP initializer now restores defaults on
  valid zeroed storage, but the firmware does not establish that precondition
  or ensure the magic value is invalidated on every reset.
- The hook at `0x8000277c` is in a routine containing file-output and card-error
  handling. The branch has not established an audio-buffer ABI or a live
  audio callback at that call site.
- The active signal path ignores per-channel routing and has no pre-AINR
  implementation. The extended UI and patch combinations are incomplete.
- Shared mutable state and interrupt masking have not been proven safe. There
  are no measured target deadline, stack, memory-ownership, or output bounds.

Some standalone DSP defects have been fixed and tested on the host. Those
tests do not exercise the firmware bindings or establish hardware safety.
Generic emulator boot, panel, and audio-callback checks do not qualify effects
audio, controls, or behavior on a physical device.

Before re-enabling the builder, trace the relevant firmware interfaces against
the exact supported MAIN image; reserve and initialize state with a documented
ownership and synchronization model; validate routing, UI and audio behavior;
and record identities for the tested firmware, fixtures, and executable.
