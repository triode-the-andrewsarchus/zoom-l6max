# Dual-EFX safety status

The patch is disabled in the native and browser patch builders. Do not install
an image produced by earlier versions of this branch. The firmware integration
is not qualified for physical hardware.

The blocking findings for L6max MAIN 1.10
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461` are:

- The supposed effect input and output audio addresses identify stock control
  structures. The output loop overwrites a structure containing code pointers.
- The disabled legacy firmware now initializes its `NOLOAD` storage in place
  before forming a DSP reference when the magic tag is invalid. It still does
  not establish ownership, exclusivity, or ensure the magic value is
  invalidated on every reset; a stale tag can bypass initialization. In one settled
  emulator run the largest stock-heap free block was 160,008 bytes, less than
  the then-current payload state of 319,216 bytes; see the
  [callback investigation](firmware/l6max-effect-callback.md). The current
  processor type measures 279,344 bytes for `thumbv7em-none-eabihf` after a
  25,824-byte reduction in raw Plateau delay arrays; it still exceeds that free
  block. The current size query object SHA-256 is
  `1d917a7f8698898ff4e71c00824b9bf6896e6188432d3e57663f6302eba034e4`
  using `rustc 1.99.0-nightly (1ed2df61a 2026-08-04)`.
  Of that state, the raw `f32` histories comprise 36,484 bytes of
  Plateau predelay, 7,024 bytes of Plateau input diffusion, 168,372 bytes of
  Plateau tank delays and 57,624 bytes of Velvet histories (including the
  separate cascade history). The remaining 9,840 bytes include filter banks,
  parameters and bookkeeping. These counts describe allocated capacity,
  not a proof that any stock firmware memory can be reused.
  Stock code
  also references several regions well above that heap; a high SDRAM address
  cannot be treated as reserved merely because it is distant from the heap.
- The hook at `0x8000277c` is in a routine containing file-output and card-error
  handling. The branch has not established an audio-buffer ABI or a live
  audio callback at that call site.
- The stock track-to-file mix path also loads and calls the active effect
  callback, independently of the observed live audio wrapper. A pause tied
  only to the live audio task does not establish that all callback users have
  stopped. The diagnostic reconstructs sends from live mixer slots and has not
  established correct input for this file-mixing path. A direct synthetic
  QEMU call supports a candidate twelve-track buffer layout. An experimental
  call-site veneer passed the block offset to a separate callback and routed
  synthetic tracks through the effects; it does not exercise the normal
  workflow or establish a safe hook. Stock scheduling and
  exclusion between the two paths are unverified. See the
  [firmware research](https://github.com/derekstavis/zoom-l6max-research/blob/bad532c/docs/firmware/l6max-effects-ainr.md#effects-callback).
- The active firmware signal path ignores per-channel routing and has no
  pre-AINR implementation. The planned pre-AINR address is an effect-type
  getter call, not an established audio interface; the selected AINR callback
  clears the normal wet buffers. A candidate in-place audio-task interface
  observed during learning remains unqualified. Its stock indices 0 and 1
  use the master processor state; extending them to index 2 overlaps existing
  AINR storage, control words and the active effect callback pointer;
  see the [callback investigation](firmware/l6max-effect-callback.md#ai-noise-reduction-investigation).
  The standalone routing processor separates A-only
  and cascaded sends, but is not connected to firmware. Its additional DSP
  state and execution time have not been qualified on the target. The extended
  UI and patch combinations are incomplete. An earlier experimental QEMU
  callback stopped EFX SEL cycling because it did not advance the stock
  control stream. The diagnostic now advances it and the tested six-press
  sequence progresses, but the production firmware integration is unfinished.
- L6max 1.10 also supports stock effect returns to Sub-Mix and Mix Minus.
  A QEMU diagnostic with synthetic sends and replacement DSP reached modeled
  Sub-Mix transmit slots when enabled. USB Mix Minus, physical output mapping,
  actual input routing, and behavior across effect changes remain unqualified.
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
- With the later QEMU receive model switching to regular RX after startup,
  a nonzero-sends diagnostic failed to open the inspector or select the next
  effect; a DSP-bypassed control completed the same sequence. This emulator
  workload dependence does not measure physical execution time.
- A QEMU-only chain using three copied, learned AINR processors before the
  synthetic effect sends ran below modeled audio cadence. In a separate
  matched pair, two TAP presses during the active chain did not reach the
  traced stock queue or dispatcher, while the DSP-bypassed control delivered
  them. This does not establish physical behavior, but TAP is not a validated
  control workaround under that modeled workload. The copies use unowned
  scratch memory and have no independent learning or target qualification.
- A later QEMU-only automatic handoff retains a 128 KiB stock-heap allocation
  and copies it inside an audio callback. In the observed run this left 28,920
  bytes of stock heap free, and the three-processor workload ran at about 458
  callbacks per second against a nominal 750. Neither the allocation lifetime
  nor the copy's physical deadline is established. Its experimental settling
  delay and modified samples do not establish useful noise reduction.
- An opt-in QEMU boundary-handoff diagnostic moves the snapshot copy to the
  observed panel selector and bypasses AINR while that copy is made. It does
  not establish ownership, all selection paths, the bypass ABI, audio continuity,
  or target timing. Under active dry fallback, a paced run still failed to
  reach AINR; a faster instruction-clock model completed a forced retry. This
  is not a physical timing measurement or an installable integration. A trace
  of the earlier full-prefix copy observed stock CPU writes into the source
  between the start and end of the copy while the pause was acknowledged;
  nevertheless the diagnostic published and installed the snapshot. The
  current experiment excludes those observed stock rings, but it has not
  established the complete AINR footprint or excluded all other writers. Its
  packed heap snapshot now holds only a candidate 56,264-byte channel-0 tail;
  selection still reconstructs three copies in unqualified effect storage.
  The pause therefore does not establish a coherent snapshot. A smaller
  temporary prototype captured 512 bytes and reconstructed the remainder
  after selection; under injected signal the resulting state differed from
  its capture-time observation despite a successful handoff status. That
  reconstruction was discarded.

Some standalone DSP defects have been fixed and tested on the host. Those
tests do not exercise the firmware bindings or establish hardware safety.
Plateau's zero-diffusion setting now bypasses its input diffuser chain while
advancing that chain's history. Its bypass and allpass gains ramp over 64
frames on diffusion edits. Host tests cover the immediate impulse path, an
active-signal first-frame edit, reversals and edits while idle; target
execution time and audible transitions remain unqualified.
Plateau's output taps now use the same sample-rate-scaled positions on both
channels as the supplied Multistomp reference, independently of tank size.
They read the full retained delay histories. Host tests exercise each tap at
the smallest tank size; this does not establish overall audio equivalence with
the reference or physical behavior.
The input diffuser delay times are now also independent of tank size, as in
that reference. Host tests cover size edits and retained input history, but
do not establish musical quality or target execution time.
All four tank allpasses now receive independent modulation, with sufficient
history for the maximum configured depth. Host tests cover modulation and
freeze, but the additional processing cost has not been measured on target.
The inspector's Plateau predelay, size and diffusion edits now update only
the affected state instead of recalculating unrelated cutoff coefficients.
Host comparisons exercise mixed edits and endpoint clamps sample by sample;
they do not establish an audio-task deadline on the device.
The Velvet crossfade now calculates its phase step once per transition;
host tests compare its weights with the previous calculation. In a disposable
ARM diagnostic build, the former per-sample divide moved to transition start
(old and new ELF SHA-256 values `a58c0d242fafc5bbd5f61b8fc4aecd46a4fe1f55022a82b69e45e9c09a96d398`
and `197bafc27f9023b26c1b1a73a53c8d9150577343e6a53dd2d15ebc60b44cc5da`).
This instruction inspection does not measure workload or establish a deadline.
Velvet now supports the full requested density range with up to 285 taps;
its minimum of two taps and integer rounding constrain short spreads. The
parameter model displays the resulting target density while retaining the
requested setting. Meeting the audio deadline remains unresolved.
An instrumented current QEMU diagnostic with the 285 taps needed at 95 ms
and 3,000 pps ran at about 416 callbacks per modeled second against 750;
its selector hold did not reach the traced handler by the later snapshot.
That result
does not measure the target, but rules out treating a larger bank alone as a
qualified solution.
The stereo adapter now advances disabled effects on silence to avoid resuming
stale audio, increasing its work while effects are disabled; target timing for
that behavior has not been measured. Plateau also advances its bypassed
damping filters during freeze to avoid replaying stale filter state when
unfrozen; its added freeze-time work has not been measured on the target.
Generic emulator boot, panel, and audio-callback checks do not qualify effects
audio, controls, or behavior on a physical device.
Plateau now holds its tank modulation delays fixed during freeze: a host test
at maximum modulation checks tank-energy stability across four sizes and
verifies that modulation resumes on release. This does not establish physical
audio behavior, freeze-transition quality or target timing.
Earlier diagnostics using the always-on L6max SAI receive compatibility mode
observed roughly 188 callbacks per second. The nominal 48 kHz, 64-frame audio
path requires 750 callbacks per second; those earlier runs therefore do not
qualify the replacement's workload at nominal cadence. A later diagnostic model
switched to regular RX after startup. Mainline's subsequent audio update removes
that compatibility mode and changes DMA servicing; the earlier diagnostic
results do not qualify this new model. Physical timing remains unverified.

Before re-enabling the builder, trace the relevant firmware interfaces against
the exact supported MAIN image; reserve and initialize state with a documented
ownership and synchronization model; validate routing, UI and audio behavior;
and record identities for the tested firmware, fixtures, and executable.
