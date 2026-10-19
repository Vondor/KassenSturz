// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The book's database layout, documented in `docs/BOOK_FORMAT.md` (D-046).
//!
//! Until 0.1 this is a development layout that may change freely; before 0.1
//! all development changes are merged into one starting layout (D-045).

/// The layout version, stored in SQLite's `user_version`. Version 2 added the
/// book's settings (M1.1).
pub(crate) const VERSION: i32 = 2;

/// The highest number of decimal places a currency may have. 10^18 still fits
/// into 64 bits; ISO 4217 currencies use at most 4.
pub(crate) const MAX_MINOR_DIGITS: u8 = 18;

/// Creates all tables of an empty book.
///
/// Every table is `STRICT`, so SQLite refuses values of the wrong type
/// instead of converting them. Amounts are INTEGER minor units, never REAL
/// (AGENTS.md §3.1 rule 1). IDs are the 16 bytes of a UUIDv7 (D-067), dates
/// ISO 8601 text (D-061).
///
/// The one row of `settings` is added by the code that creates a book, with
/// KassenSturz's defaults.
pub(crate) const CREATE: &str = "
CREATE TABLE settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    rounding_mode TEXT NOT NULL CHECK (rounding_mode IN (
        'half_away_from_zero', 'half_even', 'half_toward_zero',
        'toward_zero', 'away_from_zero', 'floor', 'ceiling'
    ))
) STRICT;

CREATE TABLE currencies (
    code TEXT PRIMARY KEY NOT NULL
        CHECK (length(code) = 3 AND code NOT GLOB '*[^A-Z]*'),
    minor_digits INTEGER NOT NULL CHECK (minor_digits BETWEEN 0 AND 18)
) STRICT;

CREATE TABLE accounts (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('asset', 'liability', 'income', 'expense', 'equity')),
    currency TEXT NOT NULL REFERENCES currencies (code)
) STRICT;

CREATE TABLE transactions (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    booking_date TEXT NOT NULL
        CHECK (booking_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    payee TEXT,
    currency TEXT NOT NULL REFERENCES currencies (code)
) STRICT;

CREATE TABLE splits (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    account_id BLOB NOT NULL REFERENCES accounts (id),
    amount INTEGER NOT NULL,
    UNIQUE (transaction_id, position)
) STRICT;

CREATE INDEX splits_by_account ON splits (account_id);
";
