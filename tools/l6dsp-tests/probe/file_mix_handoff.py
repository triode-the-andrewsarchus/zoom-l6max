#!/usr/bin/env python3
"""Check one forced file-mix handoff observation; not hardware qualification."""
import argparse
import json
from pathlib import Path

from submix_return import observations, words


def check(report, bypass):
    items = observations(report)
    marker, first, before = words(items, 'callback_state', '<III')
    after_marker, second, after = words(items, 'callback_after', '<III')
    (mix_marker, initial, tcr3, rcr3, tcsr, rcsr, delayed,
     result, done, signaled, bypasses) = words(items, 'mix_handoff', '<11I')
    if (marker, first, after_marker, second) != (0x494e4954, 0x2028b8dc, 0x494e4954, 0x2028b8f8):
        raise ValueError('expected diagnostic callbacks were not selected')
    if not before or not initial or after <= delayed:
        raise ValueError('callback did not run before and after the handoff')
    if tcr3 & 0xf0000 or rcr3 & 0xf0000 or tcsr & 0x80000000 or rcsr & 0x80000000:
        raise ValueError('SAI stream was not stopped at the observed call')
    if (mix_marker, done, result, signaled) != (0x4d495831, 0x444f4e45, 9, 1):
        raise ValueError('forced no-card workflow did not finish as expected')
    if (delayed - initial) % (1 << 32) != (0 if bypass else 1) or bypasses != int(bypass):
        raise ValueError('late callback or bypass count differs')
    return {'before_stop': initial, 'after_signal': delayed, 'bypasses': bypasses,
            'after_restart': after}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--bypass', action='store_true')
    args = parser.parse_args()
    try:
        result = check(json.loads(args.report.read_text()), args.bypass)
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Forced QEMU interleaving only; verify fixture and executable identities separately.')


if __name__ == '__main__':
    main()
