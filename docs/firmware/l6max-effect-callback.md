# L6max 1.10 effect callback investigation

This is a partial map for the L6max MAIN image with SHA-256
`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`.
It is not a specification for L6 or another firmware revision, and does not
qualify a patch for hardware. The `dual-efx` builder remains disabled; see
[safety status](../dual-efx-safety-status.md).

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
before any reuse. In a separate diagnostic that pressed EFX SEL twice, two
breakpoints at the clear routine stopped with return address `0x20280677` in
the stock selection path, active callback `0`, and current task `Main`.
A follow-up observed `Main` at priority 12 and the two stock flags at
`0x80202e68` and `0x8020026c` set to 1 during both stops; the earlier audio
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

## Evidence and limits

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
