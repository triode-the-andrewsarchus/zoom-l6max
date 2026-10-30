# QEMU-only Cortex-M7 DSP probe

This diagnostic runs the real DSP initializer and a 2,048-frame impulse in a
stock effect buffer during stock effect initialization. It modifies copied
emulator firmware and NOR, temporarily replaces stock effect-buffer contents,
writes a result to QEMU-only scratch at `0x83e00000`, and does not create an
update package. The scratch address is not validated on physical hardware. **Do not install its output on a
physical device.** The supported patch builders still reject `dual-efx`.

From the repository root, using a disposable L6max 1.10 state whose installed
MAIN matches the unpacked stock image:

```sh
python3 tools/l6dsp-tests/probe/prepare.py UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-dsp-probe
cargo run --offline --locked -p l6dbg -- /tmp/l6-dsp-probe/recipe.toml \
  --firmware-dir /tmp/l6-dsp-probe/firmware --state-dir /tmp/l6-dsp-probe/state \
  --logs /tmp/l6-dsp-probe/logs --qemu QEMU_PATH
sha256sum QEMU_PATH
python3 -c 'import json,struct; j=json.load(open("/tmp/l6-dsp-probe/logs/diagnostic.json")); b=next(e["observation"]["bytes"] for e in j["events"] if e["observation"].get("name")=="impulse_peaks"); print(struct.unpack("<ffI",bytes(b)))'
```

The output directory must be new and under the system temporary directory.
The preparer verifies the full stock MAIN hash, the copied NOR's installed
MAIN and the displaced veneer hash. It writes only to the new directory and
reports the input NOR, probe ELF and modified MAIN hashes. Record those and the
exact QEMU hash with any result. The Rust toolchain, target and `llvm-objcopy`
must be installed; Cargo and `l6dbg` use the repository's pinned toolchain.

An observed result is approximately `(0.30966336, 0.01861336, 1180192817)`:
the first two numbers are Velvet and Plateau peak magnitudes and the last is
the `0x46585031` completion marker. A successful recipe only collects this
observation. It does not prove live signal routing, effect-switch safety,
callback timing, UI behavior, or physical-device compatibility.

## Automatic callback diagnostic

For a separate disposable run, pass `--callback` to the preparer. Its recipe
presses EFX SEL once, then reads three 32-bit words from QEMU-only scratch:

```sh
python3 tools/l6dsp-tests/probe/prepare.py UNPACKED_DIR STOCK_STATE_DIR /tmp/l6-callback-probe --callback
cargo run --offline --locked -p l6dbg -- /tmp/l6-callback-probe/recipe.toml \
  --firmware-dir /tmp/l6-callback-probe/firmware --state-dir /tmp/l6-callback-probe/state \
  --logs /tmp/l6-callback-probe/logs --qemu QEMU_PATH
python3 -c 'import json,struct; j=json.load(open("/tmp/l6-callback-probe/logs/diagnostic.json")); b=next(e["observation"]["bytes"] for e in j["events"] if e["observation"].get("name")=="callback_state"); print(struct.unpack("<III",bytes(b)))'
```

The observed marker is `0x494e4954`, the selected descriptor is `0x2028b8dc`,
and the third word is the callback count, which should be nonzero. This
diagnostic replaces the selected stock callback and uses its effect buffer;
it processes only the already-summed stereo bus. It does not implement or
validate channel routing, effect controls, UI, synchronization, real-time
deadlines, or hardware safety. Never install the generated image.
