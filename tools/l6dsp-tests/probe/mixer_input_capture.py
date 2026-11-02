#!/usr/bin/env python3
"""Validate a QEMU-only mixer input capture; not hardware qualification."""
import argparse
import json
import math
from pathlib import Path
import struct
import subprocess


def floats(data):
    return struct.unpack('<' + 'f' * (len(data) // 4), data)


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def capture(events, label, field, size):
    try:
        name = f'{label}_{field}' if field else label
        data = bytes(events[name]['bytes'])
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


def words(events, name, count):
    return struct.unpack('<' + 'I' * count, capture(events, name, None, 4 * count))


def callback_from_elf(elf):
    output = subprocess.check_output(['llvm-nm', '-n', str(elf)], text=True, stderr=subprocess.PIPE)
    symbols = [line.split() for line in output.splitlines() if line.split()[-1:] == ['diagnostic_audio']]
    if len(symbols) != 1 or len(symbols[0]) != 3 or symbols[0][1] not in ('T', 't'):
        raise ValueError('ELF must contain exactly one diagnostic_audio function')
    address = int(symbols[0][0], 16)
    if not 0x80108000 <= (address & ~1) < 0x801a2ff8:
        raise ValueError('diagnostic_audio is outside the L6max application slot')
    return address | 1


def check_ui_routes(report, callback):
    if report.get('outcome') != 'complete' or report.get('model') != 'l6max':
        raise ValueError('diagnostic did not complete for L6max')
    events = {e['observation'].get('name'): e['observation'] for e in report['events']}
    if words(events, 'active_room', 1)[0] != callback:
        raise ValueError('Room callback not active')
    if words(events, 'room_open', 8)[7] != 1:
        raise ValueError('Room inspector did not open')
    expected = {
        'route_before': [2] * 12,
        'route_1': [3] + [2] * 11,
        'route_5': [3, 2, 2, 2, 3, 3] + [2] * 6,
        'route_reverse': [2, 2, 2, 2, 3, 3] + [2] * 6,
    }
    for name, route in expected.items():
        if list(words(events, name, 12)) != route:
            raise ValueError(f'{name}: unexpected route state')
    head, tail, dropped = words(events, 'queue_room', 3)
    marker, descriptor, count = words(events, 'callback_state', 3)
    if head != tail or dropped or marker != 0x494e4954 or descriptor != 0x2028b8dc or count < 100:
        raise ValueError('queue or callback state mismatch')
    return {'callback_count': count, 'queue_events': head, 'send_peaks': check_ui_sends(events)}


def check_ui_sends(events):
    peaks = {}
    for name, bus in (('route_before', 1), ('route_1', 2), ('route_reverse', 1)):
        clean = []
        for index in range(6):
            samples = floats(capture(events, name, f'sends_{index}', 768))
            if not all(map(math.isfinite, samples)):
                raise ValueError(f'{name}: nonfinite send')
            peak = max(map(abs, samples[bus * 64:(bus + 1) * 64]))
            if peak > 1e-7 and all(not any(samples[b * 64:(b + 1) * 64]) for b in range(3) if b != bus):
                clean.append(peak)
        if not clean:
            raise ValueError(f'{name}: no clean, nonzero expected send')
        peaks[name] = max(clean)
    return peaks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path, help='l6dbg diagnostic JSON')
    parser.add_argument('--ui-route', action='store_true', help='check the combined UI routing probe')
    parser.add_argument('--probe-elf', type=Path, help='effect ELF from this combined UI diagnostic')
    args = parser.parse_args()
    if args.ui_route != (args.probe_elf is not None):
        parser.error('--ui-route and --probe-elf must be supplied together')
    try:
        report = json.loads(args.report.read_text())
        result = check_ui_routes(report, callback_from_elf(args.probe_elf)) if args.ui_route else check(report)
    except (OSError, ValueError, KeyError, TypeError, struct.error, OverflowError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'capture failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Modeled capture only; verify executable and fixture identities separately.')


if __name__ == '__main__':
    main()
