# QEMU-only Cortex-M7 DSP probe

This diagnostic runs the real DSP initializer and a 2,048-frame impulse in a
stock effect buffer during stock effect initialization. It modifies copied
emulator firmware and NOR, temporarily replaces stock effect-buffer contents,
writes a result to QEMU-only scratch at `0x83e00000`, and does not create an
update package. The scratch address is not validated on physical hardware. **Do not install its output on a
physical device.** The supported patch builders still reject `dual-efx`.

The stock clear veneer also has a caller outside effect selection. The
diagnostic dispatch accepts only the two observed selection return addresses;
other callers go directly to the stock clear without running the diagnostic
handler. This is revision-specific and does not establish that every valid
selection path has been identified or that caller identity is sufficient for
hardware safety. The dispatcher can be checked while a disposable `l6dbg`
run is active with its GDB socket enabled:

```sh
python3 tools/l6dsp-tests/probe/clear_dispatch_check.py OUTPUT/gdb.sock OUTPUT/probe.elf
```

The checker pauses the guest, forces two selector returns, the other known
return and an unrelated return through the dispatcher, then restores the
registers. It stops before either destination executes. Match the ELF and
firmware identities separately. One QEMU run reached the diagnostic handler
for both selector returns and stock clear for both other returns. Its stock
MAIN and initial NOR were
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461` and
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`;
diagnostic MAIN and effect ELF were
`341dbd589b97c1242837928761c7aadc8ff10e2d033a7ff571b8549a1384248c` and
`550bdca2fb6c79e18badc8b116508a2032642c3b336ec1ff6a0995dc6bf21c19`.
The `l6dbg` and QEMU executable hashes were
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241` and
`4902e96e8375adb69b1ebe0e5572fe1ba3fb6a0e413c8595605128a297dfd910`;
the QEMU source is unattested. The ordinary inspector diagnostic also opened,
processed eight queued edits, and closed with no drops in one separate run.
Its recipe and result hashes were
`301eecf0c229f0e3c9d889b35c91647c9ee65dd88821358167f0827ac85de8c8` and
`a6d945746de13feb565433ef55cc907362b2c8d1435b109d6552b1efe88e0085`.

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
For a diagnostic comparison, `--opt-level 3` selects speed optimization
instead of the default `z`; `--stack-sizes` emits per-function stack metadata
that can be inspected with `llvm-readobj --stack-sizes OUTPUT/probe.elf`.
Individual frame sizes exclude callers, nested functions and interrupts and
are not a stack bound. These flags do not change the patch builders.

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

## Callback admission model

Prepare a separate image with `--admission-probe` to exercise the standalone
atomic admission model on the emulated ARM target. The `admission_result`
observation should contain `0x3ffff` and marker `0x41444d31` in its first two
32-bit words. This is a sequential interleaving test, not an integrated
callback or real-time test. It uses unowned QEMU scratch; never install it.
Host tests also force a delayed pointer load and concurrent entry and close
operations. Both callback paths must participate before loading a callback
pointer or touching shared scratch; full live and file-mix leases are exclusive.
The modeled live-bypass lease excludes another live lease while permitting a
file-mix lease, and must not access effect state or other shared resources.
Its firmware resource boundaries have not been established. Storage ownership,
rejected-entry behavior, time bounds, other users and hardware synchronization
remain unresolved.

One sequential ARM emulator run returned `0x3ffff`. Its stock MAIN and source
NOR SHA-256 values were `980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`
and `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`.
Diagnostic MAIN, ELF, initial NOR, recipe and result were
`744d55c8cd9007bc278638db486124eb483f9bb1d1bff45b7cc28785d86c98bd`,
`8ce08a5ebfdd1d96be45651fbc9e627e212d7cdf3b4d65a7f8cc2ca271823e0d`,
`e9f6111ebb8a5a56465d1350e4f2b87156c2c00a21ddc448bf933e945f1e5c18`,
`1c9d727681af9cadcffdefe8f24f1e6743d60cc5ff28e538c303a9c9acff1d74`, and
`960356a96cd3b0e701c45f49b3108fbc24461652aa73a950c81239c3d739f353`.
The QEMU and `l6dbg` binaries were
`4902e96e8375adb69b1ebe0e5572fe1ba3fb6a0e413c8595605128a297dfd910` and
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`;
the external QEMU source is unattested. The image must not be installed.

## Recorded-project file mix

The opt-in project probe reconstructs effect sends from the file-mix track
workspace and compares one captured block with the stock effect input. It
uses unowned memory, guards the whole audio block experimentally, and drops a
rejected block; it is **not safe to install**. It does not test real-time
deadlines, physical memory ownership, the routing UI, output fidelity, or
concurrency across all users. It does not inject a synthetic interrupt.

Use a fresh output directory and a disposable state matching the stock
firmware. The generated card starts with EFX off; the recipe selects a custom
effect before raising sends on channels 1 and 2:

```sh
python3 tools/l6dsp-tests/probe/prepare.py FIRMWARE STATE OUTPUT \
  --callback --file-mix-handoff atomic --file-mix-project --opt-level 3
cargo run --offline --locked -p l6fs -- card build OUTPUT/card.img \
  --spec tools/l6dsp-tests/probe/bounce-card.toml
L6_SERVICE_AUDIO_QUEUE=0 target/debug/l6dbg OUTPUT/recipe.toml \
  --firmware-dir OUTPUT/firmware --state-dir OUTPUT/state --logs OUTPUT/logs \
  --qemu QEMU --gdb-socket OUTPUT/gdb.sock --sd-image OUTPUT/card.img
python3 tools/l6dsp-tests/probe/file_mix_handoff.py OUTPUT/logs/diagnostic.json --project
```

The checker requires a completed bounce, nonzero channel sends, bounded and
aligned offsets, finite input and a nonzero B-only send matching the first
captured stock block. It also requires finite, nonzero and bounded effect
output on both channels across the observed calls. Input matching covers one
block; output statistics do not establish fidelity. Preserve
the firmware, NOR, card, recipe, QEMU and result identities separately; the
external QEMU binary's source is unattested.

To investigate whether this particular bounce touches the live effect buffers,
compile `file_mix_buffer_trace.c` with the plugin-enabled QEMU headers as in
the [stock clear trace](#stock-effect-clear-trace), and use the resulting
plugin in the QEMU wrapper. Inspect `MIX_TRACE` lines in `engine.stderr` as
well as running the project checker. The trace counts CPU accesses to
`0x20010210..0x20010410` before and during the mix observer's marked window;
each in-window hit reports its instruction address and read/write count.
Require nonzero pre-window counts, a nonzero `workspace_read` count, and zero
overflow before interpreting an absence of hits. This instruments CPU memory
accesses, not DMA; it covers one diagnostic workflow and changes timing. The
observer, diagnostic callback and admission guard all modify stock behavior.
The result cannot prove non-reentrancy, ownership, or safe hardware operation.

To force an audio wake while a recorded-project effect callback holds the
file-mix lease, add `--file-mix-project-pending` to the preparer and
`--project-pending` to the checker. This variant checks a successful signal,
the held file-mix gate before and after a short delay, and one rejected audio
block. It intentionally drops that block and uses unowned scratch; it does not
test a production fallback or prove that a pending wake happens naturally.
Adding `--file-mix-project-inner` to that preparer instead guards only the
effect wrapper and lets the surrounding audio block run. The rejected
wrapper enters the stock no-callback branch, which clears shared live buffers
and skips its normal deferred work. It is unsafe if live invocations overlap
and does not guard the rest of the mixer. This is an intentionally incomplete
QEMU-only experiment, not an installable fallback. Use the same checker and
inspect the trace and its overflow count separately.

Alternatively, use `--file-mix-project-leased-bypass` with the project and
pending options, and add `--project-leased-bypass` to the checker. The outer
hook holds a modeled live-bypass lease across the stock block, while a wrapper
hook takes the stock no-callback path. The checker requires the forced wake,
both lease bits at the wrapper, one bypassed wrapper and a completed block.
The diagnostic assumes every live
wrapper entry comes through the outer hook; it uses unowned scratch, skips
deferred effect work and drops a block if both admissions fail. Stock global
sharing, the selector, DMA, control updates, output continuity and target
timing remain unqualified. Never install it.

One instrumented run recorded 6,144 routed calls, a maximum offset of 131,008,
a maximum captured input difference of `1.46e-11` and 785,470 nonzero output
samples. Left and right peaks were approximately `0.00042172` and
`0.00042241`. Its stock MAIN SHA-256 was
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`;
initial patched NOR, diagnostic MAIN, initial card and recipe were
`5300dbe0b90fa96959a0735d51b7ffc86896207e67a3d77b35035f76ab6bacbd`,
`937a880e4a89a126f07d0f83da20838cc97b574a98cbf03ad350c15cd89fafe0`,
`fb1a78fb2bbce8d8abb6e2f87da342e96aad6a02545bf6bfdecf2c564deb2c44`, and
`f8bc2dadd7668df7eccb75c2e08a01baa316eae96a6a818a437279f3abd5655b`.
The QEMU executable, `l6dbg` and `diagnostic.json` hashes were
`4902e96e8375adb69b1ebe0e5572fe1ba3fb6a0e413c8595605128a297dfd910`,
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`, and
`0404677957bfd2b3c6c338013f4990a475f32e71b157152799340d3b1dc48c41`.
The input NOR was
`1f2df1149cd7f7d028653d25e0e0f42c6de6b4b92b43595e5e089ad9c11e1a91`.

## Forced file-mix handoff

`--callback --file-mix-handoff pending` queues a stock file-mix workflow
after selecting a diagnostic effect, then forces an audio semaphore signal
after stream shutdown. `--file-mix-handoff bypass` adds a QEMU-only live-wrapper
entry bypass during that interval. `--file-mix-handoff atomic` instead uses
the admission gate around the live wrapper and the complete stock mix call.
All modes require a fresh output directory under
`/tmp`, a disposable state and an `l6dbg` GDB socket for the trigger. For each
mode, substitute the same mode and a new `OUTPUT` path:

```sh
python3 tools/l6dsp-tests/probe/prepare.py FIRMWARE STATE OUTPUT --callback --file-mix-handoff bypass
target/debug/l6dbg OUTPUT/recipe.toml --firmware-dir OUTPUT/firmware --state-dir OUTPUT/state --logs OUTPUT/logs --qemu QEMU --gdb-socket OUTPUT/gdb.sock
python3 tools/l6dsp-tests/probe/file_mix_handoff.py OUTPUT/logs/diagnostic.json --bypass
```

For the `pending` control, omit `--bypass` from the final command; use it for
both `bypass` and `atomic`. The checker
expects the no-card workflow to return 9, one successful forced signal, and
callbacks both before and after the handoff. It distinguishes the observed
late callback from the bypassed case. These experiments use unowned memory,
an injected signal and a delay; the `bypass` mode also uses a non-atomic flag.
The atomic mode initializes its gate only for this forced work item and does
not hook selection or all other users. None implements the production admission
protocol, tests a successful mix or proves safe ownership, selection
synchronization, target timing or physical behavior. Never install these images.

With `--file-mix-handoff atomic --selector-admission`, the disposable image
also closes admission after the stock selection fade, drains active leases,
and releases the guard before fade-in or at selector cleanup. During selector
closure it calls the stock float ramp generator and writes its final sample to
an indirect destination; this approximates work normally performed by an
effect callback and is not qualified as equivalent or safe. Its recipe cycles
further through stock effects, including AINR. Add `--selector` to the checker.
A timeout aborts selection without a qualified recovery. The experiment assumes
its scratch starts at zero, initializes the gate on the first participating
entry, and fails closed if initialization is in progress or scratch has an
unexpected value. Reset behavior, preexisting work, ramp destinations, scratch
ownership and all selection callers remain unproven. This experiment must not
be installed.

Add `--selector-overlap` when preparing that image and `--overlap` to the
checker to force an audio semaphore signal and an eight-tick pause with a live
lease held during one selection. The checker requires a closed gate with a
live lease, at least one wait, and a subsequently drained gate. This deliberately
changes scheduling and does not establish physical timing or every interleaving.

Add `--selector-timeout` to make that held lease outlast the diagnostic's
20-tick wait. Check the result with `--timeout` instead of the normal flags.
On timeout, the diagnostic replays stock fade-in and conditional flag cleanup,
but does not complete the requested selection. The checker explicitly reports
the observed effect-selection mismatch and skipped request; a successful check
means the partial recovery was observed, not that timeout handling is correct.

Add `--selector-feedback` to experiment with rollback in the observed panel
handler and defer its effect-selection MIDI message until after that handler's
selector call. Use `--timeout --feedback --midi-trace LOGS/engine.trace` to
check the Room-to-Spring timeout, next button press, sampled L6max lamps and
modeled DIN messages. This uses unowned shared status, changes MIDI ordering,
and does not cover other callers, simultaneous selections or all failure
cases. Add `--selector-ainr-timeout entry` or `exit` and check with
`--ainr-timeout entry` or `exit` to exercise AINR transitions. The entry
experiment skips post-selector enable work on timeout. The exit experiment
defers stock teardown until selection succeeds, runs it before reopening the
wrapper gate, and keeps the sampled AINR fade value on timeout. This changes
stock ordering and is not qualified for
real audio. Add `--selector-ainr-learned` to attempt learning before the exit
timeout, and `--ainr-learned` to require the learned marker in the checker.
Neither case proves complete AINR state preservation or installable firmware.
The stock AINR dispatcher has been observed running while this wrapper gate is
closed, so the gate does not exclude all processing during teardown.

For the exit-timeout experiment, add `--ainr-admission` to redirect the
observed AINR dispatcher veneer through the same experimental gate. Pass
`--ainr-admission` to the checker as well; it requires both stereo calls to
be admitted during normal processing and rejected during guarded teardown,
and rejects any observed call outside the in-place, two-channel form. A
rejected in-place call leaves its buffer unchanged. This probe does not
establish the dispatcher's return-value contract or the behavior of other
callers. It admits the two channels separately, so a transition can occur
between them. Scratch ownership, all other users of AINR state, target timing
and audio quality remain unverified. Never install this diagnostic image.
The same option can accompany the entry-timeout experiment; its checker
requires both channels to be admitted and no unknown call forms, but does
not require teardown rejections because that transition enters AINR.

Add `--ainr-pair` to replace the observed caller's second dispatch with a
single paired call inside the first call's live lease. The preparer checks the
full stock instruction sequence before changing it. With `--selector-overlap`,
the diagnostic forces its delay after the first dispatch and before the second,
while the lease is held. The counters report decisions for both channels;
they are not a comparison of stereo audio output. The shared veneer, caller
register effects, other entry points, signal behavior and memory ownership are
not qualified. This variant is also QEMU-only and must never be installed.

To compare the stock and paired calls on distinct synthetic left and right
signals, add `--ainr-pair-replay` to the exit-timeout preparation command.
Start its generated `l6dbg` command and wait for `OUTPUT/logs/learned_ready.txt`.
While that disposable guest is running, execute:

```sh
python3 tools/l6dsp-tests/probe/ainr_pair_replay.py \
  OUTPUT/gdb.sock OUTPUT/firmware/main_firmware.bin
```

The replay stops the guest, rewrites its executable memory and restores a
sampled state range and stack between the two calls. It compares 16 blocks,
the sampled state, and register values; it requires finite, nonzero output
on both channels. Add `--check-closed` to force the diagnostic gate closed for
each block and check that both input buffers and the sampled state are
unchanged, both rejection counters advance, and the call returns zero. That
does not establish whether zero is a safe return value for other callers.
The reported main hash identifies the supplied image, while
the guest code checks cover only the observed pair and veneer. The debugger
perturbs timing and the replay does not restore or compare every possible
side effect. Stop the disposable guest afterward; never connect this script to
physical hardware. If the script fails, the guest may remain paused or modified.

## Synthetic file-mix layout

To examine the candidate file-mix track layout, prepare a separate image with
`--callback --offline-mix-layout`, run its recipe, then check the observation:

```sh
python3 tools/l6dsp-tests/probe/offline_layout.py OUTPUT/logs/diagnostic.json
```

Add `--offline-effect-workspace` to preparation and `--effect-workspace` to
the checker to place the 768-byte workspace at `0x80db0004`, after the three
candidate AINR regions within the observed effect buffer. Compile-time checks
bound the DSP, candidate clone regions and workspace; three canaries check
the synthetic call for writes at the chosen boundaries. The first canary
deliberately overwrites the final word of the third candidate clone region;
no clones are present in this diagnostic. This tests a layout, not concurrent
AINR operation, ownership, stock-buffer lifetime or physical safety.

The probe directly calls a stock mixing routine with 128 synthetic frames in
twelve candidate track buffers, distinct per-track gains, and disabled stock
filter sections. It captures both ends of each 64-frame callback block. It
masks interrupts during the call and deliberately overwrites stock state and
unowned QEMU scratch; it is destructive to the copied emulator session and
must never be installed on hardware. A guarded call-site veneer passes the
block offset to the experimental callback, which routes the synthetic tracks
with the standalone weighted-routing engine and runs the effects. A separate
ordinary callback checks that its original third argument is retained. The checker validates
the synthetic tracks, stock sum, independently reconstructed buses and stereo
output at four frames against the host reference. With the caller-owned
workspace, the current ARM build uses at least 408 bytes for the callback's
own stack frame before deeper calls, plus 768 bytes of unowned scratch.
The capture uses checked fallbacks for its sample reads; this does not remove
panic paths from the DSP called below it.
This does not qualify the normal file-mix workflow, callback lifetime, all stock
callback behavior, concurrency, target deadlines, or physical behavior.

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
inspector and turns the normal mixer control. The revision-specific call
sites are byte-guarded. The input hook queues edits;
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

For a shorter paired selector investigation, prepare one image with
`--callback --inspector --selector-load-trace --active-sends` and another with
`--callback --inspector --selector-load-trace --timing-control`. The option
enables the IRQ, enqueue and gesture traces and creates the same hold sequence
for each image. Inspect `start`, `end`, `work`, `inspector`, `inspector_later`,
`irq`, `enqueue`, `gesture` and `selected` in each result; the runner collecting
them is not an assertion that the interaction passed. Both images perturb
timing and use unowned scratch. A separate modified source was used for the
historical 285-tap observations below. Add `--max-velvet-density` to either
selector-load image to initialize Velvet at 95 ms and 3,000 pps; the recipe
also records `tap_count`. The option is a QEMU-only stress case and does not
change the installable firmware.

For a renewal workload, add `--active-sends --velvet-fade-stress short` or
`--active-sends --velvet-fade-stress long` along with `--max-velvet-density`.
Both use a 100 ms refresh; the fades are 1 ms and 90 ms respectively. The
diagnostic increments a scratch counter when a callback starts during a fade
and records `fade_start` and `fade_end` beside the callback counters. These
snapshots are separate reads, and a callback can cross a fade boundary. The
counter and tracing perturb timing; this is not a target deadline test.

For a paired Sub-Mix observation, copy the same Hall-selected 1.10 state to
two disposable directories and use `l6nor set` to set
`mixer.current.efx_return_submix=1` in the second. Verify with `l6nor diff`
that this is the only decoded settings difference and that Mix Minus is off
in both. Prepare each with `--callback`, `--inspector`, `--active-sends` and
`--submix-return`, then run each generated recipe with `l6dbg` and the same
hash-pinned QEMU. Compare their `diagnostic.json` files:

```sh
python3 tools/l6dsp-tests/probe/submix_return.py OFF_RESULT ON_RESULT
```

The check requires the observed Room callback, progressing counts, finite
nonzero sampled DSP output, the two setting flags, and the observed SAI slot
relationship. Verify the initial state, recipe, QEMU and executable hashes
separately: the comparison does not attest them. It does not qualify physical
routing, actual mixer inputs, USB Mix Minus, control responsiveness or target
timing, and the generated images must never be installed on hardware.

`--irq-trace` additionally installs a guarded UART receive interrupt probe in
the disposable guest and records matching key edges at `0x83e00900`. It uses
an unowned low-memory veneer, changes interrupt execution, and is QEMU-only;
it is not a safe target hook.
`--irq-timing` uses that same experimental hook and a bounded single-producer
queue to time selector gestures from the interrupt. An empty queue, a polarity
mismatch or an overflow cancels the gesture, but a whole lost press/release
pair is undetectable: later events can consume stale timestamps and produce the
wrong gesture. The queue counters are captured at
`0x83e00b00`. A verified existing hook retains its decoder, queued edges and
bounded trace across effect selections; overflow recovery remains unresolved.
Neither this experiment nor its scratch memory is target-safe.

`--ring-timing` is a separate QEMU-only experiment. It tags selector events
with a tick and nonzero sequence associated with the observed receive-ring
slot, and cancels a pending gesture on a sequence gap or detected fault. It
guards four observed byte-read calls and latches a fault if the ring identity,
capacity or available space violates its assumptions, or if a pending slot
has been skipped or is about to be overwritten. It also disables timing if
the observed UART DATA word has an empty, framing, parity or noise indication
in bits 12–15, as defined in the [NXP RT1042 header](https://github.com/nxp-mcuxpresso/legacy-mcux-sdk/blob/8a289764d763ad06e0c3a05c885644ed98b970af/devices/MIMXRT1042/MIMXRT1042.h).
The L6max processor is not identified, and the emulator does not model these
receive errors. State begins at
`0x83e02800`; this is unowned scratch memory. The modified event words,
interrupt masking, ring ownership and timing are unqualified on hardware.
It does not inspect the UART overrun status, detect all bytes lost before the
interrupt, or establish that the four sites cover every possible consumer.
Consecutive press edges discard the
ambiguous gesture through its next release, even when their sequences are
consecutive; a later clean press can be recognized. Overflow recovery requires
a fresh run. If both a release and the following press disappear before
sequence assignment, a later release is indistinguishable here from a long
hold; this guard does not make the timing path safe on hardware.
The first ten state words are magic, fault, ring pointer, record head/tail,
last-tag valid/tick/edge/sequence, and the last assigned sequence. Faults 1–3
cover an invalid or changed ring, 4–5 a full ring or record queue, 6 an
unexpected stock event, 7 a skipped record, 8 a pending-slot overwrite, and
9 an invalid or errored UART data word.

Use `--ring-timing --ring-drop pair` or `--ring-timing --ring-drop release`
with `--callback --inspector` to inject one missing selector pair or release
into a disposable guest. The generated short scenario records `loss_injected`,
`loss_ui`, `loss_selected` and `ring_timing`; a completed scenario alone does
not assert the expected values. The combined popup builder also accepts
`--ring-timing`, but not the forced-loss options.

For the prepared fixture identified in the investigation, the expected
captured values are `26.0 ms` for Plateau and `84.0 %` for Velvet, followed
by `28.0 ms` and `86.0 %` when revisiting them. The mixer send changes
before and after the inspector but stays fixed during the edits. Check the
captured observations; a completed recipe alone does not assert them.

While the inspector is open, a short EFX SEL press switches the parameter
bank without changing the selected stock effect; pressing EFX changes between
parameter and routing pages. Turning a strip encoder on the routing page cycles that strip
through off, A, B and A-to-B. This experiment does not draw an OLED popup.
Opening the inspector requests EFX mode through the observed stock mode
handler if a different panel mode is active. Selecting another observed mode
closes the inspector before the stock handler runs. These call sites and the
mode state are scoped to the guarded L6max 1.10 diagnostic.
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
generation. Closing or changing the
inspector page cancels pending popup work; the audio callback applies an old
edit but only publishes its notification if its generation still matches.
The comparison and publication use the popup manager's critical section.
An encoder captures its generation before testing whether the inspector is
open, and close paths clear the open flag before invalidating that generation.
This suppresses a stale notification in a forced close-after-check experiment.
This avoids relying on a separately published queue cutoff, but all transition
interleavings, cross-task access to the UI fields, generation wrap, and
real-time effects of popup synchronization remain unqualified.

The combined experiment keeps its 16 saved parameter words, 12 route words,
and 64-entry event queue with notification generations in the popup manager's
heap sidecar. If that sidecar is unavailable,
the inspector defers to normal button and encoder handlers and the selected
effect uses its default routes. Pointers are recorded at QEMU-only scratch
`0x83e07e00`, `0x83e07e04`, and `0x83e07e0c`; the route words at
`0x83e00100` and queue counters at `0x83e00200` are only observation mirrors
for this experiment. Gesture state and other diagnostic words still use
unowned scratch. The standalone inspector continues to use its scratch
parameter, route, and event mailboxes.
This does not qualify allocation lifetime, reset, exhaustion, concurrent
selection, callback ownership, or the remaining scratch addresses on hardware.
The bounded event queue drops a new edit when full; it does not recover that
edit. The combined experiment attempts to display `EFX INPUT / BUSY - RETRY`
when this happens. It invalidates older edit notifications, but still applies
those accepted edits; a later accepted turn can replace the warning. Other
notifications or inspector transitions can also replace it, so this is not a
guaranteed delivery or recovery mechanism. Host tests in `l6dsp-tests` exercise FIFO ordering, counter wrap, full
capacity, standalone storage, and a two-thread producer/consumer exchange.
They assume one producer, one consumer and stable storage; they do not prove
those assumptions for the firmware. The consumer stops applying events if
the head and tail imply more than 64 pending entries. It cannot detect every
form of corruption and does not automatically repair invalid indices.

```sh
python3 tools/l6dsp-tests/probe/prepare_ui.py UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-ui-probe
cargo run --offline --locked -p l6dbg -- /tmp/l6-ui-probe/recipe.toml \
  --firmware-dir /tmp/l6-ui-probe/firmware --state-dir /tmp/l6-ui-probe/state \
  --logs /tmp/l6-ui-probe/logs --qemu QEMU_PATH
```

The combined builder also accepts `--irq-timing`, `--active-sends`, and
`--enqueue-trace` for the corresponding QEMU-only experiments described
above. These flags do not enable an installable firmware patch.

To exercise the combined inspector's rejection path in QEMU, prepare with
`--force-inspector-overflow`, run its generated recipe as above, and check:

```sh
python3 tools/l6dsp-tests/probe/mixer_input_capture.py --ui-overflow \
  --probe-elf /tmp/l6-ui-probe/probe.elf /tmp/l6-ui-probe/result.json
```

Redirect `l6dbg`'s JSON output to that result file. This option injects a
rejection and an older notification; it does not fill the actual queue. The
checker observes the warning, a subsequent accepted edit, queue drain and
inspector close. It does not establish target behavior or hardware safety.
One run observed one injected rejection and 11 accepted, drained events.
The stock MAIN SHA-256 was `980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
the diagnostic MAIN was `110da65a446fca1db2250704930188ca635fea10d0af105b86eaa848ec703bf1`,
and QEMU was `4902e96e8375adb69b1ebe0e5572fe1ba3fb6a0e413c8595605128a297dfd910`
(source unattested). The `l6dbg` binary was
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`.
The initial NOR was
`26e79ca08d1bff89489d34055cc661e3431354260260fd19f8811dcca7e24924`,
the recipe was `3fcf09e80a19c606281d6b12b21582ee8d1e8358bf9734cb61ebf2d31458a24d`,
and the report was `013b9f32243d0fd275d64c42ab4b6b843b1fcc1896c3c724c708127da836dfe8`.

For the modeled-input routing check, use a QEMU binary built with the
`sai-test-tone.patch` described in the mixer-input capture section, then run:

```sh
python3 tools/l6dsp-tests/probe/prepare_ui.py UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-ui-route --mixer-route-capture
target/debug/l6dbg /tmp/l6-ui-route/recipe.toml \
  --firmware-dir /tmp/l6-ui-route/firmware --state-dir /tmp/l6-ui-route/state \
  --logs /tmp/l6-ui-route/logs --qemu QEMU_WITH_TEST_TONE > /tmp/l6-ui-route/result.json
python3 tools/l6dsp-tests/probe/mixer_input_capture.py --ui-route --probe-elf /tmp/l6-ui-route/probe.elf /tmp/l6-ui-route/result.json
```

The probe opens the Room inspector, changes the first route from B to A→B
and back, and checks captured sends at each setting. The six snapshots per
setting allow for the gated stimulus; they are not atomic. The checker requires
at least one clean nonzero snapshot on the expected bus, not a continuous
audio or timing guarantee. The fixture uses unowned scratch and does not
establish physical input mapping, GUI parity, sound quality or hardware safety.
`--mixer-route-capture` excludes the synthetic-send and timing-trace options.
The checker reads `diagnostic_audio` from the supplied ELF, whose identity
must be matched to the diagnostic image separately; it needs `llvm-nm`.

The combined prototype shows the current page on opening or switching it.
The recipe captures Room and Spring popup frames and routing-page edits. Verify the captured widget
text and mixer behavior; a completed recipe alone is not an assertion. The
combined image uses emulator-only state, and does not qualify notification
contention, queue overflow, effect switching, timing or physical hardware.
Do not install it.

## Stock effect clear trace

`clear_path_trace.c` is a QEMU instruction plugin scoped to the observed
L6max MAIN 1.10 startup and effect-clear sites. It only logs instruction
execution; it does not establish all indirect callers, memory ownership, DMA
behavior or timing. Use a plugin-enabled QEMU and disposable stock state. For
example, with absolute paths to the QEMU executable and its source tree:

```sh
export L6_QEMU=/absolute/path/to/qemu-system-arm
export L6_QEMU_SOURCE=/absolute/path/to/qemu-source
L6_TRACE_DIR=$(mktemp -d /tmp/l6-clear-path.XXXXXX)
export L6_PLUGIN="$L6_TRACE_DIR/trace.so"
cc -shared -fPIC -O2 $(pkg-config --cflags glib-2.0) \
  -I "$L6_QEMU_SOURCE/include/plugins" -o "$L6_PLUGIN" \
  tools/l6dsp-tests/probe/clear_path_trace.c
cat > "$L6_TRACE_DIR/qemu-wrapper" <<'EOF'
#!/bin/sh
exec "$L6_QEMU" -plugin "$L6_PLUGIN" "$@"
EOF
chmod +x "$L6_TRACE_DIR/qemu-wrapper"
```

Run `l6dbg` with `ainr-isolation.toml`, unpacked stock firmware, a disposable
copy of stock state, and `--qemu "$L6_TRACE_DIR/qemu-wrapper"`. Inspect
`CLEAR_PATH` lines in the main engine's stderr and the scenario observations;
recipe completion alone does not validate the trace. Record the actual QEMU,
plugin, wrapper, fixture and result hashes. One startup and effect-selection
observation, with exact identities and its limits, is in the
[research findings](https://github.com/derekstavis/zoom-l6max-research/blob/290eb3d/docs/firmware/l6max-effects-ainr.md#effect-storage-and-adjacent-conversion-buffers).

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

Add `--channel0-prefix` to zero the copied state after offset `0x19514`
before those comparisons. The stock reference retains the full state. This
tests one candidate boundary for channel 0; it does not prove the omitted
region is unused for other inputs, learning states, or execution paths.

`--replay-tails REFERENCE_TAIL CANDIDATE_TAIL` replays two local 56,264-byte
captures for offsets
`0xb94c..0x19514`. The stock reference uses the supplied reference tail with
the live guest's prefix and suffix; each clone uses the candidate tail with a
zeroed prefix and suffix. The reference tail must have the observed processor
state word at offset `0xba4c` set to 1. The two input hashes are recorded in
`identities.json`. This is a hybrid reference, not a captured whole-processor
state or a test of capture synchronization. Keep captured firmware state in
local temporary storage and never commit it.

`--stereo` instead mirrors both natural stock calls, using 159,744 bytes of
copied state and 96 blocks per channel. It compares each clone output and
checks both working areas at four checkpoints. This mode needs modeled input
to reach the stock AINR call; `--minimum-input N` requires a finite input
sample greater than N before the comparison starts. A custom `--recipe PATH`
can set mixer controls and extend the wait after `learned_ready` for debugger
work. For the observed gated-input run, that recipe inserted EFX (target 20),
channel-1 encoder +20, LEVEL (target 35), channel-1 encoder +20, and a
3-second guest wait immediately before `before_learn_control`, and extended
the wait after `learned_ready` to 30 guest seconds. The QEMU build injected a
500 Hz square wave gated at 0.5-second intervals on each modeled RX word.
The recipe, exact QEMU binary, input fixture, and state are part of the
result; a normal QEMU with silent inputs will not satisfy a positive threshold.
The two channels in one observed run used identical modeled input, so matching
results do not establish behavior with independent stereo signals. This mode
does not implement or validate three independent pre-AINR processors.
Add `--invert-second` to replace the second stock input block with its
inverted half-amplitude version before each comparison. The clone receives the
same modified block. The run requires the two captured input streams and the
two output streams to differ. This is a debugger-forced correlated signal,
not an independent physical input or a test of the unmodified mixer path.
Add `--solo-prefix` to compare a second, channel-zero-only clone alongside the
full stereo clone. It copies `0x19514` bytes and zeros the remainder of its
`0x27000`-byte scratch region. At each checkpoint the probe also checks that
the solo call did not change stock or stereo-clone state. Use `--blocks N`
to vary the number of blocks per channel from 1 to 1,000; longer runs require
a correspondingly longer post-snapshot recipe wait. A match is bounded to
the captured learning state and input; it does not reserve memory or exclude
other state dependencies.
One `--stereo --invert-second --solo-prefix --blocks 1000` run matched both
stock channels and the solo prefix, with checkpoints at 0, 333, 666 and 999.
Its MAIN, NOR and QEMU matched the identities below. The recipe, probe and
result hashes were
`3f7d112139892e4d53311c74afa24b46ff6d0168edad1be06cc05a6ca11f2b66`,
`8c284fee3cdc9049a031c6c8bc3cd6d46c5c1c489d6ff4cbf3c89f17914972bd` and
`563859f7990198151b81c791b1b24440f3786491f4ff8c9938d4d7a097775961`.
A temporary negative control zeroed the solo clone's learned-state word at
offset `0xba4c`; the comparison rejected its output at block 0. Its wrapper
and captured error SHA-256 values were
`6319baa4c92126ea95fab11100fb5754df5059a4aa9b8e23d0aedaa027566cf8` and
`f2df73b5be71d0613f659d8eeb6412eba01721e1caa89798ae4d030bae093aea`.
One such run matched 96 blocks per channel, with input peaks 45,698,716 and
22,849,358. With the same MAIN, NOR, QEMU and recipe as the following run,
its probe and result hashes were
`cfbaa99fb0d4c11412a0a6c8eed55e25af5ba9cdd7506bdc152b12080b264ee4` and
`5f54a8b453f82cdd6cfe29ac47a163738fbe3b7e83e474025758db90becd879a`.
An earlier unmodified-input run matched 96 blocks per channel with a peak
input of 45,838,220.
Its stock MAIN, initial NOR, and QEMU SHA-256 values were
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a` and
`eade074b5d7a6fe9d74bbbe1ad36a34f7356cd200cbfc81e772f2a21892012e8`.
Its recipe, probe, and result hashes were
`2761c79ab8817adc22459cb54922791e8c062deade5cb848533a4613d0d49c9c`,
`821ef18d6cbf817ddbf7d1174c414ad6b965c2e9604b1b8a06e6776e98ad375f` and
`338e7698d44f0df837b09da92ebde202e11209d9ffa5bb72f0b64cd0fc1d91a3`.

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
the patch builder. The diagnostic converts its normalized sends to the stock
mixer's observed float scale before each AINR call and converts the output back.
This boundary has only been compared against stock for a short captured signal;
it does not establish full-scale limits, denoising quality, or safe behavior for
all learned states. Results from older revisions passed normalized sends
directly to the native-scale AINR call and do not validate this boundary.

One scaled diagnostic run reported the learned word as 1 and an initialized
callback; its send counter increased from 2,718 to 5,337, the sampled sends
and output were finite. It does not establish audible noise reduction. The
stock MAIN and initial NOR were
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461` and
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`.
Diagnostic MAIN, ELF, recipe and result SHA-256 values were
`267d91bb290e9d600a1f1407558380b2fa899d4cfe7bb57b6096aaf1218d2944`,
`cce3adb0e85d6987e80394be10b3102381de001f8bea41a88d7d6fcdfa4512a6`,
`c5e5052aa36d9cb64bedf7bf47b04d52c53220757a4f6b2794689be9e469b11a` and
`4aea3add16a1935872794866bd3bfc3844cc37c308423fda92531177a5a36df3`.
The QEMU executable was
`eade074b5d7a6fe9d74bbbe1ad36a34f7356cd200cbfc81e772f2a21892012e8`
with unattested source; `l6dbg` was
`ebb31e0e78b8ac31e351c7450ed3636080c85c9df212fc2c6ad3f7b561814241`.

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
A failed snapshot allocation is retried on a later custom-effect selection;
a successful retry returns the capture status to waiting. It does not
recover a previously missed learning capture automatically.
Both diagnostic heap buffers are checked against the observed L6max 1.10
heap payload bounds. An unexpected non-null allocator result is rejected
without calling free; if it was a real allocation, that leaks it for the
session. Range checks alone do not establish allocation ownership.
Add `--channel0-prefix` with `--auto-handoff` to allocate and copy only the
first 103,700 snapshot bytes, then zero each copied state's remaining bytes
at selection. The default remains the full 128 KiB. The prefix is experimental
and still copies a large amount on the audio callback; neither configuration
is safe for hardware installation.
With that option, `--hybrid-handoff` additionally copies the observed outer
pages in 4 KiB chunks, then copies the `0xb000..0x16000` span together. It
publishes only after all pieces are copied, and resets incomplete progress on
the observed selection or learning-reset paths. The recipe records progress
at `0x83e03130` after learning and selection. A versioned atomic claim rejects
publication by a cancelled capture, including after rearming; progress is only
diagnostic telemetry. In this mode the completed claim, not the later status
word, authorizes installation. The status word can also be stale across a
reset, so its captured value does not block a rearmed hybrid claim; invalid
status values still fail closed. Cancellation holds the buffer claim until an
in-flight writer retires; an early rearm request waits for that retirement.
Installation also claims the completed snapshot while copying it. Its ready
token is tied to the completed capture generation and is published only if
the installation has not been cancelled or disabled. Entering AINR invalidates
the previous visit's completed snapshot and requires a new capture; if a copy
is still in flight, the new capture waits for it to retire. This does not establish
quiescence for the destination effect buffer or other stock users. This is an
unqualified experiment: the inferred span may
miss mutable state, the last callback still copies at least 44 KiB, and the
selector has no proven audio-task quiescence or deadline guarantee.
Alternatively, `--boundary-handoff` (with `--channel0-prefix`, excluding
`--hybrid-handoff`) tests an earlier, observed panel selection call. It asks
the AINR hook to bypass indices 0 and 1, waits up to 20 firmware ticks for an
index-0 acknowledgment, and copies the candidate channel-0 tail at
`0xb94c..0x19514` on the selector task. The heap snapshot holds only those
56,264 bytes; installation zeroes each clone's prefix and suffix. Status 3
means a candidate is waiting; `boundary_handoff` records the pause word, wait
count, outcome and two bypass counters. `--boundary-noise` injects a
deterministic nonzero signal into the stock AINR input for this experiment.
After copying, the diagnostic closes the acknowledged pause atomically before
publishing the snapshot claim; an unexpected index that invalidates that pause
before closure prevents publication. This does not synchronize unobserved users.
An earlier full-prefix run observed stock writes inside the excluded range
while the pause was acknowledged and the copy was in progress. The exclusion
and packed tail are candidates based on limited comparisons, not a proven
AINR footprint. A successful handoff does not establish a coherent snapshot; see the
[boundary investigation](../../../docs/firmware/l6max-effect-callback.md#selector-side-boundary-handoff-diagnostic).
Add `--outer-audio-pause` to test parking the observed AudioProcess task before
its whole block instead of bypassing just the two AINR calls. The guarded
call-site hook acknowledges a specific request generation, waits in
one-tick delays, and invalidates the request if still paused after 20 waits.
`outer_audio_pause` records the number of acknowledgments, the last wait count,
and timeouts. This deliberately delays audio and does not cover other state
users, DMA, real-time deadlines, or audible continuity. The ordinary boundary
bypass counters do not apply in this mode.
Add `--audio-owner-refresh` with `--outer-audio-pause` to copy both the initial
snapshot and a later refresh in the audio task at block boundaries. The later
copy uses separate, unowned QEMU scratch; a generation-held reader protects it
until the selector finishes copying. `owner_refresh` records the pause and
capture words, audio claims, rejected publications, selector wait, and outcome
(1 success, 2 timeout or invalidation). The `outer_audio_pause` counters in
this mode are initial claims, completed captures, and failed captures. This is
an experimental diagnostic, not a hardware memory layout or timing guarantee.
Add `--heap-refresh` to allocate and retain a second 56,264-byte firmware-heap
buffer for refresh scratch. `heap_refresh` records the published pointer and
allocation-failure count. An unavailable or invalid allocation leaves the
capture unready; the default synthetic sends are then suppressed, which is not
a production fallback. The allocation persists for the diagnostic session;
heap headroom and its lifetime under other device workloads are unqualified.
Add `--release-heap-refresh` to free that scratch after a successful refresh;
`heap_refresh_frees` counts completed frees. The selector retains its primary
install claim during the free, and a timeout retains scratch for a later
attempt because the audio writer may still hold it. This is a QEMU-only
lifetime experiment, not a production allocation or fallback policy.
Add `--combined-admission-refresh` together with `--callback --inspector
--active-sends --pre-ainr --effect-buffer --auto-handoff --channel0-prefix
--boundary-handoff --outer-audio-pause --audio-owner-refresh --heap-refresh
--release-heap-refresh --file-mix-handoff atomic --selector-admission
--ainr-admission --ainr-pair` to exercise the shared selector and paired AINR
admission hooks with the heap-backed refresh. `combined_gate_*` and
`combined_ainr_*` record gate and dispatcher counts; `combined_copy_gate`
records the gate value and count at refresh, then the gate value and count at
the initial snapshot copy. The
default recipe does not trigger file mixing, and a successful run is not an
assertion of safe behavior. The initial snapshot precedes selector closure;
the rejection path, other callers, DMA and real-time timing remain unqualified.
This combined image uses unowned control words and flash. **Never install it.**
Add `--selector-feedback --guard-initial-capture` to defer AINR teardown and
request the initial capture after the observed panel selector closes and
drains the gate. This changes stock teardown and MIDI ordering and can bypass
AINR while the gate is closed. Other selection callers can still reach the
earlier unguarded hook; timeout recovery, audible continuity and all writers
remain unqualified. This variant is also QEMU-only and must never be installed.
The pointer ownership, call ordering, 64-frame bypass and return ABI,
other selector paths, timeout recovery, audio continuity, and target timing
are not established. The code uses unowned scratch and flash. **Never install
or run this diagnostic image on hardware.**
Add `--passthrough-unready` to keep synthetic sends active without AINR while
the snapshot is unavailable, including after a diagnostic allocation failure.
The resulting extra work can prevent the selection sequence from reaching
AINR in QEMU; inspect the selected callback and learned flag. This tests
passthrough in the probe; it does not implement a firmware fallback.

These prototypes retain a heap allocation and use unowned control words and
an experimental settling delay. The full and prefix modes copy on the audio
task; the boundary mode copies 56,264 bytes on the selector. A separate
attempt to capture only 512 bytes and reconstruct the rest after selection
produced a different state under injected signal, despite reporting success;
it was discarded. None establishes snapshot coherence, stock coexistence,
deadline safety, reset behavior or a usable learning workflow. Never install
these images on hardware.

## Mixer-input capture

`--callback --mixer-input-capture` prepares a separate, QEMU-only signal-path
probe. It selects EFX and LEVEL for strip 1, selects each custom callback in
turn, and captures one block of source slots, gains, reconstructed sends, and
stereo return. For this comparison it resets the diagnostic route mailbox to
B for Room and A for Spring on each selection. The marker is published after
at least 100 callbacks and a send sample exceeds `1e-7`; a zero marker means
no eligible block was captured. Scratch is unowned and the extra memory writes
perturb execution. Do not install the result.

The normal QEMU receive path supplies silence. To test nonzero input, supply a
separately instrumented QEMU binary with a documented modeled input signal;
this option does not generate audio. Record that executable and input's exact
identity. Check the captured active callback and route, compare all 64 send
samples to the captured source and gain data, and inspect both return channels
for finite, nonzero samples bounded by unity. `python3 tools/l6dsp-tests/probe/mixer_input_capture.py
/path/to/diagnostic.json` performs those checks for the probe's fixed test
routes. In addition to the single reconstructed block, it aggregates signal
peaks across the selection: it requires at least 100 blocks with active input,
finite bounded output and a peak in each return channel at least 1% of the
observed input peak. This fixture-specific floor catches silent or nearly
silent returns; it is not a target audio-level or fidelity specification.
The added reads perturb execution. Active gain ramps require their own capture and cannot be reconstructed
from these snapshots. A successful recipe collects
observations only; it does not establish physical input mapping, control
responsiveness, state ownership, or hardware timing.

For a repeatable L6max stimulus, `sai-test-tone.patch` applies to a disposable
copy of the configured QEMU source tree's `hw/arm/l6max.c`. It replaces the
silent receive data with a 500 Hz square wave, gated at 0.5-second intervals,
on every modeled receive word. For example, after building the normal local
`emulator/qemu/build-source` tree, copy it to a new temporary directory, apply
this patch with `patch -p1` there, and rebuild its `qemu-system-arm` with
Ninja. Use that binary only for diagnostic runs and hash it alongside the
fixture and results. It is an artificial test signal; it is not a model of
any particular analog jack, and the instrumented binary is not the normal
emulator build.
