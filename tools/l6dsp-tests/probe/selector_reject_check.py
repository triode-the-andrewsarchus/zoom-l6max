#!/usr/bin/env python3
"""Check a forced early selector rejection; this does not qualify hardware."""
import argparse
import json
from pathlib import Path

from file_mix_handoff import check_feedback_lamps
from submix_return import observations, words


def check_injection(trace, hold, closed, held_until_exit):
    expected = (
        'SELECTOR_REJECT injected gate=00000000' if closed else 'SELECTOR_REJECT injected ready=47415445',
        f'SELECTOR_REJECT held status=1 target={hold}' if hold else 'SELECTOR_REJECT restored status=1',
    ) + ((f'SELECTOR_REJECT restored callbacks={hold}',) if hold and not held_until_exit else ()) + (
        'SELECTOR_REJECT injected=1 restored=0 active=2 errors=0' if held_until_exit
        else 'SELECTOR_REJECT injected=1 restored=1 active=0 errors=0',
    )
    if tuple(line for line in trace.splitlines() if line.startswith('SELECTOR_REJECT ')) != expected:
        raise ValueError('fault injection or feedback status did not match')


def check_held(items):
    types = tuple(words(items, f'type_{suffix}', '<B')[0] for suffix in ('before', 'after', 'next'))
    if types != (1, 1, 1):
        raise ValueError('the selected effect changed while the injected gate stayed closed')
    before, after, later, next_ = (words(items, name, '<3I') for name in ('before', 'after', 'later', 'next'))
    if (before[:2] != (0x494e4954, 0x2028b8dc) or any(value[:2] != before[:2] for value in (after, later, next_))
            or not 0 < before[2] <= after[2] or after[2] != later[2] or later[2] != next_[2]):
        raise ValueError('the expected stopped callback was not observed')
    pointer, = words(items, 'pointer_before', '<I')
    if not pointer or any(words(items, name, '<I') != (pointer,) for name in ('pointer_after', 'pointer_next')):
        raise ValueError('the old callback pointer was not retained')
    if words(items, 'fade_before', '<I') != (0x3f800000,) or words(items, 'fade_after', '<I') != (0,):
        raise ValueError('the expected zero fade was not observed')
    for suffix in ('before', 'after', 'next'):
        try:
            rows = items[f'selector_feedback_{suffix}']['indicators']
            valid = rows[0] & (1 << 18) and not rows[1] & (1 << 19)
        except (KeyError, IndexError, TypeError) as error:
            raise ValueError(f'missing or invalid {suffix} effect lamps') from error
        if not valid:
            raise ValueError(f'unexpected {suffix} effect lamps')
    return {'gate_held_until_exit': True, 'callback_advanced_after_snapshot': False,
            'next_selection_observed': False, 'fade_restored': False}


def check(report, trace, *, fade_restored=False, hold=0, closed=False, held_until_exit=False):
    if held_until_exit and (not hold or fade_restored):
        raise ValueError('a held-until-exit observation needs a hold and cannot expect a restored fade')
    check_injection(trace, hold, closed, held_until_exit)
    items = observations(report)
    if held_until_exit:
        return check_held(items)
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
    expected_fade = 0x3f800000 if fade_restored else 0
    if words(items, 'fade_before', '<I') != (0x3f800000,) or words(items, 'fade_after', '<I') != (expected_fade,):
        raise ValueError('the expected sampled fade behavior was not observed')
    if words(items, 'next', '<3I')[:2] != (0x494e4954, 0x2028b8f8):
        raise ValueError('next selection did not reach Spring')
    check_feedback_lamps(items)
    return {'panel_rollback_observed': True, 'next_selection_observed': True,
            'fade_restored': fade_restored}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('trace', type=Path)
    parser.add_argument('--fade-restored', action='store_true', help='expect the experimental fade replay')
    parser.add_argument('--hold', type=int, default=0, help='expected forced wrapper-entry hold count')
    parser.add_argument('--closed', action='store_true', help='expect a forced closed gate rather than invalid ready word')
    parser.add_argument('--held-until-exit', action='store_true', help='expect the injected condition to remain active until the run ends')
    args = parser.parse_args()
    try:
        if not 0 <= args.hold <= 1000000:
            raise ValueError('--hold must be between 0 and 1000000')
        result = check(json.loads(args.report.read_text()), args.trace.read_text(),
                       fade_restored=args.fade_restored, hold=args.hold, closed=args.closed,
                       held_until_exit=args.held_until_exit)
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('QEMU-only observation; verify binary and fixture identities. Other failure cases remain unqualified.')


if __name__ == '__main__':
    main()
