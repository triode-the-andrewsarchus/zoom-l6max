"""Guard checks for disposable L6max diagnostic image preparation."""
import unittest

from fixture import MODE_CALL_SITES, install_ainr_selector_veneer, install_mode_hooks
from thumb_branch import branch


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


if __name__ == "__main__":
    unittest.main()
