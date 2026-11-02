#!/usr/bin/env python3
"""Check the synthetic file-mix layout observation; not hardware qualification."""
import argparse
import json
import math
from pathlib import Path

from submix_return import observations, words

# Captured from an independently executed host run of the same DSP settings.
EXPECTED_EFFECT = [0.007768246, 0.007729655, 0.013850859, 0.001429997,
                   0.013852504, 0.001430163, 0.013353780, -0.003259216]


def check_frame(block, edge, sums, tracks, routed):
    frame = block * 64 + edge * 63
    expected_sum = 0.0
    expected_buses = [0.0, 0.0, 0.0]
    for slot in range(12):
        expected = (slot + 1) * 1_000_000 + frame * 1000
        if tracks[block * 24 + slot * 2 + edge] != expected:
            raise ValueError(f'track {slot} frame {frame} differs')
        expected_sum += expected * (slot + 1) / 16
        expected_buses[slot % 3] += expected * (slot + 1) / 16 / 2**31
    if not math.isclose(sums[block * 2 + edge], expected_sum / 2**31,
                        rel_tol=1e-6, abs_tol=1e-9):
        raise ValueError(f'summed callback frame {frame} differs')
    for bus, expected in enumerate(expected_buses):
        if not math.isclose(routed[(block * 2 + edge) * 3 + bus], expected,
                            rel_tol=1e-6, abs_tol=1e-9):
            raise ValueError(f'routed bus {bus} frame {frame} differs')


def check(report):
    count, marker, *values = words(observations(report), 'offline_layout', '<II52f2I20f3I')
    if (count, marker) != (2, 0x4f46464c):
        raise ValueError('synthetic mix did not complete two callbacks')
    sums, tracks = values[:4], values[4:52]
    offsets, routed, output, ordinary = values[52:54], values[54:66], values[66:74], values[74:]
    if offsets != [0, 64]:
        raise ValueError('file-mix block offsets differ')
    if ordinary != [2, 0x202a7138, 0x202a7138]:
        raise ValueError('ordinary callback third arguments differ')
    for block in range(2):
        for edge in range(2):
            check_frame(block, edge, sums, tracks, routed)
    if any(not math.isclose(value, expected, rel_tol=1e-5, abs_tol=1e-6)
           for value, expected in zip(output, EXPECTED_EFFECT, strict=True)):
        raise ValueError('routed effect output differs from host reference')
    return {'callbacks': count, 'offsets': offsets, 'ordinary_arguments': ordinary[1:],
            'summed_edges': sums, 'routed_edges': routed, 'effect_edges': output}


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
