"""Guard checks for disposable L6max diagnostic image preparation."""
import unittest

from fixture import install_ainr_selector_veneer


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


if __name__ == "__main__":
    unittest.main()
