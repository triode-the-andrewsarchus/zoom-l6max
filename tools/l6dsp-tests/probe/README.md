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
