#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Lists the files of the unpacked Windows package that have the folder of
# KDE Craft's build compiled in (B-293). A library with such a path could look
# for a file in a folder any user can create. The app points the known ones at
# its install folder (install_paths.rs); this report shows which others exist,
# so a new one is noticed. It reports and never fails: a path in a library
# that is never loaded is harmless.
#
#   python tools/windows-compiled-in-paths.py <unpacked package folder>

import sys
from pathlib import Path

# Craft's roots, as ASCII and as UTF-16 (Windows' wide strings), in both
# spellings of the separator and both cases of the drive letter.
NEEDLES = []
for root in ("C:\\CraftRoot", "C:/CraftRoot", "C:\\Craft", "C:/Craft"):
    for letter in (root, root.replace("C:", "c:")):
        NEEDLES.append(letter.encode("ascii"))
        NEEDLES.append(letter.encode("utf-16-le"))


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    folder = Path(sys.argv[1])
    found = {}
    for path in sorted(folder.rglob("*")):
        if not path.is_file() or path.suffix.lower() not in {".exe", ".dll", ".pyd", ".cnf", ".xml"}:
            continue
        data = path.read_bytes()
        count = sum(data.count(needle) for needle in NEEDLES)
        if count:
            found[path.relative_to(folder).as_posix()] = count
    for name, count in found.items():
        print(f"{count:4}  {name}")
    print(f"{len(found)} file(s) with a Craft folder compiled in.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
