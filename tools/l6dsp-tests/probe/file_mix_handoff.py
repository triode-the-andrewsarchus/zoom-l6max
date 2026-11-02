#!/usr/bin/env python3
"""Check one forced file-mix handoff observation; not hardware qualification."""
import argparse
import json
import re
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


def check_feedback_lamps(items):
    # L6max GUI LED indices 35 and 36: Room=(row 0, bit 18),
    # Spring=(row 1, bit 19), respectively.
    for suffix, expected in (('before', (True, False)), ('after', (True, False)),
                             ('next', (False, True))):
        try:
            rows = items[f'selector_feedback_{suffix}']['indicators']
            actual = (bool(rows[0] & (1 << 18)), bool(rows[1] & (1 << 19)))
        except (KeyError, TypeError, IndexError) as error:
            raise ValueError(f'invalid {suffix} panel snapshot') from error
        if actual != expected:
            raise ValueError(f'unexpected {suffix} effect lamps: {actual}')


def check_timeout(report, feedback=False):
    items = observations(report)
    check_timeout_audio(items)
    attempts, acquired, failed, released, _, active = words(items, 'selector_handoff', '<6I')
    if (attempts, acquired, failed, released, active) != (3, 2, 1, 2, 0):
        raise ValueError('forced timeout did not release its guard')
    if words(items, 'selector_overlap', '<6I') != (2, 1, 2, 5, 20, 0):
        raise ValueError('live lease did not outlast the selector timeout')
    if words(items, 'type_before', '<B') != (1,):
        raise ValueError('unexpected starting effect')
    if feedback:
        if (words(items, 'type_after', '<B') != (1,)
                or words(items, 'type_next', '<B') != (2,)
                or words(items, 'next', '<3I')[:2] != (0x494e4954, 0x2028b8f8)):
            raise ValueError('panel rollback or next requested effect was not observed')
        check_feedback_lamps(items)
    elif (words(items, 'type_after', '<B') != (2,)
            or words(items, 'pointer_next', '<I') != (0x8002c041,)):
        raise ValueError('expected unresolved effect-selection mismatch was not observed')
    return {'fade_restored': True, 'old_callback_continues': True,
            'selection_mismatch': not feedback, 'requested_effect_skipped': not feedback}


def check_midi_trace(trace, values=('2b', '2b', '40')):
    payload = re.findall(r'l6 main LPUART3 \(MIDI jacks\) TX ([0-9a-f]{2})', trace)
    expected = [byte for value in values for byte in ('b0', '75', value)]
    if payload != expected:
        raise ValueError(f'unexpected modeled MIDI output: {" ".join(payload)}')
    return len(payload) // 3


def check_ainr_state(items, suffix, effect, callback, flag):
    if (words(items, f'type_{suffix}', '<B') != (effect,)
            or words(items, f'callback_{suffix}', '<I') != (callback,)
            or words(items, f'ainr_left_{suffix}', '<I') != (flag,)
            or words(items, f'ainr_right_{suffix}', '<I') != (flag,)):
        raise ValueError(f'AINR {suffix} state differs from expectation')


def check_learned_marker(items):
    if words(items, 'learned_before', '<I') != (1,) or words(items, 'learned_after', '<I') != (1,):
        raise ValueError('AINR learned marker was not established and retained')


def check_ainr_admission(items, transition):
    left, right, rejected_left, rejected_right, teardown_left, teardown_right, unsupported = words(items, 'ainr_admission', '<7I')
    required = (left, right, rejected_left, rejected_right, teardown_left, teardown_right) if transition == 'exit' else (left, right)
    if not all(required) or unsupported:
        raise ValueError('AINR admission observations did not meet the transition checks')
    return {'admitted': [left, right], 'rejected_during_teardown': [teardown_left, teardown_right]}


def check_ainr_timeout(report, transition):
    items = observations(report)
    before_type, before_callback, before_flag, next_type, next_callback = (
        (4, 0x20280781, 0, 5, 0x20280021) if transition == 'entry'
        else (5, 0x20280021, 1, 0, 0x20281649))
    before_gain = words(items, 'gain_before', '<I')
    for suffix in ('before', 'after'):
        check_ainr_state(items, suffix, before_type, before_callback, before_flag)
        if words(items, f'gain_{suffix}', '<I') != before_gain:
            raise ValueError(f'{transition} timeout did not restore {suffix} gain')
    check_ainr_state(items, 'next', next_type, next_callback, 1 - before_flag)
    if words(items, 'selector_overlap', '<6I') != (2, 1, 2, 5, 20, 0):
        raise ValueError('live lease did not outlast the selector timeout')
    attempts, acquired, failed, released, _, active = words(items, 'selector_handoff', '<6I')
    if attempts != acquired + 1 or failed != 1 or released != acquired or active:
        raise ValueError('forced AINR timeout did not release its guard')
    return {'transition': transition, 'sampled_state_preserved': True, 'retry_selected': True}


def check(report, bypass, selector=False, overlap=False, injected=True):
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
    if ((delayed - initial) % (1 << 32) != int(injected and not bypass)
            or bypasses != int(injected and bypass)):
        raise ValueError('late callback or bypass count differs')
    outcome = {'before_stop': initial, 'after_signal': delayed, 'bypasses': bypasses,
               'after_restart': after}
    if selector:
        outcome['selector_transitions'] = check_selector(items)
    if overlap:
        outcome['selector_waits'] = check_overlap(items)
    return outcome


def validate_options(args):
    if args.timeout and (args.bypass or args.selector or args.overlap):
        raise ValueError('--timeout cannot be combined with other checks')
    if args.overlap and not args.selector:
        raise ValueError('--overlap requires --selector')
    if args.feedback and (not args.timeout or not args.midi_trace):
        raise ValueError('--feedback requires --timeout and --midi-trace')


def validate_ainr_options(args):
    if args.midi_trace and not (args.feedback or args.ainr_timeout):
        raise ValueError('--midi-trace requires --feedback or --ainr-timeout')
    if args.ainr_timeout and any((args.timeout, args.feedback, args.bypass,
                                  args.selector, args.overlap)):
        raise ValueError('--ainr-timeout cannot be combined with other checks')
    if args.ainr_learned and args.ainr_timeout != 'exit':
        raise ValueError('--ainr-learned requires --ainr-timeout exit')
    if args.ainr_admission and not args.ainr_timeout:
        raise ValueError('--ainr-admission requires --ainr-timeout')


def run_ainr_check(args, report):
    result = check_ainr_timeout(report, args.ainr_timeout)
    if args.ainr_learned:
        check_learned_marker(observations(report))
        result['learned_marker_retained'] = True
    if args.ainr_admission:
        result['ainr_admission'] = check_ainr_admission(observations(report), args.ainr_timeout)
    if args.midi_trace:
        values = ('2b', '40', '55', '6b', '6b', '00') if args.ainr_timeout == 'entry' else ('2b', '40', '55', '6b', '00', '00', '15')
        result['midi_packets'] = check_midi_trace(args.midi_trace.read_text(), values)
    return result


def run_check(args):
    validate_options(args)
    validate_ainr_options(args)
    report = json.loads(args.report.read_text())
    if args.ainr_timeout:
        return run_ainr_check(args, report)
    result = check_timeout(report, args.feedback) if args.timeout else check(report, args.bypass, args.selector, args.overlap)
    if args.feedback:
        result['midi_packets'] = check_midi_trace(args.midi_trace.read_text())
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--bypass', action='store_true')
    parser.add_argument('--selector', action='store_true')
    parser.add_argument('--overlap', action='store_true')
    parser.add_argument('--timeout', action='store_true', help='check a known-incomplete timeout recovery experiment')
    parser.add_argument('--feedback', action='store_true', help='check experimental panel rollback')
    parser.add_argument('--midi-trace', type=Path, help='check the modeled DIN output trace')
    parser.add_argument('--ainr-timeout', choices=('entry', 'exit'))
    parser.add_argument('--ainr-learned', action='store_true')
    parser.add_argument('--ainr-admission', action='store_true')
    args = parser.parse_args()
    try:
        result = run_check(args)
    except (OSError, json.JSONDecodeError, KeyError, ValueError) as error:
        parser.exit(1, f'check failed: {error}\n')
    print(json.dumps(result, indent=2))
    print('Forced QEMU interleaving only; verify fixture and executable identities separately.')


if __name__ == '__main__':
    main()
