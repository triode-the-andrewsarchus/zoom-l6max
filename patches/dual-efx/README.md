# Dual-EFX: Velvet Noise Decorrelator & Plateau Reverb

**Disabled:** The firmware hooks and memory bindings are unsafe and have not
been qualified on hardware. Both patch builders reject this patch until those
issues are fixed and verified. The descriptions and estimates below describe
the intended design; they do not establish implemented behavior or safety.

Replaces the stock mono/reconstructed send effects with high-end modern DSP engines:
1. **Velvet Noise Decorrelator (VND)** (Effect A):
   Ported from [115dkk/Velvet-Noise-Decorrelator-VST3](https://github.com/115dkk/Velvet-Noise-Decorrelator-VST3) to bare-metal `no_std` ARM Cortex-M7. Replaces Spring Reverb (`efx_type = 2`). Generates a sparse, unit-energy impulse sequence to decorrelate stereo audio channels without comb filtering or transient smearing.
2. **Valley Plateau Plate Reverb** (Effect B):
   Ported from [digitalartifactmusic/PlateauNEVersio](https://github.com/digitalartifactmusic/PlateauNEVersio) / Dattorro 1997 topology to bare-metal `no_std` ARM Cortex-M7. Replaces Room Reverb (`efx_type = 1`). Features 4 input diffusers, 2 cross-coupled modulated feedback delay loops, predelay, damping, and freeze mode.
3. **Dual Concurrent Send & Routing Matrix**:
   Allows running both effects simultaneously with flexible per-channel routing (`Neither`, `Send A`, `Send B`, `Send A -> B` [cascade]) and optional Pre-AINR (AI Noise Reduction) chaining.
4. **Extended Parameter Control**:
   Expands beyond the stock 2-parameter limitation using all 8 physical channel encoders and the L6max OLED display for live parameter feedback.

## Model Support

Targeted exclusively for **LiveTrak L6max** firmware revision 1.10 (`980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461`).

## CPU & Memory Budget

The original CPU estimates and 160 KiB memory figure were not measured and
must not be used for target qualification. The processor now includes a
separate Velvet instance for cascaded sends, increasing its state and work.
Its memory ownership, stack use and worst-case execution time remain unproven
on the L6max. The `0x83e00000` state address is unverified on hardware.

## Source Layout

- `src/math.rs`: Fast, standalone math functions (`sin`, `cos`, `sqrt`, `pow10`, `exp2`, `clamp`).
- `src/velvet.rs`: Velvet Noise Decorrelator core DSP engine.
- `src/plateau.rs`: Valley Plateau Dattorro plate reverb core DSP engine.
- `src/routing.rs`: Routing matrix and `DualEfxProcessor` audio engine.
- `src/parameters.rs`: Parameter edits and copies of applied values.
- `src/parameter_view.rs`: Validation and formatting of copied display values.
- `src/firmware.rs`: Hardware and firmware bindings for L6max 1.10.
- `src/trampolines.s`, `src/trampolines.rs`: Displaced instruction veneers.
- `src/payload.rs`: Main payload entrypoints (`efx_dispatch_hook`, `knob_changed`).
- `linker/link.ld`: Payload placement at `PAYLOAD_BASE` (`0x80108000`) and `.state` at `0x83e00000`.
- `hooks.tsv`: Hook table guarding `0x8000277c`.

The standalone inspector parameter model maps encoders 1–8 as follows:

| Encoder | Velvet bank | Plateau bank |
| --- | --- | --- |
| 1 | Amount | Predelay |
| 2 | Time spread | Size |
| 3 | Density | Decay |
| 4 | Decay | Low cut |
| 5 | Refresh time | High cut |
| 6 | Dynamic mode | Diffusion |
| 7 | Transition time | Modulation depth |
| 8 | Cascade amount | Freeze |

For the two on/off settings, clockwise selects on and counterclockwise selects
off. The model and value formatter are host-tested; firmware event handling,
display presentation, bank selection and routing controls are not connected.

## Build & Test

The standalone DSP tests can be run with:

```bash
cargo test --offline --locked -p l6dsp-tests
```

Both patch builders intentionally reject `dual-efx`. The stock emulator's
`audio-path` case checks callback and device observations; it does not test
this effect or qualify its audio behavior. See the
[safety status](../../docs/dual-efx-safety-status.md) for the remaining work.
