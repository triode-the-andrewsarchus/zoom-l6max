"""Checks for the QEMU-only mixer capture evaluator."""
import copy
import struct
import unittest

from mixer_input_capture import check


def report():
    events = []
    def add(label, field, data):
        events.append({'observation': {'name': f'{label}_{field}', 'bytes': list(data)}})
    source = struct.pack('<768f', *([1073741824.0] * 768))
    gains = bytearray(512)
    struct.pack_into('<f', gains, 8, 1.0)
    struct.pack_into('<f', gains, 0x38, 0.25)
    output = struct.pack('<128f', *([0.1] * 128))
    for label, descriptor, route in (('plateau', 0x2028b8dc, 1), ('velvet', 0x2028b8f8, 0)):
        sends = [0.0] * 192
        sends[route * 64:(route + 1) * 64] = [0.125] * 64
        add(label, 'state', struct.pack('<III', 0x494e4954, descriptor, 200))
        add(label, 'marker', struct.pack('<II', 0x5349474e, 150))
        add(label, 'sends', struct.pack('<192f', *sends))
        add(label, 'output', output)
        add(label, 'source', source)
        add(label, 'gains', gains)
    return {'outcome': 'complete', 'model': 'l6max', 'events': events}


def field(result, name):
    return next(e['observation']['bytes'] for e in result['events']
                if e['observation']['name'] == name)


class MixerInputCaptureTests(unittest.TestCase):
    def test_valid_capture_and_wrong_send(self):
        good = report()
        self.assertEqual(check(good)['plateau']['send_peak'], 0.125)
        bad = copy.deepcopy(good)
        field(bad, 'plateau_sends')[64 * 4] ^= 1
        with self.assertRaisesRegex(ValueError, 'reconstructed send differs'):
            check(bad)

    def test_rejects_ramp_and_silent_return(self):
        bad = report()
        field(bad, 'velvet_gains')[0x98] = 1
        with self.assertRaisesRegex(ValueError, 'active ramp'):
            check(bad)
        bad = report()
        field(bad, 'plateau_output')[:] = [0] * 512
        with self.assertRaisesRegex(ValueError, 'silent'):
            check(bad)
        bad = report()
        field(bad, 'velvet_output')[:4] = list(struct.pack('<f', float('nan')))
        with self.assertRaisesRegex(ValueError, 'nonfinite'):
            check(bad)
        bad = report()
        field(bad, 'velvet_output')[:4] = list(struct.pack('<f', 1.01))
        with self.assertRaisesRegex(ValueError, 'exceeds normalized bounds'):
            check(bad)
        bad = report()
        field(bad, 'velvet_marker')[:4] = [0] * 4
        with self.assertRaisesRegex(ValueError, 'marker mismatch'):
            check(bad)


if __name__ == '__main__':
    unittest.main()
