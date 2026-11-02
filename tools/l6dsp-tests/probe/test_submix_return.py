"""Failure checks for the QEMU-only paired Sub-Mix observation."""
import copy
import struct
import unittest

from submix_return import compare


def report(enabled):
    data = {'return_flags': bytes((int(enabled), 0))}
    for suffix, count in (('initial', 10), ('final', 20)):
        data[f'effect_{suffix}'] = struct.pack('<III', 0x494e4954, 0x2028b8dc, count)
        data[f'audio_{suffix}'] = struct.pack('<IIff', 0x5349474e, count, 0.25, -0.5)
        frame = (0, 0, 0, 0, 12, -20, 12 if enabled else 0, -20 if enabled else 0)
        data[f'tx_{suffix}'] = struct.pack('<1024i', *(frame * 128))
    return {'outcome': 'complete', 'model': 'l6max', 'main_sha256': 'a' * 64,
            'events': [{'observation': {'name': name, 'bytes': list(value)}}
                       for name, value in data.items()]}


class SubmixComparisonTests(unittest.TestCase):
    def test_matched_observations_and_reversed_settings(self):
        off, on = report(False), report(True)
        self.assertEqual(compare(off, on)['on_counts'], [10, 20])
        with self.assertRaises(ValueError):
            compare(on, off)

    def test_rejects_silent_mismatched_and_unrelated_observations(self):
        off, on = report(False), report(True)
        for field in ('tx_final', 'audio_final', 'effect_final'):
            bad = copy.deepcopy(on)
            item = next(e['observation'] for e in bad['events'] if e['observation']['name'] == field)
            item['bytes'] = [0] * len(item['bytes'])
            with self.subTest(field=field), self.assertRaises(ValueError):
                compare(off, bad)
        on['main_sha256'] = 'different'
        with self.assertRaises(ValueError):
            compare(off, on)

    def test_rejects_wrong_submix_slot_with_other_audio_present(self):
        for enabled in (False, True):
            off, on = report(False), report(True)
            bad = on if enabled else off
            item = next(e['observation'] for e in bad['events']
                        if e['observation']['name'] == 'tx_initial')
            item['bytes'][6 * 4] ^= 1
            with self.subTest(enabled=enabled), self.assertRaises(ValueError):
                compare(off, on)


if __name__ == '__main__':
    unittest.main()
