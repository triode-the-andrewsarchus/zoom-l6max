#!/usr/bin/env python3
"""Replay stock and paired AINR calls in a disposable QEMU guest only.

This rewrites guest code and state through GDB. Never connect to hardware.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct

from ainr_isolation import Remote

CALL = 0x20228d86
AFTER = 0x20228d98
VENEER = 0x2023039c
BUFFER = 0x20011010
STATE = 0x20280000
STATE_SIZE = 0x27000
GATE = 0x83e0b000
REJECTED = 0x83e0b208
STOCK_PAIR = bytes.fromhex('07f009fb42f6105001225044014607f002fb')
PAIRED_CALL = STOCK_PAIR[:4] + bytes.fromhex('00bf') * 7
STOCK_VENEER = bytes.fromhex('40f2817cc0f2000c6047')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def registers(raw):
    return [int.from_bytes(bytes.fromhex(raw[i:i + 8]), 'little') for i in range(0, 128, 8)]


def stop_at(g, address):
    g.expect(f'Z0,{address:x},2')
    g.packet('c')
    if g.reg(15) != address:
        raise RuntimeError(f'unexpected stop instead of {address:#x}')


def check_closed_pair(g, waveform, state, block):
    gate, _, ready = struct.unpack('<3I', g.read(GATE, 12))
    if gate != 0 or ready != 0x47415445:
        raise RuntimeError('diagnostic gate is not initialized and open')
    rejected = struct.unpack('<2I', g.read(REJECTED, 8))
    g.write(GATE, struct.pack('<I', 4))
    g.packet('c')
    if (g.reg(15) != AFTER or g.reg(0) != 0 or g.read(BUFFER, len(waveform)) != waveform
            or g.read(STATE, STATE_SIZE) != state or struct.unpack('<I', g.read(GATE, 4))[0] != 4
            or struct.unpack('<2I', g.read(REJECTED, 8)) != tuple((x + 1) & 0xffffffff for x in rejected)):
        raise RuntimeError(f'closed admission did not pass through both channels in block {block}')
    g.write(GATE, struct.pack('<I', gate))


def compare_block(g, expected_veneer, waveform, block, check_closed):
    stop_at(g, CALL)
    if [g.reg(i) for i in (0, 1, 2)] != [BUFFER, BUFFER, 0]:
        raise RuntimeError(f'unexpected caller registers in block {block}')
    if struct.unpack('<I', g.read(0x8046300c, 4))[0] != 1:
        raise RuntimeError('AINR learned marker is not set')
    g.expect(f'z0,{CALL:x},2')
    g.expect(f'Z0,{AFTER:x},2')
    if g.read(CALL, len(PAIRED_CALL)) != PAIRED_CALL or g.read(VENEER, 10) != expected_veneer:
        raise RuntimeError('guest code does not match the supplied diagnostic MAIN')
    state = g.read(STATE, STATE_SIZE)
    stack_start = g.reg(13) - 0x2000
    stack = g.read(stack_start, 0x2400)
    initial_regs = g.packet('g')
    g.write(BUFFER, waveform)
    g.write(CALL, STOCK_PAIR)
    g.write(VENEER, STOCK_VENEER)
    g.packet('c')
    if g.reg(15) != AFTER:
        raise RuntimeError('stock pair did not reach its return site')
    stock_regs = g.packet('g')
    stock_output = g.read(BUFFER, len(waveform))
    stock_state = g.read(STATE, STATE_SIZE)
    g.write(STATE, state)
    g.write(stack_start, stack)
    g.write(BUFFER, waveform)
    g.write(CALL, PAIRED_CALL)
    g.write(VENEER, expected_veneer)
    g.expect('G' + initial_regs)
    if check_closed:
        check_closed_pair(g, waveform, state, block)
        g.write(STATE, state)
        g.write(stack_start, stack)
        g.write(BUFFER, waveform)
        g.expect('G' + initial_regs)
    g.packet('c')
    if g.reg(15) != AFTER:
        raise RuntimeError('paired call did not reach its return site')
    paired_regs = g.packet('g')
    if g.read(BUFFER, len(waveform)) != stock_output or g.read(STATE, STATE_SIZE) != stock_state:
        raise RuntimeError(f'output or sampled state mismatch in block {block}')
    before, after = registers(stock_regs), registers(paired_regs)
    differences = {str(i): [hex(a), hex(b)] for i, (a, b) in enumerate(zip(before, after)) if a != b}
    g.expect(f'z0,{AFTER:x},2')
    remaining = {str(i // 2): [stock_regs[i:i + 8], paired_regs[i:i + 8]]
                 for i in range(128, min(len(stock_regs), len(paired_regs)), 8)
                 if stock_regs[i:i + 8] != paired_regs[i:i + 8]}
    if len(stock_regs) != len(paired_regs):
        raise RuntimeError('debugger register packet lengths differ')
    return stock_output, differences, remaining


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('socket', type=Path, help='GDB socket of disposable QEMU guest')
    parser.add_argument('main_image', type=Path, help='exact diagnostic MAIN used by that guest')
    parser.add_argument('--blocks', type=int, default=16, help='number of replayed blocks (1..96)')
    parser.add_argument('--check-closed', action='store_true', help='also force a closed gate and check stereo passthrough')
    args = parser.parse_args()
    if not 1 <= args.blocks <= 96:
        parser.error('--blocks must be between 1 and 96')
    image = args.main_image.read_bytes()
    veneer = image[0x104de4:0x104dee]
    if image[0xfd7ce:0xfd7e0] != PAIRED_CALL or len(veneer) != 10 or veneer[:4] != bytes.fromhex('dff800f0'):
        parser.error('MAIN does not contain the paired AINR diagnostic')
    waveform = struct.pack('<128f', *([0.25 * math.sin(2 * math.pi * i / 16) for i in range(64)]
                                     + [0.15 * math.sin(2 * math.pi * i / 11) for i in range(64)]))
    g = Remote(args.socket)
    g.expect('Hg1')
    outputs = bytearray()
    differences = []
    remaining_differences = []
    nonzero = [0, 0]
    for block in range(args.blocks):
        output, changed, remaining = compare_block(g, veneer, waveform, block, args.check_closed)
        values = struct.unpack('<128f', output)
        if not all(math.isfinite(value) for value in values):
            raise RuntimeError(f'non-finite output in block {block}')
        for channel in range(2):
            nonzero[channel] += sum(value != 0 for value in values[channel * 64:(channel + 1) * 64])
        outputs.extend(output)
        differences.append(changed)
        remaining_differences.append(remaining)
    if not all(nonzero):
        raise RuntimeError('output is silent in at least one channel')
    g.expect('D')
    print(json.dumps({'main_sha256': digest(image), 'blocks': args.blocks,
                      'input_sha256': digest(waveform), 'output_sha256': digest(outputs),
                      'nonzero_samples': nonzero, 'register_differences': differences,
                      'remaining_register_packet_differences': remaining_differences,
                      'closed_passthrough_checked': args.check_closed}, indent=2))


if __name__ == '__main__':
    main()
