#!/usr/bin/env python3
"""Force caller return addresses through the clear veneer in disposable QEMU."""
import argparse
from pathlib import Path
import subprocess

from ainr_isolation import Remote


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('socket', type=Path, help='QEMU GDB socket')
    parser.add_argument('elf', type=Path, help='effect probe ELF for this image')
    args = parser.parse_args()
    symbols = {}
    for line in subprocess.check_output(['llvm-nm', '-n', str(args.elf)], text=True).splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] in ('probe', 'probe_selection'):
            if parts[2] in symbols:
                raise ValueError(f'duplicate symbol {parts[2]}')
            symbols[parts[2]] = int(parts[0], 16) & ~1
    if set(symbols) != {'probe', 'probe_selection'}:
        raise ValueError('probe ELF must contain both dispatch symbols')

    remote = Remote(args.socket)
    remote.expect('Hg1')
    remote.packet('?')
    saved = remote.packet('g')
    targets = (0x80024678, symbols['probe_selection'])
    installed = []
    try:
        for address in targets:
            remote.expect(f'Z0,{address:x},2')
            installed.append(address)
        for caller, expected in ((0x20280677, targets[1]), (0x202806c5, targets[1]),
                                 (0x20280755, targets[0]), (0x12345679, targets[0])):
            remote.setreg(14, caller)
            remote.setreg(15, symbols['probe'])
            remote.packet('c')
            actual = remote.reg(15)
            if actual != expected:
                raise ValueError(f'return {caller:#x} reached {actual:#x}, expected {expected:#x}')
            print(f'{caller:#010x} -> {actual:#010x}')
    finally:
        for address in reversed(installed):
            remote.expect(f'z0,{address:x},2')
        remote.expect('G' + saved)
        remote.expect('D')
    print('Dispatch only; no destination was executed. QEMU does not establish target safety.')


if __name__ == '__main__':
    main()
