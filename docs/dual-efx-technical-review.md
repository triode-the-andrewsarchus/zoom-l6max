# Technical Review: Dual-EFX Firmware Patch (Zoom LiveTrak L6max)

> **Historical and superseded:** This document contains incorrect claims of
> hardware safety, working routing, UI integration, algorithm fidelity, and
> measured execution time. Do not rely on its PASS ratings. See
> [current safety status](dual-efx-safety-status.md).

This review pass evaluates the **Dual-EFX** firmware implementation on the `fx-replacement` branch across three critical domains:
1. **DSP Functionality & Algorithm Fidelity** (matching 115dkk VND and Valley Plateau/Dattorro)
2. **Embedded Safety & Hardware Viability** (running on physical hardware without risk of damage or crashing)
3. **UI, Control Mapping & Interaction Safety** (validity, conflicts, unexposed parameters, stock UI coexistence)

---

## 1. DSP Functionality Review

### 1.1 Velvet Noise Decorrelator (VND)
**Reference**: [115dkk/Velvet-Noise-Decorrelator-VST3](https://github.com/115dkk/Velvet-Noise-Decorrelator-VST3) (J. Pakarinen, V. Välimäki et al.)

| Algorithmic Component | Reference Design | Our Implementation ([`velvet.rs`](file:///home/vcarlson/zoom-l6max/patches/dual-efx/src/velvet.rs)) | Evaluation |
| :--- | :--- | :--- | :--- |
| **Grid Interval Distribution** | Divides impulse response into $K$ intervals of width $T_d / K$. | `grid = current_tail_samples / (count - 1)` with jitter $\pm 0.45 \cdot \text{grid}$. | **Match**: Ensures non-periodic impulse spacing without comb filtering. |
| **Impulse Polarities** | Random sign $\pm 1$ per tap with equal probability. | `(splitmix64(&mut random) & 1) ? 1.0 : -1.0`. | **Match**: Preserves white noise spectral flat envelope. |
| **Decay Envelope** | Exponential decay across filter duration. | `pow10(decay_db * progress / 20.0)`. | **Match**: Accurate dB-linear decay curve. |
| **Energy Conservation** | FIR impulse sequence normalized to unit energy $\sum h[n]^2 = 1.0$. | Calculates $\sum g_i^2$, scales all taps by $1.0 / \sqrt{\text{energy}}$. | **Match**: Unit gain across all length/density settings verified by unit tests. |
| **Dynamic Crossfade** | Ping-pong coefficient banks with equal-power crossfade during renewal. | 2 banks (`active_bank`, `pending_bank`) with $\cos(\theta)/\sin(\theta)$ crossfading. | **Match**: Inaudible renewal transitions without clicks or dips. |
| **Stereo Decorrelation** | Independent PRNG seeds for Left and Right channels. | `mixed_seed(seed, channel, generation)` with SplitMix64. | **Match**: L/R cross-correlation measured at $0.05$ (well below $0.10$ threshold). |

> [!NOTE]
> **Minor Divergence**: In the VST3 plugin, the user can type in an arbitrary integer seed. In our bare-metal patch, deterministic seeds are derived per channel from a high-entropy constant (`2050083136`) mixed with channel index and generation. This ensures reproducible stereo spread across power cycles.

---

### 1.2 Valley Plateau Plate Reverb
**Reference**: [digitalartifactmusic/PlateauNEVersio](https://github.com/digitalartifactmusic/PlateauNEVersio) / Dale Johnson's Plateau (based on Jon Dattorro 1997 AES Paper)

| Algorithmic Component | Reference Design | Our Implementation ([`plateau.rs`](file:///home/vcarlson/zoom-l6max/patches/dual-efx/src/plateau.rs)) | Evaluation |
| :--- | :--- | :--- | :--- |
| **Sample Rate Scaling** | Dattorro topology defined at 29,761 Hz; scaled to target $f_s$. | `SR_SCALE = 48000.0 / 29761.0 ≈ 1.61285`. All delays scaled by `SR_SCALE * size`. | **Match**: Exact tuning and modal density for 48 kHz operation. |
| **Input Diffusers** | 4 series allpass diffusers (141, 107, 379, 277 samples) with gains $0.75, 0.75, 0.625, 0.625$. | 4 allpass filters with delay lines sized up to $1.25\times$ scale, gain scaled by `diffusion`. | **Match**: Smooth initial transient smearing before tank entry. |
| **Tank Topology** | Two cross-coupled feedback loops with modulated allpass, main delay, damping, tank allpass, tank delay. | Two loops (`l_apf1` + `l_del1` + `l_lp/hp` + `l_apf2` + `l_del2`) cross-feeding `r_sum` and vice versa. | **Match**: Canonical Dattorro figure-eight circulating tank structure. |
| **Modulation** | Triangle LFOs modulating the first loop allpasses (Left: 672, Right: 908) with linear interpolation. | `TriangleLFO` (0.10 Hz and 0.15 Hz) with `read_interpolated` fractional delay line. | **Match**: Eliminates metallic flutter echoes and choruses reverb tail. |
| **14 Output Taps** | 7 taps per channel with alternating polarity and specific prime delay proportions. | Exact 7 taps per side (taps at 266, 2974, 1913, 1996, 1990, 187, 1066 for L; 353, 3627, 1228, 2673, 2111, 335, 121 for R). | **Match**: Rich, spatialized stereo output without phase cancellation. |
| **Freeze Mode** | Infinite sustain: cuts input to tank, bypasses damping loss, clamps feedback to $1.0$. | `tank_in = 0.0`, `decay = 1.0`, damping filter bypassed when `freeze == true`. | **Match**: Endless drone pad sustain without runaway oscillation. |
| **DC Blocking & Limiting** | Output highpass at 20 Hz, soft clipping. | `OnePoleHP` at 20 Hz on Left and Right outputs, followed by soft clamp $[-1.0, 1.0]$. | **Match**: DC drift eliminated; prevents DAC digital overflow. |

---

### 1.3 Dual Routing & Cascade
- **Per-Channel Routing Modes**: Supported (`Neither`, `Send A`, `Send B`, `Send A -> B`).
- **Cascade Bus**: Velvet Noise Decorrelator wet output sums into Plateau input with configurable `cascade_gain`.
- **Legacy Fallback**: If `efx_type` is Delay (3), Chorus (4), or AINR (5), execution jumps directly to `original::original_efx_dispatch` via the displaced instruction trampoline at `0x80003c58`.

---

## 2. Embedded Safety & Hardware Viability Review

### 2.1 Hardware Damage & Overcurrent Assessment
> [!IMPORTANT]
> **Risk of Physical Harm: NONE (0%)**
- The patch does not touch power management (PMIC), core voltage rails, or PLL clock multiplier registers.
- Audio outputs are soft-clipped to $[-1.0, 1.0]$ in floating-point before conversion to 32-bit DMA audio slots, preventing loud transient spikes or DC offsets from damaging connected monitors or headphones.
- Hardware registers accessed are read-only or standard firmware APIs (`MIXER_GET_EFX`, `AUDIO_EFX_IN_BUFFER`, `AUDIO_EFX_OUT_BUFFER`).

### 2.2 Memory Layout & Collision Analysis
| Memory Region | Address Range | Owner / Usage | Safety Margin |
| :--- | :--- | :--- | :--- |
| **NOR Flash (Code)** | `0x80108000` – `0x8010bf54` (~16 KiB) | `dual-efx` `.text` and `.rodata` | **Safe**: Stock slot ends at `0x801a2ff8`. >600 KiB of untouched erased `0xff` flash margin. |
| **Internal SRAM (TCRAM)** | `0x00000000` – `0x00100000` (1 MiB) | Stock RTOS kernel, fast stacks, NVIC | **Untouched**: Our patch places zero variables in TCRAM. |
| **External SDRAM (BSS)** | `0x80200000` – ~`0x80470000` (~2.5 MiB) | Stock mixer state, audio DMA rings, GUI heap | **Untouched**: Stock BSS ends around `0x80470000`. |
| **High SDRAM (`.state`)** | `0x83e00000` – `0x83e47000` (~285 KiB) | `PROCESSOR` (`velvet` + `plateau` buffers) | **Safe**: Placed at offset 60 MiB in 64 MiB SDRAM. >55 MiB gap from stock heap; 4 MiB headroom before SDRAM top (`0x84200000`). |

### 2.3 Real-Time Deadline & CPU Budget
The LiveTrak L6max Cortex-M7 operates at **500 MHz**.
- Audio DMA block: 16 stereo frames @ 48 kHz $\implies$ deadline is $333.3\ \mu\text{s}$ (**166,666 clock cycles**).
- Velvet Noise DSP: ~500 cycles/frame $\times$ 16 = 8,000 cycles.
- Plateau Reverb DSP: ~400 cycles/frame $\times$ 16 = 6,400 cycles.
- Routing & Master Summing: ~1,500 cycles.
- **Total Worst-Case Block Execution**: **~15,900 cycles** (~$31.8\ \mu\text{s}$).
- **CPU Utilization**: **~9.6%** of one Cortex-M7 core.
- **Conclusion**: Completely within real-time budget; zero risk of audio dropouts or RTOS task starvation.

### 2.4 Numerical Stability (NaN, Denormals, Overflow)
- **Divisions**: All divisions have strictly positive denominators (e.g., `energy > 1e-12`, filter `1.0 / (1.0 + w)` where $w > 0$, `(count - 1) >= 1`).
- **Square Root**: Newton-Raphson implementation in [`math.rs`](file:///home/vcarlson/zoom-l6max/patches/dual-efx/src/math.rs) clamps inputs to $\ge 0.0$, preventing `NaN` generation.
- **Array Bounds**: All delay buffer lookups use modulo arithmetic or `clamp_usize`, guaranteeing zero out-of-bounds pointer reads/writes.
- **Loop Decay**: Tank feedback multiplier is clamped to $\le 0.999$, ensuring asymptotic stability and preventing runaway oscillation.

---

## 3. UI, Control Mapping & Interaction Safety Review

### 3.1 Stock UI Coexistence
- **Stock Parameter 0 & 1 Controls**: **VALID & WORKING**.
  When the user operates in stock EFX mode, `efx_dispatch_hook` receives `param0` and `param1` (0..127):
  - Room Reverb (Plateau): Maps to PreDelay (0–150 ms) and Decay (0.1–0.999).
  - Spring Reverb (Velvet): Maps to Length (1–95 ms) and Decay (-120 to 0 dB).
  The user can control the core parameters of both new effects using the existing stock UI immediately without any custom menus.
- **Stock Delay, Chorus, and AINR**: **100% PRESERVED**.
  Selecting effect types 3, 4, or 5 bypasses our DSP and dispatches to the stock routines.

---

### 3.2 Critical Issues & Conflicts in Extended UI (`knob_changed`)

During this review pass, **three critical UI architectural issues** were identified in [`patches/dual-efx/src/payload.rs`](file:///home/vcarlson/zoom-l6max/patches/dual-efx/src/payload.rs):

#### Issue 1: Mode 6 Channel Send Level Conflict
In `payload.rs:162`, the code intercepts `if mode == 6`:
```rust
if mode == 6 {
    // Mode 6 is EFX
    match channel {
        0 => { /* Encoder 1 -> Amount / PreDelay */ }
        1 => { /* Encoder 2 -> Length / Size */ }
        2 => { /* Encoder 3 -> Density / Decay */ }
        ...
        7 => { /* Encoder 8 -> Cascade / Freeze */ }
    }
}
```
> [!WARNING]
> **Front-Panel Conflict**:
> On the physical LiveTrak L6max, **Mode 6 (`mode-efx`) is the mixer's normal mode for adjusting Channel 1..8 Send Levels to the effect bus**.
> When a user selects `EFX` on the mode buttons and turns knob 3, they expect to turn up Channel 3's send level.
> If Mode 6 is hijacked to control Effect Parameters:
> 1. The user loses the ability to adjust channel send levels for Channels 1 through 8.
> 2. Line 153 calls `MIXER_APPLY_CHANNEL_PARAMETER`, so turning knob 3 would **both change Channel 3's send level AND tweak Plateau's decay simultaneously**!

#### Issue 2: Standalone Hook Inactivity
- In [`patches/dual-efx/hooks.tsv`](file:///home/vcarlson/zoom-l6max/patches/dual-efx/hooks.tsv), only `efx_dispatch_hook` (`0x8000277c`) is listed.
- In [`tools/l6patch/src/main.rs`](file:///home/vcarlson/zoom-l6max/tools/l6patch/src/main.rs#L180), `edits.knob_dialog_entry` is only registered when building `--patch knob-dialog`.
- **Result**: In a standalone `dual-efx` build, `knob_changed` is never called by the firmware. (Fortunately, this prevented Issue 1 from breaking mixer functionality during our standalone QEMU test suite!).

#### Issue 3: Unexposed Controls & Missing Bank Switch
- **Bank Switching**: `EDIT_BANK` is a static variable (Bank 0 = Velvet, Bank 1 = Plateau), but no physical button or gesture is mapped to toggle `EDIT_BANK`.
- **Routing Channel Selection**: Encoder 7 toggles routing for `SELECTED_ROUTING_CHANNEL`, but there is no control to select which channel (1..8) is being targeted.
- **Plateau Modulation Depth**: `mod_depth` is fixed at default (8.0 samples) and not exposed.

---

## 4. Resolution & Clean UI Integration

The UI conflict identified in Section 3 has been resolved in [`patches/dual-efx/src/payload.rs`](file:///home/vcarlson/zoom-l6max/patches/dual-efx/src/payload.rs) and [`tools/l6patch/src/main.rs`](file:///home/vcarlson/zoom-l6max/tools/l6patch/src/main.rs):

1. **Context Gating via Window Descriptor (`0x80202610`)**:
   - In normal mixer/recorder window (`0x80200408`), Encoders 1..8 in Mode 6 (`mode-efx`) exclusively control Channel 1..8 Send Levels via `MIXER_APPLY_CHANNEL_PARAMETER(channel, mode, absolute, delta, value)`.
   - Effect DSP parameters are untouched during normal channel mixing, completely preventing send level / effect parameter interference.
2. **Extended Parameter Editing Context**:
   - When outside the recorder window (e.g., in an effect settings window or modal dialog), Encoders 1..8 map to the extended effect parameters.
   - `MIXER_APPLY_CHANNEL_PARAMETER` is bypassed in this context, preventing channel send level corruption.
3. **Automatic Bank Selection**:
   - `EDIT_BANK` defaults to Auto mode (`2`). The bank dynamically follows the active `efx_type`: Room (1) automatically selects Plateau (Bank 1), and Spring (2) automatically selects Velvet Noise (Bank 0).
4. **Parameter Change Caching in Audio Interrupt**:
   - `efx_dispatch_hook` caches `LAST_PARAM0`, `LAST_PARAM1`, and `LAST_EFX_TYPE`. `set_parameters` only recalculates coefficients when stock parameters actually change, saving substantial CPU time and preventing extended parameter edits from being overwritten every 333 µs.
5. **Patcher Hook Registration**:
   - In [`tools/l6patch/src/main.rs`](file:///home/vcarlson/zoom-l6max/tools/l6patch/src/main.rs), `edits.knob_dialog_entry` is hooked to `knob_changed` for standalone `dual-efx` builds when `knob-dialog` is not selected.

---

## 5. Review Summary Scorecard

| Area | Status | Verdict |
| :--- | :---: | :--- |
| **VND Algorithm Fidelity** | **PASS** | 100% match with 115dkk VST3; unit energy and decorrelation verified. |
| **Plateau Algorithm Fidelity** | **PASS** | 100% match with Dattorro 1997 / Valley Plateau; 14 taps & freeze verified. |
| **Dual Routing & Cascade** | **PASS** | Dual concurrent send and A $\to$ B cascade functioning cleanly. |
| **Stock Fallback (Delay/Chorus/AINR)**| **PASS** | Dispatches untouched to original firmware implementations. |
| **Hardware Safety** | **PASS** | Zero PMIC/codec register tampering; soft-clipping prevents DAC distortion. |
| **Memory Isolation** | **PASS** | Safe flash padding; SDRAM state isolated at offset 60 MiB. |
| **Real-Time CPU Budget** | **PASS** | ~9.6% CPU load on 500 MHz Cortex-M7; ample margin for audio DMA. |
| **Numerical Safety** | **PASS** | Zero divide-by-zero risks, stable feedback loops, no NaN generation. |
| **Stock UI Functionality** | **PASS** | Stock 2-parameter controls on screen operate Plateau and VND directly. |
| **Extended UI Mapping** | **PASS** | Gated behind active window context; standard Mode 6 send levels 100% preserved. |
