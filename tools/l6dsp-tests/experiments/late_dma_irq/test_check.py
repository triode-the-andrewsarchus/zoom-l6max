import copy
import io
import struct
import sys
import tempfile
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'probe'))
from test_file_mix_handoff import report
from check import EXPECTED, validate
from check_restart import BASE, read_markers, records, summarize


class DmaRecordTests(unittest.TestCase):
    def test_reads_addressed_fixed_frames(self):
        data = struct.pack('<II', BASE, 32) + bytes(range(32))
        self.assertEqual(records(io.BytesIO(b'head' + data), 4, 1), [(BASE, bytes(range(32)))])

    def test_markers_require_matching_aligned_offsets(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'markers.txt'
            offset = 16 + 5000 * 40
            path.write_text(f'inject {offset}\nrestart {offset}\n')
            self.assertEqual(read_markers(path), (offset, offset))
            for bad in (f'inject {offset}\nrestart {offset + 40}\n', 'inject\nrestart 1\n'):
                path.write_text(bad)
                with self.subTest(markers=bad), self.assertRaises(ValueError):
                    read_markers(path)

    def test_summary_requires_nonzero_output_on_both_sides(self):
        silent = [(BASE, bytes(32))]
        active = [(BASE, b'\x01' + bytes(31))]
        for before, after in ((silent, active), (active, silent)):
            with self.subTest(before=before), self.assertRaises(ValueError):
                summarize(bytes(4096), before, after, 0)

    def test_rejects_truncated_or_out_of_range_records(self):
        for data in (b'bad', struct.pack('<II', BASE - 4, 32) + bytes(32),
                     struct.pack('<II', BASE, 31) + bytes(32),
                     struct.pack('<II', BASE + 1, 32) + bytes(32)):
            with self.subTest(data=data), self.assertRaises(ValueError):
                records(io.BytesIO(data), 0, 1)


class LateDmaIrqTests(unittest.TestCase):
    def setUp(self):
        self.report = report(False)
        self.report['events'].append({'observation': {'name': 'mix_irq',
            'bytes': list(struct.pack('<4I', 0, 0x2000b740, 0, 0))}})
        self.report['events'].append({'observation': {'name': 'mix_buffers',
            'bytes': list(struct.pack('<2I', 64, 64))}})
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
            data = bad['events'][-2]['observation']['bytes']
            data[index * 4:index * 4 + 4] = struct.pack('<I', value)
            with self.subTest(index=index), self.assertRaises(ValueError):
                validate(bad, self.trace, False)

    def test_no_interrupt_control_requires_unchanged_buffers_and_no_entries(self):
        control = report(True)
        control['events'].extend(copy.deepcopy(self.report['events'][-2:]))
        data = control['events'][-3]['observation']['bytes']
        data[40:44] = struct.pack('<I', 0)
        control['events'][-1]['observation']['bytes'] = list(struct.pack('<2I', 0, 0))
        quiet = self.trace.replace(' 1', ' 0')
        self.assertEqual(validate(control, quiet, False, True)['bypasses'], 0)
        with self.assertRaises(ValueError):
            validate(control, self.trace, False, True)
        control['events'][-1]['observation']['bytes'] = list(struct.pack('<2I', 0, 1))
        with self.assertRaises(ValueError):
            validate(control, quiet, False, True)

    def test_outer_guard_rejects_audio_before_scratch_writes(self):
        outer = report(True)
        outer['events'].extend(copy.deepcopy(self.report['events'][-2:]))
        outer['events'][-1]['observation']['bytes'] = list(struct.pack('<2I', 0, 0))
        outer_trace = self.trace.replace('IRQ_PC 800149b0 1', 'IRQ_PC 800149b0 0')
        self.assertEqual(validate(outer, outer_trace, True, outer=True)['bypasses'], 1)
        with self.assertRaises(ValueError):
            validate(outer, self.trace, True, outer=True)
        outer['events'][-1]['observation']['bytes'] = list(struct.pack('<2I', 1, 0))
        with self.assertRaises(ValueError):
            validate(outer, outer_trace, True, outer=True)

    def test_guarded_result_cannot_be_confused_with_unguarded(self):
        guarded = report(True)
        guarded['events'].extend(self.report['events'][-2:])
        self.assertEqual(validate(guarded, self.trace, True)['bypasses'], 1)
        with self.assertRaises(ValueError):
            validate(guarded, self.trace, False)


if __name__ == '__main__':
    unittest.main()
