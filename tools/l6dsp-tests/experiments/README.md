# Host-only DSP experiments

`python3 tools/l6dsp-tests/experiments/plateau_rate.py` requires NumPy and
models a 48 kHz → 32 kHz → 48 kHz stereo identity path. It uses the supplied
Multistomp converter's 64-tap, 14 kHz Kaiser-window design, with two down and
three up phases for this ratio. Checks cover frame count, streaming boundaries,
DC gain, channel isolation, selected audio-band and alias frequencies, both
downsampling stopbands, impulse alignment and 12 kHz phase-cycle residual.
This is a floating-point host model with no reverb or firmware integration;
it does not establish sound equivalence, target execution time, control-event
ordering, effect-return alignment or hardware safety. Its impulse peak is
79 host samples after the input in each of the three input phases.

The matching experimental Rust converter is checked with
`RUSTFLAGS='--cfg experimental_plateau_32k' cargo test --offline --locked -p l6dsp-tests plateau_rate::tests`.
The committed filter tables can be verified with
`PYTHONDONTWRITEBYTECODE=1 python3 tools/l6dsp-tests/experiments/plateau_rate_coeffs.py --check`.
Under `experimental_plateau_32k` the standalone routed reverb uses this
converter. Focused `plateau_32k` tests compare it to separately assembled
converter/core processing, cover the control endpoints, reset, and discarded
freeze input. It is not connected to firmware; return alignment and per-frame
CPU cost have not been qualified on target.

The `experimental_plateau_32k` compile configuration selects a standalone
32 kHz core with size 0.01–4 and 0–500 ms predelay. Run its focused test with
`RUSTFLAGS='--cfg experimental_plateau_32k' cargo test --offline --locked -p l6dsp-tests plateau::tests`.
The normal 48 kHz test suite cannot run against it unchanged. Before adding
the routed converter, its ARM processor type was 511,072 bytes; the size-query object SHA-256 was
`0899ea4b81e4b087f9404fc4079c8a37f44fcd89608df90292b225619bf4c637`
using `rustc 1.99.0-nightly (1ed2df61a 2026-08-04)`. An earlier 510,944-byte
estimate omitted maximum modulation headroom from two allpasses; the focused
test exposed that error and the current buffers include it. The host model's
input/output histories add 1,024 bytes; coefficients and other overhead are
separate. This does not qualify allocation or target timing.

### Modeled audio at the full-range endpoints

`plateau_32k_input.patch` is a **QEMU-only** adaptation of the existing
`--callback --mixer-input-capture` probe. Apply it only in a disposable tree
containing copies of `tools/l6dsp-tests/probe` and `patches/dual-efx/src`;
it enables `experimental_plateau_32k`, initializes Plateau with size 4.0 and
500 ms predelay, records those parameters and the processor size in unowned
scratch, and delays the one-shot capture to callback 2,500. Build the probe
with the repository's pinned Rust toolchain. Never install its image.

For one run, prepared channel files from the `l6max-audio-input` scenario
were supplied through `L6_AUDIO_INPUT_DIR`; the generated diagnostic recipe
started audio after its initial wait, waited 12 seconds at each custom effect,
and read 12 bytes at `0x83e03f20` after each selection. Both reads reported
size 4.0, predelay 500.0 ms, and processor size 512,112 bytes. The normal
capture checker reconstructed the routed sends and found finite, nonzero
stereo returns for Plateau and Velvet. It counted 6,192 and 5,858 active
blocks, respectively. This is one fixed-route, instrumented modeled-input
run; it does not establish audio parity, physical timing, memory ownership,
UI control, or safe installation.

The stock MAIN, initial NOR, panel state, input files, QEMU, `l6dbg`, and
checker match the modeled-input run in the [probe README](../probe/README.md#mixer-input-capture).
The diagnostic MAIN, ELF, recipe and observation hashes were
`4f167ee7ff9d9f5b11d1df209c50b14d16f27f89ecb08755205f4b2202ea6fed`,
`3f154cf90f3a1ec970153ea00020d7905709aed2b82896c68ee18f32a7c900f5`,
`5f0932d77af9ee156c7bf055c50547674dd2a357d621e1b70105acaf90239753`, and
`28179c23ebc5e592c8414693ea21453dfc454f3b51aba67cea969756b858548f`.
The initial diagnostic NOR reconstructed from the stock NOR and diagnostic
MAIN hashed to `3857f483f7d7339b54ca21081a2f0cc8c414d1c7de73427ea7013f4e8c182582`.
The modified preparer, callback and capture source hashes were
`accef9c75f4c49686905669617179f21f2cb4ef667fb4d1aa6c7ffc17d8a9106`,
`34c09dbbc9b6bdffe5798df96f0cf03f08c187daf9d280fc7341da9ba7eb622d`, and
`a720d406a23dba63694a2fbfc93ab249132e65f6bd0ebce6e1b91d0555340709`.

### Combined three-bus stress

`combined_input_stress.patch` is a QEMU-only variant applied after
`plateau_32k_input.patch` to the same disposable capture-probe tree. It
configures full-range Plateau and a 400-tap Velvet with a 100 ms
renewal and 90 ms transition, and routes input slots 1, 2 and 3 to A, B and
A-to-B, respectively. It captures at callback 1,000. Apply it only to a
disposable source copy; `combined_input_bypass.patch` applies on top to make
the matched control return the summed sends instead of running effects.
Neither diagnostic image may be installed.

One pair used the same modeled channel files as the previous capture. Its
recipes started playback, raised EFX sends and levels on channels 1–3 by
eight encoder steps, selected Room and then Spring, and waited 20 guest
seconds after each selection. They recorded configuration at `0x83e03f20`
and the existing mixer captures. In the loaded run, all three first-selection
sends matched their individual source/gain reconstructions and both returns
were finite and nonzero. The guest reported size 4.0, predelay 500 ms,
processor size 512,112 bytes and 400 Velvet taps. It counted 4,303 callbacks
at the first observation. At the second observation the selected descriptor
was still Room, so the intended Spring selection and complete check failed.
In the DSP-bypassed control, both selections and all three reconstructed
sends passed; the first observation counted 9,947 callbacks. The nominal
20-second count is 15,000. The two inputs are deterministic modeled tones,
the capture itself adds work, and this is not a physical timing measurement.
The run does not establish the cause of the missed selection, hardware
responsiveness, audio parity, memory ownership or safe installation.

`mixer_input_capture.py --combined-stress REPORT` checks the parameters,
selections, individual sends, bounded stereo output and aggregate levels for
this variant. The loaded observation is expected to fail its second
selection check; the bypass observation passed. The original temporary
checker was `c9b4b677dc459eb0865bcdd6ceb5f058464735feed7e7c49edbe246caced537b`.
The loaded/control diagnostic MAIN hashes were
`452b330109a78867b29f2df04ced30613521b1eb0623e99468c528b1685ecba0` /
`61e2a241cefef0b0c2438f43ca5c34fd07419b3bcfe252cc7637c8cecfb561c5`,
ELFs `0e147599a43509dcf9d4e6adc189442fa1bb65ceabc0c5ad473369e7d4e8bd9e` /
`a65d62f7be635da0b386b804fe22d4d6aeb3f6abe5a3e42e77d865566e545351`,
recipes `3a2a1022076123637ca4276a026f8da5dbf6f66d6f91787894f8dd789eeff6cd` /
`5745ef76677239ce2d14bc23f40bf7932823fb203c389025cc1fe8ce6170a887`,
and observations `a2916ac3f07f9664fca5d05f1ce0ba121646adf30f3f218b21fbbc7339491e19` /
`431dee7854aed28dd0030c67cc79f482b5658236865ef4163afbd63d3602d823`.
Their initial diagnostic NORs, reconstructed from the common stock NOR and
each MAIN, hashed to
`f17354889c55b206589a4c995e65180281c0b4206d74b41efa8a8970c316eb68` /
`99ef280ccc69e46e933fadfa4a8fd9a0037f7a1d8764ce875869f777137e6688`.
Stock and panel identities, channel files, QEMU and `l6dbg` matched the
modeled-input capture cited above. QEMU source provenance is unattested.

After the audio-queue shortcut was removed, a matched set on the rebuilt QEMU
isolated the DSP paths. Apply either `combined_input_velvetonly.patch` or
`combined_input_plateauonly.patch` after the stress patch in a disposable
tree. The Velvet-only variant skips Plateau; the Plateau-only variant sends
the B and cascade inputs directly to Plateau without Velvet. These are
diagnostic substitutions, not the intended audio routing. Both retain the
same 400-tap and full-range Plateau initialization, inputs, control sequence,
capture instrumentation and 20-second observation as the combined run.

| QEMU-only variant | First callback count | Second selection |
| --- | ---: | --- |
| Both effects | 3,359 | Still Room |
| Velvet only | 4,351 | Still Room |
| Plateau only | 10,024 | Spring selected |
| Both bypassed | 9,889 | Spring selected |

The checker accepted the Plateau-only and bypass observations and rejected
both cases that stayed on Room. It also verified the three source/gain sends
and nonzero bounded stereo output in the accepted cases. The nominal count
is 15,000 per 20 seconds. These substitutions and instrumented modeled inputs
make Velvet the stronger optimization lead in this workload; the figures do
not measure hardware execution time or establish why the selection failed.

The QEMU and `l6dbg` executable hashes were
`13040494f8a58c681b18bef05886196ac37be526772bcb93c301875ed93d34da` and
`0f113207ad23054dc91f1e8900f294c60b68fd985892213515c2e7aaf4ee87ba`.
QEMU source provenance is unattested. Stock MAIN, original NOR, panel state
and prepared channel files match the preceding modeled-input runs.
In table order, the MAIN hashes were
`452b330109a78867b29f2df04ced30613521b1eb0623e99468c528b1685ecba0`,
`a6925b9a43c8318882e70c372c2f82ac1aaec9924deaf030c590539d7dc85b63`,
`5f793b0378bd9734fb3800eb9903dbe2b05dfef1e644b3cb6d40f91699c754e5`, and
`61e2a241cefef0b0c2438f43ca5c34fd07419b3bcfe252cc7637c8cecfb561c5`;
the corresponding ELF hashes were
`0e147599a43509dcf9d4e6adc189442fa1bb65ceabc0c5ad473369e7d4e8bd9e`,
`d90b4c635cca36193956a1fd29d99bee5b8c42184efed2d235a25a1dbe30049e`,
`715cc4b0ff10e04ecce6ef20c1ba271e950cd8a2ebedc958817c5da48c82760c`, and
`a65d62f7be635da0b386b804fe22d4d6aeb3f6abe5a3e42e77d865566e545351`.
The recipes hashed to
`3a2a1022076123637ca4276a026f8da5dbf6f66d6f91787894f8dd789eeff6cd`,
`02a54547193f94bd2b66205ed033c16ef30626f7a8341758722017c471411784`,
`08eb815b951c978a20f1595d670b6067e3131af52b875cfe40d3fd86241a6d8b`, and
`5745ef76677239ce2d14bc23f40bf7932823fb203c389025cc1fe8ce6170a887`;
the observations hashed to
`6e709d36cd96aa45cd8497d9f2691229d477ffcdec6063a41cfaa99907e36252`,
`8b5db0a385abf49790a4c76115945193ae5003c190ab3217f5cb85500990312c`,
`aca2d920fc9a9e83f62751ac306903953ce03663df78deb616e1adf1efa20ad7`, and
`05f49dfef8d6432d558a97f0f8408ae2886514e58476a02f5a9fbc0a3cd96c4e`.
Initial diagnostic NOR hashes in the same order were
`f17354889c55b206589a4c995e65180281c0b4206d74b41efa8a8970c316eb68`,
`5e0b6212e13bad8fd0770de525504fd63ba4d892ad2395362b7ead6a90de25a3`,
`bcdf22c4a217957aa371ac7591c06d7cb37fa8f2e061aebed8e44f1819d972d5`, and
`99ef280ccc69e46e933fadfa4a8fd9a0037f7a1d8764ce875869f777137e6688`.

An unmodulated, low-level 128,000-frame impulse comparison against the derived
Multistomp core at commit `7ea50ae6e30705989d0b8414ad5a83c97dbef0db` used
size/predelay pairs 0.01/0 ms, 1.0/0 ms and 4.0/500 ms. The two inputs to
the reference were halved to match this port's stereo average. With decay
0.85, full diffusion, wet output and approximately matched cutoff controls,
correlations were 0.9868, 0.9863 and 0.9877; difference norms relative to the
reference were 0.168, 0.174 and 0.165. The maximum case's first nonzero sample
occurred at frame 16,000. These are core-rate, finite comparisons with no
resampling, dynamic edits, freeze, or hardware; they show substantial
similarity and measurable differences, not sound parity. The Rust/reference
host libraries hashed to
`aa9fdce07c30f3bfe6bbdc93f5820414c71ff7fe9988de6247fc0dadc3f7774f` /
`2104d47b24675a463ecd48207251ba1417b86261a91de69050022fc606c88512`.

As a layout calculation, three previously observed 56,264-byte candidate
AINR tails could start at offsets `0xb94c`, `0x1b94c` and `0x2b94c` within
the stock 768,000-byte effect buffer, ending at `0x39514`. An eight-byte
aligned 511,072-byte processor beginning at `0x39518` would leave 22,152
bytes; a snapshot would need separate storage. This assumes the clone code
does not access the preceding regions, that its base addresses can be
relocated appropriately, and that the stock buffer and heap can be safely
owned across selection, audio and file mixing. None is established. The
calculation excludes converter state, executable code, workspaces, other
firmware use and allocation overhead; it is not a hardware memory plan.
A later diagnostic with the converter included observed a 512,112-byte
processor, which would leave 21,112 bytes in that same hypothetical placement
before the other exclusions. See the [safety status](../../../docs/dual-efx-safety-status.md)
for the run and its limits.

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
checks a separate tap-outer FIR against a direct sample reference. It exercises
400 taps up to the full 4,800-sample delay, two filter banks, two channels,
independent send histories, a per-frame crossfade, a non-aligned split and
ring wraparound. Each bank's output matches the reference bit for bit in this
test. It extends the ring to 4,864 samples so prewriting 64 frames cannot
overwrite history needed by the first frame; each tap splits its read into
at most two contiguous slices at the ring boundary. Two sample histories take
38,912 bytes. Extending the current processor's three 4,802-sample histories
by the same 62 samples would add 744 bytes, excluding block output buffers,
other state and integration changes. The experiment does not implement
in-block bank publication, live parameter delivery, bank generation, output
clipping, firmware integration, target timing or memory ownership. Compiling
it for `thumbv7em-none-eabihf` confirms only that it builds for ARM; the
generated bounds/panic paths have not been qualified for an audio callback.
The passing host test executable was
`27895815df24f4ec3a6f4b9f7617c2841f7f8b24515790e0a982ece2e5cf4c11`;
the ARM assembly was
`2280ad4d8d11b4fb0af93e67fb7172558e29c46901c50c214054def99c2cc7d6`.
Both used `rustc 1.99.0-nightly (1ed2df61a 2026-08-04)` and source
`81f0e3451701586709f2e4ecc423fdffed0971b80e893faf0c88c6acb1db953e`.
The approach is informed by the MIT-licensed
Multistomp Velvet port at commit `7ea50ae6e30705989d0b8414ad5a83c97dbef0db`;
that pedal's tests do not qualify the L6max implementation.

The QEMU-only `velvet_scheduler_qemu.patch` preserves a follow-up combining the block loop with the actual Velvet bank
scheduler. It split processing at fades and fell back to per-sample processing
while building banks or handling unsuitable state. In one 30,000-frame host
test, it matched the original 4,802-sample-ring sample path bit for bit through
several parameter edits, fades and wraps. A second test compared scheduler,
bank and history state against a scalar path using the expanded ring. These
are bounded host observations, not coverage of all control interleavings or
corrupt state. The host test executable and the original, expanded-ring,
block-scheduler and test-harness sources were respectively
`392eaea0f132925ca1d498d55da7dbb966e30b3324567114469f3558da94641a`,
`220a6aac4940983fce020a152f9905928aa63017cfaf20f41b282b4d43d84d47`,
`dee1b59b6c995368fb316ed5b81e6fc731e9e390a1f80faf974606660580b4ab`,
`a5965ad094478f6f32654cf38cd91986ee95e6e358feedc42db9c32a586e6f02`, and
`aed3efeb0ddac2d7102e20fccacad93face06d6d6550dd75cd17821ce565994e`.

In the combined three-bus QEMU workload, the first block-scheduler build
completed only one callback. ARM stack metadata reported 2,456 bytes for
the callback and 3,264 for the block routine, before deeper callees; the
previously observed audio-task stack was about 4 KiB. Stack exhaustion is a
plausible explanation, not a proven cause. Moving three work buffers to
unowned QEMU scratch reduced those local frames to 1,160 and 200 bytes,
respectively. That variant processed the three modeled sends and generated
bounded nonzero output, but still failed the second effect selection. With
`opt-level=z`, it reached 3,501 callbacks at the first observation versus
3,359 for the earlier scalar run. With `opt-level=3`, a matched scalar/block
pair reached 4,067/4,234; both missed the second selection. These are single
instrumented runs, not physical timing measurements or a demonstrated
solution. The prototype also reorders control writes and changes malformed
delay handling; its scratch, stack, concurrency and full state semantics
are not qualified. It was not integrated into the installable patch.

To inspect the follow-up, apply `plateau_32k_input.patch` and
`combined_input_stress.patch` to the disposable probe sources as described
above, then apply `velvet_scheduler_qemu.patch` with
`git apply --unidiff-zero`. The zero-context patch assumes those exact inputs;
check the resulting sources before building. It writes three buffers to
unowned addresses and is strictly for the emulator, never hardware. Its block
routine includes bounded host comparisons, but the callback reorders control
writes and the expanded ring changes malformed-delay behavior.

Replacing the inner indexed loop with slice iterators improved the same
`opt-level=3` workload: two first-observation runs completed 6,165 and 6,035
callbacks, versus 4,067 and 3,961 for the matched scalar build. All four
missed the second selection. Both block runs reconstructed all three modeled
sends and produced bounded nonzero stereo output at the first selection. The
nominal count is 15,000; these instrumented QEMU runs do not measure target
timing or establish an adequate deadline. The updated host suite passed 25
tests, including the bounded scheduler and cross-ring comparisons described
above. The saved QEMU patch reproduces the measured block source, not the
separate temporary host harness.

For the iterator build, MAIN and ELF were
`e18e80e37750a1b7ab7139c220171b13eb911c67712bb636242ba24cfccab910` and
`75562ad8187658e5359d83734f1a57caa67e2489fd7c35f2e60ad0740cdb24e7`;
the recipe was `0b89340c6eb10827169a286951bfcb68934d2a9e6650b473b26594f176afc851`.
Observations were
`cff04b5a5c8424b8f20939494ce823d7e306cc2db46a42df622c20bc96137cd3` and
`5c5dfa903d1e895c563120f249a9a866d87181a2ab3e8af27b168356594bf470`.
The repeated scalar observation was
`9b6f9ea74c1d354439d4f76af29ee72aee55c917589e65718b51d184be639c68`;
its executable and recipe match the scalar run identified below. QEMU and
the input fixtures match the preceding comparison, with unattested QEMU source.

The stock and panel identities, modeled channel files, QEMU
`13040494f8a58c681b18bef05886196ac37be526772bcb93c301875ed93d34da` and
`l6dbg` `0f113207ad23054dc91f1e8900f294c60b68fd985892213515c2e7aaf4ee87ba`
match the rebuilt-QEMU comparison above; QEMU source provenance is unattested.
For the first stack-resident attempt, MAIN, ELF, recipe and observation were
`aa89b17a41560d4ca969b8e0c343b30e0db98be95c82c642d72525de81f51f2f`,
`153949644aad79b15005162eda90626b364217f68b797c5a244b4b886f1bac22`,
`0ff8a23a19cc66d7adcdfed6d9340acaacb2c42c373d8d5993a3deed65cbc847`, and
`b8ef6ad84dbc33d6c8ff5061c88a7c19683ee961289f0a01e19ae4c26b050972`.
The stack-metadata ELF was
`f5abf7ed17d956772ac4fc99fc8cb3deac1b529aebdd0ebaf3e7b66ad7e6b90d`.
For the scratch variant those four hashes were
`494a1de3a6ff313ad67d14f112045c245dc6fe84aa02340989447fb3d7e50f1e`,
`379a378781950fc97f112f4c0bdbe7e765c255d2c6c389448d9bfcbdef41d151`,
`0623638e6366e42591d2a36956ac51638a96d8d147b204c475f6d428ea7e6c9f`, and
`4f6fc8c947842ff3b6bc9239e2dedf0297b0a400ddaa11231e85a13916dd2283`.
For the optimized scalar/block pair, MAIN hashes were
`ec70b9efbdc0f513458b11e0f965b3741dee2bb0700abc247ad9691c2b758471` /
`86d4d7e971ed38c1828fa14f3207f47db7a67351b4329f0f4bc2755131e74ff7`,
ELFs `bb88af29a300baad26719d92e4f68f8f55cda14b104a7ea1d6fc28e4c4d34997` /
`21b9e5f82b051761beb4c44a56db2d92c20e9d9a7359ce516fd6d78cfdc724a7`,
recipes `b2c0ff848da35bd208fe290e8cae933c5127648d5d5ad5530d67e456f4250397` /
`2e382fd45ff356026a49eae1dd77319fbb3d58cc715591b7418859f4f209e503`, and
observations `4a713dbe0a00e430664594fe590f36382fb5a122a60b8da0acf97d27f2a3457d` /
`fabeefcdf09df40e27e81e59cf0e533a4d7c98deda1b61e20bf6f25ce1f684c1`.
The initial diagnostic NORs, reconstructed from stock and the respective MAIN,
were `f1f1608c8950368ee75aef40589ae5f9af171e16c1b46c86df2a3540e7813036`,
`31a319e8f28588415bb426441e6276d044e5719abf36c2c4afa3747e9a3a63e0`,
`dc54d9845886d8a9a232afc06e1d2f8e6d53d03bda6e445ebe70c06279b6d333`, and
`ffa12db6ca9e9c7e4c2642343cddc3d1ca6d483a5300aac45017d68d948ba6b5`.
The scratch variant's temporary block, Velvet, routing and callback sources
hashed to `ad5a06a2f5e33ecccd28b8fd991011d9035c70da5cbb81bc6c982a932fd08f45`,
`dee1b59b6c995368fb316ed5b81e6fc731e9e390a1f80faf974606660580b4ab`,
`5b6861719380ea931b43dbd8cd084656320084369e26abc5d04a0613bf20a232`, and
`444c944924aac6dacf7c3fc207a29fad9b78b150899120e4b161f9fc7a33def9`.

## Forced late eDMA interrupt (QEMU only)

[`late_dma_irq`](late_dma_irq/) contains disposable patches and a QEMU plugin
for the file-mix handoff diagnostic. It replaces the synthetic semaphore give
with a custom QEMU MMIO trigger that raises eDMA channel 15 after stream stop.
The plugin counts entries to the guest IRQ handler, transfer dispatcher, RX
stream callback, audio-completion callback, and audio block processor. The
checker reuses the existing handoff checks and requires all five entries.
The `mix_handoff` signal field in this experiment marks the injected request;
it is not the return value of a semaphore give.

**Never install these diagnostic images or use the patched QEMU as a normal
emulator.** The trigger at `0x400e80f0` is invented and is not a hardware
register. This models an interrupt forced after stop; it does not establish
whether such an interrupt occurs naturally, qualify DMA/NVIC timing, exercise a
successful file mix, or resolve shared-buffer and effect-state ownership.

Starting from a staged `emulator/qemu/build-source/`, copy the source and apply
`qemu.patch` only to that disposable copy. Configure a separate build with ARM
system emulation, TCG and plugins enabled; for example, use the configure flags
recorded by your existing QEMU build and add `--enable-plugins`. The patch's
paths are relative to this repository, so when applying it in the copied
source's `hw/arm` directory use `patch -p5`. Build the copied source and record
the resulting QEMU executable's SHA-256; do not rely on a source commit alone.
Compile `trace.c` as a QEMU plugin against that build's `qemu-plugin.h` and
GLib headers, and use a wrapper that passes `-plugin PATH` to this custom QEMU.

Separately copy `patches/dual-efx/src` and `tools/l6dsp-tests/probe` into an
otherwise empty temporary tree retaining their relative paths. From that tree,
apply `probe.patch` with `patch -p1`. Use its `prepare.py` with a matching
stock firmware and state to build `--callback --file-mix-handoff pending` and
`--callback --file-mix-handoff atomic` into separate fresh directories. Run
each generated recipe with `l6dbg`, the custom QEMU wrapper, and a private
GDB socket, as described in the [handoff diagnostic](../probe/README.md#forced-file-mix-handoff).
Build a third atomic fixture with `--late-dma-control` for the no-interrupt
control. A fourth atomic fixture using `--late-dma-outer` moves live admission
to the complete AudioProcess block and removes the inner wrapper guard. The probe also fills the live audio block's two scratch arrays with
synthetic markers while its file-mix lease is held and checks them after the
injection window. These arrays are not established file-mix buffers; the
markers do not constitute a successful file mix.

Then check the results:

```sh
python3 tools/l6dsp-tests/experiments/late_dma_irq/check.py PENDING_RESULT PENDING_ENGINE_STDERR
python3 tools/l6dsp-tests/experiments/late_dma_irq/check.py ATOMIC_RESULT ATOMIC_ENGINE_STDERR --guarded
python3 tools/l6dsp-tests/experiments/late_dma_irq/check.py CONTROL_RESULT CONTROL_ENGINE_STDERR --control
python3 tools/l6dsp-tests/experiments/late_dma_irq/check.py OUTER_RESULT OUTER_ENGINE_STDERR --outer
```

Before the scratch markers were added, one paired run recorded each watched guest entry
once. The unguarded callback count increased from 28 to 29; the atomic guard
held it at 28 and recorded one bypass. Both no-card mix calls returned 9 and
callback activity resumed after restart. The custom QEMU SHA-256 was
`23e4c6a57b2a265cc4fa460d67ae561b7b69bc81de74ec724503f4686dc75f21`
(source provenance unattested); pending/atomic MAIN hashes were
`adb4fcd45182bbe55a7b5e7d236f8d52ae2dac138b330b2de3dcf45fa45836ff` and
`c0b48e49c8c909043cf19aeda213a974b154f7dab825faadaba557023db16e2b`.
Stock MAIN and initial NOR were
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461` and
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`.
The pending/atomic ELF hashes were
`6da7fc0cd1be0dac5056d322c5a1f394c2c962b3f647f8387b5c68222198d90b` and
`9af22cf47dfb6f9da6944abd4d2b632bd58f5a0feeee4a4cfe60705ea4d95bdc`;
recipe hashes were `f0b7d9d37590d6f17273cdd009c61bc6b67c3584d6b9099a54eddfb897bf1543`
and `424be4eecbc621d30aaab0d88b0a006ab65e6f2c5769fe028dc339b3af5897fe`;
result hashes were `f3d6d2140c88aab7181bb5646024e05714e7e779fd39a62f0a9e34e5f01f832f`
and `522fdf197d54af8ffaffc80bea18fe0b4ce673b30fd61ef3f4544d396876b58c`.
Both plugin logs hashed to
`e83a4b0a7457eeee13fba6fa013481b1b3abcc506f10078f48c3bd44beaa8905`;
the plugin binary was `ab2b9c89d88e0e6ff9f46b1a0b1201d184d22d8fb832d654f6b4c330312291e5`.
Initial panel flash, options and RTC were
`6569a6b15c745b7ca05339180f67a04dc3c0be7de2cc8559b34baa6486918443`,
`bf906cd362964d265fdb27547a75d2ad2ce86cccec49cdc613764a77dc5f149d`, and
`473ef0237adcc70aa6ba783e573df1258ecd1ae74d1ff7bb0e97ce7e9932d85f`;
`l6dbg` was `aed900b0f7f43f225233f7f1a33e5ba8e159d4a698905297e0fb48c5c8519cc0`.

In a later marked-buffer pair, the no-interrupt control retained all 128
markers and recorded no watched guest entries. The forced-IRQ atomic run
recorded all five entries once, rejected one callback, and changed all 64
words in each live scratch array while the file-mix lease was held. Both
no-card calls returned 9 and audio callbacks resumed. This confirms the guard
does not cover the whole live audio block in that modeled schedule; it does
not show that a real file mix uses those arrays or that normal execution
corrupts file output. The earlier pre-marker result files above lack the new
`mix_buffers` observation and cannot pass the revised checker.

For the marked atomic/control runs respectively, MAIN hashes were
`74a0de26a61f79dc7220d166c8b244d7048ea43c342f56f596f56cb092512739` and
`67415b89cdcbf515f217c6ef31ab37cc4792895da49148bb319b84f2b303855f`;
ELFs were `5361fc2365b5df33fcc3408387021a51dd94d2fb6fd76030faf7cda9d90573f1`
and `5b15d33ed0fc7f3fb441a741d114aa598ebbce7cf9b3844c53fd0ac7dbbabae4`;
recipes were `65084459b593c8a80f80586424ec8577939310e0e0034acad5d0f2dda2c955ec`
and `854620750836d6d1de830ebcd28bb49cc7b8d16b9542b56c0d822cfa6781f04c`;
results were `cfae56431d13fb421a3448c51f622417568a49f79b4020e7f321c2ec0da5cd5d`
and `0c305c5fa7e18fed2a49f2a0665e007728baf5967c4849f700916c8596166e5b`.
The custom QEMU, stock fixture and `l6dbg` identities match those above.

In one forced-IRQ run of the outer-block prototype, all four watched IRQ and
completion entries executed, but the stock audio-block entry did not. Both
64-word scratch markers stayed intact, the guard counted one rejection, and
callbacks resumed after the no-card mix returned 9. This is one deliberately
forced schedule with synthetic markers. The diagnostic drops the entire audio
block when the gate is unavailable: it does not establish output continuity,
safe handling of a block rejected while the stream is active, selector or
other-caller exclusion, or safe initialization and storage. Earlier testing
showed repeated output when whole audio blocks were skipped while the stream
was running. This is not an installable handoff.

The outer diagnostic MAIN, ELF, recipe, result and plugin log hashes were
`59806ddc4111f10901e38148f21a17d5736f9f7b827935f8a7b747f252b97cde`,
`d0e982113935b635351c6a4c2aa9f8826bc2d07b313f06f529fba0cf61824c1a`,
`136dd0e48efa4499c7d986d4b6a04378d4c60f91454fd0b379df374d1e196c41`,
`3e408378a3cb828fc2bd20c19c750986393c6d407d3bf7ca568d3edb0c80a878`, and
`2548aeb6df6060affa555db0ba500c346e78275f4e6d5dac975549c70b069880`.
The custom QEMU, stock fixture and `l6dbg` identities match those above.

### Modeled output after the outer-block handoff

For a separate output observation, apply `tone_qemu.patch` after `qemu.patch`
and `tone_probe.patch` after `probe.patch` in the disposable copies. The
additional QEMU changes supply a gated 500 Hz square wave to every modeled
receive word and record DMA-capture offsets for the injected request and TX
restart. The patches remain model-only and must not be used on hardware.
Build two fresh outer fixtures, one with `--late-dma-outer` and one with both
`--late-dma-outer --late-dma-control`. For each generated recipe, insert these
steps immediately after the initial 45-second wait, before the first EFX press:

```toml
[[step]]
action = "press"
target = 20
hold_ms = 150
[[step]]
action = "encoder"
channel = 1
steps = 8
[[step]]
action = "wait"
guest_ms = 2000
[[step]]
action = "press"
target = 35
hold_ms = 150
[[step]]
action = "encoder"
channel = 1
steps = 8
[[step]]
action = "wait"
guest_ms = 2000
```

For each run set `L6_AUDIO_DMA_SOURCE_CAPTURE` to a fresh `dma.bin` in its
output directory and `L6_DMA_MARKERS` to that directory's `markers.txt`.
Use the plugin wrapper and GDB socket as above, saving the `l6dbg` output as
`result.json`. The two directories must each contain `result.json`, `dma.bin`,
`markers.txt`, and `logs/engine.stderr`. Compare them with:

```sh
python3 tools/l6dsp-tests/experiments/late_dma_irq/check_restart.py FORCED_DIR CONTROL_DIR
```

The checker validates the handoff and capture format and reports differences;
it does not interpret a difference threshold as an audio pass. In one paired
run, neither capture gained DMA records between injection and restart. Both
saved TX buffers were all zero, and the first 1,176 resumed source records
matched those buffers before nonzero data appeared. Over the first 12,000
records the source addresses matched; nine records differed in sample values,
with a maximum signed integer difference of one. One 32-byte record contained
one modeled eight-slot frame. The capture has no per-record timestamps and uses
a synthetic input, so these counts do not prove wall-time continuity, physical
output, or safety. In particular, the guard still drops a block and the actual
successful file-mix path and other admission schedules are unqualified.

The custom QEMU binary was
`690d309797a4c588b97e50b8508dd0adc90b454a8fef07dcd28c48c38a88803f`
(source provenance unattested); the plugin, stock fixture and `l6dbg` matched
the preceding experiment. Forced/control MAIN hashes were
`83dcd3e8835596f39e50a6e7b374f19406fc6a19b29e3635d7601dbe3a7625af` and
`7654b8f99b17ac1901bfc0e3d7a97eed4c51e823bc4c986170a29d0b6233a017`;
ELFs were `528aba8145d46d9a238c1a86c16cef2678862ae2f4c61bf894bfddd5f3c9568b`
and `a4c5aeff93dc5bb18907becda9767e763dbd76d47e73d397f752ae4f591d0eab`;
recipes were `c668ea591f83ad05fed8f586c5fff5ec333ffd5b295ae6ecef5266ac3446915c`
and `d251c1ad7ebec2a42633e4e7bb712089d314b4ee3248c99bf255b63c00d3c289`;
results were `1ddf7d3d26cd8aa50ed085ccc81eadabd10736710ae52105d069c621f66aad6d`
and `0824e288ac2aae570cb9f5b2d6ce25fb85e29b4a736e6b13edc68ac6600e25f2`;
captures were `a3f3455258b6b22ab2a0a3e05e3dfd5042cc85276ead234c479d5932384f3897`
and `a65deefc0b4f19641ea2036b399b7230d12a677d3ad37458992d3dbdb91641b6`.
Both marker files hashed to
`d0ed9cb658aca39d258ef8dc40add502cd57e73900bd4f588651aadeb76c9d2f`.
