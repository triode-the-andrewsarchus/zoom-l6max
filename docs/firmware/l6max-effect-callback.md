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
