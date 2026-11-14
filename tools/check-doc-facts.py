#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fails when the documentation names a book layout the code no longer has.

An outside review (2026-11-14) found the README still naming layout 16 and
docs/BOOK_FORMAT.md saying "currently 25" while the code was at layout 33.
So the documentation names the current layout in one place only, the status
line of docs/BOOK_FORMAT.md, and this check keeps that line and the code in
step:

  1. The current layout is worked out from
     crates/kassensturz-store/src/layout.rs the way the code does it:
     FIRST_VERSION plus the number of MIGRATIONS, minus one.
  2. The status line "**Status: layout N**" of docs/BOOK_FORMAT.md must name
     that number.
  3. Every other Markdown file that claims a *current* layout ("currently
     layout N", "the current layout is N", "user_version ... currently N")
     must name that number too, except docs/DECISIONS.md, whose rows record
     what was true when they were written. Layout numbers in history ("from layout 30",
     "layout 31 adds") are fine and not checked.
  4. The README, SECURITY.md and the user guides under docs/user/ name no
     layout number of KassenSturz at all; they point to docs/BOOK_FORMAT.md.

Needs nothing but Python. Run it from the repository's top folder:

  python3 tools/check-doc-facts.py
"""

import os
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LAYOUT_RS = ROOT / "crates/kassensturz-store/src/layout.rs"
BOOK_FORMAT = ROOT / "docs/BOOK_FORMAT.md"
# Files for everyday readers: they point to docs/BOOK_FORMAT.md instead of
# naming a layout, because a number there goes stale with every migration.
NO_NUMBER = ["README.md", "SECURITY.md", "docs/user/**/*.md"]
# The decision log records what was true when each row was written.
HISTORY = {"docs/DECISIONS.md"}
# "KMyMoney ... layouts 12 to 15" is KMyMoney's own format, not ours, and
# plural "layouts" is always history, so only the singular is looked for.
LAYOUT_WORD = re.compile(r"\blayout\s+`?(\d+)`?", re.IGNORECASE)
CURRENT_CLAIM = re.compile(
    r"(?:current(?:ly)?|now|today)\b[^.\n|]{0,40}?\blayout(?:\s+version)?\b[^.\n|0-9]{0,30}?`?(\d+)`?"
    r"|\blayout(?:\s+version)?\b[^.\n|0-9]{0,30}?\bcurrently\s+`?(\d+)`?",
    re.IGNORECASE,
)


def current_layout() -> int:
    text = LAYOUT_RS.read_text(encoding="utf-8")
    first = re.search(r"pub const FIRST_VERSION: i32 = (\d+);", text)
    count = re.search(r"pub\(crate\) const MIGRATIONS: \[&str; (\d+)\]", text)
    formula = "pub const VERSION: i32 = FIRST_VERSION + MIGRATIONS.len() as i32 - 1;"
    if not first or not count or formula not in text:
        sys.exit(
            f"{LAYOUT_RS.relative_to(ROOT)}: FIRST_VERSION, MIGRATIONS or VERSION "
            "changed shape; update tools/check-doc-facts.py to read them again."
        )
    return int(first.group(1)) + int(count.group(1)) - 1


def main() -> int:
    version = current_layout()
    problems = []

    status = re.search(r"\*\*Status: layout (\d+)\*\*", BOOK_FORMAT.read_text(encoding="utf-8"))
    if not status:
        problems.append("docs/BOOK_FORMAT.md: the line '**Status: layout N**' is missing")
    elif int(status.group(1)) != version:
        problems.append(
            f"docs/BOOK_FORMAT.md: the status line names layout {status.group(1)}, "
            f"the code is at layout {version}"
        )

    no_number = set()
    for pattern in NO_NUMBER:
        no_number.update(ROOT.glob(pattern))

    for path in sorted(ROOT.glob("**/*.md")):
        rel = path.relative_to(ROOT)
        if rel.parts[0] in {"target", "build", ".git"} or "node_modules" in rel.parts:
            continue
        if rel.as_posix() in HISTORY:
            continue
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for match in CURRENT_CLAIM.finditer(line):
                number = int(match.group(1) or match.group(2))
                if number != version:
                    problems.append(
                        f"{rel}:{lineno}: says the current layout is {number}, "
                        f"the code is at layout {version}"
                    )
            if path in no_number:
                for match in LAYOUT_WORD.finditer(line):
                    problems.append(
                        f"{rel}:{lineno}: names layout {match.group(1)}; point to "
                        "docs/BOOK_FORMAT.md instead, so it can't go stale"
                    )

    for problem in problems:
        print(f"::error::{problem}" if "GITHUB_ACTIONS" in os.environ else problem)
    if problems:
        return 1
    print(f"Documentation agrees with the code: layout {version}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
