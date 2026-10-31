#!/usr/bin/env python3
"""Create an isolated QEMU-only DSP probe from a supported stock fixture."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import shlex
import struct
import subprocess
from thumb_branch import branch
from fixture import MAIN_SHA, checked_inputs, install_ainr_selector_veneer

ROOT = Path(__file__).resolve().parents[3]
PROBE = Path(__file__).resolve().parent
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
parser.add_argument("--inspector", action="store_true", help="enable QEMU-only inspector event hooks")
parser.add_argument("--gesture-trace", action="store_true", help="record selector events (perturbs timing)")
parser.add_argument("--enqueue-trace", action="store_true", help="record panel events before enqueue (perturbs timing)")
parser.add_argument("--irq-trace", action="store_true", help="trace panel bytes in a QEMU-only IRQ hook")
parser.add_argument("--irq-timing", action="store_true", help="use QEMU-only IRQ timestamps for selector gestures")
parser.add_argument("--timing-control", action="store_true", help="bypass effects in a QEMU timing comparison")
parser.add_argument("--active-sends", action="store_true", help="inject nonzero QEMU-only effect buses")
parser.add_argument("--retain-custom-state", action="store_true", help="experimentally retain state across adjacent custom effects")
parser.add_argument("--pre-ainr", action="store_true", help="run a debugger-installed AINR copy on each test send")
parser.add_argument("--effect-buffer", action="store_true", help="place test states in the observed stock effect buffer")
parser.add_argument("--auto-handoff", action="store_true", help="capture and restore AINR state inside the QEMU guest")
parser.add_argument("--passthrough-unready", action="store_true", help="keep synthetic sends active when AINR is unavailable")
parser.add_argument("--bank-stress", action="store_true", help="repeat bank changes and capture state")
parser.add_argument("--bank-stress-count", type=int, default=10, help="number of bank changes (1..64)")
args = parser.parse_args()
if args.retain_custom_state and (not args.inspector or args.pre_ainr):
    parser.error("--retain-custom-state requires --inspector and cannot be combined with --pre-ainr")
if args.switches and not args.callback:
    parser.error("--switches requires --callback")
if args.inspector and not args.callback:
    parser.error("--inspector requires --callback")
if args.inspector and args.switches:
    parser.error("--inspector has its own gesture and bank sequence; omit --switches")
if args.gesture_trace and not args.inspector:
    parser.error("--gesture-trace requires --inspector")
if args.enqueue_trace and not args.inspector:
    parser.error("--enqueue-trace requires --inspector")
if args.irq_trace and not args.inspector:
    parser.error("--irq-trace requires --inspector")
if args.irq_timing and not args.inspector:
    parser.error("--irq-timing requires --inspector")
if args.timing_control and not args.inspector:
    parser.error("--timing-control requires --inspector")
if args.active_sends and not args.inspector:
    parser.error("--active-sends requires --inspector")
if args.active_sends and args.timing_control:
    parser.error("--active-sends cannot be combined with --timing-control")
if args.pre_ainr and (not args.active_sends or not args.inspector or args.bank_stress or args.irq_trace
                      or args.irq_timing or args.gesture_trace or args.enqueue_trace):
    parser.error("--pre-ainr requires --inspector --active-sends and no other trace or stress options")
if args.effect_buffer and not args.pre_ainr:
    parser.error("--effect-buffer requires --pre-ainr")
if args.auto_handoff and not args.effect_buffer:
    parser.error("--auto-handoff requires --effect-buffer")
if args.passthrough_unready and not args.pre_ainr:
    parser.error("--passthrough-unready requires --pre-ainr")
if args.bank_stress and not args.inspector:
    parser.error("--bank-stress requires --inspector")
if not 1 <= args.bank_stress_count <= 64 or (args.bank_stress_count != 10 and not args.bank_stress):
    parser.error("--bank-stress-count requires --bank-stress and a value in 1..64")
out, main, nor = checked_inputs(parser, args.firmware, args.state, args.output)

out.mkdir(parents=True)
firmware = out / "firmware"
state = out / "state"
firmware.mkdir()
shutil.copytree(args.state, state)
for name in ("main_trailer.bin", "secondary_firmware.bin", "secondary_trailer.bin"):
    shutil.copy2(args.firmware / name, firmware)
elf = out / "probe.elf"
blob = out / "probe.bin"
rustc = [
    "rustc", str(PROBE / ("callback.rs" if args.callback else "payload.rs")),
    "--edition=2024", "--target=thumbv7em-none-eabihf",
    "-C", "opt-level=z", "-C", "panic=abort", "-C", "relocation-model=static",
    "-C", f"link-arg=-T{PROBE / 'link.ld'}", "-o", str(elf),
]
if args.inspector:
    rustc += ["--cfg", "inspector_probe"]
if args.gesture_trace:
    rustc += ["--cfg", "gesture_trace"]
if args.enqueue_trace:
    rustc += ["--cfg", "enqueue_trace"]
if args.irq_trace or args.irq_timing:
    rustc += ["--cfg", "irq_trace"]
if args.irq_timing:
    rustc += ["--cfg", "irq_timing"]
if args.timing_control:
    rustc += ["--cfg", "timing_control"]
if args.active_sends:
    rustc += ["--cfg", "active_sends"]
if args.retain_custom_state:
    rustc += ["--cfg", "retain_custom_state"]
if args.pre_ainr:
    rustc += ["--cfg", "pre_ainr_probe"]
if args.effect_buffer:
    rustc += ["--cfg", "effect_buffer_probe"]
if args.auto_handoff:
    rustc += ["--cfg", "auto_handoff_probe"]
if args.passthrough_unready:
    rustc += ["--cfg", "unready_passthrough_probe"]
subprocess.run(rustc, cwd=ROOT, check=True)
subprocess.run(["llvm-objcopy", "-O", "binary", str(elf), str(blob)], check=True)
payload = blob.read_bytes()
if not payload or 0x108000 + len(payload) > 0x1a2ff8:
    raise ValueError("diagnostic payload exceeds the application slot")
image = bytearray(main)
image[0x107de6:0x107df0] = CALLBACK_REDIRECT if args.callback else REDIRECT
if args.inspector:
    # Same Thumb BL encoding as firmware-patch-core::branch, for this
    # disposable diagnostic only. Guard both exact stock call instructions.
    symbols = {}
    for line in subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True).splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] in ("inspector_select", "inspector_knob", "inspector_mode", "inspector_enqueue", "ainr_selector_veneer"):
            symbols[parts[2]] = int(parts[0], 16) & ~1
    for offset, expected, symbol in (
        (0x36f28, bytes.fromhex("1df0bef9"), "inspector_select"),
        (0x36c74, bytes.fromhex("1cf058ff"), "inspector_knob"),
        (0x36f8a, bytes.fromhex("1cf011fe"), "inspector_mode"),
    ):
        if image[offset:offset + 4] != expected or symbol not in symbols:
            raise ValueError(f"inspector call site or symbol mismatch: {symbol}")
        image[offset:offset + 4] = branch(0x80000000 + offset, symbols[symbol])
    install_ainr_selector_veneer(image, symbols)
    if args.enqueue_trace:
        offset = 0x4c606
        if image[offset:offset + 4] != bytes.fromhex("dbf73ffd") or "inspector_enqueue" not in symbols:
            raise ValueError("panel enqueue call site or symbol mismatch")
        image[offset:offset + 4] = branch(0x80000000 + offset, symbols["inspector_enqueue"])
if args.auto_handoff:
    matches = [line.split() for line in subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True).splitlines()
               if line.endswith(" ainr_capture")]
    if len(matches) != 1 or image[0x104de4:0x104dee] != bytes.fromhex("40f2817cc0f2000c6047"):
        raise ValueError("stock AINR veneer or capture symbol mismatch")
    target = int(matches[0][0], 16) | 1
    if not 0x80108001 <= target < 0x80130000:
        raise ValueError("AINR capture target is outside diagnostic payload")
    image[0x104de4:0x104dee] = bytes.fromhex("dff800f0") + struct.pack('<I', target) + bytes.fromhex("00bf")
image.extend(b"\xff" * (0x108000 - len(image)))
image.extend(payload)
if args.effect_buffer:
    from ainr_isolation import cloned_code
    from pre_ainr_observer import relocated
    # The known stock fixture has an erased application tail. No update
    # package is produced, and this layout remains emulator-only.
    tail_start, tail_end = 0x130000, 0x142000
    tail = nor[0x50000 + tail_start:0x50000 + tail_end]
    if len(image) > tail_start or len(tail) != tail_end - tail_start or any(b != 0xff for b in tail):
        raise ValueError("experimental application tail is not available")
    image.extend(b"\xff" * (tail_start - len(image)))
    code = cloned_code(main)
    for index in range(3):
        copy = relocated(code, index, 'effect-buffer')
        image.extend(copy)
        image.extend(b"\xff" * (0x6000 - len(copy)))
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
if args.inspector:
    recipe = f'''version = 1
model = "l6max"
main_sha256 = "{image_sha}"
wall_timeout_s = 300
'''
    def add(action, **fields):
        global recipe
        recipe += '[[step]]\naction = "' + action + '"\n'
        for key, value in fields.items():
            recipe += f'{key} = "{value}"\n' if isinstance(value, str) else f'{key} = {value}\n'

    def wait(ms=2000):
        add("wait", guest_ms=ms)

    def memory(name, address, length):
        add("memory", name=name, address=address, length=length)

    def press(target, hold=150):
        add("press", target=target, hold_ms=hold)
        wait()

    wait(45000)
    press(20)  # EFX mode
    memory("mixer_initial", 0x8045ff90, 1)
    add("encoder", channel=1, steps=8)
    wait()
    memory("mixer_normal", 0x8045ff90, 1)
    press(40)  # Hall -> Room
    memory("active_room", 0x202a6f20, 4)
    press(40, 1500)
    memory("room_open", 0x83e00400, 32)
    stress_prefix = recipe
    add("encoder", channel=1, steps=8)
    wait(3000)
    memory("room_edit", 0x83e00400, 32)
    memory("mixer_room", 0x8045ff90, 1)
    memory("queue_room", 0x83e00200, 12)
    press(40)  # Switch the inspector to Velvet without selecting a new effect.
    memory("active_velvet_bank", 0x202a6f20, 4)
    add("encoder", channel=1, steps=-8)
    wait(3000)
    memory("spring_edit", 0x83e00400, 32)
    memory("mixer_spring", 0x8045ff90, 1)
    memory("queue_spring", 0x83e00200, 12)
    press(40)  # Return to Plateau and check the previous edit survived.
    add("encoder", channel=1, steps=1)
    wait(3000)
    memory("room_return_edit", 0x83e00400, 32)
    press(40)  # Return to Velvet and check its previous edit survived.
    add("encoder", channel=1, steps=1)
    wait(3000)
    memory("spring_return_edit", 0x83e00400, 32)
    memory("route_after_banks", 0x83e00100, 48)
    press(40, 1500)
    memory("closed", 0x83e00400, 32)
    add("encoder", channel=1, steps=8)
    wait(3000)
    memory("mixer_after_close", 0x8045ff90, 1)
    memory("after_close", 0x83e00400, 32)
    if args.bank_stress:
        recipe = stress_prefix
        for i in range(args.bank_stress_count):
            add("press", target=40, hold_ms=150)
            wait(500)
            memory(f"ui_{i}", 0x83e00340, 32)
            add("encoder", channel=1, steps=1)
            wait(500)
            memory(f"edit_{i}", 0x83e00400, 32)
        press(40, 1500)
        memory("ui_after_long", 0x83e00340, 32)
        memory("closed", 0x83e00400, 32)
    if args.gesture_trace:
        memory("gesture_trace", 0x83e00500, 244)
    if args.enqueue_trace:
        memory("enqueue_trace", 0x83e00600, 260)
    if args.irq_trace or args.irq_timing:
        memory("irq_trace", 0x83e00900, 276)
    if args.irq_timing:
        memory("irq_queue", 0x83e00b00, 12)
    if args.active_sends:
        memory("active_audio", 0x83e00800, 16)
    press(40)  # Outside the inspector, EFX SEL still selects Spring.
    if args.bank_stress:
        memory("ui_after_short", 0x83e00340, 32)
    else:
        memory("active_spring", 0x202a6f20, 4)
    memory("selected_after_close", 0x83e00004, 4)
if args.pre_ainr:
    # The observer or auto-handoff prototype installs learned AINR copies.
    recipe = f'''version = 1
model = "l6max"
main_sha256 = "{image_sha}"
wall_timeout_s = 420
'''
    wait(45000)
    if args.auto_handoff:
        memory("heap_before", 0x8020ab1c, 4)
    for _ in range(5):
        add("press", target=40, hold_ms=150)
        wait(3000)
    memory("ainr_selected", 0x202a6f20, 4)
    add("press", target=48, hold_ms=150)
    wait(16000 if args.auto_handoff else 14000)
    memory("learned", 0x8046300c, 4)
    if args.auto_handoff:
        memory("handoff_learned", 0x83e03100, 28)
        memory("heap_learned", 0x8020ab1c, 4)
    add("snapshot", name="learned_ready")
    wait(3000)
    for _ in range(2):
        add("press", target=40, hold_ms=150)
        wait(3000)
    wait(2000)
    memory("selected", 0x83e00000, 12)
    if args.auto_handoff:
        memory("handoff_selected", 0x83e03100, 28)
        memory("heap_selected", 0x8020ab1c, 4)
    memory("ainr_sends_1", 0x83e03000, 28)
    wait(5000)
    memory("ainr_sends_2", 0x83e03000, 28)
    if args.auto_handoff:
        memory("ainr_all_sends", 0x83e01000, 768)
        for index in range(3):
            memory(f"clone_{index}_descriptor", 0x80d5ba0c + index * 0x20000, 132)
    memory("output", 0x83e00800, 16)
(out / "recipe.toml").write_text(recipe)
if args.pre_ainr:
    metadata = {"stock_main_sha256": MAIN_SHA, "diagnostic_main_sha256": image_sha,
                "recipe_sha256": hashlib.sha256(recipe.encode()).hexdigest(),
                "layout": "effect-buffer" if args.effect_buffer else "scratch",
                "auto_handoff": args.auto_handoff,
                "passthrough_unready": args.passthrough_unready}
    if args.effect_buffer:
        matches = [line.split() for line in subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True).splitlines()
                   if line.endswith(" diagnostic_audio")]
        if len(matches) != 1:
            raise ValueError("diagnostic callback symbol missing")
        metadata["callback_address"] = int(matches[0][0], 16) & ~1
        metadata["diagnostic_elf_sha256"] = hashlib.sha256(elf.read_bytes()).hexdigest()
    (out / "pre-ainr.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(f"Input MAIN SHA-256: {MAIN_SHA}")
print(f"Input NOR SHA-256: {hashlib.sha256((args.state / 'main-nor.bin').read_bytes()).hexdigest()}")
print(f"Diagnostic ELF SHA-256: {hashlib.sha256(elf.read_bytes()).hexdigest()}")
print(f"QEMU-only diagnostic MAIN SHA-256: {image_sha}")
command = [
    "cargo", "run", "--offline", "--locked", "-p", "l6dbg", "--", str(out / "recipe.toml"),
    "--firmware-dir", str(firmware), "--state-dir", str(state), "--logs", str(out / "logs"),
    "--qemu", "QEMU_PATH",
]
if args.pre_ainr and not args.auto_handoff:
    command += ["--gdb-socket", str(out / "gdb.sock")]
print("Run: " + shlex.join(command))
if args.pre_ainr and not args.auto_handoff:
    print("Observer: " + shlex.join(["python3", str(PROBE / "pre_ainr_observer.py"), str(args.firmware), str(out)]))
