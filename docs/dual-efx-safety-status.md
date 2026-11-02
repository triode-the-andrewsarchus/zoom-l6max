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
  processor type measures 279,464 bytes for `thumbv7em-none-eabihf` after a
  25,824-byte reduction in raw Plateau delay arrays; it still exceeds that free
  block. The current size query object SHA-256 is
  `a75b7b6386e0e98324e775815ecb22f295856522777c89507321da708547d2ee`
  using `rustc 1.99.0-nightly (1ed2df61a 2026-08-04)`.
  Of that state, the raw `f32` histories comprise 36,484 bytes of
  Plateau predelay, 7,024 bytes of Plateau input diffusion, 168,372 bytes of
  Plateau tank delays and 57,624 bytes of Velvet histories (including the
  separate cascade history). The remaining 9,960 bytes include filter banks,
  parameters and bookkeeping. These counts describe allocated capacity,
  not a proof that any stock firmware memory can be reused.
  Stock code
  also references several regions well above that heap; a high SDRAM address
  cannot be treated as reserved merely because it is distant from the heap.
- The additional observed clear call at `0x20280750` is reached from the
  stock audio initializer during startup in one traced emulator run. The same
  trace later observed selector clears; it does not exclude reinitialization
  or indirect callers. The experimental dispatcher must continue to preserve
  stock behavior for this path. See the
  [firmware research](https://github.com/derekstavis/zoom-l6max-research/blob/290eb3d/docs/firmware/l6max-effects-ainr.md#effect-storage-and-adjacent-conversion-buffers).
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
  synthetic tracks through the shared weighted-routing engine. A caller-owned
  workspace reduced the observed ARM callback frame to 408 bytes before deeper
  calls and adds 768 bytes of unowned scratch. The observer's own capture
  bounds checks now fail to zero, but panic paths remain in DSP callees. It
  does not exercise the normal
  workflow or establish a safe hook. A forced QEMU semaphore signal after
  stock stream shutdown still led to an effect callback before file mixing;
  whether that signal can be pending naturally is unverified. Stock scheduling and
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
- The QEMU-only pre-AINR prototype previously passed normalized sends into
  copied stock AINR and treated its output as normalized. In an active-input
  QEMU capture, the learned stock index-0 call instead received float samples
  peaking at 46,271,680; its output peaked at 30,637,510. Eight fresh-state
  clone comparisons matched stock output after converting the input through
  normalized units and back. A second clone given normalized input without
  conversion still produced native-scale output from the copied learned
  history. The diagnostic now converts before and after that call. This short
  capture does not establish independent learning, physical full scale, or
  denoising quality. Continuous-clone comparisons without refreshed state
  diverged at zero-based blocks 65 and 73 in two runs. A follow-up observed
  stock state changes before the second channel-0 call; mirroring both stock
  channels then matched for 96 blocks per channel in one active-input run.
  A later 1,000-block experiment matched stock with a separate channel-zero
  clone even though it did not mirror channel 1 and copied only the first
  `0x19514` bytes. That weakens the omitted-update hypothesis; the earlier
  divergence remains unexplained. The smaller copy is only a candidate, not
  established independent learning or safe memory ownership. Three such
  prefixes plus the 279,464-byte processor would occupy 590,564 bytes of the
  768,000-byte effect buffer before any snapshot or other storage; this is
  arithmetic, not an allocation or ownership proof. See the
  [research observation](https://github.com/derekstavis/zoom-l6max-research/blob/0621038/docs/firmware/l6max-effects-ainr.md#selection-learning-and-processing).
- L6max 1.10 also supports stock effect returns to Sub-Mix and Mix Minus.
  A QEMU diagnostic with synthetic sends and replacement DSP reached modeled
  Sub-Mix transmit slots when enabled. USB Mix Minus, physical output mapping,
  actual input routing, and behavior across effect changes remain unqualified.
- Shared mutable state and interrupt masking have not been proven safe. There
  are no measured target deadline, stack, memory-ownership, or output bounds.
  A forced emulator interleaving has shown that audio can reach a newly
  published callback before the selecting task continues, so callback state
  must be ready before that publication. A standalone admission model now
  excludes simultaneous live and file-mix leases and drains the active lease
  in host tests. A QEMU-only prototype surrounds the stock live wrapper and
  file-mix call, but uses unowned scratch and does not cover selection. It requires a safe response
  to rejected entry, owned persistent storage, appropriate state handling for
  both paths, and all users to acquire a lease before loading a callback or
  touching shared scratch; these remain unproven. Its
  admission rejects unexpected gate state, but its strong atomic operation
  can retry on the target. Safe rejection behavior and deadlines for the atomic
  sequence and selector wait remain unproven. The diagnostic's rejected-live
  branch clears two live effect buffers; if live calls overlap, it can write
  buffers in use by the admitted call despite having failed to acquire a lease.
  A separate modeled bypass lease can exclude other live entries while file
  mixing holds the effect lease; it grants no access to effect state. Host
  tests cover concurrent claims and selector drain. In one QEMU-only forced
  bounce, an outer lease covered a complete live block and its bypassed
  wrapper while file mixing continued. This is not a firmware integration:
  stock resource separation, deferred work, selector-closed rejection,
  output continuity and hardware behavior remain unproven.
  Static inspection shows file mixing calls the same four pre-callback
  helpers as the live wrapper, including mutable pending copies and shared
  ramp updates. A QEMU trace observed those helpers during the mix and none
  during one leased bypass. Running them concurrently or simply discarding
  their live-side work is not yet qualified.
  One injected word-copy request remained pending through a bypass and was
  consumed before the same QEMU file mix returned. Another injected on the
  final callback remained pending at return and was consumed after stream
  restart in one traced run. This does not cover real producers, competing
  requests, aborted mixes, failure to restart, other destinations or the
  doubleword path.
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
  The versioned protocol invalidates a previous snapshot when the selector
  enters AINR again; host tests cover a stale token and an in-flight writer,
  and a QEMU debugger experiment exercised the selector hook with a seeded
  stale token. That experiment does not cover every selection path.
  The pause therefore does not establish a coherent snapshot. A smaller
  temporary prototype captured 512 bytes and reconstructed the remainder
  after selection; under injected signal the resulting state differed from
  its capture-time observation despite a successful handoff status. That
  reconstruction was discarded.
- An optional QEMU-only outer-block pause parks the observed `AudioProcess`
  task before the complete block rather than bypassing individual AINR calls.
  It deliberately delays audio, uses unowned scratch, and has no established
  deadline, audio continuity, DMA exclusion, or coverage of other users. It
  is an investigation, not a hardware-safe synchronization strategy.
  In a forced 20-tick timeout, 14 modeled receive completions updated a
  shared pointer while the audio task was parked; 13 semaphore gives failed.
  One-second DMA and transmit-underrun summaries did not reveal that loss.
- A QEMU-only attempt to retain the candidate snapshot in two slices of the
  stock effect buffer also failed. The sampled source was nonzero during
  capture, but both slices were zero by installation even though the capture
  protocol still held a completed token. Stock effect accesses to the second
  slice were independently observed between selections. Skipping the stock
  clear is not sufficient to reserve those slices; see the
  [transition investigation](firmware/l6max-effect-callback.md#split-effect-buffer-snapshot-experiment).

Some standalone DSP defects have been fixed and tested on the host. Those
tests do not exercise the firmware bindings or establish hardware safety.
An out-of-range Velvet bank tap count now silences that bank instead of
panicking; host and forced ARM tests cover the stereo and shared-history
paths. Other malformed processor state remains unqualified.
Out-of-range Velvet history write positions now silence the stereo or shared
history processor before either channel is written; a damaged inactive cascade
history is not advanced. Host tests cover both channels and the cascade at the
ring boundary and `usize::MAX`, and the focused test also passes under Miri.
In one disposable ARM diagnostic, a debugger write changed the first channel's
position from 2,484 to 4,802; the position remained invalid and the callback
count advanced from 2,890 to 5,347. The diagnostic uses unowned memory, and
this observation does not establish output quality, deadline, concurrent
corruption behavior, or handling of other damaged state. The diagnostic MAIN,
recipe and result SHA-256 values were
`1e1133fabcd172156ac631833bd52bd0265891776e29a7003a30e5087bb227e5`,
`a7da82c80c31442788d56a6a263eb9d032840660364ab0a0ead6d492542bd3cb`, and
`7e902b4f59c35351530227ec395c4379179d435810f862d769e6f94f12b7953f`.
The stock MAIN and QEMU binary were the identities used in the recorded-project
probe; the QEMU source is unattested. The diagnostic image must not be installed.
Plateau's zero-diffusion setting now bypasses its input diffuser chain while
advancing that chain's history. Its input allpass gains stay fixed, while its
four tank allpass gains follow ±0.7 times diffusion, as in the supplied
Multistomp control implementation. The input bypass and tank gains ramp over
256 frames on edits. Host tests cover the input-to-tank path, tank impulse
response, an active-signal first-frame edit, reversals, freeze and idle edits;
target execution time and audible transitions remain unqualified.
An ARM QEMU impulse diagnostic after this correction reported Velvet and
Plateau peaks of approximately 0.302 and 0.034 with completion marker
`0x46585031`. The stock MAIN and initial NOR SHA-256 values were
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461` and
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`.
The diagnostic MAIN, ELF, recipe and result were
`b7b2b2e4cc0767904abd0b03cb0042c0853dd20c134c3789a452221285d8cd1a`,
`f0a2528969064c19b28a99a109ab5a997c5a9a71de322d42b4241a7420997f7d`,
`75dec91ec4132a16a712cd28da523cd89b48174a6f677197e13eefe0fb45842b` and
`7b31cec633e51b192a5fef1f8c7a580f5281cbb0be9ebcd7c8b39e445bad628e`.
QEMU was `eade074b5d7a6fe9d74bbbe1ad36a34f7356cd200cbfc81e772f2a21892012e8`
with unattested source; `l6dbg` was
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`.
The impulse alone does not establish audible quality or hardware safety.
Plateau's low-cut and high-cut controls now update the tank filters without
retuning the fixed input filters. This follows the supplied Multistomp control
mapping, though this port uses different one-pole filter equations. Host
tests exercise filter responses and independent histories across cutoff edits.
A later ARM QEMU impulse diagnostic reported Velvet and Plateau peaks of
approximately 0.302 and 0.048 with marker `0x46585031`. It used the stock
MAIN, initial NOR, QEMU and `l6dbg` identities immediately above. Diagnostic
MAIN, ELF, recipe and result SHA-256 values were
`0ef3fab195941606913b98c6f3c5b0187eee886ae31cb01c5e940dc07ab39d87`,
`abf26d77dcbd4e20331359030bd0cac1fd93667414a3cb61cf729697d99e8ff9`,
`38a31af9a7f1ea237e6e673515e5b8edb2fd7f7469fba706d841ab5409d0aa0b` and
`5db4eeb3cc48690b33048ffad87fe10e4210a00dab713db731bc92d9025840b7`.
This does not qualify audio parity, live-control transitions or hardware.
Tank low-cut and high-cut edits now ramp their filter coefficients over 64
frames, including while the reverb is pristine or frozen. Host tests cover
retargeting, clear, separate filter histories and idle advancement. The
extra work has not been timed on hardware. In one QEMU-only inspector run,
16 queued turns on the two damping encoders were consumed with no queue
overflow; the final high-cut display was `8500.0 Hz`, and the inspector later
closed. The run did not separately capture the low-cut value or qualify audio
transitions. It used the stock MAIN, initial NOR, QEMU and `l6dbg` identities
above. Diagnostic MAIN, ELF, recipe and result SHA-256 values were
`663f6ac1b5cb3aa40cf1968352737011524872bbd82e5e995828eac518c08a98`,
`f087837891939d913dcfde21ae8e76ac661d1e8784453e5231f9e2a5b7836eab`,
`b8cd44b8b2c9abae2551716b402f43c0b8657cf8607f64842604a46c05d60b6d` and
`fa6e5a2cd17fc98423c8b32bb0ebade12d3a917b0dc976bed2370161bc392bc7`.
The diagnostic uses unowned scratch and cannot qualify physical hardware.
The QEMU-only parameter handoff now settles saved Plateau controls on freshly
initialized state before callback publication, without clearing the large
histories again. A host test compares the first 16,000 frames with a freshly
initialized processor using the same saved settings. In one inspector run,
the displayed predelay was `28.0 ms` before an effect change and `30.0 ms`
after reselecting, reopening the Plateau bank and turning it once; the queue
recorded 19 consumed turns and no overflow. This does not establish concurrent
mailbox consistency, callback lifetime, output continuity or target timing.
The stock MAIN, initial NOR, QEMU and `l6dbg` identities match those above.
Diagnostic MAIN, ELF, recipe and result SHA-256 values were
`bc8c69e739d3d3a48e9380cad034fe2852ecb24c3ffd14e71a88ea9221e2c215`,
`758dc5d661fb6bb4bdb17cc831721a1df8601532eac812a09298706a59b0c8d4`,
`2fb50935ab6f119f5c2447a5a45ed2f1ba4a7e72ab7afcd781bbffb004025e9a` and
`57a8721b2cbfa3bd80193bcccd973a9cfa87718c868bbe086e00a015bc3eb681`.
An additional host comparison records all 16 inspector values and compares
both effects and the cascade from the first frame over four parameter sets.
This exercises saved values, not concurrent edits or firmware integration.
Plateau predelay edits now crossfade read positions over 64 frames; a rapid
retarget starts from the preceding predelay output, and freeze cancels the
transition without retaining frozen input. Host tests cover reversal, idle
advancement and freeze; target timing and audible quality remain unqualified.
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
The allpasses now consume the preceding frame's delay output, matching the
supplied Multistomp core's ordering. Fixed, modulated and short-delay impulse
checks match that core; this adds 32 bytes of processor state and does not
establish whole-effect parity, physical timing or audible quality.
Scaled input and tank delay times now retain their fractional samples and
interpolate between history values, as the supplied Multistomp core does.
Freeze intentionally holds tank delays at integer samples to sustain energy.
Reference impulse checks and host freeze tests cover these paths; audible
transitions, full parity and target performance remain unqualified.
A modeled-input QEMU capture passed the existing send and nonzero-return
checks after this change, but its single Plateau return block peaked at only
`1.49e-8`; that check alone does not establish useful output level. A separate
four-second host render of a 500 Hz tone at default size had per-second stereo
RMS between 0.054 and 0.063 in both the preceding version and a candidate
before the maximum-size boundary adjustment. These finite
observations do not establish the cause of the quiet capture or audio fidelity.
An extended capture of the same modeled stimulus observed 3,080 Plateau
callbacks, including 1,672 blocks with active sends. Its largest send was
`3.92e-5`, and the largest left and right return samples across that selection
were `1.42e-4` and `1.16e-4`. Its one-shot return capture was only `2.24e-12`,
showing why a single block is insufficient to assess output level. The
extended checker also requires each channel's peak to exceed 1% of the send
peak and rejects invalid or out-of-range samples; this is a diagnostic floor,
not an audio quality or hardware qualification.
The diagnostic MAIN, ELF, initial NOR, recipe and result SHA-256 values were
`d6b7b798390eb43fdddbfd41542ac6dbbe5bfb058304ed74994dd1d454f38b14`,
`4ec3bb636a8208ab3fd3504240c9df6a01d75824e21a9d1f39d3eadb8fe7f12a`,
`987b5edc2ae4e6c95a8021b476e31842e7202148769cf38c4fce22180935d321`,
`51a6b89da307cc1cc92b7f6a42174435e50aeb5dedf635671b1b353686b6ee50`, and
`3609f2f46776d0bc5baf3e0d2aabe1519be75daaebe54b690987f900282e7fd4`.
It used stock MAIN and source NOR identified above, modeled-input QEMU
`eade074b5d7a6fe9d74bbbe1ad36a34f7356cd200cbfc81e772f2a21892012e8`
with unattested source and `l6dbg`
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`.
The extra observation changes execution timing and uses unowned scratch.
The fractional-delay diagnostic MAIN, ELF, initial diagnostic NOR, recipe and
result SHA-256 values were
`366dc235cf6a92e3c34a6c5590ea53816163250a3461fdd8fd89d919ff25bc9c`,
`82bc2c4e1e620cfe640db04f138673d4adf1cb5e3f5e51c57500ad19814f0eee`,
`54feb7372272d5e6fcd33d0762bfee3a339fbb1d2345d252611cebdf1f316134`,
`2762dc523196a9fa964267a99cc71fb834c533b65576e17e6b5376ee3b80d7a5`, and
`5249e55fe09c688ce6b37a983cd9e2bff6983bf09408fabb3d756e48cdc1f3e3`.
It used the same stock MAIN, source NOR, modeled-input QEMU and `l6dbg`
identities as the following capture. The diagnostic uses unowned scratch and
must not be installed.
An active modeled-input QEMU capture passed the existing send reconstruction
and finite, nonzero stereo return checks for both effects. The diagnostic
MAIN, ELF, initial diagnostic NOR, recipe and result SHA-256 values were
`64cc0b3f69cc9f35e78d14ef4dac577e1eaa40b6ebaa32137d8c894b1c8e865d`,
`2e7227321e13aba7115a5e925b707503d7d65e3d19bdae771852eba8d51e1a2e`,
`6fbea7311281a85efa6928504d5dc79b7bf6ea9ad36c7d04abc8e469786607e4`,
`c943e41053e889e05bca8dc842317af50e415c83d9fff50154df2fa41eb7df5a`, and
`2e686440a968fdd3d9349280412eabaab2f030cf5e8c6d22cbaac5922a8a9b00`.
Stock MAIN and input NOR matched the identities above; QEMU was
`eade074b5d7a6fe9d74bbbe1ad36a34f7356cd200cbfc81e772f2a21892012e8`
with unattested source and `l6dbg` was
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`.
The diagnostic uses unowned scratch and must not be installed.
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
Dynamic Velvet renewals now use consecutive generation numbers for left and
right, matching the pinned upstream core's channel sequence after the two
initial banks. A host test checks selected tap delays and signs across four
banks at one parameter setting. Filter edits use this port's incremental
transition behavior, and this test does not establish complete audio parity.
Editing the transition length during an active renewal now schedules the next
renewal using the fade duration actually completed. A host test covers both
lengthening and shortening a fade; target timing remains unqualified.
A disposable ARM diagnostic selected the custom callback, opened the
inspector, applied eight queued edits without drops and closed it. Its MAIN,
ELF, recipe and result hashes were
`5ac73427509f9f9614c6c3d43dd3a5b97682d11b5bfdf1246ad5a9cc3890c754`,
`7bf4c1a9b6e69b458cfa0a8b887a2c8ab6c713c4008d3103ce5369ded72d4b66`,
`b3b7a0c38b8ee5ab6f9a63bae0d385f211af72a7dd2ab530cff3560adcd1814d`, and
`af20172316b02edf7e95d48be9b9b918e62d6faa828570f4e6fa8abb8958e35b`.
The stock MAIN, initial NOR, QEMU and `l6dbg` matched the identities in the
following renewal diagnostic; QEMU source is unattested. This uses unowned
scratch and does not establish physical timing or audible quality.
At 285 taps, a 100 ms renewal and a 90 ms fade, the former one-tap-per-frame
normalization delayed subsequent fade starts by 90 samples. The builder now
normalizes up to two taps per channel per frame; host tests check the requested
4,800-sample cadence, per-step bounds and identical final tap values. The extra
per-frame work still needs target timing qualification. A QEMU-only long-fade,
active-send diagnostic observed 1,664 callbacks in five modeled seconds and
the inspector remained closed; this is below the nominal 750 callbacks per
second and is not a physical timing measurement. The diagnostic MAIN, ELF,
recipe and result SHA-256 values were
`a9ff7fe8747449426b013149ded1b5bf343497adcc96052e51a79d58299cbcbf`,
`4ea6b3fb142ca4054a9a996d37f29721c710f47d1eb20c0f8fd4aef0ec3405bf`,
`f8427a6c99234b642341742b26053340634a391cb4633da6f76a9abafbe99a9e`, and
`462856e21b4f32409a828c511406418aa6c3b24d8bb83409e5a5282430ab1d31`.
It used the stock MAIN identity above, initial NOR
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
and QEMU `4902e96e8375adb69b1ebe0e5572fe1ba3fb6a0e413c8595605128a297dfd910`
with unattested source provenance. The `l6dbg` executable was
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`.
No hardware was tested.
Its circular-history index calculation now avoids the prior wrap selection
without adding delay storage. A host comparison matched 40,000 output frames
bit for bit. Two matched high-density QEMU runs improved from about 416 to
444–445 callbacks per modeled second; the inspector stayed closed in both
variants. These observations do not establish a target deadline or physical
audio behavior.
A speed-optimized diagnostic of the same full-density workload reached about
497 modeled callbacks per second with the inspector still closed. Compiler
stack metadata covers individual functions, not the complete callback stack;
the patch builder still uses its existing settings and remains disabled.
The aligned shared-history loop now uses unchecked array reads only after an
in-range position check and delay normalization; other positions retain checked
reads. Exhaustive index tests and selected AddressSanitizer tests passed, but
the full target memory and concurrency model is unqualified. Focused aligned,
invalid-position and malformed-delay tests also passed under host Miri. A matched
emulator comparison reached about 527 modeled
callbacks per second and still did not open the inspector. This does not
qualify physical timing, memory ownership or behavior under corrupted state.
The single-history loop now uses the same index bound, returning silence for
an invalid position. In a separate one-history QEMU workload it advanced
about 495 callbacks per modeled second before and 526 after the change;
neither inspector opened and physical timing remains unmeasured.
With maximum density and a 100 ms renewal interval, a paired QEMU stress
probe reached about 532 callbacks per modeled second with a 1 ms fade and
339 with a 90 ms fade. Neither inspector opened; pre-AINR was omitted.
Fade-state sampling and other instrumentation perturb execution, and these
figures do not establish target timing.
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
A later matched diagnostic using the current external QEMU and the normal
L6max audio-queue behavior counted 1,697 active high-density callbacks and
2,513 bypass callbacks in five modeled seconds, both below the nominal 3,750.
The shortfall in the bypass control means that run cannot isolate effect cost
or establish a physical deadline.

Before re-enabling the builder, trace the relevant firmware interfaces against
the exact supported MAIN image; reserve and initialize state with a documented
ownership and synchronization model; validate routing, UI and audio behavior;
and record identities for the tested firmware, fixtures, and executable.
