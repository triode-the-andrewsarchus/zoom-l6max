"""Validate both the forced late callback and its experimental bypass."""
import copy
import struct
import unittest

from file_mix_handoff import check


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


if __name__ == '__main__':
    unittest.main()
