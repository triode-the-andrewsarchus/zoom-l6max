#!/usr/bin/env python3
"""Validate the disposable late-DMA-IRQ experiment, not hardware safety."""
import argparse
import json
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'probe'))
from file_mix_handoff import check
from submix_return import observations, words


EXPECTED = ('80024b70', '80026658', '80043af8', '8001f360', '800149b0')


def validate(report, trace, guarded):
    result = check(report, guarded)
    before, destination, csr, after = words(observations(report), 'mix_irq', '<4I')
    if (before | after) & (1 << 15) or csr or not 0x2000a740 <= destination <= 0x2000c740:
        raise ValueError('stopped DMA state did not match the observed experiment')
    lines = trace.splitlines()
    if lines.count('IRQ_PROFILE_BEGIN') != 1 or lines.count('IRQ_PROFILE_END') != 1:
        raise ValueError('missing or repeated IRQ observation window')
    begin, end = lines.index('IRQ_PROFILE_BEGIN'), lines.index('IRQ_PROFILE_END')
    if begin >= end:
        raise ValueError('IRQ observation window is out of order')
    matches = [re.fullmatch(r'IRQ_PC ([0-9a-f]{8}) ([0-9]+)', line)
               for line in lines[begin + 1:end]]
    if any(match is None for match in matches):
        raise ValueError('unexpected IRQ trace entry')
    counts = [(match.group(1), int(match.group(2))) for match in matches]
    if counts != [(address, 1) for address in EXPECTED]:
        raise ValueError('expected IRQ-to-audio instruction entries were not observed once')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('trace', type=Path)
    parser.add_argument('--guarded', action='store_true')
    args = parser.parse_args()
    try:
        result = validate(json.loads(args.report.read_text()), args.trace.read_text(), args.guarded)
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Forced QEMU-only IRQ; this does not qualify real hardware or memory ownership.')


if __name__ == '__main__':
    main()
