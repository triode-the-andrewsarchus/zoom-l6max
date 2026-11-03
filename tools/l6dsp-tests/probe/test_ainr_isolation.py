"""Checks for the QEMU-only AINR isolation helpers."""
import contextlib
import io
import math
import struct
import sys
import unittest
from unittest.mock import patch

from ainr_isolation import (check_allcpu_window, check_trace_controls, compare_solo_prefix,
                            parse_arguments, synthetic_stereo_input, trace_controls)


class ControlGuest:
    def __init__(self, return_pc=0x20228d8a):
        self.return_pc = return_pc
        self.writes = []
        self.registers = []
        self.commands = []

    def packet(self, command):
        self.commands.append(command)
        return 'saved-registers' if command == 'g' else 'S05'

    def expect(self, command):
        self.commands.append(command)

    def write(self, address, data):
        self.writes.append((address, data))

    def setreg(self, index, value):
        self.registers.append((index, value))

    def reg(self, index):
        assert index == 15
        return self.return_pc

    def read(self, address, length):
        assert address == 0x80e61000 and length == 256
        return bytes(256)


class IsolationHelperTests(unittest.TestCase):
    def test_trace_controls_cover_both_regions_and_restore_registers(self):
        guest = ControlGuest()
        trace_controls(guest, stereo=True)
        self.assertEqual(guest.writes, [(0x80f1ff00, bytes.fromhex('00687047') * 4)])
        self.assertEqual([value for index, value in guest.registers if index == 0],
                         [0x80e00000, 0x80e20000, 0x80e30000, 0x80e50000])
        self.assertEqual([value for index, value in guest.registers if index == 15],
                         [0x80f1ff00, 0x80f1ff04, 0x80f1ff08, 0x80f1ff0c])
        self.assertEqual(guest.commands[-2:], ['z0,20228d8a,2', 'Gsaved-registers'])
        with self.assertRaisesRegex(RuntimeError, 'positive-control return'):
            trace_controls(ControlGuest(return_pc=0), stereo=True)
        mono = ControlGuest()
        trace_controls(mono, stereo=False)
        self.assertEqual([value for index, value in mono.registers if index == 0], [0x80e00000])

    def test_synthetic_input_requires_stereo(self):
        args = ['probe', 'firmware', 'state', 'output', '--qemu', 'qemu', '--synthetic-stereo']
        with patch.object(sys, 'argv', args), contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                parse_arguments()
        with patch.object(sys, 'argv', args + ['--stereo', '--trace-control']):
            _, parsed = parse_arguments()
            self.assertTrue(parsed.synthetic_stereo and parsed.trace_control and parsed.stereo)
        with patch.object(sys, 'argv', args + ['--stereo', '--allcpu-window']), contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                parse_arguments()
        with patch.object(sys, 'argv', args + ['--stereo', '--solo-tail', '--allcpu-window']):
            _, parsed = parse_arguments()
            self.assertTrue(parsed.allcpu_window)

    def test_trace_result_requires_exact_controls(self):
        self.assertEqual(check_trace_controls('CLONE_TRACE positive=1,1,1,1\n', True), [1, 1, 1, 1])
        self.assertEqual(check_trace_controls('CLONE_TRACE positive=1,0,0,0\n', False), [1, 0, 0, 0])
        for trace in ('', 'CLONE_TRACE positive=1,1,0,1\n',
                      'CLONE_TRACE positive=1,1,1,1\nCLONE_TRACE positive=1,1,1,1\n'):
            with self.subTest(trace=trace), self.assertRaisesRegex(RuntimeError, 'positive controls'):
                check_trace_controls(trace, True)

    def test_allcpu_window_brackets_solo_and_checks_controls(self):
        guest = ControlGuest()
        compare_solo_prefix(guest, 'saved-registers', bytes(256), bytes(256), 0x20228d8a, 0, False, True)
        self.assertEqual([value for index, value in guest.registers if index == 15],
                         [0x80f1ff10, 0x80f1ff18, 0x80f1ff1c, 0x80f10780, 0x80f1ff14])
        rows = ''.join(f'ALLCPU actor={a} region={r} reads={1 if (a, r) == (0, 1) else 0} '
                       f'writes={1 if (a, r) == (0, 1) else 0} first_read=0 first_write=0 min=0 end=0\n'
                       for a in range(2) for r in range(4))
        header = 'ALLCPU starts=32 ends=32 active=0 invalid=0 positive=1,1\n'
        trace = header + rows + 'ALLCPU pc_overflow=0 ignored=0\n'
        self.assertEqual(check_allcpu_window(trace, 32)['copied_code'][1], [1, 1])
        for bad in (trace.replace('ends=32', 'ends=31'), trace.replace('positive=1,1', 'positive=1,0'),
                    trace.replace('ignored=0', 'ignored=1'), trace + rows.splitlines()[0] + '\n'):
            with self.subTest(trace=bad), self.assertRaises(RuntimeError):
                check_allcpu_window(bad, 32)

    def test_synthetic_inputs_are_distinct_bounded_and_continuous(self):
        channels = [struct.unpack('<64f', synthetic_stereo_input(1, channel)) for channel in (0, 1)]
        self.assertNotEqual(channels[0], channels[1])
        for channel, samples in enumerate(channels):
            self.assertTrue(all(math.isfinite(x) and abs(x) <= 12000000.0 for x in samples))
            period = (37, 53)[channel]
            expected = 12000000.0 * math.sin(2 * math.pi * 64 / period)
            self.assertAlmostEqual(samples[0] / 12000000.0, expected / 12000000.0, places=6)


if __name__ == '__main__':
    unittest.main()
