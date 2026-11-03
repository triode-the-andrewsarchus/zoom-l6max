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


def replay_tail(path):
    """Read one captured candidate tail for the QEMU replay argument."""
    try:
        data = Path(path).read_bytes()
    except OSError as error:
        raise argparse.ArgumentTypeError(str(error)) from error
    if len(data) != 0x19514 - 0xb94c:
        raise argparse.ArgumentTypeError('each replay tail must contain exactly 56,264 bytes')
    return data


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


def prepare_states(g, channel0_prefix, replay_tails):
    """Prepare a live reference and clone state at the stopped stock call."""
    original_source = g.read(0x20280000, 0x20000)
    if replay_tails is not None:
        reference_tail, candidate_tail = replay_tails
        if struct.unpack_from('<I', reference_tail, 0xba4c - 0xb94c)[0] != 1:
            raise RuntimeError('replayed reference processor is not enabled')
        original_source = original_source[:0xb94c] + reference_tail + original_source[0x19514:]
        g.write(0x20280000, original_source)
        if g.read(0x20280000 + 0xb94c, len(reference_tail)) != reference_tail:
            raise RuntimeError('replayed reference readback mismatch')
        source = bytes(0xb94c) + candidate_tail + bytes(0x20000 - 0x19514)
    else:
        source = (original_source[:0x19514] + bytes(0x20000 - 0x19514)
                  if channel0_prefix else original_source)
    return original_source, source


def compare(g, code, independent=False, channel0_prefix=False, replay_tails=None,
            inspect_tail=False, trace_control=False):
    g.expect('Hg1')
    g.expect('Z0,20228d86,2')
    g.packet('c')
    if [g.reg(i) for i in (0, 1, 2, 15)] != [0x20011010, 0x20011010, 0, 0x20228d86]:
        raise RuntimeError('unexpected stock AINR calling convention')
    if struct.unpack('<I', g.read(0x2028ba4c, 4))[0] != 1:
        raise RuntimeError('AINR processor is not learned')
    original_source, source = prepare_states(g, channel0_prefix, replay_tails)
    g.write(0x80e00000, source)
    g.write(0x80f00000, code)
    if g.read(0x80e00000, len(source)) != source or g.read(0x80f00000, len(code)) != code:
        raise RuntimeError('clone readback mismatch')
    if trace_control:
        # QEMU-only positive control for the instruction-scoped memory plugin.
        # Save and restore registers while executing a single Thumb load from
        # the cloned state's prefix, then stop at the existing return site.
        registers = g.packet('g')
        g.write(0x80f1ff00, bytes.fromhex('00687047'))
        g.expect('Z0,20228d8a,2')
        call_clone(g, registers, 0x80e00000, 0x80f1ff00)
        if g.reg(15) != 0x20228d8a:
            raise RuntimeError('trace positive-control return mismatch')
        g.expect('z0,20228d8a,2')
        g.expect('G' + registers)
    waveform = struct.pack('<64f', *(0.25 * math.sin(2 * math.pi * i / 16) for i in range(64)))
    other_code = prepare_second_clone(g, source, code) if independent else None
    other_waveform = (struct.pack('<64f', *(0.125 * math.sin(2 * math.pi * i / 11) for i in range(64)))
                      if independent else None)
    reference_state = original_source
    outputs = bytearray()
    other_outputs = bytearray()
    checkpoints = {0, 15, 31, 63, 95}
    for frame in range(96):
        output, other, reference_state = compare_block(g, frame, waveform, frame in checkpoints,
                                                       other_waveform, reference_state, inspect_tail)
        outputs.extend(output)
        other_outputs.extend(other)
    result = summarize_outputs(outputs, other_outputs, checkpoints, other_code)
    g.expect('D')
    return result


def summarize_outputs(outputs, other_outputs, checkpoints, other_code):
    values = struct.unpack(f'<{len(outputs) // 4}f', outputs)
    if not all(math.isfinite(x) for x in values) or not any(values):
        raise RuntimeError('output is silent or non-finite')
    result = {'blocks': 96, 'samples': len(values), 'nonzero': sum(x != 0 for x in values),
              'peak': max(abs(x) for x in values), 'output_sha256': sha(outputs),
              'state_checkpoints': sorted(checkpoints)}
    if other_code is not None:
        other_values = struct.unpack(f'<{len(other_outputs) // 4}f', other_outputs)
        if not all(math.isfinite(x) for x in other_values) or not any(other_values) or outputs == other_outputs:
            raise RuntimeError('second output is silent, non-finite or identical')
        result.update({'second_matches_stock': True, 'second_nonzero': sum(x != 0 for x in other_values),
                       'second_output_sha256': sha(other_outputs), 'second_clone_sha256': sha(other_code)})
    return result


def prepare_second_clone(g, source, code):
    other_code = bytearray(code)
    for offset in OFFSETS:
        first, second = struct.unpack_from('<HH', other_code, offset)
        if first != 0xf2c8 or second & 0xf0ff != 0x00e0:
            raise ValueError(f'unexpected relocated MOVT at {offset:#x}')
        struct.pack_into('<HH', other_code, offset, first, (second & 0x0f00) | 0xe3)
    if sha(other_code) != '4aeb87905ba0cfc270ad8a9d915b15e0322b7161ef2723f5982a8e411553e18f':
        raise ValueError('unexpected second relocated code')
    g.write(0x80e30000, source)
    g.write(0x80f10000, other_code)
    if g.read(0x80e30000, len(source)) != source or g.read(0x80f10000, len(other_code)) != other_code:
        raise RuntimeError('second clone readback mismatch')
    return other_code


def report_tail_differences(g, frame):
    stock = g.read(0x2028b94c, 0xdbc8)
    clone = g.read(0x80e0b94c, 0xdbc8)
    differences = [i for i, (a, b) in enumerate(zip(stock, clone)) if a != b]
    if frame < 4 or differences:
        first = hex(0xb94c + differences[0]) if differences else 'none'
        last = hex(0xb94c + differences[-1]) if differences else 'none'
        print(f'PRE_CALL frame={frame} differing_bytes={len(differences)} first={first} last={last}', flush=True)


def check_clone_output(g, frame, original):
    actual_return = g.reg(15)
    actual = g.read(0x80e21000, 256)
    if actual_return != 0x20228d8a or actual != original:
        differing = [i for i in range(64) if original[4*i:4*i+4] != actual[4*i:4*i+4]]
        first = differing[0] if differing else None
        raise RuntimeError(f'clone return or output mismatch at block {frame}: return={actual_return:#x}, '
                           f'differing_samples={len(differing)}, first_sample={first}, '
                           f'stock_sha256={sha(original)}, clone_sha256={sha(actual)}')


def compare_block(g, frame, waveform, checkpoint, other_waveform, reference_state, inspect_tail=False):
    if frame:
        g.expect('Z0,20228d86,2')
        g.packet('c')
        if g.reg(15) != 0x20228d86 or g.reg(2) != 0:
            raise RuntimeError(f'unexpected entry at block {frame}')
    g.expect('z0,20228d86,2')
    initial_regs = g.packet('g')
    if inspect_tail:
        report_tail_differences(g, frame)
    g.write(0x20011010, waveform)
    g.write(0x80e21000, waveform)
    if other_waveform is not None:
        g.write(0x80e51000, other_waveform)
    g.expect('Z0,20228d8a,2')
    g.packet('c')
    if g.reg(15) != 0x20228d8a:
        raise RuntimeError(f'unexpected stock return at block {frame}')
    original = g.read(0x20011010, 256)
    post_regs = g.packet('g')
    original_after = g.read(0x20280000, 0x20000) if checkpoint or other_waveform is not None else None
    call_clone(g, initial_regs, 0x80e21000, 0x80f00780)
    check_clone_output(g, frame, original)
    if checkpoint:
        check_state(g, frame, original_after)
    other = b''
    if other_waveform is not None:
        other, reference_state = compare_second_clone(g, frame, initial_regs, original_after, checkpoint, reference_state, other_waveform, original)
    g.expect('G' + post_regs)
    g.expect('z0,20228d8a,2')
    return original, other, reference_state


def compare_second_clone(g, frame, initial_regs, original_after, checkpoint, reference_state, waveform, original):
    clone_after = g.read(0x80e00000, 0x20000) if checkpoint else None
    g.write(0x20280000, reference_state)
    g.write(0x20011010, waveform)
    call_clone(g, initial_regs, 0x20011010, 0x780)
    if g.reg(15) != 0x20228d8a:
        raise RuntimeError(f'second stock reference return mismatch at block {frame}')
    reference = g.read(0x20011010, 256)
    reference_state = g.read(0x20280000, 0x20000)
    g.write(0x20280000, original_after)
    g.write(0x20011010, original)
    call_clone(g, initial_regs, 0x80e51000, 0x80f10780)
    if g.reg(15) != 0x20228d8a:
        raise RuntimeError(f'second clone return mismatch at block {frame}')
    other = g.read(0x80e51000, 256)
    if other != reference:
        raise RuntimeError(f'second clone differs from stock at block {frame}')
    if checkpoint:
        if g.read(0x20280000, 0x20000) != original_after or g.read(0x80e00000, 0x20000) != clone_after:
            raise RuntimeError(f'second clone changed checked memory at block {frame}')
        for offset, length in ((0xb97c, 20), (0xb9a4, 20), (0xb9cc, 32), (0xba0c, 132), (0xbb14, 0xda00)):
            if g.read(0x80e30000 + offset, length) != reference_state[offset:offset+length]:
                raise RuntimeError(f'second clone state mismatch block {frame} offset {offset:#x}')
        print(f'Second stock reference and state match at {frame}', flush=True)
    return other, reference_state


def call_clone(g, initial_regs, buffer, entry, channel=0, return_address=0x20228d8a):
    g.expect('G' + initial_regs)
    for index, value in ((0, buffer), (1, buffer), (2, channel), (14, return_address | 1), (15, entry)):
        g.setreg(index, value)
    g.packet('c')


def check_state(g, frame, original_after):
    if g.read(0x20280000, 0x20000) != original_after:
        raise RuntimeError(f'clone changed stock memory at block {frame}')
    for offset, length in ((0xb97c, 20), (0xb9a4, 20), (0xb9cc, 32),
                           (0xba0c, 132), (0xbb14, 0xda00)):
        if g.read(0x80e00000 + offset, length) != original_after[offset:offset + length]:
            raise RuntimeError(f'clone state mismatch at block {frame}, {offset:#x}')
    print(f'Checkpoint {frame}: output and sampled state match', flush=True)


def wait_stereo_input(g, minimum_input):
    # Skip complete stock calls until the requested live-input threshold is met.
    for skipped in range(1200):
        values = struct.unpack('<64f', g.read(0x20011010, 256))
        if all(map(math.isfinite, values)) and max(map(abs, values)) > minimum_input:
            return skipped
        g.expect('z0,20228d86,2')
        g.expect('Z0,20228d8a,2')
        g.packet('c')
        if g.reg(15) != 0x20228d8a:
            raise RuntimeError('unexpected stock return while waiting for input')
        g.expect('z0,20228d8a,2')
        g.expect('Z0,20228d86,2')
        g.packet('c')
    raise RuntimeError('stock input did not reach the requested threshold')


def compare_solo_prefix(g, initial_regs, samples, expected, ret, frame, checkpoint):
    before = (g.read(0x20280000, 0x27000), g.read(0x80e00000, 0x27000)) if checkpoint else None
    g.write(0x80e61000, samples)
    call_clone(g, initial_regs, 0x80e61000, 0x80f10780, 0, ret)
    if g.reg(15) != ret or g.read(0x80e61000, 256) != expected:
        raise RuntimeError(f'channel-zero prefix output mismatch at block {frame}')
    if before is not None and before != (g.read(0x20280000, 0x27000), g.read(0x80e00000, 0x27000)):
        raise RuntimeError(f'channel-zero clone changed checked state at block {frame}')


def compare_stereo_block(g, frame, channel, invert_second, solo_prefix, checkpoints):
    entry = (0x20228d86, 0x20228d94)[channel]
    ret = entry + 4
    if frame or channel:
        g.expect(f'Z0,{entry:x},2')
        g.packet('c')
    buffer = 0x20011010 + 0x100 * channel
    if [g.reg(i) for i in (0, 1, 2, 15)] != [buffer, buffer, channel, entry]:
        raise RuntimeError(f'unexpected entry at block {frame}, channel {channel}')
    g.expect(f'z0,{entry:x},2')
    initial_regs = g.packet('g')
    samples = g.read(buffer, 256)
    values = struct.unpack('<64f', samples)
    if not all(map(math.isfinite, values)):
        raise RuntimeError('nonfinite stock input')
    if channel == 1 and invert_second:
        samples = struct.pack('<64f', *(-0.5 * value for value in values))
        g.write(buffer, samples)
        values = struct.unpack('<64f', samples)
    peak = max(map(abs, values))
    g.write(0x80e60000, samples)
    g.expect(f'Z0,{ret:x},2')
    g.packet('c')
    if g.reg(15) != ret:
        raise RuntimeError('unexpected stock return')
    expected, post_regs = g.read(buffer, 256), g.packet('g')
    if not all(map(math.isfinite, struct.unpack('<64f', expected))):
        raise RuntimeError(f'nonfinite stock output at block {frame}, channel {channel}')
    call_clone(g, initial_regs, 0x80e60000, 0x80f00780, channel, ret)
    if g.reg(15) != ret or g.read(0x80e60000, 256) != expected:
        raise RuntimeError(f'clone output mismatch at block {frame}, channel {channel}')
    if frame in checkpoints:
        for offset, length in ((0xb97c, 20), (0xb9a4, 20), (0xb9cc, 32),
                               (0xba0c, 264), (0xbb14, 0x1b400)):
            if g.read(0x20280000 + offset, length) != g.read(0x80e00000 + offset, length):
                raise RuntimeError(f'clone state mismatch at block {frame}, channel {channel}, {offset:#x}')
    if solo_prefix and channel == 0:
        compare_solo_prefix(g, initial_regs, samples, expected, ret, frame, frame in checkpoints)
    g.expect('G' + post_regs)
    g.expect(f'z0,{ret:x},2')
    return samples, expected, peak


def prepare_stereo_clones(g, code, solo_prefix, solo_tail):
    """Copy the full stereo state and optional bounded channel-zero state."""
    source = g.read(0x20280000, 0x27000)
    g.write(0x80e00000, source)
    g.write(0x80f00000, code)
    if g.read(0x80e00000, len(source)) != source or g.read(0x80f00000, len(code)) != code:
        raise RuntimeError('stereo clone readback mismatch')
    if solo_prefix:
        start = 0xb94c if solo_tail else 0
        prepare_second_clone(g, bytes(start) + source[start:0x19514] + bytes(0x27000 - 0x19514), code)


def compare_stereo(g, code, minimum_input, invert_second, solo_prefix, blocks, solo_tail=False):
    """Mirror both natural stock calls in QEMU, including both working areas."""
    g.expect('Hg1')
    g.expect('Z0,20228d86,2')
    g.packet('c')
    if [g.reg(i) for i in (0, 1, 2, 15)] != [0x20011010, 0x20011010, 0, 0x20228d86]:
        raise RuntimeError('unexpected first stock AINR call')
    if struct.unpack('<I', g.read(0x2028ba4c, 4))[0] != 1:
        raise RuntimeError('AINR processor is not learned')
    skipped = wait_stereo_input(g, minimum_input)
    if struct.unpack('<I', g.read(0x2028ba4c, 4))[0] != 1:
        raise RuntimeError('AINR processor left the learned state while waiting for input')
    prepare_stereo_clones(g, code, solo_prefix, solo_tail)
    outputs = [bytearray(), bytearray()]
    inputs = [bytearray(), bytearray()]
    peaks = [0.0, 0.0]
    checkpoints = {0, (blocks - 1) // 3, 2 * (blocks - 1) // 3, blocks - 1}
    for frame in range(blocks):
        for channel in (0, 1):
            samples, expected, peak = compare_stereo_block(g, frame, channel, invert_second, solo_prefix, checkpoints)
            peaks[channel] = max(peaks[channel], peak)
            inputs[channel].extend(samples)
            outputs[channel].extend(expected)
    if invert_second and (inputs[0] == inputs[1] or outputs[0] == outputs[1]):
        raise RuntimeError('stereo input or output channels were not distinguishable')
    g.expect('D')
    return {'blocks_per_channel': blocks, 'input_peaks': peaks, 'skipped_blocks': skipped,
            'input_sha256': [sha(samples) for samples in inputs],
            'output_sha256': [sha(output) for output in outputs],
            'inverted_second': invert_second, 'solo_prefix_bytes': 0x19514 if solo_prefix and not solo_tail else None,
            'solo_tail_bytes': 0xdbc8 if solo_tail else None,
            'state_checkpoints': sorted(checkpoints), 'matches': True}


def parse_arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path, help='unpacked stock firmware directory')
    parser.add_argument('state', type=Path, help='stock L6max state directory')
    parser.add_argument('output', type=Path, help='new directory under the system temp directory')
    parser.add_argument('--qemu', required=True, type=Path, help='QEMU executable')
    parser.add_argument('--independent', action='store_true', help='compare a second clone using a different signal')
    parser.add_argument('--inspect-tail', action='store_true', help='report candidate-tail differences before each clone call')
    parser.add_argument('--trace-control', action='store_true', help='execute one QEMU-only watched-prefix read as a plugin positive control')
    parser.add_argument('--channel0-prefix', action='store_true', help='zero the unobserved state suffix in the copied states')
    parser.add_argument('--replay-tails', nargs=2, type=replay_tail, metavar=('REFERENCE', 'CANDIDATE'),
                        help='QEMU-only: replay captured reference and candidate tails')
    parser.add_argument('--stereo', action='store_true', help='compare both natural stock AINR calls')
    parser.add_argument('--invert-second', action='store_true',
                        help='in QEMU, invert and halve second-channel input before both calls')
    solo = parser.add_mutually_exclusive_group()
    solo.add_argument('--solo-prefix', dest='solo', action='store_const', const='prefix',
                      help='also compare a channel-zero-only clone with a 0x19514-byte prefix')
    solo.add_argument('--solo-tail', dest='solo', action='store_const', const='tail',
                      help='also compare a channel-zero-only clone with only its candidate tail')
    parser.add_argument('--blocks', type=int, default=96, help='blocks per channel in stereo mode (1-1000)')
    parser.add_argument('--recipe', type=Path, help='override diagnostic recipe')
    parser.add_argument('--minimum-input', type=float, default=0.0,
                        help='require a finite stock sample larger than this magnitude in stereo mode')
    args = parser.parse_args()
    if args.stereo and any((args.independent, args.channel0_prefix, args.replay_tails,
                            args.inspect_tail, args.trace_control)):
        parser.error('--stereo cannot be combined with mono comparison options')
    if args.invert_second and not args.stereo:
        parser.error('--invert-second requires --stereo')
    if args.solo and not args.stereo:
        parser.error('--solo-prefix and --solo-tail require --stereo')
    if not 1 <= args.blocks <= 1000 or (args.blocks != 96 and not args.stereo):
        parser.error('--blocks must be 1-1000 and requires --stereo when changed')
    if not math.isfinite(args.minimum_input) or args.minimum_input < 0 or (args.minimum_input and not args.stereo):
        parser.error('--minimum-input must be finite, nonnegative and used with --stereo')
    return parser, args


def main():
    parser, args = parse_arguments()
    replay_tails = args.replay_tails
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
    shutil.copyfile(args.recipe or RECIPE, recipe)
    identities = {'main_sha256': sha(main_image), 'initial_nor_sha256': sha(nor),
                  'qemu_path': str(qemu), 'qemu_sha256': sha(qemu.read_bytes()),
                  'qemu_source': 'unattested', 'recipe_sha256': sha(recipe.read_bytes()),
                  'probe_sha256': sha(Path(__file__).read_bytes()), 'clone_sha256': sha(code),
                  'probe_options': {'inspect_tail': args.inspect_tail, 'trace_control': args.trace_control,
                                    'stereo': args.stereo, 'solo': args.solo,
                                    'invert_second': args.invert_second,
                                    'blocks': args.blocks, 'minimum_input': args.minimum_input},
                  'initial_state_files': {
                      str(path.relative_to(out / 'state')): sha(path.read_bytes())
                      for path in sorted((out / 'state').rglob('*')) if path.is_file()},
                  'firmware_files': {
                      str(path.relative_to(args.firmware)): sha(path.read_bytes())
                      for path in sorted(args.firmware.rglob('*')) if path.is_file()}}
    if replay_tails is not None:
        identities['replay_tails'] = {'reference_sha256': sha(replay_tails[0]),
                                      'candidate_sha256': sha(replay_tails[1]),
                                      'bytes': len(replay_tails[0]),
                                      'reference_context': 'live guest prefix and suffix'}
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
            if args.stereo:
                result = compare_stereo(Remote(socket_path), code, args.minimum_input, args.invert_second,
                                        bool(args.solo), args.blocks, args.solo == 'tail')
            else:
                result = compare(Remote(socket_path), code, args.independent, args.channel0_prefix,
                                 replay_tails, args.inspect_tail, args.trace_control)
            result['snapshot_bytes'] = (0x27000 if args.stereo else len(replay_tails[1]) if replay_tails is not None
                                        else 0x19514 if args.channel0_prefix else 0x20000)
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
