#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Three-way merge of one gettext .po file by message, not by line.

    merge-po.py BASE OURS THEIRS OUTPUT

BASE is the common ancestor, THEIRS the main branch, OURS the pull request.
The result starts from THEIRS (so everything main has stays) and re-applies
what OURS added or changed against BASE:

* a message that is new in OURS is added;
* a translation that OURS changed is taken over, unless THEIRS changed the
  same message too; then THEIRS wins and the message is reported.

Input files must be written with `msgcat --no-wrap` (the wrapper script does
that), so every field is one line. Source references and obsolete entries are
not handled here; tools/update-translations.sh regenerates them afterwards.
Prints "added N, changed N, kept-from-main N" and the kept keys.
"""

import sys


def parse(path):
    """Return (header_block, {key: block}) with blocks as lists of lines."""
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    blocks = [b.split("\n") for b in text.strip("\n").split("\n\n")]
    header, entries = blocks[0], {}
    for lines in blocks[1:]:
        if any(line.startswith("#~") for line in lines):
            continue
        ctxt = next((x for x in lines if x.startswith("msgctxt ")), "")
        mid = next(x for x in lines if x.startswith("msgid "))
        plural = next((x for x in lines if x.startswith("msgid_plural ")), "")
        entries[(ctxt, mid, plural)] = lines
    return header, entries


def strs(lines):
    return [x for x in lines if x.startswith("msgstr")]


def main():
    base_p, ours_p, theirs_p, out_p = sys.argv[1:5]
    _, base = parse(base_p)
    _, ours = parse(ours_p)
    theirs_header, theirs = parse(theirs_p)

    added = changed = 0
    kept = []
    for key, block in ours.items():
        if not any(s.split(" ", 1)[1] != '""' for s in strs(block) if " " in s):
            continue  # untranslated in OURS: nothing to carry over
        if key not in base:
            if key not in theirs:
                theirs[key] = block
                added += 1
            continue
        if strs(block) == strs(base[key]):
            continue  # OURS did not touch this translation
        if key in theirs and strs(theirs[key]) != strs(base[key]):
            if strs(theirs[key]) != strs(block):
                kept.append(key[1])
            continue
        if key in theirs:
            old = theirs[key]
            theirs[key] = [x for x in old if not x.startswith("msgstr")] + strs(block)
            changed += 1
        else:
            theirs[key] = block
            added += 1

    with open(out_p, "w", encoding="utf-8") as fh:
        fh.write("\n".join(theirs_header) + "\n\n")
        fh.write("\n\n".join("\n".join(b) for b in theirs.values()) + "\n")
    print(f"added {added}, changed {changed}, kept-from-main {len(kept)}")
    for k in kept:
        print(f"  kept main's translation: {k}")


main()
