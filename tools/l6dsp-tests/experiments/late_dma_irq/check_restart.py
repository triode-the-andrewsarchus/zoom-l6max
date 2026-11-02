#!/usr/bin/env python3
"""Summarize paired, forced-QEMU DMA captures; not an audio qualification."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

from check import validate
from submix_return import observations


HEADER = struct.pack('<4sIII', b'L6DM', 48000, 0x384000, 1)
BASE = 0x20009740
BUFFER_SIZE = 4096
RECORD_SIZE = 40  # address, length, and eight signed 32-bit slots


def records(stream, offset, count):
    stream.seek(offset)
    result = []
    for _ in range(count):
        record = stream.read(RECORD_SIZE)
        if len(record) != RECORD_SIZE:
            raise ValueError('truncated DMA record')
        address, size = struct.unpack_from('<II', record)
        if (size != 32 or not BASE <= address <= BASE + BUFFER_SIZE - size
                or (address - BASE) % 32):
            raise ValueError('unexpected DMA record address or size')
        result.append((address, record[8:]))
    return result


def read_markers(path):
    pairs = [line.split() for line in path.read_text().splitlines()]
    if (len(pairs) != 2 or any(len(pair) != 2 for pair in pairs)
            or [pair[0] for pair in pairs] != ['inject', 'restart']):
        raise ValueError('expected one injection and one restart marker')
    injection, restart = (int(pair[1]) for pair in pairs)
    if injection != restart or injection < 16 + 5000 * RECORD_SIZE or (restart - 16) % RECORD_SIZE:
        raise ValueError('unexpected DMA data between markers or unaligned marker')
    return injection, restart


def summarize(snapshot, before, after, marker_gap):
    if not any(any(data) for _, data in before):
        raise ValueError('no nonzero output in the pre-injection sample')
    first_nonzero = next((i for i, (_, data) in enumerate(after) if any(data)), None)
    if first_nonzero is None:
        raise ValueError('no nonzero output in the post-restart sample')
    initial_matching = 0
    for address, data in after:
        if data != snapshot[address - BASE:address - BASE + len(data)]:
            break
        initial_matching += 1
    return {'marker_gap_bytes': marker_gap,
            'snapshot_sha256': hashlib.sha256(snapshot).hexdigest(),
            'snapshot_nonzero_bytes': sum(byte != 0 for byte in snapshot),
            'first_nonzero_record': first_nonzero,
            'initial_snapshot_matches': initial_matching}


def capture(root, control, count):
    report = json.loads((root / 'result.json').read_text())
    trace = (root / 'logs/engine.stderr').read_text()
    validate(report, trace, not control, control=control, outer=not control)
    snapshot = bytes(observations(report)['tx_before_inject']['bytes'])
    if len(snapshot) != BUFFER_SIZE:
        raise ValueError('missing pre-injection TX buffer snapshot')
    injection, restart = read_markers(root / 'markers.txt')
    with (root / 'dma.bin').open('rb') as stream:
        if stream.read(16) != HEADER:
            raise ValueError('unsupported DMA capture header')
        before = records(stream, injection - 5000 * RECORD_SIZE, 5000)
        after = records(stream, restart, count)
    return summarize(snapshot, before, after, restart - injection), after


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('forced', type=Path)
    parser.add_argument('control', type=Path)
    parser.add_argument('--records', type=int, default=12000)
    args = parser.parse_args()
    if not 1 <= args.records <= 480000:
        parser.error('--records must be between 1 and 480000')
    try:
        forced, a = capture(args.forced, False, args.records)
        control, b = capture(args.control, True, args.records)
        address_mismatches = sum(x[0] != y[0] for x, y in zip(a, b))
        different_records = sum(x[1] != y[1] for x, y in zip(a, b))
        max_sample_delta = max(abs(x - y) for left, right in zip(a, b)
                               for x, y in zip(struct.unpack('<8i', left[1]),
                                               struct.unpack('<8i', right[1])))
    except (OSError, ValueError, KeyError, struct.error, json.JSONDecodeError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps({'forced': forced, 'control': control, 'records_compared': args.records,
                      'address_mismatches': address_mismatches,
                      'different_records': different_records, 'max_sample_delta': max_sample_delta}, indent=2))
    print('QEMU-only source capture; differences are reported, not judged as an audio pass.')


if __name__ == '__main__':
    main()
