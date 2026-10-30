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

CODE_HASHES = (
    'd0ebab6792ca14ddd20b7f27fa9cb27e384e7aa0baeb37cbc6c2427895f0e04d',
    '4aeb87905ba0cfc270ad8a9d915b15e0322b7161ef2723f5982a8e411553e18f',
    '530044eac1f551d30a01a7da9cce31e64d3fc02a8f842c75d7ad559792d033a6',
)


def relocated(code, index):
    result = bytearray(code)
    for offset in OFFSETS:
        first, second = struct.unpack_from('<HH', result, offset)
        if first != 0xf2c8 or second & 0xf0ff != 0x00e0:
            raise ValueError(f'unexpected relocated MOVT at {offset:#x}')
        struct.pack_into('<HH', result, offset, first, (second & 0x0f00) | (0xe0 + 3 * index))
    if sha(result) != CODE_HASHES[index]:
        raise ValueError('unexpected relocated AINR code')
    return result


def install(g, main):
    g.expect('Hg1')
    g.expect('Z0,20228d86,2')
    try:
        g.packet('c')
        if [g.reg(i) for i in (0, 1, 2, 15)] != [0x20011010, 0x20011010, 0, 0x20228d86]:
            raise RuntimeError('unexpected stock AINR calling convention')
        if struct.unpack('<I', g.read(0x202a6f20, 4))[0] != 0x20280021:
            raise RuntimeError('stock AINR is not selected')
        if struct.unpack('<I', g.read(0x2028ba4c, 4))[0] != 1:
            raise RuntimeError('stock AINR processor is not enabled')
        if struct.unpack('<I', g.read(0x8046300c, 4))[0] != 1:
            raise RuntimeError('stock AINR learning did not complete')
        source = g.read(0x20280000, 0x20000)
        code = cloned_code(main)
        for index in range(3):
            state_addr = 0x80e00000 + index * 0x30000
            code_addr = 0x80f00000 + index * 0x10000
            copy = relocated(code, index)
            g.write(state_addr, source)
            g.write(code_addr, copy)
            if g.read(state_addr, len(source)) != source or g.read(code_addr, len(copy)) != copy:
                raise RuntimeError(f'copy {index} readback mismatch')
            print(f'Copy {index} code SHA-256: {sha(copy)}', flush=True)
        g.write(0x83e03000, bytes(28))
        g.write(0x83e00144, struct.pack('<I', 0x41494e52))
    finally:
        g.expect('z0,20228d86,2')
        g.expect('D')


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
    metadata = json.loads((out / 'pre-ainr.json').read_text())
    diagnostic_main = (out / 'firmware' / 'main_firmware.bin').read_bytes()
    nor = (out / 'state' / 'main-nor.bin').read_bytes()
    if (metadata.get('stock_main_sha256') != MAIN_SHA
            or metadata.get('diagnostic_main_sha256') != sha(diagnostic_main)
            or metadata.get('recipe_sha256') != sha((out / 'recipe.toml').read_bytes())
            or nor[0x50000:0x50000 + len(diagnostic_main)] != diagnostic_main):
        parser.error('prepared diagnostic, recipe or NOR does not match its manifest')
    deadline = time.monotonic() + 350
    marker = out / 'logs' / 'learned_ready.txt'
    while not marker.exists():
        if (out / 'logs' / 'diagnostic.json').exists():
            raise RuntimeError('diagnostic finished before learned snapshot')
        if time.monotonic() >= deadline:
            raise TimeoutError('learned snapshot not produced in 350 seconds')
        time.sleep(0.1)
    install(Remote(out / 'gdb.sock'), main_image)
    print('AINR copies installed in QEMU scratch; observer SHA-256:', sha(Path(__file__).read_bytes()), flush=True)


if __name__ == '__main__':
    main()
