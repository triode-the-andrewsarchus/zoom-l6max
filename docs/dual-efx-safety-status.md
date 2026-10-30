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
  [callback investigation](firmware/l6max-effect-callback.md). Stock code
  also references several regions well above that heap; a high SDRAM address
  cannot be treated as reserved merely because it is distant from the heap.
- The hook at `0x8000277c` is in a routine containing file-output and card-error
  handling. The branch has not established an audio-buffer ABI or a live
  audio callback at that call site.
- The active firmware signal path ignores per-channel routing and has no
  pre-AINR implementation. The planned pre-AINR address is an effect-type
  getter call, not an established audio interface; the selected AINR callback
  clears the normal wet buffers. A candidate in-place audio-task interface
  observed during learning remains unqualified. Its stock indices 0 and 1
  use the master processor state; extending them to index 2 overlaps both
  existing AINR storage and the active effect callback pointer;
  see the [callback investigation](firmware/l6max-effect-callback.md#ai-noise-reduction-investigation).
  The standalone routing processor separates A-only
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
- In QEMU, the inspector's long-hold gesture was misclassified during a
  stress run with nonzero synthetic sends on all three effect buses. Silent
  input runs passed after a DSP optimization; the active-audio control path
  and physical deadline remain unqualified. An optional QEMU-only interrupt
  timestamp prototype handles that observed sequence but uses unowned memory
  and has not qualified lost events, target synchronization or timing. A
  combined popup run also displayed a bank label instead of the edited value
  at a 500 ms snapshot under active synthetic audio; a second run with a later
  snapshot showed the value. A further observed delay occurred before the
  audio-event queue; responsiveness remains unqualified.

Some standalone DSP defects have been fixed and tested on the host. Those
tests do not exercise the firmware bindings or establish hardware safety.
The stereo adapter now advances disabled effects on silence to avoid resuming
stale audio, increasing its work while effects are disabled; target timing for
that behavior has not been measured.
Generic emulator boot, panel, and audio-callback checks do not qualify effects
audio, controls, or behavior on a physical device.
Earlier diagnostics using the always-on L6max SAI receive compatibility mode
observed roughly 188 callbacks per second. The nominal 48 kHz, 64-frame audio
path requires 750 callbacks per second; those earlier runs therefore do not
qualify the replacement's workload at nominal cadence. The newer emulator
switches to regular RX behavior after startup, but that behavior remains an
emulation model and physical timing is unverified.

Before re-enabling the builder, trace the relevant firmware interfaces against
the exact supported MAIN image; reserve and initialize state with a documented
ownership and synchronization model; validate routing, UI and audio behavior;
and record identities for the tested firmware, fixtures, and executable.
