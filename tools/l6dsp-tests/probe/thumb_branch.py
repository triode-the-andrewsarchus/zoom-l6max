"""Thumb-2 branch encoding for disposable diagnostic images.

Matches firmware-patch-core::branch; callers must guard the original bytes.
"""
def branch(source, target, link=True):
    displacement = target - (source + 4)
    if displacement & 1 or not -16777216 <= displacement < 16777216:
        raise ValueError("target out of Thumb branch range")
    d = displacement & 0x01ff_ffff
    sign = (d >> 24) & 1
    first = 0xf000 | (sign << 10) | ((d >> 12) & 0x3ff)
    second = (0xd000 if link else 0x9000) | (((~(d >> 23) ^ sign) & 1) << 13) | (((~(d >> 22) ^ sign) & 1) << 11) | ((d >> 1) & 0x7ff)
    return first.to_bytes(2, "little") + second.to_bytes(2, "little")
