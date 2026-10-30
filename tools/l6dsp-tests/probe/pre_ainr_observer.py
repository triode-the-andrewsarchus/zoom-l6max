#!/usr/bin/env python3
"""Install experimental AINR copies in a disposable QEMU guest only."""
import argparse
import json
from pathlib import Path
import struct
import tempfile
import time

from ainr_isolation import OFFSETS, Remote, cloned_code, sha
from fixture import MAIN_SHA

CODE_HASHES = {
    'scratch': (
        'd0ebab6792ca14ddd20b7f27fa9cb27e384e7aa0baeb37cbc6c2427895f0e04d',
        '4aeb87905ba0cfc270ad8a9d915b15e0322b7161ef2723f5982a8e411553e18f',
        '530044eac1f551d30a01a7da9cce31e64d3fc02a8f842c75d7ad559792d033a6',
    ),
    'effect-buffer': (
        'f523ada9cde8a466c6c8c2a71bc3b1290097c973bd05752b9914a5457a986eea',
        '1d67b28c04f4a36fbe6dc81d23cb876d37f2f2c6f108a6e51650d0c13f6aca6a',
        '6dfdd8610b9febeffce237d79ebb4b1f4f8664cbbeed2a6dd77a5c333ef84c8f',
    ),
}


def relocated(code, index, layout='scratch'):
    result = bytearray(code)
    for offset in OFFSETS:
        first, second = struct.unpack_from('<HH', result, offset)
        if first != 0xf2c8 or second & 0xf0ff != 0x00e0:
            raise ValueError(f'unexpected relocated MOVT at {offset:#x}')
        immediate = 0xe0 + 3 * index if layout == 'scratch' else 0xd5 + 2 * index
        struct.pack_into('<HH', result, offset, first, (second & 0x0f00) | immediate)
    if sha(result) != CODE_HASHES[layout][index]:
        raise ValueError('unexpected relocated AINR code')
    return result


def learned_source(g):
    if [g.reg(i) for i in (0, 1, 2, 15)] != [0x20011010, 0x20011010, 0, 0x20228d86]:
        raise RuntimeError('unexpected stock AINR calling convention')
    if struct.unpack('<I', g.read(0x202a6f20, 4))[0] != 0x20280021:
        raise RuntimeError('stock AINR is not selected')
    if struct.unpack('<I', g.read(0x2028ba4c, 4))[0] != 1:
        raise RuntimeError('stock AINR processor is not enabled')
    if struct.unpack('<I', g.read(0x8046300c, 4))[0] != 1:
        raise RuntimeError('stock AINR learning did not complete')
    return g.read(0x20280000, 0x20000)


def wait_effect_callback(g, address):
    g.expect(f'Z0,{address:x},2')
    try:
        g.packet('c')
        if g.reg(15) != address:
            raise RuntimeError('diagnostic callback was not reached')
        if struct.unpack('<II', g.read(0x83e00000, 8)) != (0x494e4954, 0x2028b8dc):
            raise RuntimeError('Room effect state is not initialized')
        size = struct.unpack('<I', g.read(0x83e0301c, 4))[0]
        if size == 0 or size > 0x50000:
            raise RuntimeError('effects state does not fit before AINR copies')
        return g.read(0x80d00000, size)
    except BaseException:
        g.expect(f'z0,{address:x},2')
        raise


def copy_states(g, source, code, layout):
    for index in range(3):
        state_addr = (0x80d50000 + index * 0x20000 if layout == 'effect-buffer'
                      else 0x80e00000 + index * 0x30000)
        code_addr = (0x80130000 + index * 0x6000 if layout == 'effect-buffer'
                     else 0x80f00000 + index * 0x10000)
        copy = relocated(code, index, layout)
        g.write(state_addr, source)
        if layout == 'scratch':
            g.write(code_addr, copy)
        if g.read(state_addr, len(source)) != source or g.read(code_addr, len(copy)) != copy:
            raise RuntimeError(f'copy {index} readback mismatch')
        print(f'Copy {index} code SHA-256: {sha(copy)}', flush=True)


def install(g, main, metadata):
    g.expect('Hg1')
    g.expect('Z0,20228d86,2')
    breakpoint = 0x20228d86
    try:
        g.packet('c')
        source = learned_source(g)
        code = cloned_code(main)
        layout = metadata.get('layout', 'scratch')
        if layout == 'effect-buffer':
            g.expect('z0,20228d86,2')
            breakpoint = None
            effects_before = wait_effect_callback(g, metadata['callback_address'])
            breakpoint = metadata['callback_address']
        copy_states(g, source, code, layout)
        if layout == 'effect-buffer' and g.read(0x80d00000, len(effects_before)) != effects_before:
            raise RuntimeError('copying AINR state changed effects state')
        g.write(0x83e03000, bytes(28))
        g.write(0x83e00144, struct.pack('<I', 0x41494e52))
    finally:
        if breakpoint is not None:
            g.expect(f'z0,{breakpoint:x},2')
        g.expect('D')


def prepared_metadata(out):
    metadata = json.loads((out / 'pre-ainr.json').read_text())
    if metadata.get('auto_handoff', False):
        raise ValueError('auto-handoff diagnostics do not use the observer')
    if metadata.get('layout', 'scratch') not in CODE_HASHES:
        raise ValueError('unknown pre-AINR layout')
    diagnostic_main = (out / 'firmware' / 'main_firmware.bin').read_bytes()
    nor = (out / 'state' / 'main-nor.bin').read_bytes()
    if (metadata.get('stock_main_sha256') != MAIN_SHA
            or metadata.get('diagnostic_main_sha256') != sha(diagnostic_main)
            or metadata.get('recipe_sha256') != sha((out / 'recipe.toml').read_bytes())
            or nor[0x50000:0x50000 + len(diagnostic_main)] != diagnostic_main):
        raise ValueError('prepared diagnostic, recipe or NOR does not match its manifest')
    if metadata.get('layout') == 'effect-buffer':
        address = metadata.get('callback_address')
        if (not isinstance(address, int) or address & 1 or not 0x80108000 <= address < 0x80130000
                or metadata.get('diagnostic_elf_sha256') != sha((out / 'probe.elf').read_bytes())):
            raise ValueError('effect-buffer callback does not match its manifest')
    return metadata


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('firmware', type=Path, help='unpacked stock firmware directory')
    parser.add_argument('output', type=Path, help='prepared QEMU-only probe directory under temporary storage')
    args = parser.parse_args()
    out = args.output.resolve(strict=True)
    if not out.is_relative_to(Path(tempfile.gettempdir()).resolve()):
        parser.error('probe directory must be under the system temporary directory')
    main_image = (args.firmware / 'main_firmware.bin').read_bytes()
    if sha(main_image) != MAIN_SHA:
        parser.error('stock MAIN SHA-256 does not match L6max 1.10')
    try:
        metadata = prepared_metadata(out)
    except ValueError as error:
        parser.error(str(error))
    deadline = time.monotonic() + 350
    marker = out / 'logs' / 'learned_ready.txt'
    while not marker.exists():
        if (out / 'logs' / 'diagnostic.json').exists():
            raise RuntimeError('diagnostic finished before learned snapshot')
        if time.monotonic() >= deadline:
            raise TimeoutError('learned snapshot not produced in 350 seconds')
        time.sleep(0.1)
    install(Remote(out / 'gdb.sock'), main_image, metadata)
    print('AINR copies installed in QEMU scratch; observer SHA-256:', sha(Path(__file__).read_bytes()), flush=True)


if __name__ == '__main__':
    main()
