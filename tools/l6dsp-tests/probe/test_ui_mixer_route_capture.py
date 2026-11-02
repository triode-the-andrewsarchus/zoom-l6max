import struct
import subprocess
import sys
import unittest
from pathlib import Path

from mixer_input_capture import check_ui_routes as check


def report():
    observations = {}
    def words(name, *values):
        observations[name] = list(struct.pack('<' + 'I' * len(values), *values))
    words('active_room', 0x8010b32d)
    words('room_open', 0, 0, 0, 0, 0, 0, 0, 1)
    words('route_before', *([2] * 12))
    words('route_1', 3, *([2] * 11))
    words('route_5', 3, 2, 2, 2, 3, 3, *([2] * 6))
    words('route_reverse', 2, 2, 2, 2, 3, 3, *([2] * 6))
    words('queue_room', 11, 11, 0)
    words('callback_state', 0x494e4954, 0x2028b8dc, 1000)
    for name, bus in (('route_before', 1), ('route_1', 2), ('route_reverse', 1)):
        samples = [0.0] * 192
        samples[bus * 64] = 1e-5
        for i in range(6):
            observations[f'{name}_sends_{i}'] = list(struct.pack('<192f', *samples))
    return {'outcome': 'complete', 'model': 'l6max', 'events': [
        {'observation': {'name': name, 'bytes': data}} for name, data in observations.items()]}


class UiMixerRouteCaptureTests(unittest.TestCase):
    def setUp(self):
        self.report = report()
        self.events = {e['observation']['name']: e['observation'] for e in self.report['events']}

    def test_valid_and_silent_snapshots(self):
        self.events['route_1_sends_0']['bytes'] = [0] * 768
        self.assertEqual(check(self.report)['callback_count'], 1000)

    def test_stuck_route_rejected(self):
        self.events['route_1']['bytes'] = self.events['route_before']['bytes']
        with self.assertRaisesRegex(ValueError, 'route_1'):
            check(self.report)

    def test_wrong_bus_and_silence_rejected(self):
        for i in range(6):
            self.events[f'route_1_sends_{i}']['bytes'] = self.events[f'route_before_sends_{i}']['bytes']
        with self.assertRaisesRegex(ValueError, 'no clean'):
            check(self.report)

    def test_nonfinite_and_queue_loss_rejected(self):
        self.events['route_before_sends_0']['bytes'][:4] = struct.pack('<f', float('nan'))
        with self.assertRaisesRegex(ValueError, 'nonfinite'):
            check(self.report)
        self.report = report()
        self.events = {e['observation']['name']: e['observation'] for e in self.report['events']}
        self.events['queue_room']['bytes'] = list(struct.pack('<III', 11, 11, 1))
        with self.assertRaisesRegex(ValueError, 'queue'):
            check(self.report)

    def test_synthetic_sends_and_timing_options_are_excluded(self):
        builder = Path(__file__).with_name('prepare_ui.py')
        for option in ('--active-sends', '--irq-timing', '--ring-timing', '--enqueue-trace'):
            result = subprocess.run([sys.executable, str(builder), 'unused', 'unused', 'unused',
                                     '--mixer-route-capture', option], capture_output=True, text=True)
            with self.subTest(option=option):
                self.assertEqual(result.returncode, 2)
                self.assertIn('--mixer-route-capture excludes', result.stderr)


if __name__ == '__main__':
    unittest.main()
