"""Validate both the forced late callback and its experimental bypass."""
import copy
import struct
import unittest

from file_mix_handoff import check, check_ainr_admission, check_ainr_timeout, check_learned_marker, check_midi_trace, check_project, check_timeout


def capture_report(captures):
    return {'outcome': 'complete', 'model': 'l6max',
            'events': [{'observation': {'name': name, 'bytes': list(struct.pack(fmt, *data))}}
                       for name, (fmt, data) in captures.items()]}


def report(bypass):
    captures = {
        'callback_state': (0x494e4954, 0x2028b8dc, 1600),
        'callback_after': (0x494e4954, 0x2028b8f8, 3000),
        'mix_handoff': (0x4d495831, 28, 0, 0, 1, 1,
                        28 if bypass else 29, 9, 0x444f4e45, 1, int(bypass)),
    }
    return {'outcome': 'complete', 'model': 'l6max',
            'events': [{'observation': {'name': name,
                                        'bytes': list(struct.pack(f'<{len(values)}I', *values))}}
                       for name, values in captures.items()]}


class FileMixHandoffTests(unittest.TestCase):
    def test_project_check_rejects_missing_calls_and_bad_audio(self):
        stock = [0.01] * 64
        samples = stock + [0.0] * 64 + stock + [0.0] * 64
        captures = {
            'selected_type': ('<B', (2,)), 'send_1': ('<B', (50,)), 'send_2': ('<B', (50,)),
            'mix_handoff': ('<11I', (0x4d495831, 1, 0, 0, 1, 1, 1, 0, 0x444f4e45, 0, 0)),
            'file_mix_routed_calls': ('<6I', (100, 6400, 0, 1, 6400, 0)),
            'file_mix_sends': ('<4I256f', (0x43415054, 0, 0, 0, *samples)),
            'file_mix_output': ('<IIff', (100, 0, 0.5, 0.25)),
        }
        check_project(capture_report(captures))
        for name, value in (
            ('send_1', ('<B', (0,))),
            ('file_mix_routed_calls', ('<6I', (0, 0, 0, 0, 0, 0))),
            ('file_mix_routed_calls', ('<6I', (100, 240000, 1, 1, 240000, 0))),
            ('file_mix_output', ('<IIff', (0, 0, 0.0, 0.0))),
            ('file_mix_output', ('<IIff', (100, 1, 0.5, 0.25))),
            ('file_mix_output', ('<IIff', (100, 0, float('nan'), 0.25))),
            ('file_mix_output', ('<IIff', (100, 0, 1.1, 0.25))),
            ('file_mix_sends', ('<4I256f', (0, 0, 0, 0, *samples))),
            ('file_mix_sends', ('<4I256f', (0x43415054, 0, 0, 0, *([float('nan')] + samples[1:])))),
            ('file_mix_sends', ('<4I256f', (0x43415054, 0, 0, 0, *(stock + [0.0] * 192)))),
        ):
            bad = dict(captures)
            bad[name] = value
            with self.subTest(name=name, value=value[1][0]), self.assertRaises(ValueError):
                check_project(capture_report(bad))

    def test_ainr_admission_requires_both_channels_and_no_unknown_callers(self):
        items = {'ainr_admission': {'bytes': list(struct.pack('<7I', 10, 10, 3, 3, 2, 2, 0))}}
        self.assertEqual(check_ainr_admission(items, 'exit')['rejected_during_teardown'], [2, 2])
        for index in range(6):
            bad = copy.deepcopy(items)
            bad['ainr_admission']['bytes'][4 * index:4 * index + 4] = [0, 0, 0, 0]
            with self.subTest(index=index), self.assertRaises(ValueError):
                check_ainr_admission(bad, 'exit')
        bad = copy.deepcopy(items)
        bad['ainr_admission']['bytes'][24] = 1
        with self.assertRaises(ValueError):
            check_ainr_admission(bad, 'exit')
        entry = {'ainr_admission': {'bytes': list(struct.pack('<7I', 10, 10, 0, 0, 0, 0, 0))}}
        check_ainr_admission(entry, 'entry')
        entry['ainr_admission']['bytes'][24] = 1
        with self.assertRaises(ValueError):
            check_ainr_admission(entry, 'entry')

    def test_learned_marker_must_exist_before_and_after_timeout(self):
        items = {'learned_before': {'bytes': [1, 0, 0, 0]}, 'learned_after': {'bytes': [1, 0, 0, 0]}}
        check_learned_marker(items)
        for name in items:
            bad = copy.deepcopy(items)
            bad[name]['bytes'] = [0, 0, 0, 0]
            with self.subTest(name=name), self.assertRaises(ValueError):
                check_learned_marker(bad)

    def test_ainr_entry_timeout_rejects_enabled_flags(self):
        captures = {
            'type_before': ('<B', (4,)), 'type_after': ('<B', (4,)), 'type_next': ('<B', (5,)),
            'callback_before': ('<I', (0x20280781,)), 'callback_after': ('<I', (0x20280781,)),
            'callback_next': ('<I', (0x20280021,)),
            'ainr_left_before': ('<I', (0,)), 'ainr_right_before': ('<I', (0,)),
            'ainr_left_after': ('<I', (0,)), 'ainr_right_after': ('<I', (0,)),
            'ainr_left_next': ('<I', (1,)), 'ainr_right_next': ('<I', (1,)),
            'gain_before': ('<I', (0x3f800000,)), 'gain_after': ('<I', (0x3f800000,)),
            'selector_overlap': ('<6I', (2, 1, 2, 5, 20, 0)),
            'selector_handoff': ('<6I', (6, 5, 1, 5, 0x83e0b000, 0)),
        }
        value = capture_report(captures)
        self.assertTrue(check_ainr_timeout(value, 'entry')['sampled_state_preserved'])
        bad = copy.deepcopy(value)
        next(event for event in bad['events'] if event['observation']['name'] == 'ainr_left_after')['observation']['bytes'][0] = 1
        with self.assertRaises(ValueError):
            check_ainr_timeout(bad, 'entry')

    def test_timeout_reports_partial_recovery_and_rejects_missing_fade(self):
        captures = {
            'before': ('<3I', (0x494e4954, 0x2028b8dc, 10)),
            'after': ('<3I', (0x494e4954, 0x2028b8dc, 20)),
            'later': ('<3I', (0x494e4954, 0x2028b8dc, 30)),
            'pointer_before': ('<I', (0x80109181,)), 'pointer_after': ('<I', (0x80109181,)),
            'fade_before': ('<I', (0x3f800000,)), 'fade_after': ('<I', (0x3f800000,)),
            'state_before': ('<I', (0,)), 'state_after': ('<I', (0,)),
            'type_before': ('<B', (1,)), 'type_after': ('<B', (2,)),
            'selector_handoff': ('<6I', (3, 2, 1, 2, 0x83e0b000, 0)),
            'selector_overlap': ('<6I', (2, 1, 2, 5, 20, 0)),
            'pointer_next': ('<I', (0x8002c041,)),
        }
        value = capture_report(captures)
        self.assertTrue(check_timeout(value)['selection_mismatch'])
        bad = copy.deepcopy(value)
        next(event for event in bad['events'] if event['observation']['name'] == 'fade_after')['observation']['bytes'] = [0, 0, 0, 0]
        with self.assertRaises(ValueError):
            check_timeout(bad)
        for event in value['events']:
            if event['observation']['name'] == 'type_after':
                event['observation']['bytes'] = [1]
        for name, fmt, data in (('type_next', '<B', (2,)),
                                ('next', '<3I', (0x494e4954, 0x2028b8f8, 5))):
            value['events'].append({'observation': {'name': name, 'bytes': list(struct.pack(fmt, *data))}})
        for suffix, rows in (('before', [1 << 18, 0]), ('after', [1 << 18, 0]),
                             ('next', [0, 1 << 19])):
            value['events'].append({'observation': {'name': f'selector_feedback_{suffix}', 'indicators': rows}})
        self.assertFalse(check_timeout(value, True)['selection_mismatch'])
        bad = copy.deepcopy(value)
        bad['events'][-1]['observation']['indicators'] = [1 << 18, 0]
        with self.assertRaises(ValueError):
            check_timeout(bad, True)
        trace = ''.join(f'l6 main LPUART3 (MIDI jacks) TX {byte}\n'
                        for byte in 'b0 75 2b b0 75 2b b0 75 40'.split())
        self.assertEqual(check_midi_trace(trace), 3)
        with self.assertRaises(ValueError):
            check_midi_trace(trace.rsplit('\n', 2)[0])

    def test_checks_each_forced_interleaving(self):
        self.assertEqual(check(report(False), False)['after_signal'], 29)
        self.assertEqual(check(report(True), True)['bypasses'], 1)
        for bypass in (False, True):
            with self.subTest(bypass=bypass), self.assertRaises(ValueError):
                check(report(bypass), not bypass)

    def test_rejects_missing_signal_enabled_stream_and_no_restart(self):
        for index, bit in ((2, 0x10000), (4, 0x80000000), (8, 1), (9, 1)):
            bad = copy.deepcopy(report(True))
            data = bad['events'][2]['observation']['bytes']
            value = struct.unpack('<I', bytes(data[index * 4:index * 4 + 4]))[0]
            data[index * 4:index * 4 + 4] = struct.pack('<I', value ^ bit)
            with self.subTest(index=index), self.assertRaises(ValueError):
                check(bad, True)
        bad = report(True)
        bad['events'][1]['observation']['bytes'][8:12] = list(struct.pack('<I', 28))
        with self.assertRaises(ValueError):
            check(bad, True)

    def test_selector_requires_balanced_guards_and_ainr(self):
        value = report(True)
        captures = {'selector_handoff': (6, 6, 0, 6, 0x83e0b000, 0),
                    'selector_callback_0': (0x20280781,),
                    'selector_callback_1': (0x20281649,),
                    'selector_callback_2': (0x20280021,),
                    'selector_callback_3': (0x20280ab9,)}
        value['events'].extend({'observation': {'name': name,
                               'bytes': list(struct.pack(f'<{len(words)}I', *words))}}
                               for name, words in captures.items())
        self.assertEqual(check(value, True, True)['selector_transitions'], 6)
        bad = copy.deepcopy(value)
        bad['events'][3]['observation']['bytes'][12] = 5
        with self.assertRaises(ValueError):
            check(bad, True, True)
        bad = copy.deepcopy(value)
        bad['events'][6]['observation']['bytes'] = list(struct.pack('<I', 0x20281649))
        with self.assertRaises(ValueError):
            check(bad, True, True)

        value['events'].append({'observation': {'name': 'selector_overlap',
                                'bytes': list(struct.pack('<6I', 2, 1, 2, 5, 3, 4))}})
        self.assertEqual(check(value, True, True, True)['selector_waits'], 3)
        for index in (0, 1, 2, 3, 4, 5):
            bad = copy.deepcopy(value)
            bad['events'][-1]['observation']['bytes'][index * 4:index * 4 + 4] = [0, 0, 0, 0]
            with self.subTest(index=index), self.assertRaises(ValueError):
                check(bad, True, True, True)


if __name__ == '__main__':
    unittest.main()
