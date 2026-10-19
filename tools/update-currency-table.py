#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Writes KassenSturz's own ISO 4217 table (D-062),
# crates/kassensturz-core/data/iso4217.txt, from ISO 4217 list one: the
# "Current currency & funds code list" that SIX, the ISO 4217 maintenance
# agency, publishes as XML at
#
#   https://www.six-group.com/dam/download/financial-information/data-center/iso-currrency/lists/list-one.xml
#
# Download the list, then run, with Python's standard library only:
#
#   python3 tools/update-currency-table.py list-one.xml
#
# and record where the list came from, its date and its SHA-256 in
# docs/THIRD_PARTY.md. A changed table changes only the decimal places of
# currencies a book starts using afterwards; books keep the ones they stored
# (D-027).

import sys
import xml.etree.ElementTree as ET
from pathlib import Path

TABLE = Path(__file__).resolve().parent.parent / "crates/kassensturz-core/data/iso4217.txt"

# REUSE-IgnoreStart
HEADER = """\
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: CC0-1.0
#
# KassenSturz's ISO 4217 table (D-062): the current currency and fund codes
# and how many decimal places each one has, sorted by code. A book takes a
# currency's decimal places from here when it first uses the currency, and
# keeps them from then on (D-027).
#
# Made from ISO 4217 list one, published {published} by SIX, the ISO 4217
# maintenance agency (docs/THIRD_PARTY.md). Left out, because the list gives
# them no decimal places: {left_out}.
#
# Written by tools/update-currency-table.py; don't edit it by hand. Each line
# below is a currency code and its number of decimal places.
"""
# REUSE-IgnoreEnd


def fail(message):
    sys.exit(f"update-currency-table: {message}")


def read_list(path):
    """The list's publication date, and each code with its decimal places
    (a digit, or None where the list says "N.A.")."""
    root = ET.parse(path).getroot()
    if root.tag != "ISO_4217" or not root.get("Pblshd"):
        fail(f"{path} isn't ISO 4217 list one: no <ISO_4217 Pblshd=...> element")
    currencies = {}
    for entry in root.iter("CcyNtry"):
        code = entry.findtext("Ccy")
        if code is None:
            # Places without a currency of their own, e.g. Antarctica.
            continue
        if len(code) != 3 or not code.isascii() or not code.isupper():
            fail(f"currency code {code!r} isn't three capital letters")
        units = (entry.findtext("CcyMnrUnts") or "").strip()
        if units == "N.A.":
            digits = None
        elif len(units) == 1 and units.isascii() and units.isdigit():
            digits = int(units)
        else:
            fail(f"{code} has decimal places {units!r}, expected one digit or N.A.")
        # A currency appears once for every country that uses it.
        if currencies.setdefault(code, digits) != digits:
            fail(f"{code} appears with different decimal places")
    if not currencies:
        fail(f"{path} lists no currencies")
    return root.get("Pblshd"), currencies


def main():
    if len(sys.argv) != 2:
        fail("usage: python3 tools/update-currency-table.py list-one.xml")
    published, currencies = read_list(sys.argv[1])
    listed = sorted(code for code, digits in currencies.items() if digits is not None)
    left_out = sorted(code for code, digits in currencies.items() if digits is None)
    with TABLE.open("w", encoding="utf-8", newline="\n") as table:
        table.write(HEADER.format(published=published, left_out=" ".join(left_out) or "none"))
        for code in listed:
            table.write(f"{code} {currencies[code]}\n")
    print(f"{TABLE}: {len(listed)} currencies from the list published {published}")


if __name__ == "__main__":
    main()
