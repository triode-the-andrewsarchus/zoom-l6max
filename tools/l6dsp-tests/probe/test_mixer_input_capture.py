"""Checks for the QEMU-only mixer capture evaluator."""
import copy
import struct
import unittest

from mixer_input_capture import check, check_combined


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
        add(label, 'levels', struct.pack('<IIIfff', 190, 120, 0, 0.125, 0.1, 0.1))
        add(label, 'sends', struct.pack('<192f', *sends))
        add(label, 'output', output)
        add(label, 'source', source)
        add(label, 'gains', gains)
    return {'outcome': 'complete', 'model': 'l6max', 'events': events}


def field(result, name):
    return next(e['observation']['bytes'] for e in result['events']
                if e['observation']['name'] == name)


class MixerInputCaptureTests(unittest.TestCase):
    def test_combined_rejects_silent_bus_and_missed_selection(self):
        good = report()
        for label in ('plateau', 'velvet'):
            good['events'].append({'observation': {
                'name': f'{label}_config',
                'bytes': list(struct.pack('<ffII', 4.0, 500.0, 512112, 400))}})
            field(good, f'{label}_state')[8:12] = list(struct.pack('<I', 1200))
            field(good, f'{label}_marker')[4:8] = list(struct.pack('<I', 1000))
            field(good, f'{label}_levels')[:] = list(struct.pack('<IIIfff', 1100, 1100, 0, 0.125, 0.1, 0.1))
            field(good, f'{label}_sends')[:] = list(struct.pack('<192f', *([0.125] * 192)))
            gains = bytearray(field(good, f'{label}_gains'))
            for slot in (1, 2):
                struct.pack_into('<f', gains, 8 + 4 * slot, 1.0)
                struct.pack_into('<f', gains, 0x38 + 8 * slot, 0.25)
            field(good, f'{label}_gains')[:] = gains
        self.assertEqual(check_combined(good)['plateau']['send_peaks'], [0.125] * 3)
        expanded = copy.deepcopy(good)
        for label in ('plateau', 'velvet'):
            field(expanded, f'{label}_config')[8:12] = list(struct.pack('<I', 512856))
        with self.assertRaisesRegex(ValueError, 'unexpected processor configuration'):
            check_combined(expanded)
        self.assertEqual(check_combined(expanded, 512856)['plateau']['send_peaks'], [0.125] * 3)
        with self.assertRaisesRegex(ValueError, 'unexpected processor configuration'):
            check_combined(expanded, 512855)
        bad = copy.deepcopy(good)
        field(bad, 'plateau_sends')[:256] = [0] * 256
        with self.assertRaisesRegex(ValueError, 'bus 0'):
            check_combined(bad)
        bad = copy.deepcopy(good)
        field(bad, 'velvet_state')[4:8] = list(struct.pack('<I', 0x2028b8dc))
        with self.assertRaisesRegex(ValueError, 'velvet: callback selection'):
            check_combined(bad)

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

    def test_rejects_insufficient_or_inaudible_aggregate_output(self):
        for values in ((190, 99, 0, 0.125, 0.1, 0.1),
                       (190, 120, 1, 0.125, 0.1, 0.1),
                       (190, 120, 0, 0.125, 1e-12, 0.1),
                       (190, 120, 0, 0.125, 0.1, float('nan'))):
            bad = report()
            field(bad, 'plateau_levels')[:] = list(struct.pack('<IIIfff', *values))
            with self.subTest(values=values), self.assertRaises(ValueError):
                check(bad)


if __name__ == '__main__':
    unittest.main()
