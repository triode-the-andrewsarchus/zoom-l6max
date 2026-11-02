import copy
import struct
import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'probe'))
from test_file_mix_handoff import report
from check import EXPECTED, validate


class LateDmaIrqTests(unittest.TestCase):
    def setUp(self):
        self.report = report(False)
        self.report['events'].append({'observation': {'name': 'mix_irq',
            'bytes': list(struct.pack('<4I', 0, 0x2000b740, 0, 0))}})
        self.trace = '\n'.join(['IRQ_PROFILE_BEGIN',
            *(f'IRQ_PC {address} 1' for address in EXPECTED), 'IRQ_PROFILE_END'])

    def test_requires_actual_interrupt_chain(self):
        self.assertEqual(validate(self.report, self.trace, False)['after_signal'], 29)
        for bad in (self.trace.replace('IRQ_PC 80043af8 1', 'IRQ_PC 80043af8 0'),
                    self.trace.replace('IRQ_PROFILE_END', '')):
            with self.subTest(trace=bad), self.assertRaises(ValueError):
                validate(self.report, bad, False)

    def test_rejects_unexpected_dma_state(self):
        for index, value in ((0, 1 << 15), (1, 0), (2, 1), (3, 1 << 15)):
            bad = copy.deepcopy(self.report)
            data = bad['events'][-1]['observation']['bytes']
            data[index * 4:index * 4 + 4] = struct.pack('<I', value)
            with self.subTest(index=index), self.assertRaises(ValueError):
                validate(bad, self.trace, False)

    def test_guarded_result_cannot_be_confused_with_unguarded(self):
        guarded = report(True)
        guarded['events'].append(self.report['events'][-1])
        self.assertEqual(validate(guarded, self.trace, True)['bypasses'], 1)
        with self.assertRaises(ValueError):
            validate(guarded, self.trace, False)


if __name__ == '__main__':
    unittest.main()
