# L6max 1.10 effect callback investigation

This is a partial map for the L6max MAIN image with SHA-256
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`.
It is not a specification for L6 or another firmware revision, and does not
qualify a patch for hardware. The `dual-efx` builder remains disabled; see
[safety status](../dual-efx-safety-status.md).
Initial host-side physical rig discovery is recorded separately in
[L6max physical audio observations](l6max-physical-audio.md); it does not
establish the connected firmware revision or qualify this callback.

## Observed callback dispatch

The stock call at `0x8000277c` reaches relocated code at `0x20280580`.
That routine selects an effect descriptor and updates parameters and callbacks;
it is not the 16-frame audio callback assumed by the original patch. The
descriptor table observed at `0x2028b8c0` has five 28-byte entries. Its first
words were, in table order:

| Entry | Callback pointer observed | Stock selector observation |
| --- | --- | --- |
| 0 | `0x20281649` | Before EFX SEL presses in a setup-complete boot |
| 1 | `0x20280ab9` | After one EFX SEL press |
| 2 | `0x20282221` | After two EFX SEL presses |
| 3 | `0x8002c041` | Not tested in this observation |
| 4 | `0x20280781` | Not tested in this observation |

The active callback pointer is at `0x202a6f20`. Static inspection shows a
wrapper at `0x202800e8` loading that pointer and calling it at `0x20280248`.
Before the call, the wrapper scales a 64-element input array at `0x20010210`
and copies the same samples into both 64-element scratch channels; afterward
it copies the two scratch channels back with the inverse scale. This observed
wrapper therefore does not expose separate per-channel sends. The
effect code for table entries 1 and 2 also contains 64-iteration floating-point
processing loops. The one- and two-press progression is consistent with Room
and Spring in the documented selector sequence; nonzero audio behavior and
other starting states require further runtime tests.

In a stock headless run, a GDB breakpoint at `0x20280248` stopped immediately
before the indirect call. Registers were `r0=0x804683a4`, `r1=0x8045fe98`,
`r2=0x202a6f3c`, and `r12=0x20281649`. At that stop, `r1 + 4` held
`0x202a6f3c`; this is the first of the wrapper's two 64-float scratch arrays.
These arguments point to descriptors and scratch, not the two interleaved
32-float buffers asserted by the old patch. The recorded sample values were
zero, so this observation does not prove nonzero signal routing or audio
fidelity.

## Candidate channel accumulation loop

A write watchpoint on the first input word `0x20010210` stopped in a
floating-point fill routine at `0x202300cc`, called from `0x20228ac0`.
Static inspection of the caller shows it clearing 128 floats beginning at
`0x20010210` and then iterating over 12 entries. Both branches of the loop
multiply 64 samples per entry and accumulate into the first 64-float region;
one also uses a per-sample multiplier. The sample source addresses in that
loop are `0x2000e410 + entry * 0x100`. This is a candidate place to obtain
separate send contributions, not yet a validated insertion point: the entry
mapping, gain semantics, thread ownership, ordering and output destinations
must still be confirmed with nonzero inputs and controls.

The watched stop used a fresh emulator state and stock MAIN; its observed
return address was `0x20228ac5` and QEMU stopped at `0x202300cc`. Captured
256-byte regions at `0x20228a80` and `0x20230080` matched MAIN file offsets
`0xfd4c8` and `0x104ac8` respectively. The watchpoint log SHA-256 was
`2c22a4b249db9e4bf7c8fad99a5ab7b5643d26709bdfe1750b90e73d7b9c107a`;
the captures hashed to
`bb643a48ea864c1404f696b0059326651f564df0455c6a9476da9af511d37fa9` and
`192d03418a27ba16f8b6a7ab7e47615a89733bd82271645dd4f34e1dcfb12d3a`.
The log establishes the clear operation, while the channel accumulation is
inferred from the following stock instructions.

A separate GDB probe stopped at `0x20228b08`, forced the mixer loop's
branch selectors and gain values, and injected a `0.1` impulse in one source
slot at a time. At `0x20228d54`, each of the 12 isolated slots produced
`0.10000000149011612` at `0x20010210`, with the rest of that 64-sample send
block zero. This confirms the candidate input-to-send path under forced
conditions; it does not establish how physical channel controls map to slots,
or which gains the firmware normally selects. The recipe, probe script and
log SHA-256 values were, respectively,
`a5f0e3fe4c0c45087c5bfa6155eef46d585a239c3087d4d9e83a414ad8b8d187`,
`c9a000d27b9bbeb5264270590d239054b48eb6ebeafcae06649b21a7b031deec`, and
`d3d61b4c37e572066b2e726f66db7c4e6238fded62f8da409080338e04aec387`.
The forced state was confined to a disposable emulator run.

In a further disposable run, each slot in turn was given a constant `0.125`
input and a forced per-sample multiplier ramp from `0/64` to `63/64`; other
slots were silent. With the other gain forced to unity, all 64 output samples
matched `0.125 * ramp` exactly for each of the 12 slots. The corresponding
descriptor's multiplier pointer advanced by `0x100` bytes after the block.
The multiplier buffer was placed in QEMU-only scratch memory: this does not
establish a usable hardware address, real control behavior, or buffer
lifetime. The recipe, debugger script and log SHA-256 values were
`a5f0e3fe4c0c45087c5bfa6155eef46d585a239c3087d4d9e83a414ad8b8d187`,
`c15a11de5f22eeb45fd9c2f18b4fc764ea7bc9e5365544b62c47219b820656e6`, and
`c9a9013fac1397a2a3c4b10b08a52a9420822fbd54a33e9053fc7df65fd5979a`.

In another forced-state stock-QEMU run, all 12 inputs were nonzero, six
entries used a constant gain and six used the injected per-sample ramp. With
the other gain forced to unity, the accumulated 64 samples exactly matched the
sum calculated from those injected values. Between the end of the mixer loop
and the following effect callback, the input-sample region, the 12 ramp
descriptors, the forced ramp data, and the summed send were unchanged in that
observation. The active ramp pointers had advanced by `0x100` bytes. The
injected ramp was in QEMU-only scratch; actual stock ramp-buffer ownership,
gain stability at the callback and behavior under real control activity are
not established. This is evidence for investigating reconstruction at the
callback, not a validated hardware hook. The recipe, debugger script and log
SHA-256 values were
`a5f0e3fe4c0c45087c5bfa6155eef46d585a239c3087d4d9e83a414ad8b8d187`,
`b10a46032106be9a7eba9ca04d8c0d982682a71c244cac3f5e4eddcf9b3120d1`, and
`98db8c0ec1a4e11e8e2aea17037a49919c488c718ad0ce565927430df136280b`.
A repeat with the same forced inputs also captured the gain region
`0x20016138..0x200161cf`: it was unchanged between the two stops, and both
wrapper scratch channels exactly matched the summed output scaled by `2^-31`.
The repeat script and log hashes were
`ef02cfd4c556bcd53e497a2e8231d298cff5c4919379a174103000ad8598f96d` and
`0a5d56950a29735621371c2cc149c41a0826fcc32d125037577e9721feb9379f`.
This remains a forced-state observation, not proof of lifetime under real
gain changes.

In a stock run with LEVEL and EFX encoder adjustments, a breakpoint on the
gain setter observed channel 1 with smoothing requested. The following mixer
block had a live ramp descriptor for slot 0. At the mixer end and subsequent
callback, the gain region, all descriptors, source buffers, summed send and
that ramp's preceding 64 floats were unchanged. In the repeat observation the
advanced ramp pointer was `0x80507118`, the ramp ranged from
`9.0857572555542` to `9.122406959533691`, the slot's scalar gain was
`9.531618118286133`, and its smoothed multiplier was `0.9999998211860657`.
These are observations from two runs of one control sequence, not a guarantee of
buffer lifetime across all updates or target hardware. The shared recipe,
first script and log, and repeat script and log SHA-256 values were
`24d61576bb37144269bdf040077b3bef5a2873b9a1e68b56e799c440b7c83fb8`,
`576e55db439995dd1d3ceada48d4fdce26592c77ea052b207059e8b54e6309dd`,
`1a40adfdfe0dbee8bb81e95c87d4a60b3040a24c0cc303e720a903e1bb419d9b`,
`7656f72184d9b07323285c8fd8c1f78df6a469e2336f7b7f57b77ed0895b60ae`, and
`073bbeb202aa7d13d343c349a6303877ccd02159e3f3eeececc46833635718c1`.

In a third run of the same control sequence, the debugger injected a constant
nonzero slot-0 source immediately before a block with the stock-generated
ramp and silenced the other 11 source slots. The 64 accumulated values exactly
matched the injected source multiplied by the observed per-frame ramp and
smoothed multiplier. The gain region, descriptors, source buffers, summed
send and ramp were unchanged at the subsequent effect callback. The ramp
began at `0x80507018`; this was a real firmware-generated gain ramp in the
emulator, while the source signal was injected by the debugger. This single
observation does not establish all update races or hardware timing. The
debugger script and log hashes were
`ab5925c45ce73f3b716c2c588f9514e72d7356fb1bf1c0ca5c0eab7a28e0b521` and
`b00cb2dc5c17e73c178dd6dcf07d86a24206c72bac2f78c07f4a9e2895d13a71`.

A QEMU-only callback prototype then read those candidate inputs and gains at
the callback and accumulated three separate sends with fixed test routes
`entry % 4`: A, B, A-to-B, and off. The debugger injected nonzero inputs for
all entries, with static gains on odd entries and forced ramps in QEMU-only
scratch on even entries. The callback's 192 accumulated samples exactly
matched independently computed test values; its returned block was finite,
bounded and nonzero, peaking at `0.008113457821309566`. This diagnostic
advanced the control stream before processing the block, rather than proving
the stock per-frame timing, and did not implement selectable routes or UI.
Its source, ELF, MAIN, recipe, debugger script and log hashes were
`5981933d68b0054ec6e391613f4325359fe8f441d71e45b11541b2abb458862b`,
`42cb755ff494ab216b2203811d640af9e116b54320f86d6642b01ef0deee5bfd`,
`a5c2aa5960903be516ad441e9140bf8bb895884b8e4014a250d789d3b2d3254b`,
`66483708859a084264b800597c05d7f9232f52afe99abb6479326d183b7fd513`,
`a7e42ae3b5fc048ec1773d9d65c840772eb3efd949a79162acac01ac134bcee9`, and
`9e4d87f8c822ba8085056d7ead50f1f946a256939ead9cfb661737fde4b33371`.

A subsequent QEMU-only variant copied the initialized DSP state to a second
location in the effect buffer, then processed the same forced inputs once via
the three reconstructed buses and once via `process_weighted_block`. The 128
returned samples matched exactly in that run. Copying the large state and
using the second buffer were diagnostic operations and are not a real-time
or hardware-safe technique. The source, ELF, MAIN, recipe, debugger script
and log hashes were
`f1953e4205429849b931e3d7d81dc26b081b246ff1f5252fc2272a866e20637a`,
`6a8da9a96c707e297b6125dae8e4c0e21cc90c862cc9bbac161a2de333eace45`,
`cdef179503a623fe414bc76b9f26fcf089cc41eb60c9e1d1a84efe1eafee8d1e`,
`b7509ba4b41122b4edac14488a1b6895735344f4d315dae1c7b052f37703d80b`,
`592dba7d12d567f2d506c6b0a699a255e101b5aafec52b2999f424db03eaa89d`, and
`819a566b00c060198a0bbad22e231b8f305ff37086a1fcdbc612eacde3752fd4`.

The checked-in QEMU-only callback diagnostic now reconstructs the three sends
from those candidate mixer inputs and uses the routed DSP one frame at a time,
advancing the stock control stream at each frame. A debugger set two different
12-slot route patterns through its QEMU-only mailbox and injected nonzero
sources with alternating fixed and per-frame ramp gains. In both cases all
192 accumulated samples matched independently calculated values exactly;
the returned blocks were finite and bounded, with peaks `0.0081134578` and
`0.0150339631`. These were forced inputs and test routes, not physical input,
button, UI, persistence, source-slot mapping, or deadline qualification.

The callback source, routing source, diagnostic ELF, MAIN, recipe, debugger
script and log SHA-256 values were respectively
`5857f1d39340596cc011a504dc718d2965333fff4837f1a9746ecc7191776299`,
`ebda32921252b24432887ec0a3d60f758cac1de83339d124e274d819207ddff2`,
`4c6b8e6616fa252a056396ed2ffa9f06bcc01203228a33df1bf9bb8208126975`,
`e91025b353e35278d7a5daf8dac76946a93e8f1f8e2f0865e778bfb00f9e35f4`,
`d1d115f29d5b54df17ae152f41dff34cdf5bdf465fab9efd4476b92adb5f4310`,
`e3b8cfb23c96b9a3f0fa3b6543926949fe5a2f30ce4ba357c83bd09be0de2dd0`, and
`2329003fb535bbfcebae7fd837084c518447330fdfab1b6495c895890c3d600f`.
After the injections, the recipe continued through the custom Spring callback
and stock Delay, Echo, AI Noise Reduction and Hall callbacks in order; its
result SHA-256 was
`0f8c6c0d3d03751c983dc0579b8572e68417765448aeb66881d15bb6fe779dcd`.
The stock MAIN, NOR and QEMU executable matched the identities below.

A control-driven attempt set a breakpoint at the ramp branch, then selected
EFX and LEVEL and turned encoder 1 in each mode. The debugger timed out without
observing the branch. A separate forced-ramp run did hit that breakpoint, so
the location can detect at least the injected case. The timeout does not prove
that normal controls never use ramps or that their data remains available at
the callback. The control recipe, debugger script, log and diagnostic SHA-256
values were
`0e7a8ad94007a37f98decbc3338c0532073232025e2012a35ba70cc150653533`,
`fc1d92c29a70b27fbdc76d059a787eb753e59bb03109f3f66037dcfad50df6f7`,
`a8fa53fc160abe22d9c6b8d6ce3ae2905cf2b65baa006c402fa81527e4e7504c`, and
`baaa86326a7dca862c860c56aab22ed8d693e3c07326c4831991bf3e097e61d3`.
The forced-breakpoint script and log SHA-256 values were
`ba45970d75d170f5c8146d8a3617ec8535243a45aa06cd5272b267c56a859fb8` and
`bb17c056adb7c6f526fa2d184181225f8aa3e3b7b0f2ee1f2a67e584c7a690a3`.

Two non-stopping follow-up experiments supplied a square wave of signed S32
values `+/-0x10000000` to every modeled SAI receive word in a disposable
QEMU build. A diagnostic callback captured the 12 source blocks, gain region,
stock accumulated send and reconstructed B send together on the first nonzero
block. Panel events set LEVEL and EFX before selecting Room; all diagnostic
routes were B. With only strip 1 adjusted, its gain was about 7.08, the other
11 gains were zero, and all 64 samples of both sends were finite and nonzero.
The maximum difference after scaling the stock send by `2^-31` was about
`1.5e-8`.

In a second run, controls for all eight strips were adjusted and the 12
captured gains were nonzero. All 64 send samples were finite and nonzero; the
largest scaled difference was approximately `1.9e-6`. The reconstructed peak
was about 18.13, exceeding normalized full scale: this is a calculation
comparison, not an audio-quality or output-safety test. The injected signal
was identical on all modeled RX words, and the single captured block does not
establish physical input mapping, independent routing, ramp lifetime, control
responsiveness under sustained audio or hardware timing. The snapshot and
unowned scratch memory alter the QEMU workload; never install this image.

Both runs used stock MAIN SHA-256
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`
and initial NOR SHA-256
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`.
The modified QEMU executable SHA-256 was
`931151b462c98c6806223b387b1e47de385b8578ea66a0a6aa0ff0775941dab6`;
its source provenance is unattested. The modified emulator source and
diagnostic callback SHA-256 values were
`d7b5fc1fc42789a03cdbbe1e06ca744a1ecd17e5d7192b33d22e8cfdadbf247b` and
`210ee9f2722ef81cf47c88575b1e8ca48bd4d4a233986b93bb29d1c7bb770577`.
The common diagnostic MAIN and ELF hashes were
`32a5a808ffaed58572e53aaa86eaffa00b3ac9ac1b9da4107377f30b47b992ca` and
`1017c7d8a4bd608762b4721231e8eef269c2cbbba928cb9859d54d6d889e5605`.
The first recipe and result hashes were
`d1479848adb75c35488c2d7ebda8ad8963c11ef5a8d0441777be73230bf3ee15` and
`2563d918ede42d9357fb26d001c64426b76ab91925a3ec9bfc20c2f4a6e48a7d`;
the second were
`d7cf321fb37704a9b530d541c302429895943caa3c58f2baf7dcbdc32e5ba8be` and
`d0e221141677d8e53b8aa9f123a9729cf64e328ecd5f11b338d20d91e762a324`.
Firmware, state and runtime captures remain local.

A further pair of disposable runs supplied different amplitudes and phases
across modeled SAI receive words. The diagnostic assigned slots 0 through 11
cyclically to off, A, B and A-to-B, and captured all source blocks, gain and
ramp descriptors, and three reconstructed buses together. In the first run,
the 12 captured source blocks were distinct, all gains were nonzero and no
ramp descriptors were active. Each of the 192 bus samples exactly matched an
independent calculation from the captured inputs and gains.

A second run captured the buses and output after at least 150 callbacks. The
192 bus samples again matched the independent calculation; all 128 output
samples were finite and nonzero. The output peaks were 1.0 and approximately
0.9992, so this intentionally high-level fixture reached the limiter and does
not establish fidelity. These routes were fixed in the diagnostic, not set by
the user interface. The observations do not establish physical input mapping,
ramp behavior, hardware ownership, responsiveness or timing.

Both runs used the stock MAIN and initial NOR identities above. The modified
QEMU source and executable SHA-256 values were
`495b2e5ff206f215afcbf35354532532a315ef8616df34ee724947b400bd9383` and
`a8c802944f9203a489b600c2fcac65945a004fdd0ee7abc7a904936d096fa954`;
its source provenance remains unattested. For the first run the diagnostic
callback, MAIN, ELF, recipe and result hashes were respectively
`f65e7ff4fb8d89ad813c9279fbc58e84e831edbc4471ed8437b0ab752d9bc0ca`,
`d042deb0d87869aa2537710c809290ad4478a2c1f05c9086f05e6fcb6f790575`,
`ce807cc24ad7c96c455eb610481d2e778e01531eab84f13ce107d3f368e86ade`,
`3496f5c4d9e445b69f0cea54f4f5bf058c6a54515e4b3786bb224302ebc13134`, and
`19e11eb90a19404246efd29fea718216309beef413e1dfa3ba1a026b6edbc5cf`.
For the output-capture run they were
`280b86337d0b486b83123d337d8566aa61f2fb625c93016157548cd5f8e1c66f`,
`90626bb9535aa9bc134bd01b14a1a068377056fc93a040c16725bac4837e3707`,
`116b80719a0923874632a9c2e359368be6a4605f05c353035257c33c04887fb9`,
`18870d8a7991f8c426783aad1494740657601a2b74075efb36cdc0a3c23f59dc`, and
`79cab44c2e0c940d3fbb254bed5b6ea10429c36566758f8dfa6dfca4a082bdf1`.
The modified source, firmware and captures are local diagnostic artifacts.

Another disposable QEMU variant held that modeled input silent until 85 guest
seconds. Before input began, the inspector's EFX page and encoder turns changed
the initial all-B route array to `(1, 3, 0, 2, 1, 1, 2, 2, 2, 2, 2, 2)`:
strip 1 A, strip 2 A-to-B, strip 3 off, paired strip 5 A, and the remainder B.
The same array was captured inside the callback after the input started; the
192 reconstructed samples matched an independent calculation, and no ramp
descriptors were active. An initial capture at signal onset had silent output;
that observation alone cannot distinguish effect latency from a fault.

A repeat waited 150 callbacks after the first nonzero send before capturing.
It recorded the same routes and exact bus comparisons, plus 128 finite,
nonzero output samples with peaks approximately 0.653 and 0.917. At guest time
88.694 seconds the test sent a further encoder turn for strip 1. Three guest
seconds later the route and displayed edit count were unchanged and the
diagnostic event queue remained empty `(5, 5, 0)`. Thus the active-audio edit
was not observed in this run. It does not prove where the event was delayed or
lost, nor establish physical behavior. The first run's capture work and the
repeat's extra capture and controls perturb the emulator workload. Both use
unowned scratch memory and must never be installed.

The modified QEMU source and executable SHA-256 values were
`9360f18a2d2a8b61afeb136f0877e5a8b6452ff499b952869ccfd881f44e3d31` and
`867c1fb8ed49c8acf4217a36cf5ff82502bc81130ae16ec2a997044751e87243`;
source provenance remains unattested. The first diagnostic callback, MAIN,
ELF, recipe and result hashes were respectively
`231dacaa6b621d82000c3a04c5732a25a498b26685085baf42b0dd580a4f8713`,
`a45bcd26190327aedd9417dc205e119e9775bd09a83f3a1fbe9a7e8fcfde755e`,
`b3edb2bed19f96e90040c6f18db47ba348787694a81d549f6608992957d92b26`,
`8ab432c5027e140580e73a555dd07fc21cf3797abb4e0574097902013d09d50f`, and
`04681db30344cf1a8f2aa2948c22e8c76f93acba5027359173daf1ce9d266709`.
The repeat's corresponding hashes were
`b5a4387272a6a1103fe790e11f3576d8ceabd7c28d9df09dafba730cb0cb33b8`,
`0f95d7e5c003c91b193beec37cdba395eae5d0b1b91a5cfb22a56fc6c64dfae0`,
`211b2fa27a2ce6e90bf4e1394d93a2007a8a807eaaf67d8851b6264f64c15f06`,
`1fca5a832992ccf851b50fd3dae13ded5be6a5d432a0a9bdda4c66eb31f6923b`, and
`430c53a8bf96577ddc65e314b381b7299fd4a08879a0589b24c7a66efdfba9d8`.

[Panel-model PR #7](https://github.com/derekstavis/zoom-l6max/pull/7), head
`c8acff7d08cf57d8f70c3ba2f4fbc3071b384d16`, identifies the panel MCU as
STM32G031C4T6 and changes its modeled memory, GPIO and timer behavior. To check
whether those changes explain the active-audio control observation, a
disposable QEMU build used that revision's panel/board source files and the
same delayed patterned RX input. It ran the same diagnostic MAIN, NOR and
recipe as the preceding repeat. The pre-input and captured routes were the
same, all 192 reconstructed bus samples matched the independent calculation,
and the captured output was finite and nonzero. After the active-audio encoder
turn, the route and edit count again remained unchanged and the event queue
was `(5, 5, 0)` at the three-second observation. This single matched comparison
does not locate the missing event, qualify the PR's complete emulator build,
or predict physical timing; its capture and input modifications remain
diagnostic only. The PR is not merged into this branch.

The modified QEMU board source and executable SHA-256 values were
`2adf53d149f109714f1c90d70578d0aff87c448e64312776de55b948f38f4a1e` and
`71bab061900eb23bc8dfb75b9c27c1ede40e873731c1e55c254c25b1cdb1adf8`;
the executable's source provenance is unattested. The recipe hash remained
`1fca5a832992ccf851b50fd3dae13ded5be6a5d432a0a9bdda4c66eb31f6923b`;
the diagnostic result and engine trace SHA-256 values were
`a9906c69321de9584c3a6f05ca2a636d62d9cb47f225818d765b74fb0207ae3b` and
`b8944774015657d234868bd06de2fcb0e97649f321a3cf44c5c58db30c31cbe5`.

To probe the scheduling hypothesis, a disposable callback variant checked
the task names and existing priorities before calling the stock priority
setter for `SubMcuRX` and `Main`. The repeat read back priority 31 for both,
above `AudioProcess` at 30. With the same PR-based emulator and delayed input,
the route setup was unchanged, and the later active-audio encoder turn changed
strip 1 from A to B; the inspector edit count advanced from five to six and
the queue ended at `(6, 6, 0)`. A first run without priority readback observed
the same control change. This is a diagnostic intervention, not a proposed
hardware priority setting: the tasks and scratch locations are revision- and
fixture-specific, and raising them may affect other real-time work.

Over the same 3.2 guest-second interval, the matched default-priority control
counted 1,753 diagnostic audio callbacks, while the two raised-priority runs
counted 1,754 and 1,755, approximately 548 per guest second in each case. The
default-priority control did not show the active encoder edit. These brief
counts do not establish missed physical samples, worst-case timing, scheduler
fairness, or the effect on recorder, MIDI, and other tasks. They remain below
the nominal modeled 750 callbacks per second and do not qualify the firmware.

The first intervention's callback, MAIN, ELF, recipe and result SHA-256 values
were respectively
`a939f91d898a4d403e13c0768ba691b17b1d64091f94e1c4f2f58ecd36f05017`,
`1210a8d2668dce4cf481724d01aa02f61fc75dfc3dc8a6930822fa030d06fff4`,
`fc70a5b7fb00c1ecb9d027c565f25439132ec3a4c85e4b0a45b0f75b85969799`,
`a3d430814a52f1af8cb9bae813b40de3eed6f6880e5b3f8e05ab3b3a0e303e6a`, and
`cef04b44e206287504cc1b4ac4fc304e937099217a3b737093ecabe665e6b63f`.
The priority-readback repeat's corresponding hashes were
`9dbe1e2aba5091e99fc85ba85567d59ce74faeb499e2ef8501ad61cb4aca6486`,
`caec1e6a5aadac417b1787807a642fe36b24e2224c2a2f7af40805ae7e375fab`,
`384acf67c9866b608698527f72a5e7739550d28735223eea3083c29b25470086`,
`24f83621c63afc568aa650302fc5c3790029e87befeb4a47d1c9cdb3cfa7c776`, and
`36873fdd0418afdfc596d489391fd66008c106bd38d4b6580d81cb9becb3ecde`.
The matched control used the preceding PR comparison's callback, MAIN and
ELF, with recipe and result hashes
`d26f2c1a6f578316a9945603400a91f0f8a8a293d5bb19a868e9626a57712d52` and
`b3437e2566f1eb66c0cd16611e1ac59b64fc6a3731949e1e1807d3d6eed57ce3`.

## EFX encoder and stock effect storage observations

With EFX mode selected, turning physical encoders 1 through 8 changed the
stock EFX control bytes accessed by `0x8000b0f0`. Their observed primary
indices were `0, 1, 2, 3, 4, 6, 8, 10` respectively; the paired byte two
positions later also changed. This agrees with the stock physical-channel
mapping at `0x800564b0`. Some values were still changing when the next
encoder was turned, so the captured values are not settled gain measurements.
A separate capture of `0x20016138..0x20016337` showed no changes during the
same style of turns. A follow-up snapshot after turning encoder 1 showed the float at
`0x20016d00` change from `0.0` to `1.7782794`. Static inspection of stock
setter `0x800118e0` shows it writes this target from the table at `0x800a70c4`
and combines it with other channel factors before updating the accumulation
loop's gain. At that observation, the other factor at `0x20016b80` was zero,
explaining why the effective gain remained zero. In another run, after the
same EFX encoder adjustment, selecting LEVEL and turning encoder 1 changed
`0x20016b80` from `0.0` to `4.6415887`; the EFX target remained `1.7782794`
and the effective gain at `0x20016170` changed from `0.0` to `8.2540417`.
This establishes that the level control affects that gain in this fixture;
the observations do not by themselves prove all factor semantics or the
audio-slot mapping. The time-varying gain path still needs verification. The
LEVEL follow-up recipe and result SHA-256 values were
`5c5d9bac1d0a2e283c6c25029a50896bf8e2939175d65701508944370c7a789e` and
`c8be50908d7eb75ebaabc44a2dc04db172c36289038a4dcaa23088f581381cee`.
The earlier follow-up recipe and
result hashes were
`15d3135943928b47698911c270247b08491df4590af2c30ff1b87b662c26cda7` and
`d6abd71b20f98528d1efba677366974368f7ade64288c85db3e5ebdce8b029db`.
The control-capture recipe and diagnostic SHA-256 values were
`fa5d64c9194c049c2a4acccd58720d95d6a92755b2220f30036371d80dccc41d` and
`14979d87a6e758e572c48a11dcc23aa38fc6e0cc101f5f77d124b28c339e8c61`.

In a separate setup-complete run, EFX sends were adjusted on all eight strips,
then LEVEL encoder turns were applied one strip at a time. The gain words at
`0x20016170 + 8 * slot` changed only in slots `0`, `1`, `2`, `3`, `4–5`,
`6–7`, `8–9`, and `10–11`, respectively. This confirms the control-to-gain
slot grouping in that fixture, including both members of each paired strip;
other link states, source-channel mapping and patch combinations remain
unqualified. The recipe and result hashes were
`06df73d2655fca9ffc1d41678671b89181603f89e8e66c81bc84785bdfc5a5c1` and
`adfd29927843223f3bf2a70db60b228e59ee79e10355656a12b57da4e4197c87`.

In a stock setup-complete run, a 1,500 ms EFX SEL hold reached the selection
routine at `0x20280580` in the priority-12 `Main` task before the panel model
had delivered the release event. At that stop, the call stack included
`0x800543a7`, `0x800542f3`, and `0x80036f2d`; static disassembly identifies
the calls at `0x800543a2` and `0x80036f28`. A long-press inspector cannot
simply react after the stock press handler without also changing the effect;
it needs a validated way to defer or suppress that action. This observation
does not establish the release-event ABI, a safe hook, or a gesture threshold.
The recipe, debugger script and log hashes were
`60d97b6ef492d7150862fef2f2bc597859c06b00c71e91784cd5522de163d376`,
`ec66bcb88a8cc3dfcde365ec1f385a5870858ac9fd8048c01b0e8f24aba4bf34`, and
`29fe6e3c5eaab4586e78fb941f83a71dff8909894c093fe40cbaa96a99b895a3`.

In a fresh stock-emulator boot, the descriptor at `0x8020a3dc`, referenced
by the observed effect context, contained `0x80d00000`, `0x80dbb800`, and
`0x000bb800` as its first three words. Static inspection of the Room callback
uses these as a start, end and circular span of 768,000 bytes. This is a
candidate stock-owned effect buffer, not free patch memory: ownership across
stock effect switches, reset, DMA, and other users must be traced before any
reuse. Stock routine `0x80024678` passes the same start and span to a
zero-fill routine, so its callers and synchronization must be controlled
before any reuse. In a separate diagnostic that pressed EFX SEL twice, stops
at the clear routine showed return address `0x20280677` in the stock
selection path, active callback `0`, and current task `Main`. The script did
not explicitly step past the breakpoint between its two stops, so those stops
cannot establish two distinct clear calls. A follow-up observed `Main` at
priority 12 and the two stock flags at `0x80202e68` and `0x8020026c` set to 1;
its repeated stops have the same limitation. The earlier audio
stops were in `AudioProcess` at priority 30. Static inspection shows those
flags alter stock parameter-copy behavior. These observations do not establish
that they are exclusion locks, or that an in-flight callback has completed;
a replacement must account for cross-task ownership explicitly. The follow-up
script and log SHA-256 values were
`df4b7cbfaa239bf9a722bb57a04b8518a873d60e7aafbb0a1c00e2c44d1a9d54` and `65b9ffb3f77fa4aea7c812ca7b607063f9d981b7b611ef2f5697855529785752`. The memory recipe and diagnostic
SHA-256 values were
`4963ea843249cdc15ea055d11877d41f7025c7a7837dc06166ce71fb4e48bfd8` and
`a9dba569c1999acc77d0de45699bf01722ff52784d8e293ee2bb649ebc76c3c6`.
The clear recipe, debugger script and log hashes were
`b7ba14f1be0d5fcd5e54b5c6b8e0a2524faca0d67cb8b909d874acce990574e5`,
`1bd3b52f27acadc0759517c5ba8b0cd76cad8b3399e1bd3bd63105835203edcc`, and
`452d160086fe099db2b9d3a8f6e037cdcd83179d9fe01fa0dd428e937b1bd826`.

A later stock-QEMU run sent six EFX SEL presses. A debugger observed five
distinct entries to `0x80024678`, removing the breakpoint and single-stepping
the first instruction between entries. All five had return address
`0x20280677`, current task `Main`, and a zero active callback. The diagnostic
finished and closed the debugger connection while it was waiting for a sixth
entry; that disconnect does not prove whether another clear occurred. These
stops can perturb scheduling and do not establish which individual press caused
each clear, or that no other actor accesses the buffer. They do establish that
the earlier repeated-stop ambiguity does not explain all observed clear calls.
The recipe, debugger script, debugger log, and diagnostic JSON SHA-256 values
were `0d782d4fea7c3d18371375b35a3d6cb88b1a23f6237fa2ac823a664578cb7003`,
`ba997f62a937970b1eaf9145a936510b624f789de2010b1557a281c0409e882a`,
`e8106f0cfe1829acd93267583ceddfc5636fc47f4512f722146cc4084dfba3dd`, and
`a6e33f47cde3f332dc964c2f9fffb7d7241dfbadaa94d18e570d66250b9e087e`.
The stock MAIN, initial NOR, and isolated QEMU identities are those recorded
above and below; QEMU source provenance remains unattested.

A separate stock-QEMU TCG memory trace watched CPU accesses to
`0x80d00000..0x80dbb800` during boot and six EFX SEL presses. The recorded
callback sequence was `0x20280ab9`, `0x20282221`, `0x8002c041`, `0x20280781`,
`0x20280021`, and `0x20281649`. The trace observed reads and writes in the
region from several code ranges, including `0x20280bdc`, `0x20282caa`,
`0x8002c1fe`, and `0x20280890`, as well as repeated writes at `0x80000714`
during selection. This supports treating the buffer as shared stock effect
storage across the exercised cycle, rather than durable private state for a
replacement while other stock effects run. It does not trace DMA, prove all
callers or lifetimes, or establish that the absence of a CPU access in any
interval means the region is free. Instrumentation can affect scheduling.
The plugin source, plugin binary, recipe, trace and diagnostic JSON SHA-256
values were
`ab84d643febbbce641f29fde615d838188f26bdaeba80342a7ab00500ff51367`,
`37acc3b39f8daaaf9e3626bf651c4566c5d5ebd93d2b7ea3b87e7cd11a6932f3`,
`8314ef439b4a84f8dc5212c7ca8ed14aa65dcfc8d936aadc225a1884cb5b20a4`,
`a6b20fc164b62494c2949f48adb7feb2aab46250147906977e556b0f167341fa`, and
`0f2c62ebc8bb7d2c03216ef5565c882f7b56a9d6e22551208fe23b5b32463a09`.
The stock MAIN and initial NOR were those above. The plugin-enabled QEMU
executable was `69b2a6775a4cab8d778d2d9b91e41cfedbbaf35788ac1417955f0be66a874afa`,
with source provenance unattested.

The same address-range plugin was then used with a disposable diagnostic
image that initialized the replacement in the stock effect buffer, injected
active synthetic sends, and exercised ten inspector bank edits. Between the
first and second custom callback publications, the plugin recorded accesses
from the diagnostic ELF's code range `0x80108000..0x8010d3a8` and from stock
write instruction `0x80000714`; it recorded no other CPU instruction accessing
the range in that interval. The publication interval includes the next stock
selection and its clear, so those clear writes cannot be attributed solely
to time spent in the active callback. The run observed ten alternating bank
states, a closed inspector, and selection of the second custom descriptor;
the diagnostic callback counted 4,472 calls with finite, nonzero sampled
output. This is evidence for investigating effect-lifetime reuse, not a
reservation: the trace is limited to CPU accesses and the exercised sequence,
does not cover DMA or other firmware activity, and cannot establish physical
memory ordering or safe coexistence. Instrumentation can change scheduling.

The plugin source and binary identities are those in the preceding trace. The
diagnostic ELF, MAIN, recipe, trace and result SHA-256 values were
`638e6ddb5b9ec8a65de4ae21b3bc1b96c03a0b9713cf2c720faae22c04614abb`,
`2e9341771ae9e90729a596c617a9495d763c922c5f23d54cde264268235bbdbc`,
`390614f08bd6f074c9f7dfe476576c81570c0ff9557f2dc90a20a119c66ec721`,
`d6d242a7f4cb80de2944e79a97dd61057e99680eb01377cf1fe2620d8e7cab03`, and
`362ef5bb49460dd68360da39e7d37df7f37084d752cbe44b7845687bfb2597e0`.
The stock MAIN, initial NOR and isolated QEMU match the preceding trace.

Static inspection of this MAIN image also identifies the initializer at
`0x80056588` writing `0x80d00000`, `0x80dbb800`, and `0x000bb800` into
the descriptor at `0x8020a3dc`. The clear at `0x80024678` independently
hardcodes the same start and span. Other stock routines return `0x80dbb800`,
`0x80e19400`, `0x81570400`, and `0x82241800` at `0x80016630`,
`0x80016640`, `0x80016650`, and `0x80016660`, respectively; calls to these
routines occur at `0x8000129c`, `0x800012a2`, `0x80001630`, and
`0x8000162a`. The initializer at `0x80016670` also writes a sequence of
addresses starting at `0x80f90400`, spaced by `0x0bc000`, into stock
structures. These static findings show that the stock memory map extends
well beyond the low BSS and heap. They do not establish the full extents or
lifetimes of those regions, all indirect references, the capacity of the
physical SDRAM, or ownership of any high address. In particular, the old
claim that `0x83e00000` is safe merely because it is far above the heap is
unsupported. All addresses here are scoped to this exact MAIN image.

The selection path stores the new callback at `0x202a6f20` at instruction
`0x202806ca`, before a later effect-specific call. In a separate disposable
QEMU run, a debugger stopped the `Main` task immediately after that store,
observing callback `0x20280ab9`. The debugger temporarily replaced the next
instruction at `0x202806cc` with a self-branch, delaying the control task, and
then observed the priority-30 `AudioProcess` task reach the indirect call at
`0x20280248` with that same callback in `r12`. The original instruction was
restored before detaching. Thus the audio task can reach a published callback
while the selection task has not yet continued past publication. Any new
callback's state must be valid before publication; an initializer placed only
in the later effect-specific call would be unsafe. The deliberate delay does
not establish that an unmodified stock switch normally hits this interleaving,
nor does it prove exclusive ownership of the effect buffer.

This probe used input NOR SHA-256
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`
and QEMU executable SHA-256
`9724d60a31400becad8f1e98d6fd67bb9456c8dfbdeebe833e7636ed8e9cf536`
(source provenance unattested). The recipe, debugger script and log hashes
were `d64c6ae7bf7f90d46e064bd56300cb942687173a9658ec17fca91866d1cbb0e8`,
`24de107f2450c262d0768555eecf6a8a4b5403b229b52b00abccfca95b6cd4a4`, and
`65b455afaea0483b8b171793ef428f201783391fb6f7490de5bc92e0081d0778`.
In a second run with the same recipe and input state, an additional breakpoint
at `0x202806ce` guarded the control task's following instruction. The audio
callback breakpoint fired first, confirming that the control task had not
passed that instruction. The second debugger script and log hashed to
`94de01afd3d6ffa3eb2a3c3ae0eaa81a4085a19940c032990e234d99f68ebeda` and
`b46ac798bb7717dad6a8d8524befcb188a17028bd1ee12337c80b0dbe9552ea1`.

The converse interval was tested separately. At the clear call at
`0x20280672`, the priority-12 `Main` task had set the active callback to zero.
The debugger replaced that call's first halfword with a temporary self-branch,
then continued execution. The priority-30 `AudioProcess` task reached the
wrapper's comparison at `0x20280108` with both the loaded callback and active
callback word zero, and next reached the no-callback branch at `0x20280330`
before the indirect call at `0x20280248`. The original call bytes were
restored before detaching. This demonstrates the disabled-callback path for
one forced interleaving; it does not establish all buffer users, reset paths,
other task priorities, memory ordering on physical hardware, or deadline safety.
The input MAIN, NOR, QEMU and recipe identities were those in the previous
probe. This debugger script and log hashed to
`f751358971c7ab6ffa6d59ac0d7ea26535655abe6ffeb3c721880ff8ad2dbd43` and
`6a56b4ea2e9f05b606cc9d57a869ecb9e34c54b6b931a554b44cebc9a235c5c9`.

## Experimental relocated-veneer redirect

The selection code calls the stock clear through a ten-byte veneer at MAIN
file offset `0x107de6`, relocated to `0x20282f76`. A disposable QEMU-only
experiment changed that veneer to load `0x80108001`, where an appended
veneer transferred control to the original clear at `0x80024678`. It did not
run the replacement DSP or make an update package. A debugger observed the
redirected bytes at the relocated RAM address, then stopped at the appended
veneer, the original clear and the stock return site `0x20280676`. Registers
`r0` through `r11`, the stack pointer and the link register matched between
the two entry stops; `r12` changed as expected to hold the next destination.
This establishes a possible far-call interception mechanism for that path in
this QEMU run. It does not establish state ownership, safe initialization,
all callers, or real-hardware timing.

The experimental MAIN SHA-256 was
`ccfd22a151b80662d1e029fc2e8250fb8027206c884dee2f2d0b02589bbacb64`.
The recipe, debugger script and log hashes were
`4ae9171edd8dcee4234362c6f978b1b3e23945bd01a849543ef0b5d410e615c9`,
`fe43855b37ecc00f620ee41d2c0de89532079ee2cdea1ca40b9946ad9043255b`, and
`5bca7006b4bac67c74800638395bc9b20eca07be225274d51760b44741c910a3`.
The modified MAIN and disposable NOR are local diagnostic artifacts only;
they are not qualified or supported firmware.

## Cortex-M7 DSP initialization probe

A separate QEMU-only diagnostic redirected the same stock clear veneer to a
compiled Cortex-M7 probe. After calling the original clear, the probe used the
in-place initializer at `0x80d00000` and processed a 2,048-frame impulse
through Velvet and Plateau. It wrote peak values and a completion marker to
QEMU-only scratch at `0x83e00000`; this scratch address is not validated for
physical hardware. The hash-guarded diagnostic recipe observed peaks
`0.3096633553504944` and `0.018613358959555626`, with marker `0x46585031`.
The equivalent host computation produced the same float bit patterns. The
probe overwrites the stock effect buffer and is not an installable patch.

The reproducible diagnostic source is in
[`tools/l6dsp-tests/probe`](../../tools/l6dsp-tests/probe/README.md). For this
run the input NOR SHA-256 was
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`;
the probe ELF, diagnostic MAIN and result JSON SHA-256 values were
`8274f2df94fd00f51ec3149fa3f58c067689c4d19fbef4c1cb8f469a1b35e3c2`,
`ae8e65af952bcc9f78e196c10d29f300778f3d25f935346eb421247795be5365`, and
`c07ba358807f20f33f1ffbb848dbf6e3ac9535ad1a2d951e36daf066e6938cf6`.
This checks target-code initialization and DSP arithmetic in QEMU; it does
not test live audio routing, effect-switch synchronization or hardware timing.

After the corrupt-input and delay-ring fixes, the same diagnostic again
reported peaks `0.3096633553504944` and `0.018613358959555626` and marker
`0x46585031`. This check used input NOR SHA-256
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
probe ELF `84eff0b48d906384d5e14d985841eccb50fef21e988989576f003105932c9654`,
diagnostic MAIN `ecf1e3300efcbefe0893b1afc3385c7ddb08513b78a484b43470373fb4733191`,
recipe `063f62f688a72eda5d689e6a4f4b32d77505bef68703b808de48cf02b25a3f94`,
and result `5afce2d0f89f5471d8d6fff9a638084d00bb5c6b67cdaebb63257ac40abdaf76`.
The QEMU executable was the one identified in the publication probe above.

## Experimental audio-callback round trip

A QEMU-only payload provided a temporary two-effect callback with the observed
stock `(context, descriptor)` ABI. In an existing disposable run, a debugger
stopped at `0x20280248`, invoked the diagnostic initializer, restored the
saved argument registers, and substituted the callback address in `r12` for
each of 256 calls. It injected a `0.5` impulse in both 64-frame scratch
channels on the first call and zeroes thereafter. At `0x2028024a`, all 256
blocks were finite and nonzero, with a peak of `0.3052642345428467`. All
32,768 returned sample values matched a host reference bit-for-bit; both
outputs had SHA-256
`5b410787521ba66713ac39a9b01069cb8636bbcab9d793786eddb99a10aa6125`.

This experiment explicitly redirected every call with a debugger and ran the
initializer on the stopped audio task. It did not install a callback in the
firmware, test per-channel routing, timing, controls or stock-effect
coexistence. The same emulator run included two earlier interrupted debugger
attempts; the successful attempt reinitialized the DSP before measurement.
The experimental MAIN, probe ELF, recipe, debugger script and log hashes were
`21e625b0a029327e4de63731890980eea8fbef18debf44793df2522f9223cc01`,
`68538c82f07498adf1ff0ea00fade27c5470a1fa181a58ee70d828968793bb20`,
`9d9b3c61c4f1ff29b23a3422962f4b1d5f0fb9af8d16620c9f0274bb3a508d0f`,
`9d40a8627c2307626e1629b57c67a7e9ff663e08025587ff59534ae6f5a88c74`, and
`f6e427dbf7fd2b6764b77b6864bed80b66eef30c06f9210914126552e451ef96`.
The modified firmware and debugger artifacts remain local.

## Experimental automatic callback installation

A follow-up QEMU-only payload redirected the stock effect-buffer clear
veneer. For descriptor entries 1 and 2, it called the original clear,
initialized the replacement state in the stock effect buffer, disabled the
copied stock post-selection callback, and changed the selected descriptor's
audio callback before the stock selector published it. After one EFX SEL
press, diagnostic telemetry counted 595 callback invocations before a debugger
connected. The debugger then injected a `0.5` impulse into the stock scratch
buffers and observed 64 consecutive callback returns without substituting the
callback pointer. All output blocks were finite and nonzero; the peak was
`0.3052642345428467`. This run used input NOR SHA-256
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
diagnostic MAIN `407561e81f59d1406aaf951347e68dec14205b1522168ee09bb126d272e78ed4`,
probe ELF `24ed3b539a3aa3f16337e900c1013369294c9136ecc469eb21a41112d12f4dd1`,
recipe `d2cb842dabc50ea3eecfbdc4c18ca9367b93d7a1e68c6c6ce9ea8f314f96c559`,
debugger script `33c3a8d965e9ba068106006c31c454633c48b44996742a74ce34d0e6bcea60db`,
log `3772cf5effc3a30daabf3eebdff6a290afea1f00112d92f69ed841b465868292`,
and returned samples `07ecbc9638d93ae6523b692476a8b4ecc2fb9ce132ab06317d2567c71abd1472`.
The QEMU executable matched the publication probe above.

The reproducible [`--callback` diagnostic](../../tools/l6dsp-tests/probe/README.md)
performed a separate run without a debugger driving the callback: it observed
marker `0x494e4954`, descriptor `0x2028b8dc`, and 586 callback invocations.
Its probe ELF, diagnostic MAIN, recipe and result hashes were
`fceda501bf4f422a85cca0b2e6daecbd9dbdbf3f78955e31ce5aa2f036cdd2a4`,
`78cf75caf8a3a70a489bf3ee9e28d7a957b92707828091e303986cd79ef72c19`,
`2bd9ec5f927f9a7d33988e88c2b45cdf15d7fff900bee7d1010cbfb8bceb077f`, and
`89632015ea159c558a67bd4afbf82cfa6d5810f75e78fe170108e744130b0ffe`.
The diagnostic uses QEMU-only scratch, overwrites stock effect storage, and
processes an already-summed bus. It does not prove routing, controls,
effect-switch ownership, real-time deadlines, or physical hardware safety.

A six-press selector diagnostic using that same QEMU-only image exposed a
failure: after the first press selected the diagnostic callback, five further
EFX SEL press/release events were delivered by the panel model, but each
subsequent observation still showed callback `0x80108ad5` and descriptor
`0x2028b8dc`, while the callback counter continued to rise. The test does not
yet establish whether the cause is the replacement callback's execution cost,
the descriptor/initialization changes, or another interaction. It must not be
treated as evidence of functioning effect switching. The recipe and result
hashes were
`603d61abb437ce1cb57c46be6e6c6a19243dae868d944a43833e93763c44fea4` and
`2803902a121079e5b3e593dadae2edc6624eb7abda72ca8f4aa548600561ee03`.

Two controls used the same six-press timing and input NOR. Stock MAIN advanced
through callbacks `0x20280ab9`, `0x20282221`, `0x8002c041`, `0x20280781`,
`0x20280021`, and `0x20281649`. A separate experimental image kept the same
initialization and descriptor changes but replaced DSP processing with a
counter-only callback. It too remained on the first custom descriptor after
all six presses. Thus the DSP workload is not necessary to reproduce the
failure in this emulator experiment; the exact cause remains unresolved.
The stock control recipe and result SHA-256 values were
`35f1f08e30d4f8c2715cc51cc1d91b6df02d8147a8335db67826e6733ac971fb` and
`6a26aa15c6e45932e3ce9675db1187789e80a4426c68591ef000a3520f0f9f22`.
The counter-only source, ELF, diagnostic MAIN, recipe and result hashes were
`ee0b33813f68f0c28849e8bce9e39a43e6278633495e30902542a7ebcb0bd52d`,
`9c944a5903bf1c5f8b06895361c1a9de19122194273093a59a14737518029190`,
`27cd480af24bae69bbfddedabde4d0feb6d9e8000319cbb54d4cd419b4164148`,
`081761346653cb3b660b43e205cc54b5d7bbf106cb9b0725108d8929a13da37c`, and
`273be1c643da9e5679dde34c6718afddc0b9850f749979e2f6e22fc3c58f7258`.

Two more counter-only experiments also remained on the first selection. One
preserved the copied stock post-selection callback while still initializing
the custom state; the other omitted custom-state initialization as well and
changed only the selected descriptor callback after the redirected clear.
The former experiment's source,
ELF, MAIN, recipe and result SHA-256 values were
`d51bfc2c869d34832bce573a426e26ca4d884afaf406830c64603e8b1bd2cfb5`,
`3ae430b269a8a67759b2ebc1671a3ea14443c91fb025d9cf6b47929c662d6d0b`,
`371bdb1b74acb52f6fe05da1d6c700fd0f519b7200f44100fa96a073afad7ef1`,
`1eae0051abf8f3b693e85ba698de279d36dce25a9b9cca9ff15fee37b2bb1a8b`, and
`ba8cdab146da40226771579cb0bbdfe9d56da4247485b26dbb38527433965ae6`.
The latter experiment's corresponding values were
`b7fe2c5abb585317655fe1dd261fd6853a6ce071d6b1675770526b619930ace8`,
`cafe945aa2abf2a91bdf0e58e727d7c88a8842055e106ab7ccadea558c0843cf`,
`71cead3da298671d078168794ec62928ce58b550a9549b5d19796778b1165957`,
`a30ceec6251cb0ea108d2880e5399da0f8ef7b437e6d23659b1a95888ab8a6f9`, and
`82168428a466b9c56de15a52d00689df5551b0679ad7733bad52b3af36509ff6`.

A pass-through payload using the same redirect but only calling the stock
clear preserved the stock six-callback selection sequence. The redirect by
itself therefore did not reproduce the failure in this run; the descriptor
change or custom callback behavior remains implicated. The pass-through
source, ELF, MAIN, recipe and result hashes were
`5fb463bd3208ca22c037d83cc45d62716320f3c26e06f9c91af079a4dd67113f`,
`1932865c93ad8d79b53786adad7a3d9ad90ed393b4cb7cb8a2e27aa4261c8cbe`,
`fa1f07e15fd9f33b70ea6da3abde488258994abfa1d950995e112eeae5f97414`,
`2951cc15684c7d53c4e8881cc01595081334bd8b2b21ffaed0eb69f86f13a905`, and
`c7f1be6ee9b3b4cbc35c194ee18a722fa26a71b19179ae6c271e16edf20265b0`.

Another counter-only variant saved the selected descriptor's original
callback and wrote it back from each custom callback invocation. The custom
callback count rose, but the active callback and telemetry descriptor still
did not advance after later presses. This run did not independently capture
the descriptor table after the write, so it does not by itself prove the
table's final contents. The source, ELF, MAIN, recipe and result hashes were
`3f41149d99076b549d74dec07c50f4804c3de7403c9538426ae9253f75fa4b89`,
`27f697fa738b92941698ce3a437f1e92ad9fd2bc6fbcee4ae07ff6f3853079cd`,
`e538468ef354ff04dc72175b10956980afeaacc53b59d5e53fb5dacac82d0375`,
`881a4bb3ac40df6f6f629f9729623db0f197a31aef436e1aa979351646fa1474`, and
`e70130ddae6cc53da6a5a51589b26716c413eb666fd0d6a4b47bbce805e0d461`.

An address-preserving variant redirected the stock Room callback's first
instructions to a counter-only stub without changing the descriptor table.
The active pointer remained the stock `0x20280ab9`, yet it too failed to
advance after later presses. In a separate counter-only run, a breakpoint at
the stock selection entry `0x20280580` did not fire during a 35-second wall
observation that included a second EFX SEL press; when interrupted, the
current task was `AudioEfxQFunc`, priority 26. This does not identify the
cause, but makes callback-address identity alone an insufficient explanation.
The address-preserving source, ELF, MAIN, recipe and result hashes were
`dff749fcd3b535d15cdf15a669ece243371070a0f0c050755ce05b8f0e7f23f7`,
`b366b6f6822c3ac8e7e8d5937d816d85caf66703e686ab16278f441874cd2254`,
`1651917feed8a401d6c2e5797c71443f07afec8d9756fb49082aa53eb78b96d4`,
`347391ba3694af35e18abd974f4f58eb36743199a3b1483a16650a8017a34a06`, and
`e23bb89bfcf98398802c9723b0bb3737bc7e04d9d7716e6ed9f56573b2504220`.
The selection-trace recipe, script and log hashes were
`4a581dcaf8aa8cd47d6fa8632acc2eeed04424c8ad5e6bd4fdf195df84e69600`,
`95fa0b243ee14eca9626d2d0fed6ee4d80cd34d50b2a831e10be4b12cdb727aa`, and
`325fceb9d3dc38709ca803d15c72f58169d337d6202c0b853d0451e0c736a0f8`.

Static inspection of the stock Room callback shows a 64-element stream read
through the descriptor's word at offset `0x20` and copied one element per
frame to the destination indirectly referenced by the word at offset `0x1c`.
At one settled stock callback, those words were `0x80463010` and `0x8020a3ec`,
which pointed to destination `0x805c5960`; the first and last stream values
and the destination all held `0x3f2d2857` before and after the callback.
This observation establishes the addresses and settled value, not the stream's
full semantics or its role during a switch. The stock recipe, debugger script
and log hashes were
`6e6c0652b2f0e21eec8c976c6dbc122d1fbb3593ce200140ffaf844a361b0056`,
`cae714e747a8385585ddf1d7b96bc1b7a1601fb1b7e537520a3fabfce4d2c4e3`, and
`d6d2f8f99b7b515d757a34675652c51a5284b819e01c516bdaf01874d1bbee39`.

With the address-preserving counter stub active, a separate two-press capture
found destination `0x805c5960` stuck at `0.8528857827186584`; the control
stream ranged from `0.0018903589807450771` to `0.12098302692174911` and
was identical at both observations. A further address-preserving stub copied
the stream's last element to the destination once per callback. With that
change, all six EFX SEL presses advanced through the same callback sequence
as stock in this QEMU fixture. This demonstrates a necessary callback side
effect for that test, but does not establish click-free audio or hardware
timing. The stuck-stream recipe and result hashes were
`ed343ef00a995143cc3f8d0dd7676a77ea775a682b284ffc4f352d3d3642c6d7` and
`0913e5c6de1951534caff14ad86895ac2ca5bfa7e7ccc80b43a339dbab531c59`.
The last-element-copy source, ELF, MAIN, recipe and result hashes were
`1640606456edb80662c1bffe0125bc632f3c153680809a07d33eea8692b89e1e`,
`dade384975992ce728d64f14690c60ae4da0dc294dc1b1d538f63d15d105bc61`,
`b0a0a1b0dc1c66892e7a2e9f86f32759c001fc6b8c989f450ec97d6841448860`,
`4cfe44e82e479dea70e17a413da207118c4a0a8066443fa2f60f6a7787ef681d`, and
`d3add41f965a11df827a737775045bd21cd8087d476a76c39fe83263d388304b`.

The full QEMU-only dual-DSP diagnostic was then changed to copy each stream
element to the destination at its corresponding frame. In a six-press run,
the two custom selections reported descriptors `0x2028b8dc` and `0x2028b8f8`
with 585 and 586 callback invocations respectively; the following four active
pointers matched the stock sequence. The stock MAIN and NOR matched the
identities above. The diagnostic ELF, modified MAIN, recipe and result hashes
were `e155959d3a96a597eba5564a238e5c604977c47d07650aacb262ba06d0ad103c`,
`77be03307e4887a95957c140d8c958df8f2aec0494cefdf385e2a84e646c48e7`,
`98a5ecbf78d8d946bbbf03aee3c23a568f795cd2c392fbf9aa2a9af42c6b7819`, and
`330f77eb0299e4a981f05ae7a4bcc040e1e91c712fbc011fba00a096e5ff3573`.
This establishes the observed selection progression, not stock-effect audio
fidelity, per-channel routing, UI behavior, or safe physical timing.
The reusable `--callback --switches` recipe independently produced the same
selection sequence, with 586 and 583 custom callback invocations. Its recipe
and result hashes were
`d02bbce4f5a00519b4f3e5a69673c2ee225d38682a853578c6b6267afa9571c2` and
`4c6575bacbcbb610a6b19761b021f00afafebd1359aa16014b456ef7221e66cb`;
the firmware and executable identities were the same as the preceding run.
After the cutoff-state and display-format changes, another run produced the
same six-selection sequence and counted 586 and 585 custom callbacks. Its
probe ELF, modified MAIN, recipe and result hashes were
`00a5dc45bfd9a8b42846bbf839883ed3e503514cd80eb6ec85b681027dc37104`,
`6d8314a1c9165c2d255372bba711144cb4c6672dea6c0b6949a0ec05dcf5b685`,
`47982e4644228f05e61b616a76a97ad83ed3572c586e58d96ec7b22fb88a8ad3`, and
`d964a77169f510822c81892cf0610465c5886da11f5b97956d7a4985795a9f79`.

## Task and memory observations

The Velvet bank builder now spreads generation and normalization across audio
frames rather than generating a whole bank at transition completion. An edited
bank remains inactive until both channels are complete; the maximum 64-tap
bank takes 128 frames to prepare. This adds state to the DSP to bound the
construction work per frame. On the host, a 30,000-frame dynamic sequence
without parameter edits matched the preceding implementation bit-for-bit;
both output byte streams hashed to
`7c23a4d8d3e598a7c3034c13b5c9944746a32375e77136185b74dfea0e959584`.
Host tests also cover a replacement edit while a bank is being prepared and
check that no more than one tap per channel changes in each preparation step.
These checks do not establish the maximum execution time of a frame or the
combined effects on the physical processor.

The QEMU-only callback diagnostic rebuilt with this change retained the
expected six-selection progression: the first two selections called the
diagnostic callback 585 and 586 times, followed by stock callback pointers
`0x8002c041`, `0x20280781`, `0x20280021`, and `0x20281649`. The input MAIN,
NOR and QEMU identities are those recorded below. The diagnostic ELF, MAIN,
recipe and result hashes were
`8171c0c1c2a52427c94e8bfc7b4dbe45fbf482938752baa609edc0dc9cbaff3c`,
`9fbfd787eca4ae599cf068982200b94728e64ffe49d19eb3ae53bb7e6ad0fef4`,
`23848ff05a147e23a328be23cd0abdc3bba1c991a7251f35af7000cb7060e15c`, and
`4bc3f1dac05f1cba215a3c5bf046ed4959ee077c17e1e37d4ff46fdc011fda8f`.
This run did not test live parameter edits or qualify hardware timing.

In a separate disposable stock-emulator run after 45 guest seconds, debugger
stops at both `0x20228b08` (the candidate accumulation loop) and
`0x20280248` (the effect callback) reported the same current task,
`AudioProcess`, priority 30, with TCB `0x808d50d0`. The observed stack bounds
were `0x808d40c0` to `0x808d50b8`; stack pointers were `0x808d5008` and
`0x808d5058` respectively. The interrupt controller's active-exception field
was zero at both stops. These are two observations of one run, not proof that
all calls use that context or that available stack meets the replacement's
worst case.

After reducing temporary routing arrays, a standalone Cortex-M7 `opt-level=z`
assembly build showed the routing function's own local stack allocation drop
from 2,864 to 1,112 bytes, with the same register-save prologue. A host
reference run spanning mixed routes, parameter changes and multi-block inputs
produced identical output bytes before and after; both output files had
SHA-256 `3137e951f31c6ee7882995fa38605f95a1ad70a373597cf596ef7298c6c7d935`.
These are function-local figures and a host comparison: neither includes all
callers, nested calls, exceptions or the actual deployed compiler context, so
full stack and timing qualification remains necessary.

After introducing weighted-channel routing, a standalone `opt-level=z`
Cortex-M7 assembly build allocated 252 bytes locally in `process_block`, 844
in its `process_weighted_block` callee, and 304 in `process_buses`, plus their
register saves. Together those three frames occupy 1,576 bytes on this build,
about 296 bytes more than the immediately preceding two-frame version. This
excludes deeper calls, interrupts and firmware callers; the assembly SHA-256
was `af10eb4b32b66040c6f3d1a7932c20239985e2130c9cab7b7350666b3202b773`.
The diagnostic callback need not use the `process_block` adapter, but its
actual complete stack and timing remain unqualified.

The stock allocator at `0x80085478` maintains a free-list head and free-byte
count at `0x8020ab2c` and `0x8020ab1c`. A debugger traversal in the same
settled run observed a single free block of 160,008 bytes; the summed blocks
matched the allocator's reported free count. The then-current payload ELF had
319,216 bytes in its `.state` section. Thus this observed heap could not hold
that state as one allocation, even before allowing for other runtime needs.
This is a snapshot, not a guaranteed heap bound or evidence that another RAM
region is owned by the patch. It rules out assuming that a simple allocation
from this observed heap is sufficient.

Static inspection of the same MAIN image gives the pool's initial extent.
On first use, `0x80085494..0x800854cc` initializes the aligned pool starting
at `0x808a4148`, with `0x7d000` (512,000) bytes, ending at `0x80921148`.
An eight-byte terminal header leaves 511,992 bytes in the initial free block.
The observed free-list terminal pointer, `0x80921140`, agrees with this
calculation. In that settled snapshot, 351,984 bytes of the initial free block
were already unavailable; adding the then-current 319,216-byte state would
exceed the initial free block by 159,208 bytes, even before any allocation
overhead for the new state. Reserving that state earlier from this same pool
therefore cannot preserve the observed stock allocation footprint. This does
not rule out a smaller state, a different allocator, or another owned region,
and does not establish behavior in other device configurations.

For perspective on reducing that deficit, the thirteen Plateau delay arrays
currently hold 59,408 `f32` samples. A preliminary capacity calculation for
the existing maximum `size` of 1.2, 190 ms predelay, 16-sample modulation,
and output taps gives candidate array capacities, in source order, of 9,121;
274, 209, 735, 538; 1,332, 8,620, 3,704, 7,201; and 1,790, 8,163,
5,142, 6,123 samples. These total 52,952 samples, saving 25,824 bytes in raw
arrays. Even if that reduction carried over exactly to the payload, the
preceding snapshot would still exceed the stock pool by 133,384 bytes before
allocation overhead. These capacities are an unimplemented estimate, not a
verified bit-identical replacement: history during size and modulation edits,
interpolated reads, and output taps would need comparison across their full
ranges. They do not establish ownership or make that heap sufficient.

The heap recipe, traversal script and log SHA-256 values were
`a23dd43d67d045af4d2bf8492c9bb9d80f5cf4be880d31a10a0b7136cdfcae48`,
`4eec680ad84db584bf74a4a01513780a0f6b0c8a9c772f0ec9bdbb3f918af601`, and
`3c932a961a13c206cf2a2d056ebe7de5001bbdff2e5d205f4492dc792a899596`.
The task probe and log hashed to
`666bbb53107c421c2426f408f948b30335653753802b6d42b34109c1b6f6de65` and
`0f7d518d11d289d55d2fc224da8b88e7cd16681c9b207b4a38022d821047ed1f`.
The compiled payload ELF SHA-256 was
`be1fd26b5212d2731a5c3a8169ad2b7998f0d9779836722723e415a953def6c6`;
this was a build check, not an installed or tested image.

## EFX SEL press and release diagnostic

On the prepared L6max 1.10 fixture, a stepped QEMU debugger trace observed the
call at `0x80036f28` reach `0x800542a8` with `r2=1` on press and `r2=0` on
release. The candidate tick counter at `0x8020ab4c` advanced from 47497 to
49001 during a 1500 ms simulated hold. The breakpoint was removed and the
instruction stepped before waiting for release. This establishes neither the
counter's rate nor event delivery in every context on hardware.

A separate QEMU-only image intercepted this call and used the gesture state
machine with a 750-tick threshold. Short (150 ms), long (1500 ms), then short
presses produced stock active callbacks `0x20280ab9`, `0x20280ab9`, and
`0x20282221`; diagnostic short/long counters were (1,0), (1,1), and (2,1).
The long action only incremented a marker in experimental scratch memory: no
inspector was opened. This image and its scratch address are not qualified for
hardware, and the production firmware hook remains unconnected.

The press/release recipe, script and log SHA-256 values were respectively
`a75693bdd6f2bf783d5cbb165d171f8f12943daf6bebb5c45cb4821bae1130b4`,
`2ddbe46e51b2b809515975d66f9ccec620970f5c0f1921b8da3462e74831c4ce`, and
`a0409318ec9070298be395584fc47fabbc04bdc5c19d459fca18189dbb16a508`.
The gesture probe source, recipe, diagnostic JSON and modified MAIN hashes were
`ac18638951d17211ff441812a6112750b783c0a4ac6a34c3df9c77d2e491ef76`,
`b53a2218c190b351266e91c39ff3267ca46c2de9f4f41c5f3c0a6033e6f9c9be`,
`8d606168f384ff6f7ebbc843c586abdf11e1437ecc87219ca6807cb4c63e861a`, and
`ff248b32e3c873c6f1a585ffc47025b01d89bde69539cf35d82cec76583e0b26`.
Both used the QEMU binary and stock package identified below and a prepared
NOR image SHA-256 `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`.

The reusable QEMU-only inspector experiment combines the deferred selector
with the routed callback and a bounded input-event queue. In one prepared
run, a normal EFX encoder turn changed the stock send from 0 to 44. A short
selection entered Room; a long hold opened the diagnostic inspector without
changing the active callback. Eight encoder events changed the captured
Plateau predelay value to `26.0 ms` while the stock send remained 44. A short
press selected Spring and closed the inspector; another long hold and eight
opposite turns changed the captured Velvet amount to `84.0 %`, again leaving
the send at 44. Closing the inspector and turning the encoder changed the
stock send to 86. Both captured queues had head and tail 8 with zero overflow.
The result contains text captured in scratch memory, not an OLED display.
The test does not qualify queue overflow handling, loss across selection,
other windows, patch combinations, ownership or physical timing.

The inspector source, callback source, preparer, diagnostic ELF, MAIN,
recipe and result SHA-256 values were
`d9d763c9949cfa48a84b870fc782d5649ff070400bd1226198b98fdfbd984a94`,
`af9cbd1cef4deaf363486c95ad3071b18bdd47b7bd837e6aa9cf6b847b976704`,
`dc305e506aa65a00ac31a92e1c5b2cf10e12b54541c28bd2d7628b1416ef0cea`,
`6d7bb483050da3bcab535b9741b0c60e3973c4a6883f75ea9144d47c8921c103`,
`52603866eb7dcaeb9fe39e2483f358c9a0a1151734a88d3f10f79707a9611ad0`,
`64e337f84af46e69e74a104f84ee10cce392e2091cea10d276c69673fc30230b`, and
`e9f7bb9cbba0123f72ec7e42865f2cebce0f17add014c08d4a8ecffcf9625f62`.
The stock MAIN, prepared NOR and QEMU executable matched the identities
recorded elsewhere in this document.

After separating copied parameter values from their formatter, a repeat of
the same two-bank diagnostic produced the same captured values and mixer
behavior. The diagnostic still formats on the audio task; this repeat does
not test a display-task handoff or OLED rendering. Its ELF, MAIN, recipe and
result SHA-256 values were
`ba19164b020ef4b86748ec6b4161abbf1ced15f78f049964b42d6c3ab0abebda`,
`732e39fb3541e7bfec5c49a3c1f7c4d1c4790934df87b9845d4883e15ceef846`,
`0ea425a25aa674ddcc05121f8726bc9c9d16d6aae8475401f14b0ce20517bcf4`, and
`6eb5a7d5b9fbbc1dd57865cd4a898285d279a6200bd7d62b2b0658317e5408db`.

A separate QEMU-only combined image used a temporary knob-dialog source
overlay and the existing notification manager to render copied inspector
values on the display task. After the Room and Spring turns, debugger reads
of the live popup's text buffers showed `PLATEAU PREDELAY` / `26.0 ms` and
`VND AMOUNT` / `84.0 %`, respectively. The captured OLED frames differed;
the normal mixer send changed before and after the inspector and stayed at
44 during both edits. Both captured event queues were empty after eight
events, with zero overflow. The debugger perturbs scheduling. Popup
contention, other windows, UI coverage, safe memory ownership and target
timing remain unqualified; the diagnostic must not be installed.

The popup ELF, effect ELF, combined MAIN, recipe, result, debugger script and
debugger log hashes for this run were
`9b6bc70f6425adc0f11942347fb105df9d144005ad6a3314d00d860b01c61bc2`,
`39855b5f3479339cfe814b4e28eb341512883933b62aef3cf8441c37a5c1a920`,
`7ddf7b722f9b562cf8ca855ba66748a9416872b20633cb9c49995c43c00cddb1`,
`57104237746ec8cea59cb900e84237f200d428d12835e6495182e144a2f5f14d`,
`97011a8c4893230ff60a97703a4bdd29cb88177b7200bc9d53ab5e243165b0ef`,
`c6967ec3cea4c4e0995d3a71c8dfc342b91fb37d1f36adcd939667caa6e70a21`, and
`5f143c7261294ce83d798a4dfce7e92ad57393582fef479fb86bfa08734620ab`.
The two captured ASCII frame hashes were
`787ff473ad09e6eb70cc53119a514d04ee85bc37f998b0ac649d517ae4f23533` and
`3bbf8502508ada2b92091c932c2bb81b4a4cf2117035ac9aaad420ec8e569b79`.
The input MAIN, NOR and QEMU executable matched the identities above. A
separate build from the same inputs produced the identical combined MAIN.
The final guarded preparer, overlay patch, inspector source, fixture helper
and branch helper hashes for that rebuild were
`945b2edb3533cac5ef73c7a79ed319dfd77f2e875c0561bb7c323c1ea05df8d4`,
`b570eb6a8333a63949e08d31526a65fb98c46e036ae336f8019f9ed55cb9dbbc`,
`e14bdcbbacfbbb1057c2528f2a98d08cedf7a492e9c244fd809a597b02e850b1`,
`3e9871574a6b5da8c383b2e1a2a857db02e38e45e0fb263a36d5e1636638a3ef`, and
`c114b75edbb390178b1bbba1024798bd5f8d13af2cf1e2315c2d9c77821cbcc1`.

## AI Noise Reduction investigation

The [L6max operation manual](https://zoomcorp.com/manuals/l6max-en/), under
“Using AI Noise Reduction,” says that the stock feature affects only the
MASTER L/R outputs. It cannot be adjusted per channel or with EFX RTN. The TAP
button starts a three-second environmental-noise analysis; pressing it again
disables the feature and resets that analysis. Consequently the desired
pre-effect use is a new routing and control requirement, not a documented
per-channel stock send. A viable implementation must account for learning,
state, and placement in the master path as well as prove the audio interface.

The historical plan calls `0x80002666` a pre-AINR audio hook. In MAIN 1.10
that address is a `bl` to the effect-type getter at `0x8000b270`, inside a
larger routine; the following instructions compare its result with 5 and
branch to `0x800027ec`. It is not an established noise-reduction API.
An independent stock routine at `0x80005520` makes the same comparison and
branches around its following per-channel effect-setting reads for type 5.
These are static observations, not identification of the denoising algorithm.

In a stock QEMU run beginning at Hall, five short EFX SEL presses selected
callback `0x20280021`, consistent with the documented AI Noise Reduction
position. Static disassembly of its code at `0x20280020` shows it copies the
64-word control stream from descriptor offset `0x20` through the indirect
destination at offset `0x1c` and clears both 64-word effect scratch channels
at descriptor offset `0x04`. The descriptor captured after selection used
`0x8020a3ec` and `0x80463010` for those two control fields, the same addresses
observed for Room above. A second run captured `0x805c5960` in the first word
at `0x8020a3ec`, the same indirect destination observed for Room. This
callback does not expose an in-place denoised
effect return. It does not establish where noise reduction is performed,
whether it can run alongside other effects, or whether its output can be
chained; those questions remain open.

The AI Noise Reduction selection recipe and result hashes were
`d40c52dfa8f1e23f2621fde7ea1b97285cfb9bbd8f4ba173ebab5d0108a7f051` and
`0589396e78896123952515283a391d0f366305f60b155b5610ab1f34e46266cc`.
The follow-up recipe and result hashes were
`debd9f63afc1bd8aa479bf739e3656c83de53634e2a2050d217a1351e0eb72a5` and
`c5c3b6771953a77caeae3fe0a569c24fdb63f970eed86cbbd174052a70bf1328`.
The stock MAIN, prepared NOR and QEMU identities are those below and above.

Static inspection gives another candidate path. When changing to effect type
5, the selector at `0x80054340` calls through `0x80003f98` to `0x80014830`
with argument 1. That routine writes 1 to both `0x2028ba4c` and `0x2028bad0`;
the same selector writes 0 there when leaving type 5. It also calls through
`0x80003bb0` to `0x80010b50`, which changes several stock audio structures.
This is evidence of selection-dependent processing state, not yet an audio
ABI or proof that either word enables the denoiser itself.

In a separate stock-QEMU run, five SEL presses selected callback `0x20280021`
and both candidate words read 1. Two target-48 TAP inputs were acknowledged,
separated by 4.5 guest seconds; the stock word at `0x8046300c` read 0 before
and after each input. The snapshots therefore did not establish successful
learning or a usable denoised output. A guessed relocated code address also
read zero and must not be used as a hook. The recipe, result JSON, and engine
trace SHA-256 values were
`72d59d4a065e9e1537e56198c8c7473fb77cc0333639f9b9c7470d45530a83b9`,
`2ed7c00ec344c3a753a0ace3d62d39fffc993411a380062c51a8939b16ca272c`, and
`f4d34cd1f829847a7d5b64d3b6ff586610a057268df8928e59aa976c2024b216`.
The exact stock MAIN, NOR and QEMU executable are identified above and below.

A follow-up sampled the eight bytes at `0x8020aa3c` 500 guest ms after the
first TAP press and again 4 seconds later; both samples were all zero, as was
`0x8046300c` after the wait. Static inspection shows a type-5 path at
`0x80055568` setting the first byte at `0x8020aa3c` while starting a timer;
the observations do not establish that this path was entered. The follow-up
recipe and JSON hashes were
`169572dfa4b7700cacc5e56ca3ea2bbf5f279afb01cb78e3d21a62a3e54eba42` and
`74800365addaf5d6c85d891fefa0e261dc856f741d051a745c50fb558049ff4e`.
A separate attempt to break at `0x800555d8` installed a breakpoint but the
guest exited and disconnected the debugger before any stop was recorded.
Without a positive breakpoint control, this does not prove that the button
handler was absent. Its recipe, script, debugger log and JSON hashes were
`5d129c2c74c6baffd20200ec2f0b8612dd357999305ece139aa507760fe28a92`,
`d8034ae82262d7e434232f38cf0c68bb5653a836937689650923a502b37706b6`,
`1f0fb5b1ece3fb885c330bc3eb503c20bd25015c47398dc64b49fd3c0f2e15f1`, and
`221adb687767e68781f9c6e6555888fdce960e586aa4a12934e5a0cd02991db9`.

The earlier QEMU mapped L6max TAP to GPIO2.22, but a traced press found no
guest test of bit 22. The L6max stock key table at `0x800c42ac` instead maps
event `0x23` to reader `0x80021db0`, which reads GPIO2.31; the event dispatcher
labels that event `EV_BTN_TAP`. After correcting the model, a fresh diagnostic
observed the guest testing bit 31 and the learning-in-progress byte changing
from 0 to 1. It remained 1 after 4.5 guest seconds, so completion and denoised
audio are still unverified. The diagnostic JSON and trace SHA-256 values were
`44b6e8c8f9e5122a06a55be2d84309b771030ec8a10feec26291773186ca01c0` and
`ec1210a75a3436c98f1f8da0ceacffd28c62ba156f228cd7d0d03044d86168f8`.

The `l6max-ainr-tap` driver case passed with the corrected QEMU, stock 1.10
MAIN and the prepared NOR above. The scenario, result, and inputs hashes were
`33ca2a6459ff424ddf00e6ab04b33396b563db7c7c54e7c5b85cb34b37e2d083`,
`ae38d85d64561becc8376df136d5068b67af11bb0fe46ee98e58a0258dcf0a56`, and
`77d9e5f2e695855620d58c46fda18a1ee766c9d3cc16ced435ce15f860e12ba0`.
The tested QEMU executable SHA-256 was
`e118041bd66ee13130998fc24810bbcb268f1fd1d25ba22328cf4f1c7f96211a`;
the driver marks its source provenance unattested. Two earlier driver runs
with shorter startup and inter-selection waits failed to observe the flag,
including one with a longer TAP hold. Their timing sensitivity remains
unresolved; the passing run does not establish a general readiness deadline or
physical wiring.

In another stock diagnostic using the corrected TAP mapping, the two words at
`0x2028ba4c` and `0x2028bad0` changed from 1 to 3 during learning. After a
further 10 guest seconds the progress byte and timer had cleared and
`0x8046300c` was 1; a subsequent TAP returned that word to 0 and the two
processor words to 1. Static inspection of the polling routine at
`0x80088d70` shows a fallback after its counter reaches 9,900 while either
processor reports state 3. This silent-input observation does not establish
that real noise was learned or that audio was denoised. The recipe, result and
engine-trace hashes were
`5efbd59d9dc6a3df732c6fde085bb40f1c899051730cf1ab9a1e4362fe00fd5b`,
`d41a50d7305fb15f60ebfcd9c9a566fbfa42ba1cdf77afc2b7ce1e91a06a9e24`, and
`9bccbbf20a132d62574d91d5da3c3bdce9b5808584860078eeb917bdf6df7d61`.

The driver case is intermittent even with the longer waits above: a repeat
failed at the learning-progress assertion. A further repeat with a 150 ms
TAP hold also failed there, despite the trace showing the requested 150 ms
input. Its result hash was
`b8b03bc56bf9a7387f1a24574049b694372c05a86568783727c657799ee6513d`.
The driver scenario is therefore unqualified until the missing event or
readiness condition is explained; increasing delays does not establish a fix.

The GPIO trace now records each L6max button-scanner read separately from
other reads of the same pin. With QEMU SHA-256
`66b93d53a35731947169df461eb8170e838ba54f64c4ce1d22149be1e91f549d`
and the same stock MAIN, prepared NOR and scenario as above, a passing run
sampled TAP low twice at 63290 and 63312 guest ms; a failing repeat sampled
it low three times at 63281, 63305 and 63327 ms. The press intervals were
63272–63322 and 63278–63328 ms respectively. These observations rule out a
complete absence of low scanner samples in that failed run, but do not prove
the debounce state, event submission or MAIN dispatch. Tracing can affect
scheduling. The pass trace and result hashes were
`eb420a6f1f4e7b408419b2dcd96e49867f940eb2106d52e6fb8f1ff8fd40b005` and
`5066579a40d5bd38ed3e4f5c222cecd44f57ae148a7725eb3b6529fafe4473b3`;
the failure trace and result hashes were
`0e10f0c880634b75ae6cd246bab980352e10e6dea76ae16de061f41319664e46` and
`2ed7e31847daec6eacc8930f24421a8f9933135ce37a1bbf0b69c028a517f9a6`.

Further static inspection shows that the slow scanner's TAP table entry has
zero debounce thresholds; the separate EventPort loop at `0x800851b0` reads
the same pin and submits the key events. QEMU's optional 1.10 diagnostic now
also records that loop's candidate raw, filtered, event-latch and timestamp
words while TAP is held. In one passing and one failing driver run, its TAP
event latch changed from 0 to `0x10` during the low input in both runs. The
store follows the stock queue-send call, but the latch alone does not show
whether the queue accepted the event or whether MAIN dispatched it. These are
observations of one pair of instrumented runs, not timing guarantees. The
QEMU executable SHA-256 was
`d157c698a5a1eaae83301ed26e2c85ec54e6355f3d537fc7262313c3fde6fd75`.
Pass trace and result hashes were
`0665f386548900e7ac6b321bb3e939eac2fd942fd6c5ad6a64eb13be9b187378` and
`6bd5d4581cf5017df48cb44b9d20d48d192d2d3d68e731a762556a3952fbac30`;
failure trace and result hashes were
`a6357901f4fc1d41184b69981a6e9aa309029ff5d1daf7b6ac57b4364e3c8110` and
`df95f8297c26fd7d8456c913155c46b1c3b3c49d1e3414eb5bd4685cc6da413d`.

Two separate stock-QEMU debugger runs stopped at the EventPort queue calls
for TAP. The down call at `0x800852c8` used event words `(0, 0, 0x23, 0)`;
the up call at `0x80085258` used `(0, 1, 0x23, 0)`. Both calls returned 0
in each run, matching the success return path of the queue wrapper at
`0x8005f5d8`. Both runs subsequently observed the learning-progress byte
set. These debugger runs used 150 ms presses and did not reproduce the
intermittent driver failure; debugger stops can change scheduling, so they
do not establish queue acceptance in the failing case. The recipe hash was
`8819efd0cf3694ee57fe560a1ba0fe5eb887b43dcb05d54bda3cd437511bd259`;
the two debugger scripts hashed to
`02a94423be32ea1f984835e7168f5522b0b1004cf2345daf4d5a002b7acb6ced` and
`12619625ba985b7994b5ba8135c7f8673b77c65b88f45c4c27369c215f2053d9`.
Both debugger logs hashed to
`183bffc4053099e515f2d5a97b0f88fcd51443ade681c8bc385287c1fda8928f`;
the result hashes were
`db341c0772352e2dae86e976ddb9c7f7a5a3e617ea831771d4623852341385c6` and
`73d87cef8f52d89c30c0b7314e21985c77f5b9040336ffea298172aceb3a8c88`.

A non-stopping QEMU plugin traced a driver run that failed the earlier
progress-byte assertion. It observed TAP event `0x23` at EventPort enqueue,
MAIN dispatch and the type-5 learning-start routine `0x80055568`. This rules
out a completely missing TAP dispatch in that run; it does not establish why
the sampled progress byte was already zero. The temporary plugin's first
version did not read register 0 correctly, so its queue-return values are not
evidence. Its source and binary hashes were
`cb740585e6ac221c89173e17f7a3bcf839e2aa1674740a774b96bef6c69dfe2b` and
`e2fcd9cc0314c5035f6530bd0e065502021b2378a4fd6018506ca3a295d42091`;
its trace and result hashes were
`845e8def17ab27e3a70737f5cb410d6097dd271923aa97ad21d537e5cbb855f1` and
`aceb9472ba725a6325e25efbb2cd6a744eae2e2f2a45395b750dee6e0aef99a9`.

The revised scenario checks the stable learned word after 14 guest seconds
and verifies a second TAP clears it, rather than requiring a transient progress
byte at 500 ms. One plugin-assisted and one uninstrumented driver run passed.
The fixed wait includes the stock fallback; it neither explains every earlier
failure nor establishes learned audio or target timing. The revised scenario,
uninstrumented result and inputs hashes were
`45a880c448af8a5cb06f19cf4bb2f48d54b7ac4fd655c2a9f31cf9456e262f1f`,
`f38e16e5c051563d2854053fddff602cfbf5d6c670ecf6377c862db69e1a92ae`, and
`7a11c0b925e4b4747970eddd9796d6d40f2f76fdc8d764b51c466b7b703ff4b6`.
The isolated QEMU executable hashed to
`69b2a6775a4cab8d778d2d9b91e41cfedbbaf35788ac1417955f0be66a874afa`;
its source provenance was unattested. The corrected plugin source and binary
hashed to `d3ed3e6bb839ab26f01d64474c77d6c43ead70c33095ce7bbe5af170511144c8`
and `2a53221f62248f0573c65ee9ff4567f037322b86755dadde615bd0bc69056242`;
its trace and result hashed to
`59518a30b7a78efb9d87ac0dd19da71342ea19388c9d3442c61bae53f3e3faa0` and
`5fc04bb9e6a507fe2a88f6b0052431e4a61582086d1cc8f0c3d79edbc2c00252`.

After rebasing on upstream `393ce52`, the same scenario passed once with the
new paced timing and an isolated QEMU executable SHA-256
`1e35161130eecdb148f2aaf6986e14f77fbd4714e2e7100cf89dc1680eb3b554`.
The driver records external QEMU source provenance as unattested. The stock
MAIN and scenario identities are those above; the input manifest and scenario
result hashed to
`8b5f4168d7c4f2ba78a20d03743d225c618082ecb323ffd169dcc8c77c2e163d`
and `f38df67dc4fe52755ba49b2083aefa2f6ecf3db76a691b4f7dd9126ce9395cfb`.
This single run does not qualify repeated reliability or denoised audio.

Two earlier plugin-assisted runs of the revised case observed the learning
poll take `0x80088dac`, the branch reached when both candidate processors
no longer report state 3, then enter `0x80051080` with argument 1. Neither
observed the timeout branch at `0x80088d9c`. This explains how a successful
run can clear the progress byte before the old 500 ms assertion, but does not
prove the cause of every earlier failure or establish useful denoising of the
emulator's silent input. One trace additionally watched flash addresses
`0x800ee6b2` and `0x800f0664`, whose code references the candidate processor
state; neither address was observed in that run. The latter plugin source,
binary, trace and result hashes were
`35a1bc4c774ba765d54a07c003c89f76a66ecebd67bb9bcea44debe7e9797806`,
`2708a43002a6ba79a969c7a830a93f44f21c99d4a013e9b671b1505b72f402e4`,
`b4bdb76633f71cc946c101525da1a39730a28f1bc759aacb9cc7fa60df67ba00`, and
`f43a46829cc17355bc0273f806b1212b15df3eadb2aff3a16da54b84b1f9a705`.
The isolated QEMU and fixture identities were those immediately above.

A debugger scan of stock guest RAM found the previously established code
patterns at `0x20228a80` and `0x20280500`, but not the patterns from MAIN
offsets `0xee6b2` and `0xf0664` in readable pages of the scanned ranges
`0x20000000..0x20080000` and `0x20200000..0x20400000`. The latter range
included 256 unreadable pages, and the first scan stopped on one of them; the
resumed scan skipped unreadable pages. This is not an exhaustive search or
evidence that those routines cannot be used. The recipe hash was
`a23dd43d67d045af4d2bf8492c9bb9d80f5cf4be880d31a10a0b7136cdfcae48`;
the first scan script and log hashed to
`fcbfe69c57b739e3c9fb598969809df96e722bb6a854f9a46fbd8018cbbb40e8` and
`d1453f73d005b8de02d3cd4b2c4e447380ac77fa102d6ad34e1320366343ba34`;
the resumed script and log hashed to
`e2534bc34aa02c9fb9be5783da7f17b571b93d270d83fa262a63bc0f3742eb07` and
`4b35576424c4edff2fa92109cf50a94452d577927616894bff4ed0651a46c619`.

The preceding RAM scan omitted the low executable region. The stock startup
table at MAIN offset `0xe8c08` specifies source `0x800ee688`, destination
`0x00000000`, length `0x5b4c`, and copy routine `0x80093e60`; that routine
copies words from source to destination. In a separate stock-QEMU run, two
256-byte snapshots at low addresses `0` and `0x1fa0` matched the corresponding
MAIN bytes at `0xee688` and `0xf0628` exactly. The selected callback was
`0x20280021`; the learned word changed from 0 to 1 after TAP and back to 0
after the next TAP. The snapshot recipe and diagnostic JSON SHA-256 values
were `3cd03d26f8df57b86d6678a10f1136bc44a5f5788a8038f70f8d6cb419ce9e79`
and `881f6589bb91d0167a40031eebb214d3ce6a17619dcd30c5ae88c644e4ec0489`.

In a second stock run, a non-stopping TCG plugin watched the relocated
candidate instructions `0x0000002a` and `0x00001fdc` (MAIN offsets `0xee6b2`
and `0xf0664`), effect publication, and learning start `0x80055568`. After
five effect selections and one observed learning start, it counted 14
executions at `0x0000002a` and none at `0x00001fdc`. The selected callback
and learned-word transitions matched the preceding run. This identifies a
candidate learning-related execution path, not an audio ABI or proof that
the routine processes useful noise-reduced audio. The plugin source, binary,
recipe, trace and result hashes were
`537f3974cc320f30e56954956742c982abcec9165ba8506385d6f10ff99a746f`,
`e6da9fa3b98e09625b21e39e6f27e8f2e7961d90318c607f54d950a0fd952eb9`,
`217f4023a90f19e21e88344ca8629846878ba799be13524e7b036e87cad85963`,
`1220d3e11e3fad9b8b6be49fdba13ec52ea389e34d1b97b21e05e6cd024869b7`, and
`1ebe40fab9b0f9c3a8feb7b4a93de92bb8e265f45b130a37740b1b70ec2d9151`.
Both runs used the stock MAIN and initial NOR above. Their QEMU executable
hashes were respectively
`1e35161130eecdb148f2aaf6986e14f77fbd4714e2e7100cf89dc1680eb3b554` and
`69b2a6775a4cab8d778d2d9b91e41cfedbbaf35788ac1417955f0be66a874afa`;
external binary source provenance remains unattested.

At a debugger stop on `0x0000002a` during a subsequent learning run, the
current task was `AudioProcess`, and the saved return address was
`0x20228d8b`. Registers `r8` and `r10` both held `0x20011010`, `r2` held 0,
and the corresponding processor-state word at `0x2028ba4c` held 4. The
sampled 128 bytes at `0x20011010` were all zero. The linked stock code at
MAIN `0xfd7ce`, relocated to `0x20228d86`, passes `0x20011010` as both `r0`
and `r1`, with `r2 = 0`, through a veneer to the low-memory dispatcher at
`0x00000780`. The next call statically passes `0x20011110` as both pointers
and `r2 = 1`; it was not separately observed at this stop. The dispatcher
uses the per-index state word at `0x2028ba4c + 132 * index` and branches to
the observed routine for state 4. This is a candidate in-place stereo
processing interface in the audio task during learning. Silent samples and
one debugger stop do not establish the data format, block length, behavior
after learning, denoising output, or whether the interface can safely be
reused for pre-effect processing.

The active callback was `0x20280021`; the learned word changed 0 to 1 and
then back to 0 after the second TAP. The recipe, debugger script, debugger
log and diagnostic JSON SHA-256 values were
`d27d3cfb5ff58eadb6d7dad52c6adaf2e037a0697bd085c0d328294f8aa0d61f`,
`dd4970786420d397b846fe0ef03684b9444da7e6eb6fcc0dcfa93adb30c3d8dd`,
`4ee37cbc491ab061118b024be97e4b00db09281a162b1ca884323c1aca34bf53`, and
`2cad5291f8d04b1b65828e27b9a7d1b2540568e403a30e7e900b0f4290180298`.
The stock MAIN, initial NOR and QEMU match the first run immediately above;
the debugger stop may perturb scheduling.

Two further debugger-controlled stock runs injected the same 64 `f32` values,
a 0.25-amplitude sinusoidal pattern, into each candidate channel buffer before
its call completed. During the first captured learning frame, both
processor-state words were 4. The first call was stopped at `0x0000002a`
before injection; the second was stopped at the call instruction
`0x20228d94` with `r0 = r1 = 0x20011110` and `r2 = 1`. Both calls returned
the 64 injected values bit-for-bit. In a separate run after the learned word
became 1, both processor-state words were 1; injection at the two caller
instructions yielded 64 zero samples from each call. One word immediately
before the first buffer and one word immediately after the second were zero
both before and after the captures. These are isolated, forced input blocks:
they do not prove the complete memory extent or sample format, distinguish
suppression from buffering or latency, establish steady-state denoising, or
justify an extra call on the effect sends. Debugger stops alter timing.

The learning-frame recipe, script, log and result SHA-256 values were
`d27d3cfb5ff58eadb6d7dad52c6adaf2e037a0697bd085c0d328294f8aa0d61f`,
`18df0ae1201e32bd7b347d59e5d779d890487c4a034d11d33992b38044d23cb9`,
`59a2d4641a06edde5e1175c94ad189102a3498916b5d324a8ffe97a706827f1e`, and
`93bb457e48a25d026f2d02e86b8404305c8c26661e166869f722adcf66d00494`.
The post-learning recipe, script, log and result hashes were
`45233d0c29dc9947da9353f8a78eb1e26788b80958fed6dd31f1cff11cbc51db`,
`76175fada83156b4fe03ec1a3cbdc79fefcb17d7f64b7e16ab5645388e88e54b`,
`4a0cd9483d67983687ed86f8b5a1fd4d827667412a83cc4c31d77dbaad4ed259`, and
`7574f27ea3240ebcbe65782c597c938319ffd515780c9cc3d6fb387f04305287`.
Both used the stock MAIN, initial NOR and QEMU identities immediately above.

A subsequent stock-QEMU run waited until the learned word was 1, then used
debugger stops at the first channel's call and return to inject the same
64-value, 0.25-amplitude sinusoidal pattern into 96 consecutive blocks.
The sampled processor-state words were 1 at both ends. Returned samples were
finite; blocks 0 through 3 were all zero, block 4 had peak magnitude about
`9.87e-9`, block 6 about `0.1251`, and block 95 about `0.2496`. Thus the
previous all-zero result for a single block cannot establish sustained
suppression. This forced, repeated waveform with debugger pauses is not a
controlled denoising-quality or latency measurement: it has no matched
no-injection control, physical signal, or second-channel sequence, and does
not establish the permissible interface or ownership for an additional call.
The recipe, debugger script, log, output samples and result SHA-256 values
were `45233d0c29dc9947da9353f8a78eb1e26788b80958fed6dd31f1cff11cbc51db`,
`9d53e53e6312350ee295fabb8049214a3b2b8008e90e16d480b11f9b7299d53d`,
`7bbbe84dbfc293c2103b2e7724007979a6044cd441242ad9a35c15d5969a2427`,
`ccb87c0c3e0a7e9bfda33be06943b9692130c4e39c849f8c67941a068baa1f00`, and
`1a8dc79eeb56dd7e9bb4912c3668fa931d6fa9b8e7c787558331e87c76f978a9`.
The stock MAIN, initial NOR and QEMU match the preceding observations.

Static inspection of the learned-state branch adds an integration constraint.
The dispatcher at `0x00000780` derives a descriptor from
`0x2028ba0c + 132 * index`; for states 1 and 2 it branches to the routine
at `0x00001060` (MAIN `0xef6e8`). That routine derives working storage from
`0x2028bb14 + 0xda00 * index`, loads 32-bit values using floating-point load
instructions, and writes them into indexed working storage while updating
the descriptor word at offset `0x20`. Its unrolled loop advances over
`0x100` input bytes per invocation. This is consistent with the observed
64-float, in-place calls and shows shared per-index state being advanced;
calling the same stock index again for a different send would also modify
the instance used by the stock master path. The two observed indices' working
bases are `0x2028bb14` and `0x20299514`, 55,808 bytes apart. These bases do
not establish the complete state extent or that copying the region creates
independent instances.

The addressing formulas also rule out simply passing new stock indices for
extra send processors. Index 2's descriptor base would be `0x2028bb14`, the
same address as index 0's working-storage base. Its computed working-storage
base would be `0x202a6f14`, only 12 bytes below the active effect callback
word at `0x202a6f20`. The learned-state routine uses that working base for
its indexed input ring, so expanding the index without changing and owning
the underlying layout could overwrite both existing AINR data and the live
callback pointer. Index 3 is likewise outside the two observed descriptors.
This is an address calculation for the supported 1.10 MAIN, not a tested
hardware modification or a claim about the full region size.

A further stock-QEMU trace recorded instruction and data addresses within
`0x2028b900..0x2028bb13` for two learned-state calls after the learning handler
and 64 subsequent entries. It completed both captures without record overflow;
the later processor snapshot showed state 1 for both channels. The calls wrote
the descriptor cursor at `0x2028ba2c` and `0x2028bab0`, and also wrote distinct
control words including `0x2028b9cc` and `0x2028b9ec`. Both calls read the same
small helper-context fields around `0x2028b964`; the observed accesses there
were reads, not proof that all paths leave the context unchanged.

Static disassembly of the output-control path at `0x000034f8..0x000035bc`
shows another indexed area with a 20-byte stride. In the exercised branch,
the word written at `0x0000356a` is `0x2028b98c + 20 * index`; for index 2
that is `0x2028b9b4`, a word the trace observed channel 0 writing at
`0x000035bc`. The indexed read at `0x0000351a` likewise computes
`0x2028b9a4 + 20 * index`, which for index 2 is `0x2028b9cc`, a control word
the trace observed channel 0 modifying at `0x000012ec`. Thus relocating only
the descriptor and large working buffer would not isolate an extra index.
These are static address calculations plus two observed calls. They do not
establish a complete footprint, whether every branch occurs for a third index,
or the safety of a modified routine.

The relocated code snapshot, plugin source, plugin binary, recipe, plugin log,
result and plugin-enabled QEMU executable SHA-256 values were
`0a4bdaf6e401a90c17d1b49e6907b375cb6cd7b4c0f02ee414b963f8610f457f`,
`e54ab97d97536b5611180e70474fc20de8f05c770f4f318a8b9c5266797a84cb`,
`987f6199c6bfae5d735b37444c5e299caaa1ef34c8e06d9d0fcab1886d3cb974`,
`169572dfa4b7700cacc5e56ca3ea2bbf5f279afb01cb78e3d21a62a3e54eba42`,
`66ce32544f16e7aa1f00134d1e1d8802801f8d0dc6c49a6b4b0673658ac723fe`,
`80579394330fe3bd237969ea4ae98f3a575e4c4918d3c0c493307c7b39d6ca26`, and
`249a29abb9e52e50d46472250475f2903a7b74e840ac0296545f3dad6ba41fee`.
The trace spans calls and may include intervening interrupt activity; it does
not cover DMA or unexecuted branches. The stock MAIN and initial NOR match the
preceding investigation; the QEMU binary's source provenance is unattested.

A fresh stock-QEMU snapshot taken after the learned word became 1 captured
the 192-byte helper/control area at `0x2028b94c` and the first 4 KiB of
channel 0 working storage. The observed helper words at `0x2028b968`,
`0x2028b96c` and `0x2028b978` were `0x800cf7e4`, `0x800bc7e0` and
`0x800e0964`, respectively, consistent with the firmware-table reads above.
The captured working prefix contained 897 zero words and 127 words equal to
`0x80000000`, with no other values. That bit pattern is also floating-point
negative zero, so this snapshot does not identify those words as pointers.
A blanket rewrite of address-looking words would therefore be unsafe.
These non-stopping snapshots are not atomic and cover only part of the state;
they do not prove its complete layout or that the table references are safe
to share in another instance. The recipe, result, engine trace and QEMU
executable SHA-256 values were
`abe6b1802670cac59cc7f39493dbe2066386152e8142462db348516c666a7fac`,
`fa75007234d1ac983563bfd782910ec325db0ebea3717e9f0c99c4eb0a628abf`,
`056aa8a8a3a197a6dcdb511a77ba8f189f73425c788bdca725438afafa2a780d`, and
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`.
Stock MAIN and initial NOR matched the preceding investigation.

A debugger-controlled QEMU-only experiment copied the stock low executable
region to `0x80f00000` and a 128 KiB stock state range
`0x20280000..0x2029ffff` to `0x80e00000..0x80e1ffff`. In the copied code,
63 decoded `movt` instructions that formed `0x2028xxxx` addresses were changed
to form `0x80e0xxxx`; no data words were rewritten. At the learned-state
channel-0 call, the experiment copied state while the guest was stopped,
injected the same 64-float sinusoid into separate buffers, and ran the stock
and copied dispatchers from the same saved register state. Both returned an
all-zero block. The copied 128 KiB state matched the stock state after their
respective calls, and the checked stock range did not change during the copied
call. This is a single forced block, not evidence of sustained denoising,
complete memory isolation, valid initialization, audio quality, timing, or
ownership of the scratch regions. Debugger stops and unowned emulator memory
make the experiment unsuitable for physical hardware.

The copied-code, debugger script, debugger log, recipe, diagnostic result and
QEMU executable SHA-256 values were
`d0ebab6792ca14ddd20b7f27fa9cb27e384e7aa0baeb37cbc6c2427895f0e04d`,
`420d6e19339676db24c53da5837f54658df120d82782ce9806d74210b4f4045e`,
`6f79dbe91bdd31ecb1946d65206297a5d9a30b59831997030fc2f45ad2d41484`,
`45233d0c29dc9947da9353f8a78eb1e26788b80958fed6dd31f1cff11cbc51db`,
`1e777d2b23af0cd256f77c84db57b9421f4e1a04e9604ff227f67990edc69e41`, and
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`.
The stock MAIN and initial NOR matched the preceding investigation.

A second debugger-controlled run used the same copied code and separate
state region for 96 consecutive channel-0 calls, injecting the same
64-sample sinusoid before each stock and copied call. All 96 output blocks
matched byte for byte; 5,932 of 6,144 output samples were nonzero, all were
finite, and the peak magnitude was approximately 0.2496. At blocks 0, 15, 31,
63 and 95, the channel-0 control, descriptor and 55,808-byte working ranges
matched after the respective calls, and the checked 128 KiB stock range was
unchanged by the copied call. The debugger restored the stock post-call
registers before continuing; the later learned and reset words were 1 and 0.

This supports the copied routine as an investigation candidate, but only for
one forced waveform and the exercised branches. The checks do not cover every
call's stock state, other memory ranges, learning the copied instance, sound
quality, simultaneous effect routing, target timing, or ownership of the
scratch regions. Stops alter scheduling and the image remains unsuitable for
hardware. The copied code, debugger script, debugger log, recipe, diagnostic
result and QEMU executable SHA-256 values were
`d0ebab6792ca14ddd20b7f27fa9cb27e384e7aa0baeb37cbc6c2427895f0e04d`,
`cc5e5710c37ab16f056f85d647849b86b73aa6cab5efd28789e8fb37a45cf1e2`,
`201fc55714716f8c80ddfa2343533df1988dae80b8425f04910c69713e512415`,
`45233d0c29dc9947da9353f8a78eb1e26788b80958fed6dd31f1cff11cbc51db`,
`72a653d58ca1ab678a02c32bc31bdd98d9a0b440d36a703939ca9d8374157c39`, and
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`.
The stock MAIN and initial NOR matched the preceding experiment.

A further debugger-controlled run initialized two relocated code/state copies
from the same learned stock snapshot. For 96 consecutive channel-0 calls, the
stock processor and first copy received the same forced waveform, while the
second copy received a different waveform. The first copy matched stock
byte-for-byte on every output block. Both copied outputs were finite, nonzero
and distinct. At blocks 0, 15, 31, 63 and 95, the first copy's sampled channel
state matched stock, and executing the second copy did not change the checked
128 KiB stock or first-copy ranges. The second copy's output was not compared
with an independent reference, and the checks do not cover all accesses,
learning, simultaneous execution, audio quality, real-time behavior, or
physical memory ownership. The debugger stops the guest and both copied
regions are unowned emulator scratch; this is not an installable design.

The test is reproducible using `ainr_isolation.py --independent`. The first
and second copied-code SHA-256 values were
`d0ebab6792ca14ddd20b7f27fa9cb27e384e7aa0baeb37cbc6c2427895f0e04d` and
`4aeb87905ba0cfc270ad8a9d915b15e0322b7161ef2723f5982a8e411553e18f`.
The probe, recipe, result, diagnostic JSON and QEMU executable hashes were
`4582b6ee8e57d5a3f6aaeb07437a9a8a5bf55f146ff1361c4dd75211acb98eae`,
`45233d0c29dc9947da9353f8a78eb1e26788b80958fed6dd31f1cff11cbc51db`,
`5a18a2bdd98b40da1638fcd313f5a8d153d9c494431f9e53b4ac7ce6ab19bf27`,
`1c849b91b8a16ea57c1459624174eff9e795945cbab5e7df22a03ae3a033a73e`, and
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`.
The stock MAIN and initial NOR matched the preceding experiment; QEMU source
provenance remains unattested.

A later version of the paired diagnostic also ran the second waveform through
the original stock routine. While stopped at each block, it saved the first
stock state, restored a separately saved 128 KiB stock reference state, ran the
second signal, captured the resulting state, then restored the first stock
state before continuing. Over 96 blocks, both copies matched their respective
stock output byte-for-byte; at blocks 0, 15, 31, 63 and 95, each copy's sampled
channel state matched its stock reference. The checked 128 KiB stock and
first-copy ranges were unchanged by the second copy at those checkpoints.
The recipe completed and its learned and reset words were 1 and 0.

The paired-reference probe, recipe, result and diagnostic JSON SHA-256 values
were `4730c22a20bb3a100c3ec23a6ff51508e9da896e440e058c082827fda6e12691`,
`45233d0c29dc9947da9353f8a78eb1e26788b80958fed6dd31f1cff11cbc51db`,
`c22f059d37224c66b7c726467843529eff71c6d9e1a7202a181ea45738c5e98f`, and
`54b44c0613baef81a6e614977313a490f491bfb545758e90b607a0f168eab906`.
The stock MAIN, initial NOR, copied code and QEMU binary matched the preceding
experiment. This is still a forced, serialized emulator comparison; it does
not establish complete isolation, denoising quality, initialization of an
independent learned instance, concurrent execution, timing or hardware memory
ownership.

A QEMU-only integration experiment copied the learned stock AINR state and
relocated code into three separate scratch regions, then called one copy on
each synthetic A, B and A-to-B send before the routed effects callback. The
observer checked the stock selection, enabled state and learned flag before
copying, and verified each code and state readback before enabling the test.
After returning to Room, one run observed the callback count rise from 2,417
to 4,715 over 5,000 guest ms, approximately 460 blocks per guest second. The
sampled send values and final effect output were finite and nonzero. At the
second snapshot, the sampled before/after pairs were approximately
`(-0.03125, -0.0312893)`, `(-0.015625, -0.0156380)` and
`(-0.0703125, -0.0703125)`; final L/R samples were approximately `-0.1269`
and `-0.2193`. These are forced periodic signals and single-sample snapshots;
they do not establish noise-reduction quality or physical input routing.

The count is below the modeled 750 blocks per second and is not a physical
deadline measurement. The experiment also does not establish independent
learning, complete state isolation, stock coexistence, target stack use, or
ownership of its code and state regions. The diagnostic changes execution and
must never be installed on hardware. The [probe instructions](../../tools/l6dsp-tests/probe/README.md#pre-ainr-send-experiment)
describe how to reproduce it. Its callback, preparer, observer, ELF, recipe,
result and QEMU executable SHA-256 values were
`e887cf586db27e4b1004a957eb27be6959009b12de9be45b6c4c2443ee34748f`,
`d0c188954fd3534d437e9b80465d29a1a1ca8e07eb56fffaf80c7c20dbd3b822`,
`cc2a00cecccbf197b2fe3d464cb6ccff5b5501b6a006aafe6cee01743810dbe0`,
`72fbfef648db39416d7f4920c1a590a821a2ef6c9406b8b6f65341a56ba1a987`,
`aa38ff10c9506b56c4b09ebc9fbca1e430fa5067409ab6ab8528f4f0786a9b0f`,
`ffff00afc4a6a6ef34bdf3c12855e02ad6b417ae629a253c8187ed74abd738ae`, and
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`.
The stock MAIN and initial NOR matched the preceding experiments; QEMU source
provenance remains unattested.

A further QEMU-only layout experiment placed the copied AINR states inside the
observed stock effect buffer instead of the earlier high scratch region. A
separate `thumbv7em-none-eabihf` layout object reported the current effects
processor as 297,936 bytes with 8-byte alignment. In this build it occupies
`0x80d00000..0x80d48bd0`; the three 128 KiB copies start at `0x80d50000`,
`0x80d70000` and `0x80d90000`, ending at `0x80db0000`, before the observed
stock buffer end `0x80dbb800`. The diagnostic put the relocated code copies
at `0x80130000`, `0x80136000` and `0x8013c000` in the erased application
tail of this fixture; it did not create an update package.

The observer captured stock learned state in host memory, waited until Room's
initialized callback, stopped the guest, copied and read back all three
states, and checked that the initialized effects state was unchanged. The
callback then ran with synthetic sends. At guest times 87,160 and 92,160 ms,
its counter was 2,416 and 4,713: approximately 459 blocks per second. The
sampled send values and final effect output were finite and nonzero. The
observer, callback, preparer, diagnostic MAIN, ELF, recipe and result hashes
were `49590e20dc27fdf2132b723165f77c038aaeddc9070a81ed1ed93ccb39b75822`,
`8169ea841b3a9bcd9cbe6f5c3844275d9a4f933e748905069cbb2ef03521a50f`,
`1c315accaa725580e9bdf126e1d5e5ff9e0d4bb1eac3a696a80f7af02349e6bc`,
`89921e238c6ad4bc3b9a14b399197005563a41a6ef600525afe99ed74a38795d`,
`b33b2dbdf7a0ce3e965fd3345b34d0a06dec9dbc50c3d782575fae119f8e9b8e`,
`598a39b78c0de7eabbc979dc36425cd4672f60d296cc539b8ca23e5acc4981db`, and
`b36586771d6e8f0bdf91b71fb1b8c70d6d89e435dc79d30d1bb94f0dd01013e8`.
The stock MAIN, initial NOR and QEMU executable matched the preceding
experiment. The layout source and object hashed to
`abeca8fcb7c91d1e200e235d60bf9ac8f08ffbde6d24584247a46b9cd617517f` and
`a7121d9439ff4cc495a209d68850ad637ef8ed052a3d9e0c0b70d7eea5673ca3`.

This supports investigating that buffer layout, not a hardware reservation.
The host-held snapshot and debugger stop are not a device initialization
protocol; other stock or DMA users, effect-switch interleavings, physical RAM,
packaging, audio quality and the target deadline remain unqualified. The image
must not be installed. The probe's `--effect-buffer` option reproduces the
experiment.

To check whether the physical TAP input could offer a control path while the
experimental chain is active, two further QEMU-only runs added two TAP presses
after returning to Room. A plugin traced the stock EventPort queue calls and
MAIN dispatcher. Both runs first selected AINR and used one TAP press for
learning; that press produced a down/up pair at the queue and dispatcher and
the learned flag became 1. In the active-effects run, QEMU acknowledged both
later presses, but the trace recorded no additional TAP queue or dispatcher
events. The routed callback remained active with finite, nonzero output.

The matched control kept the same copied-state setup and synthetic inputs but
deliberately bypassed both the three AINR calls and effect DSP, outputting
silence. It recorded all three TAP down/up pairs at both traced sites. Between
the two snapshots, the active run counted 2,079 blocks in 4,534 guest ms
(about 459/s), and the bypass counted 3,402 (about 750/s). These are individual
instrumented emulator runs; they indicate workload-dependent input delivery
in this model, not a physical deadline or proof of the precise stall location.
In particular, routing an added pre-AINR control through TAP is not validated
as a workaround for the observed active-load failure.

The active and bypass diagnostic MAIN hashes were
`1dd114e338bc41d9f4aa64d8363302240a5940ad16bd5945af233f6cb93c3fd6` and
`f9c0849e8cf99549d93b11412bff7e5a167b1fba439b7abd25f282915416d1f4`;
the ELFs were `72fbfef648db39416d7f4920c1a590a821a2ef6c9406b8b6f65341a56ba1a987`
and `5c0f0ea9cd1409eda9dd3bc66abe5ef7809979ea4a66bd890d4da620f3476e90`;
the recipes were `1acdb59b1eb0c46cf3155998e7075a14105b2be99e18b009305fbe069952a48a`
and `d9e756177d8d0cac954af878f1c2d097304701e2476fdfba5fb903d556e47925`;
the result JSONs were `24fe1e3b6f10e268ee099266cdbe63b045d1fdc6db082d585522d5b2c1a3eea1`
and `13f3e5d0da8eb81e087d199a31a10353c8f75df7978a2b2b5c4557bcbefada83`;
and the plugin logs were `1840d0e8786b0399bc681386943d52f7d15269a6419d96ec3c9976c35345a34e`
and `da1893f5768c37081bec35116c31c4ed01ef02f9f90defa84ef042d2d422d5f6`.
The bypass callback source SHA-256 was
`181862b7611104d5909072b95d56dd8e1f3e6888a7bc70bb8453d50dba22628e`.
The plugin source, plugin binary, QEMU executable and wrapper hashes were
`35a1bc4c774ba765d54a07c003c89f76a66ecebd67bb9bcea44debe7e9797806`,
`2708a43002a6ba79a969c7a830a93f4f21c99d4a013e9b671b1505b72f402e4`,
`249a29abb9e52e50d46472250475f2903a7b74e840ac0296545f3dad6ba41fee`, and
`e0cdc14726d4ab96b18345acd28b6984f38371fd46037afb0365beb2efe90efd`.
The stock MAIN and initial NOR matched the preceding experiment; QEMU source
provenance remains unattested.

An instrumented stock-QEMU run on the newer board model traced CPU memory
accesses between entry to `0x00001060` and the two stock caller returns. It
armed at the learning-start handler and skipped 64 subsequent entries before
capturing two calls; the later snapshot showed the learned word set to 1.
The captured calls read and wrote the respective 256-byte channel buffers.
Their observed working addresses extended to `0x20298513` and `0x202a5f13`,
respectively 51,712 bytes from the two previously identified working bases.
Both also read firmware tables in `0x800bc000`, `0x800cf000`, and
`0x800e0000` pages. The trace spans calls and can include interrupt or task
activity; it excludes DMA and unexecuted branches, so it is not a complete
footprint or proof that those regions can be cloned safely. The plugin source,
plugin binary, recipe, QEMU executable, trace and diagnostic result hashes were
`b771980a371d91ba9a5924ff4a8ac03922044367434558169c53c5be9f687e4e`,
`c0c1399e08fd549f09a659a8833db0a03b3335c2d7fee37e4523dbd681014f25`,
`cb1841ecc9d2fbb185e75d204d493b8b8d1e192dab6e2e41b5ff4a54670339d7`,
`249a29abb9e52e50d46472250475f2903a7b74e840ac0296545f3dad6ba41fee`,
`630143b559ca826d46234a18f2b8fb96854218650d59563c61bce5f87761811b`, and
`8a4d56bb737c56ac07e23a149ca98b2990d5d59a2aec145b99e38b98b89226a2`.
The QEMU executable was built separately with plugins enabled; its source
provenance is not attested by the executable hash.

The learned-state routine's visible prologue saves nine general registers
and four double registers and reserves `0x4b0` additional stack bytes: 1,272
bytes before deeper calls. This static lower bound excludes callers, nested
functions and interrupts. It is not a target stack or timing qualification;
any integration must account for this cost as well as state isolation.

## Plateau tank delay diagnostic

After correcting the Plateau tank rings to return a sample after their full
configured length, the Cortex-M7 diagnostic payload built successfully. A
separate active-sends QEMU observation recorded 5,980 callback invocations
and finite, nonzero final output samples. The experiment uses synthetic input
and unowned diagnostic memory; it does not establish target timing, audio
fidelity or safe firmware integration. Its ELF, MAIN, recipe and diagnostic
JSON SHA-256 values were
`5aab67156c6bc9e9409f4b90b06da34bcb8c804b90f267edd0e80fc4d3ec8feb`,
`523624b03ab0133ce0940011bc81b4287380d6f28e4c449e968c0a9a18707fb0`,
`fc954098837fedebfdec5e76499b359cdb2eff442942f6fbaf9a81d88c72710d`, and
`e534301e645c6e7e1cfed1777b22694968128347f28176e196d10f67b1ac23b2`.
It used the isolated QEMU `69b2a6775a4cab8d778d2d9b91e41cfedbbaf35788ac1417955f0be66a874afa`
and stock MAIN and NOR identities given above.

After correcting the one-pole low-pass cutoff coefficient, a separate
active-sends diagnostic recorded 5,979 callback invocations and finite,
nonzero final samples. Host sinusoidal-response tests cover selected cutoffs;
this QEMU observation does not establish filter fidelity or execution-time
margin on hardware. The new coefficient requires more work when parameters
are updated. The diagnostic ELF, MAIN, recipe and result hashes were
`47e2055ef31e21e70ffa4328c4d126cd543760a6f7ae1196c13d71c38224b351`,
`5f86da2a5e4cd29a2f41f58b10b574f538418a86a2ad7b0e874a9df301f0ee24`,
`ddf96cfaa159a94dc017ffa844d75a304115064c90dd89c5ee5ca3dcfdfaf66f`, and
`6344c3a4d919d52854146b02df2849d54afd7bbfbe1e17368ee5889954f18b61`.
It used the same stock MAIN, NOR and isolated QEMU as the preceding run.

After replacing the complementary high-pass with a normalized high-pass
recurrence, the active-sends diagnostic observed 5,978 callbacks and finite,
nonzero final samples. Host tests cover cutoff and passband response. This
does not qualify target timing or hardware audio. The diagnostic ELF, MAIN,
recipe and result hashes were
`9108b780f818570bf67187e2bb4e33af271d49846137f97e6bc435576464ceef`,
`fa9f6756e50c58ff6a0f51792371c09f62affe1732bf0002b31ab1c5c2410cf0`,
`a89bcbe8c32372180363402425c0e1c97bbca7393380c336940a850b8a3e91ee`, and
`253e289c9d830bbc70469cbf67bf02431de54e966760cd9e2d1cf6843b90e443`.
It used the same stock MAIN, NOR and isolated QEMU as the preceding run.

After reusing the shared cutoff coefficients across the three independent
filters, the active-sends diagnostic observed 5,978 callbacks and finite,
nonzero final samples. Host tests verify that cutoff updates preserve each
filter's history. Target execution time remains unmeasured. The diagnostic
ELF, MAIN, recipe and result hashes were
`638e6ddb5b9ec8a65de4ae21b3bc1b96c03a0b9713cf2c720faae22c04614abb`,
`2e9341771ae9e90729a596c617a9495d763c922c5f23d54cde264268235bbdbc`,
`ea5cae1350cad31705b876aeb0faf500350afabf1b63ce50437046bb593c68ba`, and
`b1343727c122dc93b0830d487ecfd5729de2a76958b4939c0f11d8e655446c15`.
It used the same stock MAIN, NOR and isolated QEMU as the preceding run.

## QEMU routing controls

### Exploratory audio-wrapper quiescence handshake

Static Thumb disassembly of this MAIN's relocated wrapper shows the active
callback loaded at `0x20280104` and called at `0x20280248`, with no explicit
call in between. A flag set only on entry to the custom callback cannot cover
the interval after that pointer load. The selector disables the active word
at `0x2028064c`, but it calls a selected effect-specific routine at
`0x20280670` before reaching the redirected clear at `0x20280672`. A barrier
placed only at the clear is therefore too late to protect that earlier call.
These are static observations, not proof of all task or interrupt behavior.
The wrapper and selector byte excerpts had SHA-256
`a7d776a2d712a81653dd7ff2117a3e2da0f93a46acb5d8fe6dafd4c33b8b57de` and
`127b1f6f87d5ffdb0da9d28002c8d608e35649ac3232d8d00f33496ae3119007`.

A disposable QEMU-only prototype inserted a wrapper-entry acknowledgment and
an early selector barrier immediately after the active pointer was set to
zero. The selector requested an acknowledgment and called the task-delay
routine with a bounded wait; the wrapper acknowledged only from the observed
`AudioProcess` task while the active pointer was zero. The separate research
checkout, commit `a156b2f7d4243d434538f0681889cb57519e43cf`, identifies
`0x8008e770` as a strong semantic match for `vTaskDelay` for this same stock
MAIN, while explicitly leaving the exact FreeRTOS version unproven. Its
function catalog and RTOS document hashes were
`13dbe80c5044a92475083f59900f1758fe5c7f6640c9018b174c3030b21a0afa` and
`458fd4561a6f5064446a15a58093877bf8e8453299b40b77d8b9069aca595e4c`.

In one run with added work to lengthen the callback and both `Main` and
`SubMcuRX` temporarily at priority 31, selection began with the diagnostic
in-flight marker set. The acknowledgment reported it clear after one delay;
Room switched to Spring, the callback entry/exit selection-overlap counter
was zero, and a later Spring block retained a nonzero impulse tail. A separate
forced-timeout variant withheld acknowledgment: after 20 waits, the observed
active callback was zero and the diagnostic still reported Room. It branched
to the stock selector's exit without reaching the redirected clear. This
contains that particular failure in silence; it does not restore the prior
effect or establish that earlier selector operations left every other state
consistent. The artificial workload, markers and task priorities change
scheduling; the instrumented flags, patch locations and memory are unowned.
Other callers, callback lifetime across every path, timeout recovery,
inter-task ordering, stack use, deadlines and physical behavior remain
unqualified. No part of this prototype is enabled in an installable build.

The busy variant's callback, preparer, linker script, MAIN, ELF, recipe and
result hashes were
`aa586c0be6f8906b0b1444cb1a969aa2f31d22c815b9cf2467f090d22099be6b`,
`12f4f76830963c807e9a5c7809b6b6876bda14f13ddf4b4e6872c10e353fb4ca`,
`b0a558813276bc9b6105b7348dc5e0b23b244922b9a47e7d553d8390a97a3a1f`,
`f2532f4fc2c9262643622b0eef86a7e2693d8b59227c0f87ab1a7c7dcd3fe4f5`,
`73b70ca2c99d19530c75afcfa1d738f11c3d5e1c6c0d8f3dfd5e0e59b8bf2859`,
`68cd204cfe8af86ed58ebc7d2091de10f8ecc254f47c14c7e649bff036856b38`, and
`46a5730fd9462fc7043d84d5d95c9b74fa733f5b64c6c53235407cccf7f92435`.
The timeout variant used the same preparer and linker script; its callback,
MAIN, ELF, recipe and result hashes were
`2cee1c013900cf91fe49b46a607f90664ef64eaede30a37b0eebf29fbd133d37`,
`73b8347e509651192161ce1655e2a3128cb24615adf4f25645401c969d5bc3af`,
`ecf3e1a96f355a96708ca54d41b4e13751d7d81cfda3ae5e3bc71af22f61fa37`,
`d550b29e994f3846dff4ea0e08d16e261b294a9bf1a545b594f0f2f5d76a5a56`, and
`0cafd59b68f7f9967b3761c871a79fddd95f433bce105f0cf55d3fafbfd4c2c3`.
Both used stock MAIN
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
initial NOR `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`
and QEMU executable `53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`,
whose source provenance is unattested.

A full-cycle probe exposed a gap in that barrier: the selector's special
AINR branch at `0x202805d4` bypasses the `0x20280648` hook. Across seven
presses, the earlier hook counted six transitions, omitting AINR. Returning
to Room after stock effects reused the shared buffer produced no retained
impulse tail in the sampled diagnostic output. This is one instrumented
signal sequence, not a general measurement of tail behavior.

Moving the barrier to selector entry or immediately after the stock semaphore
acquisition stalled the observed selection. A debugger snapshot showed Main
in the delay loop at `0x8002e98e`, called from the selector's fade operation
at `0x202805c6`, after the experimental hook had cleared the active callback.
The semaphore is a stock counting semaphore, and static inspection found
multiple selector call sites; neither observation proves all possible callers
or priority interactions. A timeout that leaves the callback disabled still
needs a recovery design.

A further disposable variant placed the acknowledgment after the fade and
before the normal/AINR branch, under the stock selector semaphore. It assigned
a different request value on each selection. In one full-cycle run, startup
and all seven requested selections, including AINR, produced eight recorded
acknowledgments from the observed AudioProcess task. One transition began
with the custom in-flight marker set and acknowledged with it clear. Three
sampled words of the shared buffer were zero after the recorded stock clear;
this does not establish the contents of the whole buffer. Room's sampled
tail remained lost after the stock effects. The artificial workload and
raised priorities alter scheduling, and this result does not establish target
memory ownership, ordering, deadlines, timeout recovery or safety on hardware.
The variant remains emulator-only and the production builder remains disabled.

The original full-cycle probe's callback, preparer, MAIN, ELF, recipe and
result SHA-256 values were respectively
`808b3d3564ca2e7a0c1d0bcf0f34a59c875ac28115def9218f642875f867abb1`,
`12f4f76830963c807e9a5c7809b6b6876bda14f13ddf4b4e6872c10e353fb4ca`,
`8f73b5fffa4dda0f31ef38d1bc532c4d4e225a287b57361e7c6eb62a5cd06f31`,
`6435208b1e84399b9126075c2d4557a567639eadb30b67dc49a555fa0ee56736`,
`58800cf5c9f0329f2e11e212bfe1ab186bb1229f0029c80024bc01e0417c0e33`, and
`cc51768d4c6aa45a51d031efea6b625555dc446511db9e30349c7afea4465bad`.
The post-fade variant's corresponding hashes were
`3864070e8c9aa81af34a06436c5602b982281ad25f25be2de741e43971ca8752`,
`405efeb201ecefebc118423839508fb49d09e74188ee1c854b0e3932b3a0a2ad`,
`46c196d92ca468a171e25c4ccf105c68efae47205c2e70bd00d6819307fad5b6`,
`806ef1ff43326e7f7a213397ea2ac9056ccc2f70511721c99bcbbaf2d97b49dc`,
`2d1543f979039b454e72878a21a0e7da261505ea97d19555f76b3934faa09728`, and
`03efb4c14f60c63c820f0b7a84161eccc52f033bdafb030ed11f233425e5b984`.
Both used the preceding linker script, stock MAIN, initial NOR and QEMU
executable. The debugger script and snapshot hashes were
`5acfe8f4f5516bb39d859f780345e8ce958c318f670035b9dd39b93926a87245` and
`757067e45f53969b746a3b685a12f0a6dd336559aa3fefb370ef0300e0805e07`.

A separate post-fade variant deliberately withheld acknowledgment only for
the first button selection after startup, then allowed later requests. The
forced request timed out after 20 waits and left the active callback zero.
The next press selected Spring and the following press selected a stock
effect; both subsequent requests were acknowledged. Thus this sequence did
not remain permanently stuck, but the requested first selection was skipped
and the effect was disabled until a later press. The probe does not establish
correct recovery after other failures or concurrent calls, or that the
intermediate audio and other state were acceptable. It uses the same altered
priorities and unowned memory as the preceding experiment.

Its callback, MAIN, ELF, recipe and result SHA-256 values were respectively
`ef438264120b5d11141ebea47ae1ef8cac0722f54be0ceb3f8e08e37de32cdc0`,
`7d8d19d8370d451bc3466c727c0ecda7a40dd07153b35abd9477343f186c52bb`,
`f293f6db3477434b7dede9a612e7c6c44f8637c029e1a8dc80e0f5fc5a8b9f69`,
`bddc563892cd7b990462585565831269ae9a22ea8dca45224ac264b0febe648e`, and
`da3b1b2d8d4ee6f5a818329492c883414deab00998592b832c4924ac969d424e`.
The preparer, linker script, stock MAIN, NOR and QEMU matched the post-fade
variant above.

A further disposable variant saved the previous callback before disabling it
and restored that pointer if the acknowledgment timed out. It withheld only
the request for Spring, after Room had been selected. In a repeat with a
second observation two guest seconds after the timeout, the active pointer
was again the diagnostic Room callback and its count advanced from 3,625 to
4,994; the following press selected the stock effect. The first run also
observed the restored pointer and subsequent stock selection. Neither run
measured whether the stock fade or other selector state was restored, nor
established audible output during recovery. A late acknowledgment, other
callers, interrupt ordering, and actual ownership remain unqualified. This is
evidence for a possible recovery mechanism, not an installable implementation.

Both runs used callback SHA-256
`b5d08672c1ca3d264b352181c4a9a464843d35c1307425e8459d8c7502e153c9`, MAIN
`819264b1fb5c958cb79adc75b88f3abc429e205bfb014b8b3a3eaf0da94908d2`, and ELF
`3cce71104d873d277afdfd81619ba6605c9390cffaa24007db0ab11429353f66`.
The first recipe and result hashes were
`8ef4599aa1c93d8ce11c95daedf39bcfefd90d34fe84c537d4db5c45bd6c104f` and
`e0fd28ca12f2cb4fdfd91a2f873fea5a7796754d775faef75efa509aab7e5eaf`;
the repeat's were
`c8639b9a7bdf63c199b8c04313b72ee2c0a4449ed714e0a6dc4a93e5b026e335` and
`24a548c38f84045481d0cf9505abc3619c8e5452ff2627c752c54b474e8f33ec`.
The preparer, linker script, stock MAIN, NOR and QEMU matched the preceding
post-fade variants.

The restoration variant was also run with continuous synthetic sends. After
the forced Spring timeout, the Room callback remained selected and later
continued to run; sampled left/right callback outputs were approximately
`0.02268/-0.04137` and, two guest seconds later, `0.00425/0.01305`. The
observed fade flag at `0x8020a3e8` was zero at both observations and before
the timeout. A subsequent press selected the stock effect. These are
instrumented samples inside the callback, before any later processing; they
do not prove the final mixer output, audible continuity, or restoration of
all stock selector state. The synthetic sends and raised task priorities
change the workload.

The callback, preparer, linker, stock MAIN, NOR and QEMU identities matched
the preceding restoration experiment. This active-sends variant's MAIN, ELF,
recipe and result SHA-256 values were respectively
`8a3e33f10adb345c88b8d0934df4af0d176231d2a77c55792eac2e51db377f19`,
`c594b24b6a652c577005f2d6dca093e5f83aa6067708bc96d363893e7bbbe841`,
`e349b82b688ccbd789de12e961a13e5d818f0c95a73297cccf5398ae895be388`, and
`39222f2ef02dd4a0ab3550436df19e3ee2eedafd6f0c218bc0ac913aee3b31d6`.

A follow-up inspected the modeled SAI1 transmit DMA after the same forced
timeout. Channel 14's observed descriptor had source `0x2000a720`, 32-byte
minor loops, current count 1 and initial count 128; this identifies the
4,096-byte source block beginning at `0x20009740` for that snapshot. With
continuous synthetic sends, captures of that block before, after and two
guest seconds after timeout each contained 768 nonzero 32-bit words. A
separate control forced the diagnostic callback output to zero after the
timeout: its pre-timeout block also had 768 nonzero words, but both later
captures contained only zero words. The subsequent press selected the stock
effect in both runs. This supports propagation of the restored diagnostic
effect to the modeled transmit buffer for this sequence; it does not prove
physical jack routing, audible continuity, or correctness under other timing
and failure conditions. Captures and the control alter emulator scheduling.

The source-location probe's recipe and result SHA-256 values were
`63f0808453c07a9f71a2a89ae1cef950bab0ac869e938bca5dab4508c858b895` and
`8624abb19d4592d01bc300c416c3be46372a33fd7b215b4cbc3beb278f2b1358`.
The nonzero capture used the preceding active-sends MAIN and ELF; its recipe
and result hashes were
`ddc63b8474e632197c9b2215e10187f4be6bcba5778a96262c755f793ae0bff9` and
`3aae9d379aa2d073f9d4194a440942640495b06b700f18a91c11f7e31a510de5`.
The silent control's callback, MAIN, ELF, recipe and result hashes were
`e48c4b443140ab949b510f3bb7f40a73f9fd3fae2956ffb12f046c031098c764`,
`8a576a3605837533fa783ba0e3999443768d8f3ab381048428fbdcc3f6abf0a5`,
`14ab0f9a45a66709669defcf5bb06906a381c2e2196f262d96de575b4351b1fe`,
`984122a6c5eb0c400c397263cfb2dfde5a89522dff91f35c24057c3045589e53`, and
`0e46ae94ef7b8f37f172d85c1c136f8f231262a1aef8adca08703b96921b78ca`.
Both captures used the preceding preparer, linker, stock MAIN, NOR and QEMU.

Two additional instrumented runs captured panel snapshots around the forced
Spring timeout with callback restoration. In the repeat, the mapped Room lamp
was lit at the first post-timeout snapshot and two guest seconds later, while
the restored Room callback remained active. The next press reached the stock
Delay callback and the mapped Delay lamp was lit: Spring was skipped in this
sequence. The snapshot after the initial Room selection still showed Hall,
illustrating that panel updates can lag the selector under this workload. A
byte sampled at `0x8045ff90` remained zero across the three observations and
did not identify the selected effect; it must not be used as evidence of the
internal selection state. The captured OLED frames were identical and do not
establish an effect label. A complete recovery design must reconcile the
requested selection and its feedback, not only restore the audio pointer.

The first display recipe and result SHA-256 values were
`1a488cfc7d77261f34c2b3fd3df52203c54b9d391d85d86e0d3615dcaff805e7` and
`71b15273cf283ebd1e430ace15f90b5530e3da3930f2e29c2b63919461b8055e`;
the repeat's were
`d2984e7200478343bb47fd5257399fc93b4cd99ba935637bf8b2b008aee5db5d` and
`62320735fbdb70356797474f8790c9f96e822b817dc3f1bb5ea6aee4e40c8b01`.
The repeat's settled OLED ASCII capture hash was
`92f93b4183084b065b2d99774c8f97b3b27f191a645d5bc2c538c3332f116d7e`.
Both runs used the preceding active-sends callback, MAIN, ELF, stock MAIN,
NOR and QEMU identities.

Static inspection of the observed panel path explains the skipped selection.
In this MAIN, `0x800542b8` obtains a type through `0x8000b270`, calls the
selector through `0x80054340`, updates the lamps, and then calls `0x8000bd78`.
The getter reads the byte at `0x804600b2`; the later setter writes that byte
and a word at `0x80462de8`. In a forced-timeout run, both fields progressed
from Room (1) to Spring (2) although the Room callback was restored; the next
press advanced both to Delay (3). This is scoped to the observed handler and
does not identify every caller or all side effects.

A disposable handler-specific rollback experiment substituted the previous
type in the handler's stack argument after the forced timeout and before its
lamp and setter calls. Its guard deliberately recognizes only the third
request in this test, and the selector still restores the prior callback. In
one run, the two type fields stayed at Room after the timeout, Room continued
producing nonzero diagnostic samples, and the next press selected Spring
(both fields became 2). The immediately sampled panel lamp still showed
Room, so Spring's eventual indicator state was not established. The patch
does not handle other callers, concurrent selection, arbitrary timeouts, or
all effects and side effects; it uses unowned memory and is not installable.

The unmodified-handler field-trace recipe and result SHA-256 values were
`f6aa9d53f68d68ab9fbfa15d369b0c36de3bfcbdcf7985d5b1769bb545a11b1f` and
`4593df8495e3cb204809b47762e2977608efddc6046ba56c4c0a25901484f5c3`.
The rollback variant's callback, preparer, linker, MAIN, ELF, recipe and
result hashes were respectively
`01d82e6c6f2aaf5554f98ea6a40446a4e1691889a560868b5e4dfd61cfb2cf99`,
`c6bfd30fcf616c8638de682501f022df182cf9c742c2bbf1d1fec07ff8b020a6`,
`a57f22f6ac5f974a45b5460b5e2315451558365fbe0208546ac99e24702d3653`,
`b821c93d96106d32faa9f6eec7089a93ea8b5392946ba68a98d0a072788a4f17`,
`8682df1ea6662c064faac3d94e6e032399a0f973fcc9845abe9d96c44c04e9f2`,
`1435915984093b4439da37189944ff430d6d7d1d5d5b604f32128686b6f5be66`, and
`814c58d8060b0506c56af9a8bb6167cf6fc6354ab3152edb4594a17df10d2193`.
Both used the preceding stock MAIN, NOR and QEMU identities.

The stock panel path also calls `0x8000ccf8` before the selector, which emits
an effect-selection MIDI message through the modeled LPUART3 path. In the
preceding rollback run the recorded bytes were `b0 75 2b`, `b0 75 40`,
`b0 75 40`: the failed Spring request was reported even though the callback
and stock fields were restored to Room. A further disposable variant moved
that call until after the handler-specific rollback. Its trace recorded
`b0 75 2b`, `b0 75 2b`, aligning the timeout notification with Room, but did
not record a third Spring message after the later successful selection, even
though both stock type fields became 2. The cause is unresolved; a successful
recipe only means its observations completed. This moved-call experiment is
not a valid fix and does not establish notification timing or ordering for
other paths.

The original rollback engine trace SHA-256 was
`49c7bba6d1aabda81c6bc9c3beab2237ccca5bd15ee05fef085f09aa6cadd5d5`.
The moved-call variant's callback, preparer, linker, MAIN, ELF, recipe,
result and engine trace hashes were respectively
`55f287916ba45b1fd9f5bb970da292e5948f6179a06246110ad1a0fed32a6745`,
`8fcff94654db5787b3b1bc0d253f2cb077e25214b6a6c5ed10fdfa523149e706`,
`a57f22f6ac5f974a45b5460b5e2315451558365fbe0208546ac99e24702d3653`,
`5104abc9e6eb6829aa57a21207833d0ac6cf4c7c12620cf1c0cd8fb2b030e041`,
`8bec6d04b8100498ca41fd4d22becf84e209eccda0ad3261bec2e67a0318ce50`,
`8c4aae75bf09e19e88145cd1ad020ba14c845a4b77724a01d5dcd48f9a25bb01`,
`181ef9ad2f268c91af85d2e6fce3880598c1672fa12bf118324d4c08f0581e16`, and
`7152ee8e138b883010b2a3fa5dcf3f66bd5454d3fe99259b53c2c5558cce094f`.
The stock MAIN, NOR and QEMU identities matched the preceding experiment.

A trace variant of the moved-call experiment recorded the requested type and
panel-handler mode on entry to the deferred call. It recorded `(1, 0)`,
`(2, 0)`, `(2, 0)` for Room, the failed Spring attempt, and the subsequent
Spring selection. The two stock type fields ended at 2. Even with an extra
three-guest-second wait, the LPUART3 trace contained only `b0 75 2b` twice.
The handler was therefore reached with the expected mode for the third
selection, but the trace does not establish why the Spring message was not
transmitted. Instrumentation and the changed call order affect scheduling;
the result does not prove a particular queue or hardware failure.

The trace variant's callback, preparer, linker, MAIN, ELF, recipe, result and
engine trace SHA-256 values were respectively
`29f4154e567431900d2bfdd1351da4caee41eb471a36cd0facf2127cb85e017c`,
`8fcff94654db5787b3b1bc0d253f2cb077e25214b6a6c5ed10fdfa523149e706`,
`a57f22f6ac5f974a45b5460b5e2315451558365fbe0208546ac99e24702d3653`,
`3d2fc212c0547c0886316ea971aabb4695367e9e1b84ef91b897628e26145086`,
`a999b27816f29486f215550f9c3826f276dfec9bbcd01ba8681469f5169391af`,
`079c632e81949773a2cac2ada1491ec2e8efbc6f1ca02835e8b132d071e5bb0e`,
`0278b8ff7bee4fe1dd50d98a418f90d78beeb44e2cfdd17843e236f89931469e`, and
`56410c9ecb7cc8bdfd7978436c0263f73e3fb191333b3e6fa30038684dab5cc0`.
The stock MAIN, NOR and QEMU identities matched the preceding experiment.

Further disposable tracing recorded the arguments at the stock MIDI sender
and the return values of its two output gates. The sender received internal
controller `0x5c` with values `0x2b`, `0x2b`, and `0x40`; both gates returned
zero on all three calls. The modeled LPUART3 trace still contained only the
two `b0 75 2b` messages. Thus the missing Spring bytes were not explained by
the observed sender arguments or either gate result in this run. The result
does not identify whether the later enqueue, UART path, or instrumentation
timing caused the absence.

The sender-only variant's callback, preparer, linker, MAIN, ELF, recipe,
result and trace SHA-256 values were respectively
`692061f5644a2dc256de22641b708ef0e63dcf5f03f8820c6077e824ba2e0492`,
`4f288aa5957c1f5c004603a1bee3ca6c3029ae9d69694fe9f362facf9d7dda06`,
`793296aaad37b0c4adabdf558b21d8fa37216cf86a5a116d25fdb654e7ef066c`,
`ad0e29f9a9e77bb18a95feef665d55631addb2456204dedac398e55fe8206cb9`,
`195bb76c67c0cfb643df1d31013d3d9ed89c328203dd539de90ad37259e216c7`,
`5b6cbf8a0ae3a805f87e5d5b9e52a599080efbcc74b341e1ed6e492358c9696e`,
`b181060a6b572af7b3a9468d4a02cfed8073b59c7645d1f8e94fa8102317b51c`, and
`501388106078a6e14d3f0d5a225c12c4346c65c6169120255d89cc08611ed401`.
The gate-traced variant's corresponding hashes were
`d80cbe7dc4a3e3d3219d0d6dc198dca945ca4943a0c1f0ca8d01b4625d1497b1`,
`62f87386e8a0d0fcb0e8be9280890fb4bbbb5854a959d2a1c6341c83ef7864bd`,
`e45fca2ff4054a10d3ff4e229537232614ab6a70d66fe39c10e0800475b7b14b`,
`960e57c15b84adaab566acf22b9851b772aad17d08eb7569e6ad7f6bdc6a27c9`,
`a7b1a80a5887d1c2983900f3f965cdf97ff6dafbc57c3dc4cd27de8b49b7c3f2`,
`cf2fe605a67f2e9a34c6337ec1e223a0a2765e66595754aaf6978f7fa451a3a5`,
`71b35ff08be6f629c1dbaf2bf9e6f6b2ebb9345ab511224fcd2b560efefa4253`, and
`ef41a8e78ae50690ee7af42dea4e2ceed4df7cd740c3b755ba0abe630cab2335`.
Both used the preceding stock MAIN, NOR and QEMU identities.

A disposable trace around the later stock enqueue call recorded all three
packets. The first two were `b0 75 2b`; the third was `b0 75 40`. In the
third call the observed write index advanced from 6 to 9 and the read index
was 6 in a 2,048-byte ring. After an additional fifteen guest seconds in a
repeat, the indices were still `(9, 6)`, and bytes at ring offsets 6–8 were
`b0 75 40`. The UART trace still contained only the first two messages. Thus
the Spring packet was pending in this modeled queue rather than absent from
the enqueue call. This does not establish why it was not drained, or whether
the behavior occurs without the modified priorities, DSP workload, moved
call, and tracing.

The queue trace's callback, preparer, linker, MAIN and ELF SHA-256 values were
`b7f547e9f518189a7d71bef27e6bc3ea629cecc40023bde45f269affb78fae5b`,
`74ce6243b32c278743a3926a55a8bd28610e6b247f1c582cf22ad12954c1e6f2`,
`0b07e9ac9ccb7ddfaff6980cac90e40426e4bcb7463bc21c3d07f778f83ee487`,
`58011193c9a4c7ce11380e50dac9118a58aeff0ce5414f8b65e2c51239b40148`, and
`13c9d03ea1ac9f7e7c772856a03eabda8f59a0c4b4a338f6addad09b315b770f`.
The first recipe, result and trace hashes were
`3fc002155a56e22663270641eae0816f7fbc3e13f470d121417bddba69b3fb17`,
`f7e9beee019a1928eaf2da50f420db807fb7e2298f0ad786e0fbc3564174dcd2`, and
`db3d726c8f6df17534fdeed6d275c3726a53c8663c3f383966228c65e5774a6f`;
the repeat's were
`105d9061a58e98ba6979e11f7b65fe8add00f786b0daf79c725be7d2c867c5d3`,
`0d04c6e95d5addd8d4e09e2c449df6508af885b55bf460a7b77396e0c3f1fdee`, and
`acb1601d165c640dca5107e1e59f78464ed3726e92e6c17d63681f1b43b05061`.
The stock MAIN, NOR and QEMU identities matched the preceding experiments.

The separate research checkout identifies `0x80050bd8` as the priority-18
DIN MIDI transmit task. In a debugger snapshot with the Spring packet pending,
the observed `UartMidiSendMsg` TCB was at `0x808e2860`, priority 18. Its saved
stack contained `0x800911a2` in the stock semaphore-take routine and return
address `0x80050c3b`, corresponding to the wait at `0x80050c36`. The queue
indices were still `(9, 6)`. This is one perturbed snapshot, not proof that
the task never ran or that a specific wakeup was lost. Static inspection
shows that the task checks a byte at `0x805e5d56` before servicing the second
ring. A separate recipe sampled that byte and the parser byte at `0x805e5d46`
as zero before and after the failed selection and after the longer wait; the
second-ring indices remained `(9, 6)`. Neither byte alone explains the
pending packet. Wakeup and ring-check ordering require further investigation.

The task snapshot used the preceding queue-trace MAIN and ELF. Its recipe,
result, debugger script and debugger log SHA-256 values were
`22fcdc106ad657438a85470617fd088a6d36ffb8753272ce8459ade227f764f0`,
`15e157c17866b96fe031782df326cb1bb37d4496e2011009fc77f634e053e7a5`,
`0039e77072ca6a1801290def6066b428d8460e6a23c91bab5a08be7f30b95cbb`, and
`b699cbc34203252389a0406d77847d2f8d1b4a2ffdfe601a509e046e206f8a38`.
The flag recipe, result and engine trace hashes were
`cc9476834a11c5ba026da016c2a5dd3d1505ce81a7c2d270132fdc558925ba43`,
`89d952db8838bf0254be577fd37514b204eeb26d496afbbf8230313b535555c4`, and
`bdbd826f2e7236f1ac86e481581656070c94c15604ed18dafd09d1ae2a342466`.
The research priority and DIN MIDI documents had hashes
`a5d60eb9403eb06ba3b9da60a64b8d91f077bbc4e0c742d7d3de3554a0eff644` and
`da7ec6079b6e72812969f68b532e99e6a3765bf42ed6e5a8e32f940a1f595bca`.
Stock MAIN, NOR and QEMU identities matched the preceding experiments.

A further QEMU-only intervention delayed 100 ticks after the third enqueue
and attempted an extra `xQueueGenericSend` to the wake semaphore observed at
`0x8020a86c`. The object pointer was `0x808df5d0`; the call returned zero,
the ring remained `(9, 6)`, and no Spring UART bytes were recorded. A failed
extra signal does not show that a wake was delivered, and is not a viable
firmware remedy. Static disassembly places the prior saved task PC
`0x800911a2` immediately after a scheduler-yield request inside the take
routine. The saved PC alone does not prove whether the task was blocked or
ready at the snapshot. These observations leave scheduling and semaphore
state unresolved and do not establish behavior on hardware.

The intervention's callback, preparer, linker, MAIN, ELF, recipe, result
and engine trace SHA-256 values were respectively
`f0b8046fe2894ca8da07132b3629b7c98ef89e9ccd4c627d0c400e8f5e7b2cc9`,
`74ce6243b32c278743a3926a55a8bd28610e6b247f1c582cf22ad12954c1e6f2`,
`0b07e9ac9ccb7ddfaff6980cac90e40426e4bcb7463bc21c3d07f778f83ee487`,
`e27ac1720611fef64e9bad0a602a3cdc30b1a570ea915fd9862f0b6f7bf99fce`,
`adade5d320eb50bc34bac8869c0f74804e1e887198b376e6ca36ac118169930a`,
`1439e2abbe1584dfda1979f922e29c523cf7a3df46b86c0f1e0dcfeee48e2d82`,
`b393fb4c8d49a38faeadef02343498b66d971999a98f851a3bc5fd9b8d71755d`, and
`cb46e9fa510dc72f98922658b247bde06fd65826d74ac53cae55c097fb2d8776`.
The stock MAIN, NOR and QEMU identities matched the preceding experiments.

A fresh debugger snapshot with the packet pending showed `UartMidiSendMsg` at
priority 18 on the observed priority-18 ready list (`0x808a3c5c`) with no
event-list wait. `AudioProcess` was on the priority-30 ready list and current;
the MIDI wake semaphore held one signal, and the packet ring was `(9, 6)`.
This supports a ready-task scheduling delay in that snapshot, not a proof of
continuous starvation or physical timing. In a separate disposable run,
the diagnostic checked the MIDI task name and priority before raising it from
18 to 31 after the third enqueue. The queue then reached `(9, 9)` and the
modeled UART emitted `b0 75 40` after the two Room messages. This intervention
also changes scheduling and is not a recommended target priority: audio,
other tasks, interrupts and deadlines remain unqualified.

The snapshot recipe, result, debugger script and log SHA-256 values were
`22fcdc106ad657438a85470617fd088a6d36ffb8753272ce8459ade227f764f0`,
`7e3d841d5359956a36df22f884c7caa6299a434b36dfcc3834377d53adbf7fd9`,
`779494cec9ab602f1b23292ffbfe427f814c9cae6e980c2cc03e856490599460`, and
`82deff655ec87f120d6ce3420b0a9a97fd0ba40ab782d3ff0d2be102bb0571f9`.
The priority variant's callback, preparer, linker, MAIN, ELF, recipe, result
and trace hashes were respectively
`818c1f863cbca6e9557c51ede5865f726be6973b8c395fde6aae71f336ff1e4e`,
`74ce6243b32c278743a3926a55a8bd28610e6b247f1c582cf22ad12954c1e6f2`,
`0b07e9ac9ccb7ddfaff6980cac90e40426e4bcb7463bc21c3d07f778f83ee487`,
`6ddf31e97438eaba5124c8f122d11392a4bc05ad62bfb6a29f3c5171a15477d3`,
`d6fc74bb9cff3455a88de05b8454528e90c374e3644914f9927c6b57314320fa`,
`87aa817fe7aebd41d76a2d6bf655d058cf82ab50eb6971dbdabb1101985cb070`,
`509e6260706f112b9f2b0ea17d1b48a0a301cea792e78a825f2178d0c8d85f98`, and
`c1b20b774ec00b6273564727394f4f8436185c107ab5000b14e5dd2c2d0e1311`.
Stock MAIN, NOR and QEMU identities matched the preceding experiments.

The QEMU-only interrupt-timestamp hook now initializes its decoder and queue
only on first installation. On subsequent custom selections, it preserves
pending edges if both hook words and the veneer match; a mismatch marks the
trace invalid. In matched disposable variants, a synthetic queued press with
tick 12,345 was inserted immediately before the Room-to-Spring installation.
The earlier installer changed the queue from `(1, 0, 0)` to `(0, 0, 0)`;
the updated variant retained `(1, 0, 0)` and the timestamp. This is a forced
interleaving, not proof of physical input delivery, queue overflow recovery,
or target-safe ownership of the hook and scratch memory.

The two variants used callback SHA-256
`c1fdb06cdabeed391d2525f01fc342a548120efbeb83cd58914f0eefd663e987`;
the earlier IRQ source, MAIN, ELF, recipe and result hashes were
`b59870a37e8924359af842cbd3c9fa69fd0615e3a181f8f47cdbf6a4152089e0`,
`06fd7b193aebc2c8e86a19427967fe31ea164f8887b42ea5b4ca59a6c4d9d1f3`,
`3c7d929b0a304f8e898afc875ef30ee198221952872a681a97c2a8119919823b`,
`fb9cf942b85c356a6e744735973ed7da6118e60ada5bf22bf3220e35644ba6db`, and
`97390ae7c929e69aa2674f3f60feb98915974025b20f658594efa4345dd17f67`.
The updated variant's corresponding hashes were
`e6428e0cd811e5955666e003246d0e2ae2ca4b49882ad865a0e3b2e4f9201775`,
`c935944d97a00220565eb62f6c9b6faefef827adbbcf8c5ba2d732b031863a9e`,
`918fa5152476167af62ffccfc00801ea1cb99a5d33fd49334ac85d43a14cfacf`,
`7f734b182428492f514eb1a89341bdb592b92f3756c8df4aa80493f8d7a6a197`, and
`d90cced7a8b0b693318657ea1117ef7f52759be75b1a00a9f980737bf590d373`.

In a separate silent-input ten-edit diagnostic using the updated checked-in
hook, the inspector opened, recorded ten edits, closed, and selected Spring;
the IRQ queue observation before the last short press was `(25, 25, 0)`.
This does not establish active-audio responsiveness. The diagnostic MAIN,
ELF, recipe and result hashes were
`63ec0f611f40001baf11270ea3f4172c97f140ae58dfc129544ce69f1f38a95a`,
`b662bb3155deb8904c553af3e56cdec01dcdc0202456ad578b97b24ac4ebf911`,
`8889981a8bac834912985ca862fdca9bc9028aa7e13470f5e37be77874b8c3a2`, and
`40f27f99e48b3b9ed0c9bf9b57e09db9d6989e86b6d0d2a15a2fb3d29bfcf1b5`.
All used the stock MAIN, initial NOR and unattested QEMU executable recorded
in the parameter-mailbox run below.

Another disposable variant changed the installed veneer target immediately
before the second installation with interrupts masked, then restored it.
The installer recorded the invalid-hook marker `0x42414421` instead of
accepting the changed target. The callback, IRQ source, MAIN, ELF, recipe
and result hashes were
`a7965f21ecaaa335860d9588459e81fa03527be6e46ada99469fcd55f8b1acaa`,
`e6428e0cd811e5955666e003246d0e2ae2ca4b49882ad865a0e3b2e4f9201775`,
`d75f74d980f7c7ba71063046688a210bd06fe6b66b65cc5e78c9f317ce27d337`,
`9ea440320427d33d16635983aded6201929c1b783f76be7d68d953fcb553d434`,
`6ce1f82c1fd3384c3d80ebd0f7181346c9706d1a40f83d9764002ea7857b6ed6`, and
`81279f6a508f2e0832c775aed2465066856101d7076d1d6d1f435ab56bcd63a3`.

Two temporary attempts to reduce the shared Velvet tap-loop cost did not
improve the modeled active-send cadence. Unrolling the aligned two-history
loop by two, without changing tap order, counted 2,615 callbacks over five
guest seconds versus 2,829 for a matching baseline. Forcing the shared-mono
method to inline removed its separate compiled function but counted 2,825.
The diagnostic text sizes were 23,715, 22,611 and 22,767 bytes respectively.
These single emulator observations do not measure physical cycles or prove a
deadline; neither change was adopted. For each variant, host comparisons
matched 153,600 frames through parameter edits, fades, silence and wraps, and
another 70,000 frames with zero, odd, even and maximum tap counts and
out-of-range delays. Host equivalence does not establish target equivalence.

The baseline MAIN, ELF, recipe and result hashes were
`3201cb4c042daa9c84367ff9ea88f83ded751d3789cffd72b7e005cc69c6ad51`,
`70fd5adc557a84e83b3a5b22f8234377397557b7ba6f1fd723a03702c88d77e7`,
`c4f26838e7cb35e9244edf19c22b73e1864690b82e7349ff145a66bf50dd63c6`, and
`154d31567d22f11ed7ca466b7826f22130ffd649c7393bd66fd2ebb59f00b530`.
The unrolled variant's corresponding hashes were
`71e7a881ef40b2c1d3f38638c615861b2bfac28c2bdbd0ebc7f341794bb579be`,
`02125c72cd23c7776d91b6f9fdf7908b268d0ace6b9bfc4b2b666d1449216410`,
`a6b35199893b0e98222160ea0e1b6172be6fa65ffa36cb1a59a13db77c5a39e9`, and
`b1bc63c789699036a3dfed19cfb4b433e51f81a8cffd4b31317ee4919dc61559`.
The inline variant's corresponding hashes were
`1d58d73ada492e526fa73a730fa4e80a3d48f101b39cb035d7094d9062a48f11`,
`dbb51e8048b4a4bca93719c259bfbb31038191dc89a77a23996a18587a0f5222`,
`23c553c99310fe23953824d1e6cc99149934c1fa9c9e5efcdf7365752b11e75c`, and
`51898b53a5a4e196fdab1d77191a82540dc3f5b063b8d4ed7738617358b7a89f`.
The unrolled and inline Velvet source hashes were respectively
`6efc834fee58108167bf0ddc0b7422520b5fabd8bbbcbb3869562310fd28dbc2` and
`609bfb349a136ae5495cfbd9e605cd79ca6cac2f193b61b82a736dd1fe9e1c80`.
The stock MAIN, initial NOR and unattested QEMU executable match the
parameter-mailbox run below.

The diagnostic's mixer reconstruction now rejects each invalid or extreme
weighted channel contribution before adding it to a send, using the same
guard as the weighted-channel DSP path. Previously, a single NaN contribution
could contaminate the accumulated send, causing its later input guard to
discard valid channels on that bus too.

In matched disposable QEMU variants, two instrumented channel contributions
on bus B supplied `0.5` and NaN at the same frame. The original summation
captured NaN; applying the per-contribution guard captured `0.5`. Both reached
the Room callback. The injection substitutes contributions in the diagnostic
and is not evidence of physical mixer behavior or hardware safety. The stock
MAIN, initial NOR and unattested QEMU executable match the parameter-mailbox
run below. The original callback, diagnostic MAIN, ELF, recipe and result
SHA-256 values were
`1171d611cee8276ad6a7d9807affe31a8de94016f4235bf156b49475ced9c588`,
`5b4f2a4c1d5d9510e84d7b10e9fb34b93e66d9e73f7a5f5102d50d6e073eac13`,
`217957a9be105f08e1eafd87408006df943b60ef8e4c84e60156f5545038a058`,
`eabf7ea1ab026a6f3ec1dfc85d41e21cee1d082ae60abe21e6a3534951a385bc`, and
`3cf1de026ebc5288735739ccdf4a3c1b88383107a0a64d174c706507f44c1a4e`.
The guarded variant's corresponding values were
`767df427983e892823609ab0d338029d36b7b66a8ded545ed71fbb86ae9d7417`,
`cbe25c9ca86c28fa9d5376e7368f49c0022599c881a83e820a5cbcfacc208edb`,
`d4b4ab40ec36f90e305199061ef96effec4f68c42ffc97717de12c6d45af6aaf`,
`dbc26aa997f2057c63cc44b1c00f7293e34e9b79ca43f4125b12ab2f70a4ef0d`, and
`dd38fdf2b8317a9208dada2ef51db923e935de76cb18ebfc2ca22e414bbae1df`.

In separate five-guest-second active-sends observations, the unguarded build
counted 2,755 callbacks and the guarded build 2,729. Both selected Room and
reported finite nonzero final samples. These single observations include the
diagnostic's mixer reconstruction before its synthetic buses overwrite the
results; they do not establish physical execution time or the control
latency, and both rates remain below nominal modeled cadence. The unguarded
callback, MAIN, ELF, recipe and result hashes were
`94ad859a426051163917a86430a07227a48fa33f088bcecb9d64d45e74e22a76`,
`879060f60abf307feaee256bd5628a996a7541ab87fc2c3bd6c7db3fd7b8dc09`,
`14ccccccddf070842cdc6a6e0c3895ebd6cb53aeff4d1947b7b65f83ed5132d7`,
`c1953d64bca19a5c4dae6daf97e7b9cff4027a34ebc163c29b9416a2778388cd`, and
`efcf895ab56f1f93347725159258865975a3a482b7bd8a5144f6f9d8fb70a471`.
The guarded variant's corresponding hashes were
`a8faf6592a959799d959547946b0793ed84e66872594bd98ed92d9b343b78c27`,
`113a4145031f8adca102082890934cc6338d7921f4c5756baabce3155b7e639d`,
`d552ce02dfa458263c9d4c8063eb5894f166616fef6cf9c598d5b88d17849882`,
`c3da19598daae4f4afb639b2588e1ef69ffacb5ac699683fca684efed60ce855`, and
`cd6257b46294f3c4bd2a062fd1dc385ce14293427652a682786b8e604cf746b9`.

The diagnostic now skips a channel's per-frame reconstruction when its
fixed gain is zero and its per-frame ramp is inactive. An instrumented
active-sends run found all 12 fixed gains zero in this fixture and counted
2,831 callbacks over five guest seconds, compared with 2,729 in the guarded
run above. The instrumentation adds writes and the synthetic sends overwrite
the reconstructed buses; the result does not predict performance with real
active input, establish physical timing, or by itself resolve the UI stall.
The instrumented callback, MAIN, ELF, recipe and result hashes were
`838516f4cb5e1dfe42ebd0cf11614d29f7a98bc7b33639467c0f3604a989a8d9`,
`a921594289e1e62af81cdafc4ca9a962150e8b5431882c9c68aeb2d24536e040`,
`9c36dbe88d5f281810e793b2b526011dcac8562d9c6ee2cfc571760cce721fb3`,
`4fa0e9af10030c3cf08629b10934d77ed812e4bd5ed131c3ccd0ee91f88fb941`, and
`3ece9448007521754e7e5100e339659fcde15ebe21c9ffe5e26adfe3d34dbfae`.

In matched instrumented runs with controlled fixed and ramped gains,
including zero, negative zero, NaN and infinity, the three captured 64-frame
send blocks matched byte for byte with and without the shortcut, with combined
SHA-256 `b6dac7ae4b3eb2e519f7476abfe8f6c9bbf45d79e458bc6924d04d5d51de66a4`.
This is a bounded input comparison, not proof for every floating-point state
or concurrent mixer update. The reference callback, MAIN, ELF, recipe and
result hashes were
`de1701d4610aab2e552c1c31d16ecad8009e230a171e10587c0a2470c0565f7b`,
`f55baf0a86912dd20ef248f794bd010f0a403aa6eca438118da2ba1bee7ec52c`,
`d325aa01ae42ebc1b9923fc40b8e2ee5a4cd420887b16e417d4902bc691ca710`,
`ced66a3944b91119d37ffcac12e49d426d6398705584c5f8529f03811ed20797`, and
`e4c0887186a9f12104b81e4903861c14cb063116c9c4c7922f63a9503ac892fd`.
The shortcut variant's corresponding hashes were
`0e005f2bb1487852f1502c78771cddba228fe00ff18f4aeed9386e78d7472bbb`,
`8ec7b76be30e1ad5c6ca52d8452800cb52e80101f6ff79f1e0364ab08eb56758`,
`a09281c5b2b16536402e8eee0b60898032aa8c1499fbbe37fddab88fabc2da9f`,
`c3e8aa7a5af79370ccaf9dbb7c6f69542c2640a5b44aaa9f4aa09ee0303a0404`, and
`653f2a4ffda75137b95d689255cb2bfdafff98536b863eadf94141ca207ae354`.
Both used the same stock MAIN, NOR and unattested QEMU executable as above.

In a separate active-sends ten-edit run of a disposable shortcut variant,
the inspector remained closed, none of the ten edits was recorded, and the
selected descriptor remained Room after the final short press. The callback
continued to report finite nonzero output. The runner's successful completion
is therefore not an interaction pass. The callback, MAIN, ELF, recipe and
result hashes were
`d32011b1d94f78f30ce4ca6031380ace33e2de2e927e4e0ec2c8b032c08261cc`,
`16508b75cb98cab82bcc83b001b7cf7ccbc7d81dab9721af6a5077942c7c4df6`,
`f605907f14d0e2f28a400a19da8dbee8b659f929455b665a63996aa9b1e6cea6`,
`a6258e21ea852ef27f40535974e7cb9bac7d4d136f5c2163c046b10d0913a011`, and
`acb0bb1e98bbf932b6900a1a7aea4a5304ade05229755d24c745ec3d663ba151`.

A disposable experiment tested retaining the initialized processor when
switching directly from Room to Spring, instead of clearing the stock effect
buffer and rebuilding the processor. It injected a single impulse into bus B
and then silence. With both `Main` and `SubMcuRX` temporarily raised to 31,
the Room peak was 0.0417875 and the post-switch Spring peak was 0.000886481.
An otherwise matched reset control had the same Room peak and zero Spring
peak. Peaks were attributed to the effect selected when each callback began;
this distinguishes a callback that straddles a selection from a subsequent
Spring callback. The experiment supports retaining history as an approach to
adjacent custom-effect tails, but is not a hardware qualification.

Both runs observed one callback whose selection changed between entry and
exit. The raised-priority selector can therefore run while audio is active,
and its access to the same processor storage can overlap the callback. This
invalidates the temporary priority intervention and state-reuse prototype as
safe implementations; avoiding a clear alone does not establish exclusive
ownership, safe publication or deadline behavior. A transition to a stock
effect still clears and reuses the stock effect buffer. Neither experimental
variant is included in the firmware builder.

The stock MAIN, initial NOR and QEMU identities were the same as in the
parameter-mailbox run below. The state-reuse callback, MAIN, ELF, recipe and
result SHA-256 values were respectively
`34310fbbd2447e1161b44b9b0787e5c385291f02956f7b10c2d0eed70ba14478`,
`c0637b237fbc4434b7e325274732559db8f0e3ca17f00043813a15f614f8a833`,
`71eacbb8d61d1667fa15ed13425eb05aa75e233b3d6e75b6e693a47eefdeefd2`,
`5f12776631fb36f59d3f5598c2edc45e5694ee664b67e39086f80b808792e877`, and
`dbe290db34eb7f1991ed589ebc323ec36c44aad0cbf30ae2b2b02d88fedca758`.
The reset control's corresponding hashes were
`cfcf5274ee8cd4605303e4b52ab87eb25913c0620764de17a68bfeec2a3c3fc4`,
`131eb14ff9cb7a7f2cd2dc16e19bcce559613070259b37349a122cd2f4fe9441`,
`0aae3400b50801c8eafef1d2ee3b74cc541f598bff677c5bf6ea5ae65ac100d3`,
`80d2666c449d13f15f388d592dd748ed6f4f74fb06e1b943456c40b86c140869`, and
`20d0e45a69211fc8d1e84e11e63fd3f7bc5902a12b90ea994ed7af86a84e6124`.

A follow-up disposable variant avoids borrowing the processor and avoids
resetting the event queue during direct custom-to-custom selection. With the
same temporary task priorities and impulse, it again observed a callback
spanning the switch and a subsequent Spring peak of 0.000889120. A second
variant inserted one encoded Velvet edit into an empty queue inside the
selector, after the stock callback was disabled. After selection the queue was
`(head, tail, overflow) = (1, 1, 0)` and the applied-edit count was one. This
tests a synthetic pending edit, not physical encoder delivery. The experiment
does not resolve safe entry from or exit to stock effects, scratch ownership,
reset behavior, or the priority intervention. The opt-in
`--retain-custom-state` QEMU diagnostic implements the direct-switch approach;
it is not enabled in a hardware package.

The first variant's callback, inspector, MAIN, ELF, recipe and result hashes
were respectively
`3ba1083c2bbed39fa280e0c18d6f2b08493f775535ca0efc3ef6587f8b30db1f`,
`a31e928ed6dcc89327a0997ef704e9abbc0736bcbeb54c82def639beb67d3014`,
`99ac580ded33b1e58579102a844cba089d9f9d9177330413993faf061470dd42`,
`2cf13f231ba43fa4b3b73aff5e4843e348c5342ee2d7e40b2089dd9c3bc78f70`,
`2a708a4d0c58d9f918ebe51805efd7b34a532050e118e23c6d04a504e13c82dc`, and
`3a3b638ed160236d2a84949ad270ba3bc1c12eb339e219b1e0df1b5b80516cbc`.
The second used the same inspector and had callback, MAIN, ELF, recipe and
result hashes
`6a452b3988429e180e25e4a60102179fc4140a29450c4f3bf81b0b85c9fd69e3`,
`85b14eac06ff95bc34fa66e5f19c21f3ecf3f39c7a28469dac8e7b1f53b149b5`,
`124e461bf7ccc8ea969575427fd13751b1dea48dbdbc30d09ae1a3ee161889cf`,
`fb6c5494fa102bdd53fda0590ba920cb6b41293bfeea91a33b9b6e930d3df26e`, and
`95fb990bf90feddcce8bf6af32368098083d5ef4f3872f58a4235f2d5f12b227`.

In a separate silent-input run of the opt-in diagnostic at stock task
priorities, the inspector edited Plateau pre-delay to 26 ms and Velvet amount
to 84%, revisited them at 28 ms and 86%, closed, and selected Spring. The
queue observations after the first two edits were `(8, 8, 0)` and `(16, 16,
0)`; normal mixer input changed after closing the inspector. This checks the
observed UI sequence and direct selection, not a tail or active-audio timing.
The diagnostic MAIN, ELF, recipe and result hashes were
`6ad3d47746928113fb74a2e3bcaff08400edc9b2e67bbb158754fb8dbe6edb97`,
`2473a3f38576031c5c4d99c2f0fc15a82d46e36d41b472116f051f679df0a3c0`,
`8d6b4028620531868b1098fa270eec2a22046a828d3a8c30140981724c4f296a`, and
`3981d81db07a4064aa3bc4b86b6e4a60280064a1939a432c14278ea41fca5e6d`.
It used the same stock MAIN, initial NOR and unattested QEMU executable as the
experiments above.

The inspector diagnostic now preserves its queue during a later custom
selection even when a stock effect has cleared the processor. In matched
disposable post-fade-handshake runs, a synthetic Velvet amount decrement was
inserted into an empty queue after quiescence and before selecting Delay. The
baseline returned to Room with queue `(0, 0, 0)` and no saved amount. With
the queue preserved, it returned with `(1, 1, 0)`, one applied edit, and a
saved amount of 98%. This proves only that forced interleaving in the modeled
session: the handshake and memory remain experimental, and it does not
qualify reset, concurrent callbacks, overflow or physical event delivery.

The shared callback, preparer and linker SHA-256 values were
`c13dba918605c9b87ba20dbef379eb73ea874470c17d49bbafdb85bd2f1a928e`,
`405efeb201ecefebc118423839508fb49d09e74188ee1c854b0e3932b3a0a2ad`, and
`b0a558813276bc9b6105b7348dc5e0b23b244922b9a47e7d553d8390a97a3a1f`.
The baseline inspector, MAIN, ELF, recipe and result hashes were
`c0787d4731a24981097eb521ec6a17673600770d7a0cf99336a0280d0d323300`,
`f90dad3de996e9a01876404a153583528b46ea05f614427ff74eb776fbc432ee`,
`5cab3077407add1f5ca416596583774c4380513d670f22103fba06f75233082a`,
`aca0893fff15dd69a0f4cdd5d44251daa9015ecd75dd0cc9a0c9638b08773421`, and
`f2da459f6f57884d9d3a8e4182e02c36c29357f5fe0ad07b88f2352396cc4862`;
the preserved-queue variant's corresponding hashes were
`5ab62545e4ee49b1a8250f55f1e794d1d89799210e19b7aac125122d365a7437`,
`77956b2116b2fedbe1b6eb10e854e581fe41288bf5aecb79fbc7d277d7e43e4a`,
`394c52e6cb609f1d054fcc9c31f90b84111e351d0742756fca2df1f2e944ee33`,
`c50f00d45f499584049bfd4fb01e93c8dd71e1959665a14862ad29f6bda5bab9`, and
`a221f456fcd67168ccd8dc10124994f79bd0c022102401ef4e992f13831fc6cb`.
Both used the stock MAIN, NOR and QEMU identities recorded below.

A further forced interleaving inserted nine queued edits before the stock
transition, with empty queue counters set to `0xfffffffc` so both counters
wrapped. After returning to Room, the queue was `(5, 5, 0)` and the applied
count was nine. Three Velvet amount decrements followed by three increments
left the saved amount at 100%; three routing increments moved strip 1 from B
to A. The count exceeds the eight edits consumed per callback. These final
values are consistent with the requested sequence, but do not independently
trace every intermediate edit or qualify physical input, concurrency, reset
or overflow.

The callback, inspector, preparer, linker, MAIN, ELF, recipe and result
SHA-256 values for this run were respectively
`4c16484271523afb3c53a39c782108c7198c12818edbd00ba9927f7cc20681ba`,
`ed9f00c4c5a0c81edc784579f824af5f78bdc86d9c4485ada7a1164c9cc6aaf5`,
`405efeb201ecefebc118423839508fb49d09e74188ee1c854b0e3932b3a0a2ad`,
`b0a558813276bc9b6105b7348dc5e0b23b244922b9a47e7d553d8390a97a3a1f`,
`372a0755d30a230de83646813b1f813385453189f9a8d833b5548613455d2519`,
`d3e3659273343aafacfd7b530ff9507efa7a14f906e321ef37daedd38e88e320`,
`49aa4269c4dd20cca2f94069f37738d9a5355397830a2268e8a541dc01c607c2`, and
`fd48780147d7d7dc493ba32aace41572b1661cac1638f44608ed04f841f556d5`.
Stock MAIN, NOR and QEMU identities matched the preceding comparison.

A combined-popup-path probe replaced the notification hook with a QEMU-only
counter. After a synthetic pending edit survived the stock round trip, it
recorded one notification for Velvet amount 98% even though the inspector
open flag was zero. This demonstrates an unwanted stale notification request,
not a rendered popup. A disposable selection-boundary variant suppressed that
request while still applying the edit in the same sequence. It has not been
adopted: repeated selection, close/reopen races, concurrent
producer/consumer behavior and ownership of its two additional scratch words
remain untested. Suppressing a notification at consume time also does not by
itself cancel a notification already published before the inspector closed.

A separate close-without-selection probe forced a queued Velvet amount edit
at the point where a long press closed the inspector. After the queue drained,
the saved amount was 98% and the open flag was zero, but the notification
counter had advanced from the opening-page event to a second event for that
amount. The edit was injected at close to force the ordering, and the hook was
a counter rather than a renderer. This establishes a stale request in that
synthetic path and shows that selection-boundary suppression alone would not
cover closing the inspector without changing effects.

The modified callback, inspector, preparer and linker SHA-256 values were
`32edb0983e8b28b04889b4656d055e263914a03d4b5795d9dbeed8500f0160fc`,
`d27ae3ba5c89b6d99f8b2f1d906b453a0464175b47e5d9f76bf8218562989958`,
`f36f8d0d2a2f368c65aac46f925c72b85e8ff7566299b96910c56b3fae139690`, and
`43354e222703266f199b05aa202e682f9d217df229967c1cb8e8440ad2799751`.
The diagnostic MAIN, ELF, recipe and result SHA-256 values were
`362c583e53f750ac9a1e82a4e46a0b44a11237ae932f0a66869c5449510dbbd8`,
`2ed03af77dded5efdad50a003de88cbaded6ee4e2783c05c3ef30a557113d0de`,
`51a4e60ccfcba1a15de2ede10126e513b83b93dead674e4a786c19a61b377ef6`, and
`571f3f6a66d2e87168494b4dc1ab69359182735e687b2964e2cfd971dfd77c19`.
Stock MAIN, initial NOR and QEMU binary identities matched the earlier probe.

A disposable extension captured the queue boundary when the inspector closed
as well as when an effect was selected. In one forced close sequence, the
queued Velvet edit was applied at 98% without a stale notification; reopening
the inspector emitted a page notification and an encoder edit emitted a new
predelay notification at 8 ms. Both edits drained from the queue. The old edit
was injected just before closure and the notification hook was a counter, so
this is not a physical UI or renderer test. The extension remains unadopted:
the cutoff and flag can race with the consumer or another transition, already
published notifications are not canceled, and the extra scratch is unowned.

The cutoff protocol has a concrete possible lost-clear interleaving. Suppose
the UI reads head 1 and tail 0 and stores cutoff 1 while suppression is clear.
If audio then consumes that event before the UI stores its flag, audio can
publish the old notification and advance tail to 1. The UI can subsequently
store the flag as set using its stale tail 0 observation. A later event starts
at tail 1, advances it to 2, and cannot clear cutoff 1; its notification is
suppressed. This is a source-level interleaving, not an observed hardware
trace. The stock QEMU observations above put `Main` at priority 12 and
`AudioProcess` at priority 30; the research catalog identifies DisplayCycle2
at priority 14. That priority ordering is consistent with audio interrupting
the UI transition and the display task running before Main resumes. A fix
must synchronize notification validity and cancellation, not just sample two
queue counters and publish a separate flag.

The extended inspector, diagnostic MAIN, ELF, recipe and result SHA-256 values
were `43dd1b397519d4de6a663c03ba4d075d1b7ed6a41c975176182c3fe314e53c32`,
`3c1cdbce678f28d8d349f9b87ed83b0bffb585373206f1e8c3824af6a2a5601a`,
`b71401cea7f71dc57a7d9ab03e6029dc8004b6dc11b75029548a7098dd37cc25`,
`7f72704d58f8209ffb22a0471a5f0775eed69499dde15eb5c9efca0178e5a86f`, and
`f4dcf7a4245a8b6c059544afb86c0f2a23886973839ebab3ba2fd5e7aa4e3f7c`.
Its callback, preparer, linker and input identities matched the close probe.

The maintained combined QEMU diagnostic now instead tags each queued edit
with the popup manager generation and compares that tag while publishing under
the same critical section as cancellation. Closing the inspector or changing
its page cancels pending popup work and removes its current popup; the audio
task still applies queued DSP changes. In one forced-close build, an injected
old edit was applied at 98% without being published, the closed OLED snapshot
showed the normal screen, and a fresh edit after reopening published 8.0 ms.
The force-injection build added diagnostic counters and perturbed timing. A
separate run from the maintained builder exercised normal bank and routing
edits: the queue drained without overflow, the mixer control stayed at 42
while editing, and it changed after closing. The route changes and paired
strip update matched the requested turns. Those captures do not prove all
interleavings or valid physical event delivery.

The maintained MAIN, effect ELF, popup ELF, recipe and result SHA-256 values
were `4bac61ea0435ddb3b3a13ffa800884e59699840b68ec080dd2d15423579781b0`,
`04aaed0cceba256f3c4a0ca1663a59f32fc319256436cc96003abec3fb613313`,
`be47cda2c3cfc74d0e0761853e257b0f64bacb1b0a4269b32e1f0582f72bc85b`,
`4796f8ec2d0b0c039b40a5b7205138f5e736ea66e43381c419918f15463f8c10`, and
`ca5afbe5800d26e10bc286f88da7d8b2695b338f303057d60216242396f10cdc`.
The forced-build MAIN, effect ELF, popup ELF, recipe and result hashes were
`8c8a17e46d6cc13332e904c89bdad7dbf27fce77b101110c51a58d1b0d3822e5`,
`38dbd8957a534e6e9f799db6228cdae2c3821f7575e27f03d2502e7642d5ee2a`,
`8402eb6e4473cd870f2fa54e09bc127f12fd4c27dfd3ed35cd574b1cf2000989`,
`14eabf722be6faa68205b3d5431ae40b7feaa257c1d22019640ba8572152734c`, and
`0c7382d18fc50b10348d20db50d329c7034b015310d8f666405a9cd082fd5f32`.
Its modified inspector, popup patch and builder hashes were
`f2790d09332fb71537be8f813bf1b50923ea1eac711e6d9207dfda9ae7a86a19`,
`63317a66b9ed0c1deece4a76610e3ff00e524f614ce4927f262c1a40bbb281f6`, and
`d45801cc4f3153de5828d24648468d44e6b3402462da0197d5a850cc07c24fc0`.
The stock MAIN, initial NOR and QEMU executable match the earlier probe.
The generation words remain unowned QEMU scratch; wrap, unrelated popup
cancellation, direct effect changes, overflow, callback lifetime, the cost
of critical sections on audio, and physical timing remain unqualified. This is
not an installable firmware fix.

A separate forced direct-selection probe invoked the stock selector for
effect 3 while the inspector was open. Before the diagnostic change, the
stock callback became active but the inspector's open flag remained set.
With the close operation applied at the observed stock selector hook, the
same forced sequence left the stock callback active and the inspector closed.
The selection was injected in a QEMU-only UI hook; it does not establish
physical MIDI behavior or every path into the selector. In particular, the
AINR branch bypasses that hook; the later diagnostic veneer below addresses
that observed branch only.

The baseline MAIN, effect ELF, recipe and result SHA-256 values were
`c3e75af0812687a3abf9a2a2b2206f0e8a4b73dbe080e9b376f9a1dce571c69b`,
`c4aeedb7e8faf9dc66020c864903cf832a0be07f0790b06155443752ab527dc1`,
`d269a33b564a7c0ad49536aea3379bc2879cfcc49aa920ce3cab02456cfac1da`, and
`c947c47b631ccafc13a664c08b8a333a0f740a967fdd0c951a54a0307f78643c`.
The fixed variant's corresponding hashes were
`ae1b6cbe0ba98b993adb9272fe52db159b2cb6b071d89adf82304d90709dc163`,
`a51b872b42ed0d397386c6ade780cdfb3d4a567593e6a4ffcfe0b53c9f17d195`,
`b9cbb2aca87e2d09cfd7b787eb0ec2b3162fb216858346c75ea032ac3f0b3531`, and
`b8d61e6598efe9c08f3c2cc5df237abdc4caadbad7b7dbd85852f6ff599b7640`.
Both used popup ELF
`be47cda2c3cfc74d0e0761853e257b0f64bacb1b0a4269b32e1f0582f72bc85b`
and the stock MAIN, NOR and QEMU identities above. The baseline callback and
injected inspector source hashes were
`8005848f6022a091c4bf7d82f1422c21dd9b6fb8c69e6422463806258d34d662` and
`45a394f29a2429ffe823191ffb98e0b19fc984a460eb9edcfccdccecbcc5e625`;
the fixed sources were
`2bce608eab780235328b401512f0195c32debf2e19470626bdab2454d18e9681` and
`036d5d80f58015f5a100c9f907e06632d35cebb0de83cb9a01b3c75026436b2d`.
An ordinary run from the maintained builder also completed its inspector
sequence: the queue drained, the mixer value stayed at 43 while editing and
changed after close, and the sampled routing changes matched the turns. Its
MAIN, effect ELF, recipe and result hashes were
`132c3b6a0629e64601806f9977e365c798e0c141e70de8345b72b9501fdc167f`,
`4d78bd6ca1b3080ab573ee4605eebbb270a8d09e9b7081decfb807bac1806edc`,
`c5a753ba3b83ab2d8f55ddc18ba940ce509a82936150ee9a9dceed7fa271099a`, and
`e80003704b8897cf70509af76047b583e1e3e69bac36e7b9332730587132eba1`;
its popup ELF matched the two forced runs.

A follow-up forced selection of AINR while the inspector was open. The
baseline left the inspector open after the AINR callback was published. A
QEMU-only post-fade veneer closed the inspector and preserved that callback
in the matched run. The aligned redirect replays the displaced register move
and call, restores the comparison flags, and returns to the original branch.
An earlier redirect at the halfword-aligned call site failed even the normal
selection sequence; that unsuccessful variant was discarded.

The maintained standalone diagnostic then cycled through both custom effects,
Delay, Echo, AINR, Hall and back to Room; the sampled callbacks followed that
sequence. The combined diagnostic completed its ordinary inspector sequence,
including its route changes and drained queues. These are individual emulator
observations, not proofs of all calling conventions or interleavings. The
selector holds a stock semaphore at the new hook; popup lock ordering,
callback lifetime, memory ownership and physical timing remain unqualified.

The AINR baseline MAIN, effect ELF, recipe and result hashes were
`2d4ac7499248ac50bb628c69dcc12440d96146434cdcb24842e3b972a4c243a5`,
`6b359f204bf00dec3bc2136a21b33319423bb8129c52d870ca8c027c41d20b3a`,
`3daf50564349ec83d75cd0d37b02484dc301d530a7a9386511151c4f5e970aa5`, and
`e9a014faac802f9219c28a579dd62d46564c291e833d84ce2bb66e3ff0c3d629`;
the forced fixed build's corresponding hashes were
`c8c427a54bfd8b3aa72d4829a878f57422b4831f7d5acbfc6e9b043a9de248c6`,
`f9c163f95a2caba33c377f8ff77c89c1d8220512430d3a2819fa239e2759148d`,
`0b681a21a84fe92b285961610d4fe82215879c6bbeb644c2a0d5e17636777c99`, and
`cd9d6a5a42611a8c5897689569588a648b6224968a300e89b383649593311002`.
Both popup ELFs were
`be47cda2c3cfc74d0e0761853e257b0f64bacb1b0a4269b32e1f0582f72bc85b`.
The baseline callback and builder sources were
`2bce608eab780235328b401512f0195c32debf2e19470626bdab2454d18e9681` and
`4a95fe9acd42e3eea8afba8a132b9b8b0e3dfde082f35daf2f4ef881a1106a26`;
the forced fixed callback, builder and linker sources were
`5c363b29da0631cbc9fca9325eb8ec34a7277c3d8bf93d6ca864933c3f28d159`,
`70205bd937400e8d52a811564f66dc691b9e85d2a63775b7bdfec7de52e409cc`, and
`537a04275854c4be32c77f3c180d818b6d4c927f88a9481f7550b22e0b072be3`.
The injected inspector source was shared by the two forced builds:
`c025e224234cca6a16f75d7e679e2b95bcd79b40d002cc00f5742ce0ab5fcb4f`.
The maintained cycle MAIN, ELF, recipe and result hashes were
`20abbc72830d0ea3175b9566cd898ccb40d0b072ba07740a3884f431e47a3167`,
`c36cc0fef8b3597fbb25a31ab5e70dcd406a08b5f3494cb4bce5a374f65e1b53`,
`149fd5f42d2649abc2b5dee473fde6ef27da39574ffc9824ea76c5a649428a2a`, and
`185f4e1d8b976f8370bd15aa7ee9929412ae4fd9a5963e6fe5661b2e6992319a`.
The maintained combined MAIN, effect ELF, recipe and result hashes were
`dab8ce314aaf3407f46e0776665682381aec8396e2b3c9df38d4681208afa2cd`,
`38e75941e9e6ddf467c708b7dbd4928b5d620ea4d724d1b1045c7dfffd3c5cc7`,
`760f1f4918081c9710b86b805b83b67d24c61eef93e78b5ed9dc91d17f81bcf2`, and
`c8d84bff3499ca51a5c3ff97ab21dd138290dcf1c021ac301b96dbee14736763`;
its popup ELF and stock input identities matched the earlier runs.

In a further run of the suppression variant, the old edit was applied without
a notification; opening the inspector then generated its page notification,
and a fresh encoder edit generated a notification for Room predelay at 8 ms.
The queue drained after both edits. This checks one sequential path using the
counter hook, not actual popup rendering or concurrent event handling. The
recipe and diagnostic result SHA-256 values were
`fcc24e103f26d85471773ff78e24f18b1af1f0b70c4e063e26368949a1cfdbc8` and
`b6ed03210b53295a6235a573382efd1e028d5dc3c94aaec36ba8f4f26515f228`;
the diagnostic MAIN, ELF and fixtures matched the suppression variant below.

A separate forced-batch variant injected nine old Velvet amount decrements
before the selection boundary and a synthetic Plateau size decrement after it,
before the callback resumed. The queue drained from ten pending edits to zero
across the eight-event callback limit. All ten edits were applied, and the
counter recorded one notification for the later size edit at 98%; the nine
old notifications were suppressed. The later edit was injected while the
inspector was closed, so this tests boundary accounting, not a valid physical
UI gesture or popup rendering. The modified callback, diagnostic MAIN, ELF,
recipe and result SHA-256 values were
`6eecd1f2d9f2f56253b0ef5b87e91d5487f96595fd2b726a71a286f008b3238c`,
`4019af27f43dfaef2d02d44cb72e1197cd859333d946b25b21cfb76c8faea2e7`,
`e6f138292d56ea603d6433c5ae778259ce2530b94c6b65d20aa5bf2e98f34a01`,
`e76dde9669f31d4ede003ec78b494687462cae10581bfa726a07527d48b75ee1` and
`abbf0d92750981408f2b6f5816bb1c26852f7b19ce6a113260efabbb4ddf9658`.
The inspector, preparer, linker and input identities matched the suppression
variant below; this deliberately forced sequence does not establish race safety.

The shared callback, preparer and linker SHA-256 values were
`cdb66cb544f9776d4b6b5513752b71259874bc9ba703baa4e8e25e4557a62f94`,
`24999952bfc0be07ada482839b70596f68abd4f90acbd62b913b44adb1db87b6`, and
`b0a558813276bc9b6105b7348dc5e0b23b244922b9a47e7d553d8390a97a3a1f`.
The baseline inspector, MAIN, ELF, recipe and result hashes were
`ed9f00c4c5a0c81edc784579f824af5f78bdc86d9c4485ada7a1164c9cc6aaf5`,
`b01603d8bc6778629e99a909324cdd6390c331e6edd45008d36f515521181095`,
`b140e0dcc07aed9d974fde129b9dc58eed3c6fd35b17d5005339c78921e8c36e`,
`de89d3b110301097579080340e3b9b608e51bbc036a474533afd48f29b181dcc`, and
`6ff807237909337c25a4aae3300e076e642f7c35b002ebb6135f86df70ed663e`;
the suppression variant's corresponding hashes were
`f29ca3f2e29bb52b0b306e55c422a33c92f6f4471986a4554e267fba4b59aed8`,
`c21571f70e3154f45dbf60a374288fcc501346052bca48dfee5fa33727d930e5`,
`e2ea4296475c7f433ebe1a6db0ffbd8fe3eb51c57afdeb8cf02f5aeda9e65aec`,
`0bfac5beac506a0afb186938c32242f67b0daf494f0a9d7050ccc5731595682b`, and
`f6860a880992f8f94751cbe5095400b7265a28e9ec3dd2d4413db4bcdfe10832`.
Both used the stock MAIN, NOR and QEMU identities recorded below.

The inspector diagnostic now records each applied parameter value in a
QEMU-only atomic mailbox and restores the observed values before publishing a
new custom callback. Values are stored in their native units; the diagnostic
rebuilds the saved Velvet filter on selection instead of crossfading from a
fresh default. Individual atomic words avoid reading live DSP state from the
selector, but they do not provide a transactional snapshot of edits racing a
selection. The mailbox is unowned scratch memory, queue overflow and reset
can still lose events, and DSP histories and tails are still cleared. Initialization
and filter generation add selection-time work. None of this qualifies a
hardware allocation, callback lifetime, reset, or deadline.

In one silent-input QEMU run, the inspector changed Plateau pre-delay from
10 to 26 ms and Velvet amount from 100% to 84%, closed the inspector and
selected Spring. After reopening, one turn advanced Velvet to 86%; switching
back to Plateau and turning once advanced pre-delay to 28 ms. The diagnostic
queue was empty `(2, 2, 0)` after those edits. A host test checks all 16
inspector parameters and repeated edits, but neither test checks tails,
concurrent selection, active-audio responsiveness or physical hardware.

The stock MAIN SHA-256 was
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
initial NOR `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
and QEMU executable `53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`
with unattested source provenance. The final diagnostic MAIN, ELF, recipe and
result SHA-256 values were respectively
`2382d31f9a8af011628742ac24f8475479997837a71315e715d3f8a6a5fefcd3`,
`82c9fad049d2f4ae5046f6e30d6cd34f44504ef7b66fe76ad594fd983ebfa2bc`,
`836358ec9f3dfdac50d22220541ddf30dfa3b8e7eeae4174e6a4cfc44654b50c`, and
`5e5c3994f5f8113cc1280596ee55d94e11c20aa30de4fb85b1db78ee74b7865f`.

The diagnostic used to reset its route mailbox on every custom-effect
selection. With a temporary raised-priority intervention and modeled input,
a stress run edited routes, changed both parameter banks, closed the inspector
and selected Spring; Spring reset all 12 routes to A. The diagnostic now
initializes its unowned route mailbox only on the first custom selection and
retains it on later selections. It still reinitializes DSP state and its event
queue: pending edits and effect tails can be lost. Reset behavior, ownership,
and synchronization with in-flight callbacks remain unqualified.

In a repeat of the active-input stress with that change, the edited route array
survived the Room-to-Spring switch. The extended sequence then selected stock
Delay but timed out before completing the remaining cycle; this is not a pass
for active-audio effect cycling. A separate silent-input run without the
priority intervention edited strip 1, selected Spring, cycled through Delay,
Echo, AINR and Hall, and returned to Room. Its route array remained unchanged
across both custom selections. These runs use disposable QEMU firmware and
do not qualify hardware or effect state persistence.

The active-input run used stock MAIN
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
initial NOR `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
and the PR-based modified QEMU executable
`71bab061900eb23bc8dfb75b9c27c1ede40e873731c1e55c254c25b1cdb1adf8` from
the panel comparison above. Its diagnostic callback, MAIN, ELF, recipe and
result SHA-256 values were respectively
`7922c2df26d3f52c91543497286d4e30bf0e771777c0c61c04171a785be5e083`,
`c51a8f48aa9450e995424241c8b93cdbcbb818ede8fb417bbb62f0fb676df4cf`,
`6a0583ca288784b55b156919380ad2c940883f32c7e2e9b78d401f6656876406`,
`43ea578947694460ecd7da2753bbeec555564a58ddfecc56f3f4b8b6363c15f6`, and
`c078c12620660c1e0f2cb628f7b0d2304c5cbb5cac9dddf2dc493600a8bba83c`.
The silent-input run used the same stock MAIN and NOR, and QEMU executable
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`.
Its diagnostic MAIN, ELF, recipe and result hashes were
`47bc4bbdcfbd9b5da00702f7b0d7907425e49b6d694ffd31dfe8495d4d1480ba`,
`05c2fc65ef97ed71f95b9fbb28b4d055aa40b22627667cd9d0b419bb9ea1cec3`,
`014e434b74def60afc8cd568646e962dbb11e4dcb815b24fb99395fb9a3c15f6`, and
`3c2582df4175ee9d0460391f4fcb07337e11fe2ff5fc03c61241c4b1f519c72b`.
The QEMU binaries' source provenance is unattested.

On the stock MAIN, a GDB stop at `0x80036f8a` observed the EFX button call
with `r0 = 6` and `r1 = 1` on press, then `r1 = 0` on release. A separate
QEMU-only combined inspector experiment uses this guarded call to switch
between parameter and routing pages. In the Room fixture, the recorded route
slots started at B (2); encoder 1 changed slot 0 to A-to-B (3), encoder 5
changed slots 4 and 5 to 3, and reversing encoder 1 restored slot 0 to 2.
The mixer send remained at 44 while the inspector was open. A follow-up GDB
inspection of the popup's live UTF-16 widget strings observed `CH 1 ROUTE` /
`A -> B` and `CH 5 ROUTE` / `A -> B`; attaching the debugger perturbs timing.

The MAIN, ELF, recipe and diagnostic JSON SHA-256 values were respectively
`681f245d14833c74b9120810177cea10785567a1998450634760b3ecb5adb05b`,
`6609b1d3efcc6c9edf77e44ff14733034d9d3267a6993b21dc567aae2118edcd`,
`1e3525aa2b884ed03143827fe25a85518b9f9fce91ad317523741db517ff8eb0`, and
`cfa8b026acbf65d3d4121415564fbcf839aa0d229670951dac6a4adad30bd336`.
The stock MAIN, NOR and QEMU identities are recorded below. Strip mapping,
notification behavior, synchronization and target timing remain unqualified.
The follow-up recipe, diagnostic JSON and GDB log SHA-256 values were
`633bd720dba9b9656eab312eaa6060a5ccad6a7c6806cd7dcefb657de18926b0`,
`7205ae1124a3b50b93467bf34c13c54f72246390223295675530b243276fdd7a`, and
`7c2e9d601a2fd4a722d2a10c1b7e3285672fbdf4063ff48cd7f0ae392e0cf859`.

The subsequent page-feedback experiment displayed `EFX CONTROLS` /
`PARAMETERS` on opening, `EFX CONTROLS` / `ROUTING` after the EFX press,
and `PARAMETERS` again on the next press. GDB also observed both route labels
in that run; route slot changes and mixer values matched the observations
above. Its MAIN, effect ELF, recipe, diagnostic JSON and GDB log SHA-256 were
`f42f1888998e4d4910748c165eb6faea5bd92cee35aba44fc58a6b6937307a60`,
`4186d4e766f98a06d714a38464d2e9464b3412fa9ea5cc975403feac6894c578`,
`94f5a1ae9c0939b4dc4095e6e7791094ee3f5b90fb185b6998af164f2e1ab929`,
`5ef520ea4a820b77fd3fcdd7b0196541d77dc7dc1201544d985cd30758d617e2`, and
`3e34ad0c4918a591c9f2e7f3cec8d17ebe622bc53b8c3e71a8cd20d130ad4ebe`.

The next QEMU-only experiment separates the editing bank from the selected
stock effect. While the inspector is open a short EFX SEL press switches banks;
each queued turn carries its bank. GDB observed Plateau pre-delay at 26.0 ms,
Velvet amount at 84.0%, then 28.0 ms and 86.0% after revisiting each bank and
turning once. The callback pointer and the edited route slots were unchanged
across bank changes; the mixer value remained 44 until the inspector closed.
This demonstrates preserved edits in this fixture, not an audible tail or
target synchronization guarantee. Stock effect selection still reinitializes
the diagnostic processor and can discard its settings and tails.

The combined MAIN, effect ELF, recipe, diagnostic JSON and GDB log hashes were
`293df555fdf0668b3599c1ab8d71553d15774a1e65da61677e0a266c6775d08d`,
`c9b68f5288336b5a792089d5e1a1a6640ed1888a96f50ea5b65b035b545d9641`,
`8e23aef1dd70b6a7d119c23e75883ef1fec4e1c22e196bd96ac317b221d2bb1f`,
`1ce930ce483a503e249575de9088fff7e53c5912ca2fa45026c95bec5ce9fa5e`, and
`a12c99331a2dfeb39c44d1210e3578c4a6d3d218aecf1ce8e1feac815112e3fc`.

An uninstrumented, non-popup follow-up did **not** reliably complete this
sequence: at the second return to Velvet the inspector was already closed;
the next long hold reopened it and the final short press did not select
Spring. The panel trace recorded both edges of every requested press, but that
does not prove delivery to the selector hook. This failure remains unresolved.
Two repeats with a scratch event/tick trace added to the selector hook did
complete and selected Spring; their observed short intervals were 144–160
ticks. The tracing changes timing, so these repeats do not erase the failure.

The uninstrumented MAIN, recipe and diagnostic JSON hashes were
`fdc9409a951bb7915803252037991bdd3e833fda03675262c96badec18043b8a`,
`440fdfbbb723b3e2a10ceb9918ddf6a48422c59e67603a6d48bb79e52978e74e`, and
`5d118d3954bbb72e895f6db9318d390020588f58f8f8045dec7679355c586750`.
The traced MAIN and recipe hashes were
`acd16610bafae96afc669f6b303d0a28bb52e8ea1751b744cc69e6547f50ab1a` and
`7171062de0e600ef378da04e44967dcb1f2bf0235fc768a3afcd40cf3399c34e`;
the two diagnostic JSON hashes were
`6cc2b97644f12eb0d60af189d97070f408b2124170aad29f0bcf649b95792403` and
`f7f0dd91d8f1c9ba01b27b91b1a6599190c9b548055be6614b397307b4af45c2`.

Two further untraced stress runs each switched banks ten times and edited the
selected bank successfully, but the final 1,500 ms hold did not close the
inspector. In the second run, the captured UI state showed that hold switched
the bank, consistent with its being classified as a short press. For the ten
preceding short presses, the stored firmware press tick minus the panel-input
trace time ranged from -518 to -504 ms; for the failing hold it was +403 ms.
This relative shift is consistent with roughly 900 ms of delayed press
handling, but the two clocks and the upstream delivery path have not been
qualified. A timestamp taken when the selector task handles a queued event
therefore cannot yet be assumed to measure the physical hold duration.

Both runs used untraced MAIN
`effd3707af72987bab4994f87ec6329457069e3bc2b6816e0950f3263ea854d7`.
Their recipe hashes were
`4b106d5ba272b5c07bd6fee2e18d53354530e819285d3ebb6ffd632fd16d629a` and
`03c19f6df589483b5d49739867f3aa0f15795bcea8d61aa57ff223048dbb2a0c`;
their diagnostic JSON hashes were
`d79317868466c938e2bf561d859ff7badd3ab48a6442e3efaf1dea2fcd7cc0a7` and
`9dba807dc7535f86a7a58db76c42730e4d6d575e866ba367bac12fefb5f83129`.
The second recipe's steps are reproduced by `prepare.py --callback
--inspector --bank-stress`. Its output is observational, not a passing test.

In a separate stock run, the five-word EFX SEL record observed at the
dispatcher was `(0, 0, 36, 0, 0)` for press and `(0, 1, 36, 0, 0)` for
release. A debugger stop at the direct queue wrapper identified the call at
`0x8004c606` as the producer for both events; neither record contains an
obvious timestamp. In the failing untraced stress run, the panel transmitted
`91 04 06` and the main UART accepted and read those bytes between the
69,804 ms press and the next 69,850 ms trace marker.

A separate QEMU-only hook immediately before that enqueue recorded the final
hold's press at tick 70,207 and release at 70,793. The inspector's stored
press tick was also 70,207 and it classified the hold as a short press. Thus,
in this run, measuring at this queue boundary would not fix the timing: the
observed delay arose before that boundary. With the same sequence and enqueue
hook, but the diagnostic audio callback writing silence instead of running the
effects, the hold measured 1,504 ticks, closed the inspector, and the next
press selected Spring. These single runs suggest a workload-dependent emulator
delay; they do not identify its cause or establish target execution timing.

The stock event-record and enqueue-probe logs had SHA-256 values
`b49239ac8f0284c0e1aa60bab3fa9bc18c1e42bc0b1abd354787f2aa0fdaf970` and
`72c9f15aa9176fc03118339ab0741b9ffa3d5cb9c4b992812c8f2ce5906b16db`.
The active-DSP diagnostic MAIN, recipe and result hashes were
`34e9fa79693616a66cade6cd822155e80de600e89e5a3f87f6d704ceedd4d110`,
`a84baa304aea1b431e24e191469e2cdf9062b8ebb36d3197e95dba83a268a0b6`, and
`08b3db04b2ce3c4a8c86e6c89538aaeeeb61bf4e35b27280590658ef9e9f4028`;
the silent-control hashes were
`4beb81ba475185a7784a76ae425f01440d3825afefb34c6cab6bf3b8d1940ce8`,
`9cfd89b774327b3f84403e90cc80ca8ffc1892b1b2f53d4902b288c07f5cd41a`, and
`9b09e9904436c22a1911c0658e8182c36b808e5a5e06fd45511c1c3a15b47c59`.

The Velvet jitter conversion now centers its 53-bit random integer before a
single conversion to `f32` and scales by an exact power of two. On the host,
200,000 generated samples and values at integer rounding boundaries matched
the previous double-precision expression bit for bit. In the diagnostic
Cortex-M7 build the generator's double-conversion, multiply and add calls were
replaced by one `__aeabi_l2f` call; the measured diagnostic `.text` decreased
from 22,320 to 20,228 bytes. This is a code-generation observation, not a
target-cycle measurement.

The active-DSP stress sequence still misclassified the last hold: the
enqueue trace recorded 591 ticks, the inspector remained open, and Spring was
not selected. This change does not resolve the input delay. That diagnostic
MAIN, ELF, recipe and result SHA-256 were respectively
`0563d5092f691be45997d714fb29ffd8576def1662c37611c907e6101001baaf`,
`6a5d6ccba2d638ab8b7babe49183004b8467e33695583bb268a1036c6abb398b`,
`9e1efae624ad8f79699d0ade180c268e689d5ec0e5a165575dd8a6a3240b69a0`, and
`e36ab33478143547403cc9ed60852c54ad0142a6ce549c513ba3ec37caec7df6`.

After the upstream timing change in `393ce52`, one active-send, ten-edit
sequence with the current DSP and enqueue-timestamp inspector observed the
final hold as 1,297 ticks at the enqueue hook, closed the inspector, and
selected Spring on the next short press. The callback recorded 4,472 calls
and finite, nonzero output. This is one instrumented emulator run with
synthetic inputs and unowned diagnostic memory; it does not qualify repeated
gestures, physical input timing, or the target audio deadline. The diagnostic
MAIN, ELF, recipe and result hashes were
`44ea11f7490b88e94e17c97f3434419fe92c5c650de3e1a8f49658af2531e32e`,
`97780c1fccad3b5fbc1bff3c84edd9fb280d78dd8fe4e2e120a429813981ae44`,
`1c913c056e01fd328cd1ecc76fcd4d369ea0c436538d74507c4febf00ca1c650`, and
`e44f75d4738ef53ee2fa923732ee3de736d8f86a83fbed54c4e1661944211039`.
It used stock MAIN and NOR identities as above and QEMU SHA-256
`1e35161130eecdb148f2aaf6986e14f77fbd4714e2e7100cf89dc1680eb3b554`,
whose source provenance the driver records as unattested.

After rebasing on `258acf6`, which switches L6max RX from the startup
compatibility mode to regular behavior at 40.5 guest seconds, the same
ten-edit diagnostic ran with active synthetic sends on all three buses.
The diagnostic callback was selected and reported 10,793 calls with finite,
nonzero sampled output, but the inspector-open flag remained zero, the ten
expected bank edits were not observed, and the next short press did not select
the second custom descriptor. The recipe completed; that does not make the
interaction a pass. A separate run with the diagnostic callback copying the
control stream and outputting silence instead of running DSP opened the
inspector, showed ten alternating bank states, closed it, and selected the
second descriptor. These single runs show emulator workload dependence, not
physical timing or proof of its precise cause. Both use QEMU-only diagnostic
memory and synthetic or silent inputs, and neither qualifies installation.

The active-DSP ELF, MAIN, recipe and result SHA-256 values were
`53ef6a8a851ff2b21581fe42bf38b777a78c3662b691a50df32cf440670b685c`,
`f8d6ee6c3d3b3fc7e95c5f6cd0e75333cd0898475d23e88c691f396fd851d80e`,
`a62ec4f31b7ce34cfede28d3efdf1279a5d0b95ba7d81dadf826c4f21121c9c7`, and
`1446edaf88c3d87966aa72ae6beaee3f10a18236b26c82ec9e3809bcc12757c3`.
The bypass-control ELF, MAIN, recipe and result hashes were
`1c640421f7624b9c562f9b752a299d3a3abb0e1a1d4156fea28fb7cd2b2efe7a`,
`59ff85231768b3329d2a73d238a883a9a3e1bacd7da2235838d9a31c214de548`,
`31e6954e4d68e30dde5157fad08dbc8e98cf9b40363d1682d2858e58a2bb23c0`, and
`d969fcb85102f81f040f99fc89ce1344c3c5bca8bc51311606779ad11c2c3c80`.
Both used stock MAIN and initial NOR as above, and isolated QEMU
`0fd44a21586cad3a3ce32bd1394278c05045852c52bf729e70a2b5ffd7e86acb`;
its source provenance is recorded as unattested.

An exploratory `cpu-profile` run selected the diagnostic callback after 45
guest seconds and sampled for 25 wall seconds. Of 349 main-CPU samples, 130
fell inside `diagnostic_audio`; 46 fell in one Velvet tap loop. The compiled
loop performed an integer division per tap even though generated delays fit
the ring. The callback image, ELF, profiler executable, QEMU executable and
sample TSV SHA-256 were
`c8787fead6dca81878c0f50c0b077bb32a5957023ca0856f3fedb958af758e69`,
`65f8d7b9a9729c60d554d1f7af3d04b64c69d79344fdba1cc971d3d4fb7eb399`,
`a6e5f57e4ce4139321c1cad539f44a7c6274468a7b4f9418535fb59f657c6ab4`,
`9724d60a31400becad8f1e98d6fd67bb9456c8dfbdeebe833e7636ed8e9cf536`, and
`3235d20ba1e5ae2a1ecd04109d3fda814c1657a0a128c9d116c1ff96e8f6c416`.
Sampling and emulator clock behavior limit this evidence; it is not a target
cycle count or a complete instruction profile.

A later exploratory profile used the nominal-cadence QEMU build and the
active-sends diagnostic. The diagnostic MAIN matched the MAIN installed in the
initial NOR. After selecting Room, 80 of 185 sampled main-CPU PCs fell within
`diagnostic_audio`; 26 and 24 respectively fell within two Velvet convolution
loops. The 30-second wall-time sample covered guest time 79.460 through 94.127
seconds. These samples identify an emulator hotspot, not a physical cycle
budget or proof that a particular optimization will restore responsiveness.
The MAIN, ELF, initial NOR, profiler executable, QEMU executable and sample TSV
SHA-256 were
`f8d6ee6c3d3b3fc7e95c5f6cd0e75333cd0898475d23e88c691f396fd851d80e`,
`53ef6a8a851ff2b21581fe42bf38b777a78c3662b691a50df32cf440670b685c`,
`41391b61a0eb4eed073ebaddd78e355edaed8bea15fd23097dd5e8780192e235`,
`920ede276b33c094db4cf4a5fda0578befb1e471e751ac9b6e124fabb56b891a`,
`0fd44a21586cad3a3ce32bd1394278c05045852c52bf729e70a2b5ffd7e86acb`, and
`0949e9318b8cb12b12c7d4f6071282d7d3062b0e655b61769ca3ba7af8630aef`.
Earlier profiling attempts accidentally used stock NOR and were discarded;
the firmware directory alone does not override an installed MAIN in NOR.

Iterating the validated tap slice instead of indexing every tap removes the
per-tap bank bounds check in the diagnostic Cortex-M7 build; the delay-ring
check and out-of-range delay handling remain. A host comparison against the
immediately preceding source over 40,000 frames, including filter edits and
crossfades, was byte-identical (SHA-256
`21287dcc5fa90bdb346225fa1060fb50d34b4d2ccd8a590f4ff6242172ee6337`).
The nominal active-load scenario still did not open the inspector or select
Spring. Its diagnostic MAIN, ELF, recipe and result SHA-256 were
`8d4c07ace83e9320089ba57a734b5a34a1c89f4b9eb03e09cde7ed4407cdc38a`,
`7c049b50652862ca293a5c34411d44a5a229363c9ba2352a76ca833450f5af09`,
`8d06f1f03d3e99dbb07e5d6626392511b01e9c2266e6e06041b44e4057107b43`, and
`debc9e36bad36e7cca33fac085bf070b3edd09b619c6f30edd4e348747da61f1`.
This change does not establish real-time readiness or control responsiveness.

With the nominal-cadence QEMU and active sends, an additional instrumented run
recorded 25 EFX SEL edges at the UART interrupt, but no EFX SEL records at the
stock enqueue hook and no selector-hook events. The interrupt timestamp queue
ended at `(head, tail, overflow) = (25, 0, 0)`. The inspector did not open,
and the second custom descriptor was not selected. This narrows the observed
failure to the path after the traced UART bytes and before stock enqueue; the
interrupt timestamp prototype alone cannot fix this run. The IRQ hook itself
perturbs execution and uses unowned memory, so this does not locate a physical
hardware failure or establish a safe interrupt hook. The diagnostic MAIN, ELF,
recipe and result SHA-256 were
`4a94d3dec0490d264f99c04906a6cf21d96e2992f6fd398a11e9bf1f5b7ae1b3`,
`82425824cb2ce58f5805e31ce770107ee139990e6221a23072435106f6d69463`,
`2523305fa4950bf5b0e3c4c519e40abad15830d7694c5bc07e934784a1f9f8bc`, and
`f3acef9127d0408d9e72c90a4825759cb19b4306f1a43f7552d4abe3e2a6534b`.

A repeat with the later shared-filter/idle-path DSP and rebuilt QEMU
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`
also failed to open the inspector or select Spring under active synthetic
sends. At guest time 73,627 ms, the IRQ trace contained 25 edges, the
timestamp queue was `(25, 0, 0)`, and the enqueue and selector traces had
zero records. The diagnostic callback reported 13,352 calls and finite
nonzero sampled output. Thus the interrupt-timestamp prototype alone also
fails in this later configuration; it cannot deliver events while the stock
path does not reach the selector. The result is one instrumented emulator
run, not a physical timing measurement. Its stock MAIN and initial NOR
matched the identities above. The diagnostic MAIN, ELF, recipe, result and
engine trace SHA-256 values were
`e55e9426934a8e5eda7bc2b64096cb7f7142c8fe70080d993dafb31234b1dd48`,
`07b2626e6d68a174b0c961867b75fb716f0d831cb4fece030d6e58ff56d40ec9`,
`b2c5db683b972067f8d70c24d6b02e204765dd7d9431283e8e651007dd1dde6d`,
`9d272dacafe79a9fbbc9e88c8ae766487744b7f3dd6b082e662e23efbb46088a`, and
`b64bd78322a6ae749403ece3e99b1080417b53fccc653382e1312555c8c8bcfe`.

A repeat captured the port-0 receive structure after the gesture sequence.
The observed ring fields were buffer `0x808814a4`, write index `0x488`, read
index `0x3f5`, and capacity `0x2000`: 147 bytes separated the indices at that
instant. The UART receive callback slot at `0x20018d8c` was zero, and the
interrupt queue again held 25 unconsumed edges with no traced EFX enqueue.
One snapshot cannot show whether the ring was stalled, overflowing, or simply
transiently occupied; a time series and matched control are still needed.
The MAIN, ELF, recipe and result SHA-256 were
`d1660198e0bc1f9e06b7c4cc05420215d94a28b95dc99713c074a24612cc40a2`,
`6b441d121c3234a6f0666464d7918ca95419ccad6d8eed0a7c17ac02dbffe6b6`,
`8bcc97922ae216b71c954932d0044fcd0b1bed9bcecc54fdfff035472e25c479`, and
`2f5cf8740767673375e9b5e8e12695243bed10e0ad93386df661800654b7b7d1`.

Matched diagnostic time series then sampled this receive ring at 14 points in
the same button sequence. With active synthetic sends, its read index stayed
at 1,013 from guest time 51.760 to 75.894 seconds while pending bytes rose
from 5 to 147; the inspector did not open. With the effect DSP bypassed, the
read index advanced from 1,068 to 1,680, pending bytes were zero at every
sample, the ten bank edits were observed, and the next selection reached
Spring. Both had the IRQ timestamp hook and additional QMP memory reads; these
single instrumented emulator runs show a workload-dependent receive-consumer
stall, not its precise cause or physical-device timing. The active MAIN, ELF,
recipe and result hashes were
`d1660198e0bc1f9e06b7c4cc05420215d94a28b95dc99713c074a24612cc40a2`,
`6b441d121c3234a6f0666464d7918ca95419ccad6d8eed0a7c17ac02dbffe6b6`,
`252c7508880a016491a29ccae5cec801fbaec4323c57ba97c268568ec86aa7fb`, and
`f7f8d23486442001442148da000318a57d33a955829e46efaa930dccc17bfc88`.
The bypass-control hashes in the same order were
`71f3642359bee6ef9e8eebb1d2ae3f99283d15ef3cc0fe8b30016a36ab2c39db`,
`97c806ccb546c7f63ca4db6a66bd7697147b5161727f6ab6d955d9fe28688187`,
`290b741d543dc9f3b5d84260ff5ad4cc3b3f39c226dbd272d986e11e3e57da5a`, and
`2e0583319b9001816e8a51bfe9e4df2af87183a1cd5aa246af98135de3f566f4`.

Static inspection of this firmware shows `SubMcuRX` created at priority 27;
its loop takes the semaphore at `0x8020a328` and calls the selected receive
parser. `AudioProcess` has priority 30. In a separate debugger snapshot during
the active-load sequence, `AudioProcess` was current, `SubMcuRX` was on its
priority-27 ready list with no event-list wait, its receive semaphore held one
signal, and its ring contained 72 pending bytes. At the corresponding
DSP-bypassed snapshot, `SubMcuRX` was waiting on that semaphore, its signal
count was zero, and the ring was empty. This is consistent with a ready
receiver being starved by higher-priority work, but individual debugger stops
do not establish continuous scheduling or target execution time. Changing task
priorities could affect the audio deadline and is not established as a safe
fix. The active/control recipe hashes were
`2cc3347efd3deff74fa1d1583e4fce305d22afc79405f7b9eb9107443146d280` and
`4761902c85a3a1696a4fdd5ac697004736c49f4b123cf1261a0034b8242787b7`;
debugger observer log hashes were
`e40c37d3a524a6792ff627dc5fadd7c8960eda7904457fd1b1af144ab92d2d3b` and
`07733addde00a7db5343fe424ef6d56076a642abe23fb819962070f5b67647b0`;
the observer script hash was
`da43e8a3cd90d66fa9a5818d946c9be46b15789671edb0787db45d9f21c017f0`.
The active/control diagnostic result hashes were
`ec266ecef995df31cc3a34f7aab231de2e214860798ff22c2a212a7bde449295` and
`a6f37d3852c26b633fe58edbeaea95ef16d42bca8012c1f46955b22711924c27`.
They used the active/control MAIN and ELF identities in the preceding time
series and the same stock fixture and QEMU identity recorded above.

For a further QEMU-only intervention, a hash-guarded debugger call to the
firmware priority-change routine raised the observed `SubMcuRX` task from 27
to 31 before selecting the diagnostic effect. The task handle and name were
checked in the recorded before/after snapshots. Under active synthetic sends,
the enqueue trace then recorded 25 EFX edges, but the interrupt timestamp queue
still had `(head, tail, overflow) = (25, 0, 0)`, no bank edits were observed,
and the next short press did not select Spring. The audio callback recorded
11,404 invocations and finite sampled output. This intervention shows that
raising the receiver can restore enqueue in this run, but does not restore the
downstream selector; it also does not validate audio deadlines or make a
priority change safe on hardware. The modified execution uses debugger calls
and unowned diagnostic memory. It used the active MAIN and ELF identities
above; its recipe and result SHA-256 were
`01b53375b847e6d5bbf1edb45cf9d37f78a988f5ea8f9a3e4c174e7ac3650a82` and
`2df1ee4ea91408b6ebd2e23f8c31a0b3288a63625785156fa53f9d6da25c2b0f`.

A separate matched cadence experiment selected Room and sampled the diagnostic
callback counter at guest times 51.760, 56.760, 61.760 and 66.760 seconds.
With active synthetic sends, the counts were 1,053, 3,471, 5,829 and 8,188;
with DSP bypassed they were 1,633, 5,383, 9,133 and 12,881. Thus the sampled
15-second interval averaged approximately 476 versus 750 callbacks per guest
second. The active diagnostic is not keeping up with the emulator's modeled
cadence in this observation. These counters alone do not identify dropped audio
samples or establish physical hardware timing. The active MAIN, ELF, recipe
and result hashes were
`80800cb808f7ea5e7de4c8c641cef66d788ce6c2edd44572deb886c959f3b6a9`,
`81865d430af5d4ea95451ebf1eba205ab62fe03799117b7313983ae533b94e2d`,
`9c2b12b81cdcdb99014db0c523a68a22e9824684f51d75fb6c2b59ec4a847b4a`, and
`3db71bbe6945ff6069f633c5de12cf417e5a668d7b1f614608fd4b87c3c5cc0c`;
the bypass hashes in the same order were
`724cf0870a8813ef57c719bd0969207a7009ad2481856dbd6091de3f489492d5`,
`6ec2efce652f404407f9cb4665d49bed3a915a72dd32bc6aaba69e733459647d`,
`78b13a723d77b30096e2dea33c782f29fa519d89c5375d2f1824ca20a9905af4`, and
`020abd08d2973df1618338d105b5cdfe282685617df0fdfd4e0da5c1825a24ee`.

Two later QEMU-only experiments used the same cadence sequence with active
synthetic sends and set Velvet length to 95 ms and density to 3,000 pulses/s
at diagnostic initialization. These are reachable parameter limits; the
generated filter is capped at 64 taps. At the four sampling times above, the
first callback counts were 734, 2,525, 4,224 and 6,015; the second were 734,
2,524, 4,222 and 6,014. Both average about 352 callbacks per guest second
over the 15-second interval, below the earlier default-load observation of
476 and bypass observation of 750. The second experiment also recorded 64
active taps in each Velvet instance at the final sample. This shows a more
demanding modeled load to consider when designing a fix; it does not measure
physical timing, prove audio sample loss, or qualify the callback for hardware.
The second experiment's instrumentation adds two scratch writes per callback.

The two diagnostic MAIN SHA-256 values were
`3158d8f82d56bcfa6dd2902cc09a9226b6f44fbc137f4cdaa66a7b36509e97c8` and
`36ba3182e91885958ae38ce43f4920c2e72b7df91bc045627b7fdaca02a0c74d`;
ELFs were `aca3f7123593970b2552cf122803f22375b4e0fa69fdff142f0f23a71f2c07bf`
and `660a237afc58ceefcd4295c2772c9339f64d68a31be30fffabfb4183971fc926`;
recipes were `23311eb2891b5af29f503f194d58fe72f8ea4357771fb7c7afaaf0f7eec5e575`
and `6d1a01f4161802629c96be78a81c894285a075f2a3900e4e90457e8315851c13`;
diagnostic results were `2b4e1ce7877723b252b7e89142b8b47584220bf7eca0fcb52ec245af020a83e3`
and `116dcdfffa90720bb33905da4181a8e63ab5b507d98e968632943cbb13dfb87b`.
Both used stock MAIN `980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
initial NOR `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
and the same unattested QEMU executable as the earlier cadence comparison.

A separate temporary high-density build replaced the per-tap delay-ring bounds
check with an unchecked read, guarded once per convolution by a check that the
write position is in range. The delay is still reduced to the ring length;
for an in-range write position, the wrapped subtraction is also in range.
The diagnostic disassembly no longer contained the per-tap ring bounds branch.
In the same cadence sequence, callback counts were 768, 2,628, 4,400 and
6,261, or about 366 per guest second over 15 seconds; the final sample
recorded 64 active taps in each instance. This single observation is only a
small improvement over the instrumented 352-call high-density run above,
and still far below the 750-call bypass. The unsafe experiment was discarded;
it is not a real-time or hardware qualification. The diagnostic MAIN, ELF,
recipe and result SHA-256 values were
`646054c1828a891eb611f056d5df6fed75820264221f421a7bb270a0c5540fe0`,
`15bc3f1fc272ee4362daf6b143d53deecd3711ed9609fae4b16c51ce0c9ebbf6`,
`d5899896da37845a9c65c1302307ae8bd51f813c03adfa7571988cab67e07169`, and
`fe8dcf26fce90582d0b777977045d26fe061bc44b5666f05bbee3cf49974dd42`.
It used the same stock MAIN, initial NOR, QEMU executable, and two
instrumentation writes as the preceding high-density run.

A further disposable high-density diagnostic skipped the cascade Velvet
instance entirely and substituted zero for its output, while retaining the
first Velvet instance, Plateau, synthetic inputs and the same instrumentation.
This deliberately removes the cascade and does not implement the requested
signal path. Its callback counts at the same times were 1,038, 3,492, 5,885
and 8,278, averaging about 483 per guest second over the 15-second interval.
The first instance's active bank had 64 taps at the final sample; the skipped
instance remained at its initial 30. This isolates a much larger reduction in
work than sharing some of the two instances' convolution bookkeeping, but
still falls short of the 750-call bypass observation. It is a single modeled
experiment, not a hardware timing bound or a functioning alternative.
The diagnostic MAIN, ELF, recipe and result SHA-256 values were
`6fa2086ccd5dbc71a38988e6a5023b6732c723a386def1071212c80f1a5ec186`,
`9d66af626ed3aafd94f12e56eab5b394b9f483cad710439d3bff31a546866df6`,
`a66c34ea9763309e1781e140841a453946c3fea09f322a6485830971157bc750`, and
`7ff9cd235e1c1211022187de90b1f2ea0644f16382efc94c852ad4b5a662f8ac`.
The stock MAIN, initial NOR and QEMU executable matched the prior high-density
experiments.

A different disposable prototype convolved both mono send histories in one
loop, sharing each tap's delay and gain while keeping the histories and
accumulators separate. It assumed both engines' filter banks, transition
state and positions were synchronized; that assumption is not enforced by
the public processor API. It also omitted the existing empty-history fast
path. An optimized host comparison against the current source matched
153,600 output frames bit for bit with edits, transitions, silence and ring
wraps; all 51 existing host DSP tests also passed with the prototype. These
checks do not establish equivalence for independently modified states or
physical execution.

The high-density QEMU sequence counted 942, 3,187, 5,362 and 7,581 callbacks,
or about 443 per guest second over 15 seconds; both active banks had 64 taps
at the final sample. This is an improvement over the instrumented 352-call
baseline, but remains below the 750-call bypass. The prototype is not in the
production source. Before adopting this approach, a design would need to
enforce shared filter ownership, handle stereo/routed path changes and
silence efficiently, and verify real-time behavior and audio equivalence.
Its diagnostic MAIN, ELF, recipe, result and comparison-source SHA-256 values
were `8c58d38951f01a6c4e011338cc1918ec8d72ce26e395cccc653d675ddb3d6c89`,
`ae5414a52f202d0e3ac8a3d429c64b08c136fcf4e3f9531468c845137d28ab42`,
`90b1350dccf2acb6029f3efe4d3d92ced6e7e9c0c5e116c10932823258768005`,
`86eecc95ccfb65001cfb0633cd05f5814426cf8f56f45e3418f3778622ab6332`, and
`cf70439043503d86d22fe66a6a34f6ebb5bbf118832591ed8b89f1fc00560320`.
Stock MAIN, initial NOR and QEMU matched the preceding high-density runs.

A subsequent disposable prototype replaced the second Velvet engine with one
mono cascade history and used the first engine's filter banks and transition
timeline for both sends. It kept the two audio histories independent and
skipped convolution when both histories were empty. Against the preceding
production source, host output matched bit for bit for 153,600 frames with
parameter edits, fades, silence and ring wraps, both without and with repeated
switches between routed and stereo processing; the existing 51 DSP tests
also passed. A host `size_of` comparison decreased from 319,704 to 298,192
bytes. These are host layouts, not target allocation or ownership evidence.

The matched high-density QEMU run counted 859, 2,924, 4,907 and 6,971
callbacks, about 407 per guest second over 15 seconds. Both channels of the
shared active bank contained 64 taps at the final sample. This is below the
earlier paired prototype's 443 and the 750-call bypass. The prototype remains
outside production: it still needs a one-sided silence fast path, complete
behavioral and performance validation, and target memory and timing proof.
Its diagnostic MAIN, ELF, recipe and result SHA-256 values were
`977c94f3dfe337a8f429279dc77d4c37650583cd83ecd271ebbb0efc810a36f8`,
`09a992063857d8cd2e06d8935c3b48945a042cf5e0c8689dbad69e8d5f591603`,
`57dc8669d7e93518961aa65c9db6bf2934edd6ae93919cbcaa540ed1f7bbdc8c`, and
`d2600e173550e621b42a18fd478dca0cf53cb10302643243010bdc9b6a4002c3`.
Stock MAIN, initial NOR and QEMU matched the preceding high-density runs.

A later refactored shared-filter implementation added the one-sided silence
paths and used a shared read index when the histories' cursors aligned. A
high-density diagnostic using that DSP source counted 949, 3,214, 5,405 and
7,623 callbacks at guest times 51.761, 56.761, 61.761 and 66.761 seconds,
or about 445 per guest second over 15 seconds. Both channels' active banks
contained 64 taps at the final sample. The diagnostic initialized the
maximum Velvet settings and added two tap-count instrumentation writes; it
is not an installable firmware image or a physical timing measurement.
The MAIN, ELF, recipe and result hashes were
`13e728bc082050536e7fd380db06b81252d30174fd384b7fae229c2388a046b4`,
`433583d11824f2518b86e80226989422f99f5c37db2505d47a6354c376f25158`,
`5f892659c640ba3948ac0d633e774b668ff7e605c67d6362c504904d8c0d6e79`, and
`54839a29011bd4540b763c24d5938dc6ba112d1c661a96c72d151f1fc7b36cec`.
The stock MAIN, initial NOR and unattested QEMU executable match the preceding
high-density experiments.

After integrating shared Velvet filter state, fresh ten-edit inspector
diagnostics using the nominal-cadence QEMU failed to open the inspector and
remained on Room, both without synthetic sends and with all three synthetic
sends active. The active-send callback reported finite nonzero output. A
separate traced run without synthetic sends recorded no post-selection
selector or enqueue events before the final long-hold snapshot. These traces
are reset at effect selection and tracing changes timing; this observation
does not establish the cause or physical behavior.

For comparison, a fresh untraced build from the immediately preceding DSP
commit, using the same stock MAIN, initial NOR, QEMU executable and
ten-edit recipe, also failed to open the inspector and remained on Room
without synthetic sends. These individual runs do not prove equivalence or
rule out a regression, but the failure is not unique to the shared-filter
build under this fixture. The current silent, active, traced silent and
previous-source silent diagnostic MAIN SHA-256 values were
`836e13c62d738a0269e4e7e5c32609b91cbf13a9027c1d6a5ae98a4a80fdafcd`,
`2186087bdfb47542a26102391acd0b786950f47740c578e8b60110fe035dc8c0`,
`c5e42c9b0dcf5c83f32313cc08e5aede9a2ceaa2c09170f4a4bb38de52fbfd35`, and
`ace4877ae226ddc617fcbf10f41dd0447e2ba787d9d60fea4353cfc5b9322162`;
their ELF hashes were
`fa313bcf4c646b125c50602f7a7792d89eb0022e695d9965813372655d490b4e`,
`04f32146fede565368d4d288aa681d95e21e393dfbc0106ff19629e592b4e554`,
`dc347ee9feb67bda9c19ff4be18a1a6cf961b539109e4833fb14066d08619b31`, and
`125bf1d5939ec7a72533dd0794619917d75a22277c83d7ee6511cdf98b11471f`;
recipes were
`253ee9901c467632f6750f935d0178e905434fb9c2755558605f6d1ab6d7d5e8`,
`f699249608c5048af8dbf76c8079165c86f46bfa29126464af0e469a941c2406`,
`c2d21c9279be231271a5456416f51ebcff2f1724034b96a152f64424ae39766c`, and
`9fa93a45ee7814f50b7befdc67f1f20802adf692c18a0dd7088ec64e8e2633b7`;
results were
`d0052f2c9a0d002aac19cd503b3200c1d53311d492d46f783fbd9f96e1316375`,
`95b88102bca9f07897c0647a8116478df4978589bf113f3896d5e411c8e6faab`,
`1f3be9b9ee4859b5e7ac961f5babacc6d4bfb4bb4b0ef3ea2b1c2f253c0ab6ee`, and
`1b2631fe66a4d3c8529b7b3dc893a49c94f13e80c8b70e5a7e06a621de061205`.

A separate instrumented current-source run without synthetic sends captured
all 64 samples of each reconstructed send at callback counts 2, 1,024,
4,096 and 8,192. All 768 captured values were finite zero; the final count
was 18,888. The inspector still did not open, and Room remained selected.
The snapshots narrow the observed case to idle input at four points; they do
not establish that all intervening blocks were silent or that the candidate
mixer reconstruction is correct. The capture adds scratch writes and uses
unowned QEMU-only memory, so its timing and addresses are not qualified for
hardware. The diagnostic MAIN, ELF, recipe, result and capture-source SHA-256
values were
`f1e9d2f3f8a6596de11f2dc674e4951b1766a5dcfba559e1775119313b055c2b`,
`bf889653cac9b7bfeeb935c07d91f15314b346a9af15ad67c6f83ea5d14df904`,
`05b711c422bcfe038e3040cb8fc770cb07391b9c2b2badea4c855a3a5d659d7d`,
`b9d0c25c9b5bec49f45889f21312806adac8fc7202bc7cecf64f17c61e50f93c`, and
`ec063a76d542e36561a1b7c9e9863db19bf1b09e1f914313a857ed8d2c3dd43d`.
It used the same stock MAIN, initial NOR and QEMU executable as the preceding
matched control runs.

The routed reverb now skips arithmetic while its freshly initialized audio
state is known to be zero, but advances its delay positions and LFOs. A
validated parameter update preserves that condition; nonzero or signed-zero
input and unrestricted mutable reverb access return to full processing. Host
tests compare the idle path and subsequent tails with full processing through
size, predelay and freeze edits, and verify that direct history mutation is
not skipped. This optimization does not skip a reverb tail or restore the
idle path automatically after audio has entered it.

In one fresh nominal-cadence QEMU run without synthetic sends, the integrated
diagnostic opened the inspector, recorded all ten edits, closed it and selected
Spring. The immediately preceding build had failed the same sequence. This is
one modeled observation, not qualification of active-audio responsiveness,
the reconstructed input path, target timing, or physical controls. The
diagnostic MAIN, ELF, recipe and result SHA-256 values were
`d38b274851da46820cf59053c5acf12ba8b330935027b589023b31ee243580be`,
`c971ee395071efaee2caf14325502a73a28bb683d49b5bbd86390a396bc5bdd0`,
`c04abf61ed4ab18c89654da3e0eb284434efd3f46e21557c3d1b28f01d46b276`, and
`e6c80ac6517ad3c8df01789bceb453890e2151c4e3905407f9241651c36dd4df`.
The stock MAIN, initial NOR and unattested QEMU executable matched the
preceding control runs.

A fresh run of the integrated idle-path build with all three synthetic sends
active did not open the inspector or select Spring. The callback reported
finite nonzero output and 13,348 calls at the final audio snapshot. Thus the
idle optimization does not resolve the observed active-load control failure.
This is a diagnostic observation, not a physical timing or audio qualification.
Its MAIN, ELF, recipe and result SHA-256 values were
`29b4622ca63d167c0ffadbc8acaa1a6549e1ac4266a75ec7bd2133099429cf20`,
`ce91af2f1da9701eee69206583b02fe24255978286cbe91f8e7a8f4e3d646c81`,
`ca95bc00981d603002de5cc25f2a7a860cc860b1f1c740859831eb05f636f1e9`, and
`335bb572520bb2cdd6bf4ba4416f8fc0f3b3bdbb44c60c0f96f49d8dd7d725ab`.
Stock MAIN, initial NOR and QEMU matched the preceding control runs.

A later exploratory profile used the current shared-filter and idle-path
source with active synthetic sends. A disposable profiler waited until 45
guest seconds, selected Room, waited until 51 guest seconds, and verified the
diagnostic descriptor before sampling. Of 265 main-CPU PC samples taken
between guest times 51.060 and 66.860 seconds, 62 fell in the shared Velvet
routine, 28 in the reverb routine, and 37 in the `0x80024b00` page containing
the channel-15 eDMA interrupt handler. The most sampled shared-Velvet PCs
were in its aligned two-history tap loop. Sampling, host scheduling and the
emulator's timing model limit these observations; they are not target cycle
counts, worst-case timing, or proof of an interrupt fault.

The diagnostic MAIN, ELF, sample TSV, profiler source, profiler binary and
profiler Cargo lock SHA-256 values were
`29b4622ca63d167c0ffadbc8acaa1a6549e1ac4266a75ec7bd2133099429cf20`,
`ce91af2f1da9701eee69206583b02fe24255978286cbe91f8e7a8f4e3d646c81`,
`f25b0029f9dd0d2495cf8f70a4f8c316bc8af096b4c07784fef87a58bfb86172`,
`d3b922665eca2ec59f423aa1c0617605c4a190b4882a7ddf0bb67d241142578f`,
`cc965dd99cb0fb87f5fe963d53e52a30ad9ccc7dbeb08d20dd6de6d94bcbe46f`, and
`03e74d436527fc011c5ed3718a4933547fbaddb48fc41dd90ad94b7371960ac9`.
The stock MAIN, initial NOR and unattested QEMU executable matched the
preceding active-load run.

A separate stock-firmware diagnostic checked the corrected emulator trace
windows through 47 guest seconds. Consecutive reported window lengths matched
the timestamp differences within 1 ms. After the modeled 40.5-second receive
switch, the channel-15 trace reported 750 assertions and 750 clears in each
of the 43–47 second, 1,000 ms windows; TX summaries reported 384,000 words per
window. These are modeled observations, not physical interrupt or audio-rate
measurements. The rebuilt QEMU binary, stock MAIN, initial NOR, recipe, result
and engine-trace SHA-256 values were
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`,
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
`1088085a78c0ad0bbfe921ea2f672638c3fb10d0967c7979f7b802f760e41252`,
`0fffffdd6a88f08f4f85d0d34cdd87a7f033b0ee5595635b287a337f73127f94`, and
`7e0a14d0b12a4cdaac9793fcb23f865f29ee4c28fe516352fcc14e2945c1c88b`.

A subsequent active-sends ten-edit diagnostic used the same rebuilt QEMU with
the corrected DMA trace enabled. In each of the 22 one-second windows ending
at guest times 52 through 73 seconds, channel 15 recorded 750 assertions and
750 clears, with the pending bit set at each snapshot. The inspector did not
open, Room remained selected, and the callback reported finite nonzero output.
This run does not show an excess modeled assertion count; the pending snapshot,
interrupt latency, trace overhead and physical behavior remain unqualified.
Its diagnostic MAIN, ELF, recipe, result and engine trace SHA-256 values were
`29b4622ca63d167c0ffadbc8acaa1a6549e1ac4266a75ec7bd2133099429cf20`,
`ce91af2f1da9701eee69206583b02fe24255978286cbe91f8e7a8f4e3d646c81`,
`ca95bc00981d603002de5cc25f2a7a860cc860b1f1c740859831eb05f636f1e9`,
`f4ff90266f0b3c0fdc2f2526ee2baceb2e72fb3d7d2fe08b94e518f37af33ae2`, and
`bbc83c3a2cd1f1eaeec60f0c649ffed2617ed711f8cc5aaf92499f90a4fbed49`.
The stock MAIN, initial NOR and QEMU identities match the preceding trace run.

A disposable build of the current shared-filter and idle-path source used
`opt-level=3` for the same active-send ten-edit diagnostic. The inspector
did not open and Room remained selected. At the final audio snapshot it
reported 14,101 callbacks and finite nonzero output, compared with 13,348 in
the earlier size-optimized run with DMA tracing enabled. The different trace
settings can affect timing; these individual totals include startup and are
not a per-second deadline measurement. Diagnostic text increased from
21,987 to 26,600 bytes. The faster setting was not adopted, and neither its
target stack use nor its physical timing is established. The diagnostic MAIN,
ELF, recipe, result and preparation-source SHA-256 values were
`4ac12874f4cbfc3b5a94fe03ec04ffc9041cfee74246c17c0fdbebf577a46b38`,
`03c2214f43d04eb02a4cacbd957edcb8ede6c5797170e5794b18324d0fc339b1`,
`8d8030ec6629eb8740ebaa6a23f13e644593629f020fe94d48fa47fb784035be`,
`eb4377db3e27396c6d73ca3eb566a1c479b20d3fd5d11d103dc9ef25c7baff2f`, and
`f30835137cb9b3a4f0ac3328bd2b8206a365d5fc17a18a2702eae36b3ee474a2`.
Stock MAIN, initial NOR and the rebuilt QEMU executable matched the preceding
active DMA trace run.

An uncommitted experiment used 8,192-sample power-of-two Velvet histories,
retaining the 4,802-sample logical delay and silence window. It increased the
two Velvet instances' state by 54,240 bytes. Host output over 100,000 frames
with silence, filter edits and repeated wraps matched the preceding source
byte for byte (SHA-256
`ba2cac6174cc65de04009747a9e4a3951b0669a9a7b814104a5142247245378a`).
In the same 15-second QEMU cadence sequence the experimental counts were
1,070, 3,529, 5,935 and 8,343, or approximately 485 callbacks per guest
second. This single-run improvement over approximately 476 did not justify
the additional unqualified memory, and the experiment was discarded. The
experimental MAIN, ELF, recipe and result hashes were
`4cdb52e6817f16dd6c2a51d11f9d368bdf0e3c96c61c045a9665de6d2d962a07`,
`c65b24392ebcff54b7b3cb50242f56a6b2e63d282742fb0f45b6e301a2b3929f`,
`c22e0bcb154349fecc930b7efdd6cfeb7a0efcdbf7885d64a88befe97beda5b1`, and
`17ecffe99a9cca86d5fcf13defda984b8b0070a905e41db5f4988260d2e496f7`.

Another uncommitted experiment cached Plateau's 14 output tap offsets until
the size value changed, including when that public value was changed directly.
A 120,000-frame host render with setter and direct size edits, silence, and a
non-finite direct value matched the preceding source byte for byte (SHA-256
`15848c826e41186dbfb1f75b0aa3a352a019e082f19efad7f0350ba8fa93ca56`).
The matched QEMU cadence counts were 1,053, 3,475, 5,835 and 8,196, essentially
unchanged from the approximately 476 callbacks per guest second before the
experiment. The cache and its invalidation logic were discarded. The MAIN,
ELF, recipe and result hashes were
`7fb8c78e2523815bd255076d4d4e2f6731d7610a5ed05d616db83fb76c9e22d5`,
`52045313b2c6ed0687d9fd14de5f2197429ad58b1d43fc214e4abf10c7a2b3da`,
`241cdc70e2a8fa2445584c5be5cb0805c6afef52ab1aa0b0f7a200524f0c43e0`, and
`4a75ff3368a3e47198110eb3f2e201f72bb02fe351969f5e897dd71d0b78fb28`.

A temporary diagnostic build used Rust `opt-level=3` instead of `z`; the
same 15-second QEMU cadence sequence counted 1,135, 3,736, 6,280 and 8,825
callbacks, approximately 513 per guest second. Its diagnostic text was
26,073 bytes versus 21,188 for the size-optimized build. A 40,000-frame host
render of the same source at both optimization levels matched byte for byte
(SHA-256 `21287dcc5fa90bdb346225fa1060fb50d34b4d2ccd8a590f4ff6242172ee6337`).
The faster setting improved this single emulator observation but remained
below the approximately 750-call bypass; neither target timing nor the
larger build's stack requirements are established. The experimental build
setting was not adopted. Its temporary preparation script, MAIN, ELF, recipe
and result hashes were
`79270a02d05956202bea0744184feb44cdaaab3020bde4327e884090ee0a209d`,
`556fd4ff32fdb12d00c1fc679c35c9ade7a604b8ec6b16c2422e4794b9684c36`,
`37db7dfc8195131abc97433e114a6ad3a2ce7d563da114b6253690bff35fb0c5`,
`f515584657849f83ad887970b859534f1ff7c090cd42f325417f79fedc4983c2`, and
`7279bba25d70ea99efb0c41b60bfec58bfd42e57ad6207e210d477f9d6679aee`.

To isolate the diagnostic input-path cost, a temporary active-sends build
skipped reconstruction of the twelve mixer slots whose results that diagnostic
then overwrites with synthetic signals. All three synthetic sends and both
effects remained active. Its matched cadence counts were 1,074, 3,540, 5,946
and 8,350: approximately 485 callbacks per guest second, compared with 476
with reconstruction and 750 with effect DSP bypassed. This single emulator
observation suggests that eliminating the reconstruction alone would not solve
the overload. It does not qualify a real input path, so the experiment was
not adopted. Its temporary callback source, MAIN, ELF, recipe and result hashes
were
`42f8c929fea0ab0c3c63e08b3c74e9e8b339d3cbab59a8595f909174ac26a3c1`,
`78717d60d45e6bff37542e2e6561d7efecfc35cb5c189e985aea0ae42e61716e`,
`adbda27462d294e319fdad1d95f86b697ee722659a66279fc8fd81ab3c3fe4fd`,
`79fb1bce8d5bca7e25f7b442645c1133f2cf0296aca379f220743c5654c443a4`, and
`e6709ac444e820a134e61959dafa2c0bc15a19f5b0c7896cc690298983e3d192`.

The tap loop now checks the range first and uses an out-of-line modulo only
for out-of-range delays. Cortex-M7 disassembly confirmed that valid delays
branch around the divide; the first inline version had not done so. A host
comparison across 40,000 frames with filter edits and crossfades produced
identical before/after bytes, SHA-256
`7f4eb17042484ab744a12f76d1892953a8517fe9bc988b7212bb964cfbacab80`.
Separate tests exercise delays beyond the ring size. The QEMU stress case
still measured the last hold at 560 ticks and did not select Spring, so this
does not establish a fix for responsiveness. Its MAIN, ELF, recipe and
result hashes were
`089d191dc3c89fa01bd7c9c68535d52d9f7cfb33e14273f98177cd106acf68fa`,
`000cbf1de47e22a702052e71b80fdbe2e6e5396c7db3d53b8d97237e342d7d09`,
`754ecb1b4295a6c0092a76c75b1d8f0ae2762802a15c39e3d1b0f103035d32b3`, and
`4a9683a81223b040675b73ebff7b5916b205da64579ded42c95cbcd98fe8b49f`.

Velvet now tracks the number of nonzero samples in each private delay history
and skips convolution only while that history is empty. History writes,
crossfade progress and bank generation continue during silence. Host tests
exercise the last possible delayed sample and filter advancement through
silence. Before/after host renders with sustained and intermittent inputs,
including filter edits and crossfades, matched byte for byte; their output
hashes were `7f4eb17042484ab744a12f76d1892953a8517fe9bc988b7212bb964cfbacab80`
and `f9e18d0d3dfe6cfcaf85164bd983d23dc751c0d941b9c77ffa429ea4af3799ac`.

In the silent-input QEMU stress fixture, the enqueue-traced run measured the
final hold at 1,504 ticks, closed the inspector and selected Spring. An
untraced repeat also completed the ten bank edits, closed the inspector and
selected Spring. The traced MAIN, ELF, recipe and result hashes were
`2d445d9d355d4c70ca078091f302ce6bd47f682035cb84795af27d30c8c9742d`,
`b9b602347f57bc24ff70326a564e4a6fdae495baede369cbe243780594c532fe`,
`71fddae4f2e530c7c68cc8162ef19e54f4e34d6f45cac1c4f26ff3762fff4529`, and
`4bb1dd01e4b1849f9fd87f5ec1e3eaaaa0bd62cf7201f9d8d174cb193f0dd50b`;
the untraced hashes were
`b95d650f1697cbfc0efa795ec2f52a4d05950d1438fe4f070b953c24aa3bc906`,
`f207a47f6223cf82b90b709caa0ff4409b664623beb62092ecd98178f67dab33`,
`8a5ab776e174a6f2241aa44e6c09623abbace814f3db9216f4175445b908ba39`, and
`f6fba63bd1eb9248d6c2e00b606490808187481a14d7d8ea00ef46d8ed0c297c`.
These observations do not qualify responsiveness with nonzero audio, the
worst-case processing time, or physical hardware.

The following QEMU-only stress run replaced the three reconstructed sends
with constant values `0.125`, `0.0625` and `-0.09375`, keeping both Velvet
histories and Plateau active. The diagnostic reported 4,433 callbacks and
finite nonzero last output samples near `-0.000369` and `0.021812`. All ten
bank edits were observed, but the final hold measured 534 ticks, the
inspector remained open, and Spring was not selected. This reproduces the
responsiveness problem with nonzero synthetic buses; it does not validate
physical inputs, mixer routing, audio fidelity or target timing.

The active-send MAIN, ELF, recipe and result SHA-256 were
`cfd1a5e8de06c6b900a3d03abda6e2b5fa04eb0086cbd26c7b23b0c57267305b`,
`c520801e67d45842b59f8a8e295b15c1a5e5a45c5bd459c3b157118ff8b6dcf8`,
`96a63cc090a42989c09261c0b6aeb5f86ad507421f68a7e3e3b9248cd200f216`, and
`7f149c907192309d6d00157e8be0aa10b5298a7c7ae83f55229b80afaa70df58`.
An independent rebuild reproduced the diagnostic MAIN and recipe byte for
byte. The stock MAIN, prepared NOR and QEMU identities are recorded below.

A stock debugger stop at the receive interrupt's `0x00003934` instruction
observed port index 0 and bytes `91 04 06` for EFX SEL down, with the link
register equal to the interrupted handler's stack pointer plus four. The
displaced instructions mask that byte after the UART data register has been
read. A separate guarded QEMU-only trace uses a low-memory veneer and
re-executes those instructions; neither that veneer nor the modified interrupt
is qualified for physical hardware.

In the active-send stress sequence the interrupt trace recorded the last press
and release at ticks 69,289 and 70,793 (1,504 ticks), while the later enqueue
trace recorded 70,260 and 70,794 (534 ticks). The current inspector still
classified the hold as short. This observation identifies an earlier timestamp
in this emulator run; it does not establish interrupt latency, memory
ownership, protocol completeness or real-device safety. The stock debugger
recipe and log hashes were
`a881ea6ddf7b9252fe3054070b70be6c84e699a143732828cf46d2d4693309e2` and
`f51ff0489fd7e81f08cd87b7406abb65b324b1310f4b1a826bdc353d2f175aec`.
The traced diagnostic MAIN, ELF, recipe and result hashes were
`2323f5e773df87724b3a87dd81dca3e3e66dd964fec6872ac5bf989aa7de145a`,
`c5b2e3de9897f729f520aa0ccf79d9c2c8a368ad3e36deb9b3537f43b169d4b5`,
`a069c5e94354f3755fd48d3bb7c0151f6450ad5fd0eee87e0335563b82a6b738`, and
`9d7178a68d6548c9f2121b8c42910a64af29843b2ffa350fe850b5f9a687058d`.

An optional QEMU-only gesture prototype now decodes the observed panel key
frames in that interrupt and queues their ticks for the selector. A missing,
mismatched or overflowed edge cancels the gesture rather than guessing its
duration. With active synthetic sends, one ten-edit run measured the hold at
1,504 interrupt ticks versus 550 ticks at the later enqueue, closed the
inspector and selected Spring; the queue reported `(head, tail, overflow) =
(25, 25, 0)`. A separate 40-edit run crossed the 64-entry queue boundary,
observed all 40 expected bank IDs, reported `(85, 85, 0)`, closed the
inspector and selected Spring. Its sampled output was finite and nonzero.
These are diagnostic observations with a modified interrupt, not tests of
physical input timing, lost frames, queue overflow recovery, or hardware.

The ten-edit MAIN, ELF, recipe and result hashes were
`77813ae76b328e71ac9bfa7c025f041c0eb75b06caf14a879ed3a36abb76fec0`,
`c3dbc2cca4e6c96db7a4f54ef63eceb2fbe559cebea882b66a127e01e2ba0070`,
`d1d44b44fa9105cbac7d7871f9811f15473ba70684eb9fd4d7ebf0dc71ddc649`, and
`f2823ea3ee14a17559cd751975fed6bd07af4cd16be82477dbfbd9210199a54a`.
The 40-edit hashes were
`64fd35b1c0d8f8367e26806ef6980b14c22b4c277a58d558f5c16cd1839d6cc5`,
`d39e9bb50cfb0211b78a2061df4445a2a74484f7b8a9aa892111a8e269d3fe57`,
`05fcedbd312ac7a37daae8d12b04ed8176e2e0794b992be4d7b644dd3e5ee832`, and
`e1947641ab17bbf2aae0be28e24925eb0f9d22b2b932a82828f00e188aa2fe3f`.

The interrupt-timed experiment has also been combined with the existing
QEMU-only popup overlay and nonzero synthetic effect buses. In two debugger
runs, the route slots matched the expected single and paired strip edits and
survived bank changes; the mixer stayed at 44 while editing and changed after
the inspector closed. The timestamp queues drained with no reported overflow,
and the next short press selected Spring. The callback reported finite,
nonzero output samples. These runs do not exercise physical signal inputs.

At a snapshot 500 guest ms after returning to Plateau and turning its encoder,
the first run still displayed `EFX CONTROLS / PLATEAU PARAM`; the later
captured event count showed that the edit had been consumed. In a second run
with a 1,500 ms observation window, GDB observed `PLATEAU PREDELAY / 28.0 ms`
and `VND AMOUNT / 86.0 %`, along with the earlier routing and parameter text.
The first delayed display remains a failure to investigate: the debugger
changes scheduling and these observations do not bound response time.

Both runs used combined MAIN
`2fa742a5211714667855d6272d764a4d2d412136af6ccc66ce6f6b3b1dfc14ff`, effect
ELF `46e771a31db851c38fef527388b6ffac22c6004c2b866191fb2cb116617cfcfe` and
popup ELF `df826edc6a1b7fa17ca56974a3f81d3482525303af51b75fcdf9a044c801609b`.
The recipes hashed to
`4a2d01144f3b3e3b7be749ef080775ca1182771bbd99037c8fb5f726a2c72649` and
`a41193d801763aba535040c297478d7239c5eee4480d8ff7ea149fa0998c8112`;
their diagnostic JSON hashes were
`186d0efbcf812ef66da47a4a2ced5826e1d18cd3bf3402aa3c75ee0b0a85d176` and
`f39f0b270c6f080d6526eb4a5de8c38f737fa5e085bd7572fe0fbc2ca56ed9c1`.
An additional run without debugger stops recorded the expected route slots,
edit counts, mixer behavior, inspector close and Spring selection, with queue
counters `(11, 11, 0)` and finite nonzero callback output. It did not inspect
the popup's live text. Its recipe and diagnostic JSON hashes were
`74b0fd4b2c9afc039e939778b14715f46f7725625f98707e10496c7fb053b981` and
`0dd1b4f2fd3de92897ab2641df6986aa43185bf37cadabc02059f4a406d759d2`.

A further debugger run sampled the delayed Plateau edit twice. At the first
snapshot, 500 guest ms after the turn, the popup still showed the bank label,
the audio-event queue was empty at `(19, 19, 0)`, and its consumed count was
19. At the later snapshot, the popup showed `28.0 ms`, the queue was empty at
`(20, 20, 0)`, and the consumed count was 20. Thus, in that run the edit had
not reached the audio-event queue at the first observation; its delay was
upstream of that queue. Debugger stops and emulator scheduling still prevent a
target latency conclusion. The recipe, debugger log and diagnostic JSON hashes
were `158c129ad811369392891f73d84e3296a8f1a2f1dd3ccbdf7664118b3f5cfe33`,
`582dfc0d9caf0262445d93739a593a7031e977a92c48a6e1c6b9a34a72195fb3`, and
`c0c81e3e47fe435ef84cc88cc704839dbc1f55641fde1585012c70567216dd6d`.

The same run's panel trace narrows that interval. The emulator accepted the
encoder input at 70051 guest ms; before the next 70100 ms panel heartbeat,
the panel sent `a1 00 00 01` and the MAIN LPUART log recorded all four bytes
as accepted and read. At the 70750 ms screen snapshot the edit still had not
entered the audio-event queue; by the 71750 ms snapshot it had been consumed.
Thus this instance was not simply waiting for the emulator to generate the
quadrature turn. It still does not identify whether the stock receive task,
the main UI event path, or debugger/emulator scheduling caused the delay.
In particular, capturing a timestamp in the receive interrupt alone would not
make the encoder edit reach the audio task sooner. Any early-delivery design
must also preserve ordering with bank/page changes and prevent duplicate stock
events. The recorded observations do not establish target responsiveness.
The engine trace SHA-256 was
`e155e260fab6fc335fe8b6f0ba82ee53e60e2ed4913e7d6544477577b55f5651`.

An independent run added bounded QEMU-only records at the stock encoder
enqueue call and at the inspector's knob hook, with no repeated debugger
stops. Under active synthetic sends, both arrays contained 29 records before
the next selection reset them. Pairing the records by order, the knob-hook
timestamps were 0 or 32 firmware ticks after the enqueue timestamps. The
recorded UI channels and deltas matched the input sequence. The arrays have no
event IDs, are capped at 32 records, and reset on effect selection; they do
not prove that this run reproduced the earlier delayed event or locate any
delay before stock enqueue. The instrumentation uses unowned scratch memory
and remains unsuitable for hardware. The diagnostic MAIN SHA-256 was
`0b7391d156070be992da99b30f99bae28ce06de62e37ec2adeb2b681fb5deaa7`,
recipe `7424a9cc7007b87ddee181c5e15108b08f5a6ed730220ba1147c116f825e91b3`,
diagnostic JSON `fcf314368dfd497ade08f8b07b95d286a84bb57e866fbf828ff9bd16de00792b`,
and engine trace `604fdbeeb0f30243c9719ce3196b7e90836d52390cb2e4d23e586b41aba81435`.

### Automatic AINR handoff experiment

The diagnostic probe can now capture the selected stock AINR state without a
debugger and install three copies before publishing the replacement callback.
This is an experiment, not a firmware implementation: it hooks a revision-specific
veneer, uses unowned scratch locations and erased flash, retains 128 KiB from
the stock heap, and copies that state in an audio callback. Its 10,000-tick
delay after observing the learned flag is empirical, not an established
learning deadline. An earlier immediate capture produced unchanged synthetic
samples in the observed blocks; the reason is not established.

In one delayed-capture QEMU run, the free-heap observation fell from 160,008
to 28,920 bytes. Three recorded 64-sample blocks differed from their synthetic
inputs in 64, 64 and 62 samples respectively, but this does not establish
noise-reduction quality. The callback counter advanced 2,290 times in five
guest seconds, below the nominal 750 callbacks per second. A separate forced
allocation-failure build recorded failure, did not capture or invoke the AINR
copies, and produced zero diagnostic output; this is not a qualified fallback.
An alternative diagnostic kept synthetic sends active without AINR when the
allocation was deliberately forced to fail. Its sampled effect output was
finite and nonzero, but the five-press sequence remained on the diagnostic
callback instead of selecting AINR, and the learned flag remained zero. This
is a failed control sequence under the changed modeled workload, not a
successful end-to-end fallback. The probe offers this workload as
`--passthrough-unready`; without that option the earlier diagnostic image is
unchanged. The forced-failure variant's MAIN, ELF, recipe and result hashes
were respectively
`2b183ede74203208c6af5c59d3ed30f5bf1eb9a2661a41406e3eb64492de84d4`,
`bc0d85ba90570d326b6a9924acd4d9ac59a58aa7ec6bb5744a4b050edcf6637f`,
`172c4fc7cf58fcfb745ab0a5871009fea0128268988ddf2db302f916ee33b9e5`, and
`acdb2fdff8d7dd9c351331890c70098262a5779ea3c8e1965e2c8c3d8c4fb274`.
In a further run, one reset gesture cleared both the stock learned flag and
the capture status; a subsequent learn gesture produced a new capture which
was installed before effect selection. This exercises one sequence, not all
reset, interruption, or concurrency cases. Its modified recipe and result
hashes were
`dff474839f8572b953b150a185acd3bf51a85a1d22a0382d097e5a2f53b2b81c` and
`b87bbf08e5128aa64615f916ae88e2fe0ee8ff3db2fbcf58ede7903fc4b284a6`.
Target timing, memory ownership, callback lifetime and physical output remain
unverified. The production builder remains disabled.

The diagnostic now retries a failed snapshot allocation on a later custom
selection. In matched disposable builds that forced the first allocation to
fail, both first reported `(pointer, status) = (0, 2)`. On the next custom
selection the retrying build reported `(0x808fa040, 0)` and free heap fell
from 160,008 to 28,920 bytes; the earlier build remained at `(0, 2)` with its
heap unchanged. These observations establish retry behavior, not successful
learning, snapshot coherence, or safe heap headroom. The retrying and earlier
MAIN hashes were
`b4eae887e89f6a8d41f4a73875f5be04759fead4fdff88b4a2b89426852449a3` and
`ef127b38e6219aea767df32b9bb72787e1d875a508b60766bb70e20bfa47c07c`,
effect ELF hashes `5b28342fff7ae69ae4ba53bb61c0086337c2080ba2013079cba6f994373443fd`
and `47e43bb1ffb157d744b83950eaea0f97ea1da450cc8d7440086d6c4107c33215`,
recipe hashes `b822ed147aaddc73e747f04872898f2bbf027e223232582cfa133b566fa88d25`
and `1efce83d0472c36471b1a9a4ad065765c542e4f3c3369ea352287204620c58b4`,
and result hashes `84222cf5baa0685acf7b63d4a8727e7d4a1495e7c65af38ea5f9eae617683606`
and `4e26d2387798ff0ac3a3b541db4d0115a04a9b5bfd2c55291018d1e7080f7278`.
The stock MAIN, NOR and unattested QEMU executable are identified below.
An extended run of the retrying build then selected stock AINR, observed the
learned flag and captured status, and installed the snapshot copies on return
to Room. Two of the three observed synthetic sends differed after AINR; the
third sampled pair was unchanged. The MAIN and effect ELF matched the retrying
build above; the extended recipe and result hashes were
`259a3d14e3e7f0d0805eb1defed2a6ab990811169e412c102ea3bc964364f2d2` and
`dcf05cdf65a66086da5bc6be83df5a96df84c7f2f4a9921334d72418c8f31c60`.
The capture still takes place in the audio callback and the low heap headroom,
signal quality, snapshot consistency and target deadline are unqualified.

Three further disposable QEMU variants tested moving that copy to the Main
task during effect selection. Instead of copying, the audio callback marked
the learned state ready after the same experimental delay. The first variant
copied at the redirected stock buffer clear. At that point the observed AINR
enable word `0x2028ba4c` was 0, the learned flag was 1, and the active
callback was 0. The three sampled before/after send pairs were identical.
In the second variant, forcing the copied enable words at offsets `0xba4c`
and `0xbad0` back to 1 still produced identical sampled send pairs. The third
variant copied earlier, immediately after the selector's fade call. There the
active callback was still `0x20280021`, but `0x2028ba4c` was already 0 and
the sampled pairs were again identical. The original audio-callback capture
had produced changed samples in the extended run above. These observations
rule out neither all output changes nor a different earlier capture protocol;
they show that these simple substitutions do not reproduce the observed
processing. The earlier point also lacks a proven quiescence barrier.

The clear-point, restored-word and after-fade MAIN hashes were respectively
`4c52656c991714ca3555783074885cc89813a58a13efa79240cf03174be92f97`,
`d333813088bf7ecf22a1f666d56754cb0127db53b403fc886ab9f1c32348cedd`, and
`1fbd08aed3ac169ea568ce95f7940ade76b98355a7d541e3efc91554b2537890`;
their effect ELFs were
`8026fd4680f540e19e33570dd8ca55d480ee819dc1337f045eb243c894c923ba`,
`d7aad22661f02c2f0ba4e2b342f99e6558aa9549fea55800833d45371182abda`, and
`0c7d0eb7909336ee71c45b2ab17334257171d5e1327cd60f08cbf31b39a89b65`.
Their recipe hashes were
`027cab3b3d63bb0a58d6b64cc76f3bce26d559d24cf899e1a1135b386d679c9a`,
`1a42f066e18bc3d7363d2009caa9210c6eec3f71ea2d5a40ea81f20d8b15bfd6`, and
`840ea11ed58aa4052d1e2ac0503712c3ddab7336663da503e06d7cc9a8597ef8`;
their result hashes were
`5b644e0462e8b97670f019a39869831dfd94724bd5d9a0621c8a2eeb80e1c831`,
`e1bb205aafecbc9c30d4c7ceda3b9ae9ffe0de3446ca62155c26cb261f267223`, and
`1975656c794d21eea4097656f983e7cd2169f87e03709b017f543453fd4fcab3`.
They used the stock MAIN, NOR and unattested QEMU executable identified below.
The modified source, binaries and runtime artifacts remain local.

A separate opt-in QEMU experiment tested copying only the first `0x19514`
bytes of the AINR snapshot, through the observed end of channel 0's working
buffer, and zeroing the rest of each copied state. With two independent
forced inputs, both copies matched their corresponding full-state stock
references for 96 blocks and the sampled state checkpoints. The observer,
recipe, result and diagnostic hashes were
`444d0d78a3f16c5ee5816f25dbfbd88498309ff19061492ecedbaeab88584f04`,
`45233d0c29dc9947da9353f8a78eb1e26788b80958fed6dd31f1cff11cbc51db`,
`cdf60e920ac8dfec93ed29d87609cfd9bed1fe4830fdc63276641cf36753f5b9`, and
`4d4764459d8af444ff0bbdd72381fbec1d3cde331d64a0aa5381ae6a3091a993`.
The stock MAIN, NOR, cloned code and unattested QEMU identities are those
recorded in the isolation investigation above.

In a separate automatic-handoff run using this prefix, the snapshot
allocation retained 56,296 bytes of the observed heap, versus 28,920 bytes
in the preceding full-snapshot fixture. It selected Room and changed two
of the three sampled synthetic sends. The MAIN, effect ELF, recipe and
result hashes were
`ad2a4fbca2f1757b760b4166e55831881b56a8993d2bbcbe107eaf166587b38b`,
`21b9a83071d2db1232518bf42538be23d09f81dd5424c842ab5dc953a5229ab0`,
`a5e3517186ea9cfbadad94c25c8c099b5f911357af82f750928c454a089b5392`, and
`0e47cf3d2fafacf554c5b1810d9faa3fca7b258308bbfcfe3d1620e310dc43c8`.
The default full-snapshot diagnostic rebuilt byte-for-byte unchanged. The
prefix reduces the copied bytes by 27,372 but still copies 103,700 bytes on
the audio task. These bounded runs do not establish the complete state
footprint, denoising quality, capture coherence, target timing or safe heap
headroom; the prefix is not enabled in installable firmware.

Three further disposable diagnostics split that prefix into 4 KiB copies and
compared it with a full snapshot made at the last chunk. With stock emulator
input, 14 bytes differed in the `0xb000..0xc000` region, while two copied
processors produced identical output for 512 same-input blocks. With injected
nonzero input, 13,178 bytes differed across `0xb000..0x16000`, and the
chunked and full copies differed in 576 of 32,768 same-input output samples;
two copies of the chunked snapshot matched one another. A third variant
copied the outer pages incrementally but copied `0xb000..0x16000` together
at the end. In that one nonzero-input run, the snapshot bytes and all 512
compared output blocks matched the full snapshot. This does not prove the
range is complete for other inputs or learning states. The final callback
still copies at least 44 KiB, and the full-reference capture and comparison
instrumentation themselves add work; these are not timing measurements.

The silent-input, nonzero-input and grouped-range MAIN hashes were
`73e7abf8991cbaff60e8c9473eb93437f960b2ee7d32c0fc2972b71f27e04966`,
`ac4ee39f6e926e8e6968b8d76f3f1495c03bd5515218fc818c84dd096bbc1dd4`, and
`520cb7c8d058b95fb888f606ae5fa29b981bc697cbee873cf1d2bd952434d4ae`;
their effect ELFs were
`871447b0a1e220abe310468c03be90bfc01d2188cecea1a1af3d11ff86c0fca6`,
`93c760075aaa10fbf929032d01cd8e9b813677fade4b45b4489be1264a5c350d`, and
`bf9b9234020f8f238505a0f4788f498784a21657ea1e9bef195bd25d065c68bc`;
their recipe hashes were
`9bb0320d5beb59c3ea27a2107fbfd36215f0ac8d7cfa6de73ba1733506b5a494`,
`a70575943d81cea72a60e969f9ec9f78ce393ec3ddff222aa1c29da443c69032`, and
`3bafb8a9126002f1e3fa7155d11ceffb5ae0b1084c54ad2855994dc4c70b3615`;
their results were
`b323168b721dbbd23577eee35969c06ea3621e194e878778bbbd73228b593bf8`,
`fb341a2c43d8f550422d2b492b62eb748c2c2bd3d59713e5c5096fbef629911b`, and
`875da3a559bbe3bb16f9482f5357c267afa0c27a0d6e3ed18acb8d4547d37f40`.
They used the preceding stock MAIN, NOR and unattested QEMU executable.

The QEMU-only `--hybrid-handoff` option now provides the grouped-range
experiment without the comparison instrumentation. Its normal run captured
and installed the state and observed changed samples on two sends. The MAIN,
ELF, recipe and result hashes were
`cdeff328c7cf41449e482ae12539b21534cdbc0628618abc5108060c9040f9bb`,
`ebb2b6959a1908d135329a5ce59171dc95f99859b31191b2bf2f4836ad1b489d`,
`0fa5df223fe0b08d46e2e1437465078aeb9bcf6f6db789004ceb9b15d26f61cc`, and
`6a039d1a1ece7469e264d3445b9e8d2c848693e3538c4021c96a2d93dcfe940a`.
In a forced interruption,
the diagnostic was held after one 4 KiB chunk; effect selection reset the
progress and left capture status and AINR readiness at zero. The forced MAIN,
ELF, recipe and result hashes were
`69993d65f131e8b37684f668e246df3c6d79f3b93440a18649059c1728e7b689`,
`43d6a2e952239b4c1fa02f3969ea39a7e1e4c5bd7138fbf4543c2b48c33d471f`,
`03c828495409adad6b7895383693fd557cdd964edb39dffc1417e90304abf553`, and
`c12219094f39779c6893c8abb835ca6fe95bd631ba58f7ca444d0b422cd35ade`.
This sequential test does not establish cancellation against an in-flight
audio callback, cold or warm reset behavior, or target memory ownership.

Both runs used stock MAIN SHA-256
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
initial NOR SHA-256
`866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
and QEMU executable SHA-256
`53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`
with unattested source provenance. The successful run's diagnostic MAIN,
ELF, recipe and result hashes were respectively
`2ba4b923ce869ae0b1509a8275e108ac9199f5083fce050b039324bd453464b0`,
`cd417bcce60f049470a7b5d607213d61a65fda45144021d0f5b5464d8fe53f3a`,
`db04bc8d7e486a05158f859c2fef371c84b0367a12cd1e8265eb1f5357c3cb7c`, and
`10088d151d065c11b447b149467a5be3b9ead8c586081aba0ed2d7478669dd9e`.
The forced-failure build's corresponding hashes were
`12082c8502348272e959d3aabe4391075b27670d2679cf5f0e292c3cb062802e`,
`410d24c08b63955def4eb9cdd0e319895b154711b727e37a223cac3184e9f379`,
`dc2e8f1f69c4e99f20d4179e35b46c6726f9f5034b66725df5360b1669dbde47`, and
`2e54ed34b9c458431cccac2cfe1bb6f17025b5e3b18f32865b98dad366f10ba0`.
All firmware and runtime artifacts remain local.

## Deferred inspector close under selector contention

The QEMU-only combined inspector now invalidates queued inspector events and
requests popup cleanup without waiting for the popup semaphore when stock
selection closes the inspector. The display callback performs cleanup while
holding its usual popup lock. Other close paths remain synchronous. This avoids
adding a blocking popup-lock acquisition while the selector holds its stock
semaphore; it does not establish the complete stock lock order.

In an artificially contended run, an instrumented display callback held the
popup lock for 3,000 ticks while a forced AINR selection ran. The selection
returned and the inspector closed while the lock was still held; the lock was
subsequently released and the stock AINR callback was selected. This does not
prove all popup replacement races or timing behavior. The QEMU-only MAIN,
popup ELF, effect ELF, recipe and result SHA-256 values were respectively
`fde9f436de5cfa1ac3c68954d7f847661aad462e6170b6b9f552a8dbe5c385d3`,
`a518cd5816915cd9b2a874894b1c8b4b9adae7c1ec864f1bb1d826f19b5380eb`,
`6a2781b9a74a2f654700a41424e2d94c1d5a4758cbd17bd3a7cb651e6f970f74`,
`32caf6ff3f3155f57a1d8a7879e35ab033bbebe927c43f65562aa35bb4f362ba`, and
`ec9b35211ea389700b79b56eea2b321411c66c3ecbb872ed446f4ef1f81221ac`.

A separate ordinary combined UI run exercised inspector edits, route changes,
queue draining and mixer controls after closing. Its QEMU-only MAIN, popup ELF,
effect ELF, recipe and result SHA-256 values were respectively
`159a4c83c7569ba4886930ab953c9c33295cdb579ea00547794be0768e6142ff`,
`3361d999fb0fd09fd2cb9b2c211940e5734040ff8e203cda48a1b0097782695a`,
`e2da9d801e043935ee9ae96f66941e33ad947f02ad5ed4de43cbe6c30e62c26e`,
`5fbf330bf30aaf99ab4d6879a1e4bd8707bc6f0224ee407fb2d2ba39f32f8061`, and
`7236188c3211e2836489499f8312d680b7004975888dc6b753a38f5ef032cdb0`.
Both runs used stock MAIN `980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`,
initial NOR `866ac2f085a00ccf208e9bf9394df31fb847a5bf69fe90f0e674fe88a1e9791a`,
and QEMU executable `53504c7bcb0d883f911a286a7359799bf1dec67b4c32dd9be85ef35ed6f4de9a`
with unattested source provenance. Neither diagnostic image is suitable for
hardware; the production dual-effect builder remains disabled.

## Evidence and limits

The combined QEMU-only inspector now stores its 16 saved parameter words in
the popup manager's heap sidecar, instead of the unowned scratch mailbox used
by the standalone inspector. In one run, the tagged context pointed to
`0x808cb668` and the handoff to `0x808cb68c`, 36 bytes into that allocation.
After a direct custom-effect switch, pre-delay was restored at 28 ms and a
turn changed it to 30 ms; the saved Velvet amount remained approximately 86%.
The MAIN, effect ELF, popup ELF, recipe and result SHA-256 values were
`6bdc8f5b475df07e556c58cf24ddcbaa9e7fa399e5451860f7ff3de34a6aeafc`,
`0dc978d5938f42f1eb5845c0890650e44c125fddd022d72e24e03df8f403f211`,
`968c17b6b85952a914061b17abb9aacb303283da4eb48adf58026c09b438ac72`,
`e9c7e8df2ff485ec5c044ec1704fd98436cf41ddd393d23a5aa4dcfc192c351f`, and
`ee850a78633be5d808b86142a104f18b632b68f62949edeea3296f77fbd89e9a`.

In a forced sidecar-allocation failure, the inspector stayed closed and the
normal mixer encoder changed its value from 42 to 86; its queue stayed empty.
The forced MAIN, effect ELF, popup ELF, recipe and result hashes were
`6dfa168d7c215ac0228429ede94b723cc65bc59a595b46d837fe64f19d5842b0`,
`f1dc6e76263712e8e5121c5fda1250f36a7dc06dc13331e438737c98ec253e78`,
`ffdc2d51bf79f522edd8b28711e261d789f4f755941778a2a2775b203971f549`,
`e3fbe3472c08218906033d33443dc0aaabe77acbb627c8d1216442ef60d27acf`, and
`d3d9b09a05c6261ed373bcfd5131fd78b7dee9d7378a35b97819ca4de28900b8`.
This forced variant is a separate disposable build.

In another run, the saved pre-delay and Velvet values survived a cycle through
the four stock effects back to Room. The inspector reopened, but an encoder
turn did not reach its queue. A matched traced run of the prior scratch-backed
diagnostic showed the same behavior: the turn reached the inspector hook in
LEVEL mode 9, while that hook requires EFX mode 6. The stock-cycle recipe and
result hashes were `a7b48c93f2a4ccfbc440dc7fa64c16f5afe2a4f89a92f20b06ebe170ce3bb532`
and `415547fd2ba546b3a52af1acc563571096607f3f79670ac1a64256343c026d01`.
The heap-trace MAIN, recipe and result hashes were
`1d803acec9bdf23216a5433944265effacd7e63b5447b7810a3293036a74b158`,
`2cd0b020ffc62aba2269b337b1272a660ca628bb4d220080e57e7a7e780f1195`, and
`e6c6172cc434fa86d924ff417f546b12255fa355f4fad8cf65d74ba390ff3fd8`;
the scratch-trace counterparts were
`0359ba9e18212dc38da697199ce7d516ea3d3fc1aed799f42c0396c3edf31e21`,
`65c08cbef02effd49641dc81845a435fdc197d9e07e87507408ae414d3ea0d21`, and
`25c449df60ecbe880083c44ff2eb2c669989bb68a7f0213584de37a65a70fb8a`.
At that stage the handoff allocation, queue, gesture state, routes, callback
ownership, failure recovery, target timing and physical memory remained
unqualified. All runs used the stock MAIN, NOR
and unattested QEMU executable identified in the preceding section.

The combined diagnostic now also puts its 12 route words in the popup heap
sidecar. The scratch route array is an observation mirror; the standalone
diagnostic still uses it as the functional mailbox. In a matched perturbation,
forcing the mirror to zero before a reverse turn changed the heap-backed
route from 3 to 2, while the earlier scratch-backed diagnostic changed it
from 0 to 3. Those disposable MAIN images were
`fff3873de384c2d332e6277a0d81c0ea6ec7f559c64e67fff3a414b694e6bc16` and
`daa570ce115b75394ca9de616be3ffcdf4057a9cbabee6712094d3d32c7f8e50`;
their recipe hashes were
`8c47d210dc24c18ea65ea745177f97a21c10d923f18c11121873488785e1066a` and
`076656902d59dca2b6e8f1cb2864223b8e0bc9d6f132f6b0e8d48ffce93f4ff2`, and
their result hashes were
`444fe2fc031aa6050fac4ad832e6536eaea1428792a16d2cb7a6242cced11e20` and
`0d5eccfadf63ec7d0fe91e0c4ec933d3ee732b83782b682e5a4161372d223a80`.
The effect ELFs were `cf27c91630f24104c68ffc8d866d51cc6a663885ca53e64d77ccd832f6ff5e12`
and `a6a367aa9279b9ab966f8a55cbeb1f329a4d68d0201246778d34fc4ec215d803`;
the popup ELFs were `4e5868d0f8f258d7ae6f7bea85c294f74cff0eaaecaa30e84a363179093cbe7f`
and `968c17b6b85952a914061b17abb9aacb303283da4eb48adf58026c09b438ac72`.

In another matched instrumented callback, a constant test input reached bus B
at 0.125 in every captured sample with the heap route set to B and its scratch
mirror forced off. The previous scratch-backed variant produced zero on bus B.
The respective MAIN hashes were
`5783605f58903bd2024f27f41fe8d749004b76aea855002e5aaa7d07c8ae81c4` and
`dab8c2b44afd99ec172e39882f6db461775125650b7dcb2c42d792e64e29c8b6`,
recipe hashes `fecd60aa6f02783411da579c6cf6ca7c8b8bd9ad575718914b34371720376f5f`
and `1fff827abbba910fb4284e490bb655cadbbce42b3fa5b129094e8a3a17950d12`,
and result hashes `dd39aa6e53ee8c0fa696a3c491dafaf6d8f082c098d399d1987fcf43357be478`
and `7fb1c71b93b6291b5923eeb46c1cf5e698928c95dfd3b452bc36eb397fd9abb9`.
The effect ELFs were `47aaa492302c9229f39d1e5b4ae23acb58228a25bd5d33c14260dad59a388f05`
and `b3c651dcb87a9bf3dcc659ad8b5fe876ef9d70dc4f18ba7048b306376d9eb43c`;
the popup ELFs matched the preceding pair.
These synthetic inputs and forced writes do not validate physical routing.
With forced sidecar allocation failure, the inspector stayed closed, a normal
mixer turn changed 42 to 86, and Spring's fallback routes were all A. The MAIN,
recipe and result hashes were
`8604ffd504c4b733502072fd56eafbb07725e987f3efe967e8ae094d92b1f7b8`,
`40c55d6fbcd164db5c54a137cfb3d4db7016e50e3c3b2787c8eb5da432dd3562`, and
`fadf95df00ad764996f28592f80327821eab8d11bc205432c5abbf70e028fc25`.
The effect and popup ELFs were
`5a2740970c8bede2a645afb60cd3cfe06c54453e377bd28208362c0da8f06a71` and
`88ca3d12cb98695886fbcb99b748f9bef99d9edf079cc4aceeffe892ce622072`.
In the maintained build, a stock-effect cycle preserved the edited routes
`[2, 2, 2, 2, 3, 3, 2, 2, 2, 2, 2, 2]` in the heap sidecar. The inspector
reopened in EFX mode, edited the restored parameter, and returned the LEVEL
encoder to normal operation. The MAIN, effect ELF, popup ELF, recipe and
result hashes were
`33e6b78bef5abb475c7aac257784426d2dd99a20024bb93e19d09ec0a508fe03`,
`455a9da7a0858a2ccdeb7be9764cf508e87ba94670c29ad8f3d8a8e1e8852f29`,
`4e5868d0f8f258d7ae6f7bea85c294f74cff0eaaecaa30e84a363179093cbe7f`,
`6b64eb621b08b736dff5a1bb5ad8eb938f60cb8ce6941ff5972b4adf8a456e94`, and
`2dc9c187f662f65582de1fdef9987c779071bf9d50da6b5e9c773d15e23c40df`.
Sidecar lifetime and failure recovery, the event queue, gesture state,
callback ownership, target timing and physical memory remain unqualified.

The combined diagnostic now stores its event ring and notification-generation
words in that sidecar as well. Its QEMU scratch counters are observation
mirrors. In one run, 21 control edits were consumed, the queue ended at
head 21, tail 21 and zero drops, the edited routes survived custom-effect
switches, and a normal mixer turn changed 42 to 86 after closing the inspector.
The observed sidecar queue was at `0x808cb6fc`; the tagged context was
`0x808cb669`. The MAIN, effect ELF, popup ELF, recipe and result hashes were
`d48aefd202c0cbd1df3aabdc008a00b6a2737fb601821166a38ce543b2a94156`,
`1940b710e53ccb17fc5c5e23d02df302b9aa239a1f54e0163def29d351f1b976`,
`1189474f6a8be0b1b413cab0fc08a1be2910633b7dccef247a8e137acd77d341`,
`ce37df90e7f7fa438fdfe794ce177d928cfbe0cb0c583908910eb50534b4bc40`, and
`91a2ca9a9b37b593f818778e8f00483e170f85608ed7ecf97086f3bdb6f9c1bf`.

A separate disposable build forced the scratch head counter to `0xffffffff`
before each encoder enqueue. It still consumed 11 edits and recorded the
expected route changes, with heap head and tail both 11 and zero drops. Its
MAIN, effect ELF, popup ELF, recipe and result hashes were
`5907087faf537abea239d466cc28180bfe1c300a80a38c85899b908f9b03b5b4`,
`0d9d353522511192dce0515ac51e46ddc8c6b065df218564c1e6c478d196f277`,
`1189474f6a8be0b1b413cab0fc08a1be2910633b7dccef247a8e137acd77d341`,
`1d76820b352cf9588d34e0bf7214892a618cbb5b4b36270b0aae84cc9bdcf05d`, and
`572314da6b0c758de8c2737fdb5b9dd643c344daf150306d35fa5275489f2cc8`.
The previous scratch-backed build with the same forced counter reached the
first edit with eight drops and no consumed inspector edits; its corrupted
head also caused the consumer to advance through scratch entries. Its shorter
recipe and MAIN, effect ELF, popup ELF and result hashes were
`85f5ac0fd6506d5c02672af063df0d946e7a096a7bbeee70facbfd8b1f77eb55`,
`d8b39b26f98b2f4cab1708f870178043c77a246524c2d72aa1e3da53ff574bb9`,
`2fe91837034c695908e3f4e9bbaebdb02f272066ec8585269d3e8bfc8885bbd5`,
`4e5868d0f8f258d7ae6f7bea85c294f74cff0eaaecaa30e84a363179093cbe7f`, and
`9cc0442ca57e92f76f5078e46d30b5b3f6b8218874a9178c62a6d7025e0eaf46`.
These runs do not prove sidecar lifetime, the queue's single-producer
assumption, overflow recovery, callback synchronization, target deadlines, or
physical memory ownership. They used the stock MAIN, NOR and unattested QEMU
executable identified above.

The queue operations are now shared with host tests covering full capacity,
32-bit index wrap, FIFO order, a standalone queue without generations, and
100,000 event/generation pairs exchanged between one producer and one consumer.
In a combined emulator run of that implementation, 21 edits were consumed,
the queue ended at `(21, 21, 0)`, the route edits matched the earlier run, and
the normal mixer value changed from 43 to 87 after inspector close. The MAIN,
effect ELF, popup ELF, recipe and result hashes were
`d2c4164ab2ba6ed624a78d79ab1631b48f0ba14bf9e60d5542b9a888332e961e`,
`e86eb20212f113f2dd1581ee61c113d5374a130fe6b12e3b6178a2a9b5dcf1a4`,
`1189474f6a8be0b1b413cab0fc08a1be2910633b7dccef247a8e137acd77d341`,
`264e7f0e1458b6fc940c057bf0354b75add47b05a1623847bcbc6b5ac5528233`, and
`b8d1a883bdada6b6c3d17734ad3b60173b52b047775a1aeb0d69d1b72befbd19`.
The same fixture and unattested QEMU identities apply. Full-queue edits are
still dropped, and the target-side assumptions remain unqualified.

The diagnostic consumer now rejects a head/tail distance greater than the
64-entry capacity, rather than reading from an impossible batch. Host tests
cover invalid indices, wrapped indices and a valid full ring. In a controlled
QEMU test, writing head 65 with tail 0 left the guarded tail at 0; the earlier
build advanced the tail to 65. After the test explicitly restored both indices
to zero, a new encoder edit was consumed in both builds. This check cannot
detect all corruption and does not repair the queue automatically. The guarded
MAIN, effect ELF, popup ELF, recipe and result hashes were
`d4bce9b4180e6adfc0789b9f13f91b048811d6b062a6848140b535974e34b8d6`,
`e00ec41df4737fb4019ef196f05b6836f246ba47fa26786ebf49bfd98fd33996`,
`1189474f6a8be0b1b413cab0fc08a1be2910633b7dccef247a8e137acd77d341`,
`125ee1ea7844c1e9de1e44a122a41d2349c81e82f24be607e455e4e416069abf`, and
`086c5a77bc1bf98e1a3f090f38356300a73bd99c03538df71ae29d0d0c68e697`.
The prior MAIN, effect ELF, popup ELF, recipe and result hashes were
`d2c4164ab2ba6ed624a78d79ab1631b48f0ba14bf9e60d5542b9a888332e961e`,
`e86eb20212f113f2dd1581ee61c113d5374a130fe6b12e3b6178a2a9b5dcf1a4`,
`1189474f6a8be0b1b413cab0fc08a1be2910633b7dccef247a8e137acd77d341`,
`2250cb50ae57ed3efbac125115187f4cd6dbb64d1843d0f31112cb5f75e3465e`, and
`301af1e9d6d64b0155bcafb7a004ebf607f4788b900e9741bb64f5212535181c`.
Both tests used the preceding stock MAIN, NOR and unattested QEMU executable;
debugger writes and stops make them controlled observations, not timing tests.

Velvet amount edits now ramp the wet gain over 64 samples on both the direct
and cascade histories. A host test with a fixed unity tap and constant 0.5
input measured a 0.481 full-scale step when the old amount changed from zero
to one; the ramp keeps successive output steps below 0.02 in that test. It
also checks a direction reversal during the ramp and reset to the requested
amount. This bounded test does not establish click-free behavior for arbitrary
signals. A separate QEMU impulse diagnostic retained the observed Velvet and
Plateau peaks of 0.30966336 and 0.03750144 and marker `0x46585031`; its MAIN,
effect ELF, recipe and result hashes were
`92d19eedaf6f1c76b876906e2041133445c589c7dbd18e2eb8a93113eaceb74c`,
`630a95e76d4be8304ba1f32d89cdd4d801043a1139b46f4ca8ae38a2dcafae29`,
`4a00332500e7140f8cd151252367fddedd7e7920d23e9b8dc02ce3083cadbb4e`, and
`ad1760ba333533894e66df0a86361a2d2f96a8b97aae43299be296d6929fd260`.
It used the preceding stock MAIN, NOR and unattested QEMU executable and did
not exercise live edits or target timing.
The separate combined UI run recorded the expected route edits, consumed 21
events and returned the mixer control after closing the inspector. Its MAIN,
effect ELF, popup ELF, recipe and result hashes were
`5f6d5645635dd84ac0776897b5ca543cdd0478786a458479dfe4f14884d2e560`,
`179ac33118a8f9188f2b77841354db8647ab9ed973a3448b34ace23ec7447e33`,
`1189474f6a8be0b1b413cab0fc08a1be2910633b7dccef247a8e137acd77d341`,
`53cebabc7f1cc568a4e976e8f55a319f7bbccd4ee6cd394627e485b24533178f`, and
`0fbb69ab6352658880eaf0bb0d8e4da5c09982bc781ccc09c105a92e944d491b`.
This was a silent-input UI test, not a live-audio or hardware qualification.

The QEMU-only inspector now requests EFX mode through the stock mode handler
when opened from another panel mode, and closes before another mode is
selected. Nine observed direct calls to that handler are guarded and routed
through the diagnostic hook. A mode-button sweep observed each of the nine
non-EFX modes close the inspector and a subsequent long press reopen it in
EFX mode. A normal LEVEL turn after closing changed its stored value from 0 to
1 without changing the inspector queue. After a stock-effect cycle, another
run observed LEVEL-to-EFX reopening and an edit of the restored pre-delay from
28 to 30 ms; choosing LEVEL closed the inspector and restored its encoder.

Both runs used QEMU-only MAIN, effect ELF and popup ELF SHA-256 values
`7a1d796d89d531fec44201368a24c70315b37da6e6c57ec9791ea1a389c3b6b5`,
`cdc5505344345eedc35c76f917f7af858c9b6450926cdadefabaa6949b3b856c`, and
`968c17b6b85952a914061b17abb9aacb303283da4eb48adf58026c09b438ac72`.
The sweep recipe and result hashes were
`aa8f53cd9fc490ee9ec5d5c29cf57fc6401c75c1e51f59379efb55daa08160f8` and
`4afedc41d287e66c486530324bbd5102cb7f45c744074c45222c433423328e79`;
the stock-cycle counterparts were
`fc32cad929a7cf0b045730ac681f6aeb991922f440c7c86ec5dfa2a99e4d35d7` and
`f4ce6399c40691c15335421779ae47fb01dc6945f7ac3f15d6660e685c97cdaa`.
Stock MAIN, initial NOR and the unattested QEMU executable match the identities
in the preceding section. These observations do not cover all indirect callers,
window transitions, event races, target timing or physical controls.

A reverb size change previously shortened its tank delay rings without
updating the excluded buffer contents. In a host experiment, filling the
large reverb, shrinking it for three seconds of silence, and enlarging it
again produced an old-tail peak of approximately 0.992. Tank histories now
continue through the full backing buffers while their configured tap delays
change; the same experiment stays below `1e-5` and accepts new input. This
does not eliminate all transients when changing size during active audio.
A separate host comparison at four fixed sizes, including freeze transitions,
matched the previous implementation bit for bit over 120,000 frames per size.

A QEMU-only impulse run produced Velvet and Plateau peaks of approximately
`0.30966336` and `0.03750144`, respectively, and completion marker
`0x46585031`. The diagnostic MAIN, ELF, recipe and result SHA-256 values were
`8b74de3053a21d659500bf0c99f7d7c9883834c8485bd4637dc5ee88a4007f12`,
`97a57c3744bc1a62b754bbcb3f12b83524587a366f5ff6de4d7b35b6cfd5e59b`,
`b0c68627e244796516267c7e4385da2ed444ea55a491e7e4ad34c7c3278355c3`, and
`447d1d4466c1a144c6154d451910f360329d14a3c95897ff158932c26ae9ede5`.
Stock MAIN, initial NOR and unattested QEMU executable match the identities
in the preceding section. The impulse run does not exercise live size changes
or qualify target timing, audio safety or hardware installation.

A separate combined UI run observed the expected route edits, drained queues,
and mixer controls before and after closing the inspector. Its diagnostic
MAIN, effect ELF, popup ELF, recipe and result SHA-256 values were
`f6c7357060ef203671087d3649caa54b4aefc4cdf3dd90e4e24d32edb9474d49`,
`e2c8dd6c6cfdc8f99181647ec713dd06ca095bdd9aa1285faf7e0a472b09b870`,
`3361d999fb0fd09fd2cb9b2c211940e5734040ff8e203cda48a1b0097782695a`,
`0a90ef67efe02d87d52f6912764ab85d81a971d53d879b4e3968b4aed7190671`, and
`5df3bcba809dc81d326b4eecf54cdf1cc8891bd49b4fd636c015fca3a724e162`.
This silent-input UI sequence does not establish active-audio responsiveness.

One 2% size edit during a host-rendered 250 Hz, 0.1-amplitude input produced
a 0.664 full-scale sample step in the previous reverb; an unchanged control
had a maximum step of 0.018 over the same 64 frames. A 64-sample transition
from the preceding reverb output reduced the observed edited step to 0.011.
It restarts from the latest output on repeated size edits, and is applied
before the wet control. A host test exercises the edit, repeated edits and
immediate wet mute; a separate comparison matched the previous engine bit
for bit at four unchanged sizes, including freeze transitions. These bounded
signals do not prove click-free operation for arbitrary material. During a
separate series of edits eight samples apart, the maximum observed step over
the edits and following settling interval was 0.126; the transition is not a
substitute for target timing or broader live-audio tests.

For the new diagnostic, the QEMU-only impulse MAIN, ELF, recipe and result
SHA-256 values were
`c5ef90fad2f5c85619c4c59fd01dd006b1be1554930879a87e9bfdc84caab8e8`,
`de57e4eeb1fa9bc723afa642dd732cc35cf4a7181ef6d7f42d542853b5614b8b`,
`c9531a8425e93e7c81f48a860340f6a198b7a2ec49cdfc8f6f04fb7399e29651`, and
`a3bc2950faaa050321f8bbef4e6990b46f1e0f429ad2c234839f693d01114a28`.
The separate combined UI MAIN, effect ELF, popup ELF, recipe and result were
`40d29a043b1e046962cb9dc10b0470f58d6455483bfb4a2e34678e4f95e21b12`,
`a197b06af908d2d71758317d46304ff5058e4563d5ec2ae688fd12ba78647a5c`,
`3361d999fb0fd09fd2cb9b2c211940e5734040ff8e203cda48a1b0097782695a`,
`65e4b786ab9bb8b9d7712a21bc9517724b10f7374129367b4ac0da20077cfc64`, and
`2311665d35ab426712ef4cc9c7893837ec3153eba748440e56bfa89d390c4747`.
The impulse peaks matched those above; the UI sequence observed route edits,
drained queues and mixer control after close. Both used the preceding stock
MAIN, NOR and QEMU identities. Neither run qualifies hardware behavior.

Freeze edges now use the same short output transition through both the full
setter and the inspector's live control path. In a host sweep of 64 phases
with a 60 Hz, 0.5-amplitude input, the largest measured freeze-on step fell
from about 0.119 to 0.002 full scale; these observations do not establish
click-free operation for arbitrary material. A regression test compares both
control paths through repeated toggles, and unchanged-size, unchanged-freeze
host renders matched the prior engine bit for bit at four sizes.

The QEMU-only impulse MAIN, ELF, recipe and result SHA-256 values were
`ac504f32382f2c96379d328c0931baa0609af3d59fc69b9f9bbefd9c3115fa34`,
`2ea40a5a2a8d2f64a17a77e0b404ca137b42a761e43d2e4fc45aa74b14dcb89a`,
`cb35b4243ab57fc9b3b3c143e3174aeddd57eabce0fc92acd55b504800aa4052`, and
`8f06714edcebb5dbb5c107070d30e550ec3780e44042b14b5c29b42f940624fa`.
The separate combined UI MAIN, effect ELF, popup ELF, recipe and result were
`b0bc5cd11a4894542a8f7fe8b5162f1dfd8f90b7e999e6550f3705a23d95961d`,
`d38c574012cea1635024d9ffee232e4b0ecf9fa634c35b9d68910d82d7539f39`,
`3361d999fb0fd09fd2cb9b2c211940e5734040ff8e203cda48a1b0097782695a`,
`c52901c9e1cfb164b1cb6cba430774d394f84755b4d6db5891d39bded1079478`, and
`c522ab780331452dfc1c233492b4d777097d88b225f8e2ae706eef7b4d78c029`.
The impulse and UI observations matched the checks described above, using
the same stock MAIN, NOR and unattested QEMU executable. Neither experiment
tested freeze with live audio or qualifies target timing or hardware.

A later control-path change applies Plateau decay, modulation-depth and
freeze encoder edits without recalculating unrelated delay lengths or filter
coefficients in the audio task. A host comparison exercised repeated edits
and 40,000 frames against the full setter with equal output. In a separate
QEMU-only timing-control run, the inspector accepted the three encoder turns
and reported `87.0 %`, `8.3 samples`, and `FROZEN`. This run bypassed effects
DSP, so it does not establish active-audio responsiveness or target timing.
The diagnostic MAIN, ELF, modified recipe and result SHA-256 values were
`8f00b902ccb39d492bbb7d439249176c1ad39d4cd7c29286028b1ce5693df24f`,
`7bf0b7c7ba0a92486a638d1ca601cb12da88ac46fb08cef4fb3440e024192cf5`,
`91213366cdee9c1ba82d9a456127bb3505e21caba1b37950c217e1d564b2bddc`, and
`f792afe653080a8739da60b4711eb7aab49e0e3a2194dff60c56e2975a6d5482`.
The stock MAIN, NOR and QEMU executable match the automatic AINR experiment
above; the QEMU executable's source provenance remains unattested.

- QEMU binary SHA-256:
  `9724d60a31400becad8f1e98d6fd67bb9456c8dfbdeebe833e7636ed8e9cf536`.
  The `l6test` runner records its source provenance as unattested.
- Stock package SHA-256:
  `4e20f92c9c5131ed082c0feec2b2d7d0445fb576296f11f6dace56a4fc0c9b47`.
  The state spec SHA-256 was
  `d3dedc75d71014f1cc59cb35e04ce377214369812b4f5662697dd8d6c36ee485`.
- The headless stock `audio-path` case passed with those inputs. It establishes
  callback and SAI/eDMA observations, not effect output.
- A separate hash-guarded `l6dbg` memory recipe captured `0x20280500` for
  4,096 bytes after 10 guest seconds. The capture matched bytes at MAIN file
  offset `0x105370` throughout. A second recipe read the descriptor table and
  active state; a GDB stop observed the indirect call during a stock boot.
- The recipe and diagnostic JSON hashes for the table capture were
  `daf9ae1876775bd8ff7a8b6862aa93475e767be34dcead45fe2874d81176c29d` and
  `63c54db0e60edfd4736a35b091ee8d562605f7886d5930dd7624139357ca5b50`.
  Local logs and captured firmware remain outside version control.
- A separate diagnostic waited 45 guest seconds, pressed EFX SEL twice with a
  two-second guest-time interval, and read the active descriptor before and
  after each press. The recipe and result hashes were
  `1e7f6cf442730f82da89089a0ace47fef6f32a86705f63becaa06fc19bce5666` and
  `9b437be3551f1d36651fb9073539bc77c97d57f18b7057e80581f37245fe0607`.

Next, establish the exact per-channel send accumulation point, each table
entry's effect selection, callback lifetime and synchronization, and the
scratch format with nonzero signal. Also prove target memory ownership and
worst-case timing before changing or enabling firmware hooks.
