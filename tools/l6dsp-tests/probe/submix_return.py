#!/usr/bin/env python3
"""Compare two QEMU-only l6dbg Sub-Mix observations; not hardware qualification."""
import argparse
import json
import math
from pathlib import Path
import struct


def observations(report):
    if report.get('outcome') != 'complete' or report.get('model') != 'l6max':
        raise ValueError('diagnostic did not complete for L6max')
    return {event['observation'].get('name'): event['observation']
            for event in report['events'] if event['observation'].get('name')}


def words(items, name, fmt):
    try:
        return struct.unpack(fmt, bytes(items[name]['bytes']))
    except (KeyError, TypeError, ValueError, struct.error) as error:
        raise ValueError(f'invalid {name} observation') from error


def check_run(report, enabled):
    items = observations(report)
    if words(items, 'return_flags', '<2B') != (int(enabled), 0):
        raise ValueError('unexpected Sub-Mix or Mix Minus setting')
    counts = []
    for suffix in ('initial', 'final'):
        marker, descriptor, count = words(items, f'effect_{suffix}', '<III')
        audio_marker, _, left, right = words(items, f'audio_{suffix}', '<IIff')
        if (marker, descriptor, audio_marker) != (0x494e4954, 0x2028b8dc, 0x5349474e):
            raise ValueError('diagnostic Room callback is not active')
        if not all(math.isfinite(value) and value != 0.0 for value in (left, right)):
            raise ValueError('sampled DSP output is silent or nonfinite')
        counts.append(count)
        tx = words(items, f'tx_{suffix}', '<1024i')
        if not any(tx[4::8]) or not any(tx[5::8]):
            raise ValueError('sampled main return slots are silent')
        for index in range(0, len(tx), 8):
            expected = (tx[index + 4], tx[index + 5]) if enabled else (0, 0)
            if (tx[index + 6], tx[index + 7]) != expected:
                raise ValueError('Sub-Mix slot relationship differs from the observed model')
    if not 0 < (counts[1] - counts[0]) % (1 << 32) < (1 << 31):
        raise ValueError('callback did not advance')
    return counts


def compare(off, on):
    digest = off.get('main_sha256')
    if (not isinstance(digest, str) or len(digest) != 64
            or any(c not in '0123456789abcdef' for c in digest)
            or digest != on.get('main_sha256')):
        raise ValueError('diagnostic MAIN identities differ or are absent')
    return {'off_counts': check_run(off, False), 'on_counts': check_run(on, True)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('off', type=Path, help='diagnostic.json with the return disabled')
    parser.add_argument('on', type=Path, help='diagnostic.json with the return enabled')
    args = parser.parse_args()
    try:
        result = compare(json.loads(args.off.read_text()), json.loads(args.on.read_text()))
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'comparison failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Observed slot relationship only; verify fixture and QEMU identities separately.')


if __name__ == '__main__':
    main()
