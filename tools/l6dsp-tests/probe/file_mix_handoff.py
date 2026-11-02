#!/usr/bin/env python3
"""Check one forced file-mix handoff observation; not hardware qualification."""
import argparse
import json
from pathlib import Path

from submix_return import observations, words


def check_selector(items):
    attempts, acquired, failed, released, _, active = words(items, 'selector_handoff', '<6I')
    callbacks = [words(items, f'selector_callback_{index}', '<I')[0] for index in range(4)]
    if attempts < 6 or acquired != attempts or released != acquired or failed or active:
        raise ValueError('selection admission did not acquire and release on every transition')
    if 0x20280021 not in callbacks:
        raise ValueError('AINR selection was not observed')
    return acquired


def check_overlap(items):
    phase, signal, entered, closed, waits, drained = words(items, 'selector_overlap', '<6I')
    if (phase, signal, entered, closed, drained) != (2, 1, 2, 5, 4) or not waits:
        raise ValueError('selector did not wait for the forced live lease to drain')
    return waits


def check_timeout_audio(items):
    before = words(items, 'before', '<3I')
    after = words(items, 'after', '<3I')
    later = words(items, 'later', '<3I')
    pointer_before, = words(items, 'pointer_before', '<I')
    pointer_after, = words(items, 'pointer_after', '<I')
    if (before[:2] != (0x494e4954, 0x2028b8dc) or after[:2] != before[:2]
            or later[:2] != before[:2] or not before[2] or not before[2] < after[2] < later[2]
            or not pointer_before or pointer_after != pointer_before):
        raise ValueError('old callback did not remain selected and continue running')
    if (words(items, 'fade_before', '<I') != (0x3f800000,)
            or words(items, 'fade_after', '<I') != (0x3f800000,)
            or words(items, 'state_before', '<I') != (0,)
            or words(items, 'state_after', '<I') != (0,)):
        raise ValueError('observed fade gain or selector flag was not restored')


def check_timeout(report):
    items = observations(report)
    check_timeout_audio(items)
    attempts, acquired, failed, released, _, active = words(items, 'selector_handoff', '<6I')
    if (attempts, acquired, failed, released, active) != (3, 2, 1, 2, 0):
        raise ValueError('forced timeout did not release its guard')
    if words(items, 'selector_overlap', '<6I') != (2, 1, 2, 5, 20, 0):
        raise ValueError('live lease did not outlast the selector timeout')
    if (words(items, 'type_before', '<B') != (1,)
            or words(items, 'type_after', '<B') != (2,)
            or words(items, 'pointer_next', '<I') != (0x8002c041,)):
        raise ValueError('expected unresolved effect-selection mismatch was not observed')
    return {'fade_restored': True, 'old_callback_continues': True,
            'selection_mismatch': True, 'requested_effect_skipped': True}


def check(report, bypass, selector=False, overlap=False):
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
    outcome = {'before_stop': initial, 'after_signal': delayed, 'bypasses': bypasses,
               'after_restart': after}
    if selector:
        outcome['selector_transitions'] = check_selector(items)
    if overlap:
        outcome['selector_waits'] = check_overlap(items)
    return outcome


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--bypass', action='store_true')
    parser.add_argument('--selector', action='store_true')
    parser.add_argument('--overlap', action='store_true')
    parser.add_argument('--timeout', action='store_true', help='check a known-incomplete timeout recovery experiment')
    args = parser.parse_args()
    try:
        if args.timeout and (args.bypass or args.selector or args.overlap):
            raise ValueError('--timeout cannot be combined with other checks')
        if args.overlap and not args.selector:
            raise ValueError('--overlap requires --selector')
        report = json.loads(args.report.read_text())
        result = check_timeout(report) if args.timeout else check(report, args.bypass, args.selector, args.overlap)
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Forced QEMU interleaving only; verify fixture and executable identities separately.')


if __name__ == '__main__':
    main()
