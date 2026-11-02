"""Reject incomplete and inconsistent synthetic file-mix observations."""
import copy
import struct
import unittest

from offline_layout import check


def report():
    tracks = []
    sums = []
    for block in range(2):
        frames = [block * 64, block * 64 + 63]
        sums.extend(sum(((slot + 1) * 1_000_000 + frame * 1000) * (slot + 1) / 16
                        for slot in range(12)) / 2**31 for frame in frames)
        for slot in range(12):
            tracks.extend((slot + 1) * 1_000_000 + frame * 1000 for frame in frames)
    capture = struct.pack('<II52f', 2, 0x4f46464c, *sums, *tracks)
    return {'outcome': 'complete', 'model': 'l6max',
            'events': [{'observation': {'name': 'offline_layout', 'bytes': list(capture)}}]}


class OfflineLayoutTests(unittest.TestCase):
    def test_accepts_both_weighted_blocks(self):
        self.assertEqual(check(report())['callbacks'], 2)

    def test_rejects_missing_callback_wrong_sum_or_track(self):
        for offset in (0, 4, 11, 24, 215):
            bad = copy.deepcopy(report())
            bad['events'][0]['observation']['bytes'][offset] ^= 1
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                check(bad)
        bad = report()
        bad['outcome'] = 'failed'
        with self.assertRaises(ValueError):
            check(bad)


if __name__ == '__main__':
    unittest.main()
