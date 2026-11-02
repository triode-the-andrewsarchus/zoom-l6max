#!/usr/bin/env python3
"""Validate a QEMU-only mixer input capture; not hardware qualification."""
import argparse
import json
import math
from pathlib import Path
import struct


def floats(data):
    return struct.unpack('<' + 'f' * (len(data) // 4), data)


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def capture(events, label, field, size):
    try:
        data = bytes(events[f'{label}_{field}']['bytes'])
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f'missing {label} {field}') from error
    if len(data) != size:
        raise ValueError(f'invalid {label} {field} length')
    return data


def reconstruct(source, gains, label):
    expected = [0.0] * 64
    for slot in range(12):
        smoothed = struct.unpack_from('<f', gains, 8 + 4 * slot)[0]
        gain = struct.unpack_from('<f', gains, 0x38 + 8 * slot)[0]
        ramp = struct.unpack_from('<I', gains, 0x98 + 16 * slot)[0]
        if ramp or not math.isfinite(smoothed) or not math.isfinite(gain):
            raise ValueError(f'{label} has active ramp or nonfinite gain')
        if gain == 0.0:
            continue
        for i in range(64):
            sample = f32(source[slot * 64 + i] * (1.0 / 2147483648.0))
            sample = f32(f32(sample * gain) * smoothed)
            if not -1_000_000.0 <= sample <= 1_000_000.0:
                sample = 0.0
            expected[i] = f32(expected[i] + sample)
    return expected


def check_effect(events, label, descriptor, route):
    marker, selected, count = struct.unpack('<III', capture(events, label, 'state', 12))
    signal, captured_count = struct.unpack('<II', capture(events, label, 'marker', 8))
    if (marker != 0x494e4954 or selected != descriptor or signal != 0x5349474e
            or captured_count < 100 or captured_count > count):
        raise ValueError(f'{label} callback or capture marker mismatch')
    source = floats(capture(events, label, 'source', 3072))
    sends = floats(capture(events, label, 'sends', 768))
    output = floats(capture(events, label, 'output', 512))
    gains = capture(events, label, 'gains', 512)
    if not all(math.isfinite(x) for x in (*source, *sends, *output)):
        raise ValueError(f'{label} contains nonfinite samples')
    if any(abs(x) > 1.0 for x in output):
        raise ValueError(f'{label} return exceeds normalized bounds')
    if not any(sends[route * 64:(route + 1) * 64]) or not any(output[:64]) or not any(output[64:]):
        raise ValueError(f'{label} send or return is silent')
    if any(x for bus in range(3) if bus != route for x in sends[bus * 64:(bus + 1) * 64]):
        raise ValueError(f'{label} unexpected send route')
    expected = reconstruct(source, gains, label)
    if tuple(expected) != sends[route * 64:(route + 1) * 64]:
        raise ValueError(f'{label} reconstructed send differs')
    return {'callback_count': count, 'captured_count': captured_count,
            'send_peak': max(map(abs, expected)), 'output_peak': max(map(abs, output))}


def check(report):
    if report.get('outcome') != 'complete' or report.get('model') != 'l6max':
        raise ValueError('diagnostic did not complete for L6max')
    events = {e['observation'].get('name'): e['observation'] for e in report['events']}
    return {label: check_effect(events, label, descriptor, route)
            for label, descriptor, route in (('plateau', 0x2028b8dc, 1), ('velvet', 0x2028b8f8, 0))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path, help='l6dbg diagnostic JSON')
    args = parser.parse_args()
    try:
        result = check(json.loads(args.report.read_text()))
    except (OSError, ValueError, KeyError, TypeError, struct.error, OverflowError) as error:
        parser.exit(1, f'capture failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Modeled capture only; verify executable and fixture identities separately.')


if __name__ == '__main__':
    main()
