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

## EFX encoder and stock effect storage observations

With EFX mode selected, turning physical encoders 1 through 8 changed the
stock EFX control bytes accessed by `0x8000b0f0`. Their observed primary
indices were `0, 1, 2, 3, 4, 6, 8, 10` respectively; the paired byte two
positions later also changed. This agrees with the stock physical-channel
mapping at `0x800564b0`. Some values were still changing when the next
encoder was turned, so the captured values are not settled gain measurements.
A separate capture of `0x20016138..0x20016337` showed no changes during the
same style of turns. The relationship from those control bytes to the
candidate accumulation-loop gains remains unresolved; do not use these
observations to substitute the control index for a proven audio-slot mapping.
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
