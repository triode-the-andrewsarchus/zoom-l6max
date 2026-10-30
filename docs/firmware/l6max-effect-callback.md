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

The Plateau output-tap offsets now cache the calculation for the current
size, including detecting direct writes to the public size field. An 80,000
frame host comparison against the previous implementation, including two
setter edits and one direct size edit while a tail was present, produced
identical output bytes (SHA-256
`5c3eda4dcb26005823c00866b4db50fcf9db0b6ef9984055087bb471356a4c2b`).
A small host timing sample gave median times of 5.81 ms before and 4.87 ms
after for that program; it is not a target timing measurement. The old and
new host executables had SHA-256
`fc3e8f43b08d69af5b8e20802db2dd5b8627136214ab91c651c4f91f13d82102` and
`8b961e33492bad7e2dfa95c970b56f07b6f45a3e0c96d197f6276c8ee3605caf`.

One QEMU active-send ten-edit run with this change completed the recipe but
still did not open the inspector or select Spring. At the final audio snapshot
the diagnostic reported 13,412 callbacks and finite nonzero output. This is
not a controlled timing comparison, and the active-load control failure and
physical deadline remain unresolved. The diagnostic MAIN, ELF, recipe,
result and engine trace SHA-256 values were
`77349a6d69bd4009cd1942c8e0334e12e857744038d94e4c414617d8a0d31cab`,
`036ca9d62e9ed26534d60d4c7c4cf513569ecbe3704e629de0240ec02ee7224c`,
`7f87210e16d8d839e256aea344f60b95446aa60254e2df16934bf7456b96234b`,
`214916fc69957011a7810207aeb9296fe594472952d13354d94f72729e18f8e0`, and
`466c91114c01cf63825565a227ba03463e0b08d8b0538c8191d82ade1c493943`.
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
