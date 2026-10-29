# Dual-EFX: Velvet Noise Decorrelator & Plateau Reverb

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

- **Target MCU**: NXP i.MX RT1041 / RT1040 (ARM Cortex-M7 @ 600 MHz) with single-cycle `VFMA.F32` hardware FPU.
- **Clock budget**: 12,500 clock cycles per sample at 48 kHz.
- **VND DSP**: ~30 MACs/sample $\approx 1.5\%$ CPU load.
- **Plateau Reverb**: ~120 FLOPs/sample $\approx 3.5\%$ CPU load.
- **Total DSP load**: $<6.0\%$ CPU load total, easily fitting inside the audio interrupt deadline.
- **RAM footprint**: ~160 KiB placed in high SDRAM (`0x83e00000`), zero flash `.data` bloat.

## Source Layout

- `src/math.rs`: Fast, standalone math functions (`sin`, `cos`, `sqrt`, `pow10`, `exp2`, `clamp`).
- `src/velvet.rs`: Velvet Noise Decorrelator core DSP engine.
- `src/plateau.rs`: Valley Plateau Dattorro plate reverb core DSP engine.
- `src/routing.rs`: Routing matrix and `DualEfxProcessor` audio engine.
- `src/parameters.rs`: Parameter scaling and OLED text formatting.
- `src/firmware.rs`: Hardware and firmware bindings for L6max 1.10.
- `src/trampolines.s`, `src/trampolines.rs`: Displaced instruction veneers.
- `src/payload.rs`: Main payload entrypoints (`efx_dispatch_hook`, `knob_changed`).
- `linker/link.ld`: Payload placement at `PAYLOAD_BASE` (`0x80108000`) and `.state` at `0x83e00000`.
- `hooks.tsv`: Hook table guarding `0x8000277c`.

## Build & Test

```bash
# DSP unit tests
cargo test -p l6dsp-tests

# Patch firmware
cargo run -p l6patch -- unpacked-l6max firmware-dual-efx-v113 --patch dual-efx

# Audio path qualification in emulator
cargo run -p l6test -- run --case audio-path --model l6max
```
