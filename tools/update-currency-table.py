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
# decimal places of a code already in the table.
#
# A code iso-codes no longer lists leaves the table, so no account can be in
# that currency anymore, and moves to KassenSturz's table of withdrawn
# currencies, crates/kassensturz-core/data/withdrawn.txt, with the decimal
# places it had (Q-065, D-104). The importers convert its amounts into the
# currency that replaced it, so that one, the fixed rate and the day it took
# effect are typed in once, from the regulation that fixed them, e.g.
# XYZ=EUR:1.95583:2026-01-01 for "1 EUR = 1.95583 XYZ from 1 January 2026";
# the tool names every code that needs them and changes nothing until each
# has them. The comment at the top of withdrawn.txt is kept as it is: say
# there where the new rate came from.
#
# Record the version, where it came from and its SHA-256 in
# docs/THIRD_PARTY.md. A changed table changes only the decimal places of
# currencies a book starts using afterwards; books keep the ones they stored
# (D-027).

import datetime
import json
import re
import sys
from pathlib import Path

DATA = Path(__file__).resolve().parent.parent / "crates/kassensturz-core/data"
TABLE = DATA / "iso4217.txt"
WITHDRAWN = DATA / "withdrawn.txt"

# The codes ISO 4217 gives no decimal places: precious metals, bond market
# units, the IMF's special drawing right, the Sucre, the ADB unit of account,
# the testing code and "no currency". The table leaves them out, since it
# exists to supply decimal places (D-077).
NO_DECIMAL_PLACES = frozenset(
    "XAG XAU XBA XBB XBC XBD XDR XPD XPT XSU XTS XUA XXX".split()
)

CODE = re.compile(r"[A-Z]{3}")
DIGIT = re.compile(r"[0-9]")
# How many units of a withdrawn currency one unit of its successor was
# worth, as the core reads it: digits with at most one point between them.
RATE = re.compile(r"[0-9]+(\.[0-9]+)?")
DAY = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2}")
# The most digits and decimal places a rate can have (ExchangeRate, D-051).
RATE_DIGITS = 18

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


def read_withdrawn():
    """The comment at the top of the withdrawn table, and each currency in
    it with its decimal places, successor, rate and day."""
    if not WITHDRAWN.is_file():
        fail(f"{WITHDRAWN} is missing; the withdrawn currencies are kept there")
    comment = []
    withdrawn = {}
    for number, line in enumerate(WITHDRAWN.read_text(encoding="utf-8").splitlines(), 1):
        if line.startswith("#") and not withdrawn:
            comment.append(line)
            continue
        fields = line.split(" ")
        if (
            len(fields) != 5
            or not CODE.fullmatch(fields[0])
            or not DIGIT.fullmatch(fields[1])
            or fields[0] in withdrawn
        ):
            fail(
                f"{WITHDRAWN}:{number} isn't a currency line like "
                f"`DEM 2 EUR 1.95583 1999-01-01`: {line!r}"
            )
        code, digits, successor, rate, day = fields
        withdrawn[code] = (int(digits), *check_changeover(code, successor, rate, day))
    return comment, withdrawn


def check_changeover(code, successor, rate, day):
    """`code`'s successor, rate and day, checked as the core checks them."""
    if not CODE.fullmatch(successor) or successor == code:
        fail(f"{code} can't be replaced by {successor!r}: give another currency's code")
    significant = rate.replace(".", "").lstrip("0")
    decimal_places = len(rate.partition(".")[2])
    if (
        not RATE.fullmatch(rate)
        or not significant
        or len(significant) > RATE_DIGITS
        or decimal_places > RATE_DIGITS
    ):
        fail(
            f"{rate!r} isn't a rate for {code}: how many {code} one {successor} was "
            f"worth, above zero, like 1.95583, with at most {RATE_DIGITS} digits"
        )
    try:
        if not DAY.fullmatch(day):
            raise ValueError
        datetime.date.fromisoformat(day)
    except ValueError:
        fail(f"{day!r} isn't a day for {code}'s rate, like 2026-01-01")
    return successor, rate, day


def read_arguments(arguments, codes, table):
    """The decimal places given on the command line, as CODE=DIGITS, and what
    replaced each withdrawn currency, as CODE=SUCCESSOR:RATE:DAY."""
    given = {}
    changeovers = {}
    for argument in arguments:
        code, _, value = argument.partition("=")
        if not CODE.fullmatch(code):
            fail(f"{argument!r} doesn't start with a currency code, like XYZ=2")
        if code in given or code in changeovers:
            fail(f"{code} is given twice")
        if DIGIT.fullmatch(value):
            if code not in codes:
                fail(f"iso-codes doesn't list {code}")
            if code in NO_DECIMAL_PLACES:
                fail(f"ISO 4217 gives {code} no decimal places, so the table leaves it out")
            given[code] = int(value)
        elif value.count(":") == 2:
            if code in codes:
                fail(f"iso-codes still lists {code}, so it isn't withdrawn")
            if code not in table:
                fail(f"the table doesn't list {code}, so it has no decimal places to keep")
            changeovers[code] = check_changeover(code, *value.split(":"))
        else:
            fail(
                f"{argument!r} isn't a code and its decimal places, like XYZ=2, nor a "
                "withdrawn code and what replaced it, like XYZ=EUR:1.95583:2026-01-01"
            )
    return given, changeovers


def main():
    if len(sys.argv) < 3:
        fail(
            "usage: python3 tools/update-currency-table.py VERSION iso_4217.json "
            "[CODE=DIGITS ...] [CODE=SUCCESSOR:RATE:DAY ...]"
        )
    version, path, arguments = sys.argv[1], sys.argv[2], sys.argv[3:]
    if not re.fullmatch(r"[0-9]+(\.[0-9]+)*", version):
        fail(f"{version!r} isn't a version of iso-codes, like 4.20.1")
    codes = read_codes(path)
    old = read_table()
    comment, old_withdrawn = read_withdrawn()
    given, changeovers = read_arguments(arguments, codes, old)

    back = sorted(codes & old_withdrawn.keys())
    if back:
        fail(
            "iso-codes lists codes that withdrawn.txt has as withdrawn; "
            f"check them and take them out of it by hand: {' '.join(back)}"
        )

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

    # A currency that leaves the table is withdrawn, with the decimal places
    # it had, once what replaced it is given.
    leaving = sorted(old.keys() - codes - NO_DECIMAL_PLACES)
    missing = [code for code in leaving if code not in changeovers]
    if missing:
        fail(
            "iso-codes no longer lists some codes; give the currency that replaced "
            "each one, the fixed rate (how many units of it one unit of the new one "
            "was worth) and the day it took effect, from the regulation that fixed "
            f"them, e.g. {missing[0]}=EUR:1.95583:2026-01-01: {' '.join(missing)}"
        )
    withdrawn = dict(old_withdrawn)
    for code in leaving:
        withdrawn[code] = (old[code], *changeovers[code])
    for code, (_, successor, _, _) in sorted(withdrawn.items()):
        if successor in withdrawn:
            # The importers convert in one step (D-104).
            fail(
                f"{successor}, which replaced {code}, is withdrawn too; KassenSturz "
                "can't convert through two changeovers yet (Q-068)"
            )
        if successor not in new:
            fail(f"{successor}, which replaced {code}, isn't in the table of current currencies")

    left_out = sorted(codes & NO_DECIMAL_PLACES)
    with TABLE.open("w", encoding="utf-8", newline="\n") as table:
        table.write(HEADER.format(version=version, left_out=" ".join(left_out) or "none"))
        for code, digits in new.items():
            table.write(f"{code} {digits}\n")
    if withdrawn != old_withdrawn:
        with WITHDRAWN.open("w", encoding="utf-8", newline="\n") as table:
            table.write("".join(f"{line}\n" for line in comment))
            for code, (digits, successor, rate, day) in sorted(withdrawn.items()):
                table.write(f"{code} {digits} {successor} {rate} {day}\n")

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
            _, successor, rate, day = withdrawn[code]
            print(
                f"  withdrew {code}, which iso-codes no longer lists, into {WITHDRAWN.name}: "
                f"1 {successor} = {rate} {code} from {day}"
            )


if __name__ == "__main__":
    main()
