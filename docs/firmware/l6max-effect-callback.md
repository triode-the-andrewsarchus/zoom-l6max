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

| Entry | Callback pointer observed |
| --- | --- |
| 0 | `0x20281649` |
| 1 | `0x20280ab9` |
| 2 | `0x20282221` |
| 3 | `0x8002c041` |
| 4 | `0x20280781` |

The active callback pointer is at `0x202a6f20`. Static inspection shows a
wrapper at `0x202800e8` loading that pointer and calling it at `0x20280248`.
Before the call, the wrapper scales and copies two 64-element arrays into
scratch memory; afterward it copies them back with the inverse scale. The
effect code for table entries 1 and 2 also contains 64-iteration floating-point
processing loops. Their functional assignment and behavior require further
runtime tests.

In a stock headless run, a GDB breakpoint at `0x20280248` stopped immediately
before the indirect call. Registers were `r0=0x804683a4`, `r1=0x8045fe98`,
`r2=0x202a6f3c`, and `r12=0x20281649`. At that stop, `r1 + 4` held
`0x202a6f3c`; this is the first of the wrapper's two 64-float scratch arrays.
These arguments point to descriptors and scratch, not the two interleaved
32-float buffers asserted by the old patch. The recorded sample values were
zero, so this observation does not prove nonzero signal routing or audio
fidelity.

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

Next, establish the exact per-channel send accumulation point, each table
entry's effect selection, callback lifetime and synchronization, and the
scratch format with nonzero signal. Also prove target memory ownership and
worst-case timing before changing or enabling firmware hooks.
