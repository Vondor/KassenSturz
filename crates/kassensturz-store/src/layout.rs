// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The book's database layout and the migrations that build it, documented in
//! `docs/BOOK_FORMAT.md` (D-046).
//!
//! Every book is built by the same migrations, starting from an empty
//! database: a new book runs all of them, an older one the ones it lacks
//! (D-045). Until 0.1 these are development migrations that may change
//! freely; before 0.1 they are merged into one starting layout.

use rusqlite::Connection;

use crate::error::{Corruption, StoreError};

/// KassenSturz's SQLite application ID (`PRAGMA application_id`), which marks
/// a file as a household book whatever it's called (D-037): the letters
/// `KSZB`, for KassenSturz book. Neither SQLite's list of application IDs
/// (`magic.txt`) nor `file`'s lists it for another program (checked on
/// 2026-10-19).
pub(crate) const APPLICATION_ID: i32 = i32::from_be_bytes(*b"KSZB");

/// The migrations, oldest first. The first builds layout [`FIRST_VERSION`]
/// in an empty database; each one after it takes a book from one layout
/// version to the next.
///
/// A migration is never changed once a book may have been written with it,
/// so it contains fixed values, never KassenSturz's current defaults: a book
/// gets the same layout whether it was created today or upgraded later.
/// Before 0.1 the development migrations are merged into one (D-045), and
/// `FIRST_VERSION` moves past them, so development books are turned away.
///
/// A migration that changes a table the change history records also changes
/// its `changed_` table the same way and converts the rows kept there, so
/// changes made before the update can still be undone (D-066); so does the
/// list of recorded tables in `history.rs`.
pub(crate) const MIGRATIONS: [&str; 14] = [
    LAYOUT_1, LAYOUT_2, LAYOUT_3, LAYOUT_4, LAYOUT_5, LAYOUT_6, LAYOUT_7, LAYOUT_8, LAYOUT_9,
    LAYOUT_10, LAYOUT_11, LAYOUT_12, LAYOUT_13, LAYOUT_14,
];

/// The layout version the first migration creates.
pub(crate) const FIRST_VERSION: i32 = 1;

/// The current layout version, stored in SQLite's `user_version`. The list
/// of migrations is far too short for the conversion to lose anything.
pub(crate) const VERSION: i32 = FIRST_VERSION + MIGRATIONS.len() as i32 - 1;

/// Layout 1, the M0 walking skeleton (D-068): currencies, accounts,
/// transactions and their parts.
///
/// Every table is `STRICT`, so SQLite refuses values of the wrong type
/// instead of converting them. Amounts are INTEGER minor units, never REAL
/// (AGENTS.md §3.1 rule 1). IDs are the 16 bytes of a UUIDv7 (D-067), dates
/// ISO 8601 text (D-061).
const LAYOUT_1: &str = "
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

/// Layout 2 (M1.1): the book's settings, one row, with the rounding mode set
/// to round half away from zero (D-027).
const LAYOUT_2: &str = "
CREATE TABLE settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    rounding_mode TEXT NOT NULL CHECK (rounding_mode IN (
        'half_away_from_zero', 'half_even', 'half_toward_zero',
        'toward_zero', 'away_from_zero', 'floor', 'ceiling'
    ))
) STRICT;

INSERT INTO settings (id, rounding_mode) VALUES (1, 'half_away_from_zero');
";

/// Layout 3 (M1.2): the price table for exchange rates (D-028). Rates are two
/// INTEGERs, their digits and decimal places (D-051).
const LAYOUT_3: &str = "
CREATE TABLE prices (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    date TEXT NOT NULL
        CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    from_currency TEXT NOT NULL REFERENCES currencies (code),
    to_currency TEXT NOT NULL REFERENCES currencies (code),
    rate_digits INTEGER NOT NULL CHECK (rate_digits BETWEEN 1 AND 999999999999999999),
    rate_decimal_places INTEGER NOT NULL CHECK (rate_decimal_places BETWEEN 0 AND 18),
    source TEXT NOT NULL CHECK (source IN ('manual')),
    CHECK (from_currency <> to_currency),
    UNIQUE (from_currency, to_currency, date)
) STRICT;
";

/// Layout 4 (M1.4): what sort of account an account is, and whether it is
/// closed. The first migration of a book that already exists; the old
/// accounts get no type and stay open.
const LAYOUT_4: &str = "
ALTER TABLE accounts ADD COLUMN account_type TEXT
    CHECK (account_type IN ('checking', 'savings', 'cash', 'credit_card'));
ALTER TABLE accounts ADD COLUMN closed INTEGER NOT NULL DEFAULT 0
    CHECK (closed IN (0, 1));
";

/// Layout 5 (M1.5): categories form a tree. A category says which category
/// it sits inside; only income and expense accounts can.
const LAYOUT_5: &str = "
ALTER TABLE accounts ADD COLUMN parent_id BLOB REFERENCES accounts (id)
    CHECK (parent_id IS NULL OR (
        length(parent_id) = 16 AND parent_id <> id AND kind IN ('income', 'expense')
    ));
CREATE INDEX accounts_by_parent ON accounts (parent_id);
";

/// Layout 6 (M1.6): transactions and their parts in full (D-083). A part can
/// be without a category yet, so `splits.account_id` may be NULL, which
/// needs the table to be rebuilt (sqlite.org/lang_altertable.html); its rows
/// are copied unchanged. A part in another currency keeps its amount there,
/// with the rate it was converted with in `split_rates` (D-043). What a bank
/// or file delivered for a part is kept in `split_imports` (D-067).
const LAYOUT_6: &str = "
CREATE TABLE new_splits (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    account_id BLOB REFERENCES accounts (id),
    amount INTEGER NOT NULL,
    foreign_amount INTEGER,
    foreign_currency TEXT REFERENCES currencies (code),
    CHECK ((foreign_amount IS NULL) = (foreign_currency IS NULL)),
    CHECK (foreign_amount IS NULL OR account_id IS NOT NULL),
    UNIQUE (transaction_id, position)
) STRICT;
INSERT INTO new_splits (id, transaction_id, position, account_id, amount)
    SELECT id, transaction_id, position, account_id, amount FROM splits;
DROP TABLE splits;
ALTER TABLE new_splits RENAME TO splits;
CREATE INDEX splits_by_account ON splits (account_id);

CREATE TABLE split_rates (
    split_id BLOB PRIMARY KEY NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    rate_id BLOB NOT NULL CHECK (length(rate_id) = 16),
    date TEXT NOT NULL
        CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    from_currency TEXT NOT NULL REFERENCES currencies (code),
    to_currency TEXT NOT NULL REFERENCES currencies (code),
    rate_digits INTEGER NOT NULL CHECK (rate_digits BETWEEN 1 AND 999999999999999999),
    rate_decimal_places INTEGER NOT NULL CHECK (rate_decimal_places BETWEEN 0 AND 18),
    source TEXT NOT NULL CHECK (source IN ('manual')),
    CHECK (from_currency <> to_currency)
) STRICT;

CREATE TABLE split_imports (
    split_id BLOB PRIMARY KEY NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    bank_id TEXT,
    booking_date TEXT
        CHECK (booking_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    value_date TEXT
        CHECK (value_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    other_party_name TEXT,
    other_party_iban TEXT,
    remittance_information TEXT
) STRICT;
";

/// Layout 7 (M1.9): the change history (D-066, D-086). How long it is kept
/// is a setting of the book: a number of days, or forever (`NULL`); new and
/// updated books keep it for 30 days. Each user action is one row of
/// `changes`, numbered in the order the actions were made. The rows it
/// touched are copied into the `changed_` table of their table, once with
/// their values before the action and once with those after it; a row it
/// added has no values before, one it removed none after. The copies have
/// the same columns as the table, without its checks, which apply when the
/// values are written back.
const LAYOUT_7: &str = "
CREATE TABLE history_settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    keep_days INTEGER CHECK (keep_days BETWEEN 0 AND 4294967295)
) STRICT;

INSERT INTO history_settings (id, keep_days) VALUES (1, 30);

CREATE TABLE changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1))
) STRICT;

CREATE TABLE changed_settings (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id INTEGER NOT NULL,
    rounding_mode TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_currencies (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    code TEXT NOT NULL,
    minor_digits INTEGER,
    PRIMARY KEY (change_id, code, side)
) STRICT;

CREATE TABLE changed_accounts (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    kind TEXT,
    currency TEXT,
    account_type TEXT,
    closed INTEGER,
    parent_id BLOB,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_transactions (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    booking_date TEXT,
    payee TEXT,
    currency TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_splits (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    transaction_id BLOB,
    position INTEGER,
    account_id BLOB,
    amount INTEGER,
    foreign_amount INTEGER,
    foreign_currency TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_split_rates (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    split_id BLOB NOT NULL,
    rate_id BLOB,
    date TEXT,
    from_currency TEXT,
    to_currency TEXT,
    rate_digits INTEGER,
    rate_decimal_places INTEGER,
    source TEXT,
    PRIMARY KEY (change_id, split_id, side)
) STRICT;

CREATE TABLE changed_split_imports (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    split_id BLOB NOT NULL,
    bank_id TEXT,
    booking_date TEXT,
    value_date TEXT,
    other_party_name TEXT,
    other_party_iban TEXT,
    remittance_information TEXT,
    PRIMARY KEY (change_id, split_id, side)
) STRICT;

CREATE TABLE changed_prices (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    date TEXT,
    from_currency TEXT,
    to_currency TEXT,
    rate_digits INTEGER,
    rate_decimal_places INTEGER,
    source TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
";

/// Layout 8 (M1.10): joining the two sides of a Transfer is a user action
/// of its own (D-087), so `changes` is rebuilt with `join_transfer` in its
/// list of actions. Its rows stay as they were; the `changed_` tables refer
/// to it by name and keep their rows too.
const LAYOUT_8: &str = "
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1))
) STRICT;
INSERT INTO new_changes (id, sequence, action, made_at, undone)
    SELECT id, sequence, action, made_at, undone FROM changes;
DROP TABLE changes;
ALTER TABLE new_changes RENAME TO changes;
";

/// Layout 9 (M1.11): Rules (D-014, D-031, D-088). `rules` holds the tree of
/// Rules with their actions, `rule_conditions` their conditions, and
/// `split_rules` marks each part whose category a Rule set, so running the
/// Rules again may change it while a part the user set stays as it is. All
/// three are recorded by the change history, so their `changed_` tables come
/// too, and `changes` is rebuilt with the new actions in its list (D-086);
/// its rows stay.
const LAYOUT_9: &str = "
CREATE TABLE rules (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    parent_id BLOB REFERENCES rules (id) ON DELETE CASCADE
        CHECK (parent_id IS NULL OR (length(parent_id) = 16 AND parent_id <> id)),
    position INTEGER NOT NULL CHECK (position >= 0),
    name TEXT NOT NULL,
    match_mode TEXT NOT NULL CHECK (match_mode IN ('all', 'any')),
    category_id BLOB REFERENCES accounts (id),
    transfer_account_id BLOB REFERENCES accounts (id),
    payee TEXT,
    CHECK (category_id IS NULL OR transfer_account_id IS NULL)
) STRICT;
CREATE INDEX rules_by_parent ON rules (parent_id);

CREATE TABLE rule_conditions (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    rule_id BLOB NOT NULL REFERENCES rules (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    field TEXT NOT NULL CHECK (field IN (
        'other_party_name', 'other_party_iban', 'remittance_information', 'payee',
        'amount', 'booking_date', 'currency', 'account'
    )),
    operator TEXT NOT NULL CHECK (operator IN (
        'contains', 'equals', 'starts_with', 'ends_with', 'matches',
        'between', 'money_in', 'money_out', 'is'
    )),
    text_value TEXT,
    case_sensitive INTEGER CHECK (case_sensitive IN (0, 1)),
    amount_min INTEGER,
    amount_max INTEGER,
    currency TEXT REFERENCES currencies (code),
    date_first TEXT
        CHECK (date_first GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    date_last TEXT
        CHECK (date_last GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    account_id BLOB REFERENCES accounts (id),
    UNIQUE (rule_id, position)
) STRICT;

CREATE TABLE split_rules (
    split_id BLOB PRIMARY KEY NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    rule_id BLOB REFERENCES rules (id) ON DELETE SET NULL
) STRICT;
CREATE INDEX split_rules_by_rule ON split_rules (rule_id);

CREATE TABLE changed_rules (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    parent_id BLOB,
    position INTEGER,
    name TEXT,
    match_mode TEXT,
    category_id BLOB,
    transfer_account_id BLOB,
    payee TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_rule_conditions (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    rule_id BLOB,
    position INTEGER,
    field TEXT,
    operator TEXT,
    text_value TEXT,
    case_sensitive INTEGER,
    amount_min INTEGER,
    amount_max INTEGER,
    currency TEXT,
    date_first TEXT,
    date_last TEXT,
    account_id BLOB,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_split_rules (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    split_id BLOB NOT NULL,
    rule_id BLOB,
    PRIMARY KEY (change_id, split_id, side)
) STRICT;

CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1))
) STRICT;
INSERT INTO new_changes (id, sequence, action, made_at, undone)
    SELECT id, sequence, action, made_at, undone FROM changes;
DROP TABLE changes;
ALTER TABLE new_changes RENAME TO changes;
";

/// Layout 10 (M2.2): importing a statement into an account is a user action
/// of its own (D-092), so `changes` is rebuilt with `import_statement` in its
/// list of actions, as in layouts 8 and 9. Its rows stay as they were.
const LAYOUT_10: &str = "
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1))
) STRICT;
INSERT INTO new_changes (id, sequence, action, made_at, undone)
    SELECT id, sequence, action, made_at, undone FROM changes;
DROP TABLE changes;
ALTER TABLE new_changes RENAME TO changes;
";

/// Layout 11 (M2.9b): a part converted from a withdrawn currency, such as an
/// amount in German marks, keeps the amount it had and its currency in two
/// new columns of `splits`, both set or both empty (D-077, D-104, Q-064).
/// The fixed rate it was converted with is kept in `split_rates`, like the
/// rate of a foreign amount, so that table is rebuilt with `changeover` in
/// its list of sources; its rows are copied unchanged. `changed_splits` gets
/// the two columns too, empty in the rows it has, as the parts had no
/// original amount then; `changed_split_rates` doesn't check its sources.
const LAYOUT_11: &str = "
ALTER TABLE splits ADD COLUMN original_amount INTEGER;
ALTER TABLE splits ADD COLUMN original_currency TEXT REFERENCES currencies (code)
    CHECK ((original_amount IS NULL) = (original_currency IS NULL));

CREATE TABLE new_split_rates (
    split_id BLOB PRIMARY KEY NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    rate_id BLOB NOT NULL CHECK (length(rate_id) = 16),
    date TEXT NOT NULL
        CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    from_currency TEXT NOT NULL REFERENCES currencies (code),
    to_currency TEXT NOT NULL REFERENCES currencies (code),
    rate_digits INTEGER NOT NULL CHECK (rate_digits BETWEEN 1 AND 999999999999999999),
    rate_decimal_places INTEGER NOT NULL CHECK (rate_decimal_places BETWEEN 0 AND 18),
    source TEXT NOT NULL CHECK (source IN ('manual', 'changeover')),
    CHECK (from_currency <> to_currency)
) STRICT;
INSERT INTO new_split_rates
    (split_id, rate_id, date, from_currency, to_currency,
     rate_digits, rate_decimal_places, source)
    SELECT split_id, rate_id, date, from_currency, to_currency,
           rate_digits, rate_decimal_places, source
    FROM split_rates;
DROP TABLE split_rates;
ALTER TABLE new_split_rates RENAME TO split_rates;

ALTER TABLE changed_splits ADD COLUMN original_amount INTEGER;
ALTER TABLE changed_splits ADD COLUMN original_currency TEXT;
";

/// Layout 12 (#44): a part can have a date and a payee of its own, which its
/// account's register shows instead of the transaction's. Joining a Transfer
/// gives them to the second side's part, so each account keeps showing the
/// day its own bank booked the money and the payee it had (D-108). Both are
/// empty in the parts a book has, which show their transaction's as before;
/// `changed_splits` gets the two columns too, empty in the rows it has.
const LAYOUT_12: &str = "
ALTER TABLE splits ADD COLUMN own_date TEXT
    CHECK (own_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');
ALTER TABLE splits ADD COLUMN own_payee TEXT;

ALTER TABLE changed_splits ADD COLUMN own_date TEXT;
ALTER TABLE changed_splits ADD COLUMN own_payee TEXT;
";

/// Layout 13 (#51): a change keeps the name of the file it read, such as the
/// statement file of an import, without its folder, so undo can say which
/// import it takes back (D-109, Q-070). Changes made before have none.
const LAYOUT_13: &str = "
ALTER TABLE changes ADD COLUMN file_name TEXT;
";

/// Layout 14 (M3.4a): an account keeps the balance the user stated for it,
/// taken from the bank, and the day it was for, in two new columns of
/// `accounts`, both set or both empty, and only on accounts, never on
/// categories or the internal account (D-111, D-117, Q-072). The accounts a
/// book has get none: KassenSturz knows their stated balance only from their
/// starting-balance transaction. `changed_accounts` gets the two columns too,
/// empty in the rows it has. Correcting the starting balance becomes a user
/// action of its own, so `changes` is rebuilt with `correct_starting_balance`
/// in its list of actions, as in layouts 8 to 10; its rows stay as they were.
const LAYOUT_14: &str = "
ALTER TABLE accounts ADD COLUMN stated_balance INTEGER
    CHECK (stated_balance IS NULL OR kind IN ('asset', 'liability'));
ALTER TABLE accounts ADD COLUMN stated_on TEXT
    CHECK ((stated_on IS NULL) = (stated_balance IS NULL))
    CHECK (stated_on IS NULL
           OR stated_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');

ALTER TABLE changed_accounts ADD COLUMN stated_balance INTEGER;
ALTER TABLE changed_accounts ADD COLUMN stated_on TEXT;

CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1)),
    file_name TEXT
) STRICT;
INSERT INTO new_changes (id, sequence, action, made_at, undone, file_name)
    SELECT id, sequence, action, made_at, undone, file_name FROM changes;
DROP TABLE changes;
ALTER TABLE new_changes RENAME TO changes;
";

/// Brings a book from layout `from` to the current one, or a new, empty
/// database (`from` = 0) to a complete book. See [`migrate_with`].
pub(crate) fn migrate(conn: &mut Connection, from: i32) -> Result<(), StoreError> {
    migrate_with(conn, &MIGRATIONS, FIRST_VERSION, from)
}

/// Runs the `migrations` a book at layout `from` lacks, all in one
/// transaction: if one fails, the book stays exactly as it was. The
/// application ID and the new layout version are written in the same
/// transaction, so a half-made book is never taken for a book.
///
/// Foreign keys are turned off while it runs: with them on, a migration that
/// rebuilds a table would delete the rows that refer to it. Instead, every
/// reference is checked before the transaction is committed. The caller turns
/// them on again.
pub(crate) fn migrate_with(
    conn: &mut Connection,
    migrations: &[&str],
    first_version: i32,
    from: i32,
) -> Result<(), StoreError> {
    let pending = if from == 0 {
        migrations
    } else {
        usize::try_from(from - first_version + 1)
            .ok()
            .and_then(|done| migrations.get(done..))
            .ok_or(StoreError::UnsupportedLayout { found: from })?
    };
    let target = first_version - 1 + i32::try_from(migrations.len()).expect("a short list");
    // Outside a transaction: inside one, SQLite ignores it. The bundled
    // SQLite turns foreign keys on by default.
    conn.pragma_update(None, "foreign_keys", false)?;
    let transaction = conn.transaction()?;
    for migration in pending {
        transaction.execute_batch(migration)?;
    }
    let broken_references: i64 =
        transaction.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if broken_references > 0 {
        return Err(StoreError::Corrupt(Corruption::BrokenReferences));
    }
    transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
    transaction.pragma_update(None, "user_version", target)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use kassensturz_core::{
        AccountId, CurrencyCode, Date, Money, RoundingMode, SplitId, TransactionId,
    };
    use rusqlite::params;

    use super::*;
    use crate::lock::lock_path;
    use crate::test_folder::TempFolder;
    use crate::{Book, Computer, HistoryRetention};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// Made-up data in layout 1's tables, which every layout has: a checking
    /// account, a category and one purchase. Returns the checking account.
    fn sample_data(conn: &Connection) -> AccountId {
        let checking = AccountId::generate();
        let groceries = AccountId::generate();
        let shopping = TransactionId::generate().to_bytes();
        conn.execute_batch("INSERT INTO currencies (code, minor_digits) VALUES ('EUR', 2);")
            .expect("currency written");
        conn.execute(
            "INSERT INTO accounts (id, name, kind, currency)
             VALUES (?1, 'Checking account', 'asset', 'EUR'), (?2, 'Groceries', 'expense', 'EUR')",
            params![checking.to_bytes(), groceries.to_bytes()],
        )
        .expect("accounts written");
        conn.execute(
            "INSERT INTO transactions (id, booking_date, payee, currency)
             VALUES (?1, '2026-10-05', 'Corner Grocery', 'EUR')",
            [shopping],
        )
        .expect("transaction written");
        conn.execute(
            "INSERT INTO splits (id, transaction_id, position, account_id, amount)
             VALUES (?1, ?3, 0, ?4, -5320), (?2, ?3, 1, ?5, 5320)",
            params![
                SplitId::generate().to_bytes(),
                SplitId::generate().to_bytes(),
                shopping,
                checking.to_bytes(),
                groceries.to_bytes()
            ],
        )
        .expect("parts written");
        checking
    }

    /// Writes a book at the earlier layout `version` to `path`, as an older
    /// KassenSturz did, with [`sample_data`] in it.
    fn old_book(path: &Path, version: i32) -> AccountId {
        let mut conn = Connection::open(path).expect("book created");
        let done = usize::try_from(version - FIRST_VERSION + 1).expect("an earlier layout");
        migrate_with(&mut conn, &MIGRATIONS[..done], FIRST_VERSION, 0).expect("old layout");
        sample_data(&conn)
    }

    fn computer(folder: &TempFolder) -> Computer {
        Computer {
            name: "Laptop".into(),
            backup_folder: folder.join("Backups"),
        }
    }

    fn pragma(conn: &Connection, name: &str) -> i32 {
        conn.pragma_query_value(None, name, |row| row.get(0))
            .expect("pragma read")
    }

    /// Everything a layout defines: tables, indexes and their SQL.
    fn schema(conn: &Connection) -> Vec<(String, String, Option<String>)> {
        let mut statement = conn
            .prepare("SELECT type, name, sql FROM sqlite_schema ORDER BY type, name")
            .expect("schema read");
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("schema read")
            .collect::<Result<_, _>>()
            .expect("schema read")
    }

    fn files_in(folder: &Path) -> Vec<PathBuf> {
        fs::read_dir(folder)
            .expect("folder read")
            .map(|entry| entry.expect("entry").path())
            .collect()
    }

    #[test]
    fn a_new_book_gets_every_migration() {
        let book = Book::create_in_memory().expect("book created");
        assert_eq!(pragma(&book.conn, "user_version"), VERSION);
        assert_eq!(pragma(&book.conn, "application_id"), APPLICATION_ID);
        assert_eq!(pragma(&book.conn, "foreign_keys"), 1);
    }

    #[test]
    fn books_with_every_earlier_layout_are_updated_after_a_backup() {
        let current = schema(&Book::create_in_memory().expect("book created").conn);
        for version in FIRST_VERSION..VERSION {
            let folder = TempFolder::new();
            let path = folder.join("Household.ksz");
            let checking = old_book(&path, version);
            let computer = computer(&folder);

            let book = Book::open(&path, &computer).expect("book updated");
            assert_eq!(pragma(&book.conn, "user_version"), VERSION);
            assert_eq!(pragma(&book.conn, "foreign_keys"), 1);
            assert_eq!(schema(&book.conn), current, "layout {version}");
            assert_eq!(
                book.balance(checking).expect("balance"),
                Money::from_minor_units(-5320, EUR)
            );
            let register = book.register(checking).expect("register");
            assert_eq!(register.len(), 1);
            // Parts from before layout 6 keep their accounts and are in
            // their transaction's currency, with nothing imported.
            let shopping = book.transaction(register[0].transaction).expect("read");
            for part in shopping.splits() {
                assert!(part.account.is_some(), "layout {version}");
                assert_eq!(
                    (&part.foreign, &part.imported, &part.original),
                    (&None, &None, &None)
                );
                assert_eq!((part.own_date, &part.own_payee), (None, &None));
            }
            // Accounts from before layout 4 have no type and are open.
            let account = book.account(checking).expect("account");
            assert_eq!(
                (account.account_type, account.closed, account.parent),
                (None, false, None)
            );
            // Accounts from before layout 14 have no stated balance.
            assert_eq!(book.stated_balance(checking).expect("statement"), None);
            assert_eq!(
                book.rounding_mode().expect("rounding mode"),
                RoundingMode::HalfAwayFromZero
            );
            // The change history starts empty, kept for 30 days, and the
            // update itself is no change the user could undo.
            assert_eq!(book.history().expect("history"), Vec::new());
            assert_eq!(
                book.history_retention().expect("retention"),
                HistoryRetention::Days(30)
            );
            drop(book);

            // The backup is the book as it was before the update.
            let backups = files_in(&computer.backup_folder);
            assert_eq!(backups.len(), 1, "layout {version}");
            let backup = Connection::open(&backups[0]).expect("backup opened");
            assert_eq!(pragma(&backup, "user_version"), version);
            assert_eq!(pragma(&backup, "application_id"), APPLICATION_ID);
            let parts: i64 = backup
                .query_row("SELECT count(*) FROM splits", [], |row| row.get(0))
                .expect("parts counted");
            assert_eq!(parts, 2);

            // Once updated, the book opens without another backup.
            drop(Book::open(&path, &computer).expect("book opened again"));
            assert_eq!(files_in(&computer.backup_folder).len(), 1);
        }
    }

    #[test]
    fn changes_made_before_changes_was_rebuilt_can_still_be_undone() {
        // Layouts 8, 9, 10 and 14 rebuild `changes`, and layout 13 adds a
        // column to it; what a book recorded before stays.
        for version in 7..VERSION {
            undo_after_update(version);
        }
    }

    fn undo_after_update(version: i32) {
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        let checking = old_book(&path, version);
        let conn = Connection::open(&path).expect("book opened");
        let change = uuid::Uuid::now_v7().into_bytes();
        let made_at = kassensturz_core::Timestamp::from_second(
            kassensturz_core::Timestamp::now().as_second(),
        )
        .expect("now");
        conn.execute(
            "INSERT INTO changes (id, sequence, action, made_at, undone)
             VALUES (?1, 1, 'rename_account', ?2, 0)",
            params![change, made_at.to_string()],
        )
        .expect("change written");
        conn.execute(
            "INSERT INTO changed_accounts (change_id, side, id, name, kind, currency, closed)
             VALUES (?1, 'before', ?2, 'Main account', 'asset', 'EUR', 0),
                    (?1, 'after', ?2, 'Checking account', 'asset', 'EUR', 0)",
            params![change, checking.to_bytes()],
        )
        .expect("rows written");
        drop(conn);

        let mut book = Book::open(&path, &computer(&folder)).expect("book updated");
        let history = book.history().expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].action, crate::Action::RenameAccount);
        assert_eq!(history[0].file_name, None);
        book.undo().expect("undone");
        assert_eq!(
            book.account(checking).expect("account").name,
            "Main account"
        );
        // The updated `changed_accounts` records the columns layout 14 added,
        // so a stated balance noted after the update can be undone too.
        let stated = crate::StatedBalance {
            amount: Money::from_minor_units(-5320, EUR),
            on: Date::constant(2026, 10, 5),
        };
        book.set_stated_balance(checking, Some(&stated))
            .expect("noted");
        assert_eq!(book.stated_balance(checking).expect("read"), Some(stated));
        book.undo().expect("undone");
        assert_eq!(book.stated_balance(checking).expect("read"), None);
    }

    #[test]
    fn rates_and_recorded_parts_stay_when_split_rates_is_rebuilt() {
        // Layout 11 rebuilds `split_rates` and adds two columns to `splits`
        // and `changed_splits`, and layout 12 two more.
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        let checking = old_book(&path, 10);
        let conn = Connection::open(&path).expect("book opened");
        let (shopping, groceries_part): ([u8; 16], [u8; 16]) = conn
            .query_row(
                "SELECT transaction_id, id FROM splits WHERE position = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("part read");
        // The purchase was 50.00 EUR before an edit made it 53.20: the
        // history kept both, as layout 10 wrote them.
        let change = uuid::Uuid::now_v7().into_bytes();
        conn.execute(
            "INSERT INTO changes (id, sequence, action, made_at, undone)
             VALUES (?1, 1, 'edit_transaction', '2026-10-05T12:00:00Z', 0)",
            [change],
        )
        .expect("change written");
        conn.execute(
            "INSERT INTO changed_splits
                 (change_id, side, id, transaction_id, position, account_id, amount)
             SELECT ?1, side, id, transaction_id, position, account_id, amount
             FROM splits, (SELECT 'before' AS side UNION SELECT 'after')",
            [change],
        )
        .expect("rows written");
        conn.execute(
            "UPDATE changed_splits SET amount = CASE WHEN amount < 0 THEN -5000 ELSE 5000 END
             WHERE side = 'before'",
            [],
        )
        .expect("rows written");
        // A dollar part kept the rate it was converted with.
        let dollars = AccountId::generate();
        let trip = TransactionId::generate();
        let received = SplitId::generate();
        let rate = uuid::Uuid::now_v7().into_bytes();
        conn.execute_batch("INSERT INTO currencies (code, minor_digits) VALUES ('USD', 2);")
            .expect("currency written");
        conn.execute(
            "INSERT INTO accounts (id, name, kind, currency) VALUES (?1, 'Dollars', 'asset', 'USD')",
            [dollars.to_bytes()],
        )
        .expect("account written");
        conn.execute(
            "INSERT INTO transactions (id, booking_date, currency)
             VALUES (?1, '2026-10-07', 'EUR')",
            [trip.to_bytes()],
        )
        .expect("transaction written");
        conn.execute(
            "INSERT INTO splits
                 (id, transaction_id, position, account_id, amount, foreign_amount, foreign_currency)
             VALUES (?1, ?3, 0, ?4, -100000, NULL, NULL), (?2, ?3, 1, ?5, 100000, 108560, 'USD')",
            params![
                SplitId::generate().to_bytes(),
                received.to_bytes(),
                trip.to_bytes(),
                checking.to_bytes(),
                dollars.to_bytes()
            ],
        )
        .expect("parts written");
        conn.execute(
            "INSERT INTO split_rates
                 (split_id, rate_id, date, from_currency, to_currency,
                  rate_digits, rate_decimal_places, source)
             VALUES (?1, ?2, '2026-10-07', 'EUR', 'USD', 10856, 4, 'manual')",
            params![received.to_bytes(), rate],
        )
        .expect("rate written");
        drop(conn);

        let mut book = Book::open(&path, &computer(&folder)).expect("book updated");
        let trip = book.transaction(trip).expect("read");
        let foreign = trip.splits()[1].foreign.expect("a part in dollars");
        let kept = foreign.rate.expect("its rate");
        assert_eq!(
            (
                foreign.amount,
                kept.rate,
                kept.source,
                trip.splits()[1].original
            ),
            (
                Money::from_minor_units(108_560, CurrencyCode::new("USD").expect("valid")),
                "1.0856".parse().expect("valid rate"),
                kassensturz_core::RateSource::Manual,
                None
            )
        );
        book.undo().expect("undone");
        let shopping = book
            .transaction(TransactionId::from_bytes(shopping))
            .expect("read");
        let part = shopping
            .splits()
            .iter()
            .find(|part| part.id == SplitId::from_bytes(groceries_part))
            .expect("the groceries part");
        assert_eq!(
            (part.amount, part.original),
            (Money::from_minor_units(5000, EUR), None)
        );
    }

    #[test]
    fn a_book_is_not_updated_without_a_backup() {
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        old_book(&path, FIRST_VERSION);
        let before = fs::read(&path).expect("book read");
        // The backup folder can't be made: a file is in its place.
        let computer = computer(&folder);
        fs::write(&computer.backup_folder, "in the way").expect("file written");
        assert!(matches!(
            Book::open(&path, &computer),
            Err(StoreError::BackupFailed(_))
        ));
        assert_eq!(fs::read(&path).expect("book read"), before);
        assert!(!lock_path(&path).exists());
    }

    #[test]
    fn a_failed_update_leaves_the_book_as_it_was() {
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        old_book(&path, FIRST_VERSION);
        let before = fs::read(&path).expect("book read");
        let mut conn = Connection::open(&path).expect("book opened");
        // The second migration fails halfway through.
        let migrations = [
            LAYOUT_1,
            "CREATE TABLE notes (text TEXT) STRICT; CREATE TABLE notes;",
        ];
        assert!(migrate_with(&mut conn, &migrations, FIRST_VERSION, FIRST_VERSION).is_err());
        drop(conn);
        assert_eq!(fs::read(&path).expect("book read"), before);
    }

    #[test]
    fn an_update_that_breaks_a_reference_is_refused() {
        let mut conn = Connection::open_in_memory().expect("book created");
        migrate_with(&mut conn, &[LAYOUT_1], FIRST_VERSION, 0).expect("layout 1");
        // A currency the book doesn't use.
        let dangling = "INSERT INTO accounts (id, name, kind, currency)
                        VALUES (zeroblob(16), 'Savings', 'asset', 'CHF');";
        assert!(matches!(
            migrate_with(
                &mut conn,
                &[LAYOUT_1, dangling],
                FIRST_VERSION,
                FIRST_VERSION
            ),
            Err(StoreError::Corrupt(Corruption::BrokenReferences))
        ));
        let accounts: i64 = conn
            .query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))
            .expect("accounts counted");
        assert_eq!(accounts, 0);
        assert_eq!(pragma(&conn, "user_version"), FIRST_VERSION);
    }

    #[test]
    fn rebuilding_a_table_keeps_the_rows_that_refer_to_it() {
        let mut conn = Connection::open_in_memory().expect("book created");
        migrate_with(&mut conn, &[LAYOUT_1], FIRST_VERSION, 0).expect("layout 1");
        sample_data(&conn);
        // As a system SQLite may have them, and the bundled one does.
        conn.pragma_update(None, "foreign_keys", true)
            .expect("foreign keys on");
        // SQLite's way to change a table: make a new one and replace the old
        // one with it (sqlite.org/lang_altertable.html). With foreign keys
        // on, dropping the old table would delete the parts of every
        // transaction.
        let rebuild = "
            CREATE TABLE new_transactions (
                id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
                booking_date TEXT NOT NULL,
                payee TEXT,
                currency TEXT NOT NULL REFERENCES currencies (code)
            ) STRICT;
            INSERT INTO new_transactions SELECT * FROM transactions;
            DROP TABLE transactions;
            ALTER TABLE new_transactions RENAME TO transactions;
        ";
        migrate_with(
            &mut conn,
            &[LAYOUT_1, rebuild],
            FIRST_VERSION,
            FIRST_VERSION,
        )
        .expect("table rebuilt");
        let parts: i64 = conn
            .query_row("SELECT count(*) FROM splits", [], |row| row.get(0))
            .expect("parts counted");
        assert_eq!(parts, 2);
    }

    #[test]
    fn layouts_without_their_migrations_are_refused() {
        let mut conn = Connection::open_in_memory().expect("database opened");
        for found in [-1, VERSION + 1] {
            assert!(matches!(
                migrate_with(&mut conn, &MIGRATIONS, FIRST_VERSION, found),
                Err(StoreError::UnsupportedLayout { found: reported }) if reported == found
            ));
        }
        // Nothing was changed.
        assert_eq!(pragma(&conn, "user_version"), 0);
        assert_eq!(pragma(&conn, "application_id"), 0);
    }
}
