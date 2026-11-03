#!/usr/bin/env python3
"""Check a forced early selector rejection; this does not qualify hardware."""
import argparse
import json
from pathlib import Path

from file_mix_handoff import check_feedback_lamps
from submix_return import observations, words


def check(report, trace):
    expected = (
        'SELECTOR_REJECT injected ready=47415445',
        'SELECTOR_REJECT restored status=1',
        'SELECTOR_REJECT injected=1 restored=1 active=0 errors=0',
    )
    if tuple(line for line in trace.splitlines() if line.startswith('SELECTOR_REJECT ')) != expected:
        raise ValueError('fault injection or feedback status did not match')
    items = observations(report)
    if tuple(words(items, f'type_{suffix}', '<B')[0] for suffix in ('before', 'after', 'next')) != (1, 1, 2):
        raise ValueError('effect type did not roll back and advance on the next press')
    before = words(items, 'before', '<3I')
    after = words(items, 'after', '<3I')
    later = words(items, 'later', '<3I')
    pointer, = words(items, 'pointer_before', '<I')
    if (before[:2] != (0x494e4954, 0x2028b8dc) or after[:2] != before[:2]
            or later[:2] != before[:2] or not 0 < before[2] < after[2] < later[2]
            or not pointer or words(items, 'pointer_after', '<I') != (pointer,)):
        raise ValueError('old callback was not retained and advanced')
    if words(items, 'fade_before', '<I') != (0x3f800000,) or words(items, 'fade_after', '<I') != (0,):
        raise ValueError('the expected unresolved fade behavior was not observed')
    if words(items, 'next', '<3I')[:2] != (0x494e4954, 0x2028b8f8):
        raise ValueError('next selection did not reach Spring')
    check_feedback_lamps(items)
    return {'panel_rollback_observed': True, 'next_selection_observed': True,
            'fade_restored': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('trace', type=Path)
    args = parser.parse_args()
    try:
        result = check(json.loads(args.report.read_text()), args.trace.read_text())
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('QEMU-only observation; fade recovery remains unresolved. Verify binary and fixture identities.')


if __name__ == '__main__':
    main()
