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
from fixture import MAIN_SHA, checked_inputs, install_ainr_selector_veneer, install_boundary_hook, install_file_mix_handoff, install_mode_hooks, install_offline_mix_hook, install_panel_receive_hooks, validate_probe_entry

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
parser.add_argument("--admission-probe", action="store_true", help="exercise the standalone callback admission model in QEMU")
parser.add_argument("--switches", action="store_true", help="observe all six effect selections")
parser.add_argument("--mixer-input-capture", action="store_true", help="capture routed mixer input and output for each custom effect (QEMU only)")
parser.add_argument("--offline-mix-layout", action="store_true", help="run a destructive synthetic file-mix layout probe in QEMU")
parser.add_argument("--file-mix-handoff", choices=("pending", "bypass"), help="force a late audio signal in the stock file-mix workflow (QEMU only)")
parser.add_argument("--inspector", action="store_true", help="enable QEMU-only inspector event hooks")
parser.add_argument("--gesture-trace", action="store_true", help="record selector events (perturbs timing)")
parser.add_argument("--enqueue-trace", action="store_true", help="record panel events before enqueue (perturbs timing)")
parser.add_argument("--irq-trace", action="store_true", help="trace panel bytes in a QEMU-only IRQ hook")
parser.add_argument("--irq-timing", action="store_true", help="use QEMU-only IRQ timestamps for selector gestures")
parser.add_argument("--ring-timing", action="store_true", help="use QEMU-only receive-slot selector timestamps and sequences")
parser.add_argument("--ring-drop", choices=("pair", "release"), help="QEMU-only forced selector-event loss case")
parser.add_argument("--timing-control", action="store_true", help="bypass effects in a QEMU timing comparison")
parser.add_argument("--active-sends", action="store_true", help="inject nonzero QEMU-only effect buses")
parser.add_argument("--selector-load-trace", action="store_true", help="trace a selector hold under active or bypassed effects")
parser.add_argument("--max-velvet-density", action="store_true", help="initialize Velvet at 95 ms and 3000 pps in the selector-load trace")
parser.add_argument("--submix-return", action="store_true", help="capture active DSP output with the modeled Sub-Mix switch held")
parser.add_argument("--retain-custom-state", action="store_true", help="experimentally retain state across adjacent custom effects")
parser.add_argument("--pre-ainr", action="store_true", help="run a debugger-installed AINR copy on each test send")
parser.add_argument("--effect-buffer", action="store_true", help="place test states in the observed stock effect buffer")
parser.add_argument("--auto-handoff", action="store_true", help="capture and restore AINR state inside the QEMU guest")
parser.add_argument("--channel0-prefix", action="store_true", help="experimentally snapshot only the observed channel-0 state prefix")
parser.add_argument("--hybrid-handoff", action="store_true", help="experimentally split the channel-0 snapshot over several callbacks")
parser.add_argument("--boundary-handoff", action="store_true", help="QEMU-only selector-side snapshot after an audio-boundary pause")
parser.add_argument("--boundary-noise", action="store_true", help="inject nonzero stock AINR input in the boundary experiment")
parser.add_argument("--passthrough-unready", action="store_true", help="keep synthetic sends active when AINR is unavailable")
parser.add_argument("--bank-stress", action="store_true", help="repeat bank changes and capture state")
parser.add_argument("--bank-stress-count", type=int, default=10, help="number of bank changes (1..64)")
args = parser.parse_args()
other_diagnostics = (args.switches, args.inspector, args.mixer_input_capture, args.gesture_trace, args.enqueue_trace,
        args.irq_trace, args.irq_timing, args.ring_timing, args.ring_drop, args.timing_control,
        args.active_sends, args.selector_load_trace, args.max_velvet_density, args.submix_return,
        args.retain_custom_state, args.pre_ainr, args.effect_buffer, args.auto_handoff,
        args.channel0_prefix, args.hybrid_handoff, args.boundary_handoff, args.boundary_noise,
        args.passthrough_unready, args.bank_stress)
if args.file_mix_handoff and (not args.callback or args.admission_probe
        or args.offline_mix_layout or any(other_diagnostics)):
    parser.error("--file-mix-handoff requires --callback alone")
if args.admission_probe and args.callback:
    parser.error("--admission-probe cannot be combined with --callback")
if args.offline_mix_layout and (not args.callback or any(other_diagnostics)):
    parser.error("--offline-mix-layout requires --callback alone")
if args.max_velvet_density and not args.selector_load_trace:
    parser.error("--max-velvet-density requires --selector-load-trace")
if args.mixer_input_capture and (not args.callback or args.switches or args.inspector
        or args.active_sends or args.timing_control or args.pre_ainr or args.submix_return
        or args.bank_stress or args.retain_custom_state or args.selector_load_trace):
    parser.error("--mixer-input-capture requires --callback without other diagnostic variants")
if args.selector_load_trace:
    if (not args.callback or not args.inspector or args.active_sends == args.timing_control
            or args.pre_ainr or args.submix_return or args.bank_stress or args.retain_custom_state
            or args.switches or args.irq_timing or args.ring_timing or args.ring_drop):
        parser.error("--selector-load-trace requires --callback --inspector and exactly one of --active-sends or --timing-control, without other diagnostic variants")
    args.gesture_trace = args.enqueue_trace = args.irq_trace = True
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
if args.ring_timing and (not args.inspector or args.irq_timing):
    parser.error("--ring-timing requires --inspector and excludes --irq-timing")
if args.ring_drop and (not args.ring_timing or args.bank_stress or args.submix_return):
    parser.error("--ring-drop requires --ring-timing and excludes --bank-stress and --submix-return")
if args.timing_control and not args.inspector:
    parser.error("--timing-control requires --inspector")
if args.active_sends and not args.inspector:
    parser.error("--active-sends requires --inspector")
if args.active_sends and args.timing_control:
    parser.error("--active-sends cannot be combined with --timing-control")
if args.submix_return and (not args.active_sends or not args.inspector or args.pre_ainr
                           or args.bank_stress or args.retain_custom_state or args.irq_trace
                           or args.irq_timing or args.ring_timing or args.gesture_trace or args.enqueue_trace):
    parser.error("--submix-return requires --callback --inspector --active-sends without other diagnostic variants")
if args.pre_ainr and (not args.active_sends or not args.inspector or args.bank_stress or args.irq_trace
                      or args.irq_timing or args.ring_timing or args.gesture_trace or args.enqueue_trace):
    parser.error("--pre-ainr requires --inspector --active-sends and no other trace or stress options")
if args.effect_buffer and not args.pre_ainr:
    parser.error("--effect-buffer requires --pre-ainr")
if args.auto_handoff and not args.effect_buffer:
    parser.error("--auto-handoff requires --effect-buffer")
if args.channel0_prefix and not args.auto_handoff:
    parser.error("--channel0-prefix requires --auto-handoff")
if args.hybrid_handoff and not args.channel0_prefix:
    parser.error("--hybrid-handoff requires --channel0-prefix")
if args.boundary_handoff and (not args.channel0_prefix or args.hybrid_handoff):
    parser.error("--boundary-handoff requires --channel0-prefix and excludes --hybrid-handoff")
if args.boundary_noise and not args.boundary_handoff:
    parser.error("--boundary-noise requires --boundary-handoff")
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
if args.mixer_input_capture:
    rustc += ["--cfg", "mixer_input_capture"]
if args.admission_probe:
    rustc += ["--cfg", "admission_probe"]
if args.offline_mix_layout:
    rustc += ["--cfg", "offline_mix_layout"]
if args.file_mix_handoff:
    rustc += ["--cfg", "file_mix_handoff"]
    if args.file_mix_handoff == "bypass":
        rustc += ["--cfg", "file_mix_bypass"]
if args.inspector:
    rustc += ["--cfg", "inspector_probe"]
if args.gesture_trace:
    rustc += ["--cfg", "gesture_trace"]
if args.enqueue_trace:
    rustc += ["--cfg", "enqueue_trace"]
if args.irq_trace or args.irq_timing or args.ring_timing:
    rustc += ["--cfg", "irq_trace"]
if args.irq_timing:
    rustc += ["--cfg", "irq_timing"]
if args.ring_timing:
    rustc += ["--cfg", "ring_timing"]
if args.ring_drop:
    rustc += ["--cfg", f"ring_drop_{args.ring_drop}"]
if args.timing_control:
    rustc += ["--cfg", "timing_control"]
if args.active_sends:
    rustc += ["--cfg", "active_sends"]
if args.max_velvet_density:
    rustc += ["--cfg", "max_velvet_density"]
if args.retain_custom_state:
    rustc += ["--cfg", "retain_custom_state"]
if args.pre_ainr:
    rustc += ["--cfg", "pre_ainr_probe"]
if args.effect_buffer:
    rustc += ["--cfg", "effect_buffer_probe"]
if args.auto_handoff:
    rustc += ["--cfg", "auto_handoff_probe"]
if args.channel0_prefix:
    rustc += ["--cfg", "compact_ainr_probe"]
if args.hybrid_handoff:
    rustc += ["--cfg", "hybrid_ainr_probe"]
if args.boundary_handoff:
    rustc += ["--cfg", "boundary_ainr_probe"]
if args.boundary_noise:
    rustc += ["--cfg", "boundary_noise_probe"]
if args.passthrough_unready:
    rustc += ["--cfg", "unready_passthrough_probe"]
subprocess.run(rustc, cwd=ROOT, check=True)
validate_probe_entry(subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True))
subprocess.run(["llvm-objcopy", "-O", "binary", str(elf), str(blob)], check=True)
payload = blob.read_bytes()
if not payload or 0x108000 + len(payload) > 0x1a2ff8:
    raise ValueError("diagnostic payload exceeds the application slot")
image = bytearray(main)
image[0x107de6:0x107df0] = CALLBACK_REDIRECT if args.callback else REDIRECT
if args.file_mix_handoff:
    symbols = {}
    for line in subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True).splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] in ("mix_handoff_observer", "mix_handoff_bypass"):
            if parts[2] in symbols:
                raise ValueError("duplicate file-mix handoff symbol")
            symbols[parts[2]] = int(parts[0], 16) & ~1
    bypass = symbols.get("mix_handoff_bypass") if args.file_mix_handoff == "bypass" else None
    if args.file_mix_handoff == "bypass" and bypass is None:
        raise ValueError("missing file-mix bypass symbol")
    install_file_mix_handoff(image, symbols.get("mix_handoff_observer"), bypass)
if args.offline_mix_layout:
    matches = [line.split() for line in subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True).splitlines()
               if line.endswith(" offline_mix_veneer")]
    if len(matches) != 1:
        raise ValueError("file-mix veneer symbol missing")
    install_offline_mix_hook(image, int(matches[0][0], 16) & ~1)
if args.inspector:
    # Same Thumb BL encoding as firmware-patch-core::branch, for this
    # disposable diagnostic only. Guard the stock call instructions.
    symbols = {}
    for line in subprocess.check_output(["llvm-nm", "-n", str(elf)], text=True).splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] in ("inspector_select", "inspector_tag_veneer", "inspector_rx_read", "inspector_knob", "inspector_mode", "inspector_enqueue", "ainr_selector_veneer", "before_ainr_disable"):
            symbols[parts[2]] = int(parts[0], 16) & ~1
    hooks = [
        (0x36f28, bytes.fromhex("1df0bef9"), "inspector_tag_veneer" if args.ring_timing else "inspector_select"),
        (0x36c74, bytes.fromhex("1cf058ff"), "inspector_knob"),
    ]
    for offset, expected, symbol in hooks:
        if image[offset:offset + 4] != expected or symbol not in symbols:
            raise ValueError(f"inspector call site or symbol mismatch: {symbol}")
        image[offset:offset + 4] = branch(0x80000000 + offset, symbols[symbol])
    if args.ring_timing:
        install_panel_receive_hooks(image, symbols.get("inspector_rx_read"))
    install_mode_hooks(image, symbols.get("inspector_mode"))
    install_ainr_selector_veneer(image, symbols)
    if args.boundary_handoff:
        install_boundary_hook(image, symbols.get("before_ainr_disable"))
    if args.enqueue_trace or args.ring_timing:
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
def add(action, **fields):
    global recipe
    recipe += '[[step]]\naction = "' + action + '"\n'
    for key, value in fields.items():
        if isinstance(value, bool):
            recipe += f'{key} = {str(value).lower()}\n'
        else:
            recipe += f'{key} = "{value}"\n' if isinstance(value, str) else f'{key} = {value}\n'


if args.mixer_input_capture:
    recipe = f'''version = 1
model = "l6max"
main_sha256 = "{image_sha}"
wall_timeout_s = 420
'''
    add("wait", guest_ms=45000)
    add("press", target=20, hold_ms=150)
    add("encoder", channel=1, steps=8)
    add("wait", guest_ms=2000)
    add("press", target=35, hold_ms=150)
    add("encoder", channel=1, steps=8)
    add("wait", guest_ms=2000)
    for label in ("plateau", "velvet"):
        add("press", target=40, hold_ms=150)
        add("wait", guest_ms=6000)
        for field, address, length in (
            ("state", 0x83e00000, 12), ("marker", 0x83e00800, 8),
            ("sends", 0x83e01000, 768), ("output", 0x83e01400, 512),
            ("source", 0x83e02000, 3072), ("gains", 0x83e02c00, 512),
        ):
            add("memory", name=f"{label}_{field}", address=address, length=length)

if args.inspector:
    recipe = f'''version = 1
model = "l6max"
main_sha256 = "{image_sha}"
wall_timeout_s = 300
'''
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
    if args.irq_trace or args.irq_timing or args.ring_timing:
        memory("irq_trace", 0x83e00900, 276)
    if args.irq_timing:
        memory("irq_queue", 0x83e00b00, 12)
    if args.ring_timing:
        memory("ring_timing", 0x83e02800, 40)
    if args.active_sends:
        memory("active_audio", 0x83e00800, 16)
    press(40)  # Outside the inspector, EFX SEL still selects Spring.
    if args.bank_stress:
        memory("ui_after_short", 0x83e00340, 32)
    else:
        memory("active_spring", 0x202a6f20, 4)
    memory("selected_after_close", 0x83e00004, 4)
    if args.ring_drop:
        # Isolated failure case: lose a short edge/pair after selecting Room,
        # then classify another press and capture both state and injection.
        recipe = f'version = 1\nmodel = "l6max"\nmain_sha256 = "{image_sha}"\nwall_timeout_s = 300\n'
        wait(45000)
        press(40)  # Hall -> Room
        press(40)  # Forced missing pair or release.
        press(40, 1500 if args.ring_drop == "pair" else 150)
        memory("loss_ui", 0x83e00400, 32)
        memory("loss_selected", 0x83e00004, 4)
        memory("loss_injected", 0x83e02c80, 4)
        memory("ring_timing", 0x83e02800, 40)
        memory("irq_trace", 0x83e00900, 276)
        if args.gesture_trace:
            memory("gesture_trace", 0x83e00500, 244)
        if args.enqueue_trace:
            memory("enqueue_trace", 0x83e00600, 260)
        if args.active_sends:
            memory("active_audio", 0x83e00800, 16)
    if args.submix_return:
        recipe = f'version = 1\nmodel = "l6max"\nmain_sha256 = "{image_sha}"\nwall_timeout_s = 360\n'
        wait(45000)
        add("button", target=47, pressed=True)
        wait(2000)
        add("press", target=40, hold_ms=150)  # Hall -> Room
        wait(5000)
        memory("return_flags", 0x804600dd, 2)
        for suffix in ("initial", "final"):
            if suffix == "final":
                wait(5000)
            memory(f"effect_{suffix}", 0x83e00000, 12)
            memory(f"audio_{suffix}", 0x83e00800, 16)
            memory(f"tx_{suffix}", 0x20009740, 4096)
        add("button", target=47, pressed=False)
    if args.selector_load_trace:
        recipe = f'version = 1\nmodel = "l6max"\nmain_sha256 = "{image_sha}"\nwall_timeout_s = 300\n'
        wait(45000)
        press(40)  # Hall -> Room, with the normal post-press wait.
        memory("start", 0x83e00000, 12)
        wait(5000)
        memory("end", 0x83e00000, 12)
        memory("work", 0x83e00800, 16)
        press(40, 1500)
        memory("inspector", 0x83e00400, 32)
        wait(3000)
        memory("inspector_later", 0x83e00400, 32)
        memory("gesture", 0x83e00500, 244)
        memory("enqueue", 0x83e00600, 260)
        memory("irq", 0x83e00900, 276)
        memory("selected", 0x83e00004, 4)
        if args.max_velvet_density:
            memory("tap_count", 0x83e03f00, 4)
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
        if args.hybrid_handoff:
            memory("hybrid_progress_learned", 0x83e03130, 4)
            memory("hybrid_control_learned", 0x83e03134, 4)
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
        if args.hybrid_handoff:
            memory("hybrid_progress_selected", 0x83e03130, 4)
            memory("hybrid_control_selected", 0x83e03134, 4)
        if args.boundary_handoff:
            memory("boundary_handoff", 0x83e03138, 20)
    memory("ainr_sends_1", 0x83e03000, 28)
    wait(5000)
    memory("ainr_sends_2", 0x83e03000, 28)
    if args.auto_handoff:
        memory("ainr_all_sends", 0x83e01000, 768)
        for index in range(3):
            memory(f"clone_{index}_descriptor", 0x80d5ba0c + index * 0x20000, 132)
    memory("output", 0x83e00800, 16)
if args.offline_mix_layout:
    recipe = recipe.replace('wall_timeout_s = 180', 'wall_timeout_s = 300', 1)
    add("memory", name="offline_layout", address=0x83e07000, length=316)
if args.admission_probe:
    recipe = recipe.replace('name = "impulse_peaks"', 'name = "admission_result"', 1)
if args.file_mix_handoff:
    recipe = recipe.replace('wall_timeout_s = 180', 'wall_timeout_s = 400', 1)
    add("memory-write", name="trigger", address=0x81fff000, bytes=[71, 73, 82, 84])
    add("press", target=40, hold_ms=150)
    add("wait", guest_ms=4000)
    add("memory", name="callback_after", address=0x83e00000, length=12)
    add("memory", name="mix_handoff", address=0x83e0a000, length=44)
(out / "recipe.toml").write_text(recipe)
if args.pre_ainr:
    metadata = {"stock_main_sha256": MAIN_SHA, "diagnostic_main_sha256": image_sha,
                "recipe_sha256": hashlib.sha256(recipe.encode()).hexdigest(),
                "layout": "effect-buffer" if args.effect_buffer else "scratch",
                "auto_handoff": args.auto_handoff,
                "passthrough_unready": args.passthrough_unready}
    if args.auto_handoff:
        metadata["snapshot_bytes"] = (0x19514 - 0xb94c if args.boundary_handoff
                                      else 0x19514 if args.channel0_prefix else 0x20000)
        metadata["hybrid_handoff"] = args.hybrid_handoff
        if args.boundary_handoff:
            metadata["boundary_handoff"] = True
            metadata["boundary_noise"] = args.boundary_noise
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
if args.file_mix_handoff or (args.pre_ainr and not args.auto_handoff):
    command += ["--gdb-socket", str(out / "gdb.sock")]
print("Run: " + shlex.join(command))
if args.pre_ainr and not args.auto_handoff:
    print("Observer: " + shlex.join(["python3", str(PROBE / "pre_ainr_observer.py"), str(args.firmware), str(out)]))
