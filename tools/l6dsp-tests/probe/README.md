# QEMU-only Cortex-M7 DSP probe

This diagnostic runs the real DSP initializer and a 2,048-frame impulse in a
stock effect buffer during stock effect initialization. It modifies copied
emulator firmware and NOR, temporarily replaces stock effect-buffer contents,
writes a result to QEMU-only scratch at `0x83e00000`, and does not create an
update package. The scratch address is not validated on physical hardware. **Do not install its output on a
physical device.** The supported patch builders still reject `dual-efx`.

From the repository root, using a disposable L6max 1.10 state whose installed
MAIN matches the unpacked stock image:

```sh
python3 tools/l6dsp-tests/probe/prepare.py UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-dsp-probe
cargo run --offline --locked -p l6dbg -- /tmp/l6-dsp-probe/recipe.toml \
  --firmware-dir /tmp/l6-dsp-probe/firmware --state-dir /tmp/l6-dsp-probe/state \
  --logs /tmp/l6-dsp-probe/logs --qemu QEMU_PATH
sha256sum QEMU_PATH
python3 -c 'import json,struct; j=json.load(open("/tmp/l6-dsp-probe/logs/diagnostic.json")); b=next(e["observation"]["bytes"] for e in j["events"] if e["observation"].get("name")=="impulse_peaks"); print(struct.unpack("<ffI",bytes(b)))'
```

The output directory must be new and under the system temporary directory.
The preparer verifies the full stock MAIN hash, the copied NOR's installed
MAIN and the displaced veneer hash. It writes only to the new directory and
reports the input NOR, probe ELF and modified MAIN hashes. Record those and the
exact QEMU hash with any result. The Rust toolchain, target and `llvm-objcopy`
must be installed; Cargo and `l6dbg` use the repository's pinned toolchain.

An observed result is approximately `(0.30966336, 0.03750144, 1180192817)`:
the first two numbers are Velvet and Plateau peak magnitudes and the last is
the `0x46585031` completion marker. This observation used stock MAIN
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
initial NOR `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
and QEMU executable `53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`
with unattested source provenance. The diagnostic MAIN, ELF, recipe and
result SHA-256 values were
`7a83a32ad3e73d329c6207bbc248c26a67c1bb10f67df3fd24c67ce1f393ac94`,
`85dd9e854200ce057f29382246ec059f2867b69d26bd2cceb6404c69a6464f25`,
`7688748588fcd0da093a2bd9f0c2f1d58feb4db3aa0f7d7bb9ecf8fb00ad9d40`, and
`82f96b336c65001ae9fc6654702b9180bcbaca1091a933d383c6eddc6f99a667`.
A successful recipe only collects this observation. It does not prove live
signal routing, effect-switch safety, callback timing, UI behavior, or
physical-device compatibility.

## Automatic callback diagnostic

For a separate disposable run, start with Hall selected and pass `--callback`
to the preparer. Its recipe presses EFX SEL once, then reads three 32-bit words
from QEMU-only scratch:

```sh
python3 tools/l6dsp-tests/probe/prepare.py UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-callback-probe --callback
cargo run --offline --locked -p l6dbg -- /tmp/l6-callback-probe/recipe.toml \
  --firmware-dir /tmp/l6-callback-probe/firmware --state-dir /tmp/l6-callback-probe/state \
  --logs /tmp/l6-callback-probe/logs --qemu QEMU_PATH
python3 -c 'import json,struct; j=json.load(open("/tmp/l6-callback-probe/logs/diagnostic.json")); b=next(e["observation"]["bytes"] for e in j["events"] if e["observation"].get("name")=="callback_state"); print(struct.unpack("<III",bytes(b)))'
```

The observed marker is `0x494e4954`, the selected descriptor is `0x2028b8dc`,
and the third word is the callback count, which should be nonzero. This
diagnostic replaces the selected stock callback and uses its effect buffer;
it reconstructs three sends from candidate mixer slots and gains. A QEMU-only
mailbox at `0x83e00100` contains 12 32-bit route values: 0 off, 1 A, 2 B,
3 A-to-B. The first custom selection initializes the mailbox to B for Room or A for
Spring. A debugger can change these values for controlled tests; this is not
a device UI or persistent configuration. Writing a nonzero word to
`0x83e00140` captures the three 64-float sends at `0x83e01000` for inspection.
Those addresses and the reconstructed gain/ramp lifetimes are not qualified
on hardware. The diagnostic does not establish source-channel mapping,
effect controls, UI, synchronization, real-time deadlines, or hardware
safety. Never install the generated image.

To observe selection changes, add `--switches` along with `--callback` when
preparing a fresh output directory. The resulting recipe presses EFX SEL six
times and records `active_1` through `active_6`, `callback_state`, and
`callback_state_2` through `callback_state_6`. For the setup-complete fixture
starting at Hall, the first two active pointers should be the same diagnostic
callback, the first two recorded descriptors should be `0x2028b8dc` and
`0x2028b8f8`, and the next four pointers should be `0x8002c041`, `0x20280781`,
`0x20280021`, and `0x20281649`. The custom callback count should be nonzero
for both custom selections. This is an observation recipe; check its captured
values rather than treating a successful runner exit as a pass. See the
[investigation](../../../docs/firmware/l6max-effect-callback.md) for evidence
and limitations.

## Inspector event experiment

Pass `--callback --inspector` to prepare a separate QEMU-only image. Its
generated recipe selects EFX mode and Room, opens the inspector with a long
EFX SEL hold, edits both parameter banks, revisits each, then closes the
inspector and turns the normal mixer control. The three
revision-specific call sites are byte-guarded. The input hook queues edits;
the diagnostic audio callback applies up to eight events per block and formats
their captured text itself. That formatting placement is not qualified for
real-time use. Summary
memory at `0x83e00400` contains parameter ID, value length, 16 text bytes,
consumed-event count and inspector-open flag. Queue head, tail and overflow
count are captured at `0x83e00200`.
The observed selector hook also closes the inspector when it reaches a stock
effect, including a direct selection outside the inspector button path. A
separately guarded post-fade veneer covers the AINR branch, which bypasses the
clear veneer. The selector holds a stock semaphore there; lock ordering and
timing with the popup manager remain unqualified.
Add `--retain-custom-state` to investigate direct Room-to-Spring transitions
without clearing the processor or resetting its event queue. This opt-in uses
an unowned QEMU-only marker at `0x83e00148`; it cannot be combined with
`--pre-ainr`. A switch to a stock effect still clears the stock buffer, and
the inspector and processor lifetime across such a transition are not
synchronized. The option does not qualify memory ownership, cold or warm
reset, the stock clear's other effects, concurrency, deadlines, or hardware.
The diagnostic inspector queue is initialized only on its first custom
selection, so pending edits can remain queued while a stock effect is active.
One forced QEMU interleaving demonstrated an edit surviving that round trip;
queue overflow, reset, physical event delivery and callback synchronization
are still unqualified. See the [callback investigation](../../../docs/firmware/l6max-effect-callback.md).
With `--gesture-trace`, `0x83e00500` stores a count followed by up to
20 triples of event, tick, and inspector-active flag, recorded at the selector
hook. The trace adds work to that hook and can change timing.
Add `--bank-stress` to repeat bank switching and encoder edits ten times,
capture the inspector state, then exercise close and stock selection. Inspect
the captured values; completion alone does not assert the expected behavior.
`--bank-stress-count N` changes the count within 1 through 64.
`--enqueue-trace` separately records the EFX SEL edge and tick just before
the stock panel parser enqueues it, at `0x83e00600`. `--timing-control` bypasses
effect processing and writes silence for a QEMU-only scheduling comparison;
it is not an effect implementation. Both options alter the diagnostic image.
`--active-sends` replaces the three reconstructed buses with constant nonzero
test signals and records the latest output samples at `0x83e00800`. It tests
active DSP workload rather than the physical input or mixer routing path.
`--irq-trace` additionally installs a guarded UART receive interrupt probe in
the disposable guest and records matching key edges at `0x83e00900`. It uses
an unowned low-memory veneer, changes interrupt execution, and is QEMU-only;
it is not a safe target hook.
`--irq-timing` uses that same experimental hook and a bounded single-producer
queue to time selector gestures from the interrupt. A missing, mismatched or
overflowed event cancels the gesture. The queue counters are captured at
`0x83e00b00`. A verified existing hook retains its decoder, queued edges and
bounded trace across effect selections; overflow recovery remains unresolved.
Neither this experiment nor its scratch memory is target-safe.

For the prepared fixture identified in the investigation, the expected
captured values are `26.0 ms` for Plateau and `84.0 %` for Velvet, followed
by `28.0 ms` and `86.0 %` when revisiting them. The mixer send changes
before and after the inspector but stays fixed during the edits. Check the
captured observations; a completed recipe alone does not assert them.

While the inspector is open, a short EFX SEL press switches the parameter
bank without changing the selected stock effect; pressing EFX changes between
parameter and routing pages. Turning a strip encoder on the routing page cycles that strip
through off, A, B and A-to-B. This experiment does not draw an OLED popup.
Effect changes can reinitialize the DSP, so tails can be lost. The queue is
retained within the modeled session, but overflow or reset can still lose
pending edits. Applied inspector values and routes are retained in QEMU-only
mailboxes across effect selections within the session. The parameter values
are read individually, not as a transaction with concurrent edits; mailbox
ownership, callback lifetime and reset behavior are not qualified.
The queue can overflow, and synchronization, context changes, stock patch
combinations, memory ownership and timing are not qualified. The scratch
addresses are emulator-only. Never install the generated image.

## Combined popup experiment

`prepare_ui.py` makes a temporary source overlay of knob-dialog and combines
its guarded hooks with the inspector diagnostic. It reuses the notification
manager to render copied parameter values on the display task. The normal
knob-dialog source is not modified. Its standard patch builder creates an
intermediate knob-only package, which the script removes; no combined update
package is created.

The combined experiment tags queued edits with the popup manager's current
generation in QEMU-only scratch at `0x83e02400`. Closing or changing the
inspector page cancels pending popup work; the audio callback applies an old
edit but only publishes its notification if its generation still matches.
The comparison and publication use the popup manager's critical section.
This avoids relying on a separately published queue cutoff, but scratch
ownership, all transition interleavings, generation wrap, and real-time
effects of the popup synchronization remain unqualified.

The combined experiment keeps its 16 saved parameter words in an extra 64
bytes of the popup manager's heap sidecar. If that sidecar is unavailable,
the inspector defers to the normal button and encoder handlers. The pointer
is recorded at QEMU-only scratch `0x83e07e00` for observation; the queue,
routing, gesture state and other diagnostic words still use unowned scratch.
The standalone inspector continues to use its scratch parameter mailbox.
This does not qualify allocation lifetime, reset, exhaustion, concurrent
selection, callback ownership, or the remaining scratch addresses on hardware.

```sh
python3 tools/l6dsp-tests/probe/prepare_ui.py UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-ui-probe
cargo run --offline --locked -p l6dbg -- /tmp/l6-ui-probe/recipe.toml \
  --firmware-dir /tmp/l6-ui-probe/firmware --state-dir /tmp/l6-ui-probe/state \
  --logs /tmp/l6-ui-probe/logs --qemu QEMU_PATH
```

The combined builder also accepts `--irq-timing`, `--active-sends`, and
`--enqueue-trace` for the corresponding QEMU-only experiments described
above. These flags do not enable an installable firmware patch.

The combined prototype shows the current page on opening or switching it.
The recipe captures Room and Spring popup frames and routing-page edits. Verify the captured widget
text and mixer behavior; a completed recipe alone is not an assertion. The
combined image uses emulator-only state, and does not qualify notification
contention, queue overflow, effect switching, timing or physical hardware.
Do not install it.

## Isolated AINR comparison

This focused QEMU diagnostic reruns the copied-AINR experiment described in
[the investigation](../../../docs/firmware/l6max-effect-callback.md#ai-noise-reduction-investigation):

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tools/l6dsp-tests/probe/ainr_isolation.py \
  UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-ainr-isolation --qemu QEMU_PATH
```

The output directory must be new and under the system temporary directory. The
script checks the stock MAIN revision and the MAIN in the supplied NOR, copies
the state, records binary identities and runs `l6dbg` with the copied state.
It waits for AINR learning, copies processor memory and guarded code into QEMU
scratch RAM, then compares 96 forced signal blocks and sampled state at five
checkpoints. `result.json` is written only after the comparisons and recipe
complete successfully. Logs and generated state remain in the temporary output.
QEMU executable provenance is recorded as unattested.

Add `--independent` to create a second relocated code and state copy receiving
a different waveform. A second stock reference is run with its own saved state;
each copy must match its corresponding stock output. Both outputs must be
finite and nonzero. At the five checkpoints each copy's sampled state must
match its stock reference, and the second copy must leave the checked stock
and first-copy ranges unchanged. The debugger swaps 128 KiB of stock state
for every block, so this mode takes longer. It does not verify denoising
quality, all memory accesses or execution without debugger stops.

This diagnostic modifies emulated execution and uses memory whose ownership on
hardware is unknown. It does not establish complete relocation coverage,
concurrent isolation, timing, pre-AINR integration, or physical safety, and
must never be used with a real device. It does not enable the dual-effect
patch builder.

## Pre-AINR send experiment

The `--pre-ainr` diagnostic calls three relocated AINR copies before the
routed effects on synthetic sends. It is a QEMU-only experiment: it modifies
emulated execution and uses scratch code and state with no established physical
ownership. **Never install or run its output on hardware.** It does not enable
the patch builder.

Prepare the fixture, then run the printed `l6dbg` command in one terminal and
the printed observer command in another. Supply the exact QEMU executable to
`l6dbg` and record its SHA-256:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tools/l6dsp-tests/probe/prepare.py \
  UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-pre-ainr --callback --inspector \
  --active-sends --pre-ainr
sha256sum QEMU_PATH
```

The recipe selects stock AINR and starts learning. After the learned snapshot,
the observer verifies the prepared fixture and learned call, copies the guarded
code and 128 KiB of state into each of three QEMU scratch regions, checks the
readbacks and enables the synthetic signals. The recipe returns to Room and
captures `selected`, `ainr_sends_1`, `ainr_sends_2` and `output` in
`logs/diagnostic.json`. For each `ainr_sends` observation, the first word is a
callback count; the following six words are three pairs of before/after `f32`
samples. Confirm that Room was selected, the count increases, and the samples
and output are finite. Recipe completion alone does not assert these results.

This experiment copies a stock learned snapshot; it does not implement a way
to learn an independent instance, prove denoising quality, validate physical
inputs or routing, establish scratch ownership, or meet a real-time deadline.
The observer must not connect to a real device. The results depend on the exact
firmware, state, QEMU binary and emulator timing.

Add `--effect-buffer` to that preparation command to test an alternative
layout. It places three guarded code copies in the erased application tail and
three 128 KiB state copies after the effects state inside the observed stock
effect buffer. The build rejects an oversized effects state or a non-erased
tail. The observer captures the learned state, waits for Room's initialized
callback, then copies the states while the guest is stopped and verifies that
the effects state was not changed by the copies. It holds the learned snapshot
on the host across effect selection; this is not a firmware initialization
scheme. Stock buffer ownership, callback publication, concurrent users, update
packaging and physical memory are unqualified. Never install this image.

Adding `--auto-handoff` as well tests a device-side handoff without the
observer. A guarded QEMU-only veneer calls the stock AINR routine, waits 10,000
firmware ticks after the learned flag appears, then copies 128 KiB from the audio task
to a stock-heap allocation. The effect initializer copies that snapshot into
the three test states before publishing the replacement callback. The recipe
records `heap_before`, `handoff_learned`, `heap_learned`, `handoff_selected`,
`heap_selected`, all three send blocks, and the copied descriptors. The
handoff words are allocation pointer, status (0 waiting, 1 captured, 2 failed),
capture start/end ticks, source and first-copy state words, and learning-start
tick. Inspect these observations; recipe completion alone is not a pass.
Add `--passthrough-unready` to keep synthetic sends active without AINR while
the snapshot is unavailable, including after a diagnostic allocation failure.
The resulting extra work can prevent the selection sequence from reaching
AINR in QEMU; inspect the selected callback and learned flag. This tests
passthrough in the probe; it does not implement a firmware fallback.

This prototype retains a large heap allocation, copies 128 KiB on the audio
task, uses unowned control words, and assumes an experimental settling delay.
It does not establish snapshot coherence, stock coexistence, deadline safety,
reset behavior or a usable learning workflow. Never install it on hardware.
