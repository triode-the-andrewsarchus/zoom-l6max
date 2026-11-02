#!/usr/bin/env python3
"""Check the synthetic file-mix layout observation; not hardware qualification."""
import argparse
import json
import math
from pathlib import Path

from submix_return import observations, words


def check(report):
    count, marker, *values = words(observations(report), 'offline_layout', '<II52f')
    if (count, marker) != (2, 0x4f46464c):
        raise ValueError('synthetic mix did not complete two callbacks')
    sums, tracks = values[:4], values[4:]
    for block in range(2):
        for edge in range(2):
            frame = block * 64 + edge * 63
            expected_sum = 0.0
            for slot in range(12):
                expected = (slot + 1) * 1_000_000 + frame * 1000
                if tracks[block * 24 + slot * 2 + edge] != expected:
                    raise ValueError(f'track {slot} frame {frame} differs')
                expected_sum += expected * (slot + 1) / 16
            if not math.isclose(sums[block * 2 + edge], expected_sum / 2**31,
                                rel_tol=1e-6, abs_tol=1e-9):
                raise ValueError(f'summed callback frame {frame} differs')
    return {'callbacks': count, 'summed_edges': sums}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path, help='l6dbg diagnostic.json')
    args = parser.parse_args()
    try:
        result = check(json.loads(args.report.read_text()))
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Synthetic QEMU observation only; verify fixture and executable identities separately.')


if __name__ == '__main__':
    main()
