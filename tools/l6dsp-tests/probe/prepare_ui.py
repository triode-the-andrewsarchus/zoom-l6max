#!/usr/bin/env python3
"""Build a disposable QEMU-only combined inspector/popup experiment.

No combined update package is created. This does not qualify physical hardware.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tomllib
from thumb_branch import branch
from fixture import MAIN_SHA, checked_inputs

ROOT = Path(__file__).resolve().parents[3]
PROBE = Path(__file__).resolve().parent

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("firmware", type=Path)
parser.add_argument("state", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
out, main, nor = checked_inputs(parser, args.firmware, args.state, args.output)
for name, expected in (
    ("payload.rs", "9034539aefa64cad6797570ad8de9da9dbe4d51718a81fa774da1ad478441488"),
    ("notification_hooks.rs", "4da2027cf38a3b5a7a6689abf711db553b8e0b8feb77dc75a6cfb05d43efb729"),
):
    source = ROOT / "patches/knob-dialog/src" / name
    if hashlib.sha256(source.read_bytes()).hexdigest() != expected:
        parser.error(f"knob-dialog overlay source mismatch: {name}")

out.mkdir(parents=True)
# Use the existing guarded patch builder for the standard hook map. It emits
# a stock knob-dialog package as an intermediate; remove it immediately.
base = out / "knob-base"
subprocess.run(["cargo", "run", "--offline", "--locked", "-p", "l6patch", "--",
                str(args.firmware.resolve()), str(base), "--patch", "knob-dialog"], cwd=ROOT, check=True)
(base / "L6max.bin").unlink()
report = json.loads((base / "patch.json").read_text())
standard = (base / "main_firmware.bin").read_bytes()
if hashlib.sha256(standard).hexdigest() != report["patched_sha256"] or report["source_sha256"] != MAIN_SHA:
    raise ValueError("standard patch identity mismatch")

# Apply the QEMU-only display extension to a disposable copy of the existing
# popup source. The live knob-dialog source and its normal build are unchanged.
overlay = out / "overlay" / "patches"
shutil.copytree(ROOT / "patches/knob-dialog", overlay / "knob-dialog")
(overlay / "dual-efx/src").mkdir(parents=True)
shutil.copy2(ROOT / "patches/dual-efx/src/parameter_view.rs", overlay / "dual-efx/src")
subprocess.run(["patch", "-p1", "-i", str(PROBE / "knob-inspector.patch")], cwd=overlay.parent, check=True)
channel = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
knob_root = overlay / "knob-dialog"
knob_elf = out / "knob-ui.elf"
subprocess.run([
    "rustup", "run", channel, "rustc", str(knob_root / "src/payload.rs"), "--cfg", "dual_efx",
    f"--remap-path-prefix={knob_root / 'src'}=patches/knob-dialog",
    f"--remap-path-prefix={overlay / 'dual-efx/src'}=patches/dual-efx",
    "--edition=2024", "--target=thumbv7em-none-eabihf", "-C", "opt-level=z",
    "-C", "panic=abort", "-C", "relocation-model=static",
    "-C", "link-arg=--defsym=PAYLOAD_BASE=0x80108000",
    "-C", f"link-arg=-T{knob_root / 'linker/link.ld'}", "-o", str(knob_elf),
], cwd=args.firmware.resolve(), check=True)
knob_blob = out / "knob-ui.bin"
subprocess.run(["llvm-objcopy", "-O", "binary", str(knob_elf), str(knob_blob)], check=True)

def symbols(elf):
    result = {}
    for line in subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True).splitlines():
        fields = line.split()
        if len(fields) == 3:
            result[fields[2]] = int(fields[0], 16) & ~1
    return result

knob_symbols = symbols(knob_elf)
if knob_symbols.get("knob_changed") != 0x80108000 or "inspector_notify_hook" not in knob_symbols:
    raise ValueError("unexpected diagnostic popup symbols")
image = bytearray(standard[:0x108000])
for hook in report["hooks"]:
    address, old, name = hook["address"], hook["target"], hook["symbol"]
    link = "displaced_length" not in hook
    offset = address - 0x80000000
    if image[offset:offset + 4] != branch(address, old, link) or name not in knob_symbols:
        raise ValueError(f"standard popup hook mismatch: {name}")
    image[offset:offset + 4] = branch(address, knob_symbols[name], link)
image.extend(knob_blob.read_bytes())
probe_base = (0x80000000 + len(image) + 15) & ~15
probe_elf = out / "probe.elf"
subprocess.run([
    "rustc", str(PROBE / "callback.rs"), "--cfg", "inspector_probe", "--cfg", "knob_inspector",
    "--edition=2024", "--target=thumbv7em-none-eabihf", "-C", "opt-level=z",
    "-C", "panic=abort", "-C", "relocation-model=static",
    "-C", f"link-arg=--defsym=PROBE_BASE={probe_base:#x}",
    "-C", f"link-arg=--defsym=inspector_notify_hook={knob_symbols['inspector_notify_hook'] | 1:#x}",
    "-C", f"link-arg=-T{PROBE / 'link.ld'}", "-o", str(probe_elf),
], cwd=ROOT, check=True)
probe_blob = out / "probe.bin"
subprocess.run(["llvm-objcopy", "-O", "binary", str(probe_elf), str(probe_blob)], check=True)
probe_symbols = symbols(probe_elf)
if probe_symbols.get("probe") != probe_base or not {"inspector_select", "inspector_knob"} <= probe_symbols.keys():
    raise ValueError("unexpected diagnostic effect symbols")
if probe_base + probe_blob.stat().st_size > 0x801a2ff8:
    raise ValueError("diagnostic payload exceeds application slot")
image.extend(b"\xff" * (probe_base - 0x80000000 - len(image)))
image.extend(probe_blob.read_bytes())
for address, expected, target in (
    (0x80036c74, branch(0x80036c74, report["knob_changed"]), probe_symbols["inspector_knob"]),
    (0x80036f28, bytes.fromhex("1df0bef9"), probe_symbols["inspector_select"]),
):
    offset = address - 0x80000000
    if image[offset:offset + 4] != expected:
        raise ValueError(f"inspector call site mismatch at {address:#x}")
    image[offset:offset + 4] = branch(address, target)
if image[0x107de6:0x107df0] != main[0x107de6:0x107df0]:
    raise ValueError("relocated veneer changed unexpectedly")
image[0x107de6:0x107df0] = bytes.fromhex("2046dff800f0") + struct.pack("<I", probe_base | 1)

firmware = out / "firmware"
firmware.mkdir()
for name in ("main_trailer.bin", "secondary_firmware.bin", "secondary_trailer.bin"):
    shutil.copy2(args.firmware / name, firmware)
(firmware / "main_firmware.bin").write_bytes(image)
state = out / "state"
shutil.copytree(args.state, state)
nor = bytearray(nor)
nor[0x50000:0x50000 + len(image)] = image
(state / "main-nor.bin").write_bytes(nor)

# Reuse the inspector recipe, then capture the OLED before popup expiry.
seed = out / "recipe-seed"
subprocess.run(["python3", str(PROBE / "prepare.py"), str(args.firmware.resolve()),
                str(args.state.resolve()), str(seed), "--callback", "--inspector"], cwd=ROOT, check=True)
recipe = (seed / "recipe.toml").read_text()
seed_hash = hashlib.sha256((seed / "firmware/main_firmware.bin").read_bytes()).hexdigest()
image_hash = hashlib.sha256(image).hexdigest()
recipe = recipe.replace(seed_hash, image_hash).replace("guest_ms = 3000", "guest_ms = 500")
for name in ("room_edit", "spring_edit"):
    marker = f'[[step]]\naction = "memory"\nname = "{name}"'
    snapshot = f'[[step]]\naction = "snapshot"\nname = "{name}_screen"\n'
    if recipe.count(marker) != 1:
        raise ValueError(f"recipe marker mismatch: {name}")
    recipe = recipe.replace(marker, snapshot + marker)
(out / "recipe.toml").write_text(recipe)
print("QEMU-only MAIN SHA-256:", image_hash)
print("Popup ELF SHA-256:", hashlib.sha256(knob_elf.read_bytes()).hexdigest())
print("Effect ELF SHA-256:", hashlib.sha256(probe_elf.read_bytes()).hexdigest())
print("No combined update package was created; never install this diagnostic image.")
