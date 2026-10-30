"""Validation shared by the disposable L6max diagnostic builders."""
import hashlib
from pathlib import Path
import tempfile

MAIN_SHA = "980d92272c1f84bed200b4d2f83575202f6a00082c928d99b9e8b53af8d44461"
VENEER_SHA = "c366ef222fb2b2b87302d6eae469b721f11ba6b41dd79d4b7fe16a582e332c13"


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
