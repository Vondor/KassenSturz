#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Updates KassenSturz's own ISO 4217 table (D-062, D-077),
# crates/kassensturz-core/data/iso4217.txt. Which currencies exist comes from
# Debian's iso-codes (https://salsa.debian.org/iso-codes-team/iso-codes),
# whose data/iso_4217.json lists the current ISO 4217 codes. It has no
# decimal places, so the table keeps its own: the tool takes them from the
# table as it is.
#
# Download a release of iso-codes, then run, with Python's standard library
# only, giving the release's version and its JSON file:
#
#   python3 tools/update-currency-table.py 4.20.1 iso-codes-4.20.1/data/iso_4217.json
#
# A code iso-codes adds needs its number of decimal places from ISO 4217,
# typed in once after the file, e.g. XYZ=2; the tool names every code that
# needs one and changes nothing until each has it. The same form changes the
# decimal places of a code already in the table. A code iso-codes no longer
# lists leaves the table, so a book can't start using that currency anymore;
# the importers convert it into the currency that replaced it (D-077).
#
# Record the version, where it came from and its SHA-256 in
# docs/THIRD_PARTY.md. A changed table changes only the decimal places of
# currencies a book starts using afterwards; books keep the ones they stored
# (D-027).

import json
import re
import sys
from pathlib import Path

TABLE = Path(__file__).resolve().parent.parent / "crates/kassensturz-core/data/iso4217.txt"

# The codes ISO 4217 gives no decimal places: precious metals, bond market
# units, the IMF's special drawing right, the Sucre, the ADB unit of account,
# the testing code and "no currency". The table leaves them out, since it
# exists to supply decimal places (D-077).
NO_DECIMAL_PLACES = frozenset(
    "XAG XAU XBA XBB XBC XBD XDR XPD XPT XSU XTS XUA XXX".split()
)

CODE = re.compile(r"[A-Z]{3}")
DIGIT = re.compile(r"[0-9]")

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
# The codes are those Debian's iso-codes {version} lists (docs/THIRD_PARTY.md).
# It has no decimal places, so they are kept here, as ISO 4217 gives them.
# Left out, because ISO 4217 gives them no decimal places:
# {left_out}.
#
# Written by tools/update-currency-table.py; don't edit it by hand. Each line
# below is a currency code and its number of decimal places.
"""
# REUSE-IgnoreEnd


def fail(message):
    sys.exit(f"update-currency-table: {message}")


def read_codes(path):
    """The codes iso-codes lists in its iso_4217.json."""
    try:
        with open(path, encoding="utf-8") as file:
            data = json.load(file)
    except (OSError, ValueError) as error:
        fail(f"can't read {path}: {error}")
    entries = data.get("4217") if isinstance(data, dict) else None
    if not isinstance(entries, list) or not entries:
        fail(f'{path} isn\'t iso-codes\' iso_4217.json: no list under "4217"')
    codes = set()
    for entry in entries:
        code = entry.get("alpha_3") if isinstance(entry, dict) else None
        if not isinstance(code, str) or not CODE.fullmatch(code):
            fail(f"{path} has an entry whose code isn't three capital letters: {entry!r}")
        if code in codes:
            fail(f"{path} lists {code} twice")
        codes.add(code)
    return codes


def read_table():
    """Each currency in the table now, with its decimal places."""
    if not TABLE.is_file():
        fail(f"{TABLE} is missing; the decimal places are kept there")
    table = {}
    for number, line in enumerate(TABLE.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        code, _, digits = line.partition(" ")
        if not CODE.fullmatch(code) or not DIGIT.fullmatch(digits) or code in table:
            fail(f"{TABLE}:{number} isn't a currency line like `EUR 2`: {line!r}")
        table[code] = int(digits)
    return table


def read_digits(arguments, codes):
    """The decimal places given on the command line, as CODE=DIGITS."""
    given = {}
    for argument in arguments:
        code, _, digits = argument.partition("=")
        if not CODE.fullmatch(code) or not DIGIT.fullmatch(digits):
            fail(f"{argument!r} isn't a code and its decimal places, like XYZ=2")
        if code not in codes:
            fail(f"iso-codes doesn't list {code}")
        if code in NO_DECIMAL_PLACES:
            fail(f"ISO 4217 gives {code} no decimal places, so the table leaves it out")
        if code in given:
            fail(f"{code} is given twice")
        given[code] = int(digits)
    return given


def main():
    if len(sys.argv) < 3:
        fail(
            "usage: python3 tools/update-currency-table.py VERSION iso_4217.json [CODE=DIGITS ...]"
        )
    version, path, arguments = sys.argv[1], sys.argv[2], sys.argv[3:]
    if not re.fullmatch(r"[0-9]+(\.[0-9]+)*", version):
        fail(f"{version!r} isn't a version of iso-codes, like 4.20.1")
    codes = read_codes(path)
    old = read_table()
    given = read_digits(arguments, codes)

    new = {}
    missing = []
    for code in sorted(codes - NO_DECIMAL_PLACES):
        if code in given:
            new[code] = given[code]
        elif code in old:
            new[code] = old[code]
        else:
            missing.append(code)
    if missing:
        fail(
            "iso-codes lists new codes; give each one's decimal places from ISO 4217, "
            f"e.g. {missing[0]}=2, or add it to NO_DECIMAL_PLACES if ISO 4217 gives it "
            f"none: {' '.join(missing)}"
        )

    left_out = sorted(codes & NO_DECIMAL_PLACES)
    with TABLE.open("w", encoding="utf-8", newline="\n") as table:
        table.write(HEADER.format(version=version, left_out=" ".join(left_out) or "none"))
        for code, digits in new.items():
            table.write(f"{code} {digits}\n")

    print(f"{TABLE}: {len(new)} currencies from iso-codes {version}")
    for code in sorted(new.keys() - old.keys()):
        print(f"  added {code} with {new[code]} decimal places")
    for code in sorted(old.keys() & new.keys()):
        if old[code] != new[code]:
            print(f"  changed {code} from {old[code]} to {new[code]} decimal places")
    for code in sorted(old.keys() - new.keys()):
        if code in NO_DECIMAL_PLACES:
            print(f"  removed {code}, which ISO 4217 gives no decimal places")
        else:
            print(f"  removed {code}, which iso-codes no longer lists")


if __name__ == "__main__":
    main()
