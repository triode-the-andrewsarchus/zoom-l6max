"""Failure checks for the disposable closed-admission replay."""
import struct
import unittest

from ainr_pair_replay import AFTER, BUFFER, GATE, REJECTED, STATE, check_closed_pair


class ClosedReplayGuest:
    def __init__(self, fault=None):
        self.fault = fault
        self.gate = 0
        self.counts = (5, 9)
        self.state = b'unchanged'
        self.audio = b'left-and-right'

    def read(self, address, length):
        if address == GATE:
            return struct.pack('<3I', self.gate, 0, 0x47415445)[:length]
        if address == REJECTED:
            return struct.pack('<2I', *self.counts)
        if address == BUFFER:
            return self.audio
        if address == STATE:
            return self.state
        raise AssertionError(hex(address))

    def write(self, address, value):
        if address != GATE:
            raise AssertionError(hex(address))
        self.gate = struct.unpack('<I', value)[0]

    def packet(self, command):
        if command != 'c':
            raise AssertionError(command)
        self.counts = (6, 9 if self.fault == 'right_counter' else 10)
        if self.fault == 'audio':
            self.audio = b'altered'
        if self.fault == 'state':
            self.state = b'altered'
        if self.fault == 'gate':
            self.gate = 0

    def reg(self, index):
        if index == 15:
            return AFTER
        if index == 0:
            return int(self.fault == 'return')
        raise AssertionError(index)


class ClosedReplayTests(unittest.TestCase):
    def test_both_channels_and_state_must_survive_rejection(self):
        guest = ClosedReplayGuest()
        check_closed_pair(guest, guest.audio, guest.state, 0)
        self.assertEqual(guest.gate, 0)
        for fault in ('right_counter', 'audio', 'state', 'gate', 'return'):
            with self.subTest(fault=fault), self.assertRaises(RuntimeError):
                guest = ClosedReplayGuest(fault)
                check_closed_pair(guest, guest.audio, guest.state, 0)


if __name__ == '__main__':
    unittest.main()
