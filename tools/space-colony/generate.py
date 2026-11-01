#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
"""Makes the five-year Space Colony test book (D-270).

The book is a KMyMoney SQLite file (layout 15) that KassenSturz imports like
any other. It keeps the accounts, categories, institutions and securities of
the older `space-colony-v15.sqlite`, keeps that book's entries up to
2026-11-01 as realistic examples (with a payee that fits each entry's
category; the ones dated later are left out), and fills the five years around
them with made-up, believable life from 2021-11-01 to 2026-11-01: pay slips split into several parts,
rent and utility bills, seasonal food and energy costs, insurance and tax
installments, saving, a loan being paid off, summer and winter trips paid in
US dollars from a dollar account, and the last weeks as lines the bank
delivered that are not sorted into a category yet.

Everything is exact whole minor units (cents); nothing here uses a floating
point number for money, prices or rates (AGENTS.md §3.1 rule 1). Every
random choice comes from the small generator below, one stream per kind of
event, so the same call always writes the same book, and a change to one
kind of event does not shift the others. Nothing is taken from the older
book that is not already in the repository.

    tools/space-colony/generate.py            # writes the committed book
    tools/space-colony/generate.py --check    # fails if the committed book differs

Python 3.9 or newer, standard library only.
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
SEED = os.path.join(
    ROOT, "crates/kassensturz-import/tests/golden/kmy-sqlite/space-colony-v15.sqlite"
)
SAMPLE = os.path.join(
    ROOT, "crates/kassensturz-import/tests/golden/kmy-sqlite/kassensturz-sample.sqlite"
)
OUT = os.path.join(
    ROOT, "crates/kassensturz-import/tests/books/space-colony-5y-v15.sqlite"
)

START = date(2021, 11, 1)
END = date(2026, 11, 1)
LAST_RECONCILED = date(2026, 8, 31)
LAST_CLEARED = date(2026, 10, 14)
UNSORTED_FROM = date(2026, 9, 1)
MASK = (1 << 64) - 1


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

    def chance(self, percent):
        return self.below(100) < percent

    def pick(self, items):
        return items[self.below(len(items))]


_streams = {}


def stream(name):
    """One generator for each kind of event, seeded from its name (FNV-1a)."""
    if name not in _streams:
        h = 0xCBF29CE484222325
        for byte in ("space-colony-5y/" + name).encode():
            h = ((h ^ byte) * 0x100000001B3) & MASK
        _streams[name] = Rng(h)
    return _streams[name]


# ------------------------------------------------------------- small helpers

MONTHS = [
    "", "January", "February", "March", "April", "May", "June", "July",
    "August", "September", "October", "November", "December",
]
# Prices rise a few percent a year; amounts are scaled once, in whole cents.
PRICE_INDEX = {2021: 100, 2022: 104, 2023: 109, 2024: 112, 2025: 115, 2026: 118}
# Energy costs by month, in percent of the yearly average (cold, dark months).
ENERGY = [0, 130, 125, 110, 95, 90, 85, 85, 85, 90, 105, 120, 130]


def scaled(cents, year):
    return (cents * PRICE_INDEX[year] + 50) // 100


def percent(cents, share):
    return (cents * share + 50) // 100


def day(year, month, number):
    return date(year, month, min(number, calendar.monthrange(year, month)[1]))


def workday(when):
    """The Friday before a weekend day: banks book on working days."""
    return when - timedelta(days=max(0, when.weekday() - 4))


def next_workday(when):
    """The Monday after a weekend day: a bill due then is booked then."""
    return when + timedelta(days=max(0, 7 - when.weekday()) % 7 if when.weekday() > 4 else 0)


def months():
    year, month = START.year, START.month
    while (year, month) <= (END.year, END.month):
        yield year, month
        year, month = (year + 1, 1) if month == 12 else (year, month + 1)


def german(minor):
    sign = "-" if minor < 0 else ""
    minor = abs(minor)
    return f"{sign}{minor // 100},{minor % 100:02d}"


# ----------------------------------------------------- the book being written

TX = []  # transactions in the order they are made
SEQ = [0]


def add(when, parts, memo="", payee=None, currency="EUR", unsorted_ok=False):
    """parts: (account name, value, shares or None, memo). Value is in the
    transaction's currency, shares in the account's own."""
    if when < START or when > END:
        return
    full = []
    for part in parts:
        name, value = part[0], part[1]
        shares = part[2] if len(part) > 2 and part[2] is not None else value
        note = part[3] if len(part) > 3 else ""
        full.append((name, value, shares, note))
    SEQ[0] += 1
    TX.append(
        dict(seq=SEQ[0], date=when, memo=memo, payee=payee, currency=currency,
             parts=full, unsorted_ok=unsorted_ok)
    )


CHECKING = "Everyday Checking"
SAVINGS = "Mars Rainy-Day Savings"
PETTY = "Dome Petty Cash"
TRAVEL = "Shuttle Travel Fund"
LOAN = "Shipyard Loan"
EQUITY = "Colony Founding Capital"
WALLET = "Earth Trip Wallet"


def pay(when, source, category, cents, payee, memo, note="", unsorted_ok=False):
    add(when, [(category, cents, None, note), (source, -cents)], memo, payee,
        unsorted_ok=unsorted_ok)


def earn(when, target, category, cents, payee, memo):
    add(when, [(category, -cents), (target, cents)], memo, payee)


def move(when, source, target, cents, memo):
    add(when, [(source, -cents), (target, cents)], memo, None)


# ------------------------------------------------------------------ payees
# name -> category its Rule books to (None: no Rule). Existing payees keep
# their texts for recognizing; only the default category is set right.

PAYEE_DEFAULTS = {
    "Hydroponics Cooperative": "Salary Hydroponics Engineer",
    "Suit Tailors Guild": "Pressure Suits",
    "Rover Garage Gamma": "Rover Maintenance",
    "Olympus Mons Market": "Food",
    "Venus Library": "Books and Data Crystals",
    "Europa Water Board": "Oxygen Quota Fee",
    "Moonbase Medical Center": "Health",
    "Callisto Cafe": "Everyday Spending",
    "Galaxy Electronics": "Tools",
    "Kepler Sports Club": None,
    "Redwood Seed Bank": "Hydroponic Greens",
    "Asteroid Mining Corp.": "Mine Shift Bonus",
    "Subspace Telecom": "Subspace Communications",
    "Aurora Dome Cinema": "Dome 3 Cinema",
    "Titan Fuel Station": "Rover and Vehicles",
    "Helios Insurance": "Hull Breach Cover",
    "Nebula Streaming": "Subspace Music Streaming",
    "Pioneer Fund": "Mars Relief Fund",
    "Colony Council Tax Office": "Land Lease",
    "Phobos Canteen": "Food",
    "Northern Ice Works": "Oxygen and Water",
    "Andromeda Travel Agency": "Earth-Mars Flights",
    "Dome Management Co.": "Rent",
    "Crater Bakery": "Food",
    "Ceres Greengrocer": "Hydroponic Greens",
    "Luna Pharmacy": "Health",
}
NEW_PAYEES = {
    "Fusion Grid Utility": "Fusion Power",
    "Phobos Academy": "Academy Tuition",
    "Zero-G Dental Clinic": "Zero-G Dentist",
    "Tycho Observatory": "Observatory Visits",
    "Selene Orbital Hotel": "Orbital Hotel",
    "Earth Souvenir Bazaar": "Souvenirs",
    "Terra Diner": "Meals on the Road",
    "Earth Tour Company": "Olympus Mons Tours",
}

# ------------------------------------------------------------- exchange rate


def usd_per_eur(year, month):
    """US dollars for 10,000 euro cents x 1: 11560 means 1.1560 (made up)."""
    anchors = [
        ((2021, 11), 11560), ((2022, 9), 9900), ((2023, 6), 10900),
        ((2024, 6), 10700), ((2025, 6), 11500), ((2026, 6), 11700),
        ((2026, 10), 11600), ((2026, 11), 11600),
    ]
    at = year * 12 + month
    for (a, ar), (b, br) in zip(anchors, anchors[1:]):
        lo, hi = a[0] * 12 + a[1], b[0] * 12 + b[1]
        if lo <= at <= hi:
            base = ar + (br - ar) * (at - lo) // (hi - lo)
            break
    wobble = RATE_WOBBLE[(year, month)]
    return base + wobble


RATE_WOBBLE = {}
for _y, _m in months():
    RATE_WOBBLE[(_y, _m)] = stream("rate").between(-150, 150)


def to_usd(eur_cents, year, month):
    return (eur_cents * usd_per_eur(year, month) + 5000) // 10000


def to_eur(usd_cents, year, month):
    return (usd_cents * 10000 + usd_per_eur(year, month) // 2) // usd_per_eur(year, month)


# ------------------------------------------------------------------ events

GROSS = {2021: 405000, 2022: 415000, 2023: 428000, 2024: 440000, 2025: 452000, 2026: 465000}
ALLOWANCE = {2021: 15000, 2022: 15000, 2023: 15000, 2024: 16000, 2025: 16000, 2026: 16000}
RENT = {2021: 66000, 2022: 67500, 2023: 69000, 2024: 70500, 2025: 72000, 2026: 73500}
SUBSPACE = {2021: 3990, 2022: 3990, 2023: 4290, 2024: 4590, 2025: 4590, 2026: 4990}
STREAMING = {2021: 1299, 2022: 1299, 2023: 1299, 2024: 1499, 2025: 1499, 2026: 1599}
SAVINGS_RATE_BP = {2021: 10, 2022: 50, 2023: 200, 2024: 250, 2025: 220, 2026: 180}
GROCER_NOTES = ["Weekly greens", "Fresh herbs", "Vegetable box", "Fruit and greens", "Salad kit"]
SAVINGS_BALANCE = [0]


def salary(year, month):
    gross = GROSS[year]
    tax = percent(gross, 22)
    pension = percent(gross, 5)
    allowance = ALLOWANCE[year]
    net = gross - tax - pension + allowance
    when = workday(day(year, month, 25))
    add(
        when,
        [
            (CHECKING, net, None, f"Pay {MONTHS[month]} {year}"),
            ("Colony Income Tax", tax, None, "Income tax"),
            ("Colony Pension Fund", pension, None, "Pension share"),
            ("Salary Hydroponics Engineer", -gross, None, "Gross salary"),
            ("Low-Gravity Allowance", -allowance, None, "Low-gravity allowance"),
        ],
        f"Pay {MONTHS[month]} {year}",
        "Hydroponics Cooperative",
    )
    rng = stream("bonus")
    if month in (3, 12):
        amount = percent(80000, rng.between(70, 130))
        earn(workday(day(year, month, 20)), CHECKING, "Mine Shift Bonus", amount,
             "Asteroid Mining Corp.", f"Shift bonus {MONTHS[month]} {year}")


def housing(year, month):
    rng = stream("housing")
    rent = RENT[year]
    air = scaled(6500, year) + rng.between(0, 2800)
    add(
        next_workday(day(year, month, 1)),
        [
            ("Rent", rent, None, "Rent"),
            ("Air Recycling", air, None, "Air recycling service"),
            (CHECKING, -(rent + air)),
        ],
        f"Dome invoice {MONTHS[month]} {year}",
        "Dome Management Co.",
    )
    power = percent(scaled(5600, year), ENERGY[month]) + rng.between(-400, 400)
    pay(workday(day(year, month, 5)), CHECKING, "Fusion Power", power, "Fusion Grid Utility",
        f"Power {MONTHS[month]}")
    water = scaled(3600, year) + rng.between(0, 2400)
    pay(day(year, month, 12), CHECKING, "Oxygen and Water", water, "Northern Ice Works",
        "Ice delivery")
    if month == 3:
        pay(workday(day(year, 3, 14)), CHECKING, "Oxygen Quota Fee", scaled(9600, year) + rng.between(0, 1800),
            "Europa Water Board", f"Oxygen quota {year}")
    if month in (3, 6, 9, 12):
        pay(workday(day(year, month, 15)), CHECKING, "Land Lease", scaled(21500, year),
            "Colony Council Tax Office", f"Land lease {MONTHS[month]}")


def connected_life(year, month):
    rng = stream("leisure")
    pay(workday(day(year, month, 3)), CHECKING, "Subspace Communications", SUBSPACE[year],
        "Subspace Telecom", f"Subspace plan {MONTHS[month]}")
    pay(day(year, month, 8), CHECKING, "Subspace Music Streaming", STREAMING[year],
        "Nebula Streaming", "Streaming subscription", unsorted_ok=True)
    for _ in range(rng.between(1, 3)):
        evening = day(year, month, rng.between(1, 28))
        evening += timedelta(days=(4 - evening.weekday()) % 7)
        pay(evening, CHECKING, "Dome 3 Cinema", scaled(rng.between(850, 1950), year),
            "Aurora Dome Cinema", "Cinema night", unsorted_ok=True)
    if rng.chance(35):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Observatory Visits",
            scaled(rng.between(1200, 2200), year), "Tycho Observatory", "Observatory visit",
            unsorted_ok=True)
    if rng.chance(50):
        pay(day(year, month, rng.between(1, 28)), PETTY, "Books and Data Crystals",
            scaled(rng.between(1200, 3000), year), None, "Data crystals")
    if rng.chance(30):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Holodeck",
            scaled(rng.between(1500, 4200), year), None, "Holodeck evening")
    pay(workday(day(year, month, 6)), CHECKING, "Zero-G Sports", 2900 if year < 2024 else 3200,
        "Kepler Sports Club", "Club membership", unsorted_ok=True)
    if month in (4, 10):
        pay(day(year, month, 18), CHECKING, "Club Donations", 5000, "Kepler Sports Club", "Club donation")
    if month == 1:
        pay(workday(day(year, 1, 12)), CHECKING, "Books and Data Crystals", scaled(3600, year),
            "Venus Library", f"Library membership {year}")
    if rng.chance(15):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Books and Data Crystals",
            rng.between(150, 600), "Venus Library", "Late return fine")


def insurance_and_school(year, month):
    plan = {
        (1, 10): ("Hull Breach Cover", 94000, "Hull breach cover, yearly"),
        (2, 15): ("Suit Liability", 9600, "Suit liability, yearly"),
        (4, 10): ("Radiation Cover", 9800, "Radiation cover, half year"),
        (10, 10): ("Radiation Cover", 9800, "Radiation cover, half year"),
        (11, 15): ("Colonial Legal Cover", 17500, "Legal cover, yearly"),
    }
    for (when_month, when_day), (category, cents, memo) in plan.items():
        if when_month == month:
            pay(workday(day(year, month, when_day)), CHECKING, category,
                scaled(cents, year), "Helios Insurance", memo)
    if month in (1, 5, 9):
        pay(workday(day(year, month, 15)), CHECKING, "Academy Tuition", scaled(58000, year),
            "Phobos Academy", f"Tuition term {'1' if month == 1 else '2' if month == 5 else '3'}")


def saving_and_loan(year, month):
    move(workday(day(year, month, 20)), CHECKING, LOAN, 18000, "Loan repayment")
    move(workday(day(year, month, 2)), CHECKING, PETTY, 7000, "Cash for the month")
    saving = 17000
    move(workday(day(year, month, 27)), CHECKING, SAVINGS, saving, "Monthly saving")
    SAVINGS_BALANCE[0] += saving
    move(workday(day(year, month, 27)), CHECKING, TRAVEL, 22000, "Travel fund")
    if month in (3, 6, 9, 12):
        interest = SAVINGS_BALANCE[0] * SAVINGS_RATE_BP[year] // 40000
        when = workday(day(year, month, 30))
        if interest > 0 and START <= when <= END:
            earn(when, SAVINGS, "Credit Union Interest", interest, None, "Interest")
            SAVINGS_BALANCE[0] += interest


def food(year, month):
    rng = stream("food")
    festive = 140 if month == 12 else 100
    last = calendar.monthrange(year, month)[1]
    for number in range(1, last + 1):
        when = date(year, month, number)
        weekend = when.weekday() >= 5
        if weekend and rng.chance(70):
            pay(when, CHECKING, "Hydroponic Greens", percent(scaled(rng.between(1400, 4800), year), festive),
                "Ceres Greengrocer", rng.pick(GROCER_NOTES))
        elif not weekend and rng.chance(12):
            pay(when, CHECKING, "Hydroponic Greens", scaled(rng.between(600, 2200), year),
                "Ceres Greengrocer", rng.pick(GROCER_NOTES))
        if not weekend and rng.chance(26):
            pay(when, CHECKING, "Food", scaled(rng.between(700, 1400), year), "Phobos Canteen",
                "Lunch")
        if rng.chance(15):
            pay(when, PETTY if rng.chance(50) else CHECKING, "Algae Bars and Nutrient Paste",
                scaled(rng.between(300, 900), year), "Crater Bakery", "Bakery")
        if rng.chance(6):
            pay(when, PETTY, "Everyday Spending", scaled(rng.between(350, 900), year),
                "Callisto Cafe", "Coffee")
    # The big shop of the month, split between the food and the rest.
    shop = day(year, month, rng.between(1, 7))
    shop += timedelta(days=(5 - shop.weekday()) % 7)
    food_part = percent(scaled(rng.between(6500, 10500), year), festive)
    paste = scaled(rng.between(1800, 3200), year)
    home = scaled(rng.between(1200, 4200), year)
    add(
        shop,
        [
            ("Food", food_part, None, "Groceries"),
            ("Algae Bars and Nutrient Paste", paste, None, "Nutrient paste"),
            ("Everyday Spending", home, None, "Household"),
            (CHECKING, -(food_part + paste + home)),
        ],
        "Monthly shop",
        "Olympus Mons Market",
    )
    if month in (3, 4, 5) and rng.chance(75):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Hydroponic Greens",
            scaled(rng.between(2500, 7000), year), "Redwood Seed Bank", "Seeds for the new season")


def daily_life(year, month):
    rng = stream("daily")
    for _ in range(rng.between(2, 4)):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Rover and Vehicles",
            scaled(rng.between(2400, 4800), year), "Titan Fuel Station", "Fuel", unsorted_ok=True)
    pay(workday(day(year, month, 1)), CHECKING, "Rover Parking", scaled(1800, year),
        "Titan Fuel Station", "Dome parking")
    if month in (4, 10):
        pay(day(year, month, rng.between(5, 25)), CHECKING, "Rover Maintenance",
            scaled(rng.between(20000, 48000), year), "Rover Garage Gamma", "Rover service")
    for _ in range(rng.between(1, 2)):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Health",
            scaled(rng.between(600, 3800), year), "Luna Pharmacy", "Pharmacy", unsorted_ok=True)
    if rng.chance(30):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Health",
            scaled(rng.between(4000, 9000), year), "Moonbase Medical Center", "Check-up")
    if month in (3, 9):
        pay(day(year, month, rng.between(3, 25)), CHECKING, "Zero-G Dentist",
            scaled(rng.between(8500, 18000), year), "Zero-G Dental Clinic", "Dentist")
    if month in (10, 11):
        pay(day(year, month, rng.between(5, 25)), CHECKING, "Pressure Suits",
            scaled(rng.between(40000, 95000), year), "Suit Tailors Guild", "Pressure suit fitting")
    if rng.chance(25):
        category = rng.pick(["Gloves", "Boots", "Helmets", "Radios", "Tools"])
        pay(day(year, month, rng.between(1, 28)), CHECKING, category,
            scaled(rng.between(2500, 14000), year), "Galaxy Electronics", "Gear")
    for _ in range(rng.between(4, 8)):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Everyday Spending",
            scaled(rng.between(300, 2500), year), None, "Card payment")
    for _ in range(rng.between(1, 3)):
        pay(day(year, month, rng.between(1, 28)), PETTY, "Small Expenses",
            scaled(rng.between(200, 1500), year), None, "Kiosk")
    if rng.chance(20):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Minor Repairs",
            scaled(rng.between(2000, 9000), year), None, "Repair parts")
    pay(workday(day(year, month, 10)), CHECKING, "Mars Relief Fund",
        3000 if month != 12 else 15000, "Pioneer Fund", "Donation")
    if month in (2, 6, 9):
        pay(day(year, month, rng.between(1, 28)), CHECKING, "Birthdays",
            scaled(rng.between(2500, 7000), year), None, "Birthday gift")
    if month == 12:
        pay(day(year, 12, rng.between(10, 20)), CHECKING, "Council Party Donations",
            scaled(rng.between(2000, 4000), year), None, "Midwinter party")
        pay(day(year, 12, rng.between(10, 20)), CHECKING, "Birthdays",
            scaled(rng.between(6000, 12000), year), None, "Midwinter gifts")


def occasional(year, month):
    rng = stream("occasional")
    if month in (2, 6, 9, 11) and rng.chance(60):
        earn(day(year, month, rng.between(3, 25)), CHECKING, "Mineral Sales",
             scaled(rng.between(15000, 62000), year), None, "Ore sample sale")
    if month == 5 and year in (2022, 2024, 2026):
        earn(day(year, 5, 22), CHECKING, "Refund Oxygen Tax", rng.between(12000, 32000),
             "Colony Council Tax Office", "Tax refund")
    if month == 8 and year == 2023:
        earn(day(year, 8, 11), CHECKING, "Refund Radiation Shielding", 8500, "Helios Insurance",
             "Premium refund")


# ------------------------------------------------------------------- trips
# (first day away, nights, kind). The summer trips are paid from the travel
# fund, the short winter ones from the everyday account. Spending abroad is
# in US dollars from the dollar account, so a part has its own amount in
# dollars and the euro value of its category (D-043).

TRIPS = [
    (date(2022, 7, 23), 12, "summer"),
    (date(2023, 7, 22), 14, "summer"),
    (date(2023, 12, 20), 5, "winter"),
    (date(2024, 8, 3), 12, "summer"),
    (date(2025, 7, 26), 14, "summer"),
    (date(2025, 12, 20), 6, "winter"),
    (date(2026, 7, 25), 13, "summer"),
]
WALLET_USD = [0]


def ceil_to(value, step):
    return -(-value // step) * step


def trip(start, nights, kind):
    rng = stream("trip")
    stay_end = start + timedelta(days=nights)
    booked = start - timedelta(days=rng.between(40, 60))
    cents = rng.between(220000, 270000) if kind == "summer" else rng.between(120000, 160000)
    pay(workday(booked), TRAVEL if kind == "summer" else CHECKING, "Earth-Mars Flights",
        scaled(cents, booked.year), "Andromeda Travel Agency",
        f"Return flights {MONTHS[start.month]} {start.year}")
    spending = []
    for number in range(nights + 1):
        when = start + timedelta(days=number)
        spending.append((when, "Meals on the Road", "Terra Diner", rng.between(2200, 5800), "Dinner"))
        if rng.chance(35):
            spending.append((when, "Meals on the Road", "Terra Diner", rng.between(900, 2200), "Lunch"))
    for _ in range(rng.between(2, 4)):
        when = start + timedelta(days=rng.between(1, nights - 1))
        spending.append((when, "Olympus Mons Tours", "Earth Tour Company", rng.between(6500, 14000), "Day tour"))
    for _ in range(rng.between(3, 5)):
        when = start + timedelta(days=rng.between(max(1, nights - 4), nights))
        spending.append((when, "Souvenirs", "Earth Souvenir Bazaar", rng.between(900, 6000), "Souvenirs"))
    spending.append((stay_end, "Orbital Hotel", "Selene Orbital Hotel",
                     nights * rng.between(9500, 14000), f"Hotel, {nights} nights"))
    spending.sort(key=lambda item: item[0])
    need = sum(item[3] for item in spending) + rng.between(3000, 20000)
    first = ceil_to(need * 7 // 10, 5000)
    second = ceil_to(need - first, 5000)
    for when, usd in ((start - timedelta(days=3), first), (start + timedelta(days=nights // 2), second)):
        eur = to_eur(usd, when.year, when.month)
        add(when, [(CHECKING, -eur), (WALLET, eur, usd)], "Dollars for the trip", None)
    for when, category, payee, usd, memo in spending:
        eur = to_eur(usd, when.year, when.month)
        add(when, [(category, usd, eur, memo), (WALLET, -usd)], memo, payee, currency="USD")
    if rng.chance(45):
        earn(stay_end + timedelta(days=rng.between(20, 40)), CHECKING, "Refund Shuttle Fares",
             rng.between(4000, 9500), "Andromeda Travel Agency", "Fare refund")


def one_offs(year, month):
    if (year, month) == (2024, 3):
        move(date(2024, 3, 18), SAVINGS, CHECKING, 150000, "For the rover battery")
        SAVINGS_BALANCE[0] -= 150000
        pay(date(2024, 3, 19), CHECKING, "Rover Batteries", 138000, "Rover Garage Gamma",
            "New battery pack")
    if (year, month) == (2023, 6):
        pay(date(2023, 6, 9), CHECKING, "Tools", 38990, "Galaxy Electronics", "Welding rig")
    if (year, month) == (2025, 2):
        pay(date(2025, 2, 13), CHECKING, "Zero-G Dentist", 42000, "Zero-G Dental Clinic", "Crown")
    if (year, month) == (2026, 10):
        pay(END, CHECKING, "Rover and Vehicles", 3980, "Titan Fuel Station", "Fuel", unsorted_ok=True)
    if (year, month) == (2026, 11):
        # The first Sunday of the month, so "this month" has a little life
        # on the day the book was refreshed (the generator's own events
        # fall on working days or later).
        pay(END, CHECKING, "Dome 3 Cinema", 1450, "Aurora Dome Cinema", "Sunday matinee")
        pay(END, CHECKING, "Holodeck", 2850, None, "Holodeck evening")
        pay(END, CHECKING, "Observatory Visits", 1650, "Tycho Observatory", "Observatory visit")
    if (year, month) == (2026, 5):
        pay(date(2026, 5, 12), CHECKING, "Zero-G Dentist", 62000, "Zero-G Dental Clinic", "Dental work")
    if (year, month) == (2026, 2):
        pay(date(2026, 2, 6), CHECKING, "Minor Repairs", 124000, None, "Airlock seal repair")


OPENING_FLOOR = 50000  # every account that can't go below zero stays above 500.00
OPENING_BASE = {CHECKING: 400000, SAVINGS: 950000, PETTY: 8000, TRAVEL: 120000}
LOAN_AT_START = 1500000
SAVINGS_BALANCE[0] = OPENING_BASE[SAVINGS]

# What the older book's entries are about, by the text they carry, and who
# a payee of that kind is: used only where the entry's own category is a
# catch-all, so the payee never contradicts the category.
CATCH_ALL = {"Everyday Spending", "Miscellaneous", "Small Expenses", "Other Spending"}
MEMO_PAYEE = {
    "Algae bars": "Crater Bakery", "Reactor spare parts": "Galaxy Electronics",
    "Habitat supplies": "Olympus Mons Market", "Rover check-up": "Rover Garage Gamma",
    "Water allotment": "Northern Ice Works", "Seed order": "Redwood Seed Bank",
    "Library fine": "Venus Library", "Fuel top-up": "Titan Fuel Station",
    "Hull patch kit": "Galaxy Electronics", "Zero-G class": "Kepler Sports Club",
    "Subspace fee": "Subspace Telecom", "Suit repair": "Suit Tailors Guild",
    "Canteen lunch": "Phobos Canteen", "Greens, week": "Ceres Greengrocer",
    "Dome rent share": "Dome Management Co.", "Oxygen refill": "Europa Water Board",
    "Data crystals": "Venus Library", "Cinema night": "Aurora Dome Cinema",
    "Radiation badge": "Moonbase Medical Center", "Shuttle pass": "Andromeda Travel Agency",
}
OWN_ACCOUNTS = (CHECKING, SAVINGS, PETTY, TRAVEL, LOAN, EQUITY, WALLET,
                "Ceres Savings Cooperative", "Lunar Credit Union", "Asteroid Claim Shares")


def fitting_payee(category, memo, rng):
    """A payee that suits the category an older entry is booked to."""
    if category in CATCH_ALL:
        theme = "".join(ch for ch in memo if not ch.isdigit()).strip()
        return MEMO_PAYEE.get(theme)
    suited = [name for name, default in {**PAYEE_DEFAULTS, **NEW_PAYEES}.items()
              if default == category]
    return rng.pick(suited) if suited else None


def older_entries():
    """The older book's entries up to END, as the same made-up life: dates,
    amounts, accounts, categories and texts as they were, with a payee that
    fits the category. Entries after END are left out."""
    rng = stream("older-payee")
    db = sqlite3.connect(SEED)
    names = dict(db.execute("SELECT id, accountName FROM kmmAccounts"))
    kept = 0
    for ident, when, memo in db.execute(
            "SELECT id, postDate, memo FROM kmmTransactions ORDER BY postDate, id"):
        when = date.fromisoformat(when)
        if when > END:
            continue
        parts = []
        for account, value, shares, note in db.execute(
                "SELECT accountId, value, shares, memo FROM kmmSplits "
                "WHERE transactionId = ? ORDER BY splitId", (ident,)):
            # Cents, from KMyMoney's fractions of a hundred.
            assert value.endswith("/100") and shares.endswith("/100"), value
            parts.append((names[account], int(value[:-4]), int(shares[:-4]), note))
        others = [part[0] for part in parts if part[0] not in OWN_ACCOUNTS]
        payee = fitting_payee(others[0], memo, rng) if len(others) == 1 else None
        add(when, parts, memo, payee)
        kept += 1
    db.close()
    return kept


def opening():
    """What the household owned and owed on the first day, with enough in
    each account that none of them runs dry at any time."""
    running = {name: 0 for name in OPENING_BASE}
    low = dict(running)
    for tx in TX:
        for name, _, shares, _ in tx["parts"]:
            if name in running:
                running[name] += shares
                low[name] = min(low[name], running[name])
    assets = [(name, max(base, OPENING_FLOOR - low[name])) for name, base in OPENING_BASE.items()]
    balance = sum(cents for _, cents in assets) - LOAN_AT_START
    parts = [(name, cents) for name, cents in assets]
    parts.append((LOAN, -LOAN_AT_START))
    parts.append((EQUITY, -balance))
    add(START, parts, "Opening balances", None)
    TX[-1]["seq"] = 0


def build():
    for year, month in months():
        salary(year, month)
        housing(year, month)
        connected_life(year, month)
        insurance_and_school(year, month)
        one_offs(year, month)
        saving_and_loan(year, month)
        food(year, month)
        daily_life(year, month)
        occasional(year, month)
    for start, nights, kind in TRIPS:
        trip(start, nights, kind)
    older_entries()
    opening()
    TX.sort(key=lambda tx: (tx["date"], tx["seq"]))
    rng = stream("unsorted")
    for tx in TX:
        # The newest lines are what the bank delivered and nobody has sorted
        # yet: a part in the everyday account and nothing else.
        tx["imported"] = False
        if (
            tx["date"] >= UNSORTED_FROM
            and tx["unsorted_ok"]
            and len(tx["parts"]) == 2
            and any(part[0] == CHECKING for part in tx["parts"])
            and rng.chance(85)
        ):
            tx["parts"] = [part for part in tx["parts"] if part[0] == CHECKING]
            tx["imported"] = True


def balances():
    totals = {}
    low = {}
    for tx in TX:
        for name, _, shares, _ in tx["parts"]:
            totals[name] = totals.get(name, 0) + shares
            low[name] = min(low.get(name, 0), totals[name])
    return totals, low


# ------------------------------------------------------------------ writing

ASSETS_WITH_STATEMENT = (CHECKING, SAVINGS)


def iso(when):
    return when.isoformat()


def month_end(when):
    return day(when.year, when.month, 31)


def price_text(value, shares):
    ratio = (shares * 10000 + value // 2) // value
    return f"{ratio // 10000}.{ratio % 10000:04d}"


def write(path):
    work = tempfile.mkdtemp(prefix="space-colony-")
    tmp = os.path.join(work, "book.sqlite")
    shutil.copyfile(SEED, tmp)
    db = sqlite3.connect(tmp)
    db.execute("PRAGMA journal_mode = OFF")
    names = [row[0] for row in db.execute("SELECT accountName FROM kmmAccounts")]
    if len(names) != len(set(names)):
        sys.exit("the older book has two accounts with the same name")
    account_id = {name: ident for ident, name in
                  db.execute("SELECT id, accountName FROM kmmAccounts")}

    # Start from the older book's skeleton without its entries.
    db.execute("DELETE FROM kmmSplits")
    db.execute("DELETE FROM kmmTransactions")
    db.execute("DELETE FROM kmmKeyValuePairs WHERE kvpType = 'TRANSACTION'")
    db.execute("DELETE FROM kmmKeyValuePairs WHERE kvpType = 'ACCOUNT' AND kvpKey IN "
               "('lastImportedTransactionDate', 'lastStatementBalance')")

    # The dollar account, its currency and the rates between the two.
    usd = sqlite3.connect(SAMPLE).execute(
        "SELECT * FROM kmmCurrencies WHERE ISOcode = 'USD'").fetchone()
    db.execute("INSERT INTO kmmCurrencies VALUES (?,?,?,?,?,?,?,?,?,?,?)", usd)
    account_id[WALLET] = "A000086"
    db.execute(
        "INSERT INTO kmmAccounts VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        ("A000086", "I000001", "A000002", "", "", iso(START), "", "1", "Girokonto", "N",
         WALLET, "", "USD", "0/1", "0", 0))
    for year, month in months():
        rate = usd_per_eur(year, month)
        # Euro for one dollar, to four places: a rate has to be a finite decimal.
        eur_per_usd = (10**8 + rate // 2) // rate
        db.execute("INSERT INTO kmmPrices VALUES (?,?,?,?,?,?)",
                   ("USD", "EUR", iso(date(year, month, 1)), f"{eur_per_usd}/10000",
                    f"{eur_per_usd // 10000},{eur_per_usd % 10000:04d}", "User"))

    # Payees: the right default category for each, and the new ones.
    for name, category in PAYEE_DEFAULTS.items():
        found = db.execute("UPDATE kmmPayees SET defaultAccountId = ? WHERE name = ?",
                           (account_id[category] if category else "", name)).rowcount
        if found != 1:
            sys.exit(f"payee {name} is not in the older book")
    payee_id = {name: ident for ident, name in db.execute("SELECT id, name FROM kmmPayees")}
    next_payee = 121
    for name, category in NEW_PAYEES.items():
        ident = f"P{next_payee:06d}"
        next_payee += 1
        keys = "^" + name.replace(" ", "\\ ") + "$"
        db.execute("INSERT INTO kmmPayees VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                   (ident, name, "", "", "", "", "", "", "", "", account_id[category],
                    2, "Y", keys, "", ""))
        payee_id[name] = ident
    payee_total = db.execute("SELECT COUNT(*) FROM kmmPayees").fetchone()[0]

    # The entries.
    totals, counts, bank_numbers = {}, {}, [0]
    splits = 0
    for number, tx in enumerate(TX, start=1):
        ident = f"T{number:018d}"
        when = tx["date"]
        db.execute("INSERT INTO kmmTransactions VALUES (?,?,?,?,?,?,?)",
                   (ident, "N", iso(when), tx["memo"], iso(when), tx["currency"], ""))
        payee = payee_id[tx["payee"]] if tx["payee"] else ""
        for index, (name, value, shares, note) in enumerate(tx["parts"]):
            flag, reconciled, bank = "0", "", ""
            if name in ASSETS_WITH_STATEMENT and not tx["imported"]:
                if when <= LAST_RECONCILED:
                    flag, reconciled = "2", iso(min(month_end(when), LAST_RECONCILED))
                elif name == CHECKING and when <= LAST_CLEARED:
                    flag = "1"
            if name == CHECKING and when >= date(2022, 6, 1):
                bank_numbers[0] += 1
                bank = f"ID-{bank_numbers[0]}"
            memo = note or (tx["memo"] if name in (CHECKING, SAVINGS, PETTY, TRAVEL, WALLET) else "")
            db.execute(
                "INSERT INTO kmmSplits VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (ident, "N", index, payee, reconciled, "", flag,
                 f"{value}/100", german(value), f"{shares}/100", german(shares),
                 f"{shares}/{value}" if value else "1/1",
                 price_text(abs(value), abs(shares)) if value else "1.0000",
                 memo, account_id[name], "", "", iso(when), bank))
            totals[name] = totals.get(name, 0) + shares
            counts[name] = counts.get(name, 0) + 1
            splits += 1
        if tx["imported"]:
            db.execute("INSERT INTO kmmKeyValuePairs VALUES ('TRANSACTION', ?, 'Imported', 'true')",
                       (ident,))

    # What the older book stored beside the entries, brought up to date.
    for name, ident in account_id.items():
        total = totals.get(name, 0)
        db.execute("UPDATE kmmAccounts SET balance = ?, balanceFormatted = ?, transactionCount = ? "
                   "WHERE id = ?", (f"{total}/100", german(total), counts.get(name, 0), ident))
    for name in (CHECKING, SAVINGS, PETTY, TRAVEL, LOAN, EQUITY, WALLET):
        db.execute("UPDATE kmmAccounts SET openingDate = ?, lastModified = ? WHERE id = ?",
                   (iso(START), iso(END), account_id[name]))
    for name in ASSETS_WITH_STATEMENT:
        db.execute("UPDATE kmmAccounts SET lastReconciled = ? WHERE id = ?",
                   (iso(LAST_RECONCILED), account_id[name]))
    db.execute("UPDATE kmmFileInfo SET created = ?, lastModified = ?, logonAt = ?, accounts = ?, "
               "payees = ?, transactions = ?, splits = ?, currencies = 2, hiAccountId = 86, "
               "hiPayeeId = ?, hiTransactionId = ?",
               (iso(START), iso(END), iso(END) + "T08:00:00", len(account_id), payee_total,
                len(TX), splits, next_payee, len(TX) + 1))
    db.execute("UPDATE kmmBudgetConfig SET start = ?", (iso(date(2026, 1, 1)),))
    db.execute("UPDATE kmmKeyValuePairs SET kvpData = ? WHERE kvpKey = 'CreationDate'", (iso(START),))
    db.execute("UPDATE kmmKeyValuePairs SET kvpData = ? WHERE kvpKey = 'LastModificationDate'", (iso(END),))
    db.commit()
    db.execute("VACUUM")
    db.close()
    os.makedirs(os.path.dirname(path), exist_ok=True)
    shutil.copyfile(tmp, path)
    shutil.rmtree(work)
    return len(TX), splits


def content(path):
    """Everything a book holds, as text. The file's bytes also carry the
    version of the SQLite library that wrote it, so `--check` compares this."""
    db = sqlite3.connect(path)
    try:
        return list(db.iterdump())
    finally:
        db.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--out", default=OUT, help="where to write the book")
    parser.add_argument("--check", action="store_true",
                        help="write to a temporary file and fail if the committed book differs")
    args = parser.parse_args()
    build()
    totals, lows = balances()
    for name in (CHECKING, SAVINGS, PETTY, TRAVEL, WALLET):
        if totals[name] < 0:
            sys.exit(f"{name} ends below zero")
    if args.check:
        scratch = os.path.join(tempfile.mkdtemp(prefix="space-colony-check-"), "check.sqlite")
        write(scratch)
        same = content(scratch) == content(args.out)
        shutil.rmtree(os.path.dirname(scratch))
        if not same:
            sys.exit(f"{args.out} is not what this script writes; run it again and commit the result")
        print("the committed book is what the script writes")
        return
    count, splits = write(args.out)
    print(f"{count} transactions, {splits} splits, {START} to {END}: {args.out}")


if __name__ == "__main__":
    main()
