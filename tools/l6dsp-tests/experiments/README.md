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

The `experimental_plateau_32k` compile configuration selects a standalone
32 kHz core with size 0.01–4 and 0–500 ms predelay. Run its focused test with
`RUSTFLAGS='--cfg experimental_plateau_32k' cargo test --offline --locked -p l6dsp-tests plateau::tests`.
It is not connected to the converter, UI, or firmware, and the normal 48 kHz
test suite cannot run against it unchanged. Its ARM processor type is 511,072
bytes excluding the rate converter; the size-query object SHA-256 was
`0899ea4b81e4b087f9404fc4079c8a37f44fcd89608df90292b225619bf4c637`
using `rustc 1.99.0-nightly (1ed2df61a 2026-08-04)`. An earlier 510,944-byte
estimate omitted maximum modulation headroom from two allpasses; the focused
test exposed that error and the current buffers include it. The host model's
input/output histories add 1,024 bytes; coefficients and other overhead are
separate. This does not qualify allocation or target timing.

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
checks a separate tap-outer, mirrored-history FIR against a direct sample
reference. It exercises 285 taps, two filter banks, two channels, independent
send histories, a per-frame crossfade, a non-aligned split and wraparound.
It does not implement live parameter delivery, bank generation, output
clipping, firmware integration or memory ownership. The extra mirrored history
is included in neither the existing processor size nor the heap estimate. The
two mirrored sample arrays take 76,832 bytes; the current processor's three
unmirrored sample arrays take 57,624 bytes, excluding other state in both cases.
The block size and implementation are informed by the MIT-licensed
Multistomp Velvet port at commit `7ea50ae6e30705989d0b8414ad5a83c97dbef0db`;
that pedal's tests do not qualify the L6max implementation.

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
