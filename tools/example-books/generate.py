#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
"""Makes the example books and their bank files (M7a.8, D-561).

Writes three made-up German books as KMyMoney SQLite files (layout 15), which
KassenSturz imports like any other KMyMoney book, and for each of the four
example books (these three and the older Space Colony book) one CSV and one
CAMT.053 statement:

* Suburban Family  (`suburban-family`): the everyday German household, the
  suggested start. Girokonto, Sparkonto, Kreditkarte and Bargeld, the default
  German categories, two years and monthly Umbuchungen.
* Flower Utopia    (`flower-utopia`): a small garden cooperative with a shared
  cash box. Its budgets and charts are added by the pipeline test
  `example_books`, which writes `flower-utopia.json`.
* UdSSR Government Agent (`udssr-agent`): a fictional agent's book with rubles,
  dollars, pounds and francs. The agency is made up. Only codes that are valid
  today are used (RUB, USD, GBP, CHF), never a withdrawn one such as SUR, DEM
  or FRF (D-077). The dates are recent; the 1980s are in the names and prices.
* Space Colony: the book already in `tests/books/`; only its statements are
  made here. It is read, never changed.

Every book has a food category and at least one Transfer. Every statement
holds one line that is already in the book (the CAMT line has the same bank
reference, the CSV line the same date and amount) and new lines in the
reference month, so the import check has something to find (D-030, D-092).
The answer sheets are NOT written here: the pipeline test `example_books`
computes them from the real import and report code.

Usage:

    tools/example-books/generate.py                          # reference month 2026-11
    tools/example-books/generate.py --reference-month 2027-01
    tools/example-books/generate.py --check                  # fail if files differ

The books hold the 24 months before the reference month, so "last month"
and the last twelve months are never empty, plus some cash and card spending
in the reference month (never on the statement's account). The app moves the
dates by whole months when it makes a book, so the reference month is the
running one, and leaves out what lies after today (D-584). The reference month is saved in `reference-month.txt`; the
test reads it. Python 3.9 or newer, standard library only. Every random
choice comes from a small generator seeded from the book's name, so the same
call always writes the same files. Money is whole minor units (cents,
pence, kopecks), never a floating point number (AGENTS.md section 3.1 rule 1).

Sources for the amounts (public figures consulted for plausibility, no data
copied; the amounts here are made up inside these ranges):

* Suburban Family: the Federal Statistical Office of Germany (Destatis),
  "Einkommens- und Verbrauchsstichprobe" and "Konsumausgaben privater
  Haushalte" (about 14 percent of a household's spending goes to food and
  drink; a family of four spends roughly 600 to 800 euros a month on it);
  rents and ancillary costs from Destatis' housing statistics and the
  Mikrozensus; the Deutschlandticket price published by the transport
  ministries (63 euros a month in 2026); child benefit (255 euros per child
  and month, Familienkasse).
* Flower Utopia: typical allotment and garden club costs from the
  Bundesverband Deutscher Gartenfreunde (leases of 0.10 to 0.50 euros per
  square metre and year, water, insurance, members' fees of a few euros a
  month).
* UdSSR agent: the Soviet averages of the 1980s as published in the
  statistical yearbook "Narodnoe khozyaystvo SSSR" (monthly pay of about
  200 rubles, rents of a few rubles, bread at 0.20 rubles) and the official
  exchange rates of the State Bank. The agency's pay, the per-diems and the
  agency rates between the currencies are invented.
* Space Colony: the book is not changed here. Public figures for the real
  analogues were looked up through a web search on 2026-11-02; only the
  search-result text was read, because the pages themselves could not be
  opened from the build session. What it showed: launching cargo to the ISS
  costs tens of thousands of US dollars per kilogram (NASA Technical Reports
  Server, document 20180007067: Space Shuttle about 54,500 USD/kg to low Earth
  orbit, Falcon 9 list price about 2,720 USD/kg); the Mars-500 isolation study
  (6 crew, 520 days) was reported to cost about 15 million USD in total
  (space.com, 2008 report); HI-SEAS crews were paid 25 USD per day plus a
  bonus (University of Hawaii at Manoa news, "Six-member crew selected for
  Mars food mission"). None of these gives household prices, so they only
  bound what the book must not pretend: they are costs of launching or
  running a mission, not what a colonist pays in a shop. The book's amounts
  (pay, rent, food, tax) stay in everyday euro ranges and were not changed.
  Food and household prices of the German books come from the Destatis
  figures above.
"""

import argparse
import calendar
import os
import shutil
import sqlite3
import sys
import tempfile
from datetime import date, timedelta

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
TESTS = os.path.join(ROOT, "crates/kassensturz-import/tests")
SEED = os.path.join(TESTS, "golden/kmy-sqlite/space-colony-v15.sqlite")
SAMPLE = os.path.join(TESTS, "golden/kmy-sqlite/kassensturz-sample.sqlite")
SPACE_COLONY = os.path.join(TESTS, "books/space-colony-5y-v15.sqlite")
OUT = os.path.join(TESTS, "books/examples")
MASK = (1 << 64) - 1

# Example IBANs of the SWIFT IBAN Registry only (D-055).
IBAN_DE = "DE89370400440532013000"
IBAN_AT = "AT611904300234573201"
IBAN_CH = "CH9300762011623852957"
IBAN_FR = "FR1420041010050500013M02606"
IBAN_NL = "NL91ABNA0417164300"
IBAN_GB = "GB29NWBK60161331926819"
OTHER_IBANS = [IBAN_AT, IBAN_CH, IBAN_FR, IBAN_NL, IBAN_GB]


# --------------------------------------------------------------- randomness


class Rng:
    """SplitMix64: the same numbers on every machine and Python version."""

    def __init__(self, seed):
        self.state = seed & MASK

    def next(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def below(self, n):
        return self.next() % n

    def between(self, low, high):
        return low + self.below(high - low + 1)

    def pick(self, items):
        return items[self.below(len(items))]


def stream(slug, name):
    """One generator per book and kind of event (FNV-1a of its name)."""
    h = 0xCBF29CE484222325
    for byte in f"example-books/{slug}/{name}".encode():
        h = ((h ^ byte) * 0x100000001B3) & MASK
    return Rng(h)


# ------------------------------------------------------------------ helpers


def day(year, month, number):
    return date(year, month, min(number, calendar.monthrange(year, month)[1]))


def workday(when):
    """Banks book on working days: a weekend day moves to the Friday before."""
    return when - timedelta(days=max(0, when.weekday() - 4))


def next_workday(when):
    return when + timedelta(days=(7 - when.weekday()) if when.weekday() > 4 else 0)


def german(minor):
    sign = "-" if minor < 0 else ""
    minor = abs(minor)
    return f"{sign}{minor // 100},{minor % 100:02d}"


def plain(minor):
    """2024-style amount text with a point, as CAMT.053 writes it."""
    sign = "-" if minor < 0 else ""
    minor = abs(minor)
    return f"{sign}{minor // 100}.{minor % 100:02d}"


def iso(when):
    return when.isoformat()


# How many months before the reference month each made book holds: about
# two years, so year-on-year charts and the last twelve months are full.
MONTHS = 24


def earlier_months(reference, count=MONTHS):
    """The `count` months before the reference month, oldest first."""
    year, month = reference
    out = []
    for _ in range(count):
        year, month = (year - 1, 12) if month == 1 else (year, month - 1)
        out.append((year, month))
    return list(reversed(out))


def price_text(value, shares):
    ratio = (shares * 10000 + value // 2) // value
    return f"{ratio // 10000}.{ratio % 10000:04d}"


# Account kinds as KMyMoney numbers them, with the German texts it writes.
CHECKING, SAVINGS, CASH, CREDIT, ASSET, LIABILITY, INCOME, EXPENSE, EQUITY = (
    1, 2, 3, 4, 9, 10, 12, 13, 16)
KIND_TEXT = {
    CHECKING: "Girokonto", SAVINGS: "Sparkonto", CASH: "Bargeld",
    CREDIT: "Kreditkarte", ASSET: "Anlagen", LIABILITY: "Verbindlichkeiten",
    INCOME: "Einnahme", EXPENSE: "Ausgabe", EQUITY: "Eigenkapital",
}
ROOTS = {ASSET: "AStd::Asset", LIABILITY: "AStd::Liability", EXPENSE: "AStd::Expense",
         INCOME: "AStd::Income", EQUITY: "AStd::Equity"}


class Book:
    """A made-up book being collected, written as a KMyMoney SQLite file."""

    def __init__(self, slug, title, currency, owner, months, institution):
        self.slug, self.title, self.currency, self.owner = slug, title, currency, owner
        self.months = months
        self.first = date(months[0][0], months[0][1], 1)
        last_year, last_month = months[-1]
        self.last = day(last_year, last_month, 31)
        self.institution = institution
        self.accounts = {}   # name -> dict(id, kind, parent, currency, iban, number)
        self.order = []
        self.payees = {}     # name -> category name (or None)
        self.tx = []
        self.rates = []      # (from, to, date, numerator over 10000)
        self.bank_counter = 0
        self.bank_accounts = set()  # accounts whose lines carry a bank reference

    # --- structure
    def account(self, name, kind, parent=None, currency=None, iban=None, bank=False):
        ident = f"A{len(self.order) + 2:06d}"
        root = {CHECKING: ASSET, SAVINGS: ASSET, CASH: ASSET, CREDIT: LIABILITY}.get(kind, kind)
        parent_id = ROOTS[root] if parent is None else self.accounts[parent]["id"]
        self.accounts[name] = dict(id=ident, kind=kind, parent=parent_id,
                                   currency=currency or self.currency, iban=iban,
                                   institution=bool(bank))
        self.order.append(name)
        if bank:
            self.bank_accounts.add(name)

    def tree(self, spec, kind, parent=None):
        """spec: list of names or (name, [children])."""
        for item in spec:
            if isinstance(item, tuple):
                self.account(item[0], kind, parent)
                self.tree(item[1], kind, item[0])
            else:
                self.account(item, kind, parent)

    def payee(self, name, category=None):
        self.payees[name] = category

    # --- transactions
    def add(self, when, parts, memo, payee=None, imported_ref=None):
        """parts: (account, value, shares or None). Values are in the book's
        currency, shares in the account's own."""
        full = []
        for part in parts:
            shares = part[2] if len(part) > 2 and part[2] is not None else part[1]
            full.append((part[0], part[1], shares))
        assert sum(v for _, v, _ in full) == 0, (memo, full)
        self.tx.append(dict(date=when, parts=full, memo=memo, payee=payee, seq=len(self.tx)))

    def spend(self, when, source, category, cents, memo, payee=None):
        self.add(when, [(category, cents), (source, -cents)], memo, payee)

    def earn(self, when, target, category, cents, memo, payee=None):
        self.add(when, [(category, -cents), (target, cents)], memo, payee)

    def move(self, when, source, target, cents, memo):
        self.add(when, [(source, -cents), (target, cents)], memo, None)

    # --- derived
    def finish(self):
        """Sort, give the bank accounts their references, check no cash
        account runs dry."""
        self.tx.sort(key=lambda t: (t["date"], t["seq"]))
        counters = {}
        for tx in self.tx:
            tx["refs"] = {}
            for name, _, _ in tx["parts"]:
                if name in self.bank_accounts:
                    counters[name] = counters.get(name, 0) + 1
                    tx["refs"][name] = f"{self.slug[:2].upper()}{self.last.strftime('%y%m')}-{counters[name]:04d}"

    def totals(self):
        totals, low = {}, {}
        for tx in self.tx:
            for name, _, shares in tx["parts"]:
                totals[name] = totals.get(name, 0) + shares
                low[name] = min(low.get(name, 0), totals[name])
        return totals, low

    def balance_through(self, name, last_day):
        return sum(shares for tx in self.tx if tx["date"] <= last_day
                   for part, _, shares in tx["parts"] if part == name)


def opening(book, base, equity="Anfangsbestand", floor=15000):
    """Opening balances on the first day: enough in each own account that
    none runs dry. base: account -> cents (negative: owed)."""
    run, low = {n: 0 for n in base}, {n: 0 for n in base}
    for tx in book.tx:
        for name, _, shares in tx["parts"]:
            if name in run:
                run[name] += shares
                low[name] = min(low[name], run[name])
    parts = []
    for name, cents in base.items():
        if cents >= 0 and book.accounts[name]["kind"] != CREDIT:
            cents = max(cents, floor - low[name])
        if book.accounts[name]["currency"] != book.currency:
            raise AssertionError("opening balances are in the book's currency")
        parts.append((name, cents))
    parts.append((equity, -sum(c for _, c in parts)))
    book.add(book.first, parts, "Anfangsbestand", None)
    book.tx[-1]["seq"] = -1


# ----------------------------------------------------------------- writing


def schema_of(path):
    db = sqlite3.connect(path)
    try:
        return [sql for (sql,) in db.execute(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND sql IS NOT NULL")]
    finally:
        db.close()


def write_book(book, path):
    work = tempfile.mkdtemp(prefix="example-book-")
    tmp = os.path.join(work, "book.sqlite")
    db = sqlite3.connect(tmp)
    db.execute("PRAGMA journal_mode = OFF")
    for sql in schema_of(SEED):
        db.execute(sql)
    sample = sqlite3.connect(SAMPLE)
    codes = sorted({a["currency"] for a in book.accounts.values()} | {book.currency})
    for code in codes:
        row = sample.execute("SELECT * FROM kmmCurrencies WHERE ISOcode = ?", (code,)).fetchone()
        assert row, code
        db.execute("INSERT INTO kmmCurrencies VALUES (?,?,?,?,?,?,?,?,?,?,?)", row)
    seed_info = sqlite3.connect(SEED).execute("SELECT * FROM kmmFileInfo").fetchone()
    sample.close()

    institution = "I000001"
    db.execute("INSERT INTO kmmInstitutions VALUES (?,?,?,?,?,?,?,?)",
               (institution, book.institution, "", "", "", "", "", ""))

    ids = {}
    for kind, ident in ROOTS.items():
        db.execute("INSERT INTO kmmAccounts VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                   (ident, "", "", "", "", "", "", str(kind), KIND_TEXT[kind], "N",
                    KIND_TEXT[kind], "", book.currency, "0/1", "0", 0))
    totals, _ = book.totals()
    counts = {}
    for tx in book.tx:
        for name, _, _ in tx["parts"]:
            counts[name] = counts.get(name, 0) + 1
    for name in book.order:
        a = book.accounts[name]
        ids[name] = a["id"]
        total = totals.get(name, 0)
        opening_date = iso(book.first) if a["kind"] not in (INCOME, EXPENSE) else "1900-01-01"
        db.execute("INSERT INTO kmmAccounts VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                   (a["id"], institution if a["institution"] else "", a["parent"], "",
                    iso(book.last) if counts.get(name) else "", opening_date, "",
                    str(a["kind"]), KIND_TEXT[a["kind"]], "N", name, "", a["currency"],
                    f"{total}/100", german(total), counts.get(name, 0)))
        if a["iban"]:
            db.execute("INSERT INTO kmmKeyValuePairs VALUES ('ACCOUNT', ?, 'iban', ?)",
                       (a["id"], a["iban"]))
    db.execute("INSERT INTO kmmKeyValuePairs VALUES ('ACCOUNT', ?, 'OpeningBalanceAccount', 'Yes')",
               (ids["Anfangsbestand"],))

    for from_code, to_code, when, numerator in book.rates:
        db.execute("INSERT INTO kmmPrices VALUES (?,?,?,?,?,?)",
                   (from_code, to_code, iso(when), f"{numerator}/10000",
                    f"{numerator // 10000},{numerator % 10000:04d}", "User"))

    # Payees: the owner first, then the made-up ones with a default category.
    db.execute("INSERT INTO kmmPayees VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
               ("USER", book.owner, "", "", "Musterstraße 1", "Musterstadt", "00000", "XX",
                "", "", "", 0, "Y", "", "", ""))
    payee_id = {}
    for number, (name, category) in enumerate(sorted(book.payees.items()), start=1):
        ident = f"P{number:06d}"
        payee_id[name] = ident
        keys = "^" + name.replace(" ", "\\ ") + "$"
        db.execute("INSERT INTO kmmPayees VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                   (ident, name, "", "", "", "", "", "", "", "",
                    ids[category] if category else "", 2, "Y", keys, "", ""))

    splits = 0
    for number, tx in enumerate(book.tx, start=1):
        ident = f"T{number:018d}"
        when = tx["date"]
        db.execute("INSERT INTO kmmTransactions VALUES (?,?,?,?,?,?,?)",
                   (ident, "N", iso(when), tx["memo"], iso(when), book.currency, ""))
        payee = payee_id[tx["payee"]] if tx["payee"] else ""
        for index, (name, value, shares) in enumerate(tx["parts"]):
            ratio = "1/1" if value == shares or not value else f"{shares}/{value}"
            ratio_text = "1.0000" if value == shares or not value else price_text(abs(value), abs(shares))
            db.execute(
                "INSERT INTO kmmSplits VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (ident, "N", index, payee, "", "", "0",
                 f"{value}/100", german(value), f"{shares}/100", german(shares),
                 ratio, ratio_text, tx["memo"], ids[name], "", "", iso(when),
                 tx["refs"].get(name, "")))
            splits += 1

    prices = len(book.rates)
    db.execute("INSERT INTO kmmFileInfo VALUES (" + ",".join("?" * len(seed_info)) + ")",
               ("15", iso(book.first), iso(book.last), book.currency, 1,
                len(ids) + len(ROOTS), len(payee_id) + 1, 0, len(book.tx), splits, 0,
                prices, len(codes), 0, 0, 0, "", "", 1, len(payee_id) + 1, 1,
                len(ids) + 1, len(book.tx) + 1, 1, 1, 1, "", "N", 0, 1, 1, 1, "",
                iso(book.last) + "T08:00:00", ""))
    for key, value in (("CreationDate", iso(book.first)), ("LastModificationDate", iso(book.last)),
                       ("kmm-baseCurrency", book.currency),
                       ("kmm-id", "{00000000-0000-4000-8000-000000000000}")):
        db.execute("INSERT INTO kmmKeyValuePairs VALUES ('STORAGE', '', ?, ?)", (key, value))
    db.commit()
    db.execute("VACUUM")
    db.close()
    os.makedirs(os.path.dirname(path), exist_ok=True)
    shutil.copyfile(tmp, path)
    shutil.rmtree(work)
    return len(book.tx), splits


# --------------------------------------------------------------- statements


class Line:
    def __init__(self, when, name, iban, memo, cents, ref):
        self.when, self.name, self.iban, self.memo, self.cents, self.ref = (
            when, name, iban, memo, cents, ref)


def csv_text(currency, lines):
    out = ["Buchungstag;Valuta;Name Zahlungsbeteiligter;IBAN Zahlungsbeteiligter;"
           "Verwendungszweck;Betrag;Währung"]
    for line in lines:
        d = line.when.strftime("%d.%m.%Y")
        out.append(";".join([d, d, line.name, line.iban, line.memo, german(line.cents), currency]))
    return "\r\n".join(out) + "\r\n"


CSV_SETTINGS = '''delimiter = ";"
decimal-mark = comma
date-order = day-month-year
booking-date = "Buchungstag"
value-date = "Valuta"
amount = "Betrag"
other-party-name = "Name Zahlungsbeteiligter"
other-party-iban = "IBAN Zahlungsbeteiligter"
remittance-information = "Verwendungszweck"
currency = "Währung"
'''


def xml_escape(text):
    return (text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"))


def camt_text(slug, currency, account_id, account_is_iban, lines, closing, closing_date, stamp):
    opening_cents = closing - sum(line.cents for line in lines)
    first = min(line.when for line in lines)

    def balance(code, cents, when):
        sign = "CRDT" if cents >= 0 else "DBIT"
        return (f"      <Bal>\n        <Tp><CdOrPrtry><Cd>{code}</Cd></CdOrPrtry></Tp>\n"
                f'        <Amt Ccy="{currency}">{plain(abs(cents))}</Amt>\n'
                f"        <CdtDbtInd>{sign}</CdtDbtInd>\n"
                f"        <Dt><Dt>{iso(when)}</Dt></Dt>\n      </Bal>\n")

    acct = (f"<IBAN>{account_id}</IBAN>" if account_is_iban
            else f"<Othr><Id>{account_id}</Id></Othr>")
    out = ['<?xml version="1.0" encoding="UTF-8"?>\n',
           '<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08">\n',
           "  <BkToCstmrStmt>\n    <GrpHdr>\n"
           f"      <MsgId>EXAMPLE-{slug.upper()}-{closing_date.strftime('%Y%m%d')}</MsgId>\n"
           f"      <CreDtTm>{stamp}</CreDtTm>\n    </GrpHdr>\n"
           f"    <Stmt>\n      <Id>{slug}-{closing_date.strftime('%Y-%m')}</Id>\n"
           f"      <Acct>\n        <Id>{acct}</Id>\n        <Ccy>{currency}</Ccy>\n      </Acct>\n"]
    out.append(balance("OPBD", opening_cents, first))
    out.append(balance("CLBD", closing, closing_date))
    for line in lines:
        direction = "CRDT" if line.cents >= 0 else "DBIT"
        party = "Dbtr" if line.cents >= 0 else "Cdtr"
        out.append(
            "      <Ntry>\n"
            f'        <Amt Ccy="{currency}">{plain(abs(line.cents))}</Amt>\n'
            f"        <CdtDbtInd>{direction}</CdtDbtInd>\n"
            "        <Sts><Cd>BOOK</Cd></Sts>\n"
            f"        <BookgDt><Dt>{iso(line.when)}</Dt></BookgDt>\n"
            f"        <ValDt><Dt>{iso(line.when)}</Dt></ValDt>\n"
            f"        <AcctSvcrRef>{line.ref}</AcctSvcrRef>\n"
            "        <NtryDtls>\n          <TxDtls>\n            <RltdPties>\n"
            f"              <{party}><Nm>{xml_escape(line.name)}</Nm></{party}>\n"
            f"              <{party}Acct><Id><IBAN>{line.iban}</IBAN></Id></{party}Acct>\n"
            "            </RltdPties>\n"
            f"            <RmtInf><Ustrd>{xml_escape(line.memo)}</Ustrd></RmtInf>\n"
            "          </TxDtls>\n        </NtryDtls>\n      </Ntry>\n")
    out.append("    </Stmt>\n  </BkToCstmrStmt>\n</Document>\n")
    return "".join(out)


def statement_files(slug, currency, account_id, account_is_iban, existing, new_lines,
                    closing, closing_date, stamp):
    """existing: the Line already in the book; new_lines: Lines in the
    reference month. Returns {file name: text}."""
    lines = [existing] + sorted(new_lines, key=lambda l: l.when)
    return {
        f"{slug}.csv": csv_text(currency, lines),
        f"{slug}.csv.settings": CSV_SETTINGS,
        f"{slug}.camt053.xml": camt_text(slug, currency, account_id, account_is_iban,
                                         lines, closing, closing_date, stamp),
    }


def book_statement(book, name, account_id, account_is_iban, existing_index, new_specs, reference):
    """The statement of the book's bank account `name`. The existing line is
    the account's `existing_index`-th line from the end (1 is the last)."""
    mine = [tx for tx in book.tx if any(p[0] == name for p in tx["parts"])]
    tx = mine[-existing_index]
    shares = next(s for n, _, s in tx["parts"] if n == name)
    other = next((n for n, _, _ in tx["parts"] if n != name), "")
    existing = Line(tx["date"], tx["payee"] or other, OTHER_IBANS[0], tx["memo"], shares,
                    tx["refs"][name])
    ry, rm = reference
    new_lines, closing = [], book.balance_through(name, book.last) + 0
    # Lines after the existing one in the book would also be on the bank's
    # statement; the books are made so that the existing line is the last.
    assert tx is mine[-1] or existing_index > 1
    counter = 0
    rng = stream(book.slug, "statement")
    for number, (offset, who, memo, cents) in enumerate(new_specs, start=1):
        when = next_workday(date(ry, rm, 1) + timedelta(days=offset - 1))
        counter += 1
        new_lines.append(Line(when, who, rng.pick(OTHER_IBANS), memo, cents,
                              f"{book.slug[:2].upper()}{ry % 100:02d}{rm:02d}-N{counter:03d}"))
        closing += cents
    closing_date = max(line.when for line in new_lines)
    return statement_files(book.slug, book.accounts[name]["currency"], account_id,
                           account_is_iban, existing, new_lines, closing, closing_date,
                           f"{iso(closing_date)}T20:00:00+01:00")


# ======================================================== Suburban Family

def suburban_family(reference):
    months = earlier_months(reference)
    ry, rm = reference
    book = Book("suburban-family", "Suburban Family", "EUR", "Familie Sommerfeld", months,
                "Sonnenauer Bank")
    s = lambda name: stream("suburban-family", name)
    G, S, K, B = "Girokonto", "Sparkonto", "Kreditkarte", "Bargeld"
    book.account(G, CHECKING, None, "EUR", IBAN_DE, bank=True)
    book.account(S, SAVINGS, None, "EUR", None, bank=True)
    book.account(B, CASH)
    book.account(K, CREDIT, None, "EUR", None, bank=True)
    book.account("Anfangsbestand", EQUITY)
    book.tree(["Gehalt", "Rente", "Sozialleistungen", "Zinsen", "Erhaltene Geschenke",
               "Erstattungen", "Sonstige Einnahmen"], INCOME)
    book.tree([
        ("Wohnen", ["Miete oder Hauskredit", "Strom und Heizung", "Wasser und Müll",
                    "Internet und Telefon", "Haushaltswaren", "Reparaturen"]),
        ("Essen", ["Lebensmittel", "Auswärts essen"]),
        ("Unterwegs", ["Bus und Bahn", "Auto", "Fahrrad"]),
        ("Gesundheit", ["Arzt und Apotheke", "Krankenversicherung"]),
        "Versicherungen", "Kleidung",
        ("Kinder", ["Schule und Kinderbetreuung"]), "Haustiere",
        ("Freizeit", ["Hobbys und Sport", "Urlaub", "Abos und Streaming", "Bücher und Medien"]),
        "Bildung", "Geschenke und Spenden", "Gebühren", "Sonstige Ausgaben",
    ], EXPENSE)

    employer_a, employer_b = "Gartenbau Nordhang GmbH", "Stadtbibliothek Sonnenau"
    for name, category in {
        employer_a: "Gehalt", employer_b: "Gehalt", "Kindergeldstelle Sonnenau": "Sozialleistungen",
        "Wohnbau Sonnenau eG": "Miete oder Hauskredit", "Stadtwerke Sonnenau": "Strom und Heizung",
        "Zweckverband Wasser Sonnenau": "Wasser und Müll",
        "Nordlicht Internet": "Internet und Telefon", "Funkstern Mobilfunk": "Internet und Telefon",
        "Drogerie Regenbogen": "Haushaltswaren", "Supermarkt Sonnental": "Lebensmittel",
        "Bäckerei Kornblume": "Lebensmittel", "Hofgut Linde Wochenmarkt": "Lebensmittel",
        "Gasthaus Eichenhain": "Auswärts essen", "Eisdiele Zuckerhut": "Auswärts essen",
        "Verkehrsverbund Nordland": "Bus und Bahn", "Tankstelle Waldweg": "Auto",
        "Autohaus Kalkberg": "Auto", "Apotheke am Markt": "Arzt und Apotheke",
        "Versicherung Sternschnuppe": "Versicherungen", "Modehaus Pfau": "Kleidung",
        "Grundschule Sonnenau Förderverein": "Schule und Kinderbetreuung",
        "Turnverein Sonnenau 1892": "Hobbys und Sport", "Ferienhof Seeblick": "Urlaub",
        "Streamfabrik": "Abos und Streaming", "Zeitungsverlag Sonnenau": "Bücher und Medien",
        "Buchhandlung Lesezeichen": "Bücher und Medien", "Sonnenauer Bank": "Gebühren",
        "Blumen Lindenhof": "Geschenke und Spenden", "Tierarzt Dr. Weidenfeld": "Haustiere",
        "Spielwaren Kreisel": "Geschenke und Spenden",
    }.items():
        book.payee(name, category)

    rng_food, rng_day, rng_misc = s("food"), s("days"), s("misc")
    for index, (y, m) in enumerate(months):
        first = workday(day(y, m, 1))
        # Pay and benefits
        book.earn(workday(day(y, m, 1)), G, "Gehalt", 246840, "Gehalt", employer_a)
        book.earn(workday(day(y, m, 2)), G, "Gehalt", 138675, "Gehalt", employer_b)
        book.earn(workday(day(y, m, 3)), G, "Sozialleistungen", 51000, "Kindergeld", "Kindergeldstelle Sonnenau")
        book.spend(first, G, "Miete oder Hauskredit", 118500, "Miete inkl. Nebenkosten", "Wohnbau Sonnenau eG")
        book.spend(workday(day(y, m, 3)), G, "Strom und Heizung", 14800, "Abschlag Strom und Gas", "Stadtwerke Sonnenau")
        book.spend(workday(day(y, m, 15)), G, "Wasser und Müll", 5800, "Abschlag Wasser und Müll", "Zweckverband Wasser Sonnenau")
        book.spend(workday(day(y, m, 5)), G, "Internet und Telefon", 4495, "Internet", "Nordlicht Internet")
        book.spend(workday(day(y, m, 7)), G, "Internet und Telefon", 2999, "Handyvertrag", "Funkstern Mobilfunk")
        book.spend(first, G, "Bus und Bahn", 6300, "Deutschlandticket", "Verkehrsverbund Nordland")
        book.spend(workday(day(y, m, 4)), G, "Versicherungen", 7130, "Kfz-Versicherung", "Versicherung Sternschnuppe")
        book.spend(workday(day(y, m, 4)), G, "Versicherungen", 1790, "Haftpflicht und Hausrat", "Versicherung Sternschnuppe")
        book.spend(workday(day(y, m, 6)), G, "Schule und Kinderbetreuung", 4200, "Mittagsverpflegung Grundschule", "Grundschule Sonnenau Förderverein")
        book.spend(workday(day(y, m, 6)), G, "Schule und Kinderbetreuung", 9500, "Hortbeitrag", "Grundschule Sonnenau Förderverein")
        book.spend(workday(day(y, m, 10)), G, "Hobbys und Sport", 2250, "Mitgliedsbeitrag Turnen", "Turnverein Sonnenau 1892")
        book.spend(workday(day(y, m, 28)), G, "Gebühren", 490, "Kontoführung", "Sonnenauer Bank")
        book.earn(workday(day(y, m, 29)), S, "Zinsen", 940, "Zinsen Sparkonto", "Sonnenauer Bank")
        book.move(workday(day(y, m, 8)), G, B, 5000, "Bargeld abgehoben")
        book.move(workday(day(y, m, 26)), G, S, 60000, "Sparrate")
        book.spend(day(y, m, 12), K, "Abos und Streaming", 1299, "Streaming", "Streamfabrik")
        book.spend(day(y, m, 12), K, "Gebühren", 290, "Kartenentgelt", "Sonnenauer Bank")
        # What comes once a year, by the season.
        if m == 11:
            book.earn(workday(day(y, m, 27)), G, "Gehalt", 123420, "Weihnachtsgeld", employer_a)
        if m == 12:
            book.spend(day(y, m, 9), K, "Geschenke und Spenden", rng_misc.between(16000, 24000),
                       "Weihnachtsgeschenke Kinder", "Spielwaren Kreisel")
            book.spend(day(y, m, 14), B, "Sonstige Ausgaben", 3900, "Weihnachtsbaum", None)
            book.spend(day(y, m, 18), G, "Geschenke und Spenden", 5000, "Spende Tafel Sonnenau", None)
        if m == 1:
            book.spend(day(y, m, 17), G, "Haushaltswaren", rng_misc.between(4500, 9800),
                       "Winterschlussverkauf Bettwäsche", None)
        if m == 3:
            book.earn(workday(day(y, m, 20)), G, "Erstattungen", rng_misc.between(9000, 21000),
                      "Nebenkostenabrechnung Guthaben", "Wohnbau Sonnenau eG")
        if m == 4:
            book.spend(day(y, m, 14), G, "Arzt und Apotheke", 4500, "Zahnarzt Zuzahlung", None)
            book.spend(day(y, m, 21), G, "Fahrrad", rng_misc.between(6500, 14500),
                       "Fahrradinspektion", None)
        if m == 5:
            book.spend(day(y, m, 2), G, "Urlaub", 64000, "Anzahlung Ferienwohnung", "Ferienhof Seeblick")
            book.spend(day(y, m, 9), B, "Geschenke und Spenden", 1250, "Blumen zum Muttertag", "Blumen Lindenhof")
        if m == 6:
            book.spend(day(y, m, 21), G, "Geschenke und Spenden", 3500, "Geburtstagsgeschenk", None)
        if m == 7:
            book.spend(day(y, m, 3), G, "Urlaub", 96000, "Restzahlung Ferienwohnung", "Ferienhof Seeblick")
        if m == 8:
            book.spend(day(y, m, 20), K, "Urlaub", 11850, "Fähre Ostsee", None)
            book.spend(day(y, m, 22), K, "Urlaub", rng_misc.between(9000, 16000), "Ausflüge im Urlaub", None)
            book.spend(day(y, m, 9), G, "Hobbys und Sport", 3600, "Schwimmkurs Kinder", "Turnverein Sonnenau 1892")
        if m == 9:
            book.spend(day(y, m, 8), K, "Kleidung", rng_misc.between(3800, 5200), "Schulkleidung", "Modehaus Pfau")
            book.spend(day(y, m, 10), G, "Schule und Kinderbetreuung", 3520, "Schulbedarf Schuljahresbeginn", None)
            book.spend(day(y, m, 17), G, "Auto", 18640, "Inspektion", "Autohaus Kalkberg")
            book.spend(day(y, m, 24), G, "Bücher und Medien", 3890, "Zeitungsabo", "Zeitungsverlag Sonnenau")
        if m == 10:
            book.spend(day(y, m, 9), K, "Kleidung", rng_misc.between(5500, 8900), "Winterjacke Kind", "Modehaus Pfau")
            book.spend(day(y, m, 14), G, "Arzt und Apotheke", 4500, "Zahnarzt Zuzahlung", None)
            book.spend(day(y, m, 6), G, "Haustiere", 5240, "Tierarzt Impfung", "Tierarzt Dr. Weidenfeld")
            book.spend(day(y, m, 22), K, "Bücher und Medien", 2495, "Kinderbuch", "Buchhandlung Lesezeichen")
            book.spend(day(y, m, 16), B, "Geschenke und Spenden", 1250, "Blumen", "Blumen Lindenhof")
            book.spend(day(y, m, 21), G, "Geschenke und Spenden", 3500, "Geburtstagsgeschenk", None)
        if index == len(months) - 1:
            book.spend(day(y, m, 19), G, "Reparaturen", 7490, "Wasserhahn Küche", None)
        # Food and household: the biggest part of daily life.
        for number in range(rng_food.between(6, 7)):
            when = workday(day(y, m, 2 + number * 4 + rng_day.between(0, 2)))
            book.spend(when, G, "Lebensmittel", rng_food.between(2860, 11840),
                       "Wocheneinkauf", "Supermarkt Sonnental")
        for number in range(rng_food.between(3, 4)):
            when = day(y, m, 3 + number * 8 + rng_day.between(0, 3))
            book.spend(when, B, "Lebensmittel", rng_food.between(620, 1580), "Brötchen", "Bäckerei Kornblume")
        for number in range(2):
            book.spend(workday(day(y, m, 9 + number * 14)), G, "Lebensmittel",
                       rng_food.between(1840, 4220), "Obst und Gemüse", "Hofgut Linde Wochenmarkt")
        for number in range(4):
            who = ["Gasthaus Eichenhain", "Eisdiele Zuckerhut", "Gasthaus Eichenhain", "Eisdiele Zuckerhut"][number]
            book.spend(workday(day(y, m, 5 + number * 7 + rng_day.between(0, 3))),
                       K if number == 0 else G, "Auswärts essen", rng_food.between(1250, 6840),
                       "Essen gehen" if number != 1 else "Eis", who)
        for number in range(3):
            book.spend(workday(day(y, m, 8 + number * 9)), G, "Haushaltswaren",
                       rng_misc.between(1450, 6880), "Drogerie", "Drogerie Regenbogen")
        book.spend(day(y, m, 14), K, "Kleidung", rng_misc.between(6500, 19800),
                   "Kleidung Kinder und Erwachsene", "Modehaus Pfau")
        book.spend(workday(day(y, m, 16)), G, "Hobbys und Sport", rng_misc.between(4500, 11000),
                   "Ausflug mit den Kindern", None)
        book.spend(workday(day(y, m, 25)), G, "Sonstige Ausgaben", rng_misc.between(2500, 9000),
                   "Sonstiges", None)
        for number in range(2):
            book.spend(workday(day(y, m, 11 + number * 12)), G, "Auto",
                       rng_misc.between(5260, 7810), "Tanken", "Tankstelle Waldweg")
        book.spend(workday(day(y, m, 18)), G, "Arzt und Apotheke", rng_misc.between(845, 2490),
                   "Apotheke", "Apotheke am Markt")
    # The running month (the reference month): paid by card and in cash only,
    # because the Girokonto's lines of this month come from the statement.
    # KassenSturz leaves out what lies after the day an example book is made
    # (D-584), so the first day always has something.
    rng_now = s("reference-month")
    for offset, account, category, memo, who, low, high in [
        (1, B, "Lebensmittel", "Brötchen", "Bäckerei Kornblume", 620, 1580),
        (1, K, "Lebensmittel", "Einkauf", "Supermarkt Sonnental", 2860, 6840),
        (2, K, "Auto", "Tanken", "Tankstelle Waldweg", 5260, 7810),
        (4, B, "Lebensmittel", "Brötchen", "Bäckerei Kornblume", 620, 1580),
        (6, K, "Auswärts essen", "Essen gehen", "Gasthaus Eichenhain", 2450, 6840),
        (8, K, "Haushaltswaren", "Drogerie", "Drogerie Regenbogen", 1450, 4880),
        (10, B, "Lebensmittel", "Obst und Gemüse", "Hofgut Linde Wochenmarkt", 1840, 4220),
        (12, K, "Abos und Streaming", "Streaming", "Streamfabrik", 1299, 1299),
        (12, K, "Gebühren", "Kartenentgelt", "Sonnenauer Bank", 290, 290),
        (14, K, "Kleidung", "Kleidung Kinder und Erwachsene", "Modehaus Pfau", 6500, 14800),
        (17, B, "Lebensmittel", "Brötchen", "Bäckerei Kornblume", 620, 1580),
        (19, K, "Lebensmittel", "Einkauf", "Supermarkt Sonnental", 2860, 9840),
        (23, K, "Auto", "Tanken", "Tankstelle Waldweg", 5260, 7810),
        (25, B, "Auswärts essen", "Eis", "Eisdiele Zuckerhut", 850, 1650),
        (28, K, "Lebensmittel", "Einkauf", "Supermarkt Sonnental", 2860, 9840),
    ]:
        book.spend(day(ry, rm, offset), account, category, rng_now.between(low, high), memo, who)
    # The credit card is paid off from the Girokonto early each month: the
    # book's Umbuchungen. The first one also pays what was owed at the start.
    for index, (y, m) in enumerate(months[1:]):
        pay_day = workday(day(y, m, 3))
        owed = -sum(sh for tx in book.tx if tx["date"] < pay_day
                    for n, _, sh in tx["parts"] if n == K) + (31240 if index == 0 else 0)
        book.move(pay_day, G, K, owed, "Kreditkarte beglichen")
    # The last line of the Girokonto is the one the statement repeats.
    book.spend(day(*months[-1], 30), G, "Lebensmittel", 6318, "Wocheneinkauf", "Supermarkt Sonnental")
    opening(book, {G: 325000, S: 960000, B: 14000, K: -31240})
    book.finish()
    totals, _ = book.totals()
    assert totals[G] > 0 and totals[B] >= 0, totals
    new = [
        (2, "Gartenbau Nordhang GmbH", "Gehalt", 246840),
        (2, "Wohnbau Sonnenau eG", "Miete inkl. Nebenkosten", -118500),
        (2, "Verkehrsverbund Nordland", "Deutschlandticket", -6300),
        (3, "Supermarkt Sonnental", "Wocheneinkauf", -9275),
        (4, "Versicherung Sternschnuppe", "Kfz-Versicherung", -7130),
        (4, "Nordlicht Internet", "Internet", -4495),
        (5, "Tankstelle Waldweg", "Tanken", -6104),
        (6, "Hofgut Linde Wochenmarkt", "Obst und Gemüse", -2386),
    ]
    files = book_statement(book, G, IBAN_DE, True, 1, new, reference)
    return book, files


# ============================================================ Flower Utopia

def flower_utopia(reference):
    months = earlier_months(reference)
    ry, rm = reference
    book = Book("flower-utopia", "Flower Utopia", "EUR", "Blumenwiese Gartengemeinschaft",
                months, "Gartenbank Sonnenau")
    s = lambda name: stream("flower-utopia", name)
    V, K, R = "Vereinskonto", "Gemeinschaftskasse", "Rücklage"
    book.account(V, CHECKING, None, "EUR", IBAN_AT, bank=True)
    book.account(K, CASH)
    book.account(R, SAVINGS, None, "EUR", None, bank=True)
    book.account("Anfangsbestand", EQUITY)
    book.tree(["Mitgliedsbeiträge", "Verkauf Markttag", "Spenden", "Zuschüsse", "Zinsen"], INCOME)
    book.tree([
        ("Essen und Trinken", ["Sommerfest", "Kaffee und Kuchen"]),
        "Saatgut und Pflanzen", "Erde und Dünger", "Geräte und Werkzeug", "Wasser", "Pacht",
        "Versicherung", "Reparaturen", "Werbung und Druck", "Gebühren",
    ], EXPENSE)
    for name, category in {
        "Mitglieder Sammelbeitrag": "Mitgliedsbeiträge", "Marktbesucher": "Verkauf Markttag",
        "Stiftung Grünes Band": "Zuschüsse", "Stadt Sonnenau Grünflächenamt": "Pacht",
        "Wasserwerk Sonnenau": "Wasser", "Samenhaus Löwenzahn": "Saatgut und Pflanzen",
        "Gärtnerei Hohlweg": "Saatgut und Pflanzen", "Erdenwerk Moorkamp": "Erde und Dünger",
        "Baumarkt Eisenkraut": "Geräte und Werkzeug",
        "Versicherung Sternschnuppe": "Versicherung", "Getränke Quellwiese": "Sommerfest",
        "Schreinerei Holzwurm": "Reparaturen", "Druckerei Tintenfass": "Werbung und Druck",
        "Gartenbank Sonnenau": "Gebühren", "Bäckerei Kornblume": "Kaffee und Kuchen",
    }.items():
        book.payee(name, category)
    rng = s("life")
    # Watering by the season: little in winter, most in high summer.
    water = {1: 600, 2: 600, 3: 900, 4: 1800, 5: 2900, 6: 3900, 7: 4600, 8: 4180,
             9: 3640, 10: 2910, 11: 1200, 12: 600}
    market = {5: "Pflanzenmarkt Mai: Setzlinge", 6: "Markttag Juni: Sommerblumen",
              8: "Markttag August: Sträuße und Setzlinge", 9: "Markttag September: Herbststräuße",
              10: "Herbstmarkt Oktober", 12: "Adventsmarkt: Kränze und Gestecke"}
    for index, (y, m) in enumerate(months):
        book.earn(workday(day(y, m, 2)), V, "Mitgliedsbeiträge", 14400, "Beiträge 24 Mitglieder × 6 €",
                  "Mitglieder Sammelbeitrag")
        book.spend(workday(day(y, m, 1)), V, "Pacht", 5200, "Pacht Gartenfläche", "Stadt Sonnenau Grünflächenamt")
        book.spend(workday(day(y, m, 28)), V, "Gebühren", 350, "Kontoführung", "Gartenbank Sonnenau")
        if index < len(months) - 1:
            book.spend(workday(day(y, m, 20)), V, "Wasser", water[m], "Wasser Gießen",
                       "Wasserwerk Sonnenau")
        book.spend(day(y, m, 10), K, "Kaffee und Kuchen", rng.between(850, 1240), "Kaffee, Tee und Kekse",
                   "Bäckerei Kornblume")
        book.spend(day(y, m, 24), K, "Kaffee und Kuchen", rng.between(1450, 3240), "Kuchen Arbeitseinsatz",
                   "Bäckerei Kornblume")
        book.earn(workday(day(y, m, 29)), R, "Zinsen", 410, "Zinsen Rücklage", "Gartenbank Sonnenau")
        if m in market:
            book.earn(day(y, m, 14 if m != 12 else 6), K, "Verkauf Markttag",
                      rng.between(8000, 19000), market[m], "Marktbesucher")
            # What the market brought goes into the club account: a Transfer.
            book.move(workday(day(y, m, 17)), K, V, 7500, "Einzahlung Kasse")
        if m == 3:
            book.spend(day(y, m, 6), V, "Saatgut und Pflanzen", rng.between(4800, 9600),
                       "Saatgut Frühjahr", "Samenhaus Löwenzahn")
            book.spend(day(y, m, 12), V, "Erde und Dünger", 6480, "Blumenerde, 4 Säcke", "Erdenwerk Moorkamp")
        if m == 4:
            book.spend(day(y, m, 18), V, "Geräte und Werkzeug", rng.between(3200, 7400),
                       "Gießkannen und Handschaufeln", "Baumarkt Eisenkraut")
        if m == 8:
            book.spend(day(y, m, 8), V, "Sommerfest", 14860, "Getränke Sommerfest", "Getränke Quellwiese")
            book.spend(day(y, m, 8), K, "Sommerfest", 3240, "Eis und Luftballons", None)
            book.spend(day(y, m, 14), V, "Versicherung", 9600, "Haftpflicht Gartengemeinschaft",
                       "Versicherung Sternschnuppe")
        if m == 9:
            book.spend(day(y, m, 5), V, "Saatgut und Pflanzen", 14320, "Blumenzwiebeln für das Frühjahr",
                       "Samenhaus Löwenzahn")
            book.earn(day(y, m, 16), V, "Zuschüsse", 25000, "Förderung Gemeinschaftsgärten",
                      "Stiftung Grünes Band")
            book.spend(day(y, m, 22), V, "Werbung und Druck", 2490, "Flyer Herbstmarkt", "Druckerei Tintenfass")
        if m == 10:
            book.spend(day(y, m, 4), V, "Erde und Dünger", 6480, "Blumenerde, 4 Säcke", "Erdenwerk Moorkamp")
            book.spend(day(y, m, 6), V, "Saatgut und Pflanzen", 3180, "Herbstastern", "Gärtnerei Hohlweg")
            book.earn(day(y, m, 20), V, "Spenden", 5000, "Spende eines Nachbarn", None)
        if m == 12:
            book.earn(day(y, m, 15), V, "Spenden", rng.between(3000, 8000), "Weihnachtsspende", None)
        if m == 2:
            book.spend(day(y, m, 12), V, "Reparaturen", rng.between(5200, 9800),
                       "Gewächshaustür repariert" if y % 2 else "Zaun ausgebessert",
                       "Schreinerei Holzwurm")
    # The last line of the Vereinskonto is the one the statement repeats.
    book.spend(day(*months[-1], 30), V, "Wasser", water[months[-1][1]], "Wasser Gießen",
               "Wasserwerk Sonnenau")
    # The running month (the reference month): the cash box only, because the
    # club account's lines of this month come from the statement (D-584).
    rng_now = s("reference-month")
    for offset, category, memo, low, high in [
        (1, "Kaffee und Kuchen", "Kaffee für den Arbeitseinsatz", 650, 1180),
        (7, "Kaffee und Kuchen", "Kaffee, Tee und Kekse", 850, 1240),
        (15, "Kaffee und Kuchen", "Kuchen Arbeitseinsatz", 1450, 3240),
        (26, "Kaffee und Kuchen", "Kaffee, Tee und Kekse", 850, 1240),
    ]:
        book.spend(day(ry, rm, offset), K, category, rng_now.between(low, high), memo,
                   "Bäckerei Kornblume")
    book.earn(day(ry, rm, 1), K, "Verkauf Markttag", rng_now.between(1800, 3600),
              "Pflanzenverkauf am Gartentor", "Marktbesucher")
    book.earn(day(ry, rm, 21), K, "Verkauf Markttag", rng_now.between(2400, 5200),
              "Pflanzenverkauf am Gartentor", "Marktbesucher")
    opening(book, {V: 142000, K: 6000, R: 200000})
    book.finish()
    totals, _ = book.totals()
    assert totals[V] > 0 and totals[K] >= 0, totals
    new = [
        (2, "Mitglieder Sammelbeitrag", "Beiträge 24 Mitglieder × 6 €", 14400),
        (2, "Stadt Sonnenau Grünflächenamt", "Pacht Gartenfläche", -5200),
        (3, "Versicherung Sternschnuppe", "Haftpflicht Gartengemeinschaft", -2400),
        (4, "Samenhaus Löwenzahn", "Tulpenzwiebeln Nachbestellung", -4860),
        (5, "Wasserwerk Sonnenau", "Wasser Gießen", -2240),
        (5, "Erdenwerk Moorkamp", "Laubkompost", -3290),
        (6, "Marktbesucher", "Spende Herbstmarkt", 3000),
        (6, "Druckerei Tintenfass", "Plakate Wintermarkt", -1890),
    ]
    files = book_statement(book, V, IBAN_AT, True, 1, new, reference)
    return book, files


# ========================================================= UdSSR Government Agent

# Agency rate at the start: rubles per 1 unit of the currency, over 10000. It
# moves a little every month.
RATES_START = {"USD": 7234, "GBP": 9112, "CHF": 3350}


def agency_rates(count):
    """One rate per currency and month, `count` months, as a list of dicts."""
    rng = stream("udssr-agent", "rates")
    now = dict(RATES_START)
    out = []
    for _ in range(count):
        out.append(dict(now))
        for code in now:
            now[code] += rng.between(0, 80) - 40
    return out


def udssr_agent(reference):
    months = earlier_months(reference)
    ry, rm = reference
    rates = agency_rates(len(months) + 1)
    book = Book("udssr-agent", "UdSSR Government Agent", "RUB",
                "Genosse Iwan Petrowitsch Nebelow", months, "Staatsbank der Wolkenverwaltung")
    s = lambda name: stream("udssr-agent", name)
    G, SP, CA = "Gehaltskonto", "Sparbuch", "Bargeld Rubel"
    USD, GBP, CHF = "Dienstkasse Dollar", "Dienstkonto London", "Nummernkonto Zürich"
    book.account(G, CHECKING, None, "RUB", None, bank=True)
    book.account(SP, SAVINGS, None, "RUB", None, bank=True)
    book.account(CA, CASH, None, "RUB")
    book.account(USD, CHECKING, None, "USD")
    book.account(GBP, CHECKING, None, "GBP")
    book.account(CHF, SAVINGS, None, "CHF")
    book.account("Anfangsbestand", EQUITY, None, "RUB")
    book.tree(["Gehalt", "Auslandszulage", "Prämien", "Zinsen"], INCOME)
    book.tree([
        ("Verpflegung", ["Lebensmittel", "Kantine"]),
        ("Wohnen", ["Kommunalwohnung", "Strom und Gas"]),
        "Verkehr", "Kleidung", "Tarnung und Requisiten", "Bücher und Zeitungen", "Kultur",
        ("Dienstreisen", ["Hotel", "Essen unterwegs", "Fahrten"]), "Gebühren",
    ], EXPENSE)
    for name, category in {
        "Hauptverwaltung für Windstille": "Gehalt", "Staatsbank der Wolkenverwaltung": "Zinsen",
        "Gastronom Nr. 7": "Lebensmittel", "Brotladen Nr. 3": "Lebensmittel",
        "Kantine der Hauptverwaltung": "Kantine", "Hausverwaltung Nr. 12": "Kommunalwohnung",
        "Elektrizitätswerk Nordstadt": "Strom und Gas", "Metro Nordstadt": "Verkehr",
        "Kaufhaus Morgenrot": "Kleidung", "Theaterwerkstatt am Fluss": "Tarnung und Requisiten",
        "Zeitungsvertrieb Nordstadt": "Bücher und Zeitungen", "Ballett am Kanal": "Kultur",
        "Hotel Brackenhill Court": "Hotel", "Gasthof Zum Stern": "Hotel",
        "Bahngesellschaft Nordland": "Fahrten",
    }.items():
        book.payee(name, category)
    rng = s("life")

    def rub_of(foreign_cents, code, index):
        return (foreign_cents * rates[index][code] + 5000) // 10000

    def abroad(when, category, account, code, index, cents, memo, who=None):
        rub = rub_of(cents, code, index)
        book.add(when, [(category, rub), (account, -rub, -cents)], memo, who)

    for index, (y, m) in enumerate(months + [reference]):
        for code in sorted(RATES_START):
            book.rates.append((code, "RUB", date(y, m, 1), rates[index][code]))
    for index, (y, m) in enumerate(months):
        book.earn(workday(day(y, m, 20)), G, "Gehalt", 42000, "Gehalt Hauptverwaltung",
                  "Hauptverwaltung für Windstille")
        book.spend(workday(day(y, m, 3)), G, "Kommunalwohnung", 1860, "Miete Kommunalwohnung", "Hausverwaltung Nr. 12")
        book.spend(workday(day(y, m, 3)), G, "Strom und Gas", rng.between(380, 520), "Strom und Gas", "Elektrizitätswerk Nordstadt")
        book.spend(workday(day(y, m, 1)), G, "Verkehr", 600, "Metro-Monatskarte", "Metro Nordstadt")
        book.spend(workday(day(y, m, 5)), G, "Bücher und Zeitungen", 150, "Zeitungsabo", "Zeitungsvertrieb Nordstadt")
        book.earn(workday(day(y, m, 29)), SP, "Zinsen", 85, "Zinsen Sparbuch", "Staatsbank der Wolkenverwaltung")
        book.spend(workday(day(y, m, 28)), G, "Gebühren", 30, "Kontoführung", "Staatsbank der Wolkenverwaltung")
        for number in range(6):
            book.spend(workday(day(y, m, 2 + number * 5 + rng.between(0, 2))), G, "Lebensmittel",
                       rng.between(380, 2250), "Einkauf Lebensmittel", "Gastronom Nr. 7")
        for number in range(3):
            book.spend(day(y, m, 6 + number * 9), CA, "Lebensmittel", rng.between(40, 180),
                       "Brot und Milch", "Brotladen Nr. 3")
        for number in range(4):
            book.spend(workday(day(y, m, 7 + number * 7)), G, "Kantine", rng.between(550, 780),
                       "Mittagessen Kantine (Woche)", "Kantine der Hauptverwaltung")
        book.spend(day(y, m, 16), CA, "Kultur", rng.between(150, 350), "Kino und Theater", "Ballett am Kanal")
        book.move(workday(day(y, m, 11)), G, CA, 1500, "Bargeld abgehoben")
        # Allowance paid in dollars into the service cash box, every quarter.
        if m in (3, 6, 9, 12):
            usd_cents = 25000
            rub = rub_of(usd_cents, "USD", index)
            book.add(workday(day(y, m, 21)), [("Auslandszulage", -rub), (USD, rub, usd_cents)],
                     "Auslandszulage in Dollar", "Hauptverwaltung für Windstille")
        # The Transfer: rubles to the savings book.
        book.move(workday(day(y, m, 22)), G, SP, 8000, "Auf das Sparbuch")
        if m in (1, 4, 7, 10):
            book.earn(workday(day(y, m, 9)), G, "Prämien", 15000, "Quartalsprämie Windstille",
                      "Hauptverwaltung für Windstille")
        if m in (2, 5, 8, 11):
            abroad(day(y, m, 17), "Fahrten", USD, "USD", index, rng.between(2200, 4800),
                   "Taxi und Trinkgeld")
        if m in (3, 8):
            # London.
            pounds = 30000
            book.add(workday(day(y, m, 11)),
                     [(G, -rub_of(pounds, "GBP", index)), (GBP, rub_of(pounds, "GBP", index), pounds)],
                     "Rubel in Pfund getauscht für die Londoner Reise", None)
            abroad(day(y, m, 12), "Hotel", GBP, "GBP", index, 21600, "Hotel London, 3 Nächte",
                   "Hotel Brackenhill Court")
            abroad(day(y, m, 13), "Essen unterwegs", GBP, "GBP", index, rng.between(1800, 3200),
                   "Abendessen London")
            abroad(day(y, m, 14), "Fahrten", GBP, "GBP", index, rng.between(1200, 2200),
                   "Bahn und Bus London")
        if m in (6, 9):
            # Zürich, paid from the francs account.
            francs = 60000
            book.add(workday(day(y, m, 8)),
                     [(G, -rub_of(francs, "CHF", index)), (CHF, rub_of(francs, "CHF", index), francs)],
                     "Rubel in Franken getauscht für die Zürcher Reise", None)
            abroad(day(y, m, 10), "Hotel", CHF, "CHF", index, 28000, "Gasthof Zürich, 2 Nächte",
                   "Gasthof Zum Stern")
            abroad(day(y, m, 11), "Essen unterwegs", CHF, "CHF", index, rng.between(3600, 5600),
                   "Abendessen Zürich")
            abroad(day(y, m, 12), "Fahrten", CHF, "CHF", index, 7200, "Bahnfahrkarte Zürich",
                   "Bahngesellschaft Nordland")
        if m == 9:
            book.spend(day(y, m, 18), G, "Kleidung", 14800, "Wintermantel", "Kaufhaus Morgenrot")
        if m == 12:
            book.spend(day(y, m, 15), G, "Kleidung", 6400, "Pelzmütze", "Kaufhaus Morgenrot")
        if m in (5, 10):
            book.spend(day(y, m, 15), G, "Tarnung und Requisiten", rng.between(1600, 2800),
                       "Hut und Schnurrbart", "Theaterwerkstatt am Fluss")
    # The statement repeats this last line of the Gehaltskonto.
    book.spend(day(*months[-1], 30), G, "Lebensmittel", 1475, "Einkauf Lebensmittel", "Gastronom Nr. 7")
    # The running month (the reference month): cash and the dollar box only,
    # because the Gehaltskonto's lines of this month come from the statement
    # (D-584).
    rng_now = s("reference-month")
    now = len(months)
    for offset, category, memo, who, low, high in [
        (1, "Lebensmittel", "Brot und Milch", "Brotladen Nr. 3", 40, 180),
        (6, "Lebensmittel", "Brot und Milch", "Brotladen Nr. 3", 40, 180),
        (10, "Kultur", "Kino und Theater", "Ballett am Kanal", 150, 350),
        (15, "Lebensmittel", "Brot und Milch", "Brotladen Nr. 3", 40, 180),
        (24, "Lebensmittel", "Brot und Milch", "Brotladen Nr. 3", 40, 180),
    ]:
        book.spend(day(ry, rm, offset), CA, category, rng_now.between(low, high), memo, who)
    abroad(day(ry, rm, 1), "Bücher und Zeitungen", USD, "USD", now, rng_now.between(300, 900),
           "Zeitschriften am Kiosk")
    abroad(day(ry, rm, 4), "Fahrten", USD, "USD", now, rng_now.between(2200, 4800),
           "Taxi und Trinkgeld")
    abroad(day(ry, rm, 18), "Essen unterwegs", USD, "USD", now, rng_now.between(1200, 2600),
           "Mittagessen mit einem Kontakt")
    opening(book, {G: 80000, SP: 250000, CA: 3000}, floor=2000)
    book.finish()
    totals, _ = book.totals()
    assert totals[G] > 0, totals
    new = [
        (2, "Hauptverwaltung für Windstille", "Quartalsbericht Windstille, Zuschlag", 12500),
        (2, "Hausverwaltung Nr. 12", "Miete Kommunalwohnung", -1860),
        (2, "Metro Nordstadt", "Metro-Monatskarte", -600),
        (3, "Gastronom Nr. 7", "Einkauf Lebensmittel", -1280),
        (3, "Elektrizitätswerk Nordstadt", "Strom und Gas", -455),
        (4, "Kantine der Hauptverwaltung", "Mittagessen Kantine (Woche)", -640),
        (5, "Zeitungsvertrieb Nordstadt", "Zeitungsabo", -150),
        (6, "Gastronom Nr. 7", "Einkauf Lebensmittel", -865),
    ]
    files = book_statement(book, G, "40817-810-0-0000-0001", False, 1, new, reference)
    return book, files


# =========================================================== Space Colony

def space_colony(reference):
    """Statements for the existing book, read and never changed. The existing
    line is the last one of Everyday Checking; the new lines come after the
    book's end, in the reference month."""
    db = sqlite3.connect(SPACE_COLONY)
    row = db.execute(
        "SELECT s.postDate, s.shares, s.bankId, t.memo, t.id FROM kmmSplits s "
        "JOIN kmmTransactions t ON t.id = s.transactionId "
        "WHERE s.accountId = 'A000004' ORDER BY s.postDate DESC, t.id DESC LIMIT 1").fetchone()
    when, shares, ref, memo, _ = row
    cents = int(shares[:-4])
    balance = int(db.execute("SELECT balance FROM kmmAccounts WHERE id = 'A000004'")
                  .fetchone()[0][:-4])
    db.close()
    existing = Line(date.fromisoformat(when), "Colony Pay Office", IBAN_NL, memo, cents, ref)
    ry, rm = reference
    specs = [
        (2, "Hydroponics Cooperative", "Pay slip, hydroponics shift", 465000),
        (2, "Dome Management Co.", "Dome rent", -73500),
        (2, "Fusion Grid Utility", "Power", -6601),
        (3, "Olympus Mons Market", "Weekly groceries", -9812),
        (3, "Ceres Greengrocer", "Fresh herbs", -2475),
        (4, "Titan Fuel Station", "Fuel", -4115),
        (5, "Subspace Telecom", "Subspace link", -4990),
        (6, "Crater Bakery", "Algae bread", -1240),
    ]
    rng = stream("space-colony", "statement")
    lines = []
    for number, (offset, who, text, amount) in enumerate(specs, start=1):
        d = next_workday(date(ry, rm, 1) + timedelta(days=offset - 1))
        lines.append(Line(d, who, rng.pick(OTHER_IBANS), text, amount, f"SC{ry % 100:02d}{rm:02d}-N{number:03d}"))
    closing = balance + sum(line.cents for line in lines)
    closing_date = max(line.when for line in lines)
    return statement_files("space-colony", "EUR", IBAN_DE, True, existing, lines, closing,
                           closing_date, f"{iso(closing_date)}T20:00:00+01:00")


# -------------------------------------------------------------------- main


def content(path):
    db = sqlite3.connect(path)
    try:
        return list(db.iterdump())
    finally:
        db.close()


def build_all(reference, out):
    """Returns {relative path: ("sqlite" | "text", path or text)} in a work
    folder, so --check can compare without touching the committed files."""
    made = {}
    for factory in (suburban_family, flower_utopia, udssr_agent):
        book, files = factory(reference)
        path = os.path.join(out, f"{book.slug}-v15.sqlite")
        count, splits = write_book(book, path)
        print(f"{book.slug}: {count} transactions, {splits} splits, "
              f"{iso(book.first)} to {iso(book.last)}")
        made[os.path.basename(path)] = ("sqlite", path)
        for name, text in files.items():
            made[f"statements/{name}"] = ("text", text)
    for name, text in space_colony(reference).items():
        made[f"statements/{name}"] = ("text", text)
    made["reference-month.txt"] = ("text", f"{reference[0]}-{reference[1]:02d}\n")
    return made


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--reference-month", default="2026-11",
                        help="YYYY-MM: the books hold the 24 months before it")
    parser.add_argument("--out", default=OUT, help="where to write the files")
    parser.add_argument("--check", action="store_true",
                        help="fail if the files in --out differ from what this writes")
    args = parser.parse_args()
    year, month = (int(part) for part in args.reference_month.split("-"))
    assert 1 <= month <= 12 and 2000 <= year <= 2100
    if args.check:
        work = tempfile.mkdtemp(prefix="example-books-check-")
        made = build_all((year, month), work)
        bad = []
        for name, (kind, value) in made.items():
            committed = os.path.join(args.out, name)
            if not os.path.exists(committed):
                bad.append(name)
            elif kind == "sqlite":
                if content(value) != content(committed):
                    bad.append(name)
            else:
                with open(committed, encoding="utf-8", newline="") as handle:
                    if handle.read() != value:
                        bad.append(name)
        shutil.rmtree(work)
        if bad:
            sys.exit("not what this script writes: " + ", ".join(bad)
                     + "; run it again and commit the result")
        print("the committed example books and statements are what the script writes")
        return
    os.makedirs(os.path.join(args.out, "statements"), exist_ok=True)
    made = build_all((year, month), args.out)
    for name, (kind, value) in made.items():
        if kind == "text":
            with open(os.path.join(args.out, name), "w", encoding="utf-8", newline="") as handle:
                handle.write(value)


if __name__ == "__main__":
    main()
