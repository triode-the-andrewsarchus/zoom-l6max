#!/usr/bin/env python3
"""Compare a copied stock AINR with stock under QEMU; never use on hardware."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import signal
import socket
import struct
import subprocess
import time

from fixture import checked_inputs

ROOT = Path(__file__).resolve().parents[3]
RECIPE = Path(__file__).with_name('ainr-isolation.toml')
OFFSETS = [int(x, 16) for x in '''
12 2e 58 e0 148 15c 180 1c4 210 2b0 364 498 62e 788 7cc 7dc
8ac 8fe 950 c58 ca6 d20 db8 e72 f3c f94 1076 1088 11aa 11cc
12c2 160c 1640 1700 17a6 1afa 1c54 1e72 1fac 1fe0 2020 2038
203c 20c2 21a2 2270 2282 2380 2388 23cc 23d0 23f8 2406 2438
243c 2458 2792 2a54 2d38 2e16 34fe 350e 3660
'''.split()]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def cloned_code(main):
    code = bytearray(main[0xee688:0xee688 + 0x5b4c])
    if sha(code) != '0a4bdaf6e401a90c17d1b49e6907b375cb6cd7b4c0f02ee414b963f8610f457f':
        raise ValueError('unexpected stock AINR code')
    for offset in OFFSETS:
        first, second = struct.unpack_from('<HH', code, offset)
        if first != 0xf2c2 or second & 0xf0ff != 0x0028:
            raise ValueError(f'unexpected MOVT at {offset:#x}')
        struct.pack_into('<HH', code, offset, 0xf2c8, (second & 0x0f00) | 0xe0)
    if sha(code) != 'd0ebab6792ca14ddd20b7f27fa9cb27e384e7aa0baeb37cbc6c2427895f0e04d':
        raise ValueError('unexpected relocated code')
    return bytes(code)


class Remote:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(180)
        self.sock.connect(str(path))

    def packet(self, command):
        data = command.encode()
        self.sock.sendall(b'$' + data + b'#' + f'{sum(data) % 256:02x}'.encode())
        while True:
            char = self.sock.recv(1)
            if not char:
                raise EOFError('GDB connection closed')
            if char == b'$':
                break
        data = bytearray()
        while True:
            char = self.sock.recv(1)
            if not char:
                raise EOFError('GDB connection closed')
            if char == b'#':
                break
            data.extend(char)
        checksum = b''
        while len(checksum) < 2:
            char = self.sock.recv(2 - len(checksum))
            if not char:
                raise EOFError('GDB connection closed in checksum')
            checksum += char
        if checksum != f'{sum(data) % 256:02x}'.encode():
            raise ValueError('bad GDB checksum')
        self.sock.sendall(b'+')
        decoded = bytearray()
        i = 0
        while i < len(data):
            if data[i] == 125:
                i += 1
                decoded.append(data[i] ^ 0x20)
            elif data[i] == 42:
                i += 1
                decoded.extend([decoded[-1]] * (data[i] - 29))
            else:
                decoded.append(data[i])
            i += 1
        return decoded.decode()

    def expect(self, command, expected='OK'):
        result = self.packet(command)
        if result != expected:
            raise RuntimeError(f'{command[:20]}: {result}, expected {expected}')

    def reg(self, index):
        return int.from_bytes(bytes.fromhex(self.packet(f'p{index:x}')), 'little')

    def setreg(self, index, value):
        self.expect(f'P{index:x}={struct.pack("<I", value).hex()}')

    def read(self, address, length):
        return b''.join(bytes.fromhex(self.packet(f'm{address + i:x},{min(1024, length - i):x}'))
                        for i in range(0, length, 1024))

    def write(self, address, data):
        for i in range(0, len(data), 1024):
            part = data[i:i + 1024]
            self.expect(f'M{address + i:x},{len(part):x}:{part.hex()}')


def compare(g, code):
    g.expect('Hg1')
    g.expect('Z0,20228d86,2')
    g.packet('c')
    if [g.reg(i) for i in (0, 1, 2, 15)] != [0x20011010, 0x20011010, 0, 0x20228d86]:
        raise RuntimeError('unexpected stock AINR calling convention')
    if struct.unpack('<I', g.read(0x2028ba4c, 4))[0] != 1:
        raise RuntimeError('AINR processor is not learned')
    source = g.read(0x20280000, 0x20000)
    g.write(0x80e00000, source)
    g.write(0x80f00000, code)
    if g.read(0x80e00000, len(source)) != source or g.read(0x80f00000, len(code)) != code:
        raise RuntimeError('clone readback mismatch')
    waveform = struct.pack('<64f', *(0.25 * math.sin(2 * math.pi * i / 16) for i in range(64)))
    outputs = bytearray()
    checkpoints = {0, 15, 31, 63, 95}
    for frame in range(96):
        output = compare_block(g, frame, waveform, frame in checkpoints)
        outputs.extend(output)
    values = struct.unpack(f'<{len(outputs) // 4}f', outputs)
    if not all(math.isfinite(x) for x in values) or not any(values):
        raise RuntimeError('output is silent or non-finite')
    g.expect('D')
    return {'blocks': 96, 'samples': len(values), 'nonzero': sum(x != 0 for x in values),
            'peak': max(abs(x) for x in values), 'output_sha256': sha(outputs),
            'state_checkpoints': sorted(checkpoints)}


def compare_block(g, frame, waveform, checkpoint):
    if frame:
        g.expect('Z0,20228d86,2')
        g.packet('c')
        if g.reg(15) != 0x20228d86 or g.reg(2) != 0:
            raise RuntimeError(f'unexpected entry at block {frame}')
    g.expect('z0,20228d86,2')
    initial_regs = g.packet('g')
    g.write(0x20011010, waveform)
    g.write(0x80e21000, waveform)
    g.expect('Z0,20228d8a,2')
    g.packet('c')
    if g.reg(15) != 0x20228d8a:
        raise RuntimeError(f'unexpected stock return at block {frame}')
    original = g.read(0x20011010, 256)
    post_regs = g.packet('g')
    original_after = g.read(0x20280000, 0x20000) if checkpoint else None
    g.expect('G' + initial_regs)
    for index, value in ((0, 0x80e21000), (1, 0x80e21000), (2, 0),
                         (14, 0x20228d8b), (15, 0x80f00780)):
        g.setreg(index, value)
    g.packet('c')
    if g.reg(15) != 0x20228d8a or g.read(0x80e21000, 256) != original:
        raise RuntimeError(f'clone return or output mismatch at block {frame}')
    if checkpoint:
        check_state(g, frame, original_after)
    g.expect('G' + post_regs)
    g.expect('z0,20228d8a,2')
    return original


def check_state(g, frame, original_after):
    if g.read(0x20280000, 0x20000) != original_after:
        raise RuntimeError(f'clone changed stock memory at block {frame}')
    for offset, length in ((0xb97c, 20), (0xb9a4, 20), (0xb9cc, 32),
                           (0xba0c, 132), (0xbb14, 0xda00)):
        if g.read(0x80e00000 + offset, length) != original_after[offset:offset + length]:
            raise RuntimeError(f'clone state mismatch at block {frame}, {offset:#x}')
    print(f'Checkpoint {frame}: output and sampled state match', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path, help='unpacked stock firmware directory')
    parser.add_argument('state', type=Path, help='stock L6max state directory')
    parser.add_argument('output', type=Path, help='new directory under the system temp directory')
    parser.add_argument('--qemu', required=True, type=Path, help='QEMU executable')
    args = parser.parse_args()
    out, main_image, nor = checked_inputs(parser, args.firmware, args.state, args.output)
    code = cloned_code(main_image)
    qemu = args.qemu.resolve(strict=True)
    socket_path = out / 'gdb.sock'
    if len(os.fsencode(socket_path)) >= 100:
        parser.error('output path is too long for a Unix socket')
    out.mkdir(parents=True)
    shutil.copytree(args.state, out / 'state')
    if (out / 'state' / 'main-nor.bin').read_bytes() != nor:
        raise RuntimeError('state NOR changed while copying')
    recipe = out / 'recipe.toml'
    shutil.copyfile(RECIPE, recipe)
    identities = {'main_sha256': sha(main_image), 'initial_nor_sha256': sha(nor),
                  'qemu_path': str(qemu), 'qemu_sha256': sha(qemu.read_bytes()),
                  'qemu_source': 'unattested', 'recipe_sha256': sha(recipe.read_bytes()),
                  'probe_sha256': sha(Path(__file__).read_bytes()), 'clone_sha256': sha(code),
                  'initial_state_files': {
                      str(path.relative_to(out / 'state')): sha(path.read_bytes())
                      for path in sorted((out / 'state').rglob('*')) if path.is_file()},
                  'firmware_files': {
                      str(path.relative_to(args.firmware)): sha(path.read_bytes())
                      for path in sorted(args.firmware.rglob('*')) if path.is_file()}}
    (out / 'identities.json').write_text(json.dumps(identities, indent=2) + '\n')
    cmd = ['cargo', 'run', '--offline', '--locked', '-p', 'l6dbg', '--', str(recipe),
           '--firmware-dir', str(args.firmware.resolve()), '--state-dir', str(out / 'state'),
           '--logs', str(out / 'logs'), '--qemu', str(qemu), '--gdb-socket', str(socket_path)]
    with (out / 'l6dbg.log').open('w') as log:
        process = subprocess.Popen(cmd, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        try:
            deadline = time.monotonic() + 300
            marker = out / 'logs' / 'learned_ready.txt'
            print('Waiting for learned AINR snapshot; see l6dbg.log for progress.', flush=True)
            while not marker.exists():
                if process.poll() is not None:
                    raise RuntimeError(f'l6dbg exited {process.returncode} before learned snapshot')
                if time.monotonic() >= deadline:
                    raise TimeoutError('learned snapshot not produced in 300 seconds')
                time.sleep(.1)
            result = compare(Remote(socket_path), code)
            process.wait(timeout=180)
            if process.returncode:
                raise RuntimeError(f'l6dbg exited {process.returncode}')
            (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
            print(json.dumps(result, indent=2), flush=True)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()


if __name__ == '__main__':
    main()
