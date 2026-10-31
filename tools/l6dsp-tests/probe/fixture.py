"""Validation shared by the disposable L6max diagnostic builders."""
import hashlib
from pathlib import Path
import tempfile
from thumb_branch import branch

MAIN_SHA = "980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461"
# L6max 1.10 direct calls from the recorder event dispatcher.
MODE_CALL_SITES = (0x80036dd4, 0x80036e2e, 0x80036e4e, 0x80036ec2,
                   0x80036f4a, 0x80036f6a, 0x80036f8a, 0x8003714e, 0x8003716e)
VENEER_SHA = "c366ef222fb2b2b87302d6eae469b721f11ba6b41dd79d4b7fe16a582e332c13"


def install_mode_hooks(image, target):
    """Guard every observed direct mode call before changing the copied image."""
    if target is None or target & 1 or not 0x80108000 <= target < 0x801a2ff8:
        raise ValueError("invalid diagnostic mode hook")
    for address in MODE_CALL_SITES:
        offset = address - 0x80000000
        if image[offset:offset + 4] != branch(address, 0x80053bb0):
            raise ValueError(f"mode call site mismatch at {address:#x}")
    for address in MODE_CALL_SITES:
        offset = address - 0x80000000
        image[offset:offset + 4] = branch(address, target)


def install_ainr_selector_veneer(image, symbols):
    """Redirect the revision-guarded, aligned selector site in a disposable image."""
    offset = 0x10543c
    target = symbols.get("ainr_selector_veneer")
    if (image[offset:offset + 8] != bytes.fromhex("214602f0cdfc042e")
            or target is None or target & 1 or not 0x80108000 <= target < 0x801a2ff8):
        raise ValueError("AINR selection site or veneer mismatch")
    image[offset:offset + 8] = bytes.fromhex("dff800f0") + (target | 1).to_bytes(4, "little")


def install_boundary_hook(image, target):
    """Guard the observed L6max 1.10 panel pre-disable call."""
    offset = 0x5435c
    if (image[offset:offset + 4] != bytes.fromhex("b6f7c0ff")
            or target is None or target & 1 or not 0x80108000 <= target < 0x801a2ff8):
        raise ValueError("pre-disable call or boundary hook mismatch")
    image[offset:offset + 4] = branch(0x8005435c, target)


def checked_inputs(parser, firmware, state, output):
    out = output.resolve()
    if not out.is_relative_to(Path(tempfile.gettempdir()).resolve()) or out.exists():
        parser.error("output must be a new path under the system temporary directory")
    for source in (firmware.resolve(), state.resolve()):
        if out.is_relative_to(source) or source.is_relative_to(out):
            parser.error("output must be separate from input directories")
    main = (firmware / "main_firmware.bin").read_bytes()
    nor = (state / "main-nor.bin").read_bytes()
    if hashlib.sha256(main).hexdigest() != MAIN_SHA:
        parser.error("stock MAIN SHA-256 does not match L6max 1.10")
    if nor[0x50000:0x50000 + len(main)] != main:
        parser.error("state NOR does not contain the selected stock MAIN")
    if hashlib.sha256(main[0x107de6:0x107df0]).hexdigest() != VENEER_SHA:
        parser.error("stock clear veneer does not match")
    if len(main) > 0x108000:
        parser.error("stock MAIN overlaps probe payload")
    return out, main, nor
