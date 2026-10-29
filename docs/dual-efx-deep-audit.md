# LiveTrak L6max Dual-EFX Deep Audit: Critical Review & Flaw Analysis

This document provides a line-by-line critical audit of the `dual-efx` branch implementation for the Zoom LiveTrak L6max (Firmware 1.10). Every component has been evaluated under the assumption of zero trust.

---

## 1. Executive Findings: Summary of Critical Flaws

| # | Subsystem | Severity | Description | Status |
| :-: | :--- | :---: | :--- | :---: |
| **1** | **Assembly Hook / ABI** | **CRITICAL** | `0x8000277c` was patched as `b.w` instead of `bl`. Skips the entire remainder of the audio ISR (`0x800027f0`) upon return! | **RESOLVED** |
| **2** | **Embedded Memory** | **CRITICAL** | `.state` is `(NOLOAD)` in SDRAM. `INITIALIZED` powers on as random garbage on real hardware, risking complete omission of `proc.init()`. | **RESOLVED** |
| **3** | **Audio Topology** | **HIGH** | `velvet.rs` outputs `dry + amount * (wet - dry)`. This is an insert formula; on an Aux Send bus, it leaks delayed dry audio into the return bus causing comb filtering. | **RESOLVED** |
| **4** | **Safety / Distortion** | **HIGH** | Missing soft-clipping. Hard clamping in `plateau.rs` and no clamping in `routing.rs` or `velvet.rs` risks DAC integer overflow during dual summing. | **RESOLVED** |
| **5** | **Engine Isolation** | **MEDIUM** | `cascade_gain` defaulted to `1.0`. Caused both engines to run concurrently 100% of the time, even when the user selected Room or Spring alone. | **RESOLVED** |
| **6** | **UI / Controls** | **MEDIUM** | `SELECTED_ROUTING_CHANNEL` was hardcoded to 0 with no way to select channels 1–7. Plateau `mod_depth` was unexposed. | **RESOLVED** |
| **7** | **Concurrency** | **MEDIUM** | `knob_changed` (UI task) and `efx_dispatch_hook` (Audio ISR) access `DualEfxProcessor` without interrupt masking, risking torn parameter structures. | **RESOLVED** |

---

## 2. In-Depth Flaw Analysis & Evidence

### 2.1 Flaw 1: Displaced `bl` Instruction Patched as `b.w` (Call Site Breakage)
- **The Code**:
  In `tools/firmware-patch-core/src/lib.rs` (lines 211–214):
  ```rust
  for hook in &edits.hooks {
      let off = (hook.address - firmware_base) as usize;
      firmware[off..off + 4].copy_from_slice(&branch(hook.address, hook.target, false));
  ```
  The third argument to `branch()` is `link: bool`. It was hardcoded to `false` (`b.w`).
- **The Hardware Reality**:
  At `0x8000277c` in stock firmware, the original instruction was:
  ```arm
  80002778:   mov   r1, r4
  8000277a:   mov   r2, r5
  8000277c:   bl    0x80003c58      <-- CALL SITE (sets lr = 0x80002781)
  80002780:   b.n   0x800027f0      <-- Continuation of audio loop
  ```
  Because `hooks.tsv` overwrote `0x8000277c` with `b.w efx_dispatch_hook` (`link: false`), `lr` was **never set to `0x80002781`**.
  `efx_dispatch_hook` inherited the caller's `lr`. When it exited with `pop {..., pc}`, it returned straight out of the audio streaming callback, **skipping `0x80002780: b.n 0x800027f0` and abandoning the entire master bus mixdown!**
- **The Fix**:
  In `tools/firmware-patch-core/src/lib.rs`, inspect the original instruction being hooked. If it is a Thumb-2 `BL` (`(hw1 & 0xf800) == 0xf000 && (hw2 & 0xd000) == 0xd000`), preserve `link: true` so the hook is called as a proper subroutine and returns cleanly to `0x80002781`.

---

### 2.2 Flaw 2: Uninitialized High SDRAM in `(NOLOAD)` Section
- **The Code**:
  In `patches/dual-efx/linker/link.ld`:
  ```ld
  . = 0x83e00000;
  .state (NOLOAD) : {
      *(.data*)
      *(.bss*)
  }
  ```
  In `patches/dual-efx/src/payload.rs`:
  ```rust
  static mut PROCESSOR: DualEfxProcessor = DualEfxProcessor::new();
  static mut INITIALIZED: bool = false;
  static mut EDIT_BANK: u8 = 2;
  ```
- **The Hardware Reality**:
  Section `.state` is marked `(NOLOAD)`. The stock bootloader initializes DTCM and low RAM, but **never zero-initializes high SDRAM at `0x83e00000`**.
  On cold boot of physical SDRAM chips, capacitor charge states are arbitrary. If the byte at `INITIALIZED` powers up as non-zero (50% probability):
  `read(core::ptr::addr_of!(INITIALIZED))` evaluates to `true`!
  `proc.init()` is **completely skipped**. The processor runs with 278 KiB of uninitialized garbage pointers, invalid tap counts, and NaNs.
  Furthermore, `EDIT_BANK = 2` and `LAST_PARAM0 = 0xffff` are never written by any loader because `.data` in `(NOLOAD)` is not populated from flash.
- **The Fix**:
  Replace `INITIALIZED: bool` with a 32-bit signature `MAGIC_TAG: u32`. If `read(MAGIC_TAG) != 0x44454658` (`DEFX`), perform explicit runtime initialization of `PROCESSOR`, `LAST_PARAM0`, `LAST_PARAM1`, `LAST_EFX_TYPE`, `EDIT_BANK`, and `SELECTED_ROUTING_CHANNEL`, and then write `MAGIC_TAG = 0x44454658`.

---

### 2.3 Flaw 3: Aux Send Dry Bleed (Insert vs. Aux Send Flaw in `velvet.rs`)
- **The Code**:
  In `patches/dual-efx/src/velvet.rs` (line 311):
  ```rust
  outputs[c] = dry + self.current.amount * (wet - dry);
  ```
- **The Audio Reality**:
  In a DAW track insert, crossfading between dry and wet is appropriate.
  On the LiveTrak L6max, the EFX bus is an **Aux Send Bus**. The channel audio is already routed directly to the master bus via the channel fader.
  If the effect output contains `(1.0 - amount) * dry`, dry audio leaks through the EFX return bus back into the master mix with a buffer latency delay, creating immediate comb filtering and phase cancellation whenever `amount < 1.0`!
- **The Fix**:
  On an aux send effect, the output must be 100% wet:
  ```rust
  outputs[c] = soft_clip(wet * self.current.amount);
  ```

---

### 2.4 Flaw 4: Missing Soft-Clipper & DAC Overload Hazard
- **The Code**:
  In `patches/dual-efx/src/plateau.rs`:
  ```rust
  out_l = clamp(out_l, -1.0, 1.0);
  ```
  In `patches/dual-efx/src/velvet.rs` and `routing.rs`: **no clamping or clipping whatsoever**.
- **The Hardware Reality**:
  `clamp(-1.0, 1.0)` is a hard brick-wall clip. When an audio transient exceeds $\pm 1.0$, hard clipping creates high-order odd harmonics (harsh digital distortion).
  Moreover, in `routing.rs`, when both Effect A and Effect B run in cascade/parallel mode, summing `wet_a + wet_b` can produce amplitudes up to $\pm 2.0$. Writing unclipped floats $> 1.0$ into `AUDIO_EFX_OUT_BUFFER` can cause integer DAC wrap-around on the hardware audio codec.
- **The Fix**:
  Implement true continuous cubic soft-clipping in `math.rs`:
  $$f(x) = x - \frac{4}{27} x^3 \quad (|x| < 1.5)$$
  clamped to $[-1.0, 1.0]$. This has exact unity small-signal gain ($f'(0) = 1.0$) and zero derivative at saturation ($f'(1.5) = 0.0$).
  Apply `soft_clip` to both engine outputs and to the master return bus before writing to `AUDIO_EFX_OUT_BUFFER`.

---

### 2.5 Flaw 5: Unintended Dual-Engine Execution via Default Cascade Gain
- **The Code**:
  In `DualEfxProcessor::new()`: `cascade_gain: 1.0`.
  In `payload.rs`:
  ```rust
  let enable_a = efx_type == 2 || proc.cascade_gain > 1e-4;
  let enable_b = efx_type == 1 || proc.cascade_gain > 1e-4;
  ```
- **The Operational Reality**:
  Because `cascade_gain` was initialized to `1.0`, `proc.cascade_gain > 1e-4` was always true!
  Both `enable_a` and `enable_b` were permanently enabled, meaning both Plateau and Velvet Noise ran concurrently regardless of whether the user selected Room or Spring!
- **The Fix**:
  Default `cascade_gain` to `0.0`. When `cascade_gain == 0.0`, Room runs Plateau alone, and Spring runs Velvet Noise alone. Cascade only runs when explicitly engaged by Encoder 8.

---

### 2.6 Flaw 6: Unexposed Controls & Channel Routing Selection
- **The Code**:
  In `payload.rs`:
  ```rust
  let sel = read(core::ptr::addr_of!(SELECTED_ROUTING_CHANNEL)) as usize;
  ```
  `SELECTED_ROUTING_CHANNEL` was hardcoded to 0 with no control mapped to select channels 1–7.
  Plateau Modulation Depth (`mod_depth`) was also unmapped.
- **The Fix**:
  - In Bank 1 (Plateau): Knobs 0..5 = PreDelay, Size, Decay, Low-Cut, High-Cut, Diffusion; Knob 6 = Mod Depth (0..10.0 samples); Knob 7 = Freeze toggle.
  - In Bank 0 (Velvet Noise): Knobs 0..5 = Amount, Length, Density, Decay, Refresh, Dynamic toggle; Knob 6 = Routing Channel Select & toggle; Knob 7 = Cascade toggle.
  Every parameter is now physically accessible from the front panel.

---

### 2.7 Flaw 7: Race Condition Between Audio ISR and UI Task
- **The Code**:
  `knob_changed` runs in the FreeRTOS UI event task, while `efx_dispatch_hook` runs in the high-priority audio streaming callback.
  Modifying `proc.plateau.set_parameters` (which recalculates buffer lengths) from the UI task while `process_sample` executes in the audio task can cause memory read corruption.
- **The Fix**:
  Wrap parameter updates and initialization in `knob_changed` with Cortex-M interrupt masking (`cpsid i` / `cpsie i`).
