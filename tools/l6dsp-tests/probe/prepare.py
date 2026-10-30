#!/usr/bin/env python3
"""Create an isolated QEMU-only DSP probe from a supported stock fixture."""
import argparse
import hashlib
from pathlib import Path
import shutil
import shlex
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
PROBE = Path(__file__).resolve().parent
MAIN_SHA = "980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461"
VENEER_SHA = "c366ef222fb2b2b87302d6eae469b721f11ba6b41dd79d4b7fe16a582e332c13"
# Thumb MOVW/MOVT/BX redirect to diagnostic payload 0x80108001.
REDIRECT = bytes.fromhex("48f2010cc8f2100c6047")
# Pass the stock selector's descriptor pointer in r0, then branch to payload.
CALLBACK_REDIRECT = bytes.fromhex("2046dff800f001801080")

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("firmware", type=Path, help="unpacked stock firmware directory")
parser.add_argument("state", type=Path, help="disposable stock emulator state directory")
parser.add_argument("output", type=Path, help="new directory under the system temporary directory")
parser.add_argument("--callback", action="store_true", help="observe automatic effect callback installation")
parser.add_argument("--switches", action="store_true", help="observe all six effect selections")
args = parser.parse_args()
if args.switches and not args.callback:
    parser.error("--switches requires --callback")
out = args.output.resolve()
if not out.is_relative_to(Path(tempfile.gettempdir()).resolve()):
    parser.error("output must be under the system temporary directory")
if out.exists():
    parser.error("output must not already exist")
for source in (args.firmware.resolve(), args.state.resolve()):
    if out.is_relative_to(source) or source.is_relative_to(out):
        parser.error("output must be separate from input directories")
main = (args.firmware / "main_firmware.bin").read_bytes()
nor = (args.state / "main-nor.bin").read_bytes()
if hashlib.sha256(main).hexdigest() != MAIN_SHA:
    parser.error("stock MAIN SHA-256 does not match L6max 1.10")
if nor[0x50000:0x50000 + len(main)] != main:
    parser.error("state NOR does not contain the selected stock MAIN")
if hashlib.sha256(main[0x107de6:0x107df0]).hexdigest() != VENEER_SHA:
    parser.error("stock clear veneer does not match")
if len(main) > 0x108000:
    parser.error("stock MAIN overlaps probe payload")

out.mkdir(parents=True)
firmware = out / "firmware"
state = out / "state"
firmware.mkdir()
shutil.copytree(args.state, state)
for name in ("main_trailer.bin", "secondary_firmware.bin", "secondary_trailer.bin"):
    shutil.copy2(args.firmware / name, firmware)
elf = out / "probe.elf"
blob = out / "probe.bin"
subprocess.run([
    "rustc", str(PROBE / ("callback.rs" if args.callback else "payload.rs")),
    "--edition=2024", "--target=thumbv7em-none-eabihf",
    "-C", "opt-level=z", "-C", "panic=abort", "-C", "relocation-model=static",
    "-C", f"link-arg=-T{PROBE / 'link.ld'}", "-o", str(elf),
], cwd=ROOT, check=True)
subprocess.run(["llvm-objcopy", "-O", "binary", str(elf), str(blob)], check=True)
payload = blob.read_bytes()
if not payload or 0x108000 + len(payload) > 0x1a2ff8:
    raise ValueError("diagnostic payload exceeds the application slot")
image = bytearray(main)
image[0x107de6:0x107df0] = CALLBACK_REDIRECT if args.callback else REDIRECT
image.extend(b"\xff" * (0x108000 - len(image)))
image.extend(payload)
(firmware / "main_firmware.bin").write_bytes(image)
nor = bytearray(nor)
nor[0x50000:0x50000 + len(image)] = image
(state / "main-nor.bin").write_bytes(nor)
image_sha = hashlib.sha256(image).hexdigest()
recipe = f'''version = 1
model = "l6max"
main_sha256 = "{image_sha}"
wall_timeout_s = 180
''' + ('''[[step]]
action = "wait"
guest_ms = 45000
[[step]]
action = "press"
target = 40
hold_ms = 150
[[step]]
action = "wait"
guest_ms = 3000
[[step]]
action = "memory"
name = "callback_state"
address = 0x83e00000
length = 12
''' if args.callback else '''
[[step]]
action = "wait"
guest_ms = 15000
[[step]]
action = "memory"
name = "impulse_peaks"
address = 0x83e00000
length = 12
''')
if args.switches:
    recipe += '''[[step]]
action = "memory"
name = "active_1"
address = 0x202a6f20
length = 4
'''
    for i in range(2, 7):
        recipe += f'''[[step]]
action = "press"
target = 40
hold_ms = 150
[[step]]
action = "wait"
guest_ms = 3000
[[step]]
action = "memory"
name = "callback_state_{i}"
address = 0x83e00000
length = 12
[[step]]
action = "memory"
name = "active_{i}"
address = 0x202a6f20
length = 4
'''
(out / "recipe.toml").write_text(recipe)
print(f"Input MAIN SHA-256: {MAIN_SHA}")
print(f"Input NOR SHA-256: {hashlib.sha256((args.state / 'main-nor.bin').read_bytes()).hexdigest()}")
print(f"Diagnostic ELF SHA-256: {hashlib.sha256(elf.read_bytes()).hexdigest()}")
print(f"QEMU-only diagnostic MAIN SHA-256: {image_sha}")
print("Run: " + shlex.join([
    "cargo", "run", "--offline", "--locked", "-p", "l6dbg", "--", str(out / "recipe.toml"),
    "--firmware-dir", str(firmware), "--state-dir", str(state), "--logs", str(out / "logs"),
    "--qemu", "QEMU_PATH",
]))
