"""Check that the immediate-rejection observer rejects misleading evidence."""
import copy
import struct
import unittest

from selector_reject_check import check


def fixture():
    fields = {
        'type_before': ('<B', (1,)), 'type_after': ('<B', (1,)), 'type_next': ('<B', (2,)),
        'before': ('<3I', (0x494e4954, 0x2028b8dc, 10)),
        'after': ('<3I', (0x494e4954, 0x2028b8dc, 20)),
        'later': ('<3I', (0x494e4954, 0x2028b8dc, 30)),
        'next': ('<3I', (0x494e4954, 0x2028b8f8, 1)),
        'pointer_before': ('<I', (0x8010bba1,)), 'pointer_after': ('<I', (0x8010bba1,)),
        'fade_before': ('<I', (0x3f800000,)), 'fade_after': ('<I', (0,)),
    }
    events = [{'observation': {'name': name, 'bytes': list(struct.pack(fmt, *values))}}
              for name, (fmt, values) in fields.items()]
    for suffix, rows in [('before', [1 << 18, 0]), ('after', [1 << 18, 0]),
                         ('next', [0, 1 << 19])]:
        events.append({'observation': {'name': 'selector_feedback_' + suffix, 'indicators': rows}})
    trace = ('SELECTOR_REJECT injected ready=47415445\n'
             'SELECTOR_REJECT restored status=1\n'
             'SELECTOR_REJECT injected=1 restored=1 active=0 errors=0\n')
    return {'outcome': 'complete', 'model': 'l6max', 'events': events}, trace


class SelectorRejectCheckTests(unittest.TestCase):
    def test_requires_failure_feedback_and_reports_unrestored_fade(self):
        report, trace = fixture()
        self.assertFalse(check(report, trace)['fade_restored'])
        for bad_trace in (trace.replace('status=1', 'status=0'), trace + trace, ''):
            with self.subTest(trace=bad_trace), self.assertRaises(ValueError):
                check(report, bad_trace)
        for name, value in [('type_after', 2), ('pointer_after', 0), ('fade_after', 1)]:
            bad = copy.deepcopy(report)
            item = next(e['observation'] for e in bad['events'] if e['observation']['name'] == name)
            item['bytes'][0] = value
            with self.subTest(name=name), self.assertRaises(ValueError):
                check(bad, trace)

    def test_held_closed_gate_requires_matching_trace_and_restored_fade(self):
        report, _ = fixture()
        fade = next(e['observation'] for e in report['events'] if e['observation']['name'] == 'fade_after')
        fade['bytes'] = list(struct.pack('<I', 0x3f800000))
        trace = ('SELECTOR_REJECT injected gate=00000000\n'
                 'SELECTOR_REJECT held status=1 target=750\n'
                 'SELECTOR_REJECT restored callbacks=750\n'
                 'SELECTOR_REJECT injected=1 restored=1 active=0 errors=0\n')
        self.assertTrue(check(report, trace, fade_restored=True, hold=750, closed=True)['fade_restored'])
        for kwargs in ({'fade_restored': False, 'hold': 750, 'closed': True},
                       {'fade_restored': True, 'hold': 749, 'closed': True},
                       {'fade_restored': True, 'hold': 750, 'closed': False}):
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                check(report, trace, **kwargs)

    def test_gate_held_until_exit_requires_stalled_callback_and_unchanged_lamps(self):
        report, _ = fixture()
        items = {e['observation']['name']: e['observation'] for e in report['events']}
        items['type_next']['bytes'] = [1]
        items['next']['bytes'] = list(struct.pack('<3I', 0x494e4954, 0x2028b8dc, 20))
        items['later']['bytes'] = list(struct.pack('<3I', 0x494e4954, 0x2028b8dc, 20))
        items['selector_feedback_next']['indicators'] = [1 << 18, 0]
        report['events'].append({'observation': {'name': 'pointer_next', 'bytes': list(struct.pack('<I', 0x8010bba1))}})
        trace = ('SELECTOR_REJECT injected gate=00000000\n'
                 'SELECTOR_REJECT held status=1 target=1000000\n'
                 'SELECTOR_REJECT injected=1 restored=0 active=2 errors=0\n')
        kwargs = {'closed': True, 'hold': 1000000, 'held_until_exit': True}
        result = check(report, trace, **kwargs)
        self.assertFalse(result['next_selection_observed'])
        self.assertFalse(result['fade_restored_at_first_snapshot'])
        for name, replacement in [('type_next', [2]), ('pointer_next', [0, 0, 0, 0]),
                                  ('fade_after', [1, 0, 0, 0])]:
            bad = copy.deepcopy(report)
            item = next(e['observation'] for e in bad['events'] if e['observation']['name'] == name)
            item['bytes'] = replacement
            with self.subTest(name=name), self.assertRaises(ValueError):
                check(bad, trace, **kwargs)
        bad = copy.deepcopy(report)
        next(e['observation'] for e in bad['events'] if e['observation']['name'] ==
             'selector_feedback_next')['indicators'] = []
        with self.assertRaises(ValueError):
            check(bad, trace, **kwargs)


if __name__ == '__main__':
    unittest.main()
