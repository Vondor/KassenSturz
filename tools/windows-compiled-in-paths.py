#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The libraries of the Windows package come from KDE Craft's build and have
# its folder compiled in as a string: "C:\Craft..." or "C:/Craft...". Any user
# can create such a folder on a computer that has no Craft, and a library that
# looks there for a plugin or a program (Sonnet's spell-check plugins,
# kconf_update.exe, translation catalogs) could be made to load a planted file
# (CWE-427, B-293).
#
# The Craft recipe runs `patch` on the package before it is packed: each
# "C:\Craft" becomes "\\.\NUL\" and each "C:/Craft" becomes "//./NUL/", the
# same number of characters, so nothing in the file moves. A path below the
# NUL device can never exist, so nothing can be planted there; the library
# finds nothing, exactly as on a computer that never had Craft. The workflow
# runs `check` on the unpacked zip and fails if one is left.
#
#   python tools/windows-compiled-in-paths.py patch|check|report <folder>
#   python tools/windows-compiled-in-paths.py self-test
#
# All three list the files and the number of strings. `check` exits with 1
# when there is one, `report` never does. Both ASCII and UTF-16 (the wide
# strings Windows programs use) are handled, in any case of the letters.

import re
import sys
from pathlib import Path

# Files that can hold a path: programs, libraries and the text files that
# configure them.
SUFFIXES = {".exe", ".dll", ".pyd", ".cnf", ".xml", ".ini", ".conf", ".json", ".txt", ".cfg"}

# "C:\Craft" or "C:/Craft", any case, as ASCII; as UTF-16-LE the same with a
# zero byte after each character.
ASCII = re.compile(rb"(?i)[a-z]:([\\/])craft")
WIDE = re.compile(rb"(?is)[a-z]\x00:\x00([\\/])\x00c\x00r\x00a\x00f\x00t\x00")

# What replaces them, per separator, with the same length.
NUL = {b"\\": b"\\\\.\\NUL\\", b"/": b"//./NUL/"}


def _ascii(match: re.Match) -> bytes:
    return NUL[match.group(1)]


def _wide(match: re.Match) -> bytes:
    return NUL[match.group(1)].decode("ascii").encode("utf-16-le")


def count(data: bytes) -> int:
    return len(ASCII.findall(data)) + len(WIDE.findall(data))


def patched(data: bytes) -> bytes:
    new = ASCII.sub(_ascii, data)
    new = WIDE.sub(_wide, new)
    assert len(new) == len(data), "a replacement changed the length"
    return new


def files(folder: Path):
    for path in sorted(folder.rglob("*")):
        if path.is_file() and path.suffix.lower() in SUFFIXES:
            yield path


def run(mode: str, folder: Path) -> int:
    found = {}
    for path in files(folder):
        data = path.read_bytes()
        n = count(data)
        if not n:
            continue
        found[path.relative_to(folder).as_posix()] = n
        if mode == "patch":
            path.write_bytes(patched(data))
    for name, n in found.items():
        print(f"{n:4}  {name}")
    verb = "patched" if mode == "patch" else "with a Craft folder compiled in"
    print(f"{len(found)} file(s) {verb}.")
    return 1 if mode == "check" and found else 0


def self_test() -> int:
    """Checks the patching on made-up bytes, the way the recipe relies on."""
    wide = "C:\\Craft\\z".encode("utf-16-le")
    before = b"x\0C:\\CraftRoot\\plugins\0 c:/craft/y \0" + wide + b"ok"
    after = patched(before)
    assert count(before) == 3, count(before)
    assert count(after) == 0, count(after)
    assert len(after) == len(before)
    assert b"\\\\.\\NUL\\Root\\plugins" in after
    assert b"//./NUL//y" in after
    assert "\\\\.\\NUL\\\\z".encode("utf-16-le") in after
    assert after.endswith(b"ok")
    # What isn't Craft's stays as it is.
    other = b"C:\\Windows\\System32 and /usr/lib/craft"
    assert patched(other) == other and count(other) == 0
    print("self-test passed")
    return 0


def main() -> int:
    if sys.argv[1:] == ["self-test"]:
        return self_test()
    if len(sys.argv) != 3 or sys.argv[1] not in {"patch", "check", "report"}:
        print(__doc__)
        return 2
    return run(sys.argv[1], Path(sys.argv[2]))


if __name__ == "__main__":
    sys.exit(main())
