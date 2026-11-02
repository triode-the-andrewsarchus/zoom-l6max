"""Guard checks for disposable L6max diagnostic image preparation."""
from pathlib import Path
import subprocess
import sys
import unittest

from fixture import MODE_CALL_SITES, install_ainr_selector_veneer, install_boundary_hook, install_file_mix_handoff, install_mode_hooks, install_offline_mix_hook, install_panel_receive_hooks, validate_probe_entry
from thumb_branch import branch


class ProbeEntryTests(unittest.TestCase):
    def test_fixed_redirect_rejects_missing_moved_or_ambiguous_entry(self):
        validate_probe_entry('80108000 A PROBE_BASE\n80108000 T probe\n')
        for output in ('', '80108078 T probe\n', '80108000 D probe\n',
                       '80108000 T probe\n80108078 T probe\n'):
            with self.subTest(output=output), self.assertRaises(ValueError):
                validate_probe_entry(output)


class PreparerModeTests(unittest.TestCase):
    def test_rejects_incomplete_or_conflicting_modes_before_reading_inputs(self):
        preparer = Path(__file__).with_name('prepare.py')
        selector = ['--callback', '--inspector', '--selector-load-trace']
        cases = [
            (['--callback', '--admission-probe'], '--admission-probe cannot'),
            (['--offline-mix-layout'], '--offline-mix-layout requires'),
            (['--file-mix-handoff', 'bypass'], '--file-mix-handoff requires'),
            (['--callback', '--inspector', '--file-mix-handoff', 'pending'], '--file-mix-handoff requires'),
            (['--callback', '--offline-mix-layout', '--inspector'], '--offline-mix-layout requires'),
            (['--callback', '--offline-mix-layout', '--switches'], '--offline-mix-layout requires'),
            (['--max-velvet-density'], '--max-velvet-density requires'),
            (['--mixer-input-capture'], '--mixer-input-capture requires'),
            (['--callback', '--mixer-input-capture', '--inspector'], '--mixer-input-capture requires'),
            (['--callback', '--mixer-input-capture', '--switches'], '--mixer-input-capture requires'),
            (['--callback', '--mixer-input-capture', '--active-sends'], '--mixer-input-capture requires'),
            (['--submix-return'], '--submix-return requires'),
            (['--callback', '--inspector', '--active-sends', '--submix-return', '--bank-stress'], '--submix-return requires'),
            (['--selector-load-trace', '--callback', '--active-sends'], '--selector-load-trace requires'),
            (['--selector-load-trace', '--inspector', '--timing-control'], '--selector-load-trace requires'),
            (selector, '--selector-load-trace requires'),
            (selector + ['--active-sends', '--timing-control'], '--selector-load-trace requires'),
            (selector + ['--active-sends', '--ring-timing'], '--selector-load-trace requires'),
            (selector + ['--active-sends', '--irq-timing'], '--selector-load-trace requires'),
            (selector + ['--active-sends', '--pre-ainr'], '--selector-load-trace requires'),
            (selector + ['--timing-control', '--bank-stress'], '--selector-load-trace requires'),
        ]
        for flags, message in cases:
            result = subprocess.run([sys.executable, str(preparer), 'missing', 'missing', 'missing', *flags],
                                    capture_output=True, text=True, check=False)
            with self.subTest(flags=flags):
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)


class FileMixHandoffGuardTests(unittest.TestCase):
    def test_guards_both_sites_before_writing(self):
        original = bytearray(0x104f60)
        original[0x4ed8:0x4edc] = bytes.fromhex('fcf76efb')
        original[0x104f58:0x104f60] = bytes.fromhex('f0b581b002f0f3fe')
        for offset in (0x4ed8, 0x104f58):
            image = original[:]
            image[offset] ^= 1
            before = image[:]
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                install_file_mix_handoff(image, 0x80109000, 0x80109100)
            self.assertEqual(image, before)
        for observer, bypass in ((None, None), (0x80109001, None),
                                 (0x80109000, 0x80109101)):
            image = original[:]
            with self.subTest(observer=observer, bypass=bypass), self.assertRaises(ValueError):
                install_file_mix_handoff(image, observer, bypass)
            self.assertEqual(image, original)
        control = original[:]
        install_file_mix_handoff(control, 0x80109000)
        self.assertEqual(control[0x4ed8:0x4edc], branch(0x80004ed8, 0x80109000))
        self.assertEqual(control[0x104f58:0x104f60], original[0x104f58:0x104f60])
        install_file_mix_handoff(original, 0x80109000, 0x80109100)
        self.assertEqual(original[0x4ed8:0x4edc], branch(0x80004ed8, 0x80109000))
        self.assertEqual(original[0x104f58:0x104f60], bytes.fromhex('dff800f001911080'))


class OfflineMixVeneerGuardTests(unittest.TestCase):
    def test_guards_call_site_and_veneer_before_writing(self):
        original = bytearray(0x196f2)
        original[0x196ea:] = bytes.fromhex('d5e900014e60e047')
        for target in (None, 0x80109001, 0x80001000, 0x801a2ff8):
            image = original[:]
            with self.subTest(target=target), self.assertRaises(ValueError):
                install_offline_mix_hook(image, target)
            self.assertEqual(image, original)
        image = original[:]
        image[0x196ea] ^= 1
        before = image[:]
        with self.assertRaises(ValueError):
            install_offline_mix_hook(image, 0x80109000)
        self.assertEqual(image, before)
        install_offline_mix_hook(original, 0x80109000)
        self.assertEqual(original[0x196ea:], branch(0x800196ea, 0x80109000) + bytes.fromhex('00bf00bf'))


class SelectorVeneerGuardTests(unittest.TestCase):
    def setUp(self):
        self.offset = 0x10543c
        self.image = bytearray(self.offset + 8)
        self.image[self.offset:] = bytes.fromhex("214602f0cdfc042e")

    def test_valid_site_redirects_to_thumb_veneer(self):
        install_ainr_selector_veneer(self.image, {"ainr_selector_veneer": 0x80109000})
        self.assertEqual(self.image[self.offset:], bytes.fromhex("dff800f001901080"))

    def test_rejects_changed_site_and_invalid_targets_without_writing(self):
        for target in (None, 0x80109001, 0x80001000, 0x801a2ff8):
            before = self.image[:]
            with self.subTest(target=target), self.assertRaises(ValueError):
                install_ainr_selector_veneer(self.image, {"ainr_selector_veneer": target})
            self.assertEqual(self.image, before)
        self.image[self.offset] ^= 1
        before = self.image[:]
        with self.assertRaises(ValueError):
            install_ainr_selector_veneer(self.image, {"ainr_selector_veneer": 0x80109000})
        self.assertEqual(self.image, before)


class ModeHookGuardTests(unittest.TestCase):
    def setUp(self):
        self.image = bytearray(0x37172)
        # Exact L6max 1.10 Thumb BL bytes, independently captured from MAIN.
        originals = ("1cf0ecfe", "1cf0bffe", "1cf0affe", "1cf075fe",
                     "1cf031fe", "1cf021fe", "1cf011fe", "1cf02ffd", "1cf01ffd")
        for address, code in zip(MODE_CALL_SITES, originals, strict=True):
            offset = address - 0x80000000
            self.image[offset:offset + 4] = bytes.fromhex(code)

    def test_redirects_all_observed_mode_calls(self):
        install_mode_hooks(self.image, 0x80109000)
        for address in MODE_CALL_SITES:
            offset = address - 0x80000000
            self.assertEqual(self.image[offset:offset + 4], branch(address, 0x80109000))

    def test_any_changed_site_or_bad_target_rejects_without_writes(self):
        for address in MODE_CALL_SITES:
            image = self.image[:]
            image[address - 0x80000000] ^= 1
            before = image[:]
            with self.subTest(address=hex(address)), self.assertRaises(ValueError):
                install_mode_hooks(image, 0x80109000)
            self.assertEqual(image, before)
        for target in (None, 0x80109001, 0x80001000, 0x801a2ff8):
            before = self.image[:]
            with self.subTest(target=target), self.assertRaises(ValueError):
                install_mode_hooks(self.image, target)
            self.assertEqual(self.image, before)


class BoundaryHookGuardTests(unittest.TestCase):
    def test_guarded_pre_disable_call(self):
        image = bytearray(0x54360)
        image[0x5435c:] = bytes.fromhex("b6f7c0ff")
        install_boundary_hook(image, 0x80109000)
        self.assertEqual(image[0x5435c:], branch(0x8005435c, 0x80109000))
        for target in (None, 0x80109001, 0x80001000, 0x801a2ff8):
            candidate = bytearray(0x54360)
            candidate[0x5435c:] = bytes.fromhex("b6f7c0ff")
            before = candidate[:]
            with self.subTest(target=target), self.assertRaises(ValueError):
                install_boundary_hook(candidate, target)
            self.assertEqual(candidate, before)
        before = image[:]
        with self.assertRaises(ValueError):
            install_boundary_hook(image, 0x80109000)
        self.assertEqual(image, before)


class PanelReceiveHookGuardTests(unittest.TestCase):
    def test_all_sites_are_checked_before_any_redirect(self):
        sites = ((0x4cf14, "45f098f8"), (0x4cf4e, "45f07bf8"),
                 (0x4d090, "44f0daff"), (0x4d0e2, "44f0b1ff"))
        original = bytearray(0x4d0e6)
        for offset, encoded in sites:
            original[offset:offset + 4] = bytes.fromhex(encoded)
        for offset, _ in sites:
            image = original[:]
            image[offset] ^= 1
            before = image[:]
            with self.subTest(offset=hex(offset)), self.assertRaises(ValueError):
                install_panel_receive_hooks(image, 0x80109000)
            self.assertEqual(image, before)
        for target in (None, 0x80109001, 0x80001000, 0x801a2ff8):
            image = original[:]
            with self.subTest(target=target), self.assertRaises(ValueError):
                install_panel_receive_hooks(image, target)
            self.assertEqual(image, original)
        install_panel_receive_hooks(original, 0x80109000)
        for offset, _ in sites:
            self.assertEqual(original[offset:offset + 4], branch(0x80000000 + offset, 0x80109000))


if __name__ == "__main__":
    unittest.main()
