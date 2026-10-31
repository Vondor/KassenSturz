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
///
/// A migration that changes a table the change history records also changes
/// its `changed_` table the same way and converts the rows kept there, so
/// changes made before the update can still be undone (D-066); so does the
/// list of recorded tables in `history.rs`.
pub(crate) const MIGRATIONS: [&str; 7] = [
    LAYOUT_16, LAYOUT_17, LAYOUT_18, LAYOUT_19, LAYOUT_20, LAYOUT_21, LAYOUT_22,
];

/// The layout version the first migration creates. Layouts 1 to 15 were the
/// development layouts from before 0.1, merged into layout 16 (D-045,
/// D-135); a book with one of them is a [development
/// book](StoreError::DevelopmentLayout), which 0.1 turns away.
pub(crate) const FIRST_VERSION: i32 = 16;

/// The first development layout. Every version from here up to
/// [`FIRST_VERSION`] belongs to a book a development build wrote.
pub(crate) const FIRST_DEVELOPMENT_VERSION: i32 = 1;

/// The current layout version, stored in SQLite's `user_version`. The list
/// of migrations is far too short for the conversion to lose anything.
pub(crate) const VERSION: i32 = FIRST_VERSION + MIGRATIONS.len() as i32 - 1;

/// Layout 16, the starting layout of 0.1 (D-045, D-135): the 15 development
/// layouts of M0 to M3 merged into one, table for table what they built
/// together, which the test `development_layouts` in this crate's `tests`
/// folder proves.
///
/// Every table is `STRICT`, so SQLite refuses values of the wrong type
/// instead of converting them. Amounts are INTEGER minor units, never REAL
/// (AGENTS.md §3.1 rule 1); rates are two INTEGERs, their digits and decimal
/// places (D-051). IDs are the 16 bytes of a UUIDv7 (D-067), dates ISO 8601
/// text (D-061). Each table is described in `docs/BOOK_FORMAT.md`; in short:
///
/// - `currencies`, `accounts`, `transactions` and their parts in `splits`
///   are the book (D-068, D-083). A part can be without a category yet, so
///   `splits.account_id` may be NULL. A part in another currency keeps its
///   amount there, with the rate it was converted with in `split_rates`
///   (D-043); one converted out of a withdrawn currency keeps the amount it
///   had (D-104). What a bank or file delivered for a part is kept in
///   `split_imports` (D-067). A part can have a date and a payee of its own,
///   which its account's register shows (D-108). Categories form a tree, and
///   an account can sit inside another one of its kind, as another program's
///   account groups do (D-081, D-132). An account keeps the balance the user
///   stated for it and the day it was for (D-111, D-117).
/// - `institutions`, `payees`, `tags` with `split_tags`, and `attachments`
///   hold what a book another program kept has besides that (D-132).
/// - `prices` is the price table for exchange rates (D-028).
/// - `rules`, `rule_conditions` and `split_rules` are the Rules and which
///   part's category one of them set (D-014, D-031, D-088).
/// - `settings` is the book's one row of settings, with the rounding mode
///   set to round half away from zero (D-027).
/// - `history_settings`, `changes` and the `changed_` tables are the change
///   history (D-066, D-086). How long it is kept is a setting of the book: a
///   number of days, or forever (`NULL`); a new book keeps it for 30 days.
///   Each user action is one row of `changes`, numbered in the order the
///   actions were made, with the name of the file it read, if any (D-109).
///   The rows it touched are copied into the `changed_` table of their
///   table, once with their values before the action and once with those
///   after it; a row it added has no values before, one it removed none
///   after. The copies have the same columns as the table, without its
///   checks, which apply when the values are written back.
const LAYOUT_16: &str = "
CREATE TABLE currencies (
    code TEXT PRIMARY KEY NOT NULL
        CHECK (length(code) = 3 AND code NOT GLOB '*[^A-Z]*'),
    minor_digits INTEGER NOT NULL CHECK (minor_digits BETWEEN 0 AND 18)
) STRICT;

CREATE TABLE institutions (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL,
    bank_code TEXT,
    contact TEXT,
    address TEXT,
    phone TEXT
) STRICT;

CREATE TABLE accounts (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('asset', 'liability', 'income', 'expense', 'equity')),
    currency TEXT NOT NULL REFERENCES currencies (code),
    account_type TEXT
        CHECK (account_type IN ('checking', 'savings', 'cash', 'credit_card')),
    closed INTEGER NOT NULL DEFAULT 0
        CHECK (closed IN (0, 1)),
    parent_id BLOB REFERENCES accounts (id)
        CHECK (parent_id IS NULL OR (
            length(parent_id) = 16 AND parent_id <> id AND kind <> 'equity'
        )),
    stated_balance INTEGER
        CHECK (stated_balance IS NULL OR kind IN ('asset', 'liability')),
    stated_on TEXT
        CHECK ((stated_on IS NULL) = (stated_balance IS NULL))
        CHECK (stated_on IS NULL
               OR stated_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    number TEXT CHECK (number IS NULL OR kind IN ('asset', 'liability')),
    notes TEXT,
    institution_id BLOB REFERENCES institutions (id)
        CHECK (institution_id IS NULL
               OR (length(institution_id) = 16 AND kind IN ('asset', 'liability')))
) STRICT;
CREATE INDEX accounts_by_parent ON accounts (parent_id);
CREATE INDEX accounts_by_institution ON accounts (institution_id);

CREATE TABLE payees (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL,
    address TEXT,
    email TEXT,
    phone TEXT,
    reference TEXT,
    recognition TEXT NOT NULL CHECK (recognition IN ('never', 'name', 'key_words')),
    key_words TEXT CHECK ((key_words IS NULL) = (recognition <> 'key_words')),
    recognition_ignores_case INTEGER NOT NULL CHECK (recognition_ignores_case IN (0, 1))
) STRICT;

CREATE TABLE tags (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL
) STRICT;

CREATE TABLE transactions (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    booking_date TEXT NOT NULL
        CHECK (booking_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    payee TEXT,
    currency TEXT NOT NULL REFERENCES currencies (code),
    note TEXT,
    entered_on TEXT
        CHECK (entered_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')
) STRICT;

CREATE TABLE splits (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    account_id BLOB REFERENCES accounts (id),
    amount INTEGER NOT NULL,
    foreign_amount INTEGER,
    foreign_currency TEXT REFERENCES currencies (code),
    original_amount INTEGER,
    original_currency TEXT REFERENCES currencies (code)
        CHECK ((original_amount IS NULL) = (original_currency IS NULL)),
    own_date TEXT
        CHECK (own_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    own_payee TEXT,
    note TEXT,
    number TEXT,
    status TEXT CHECK (status IN ('cleared', 'reconciled')),
    CHECK ((foreign_amount IS NULL) = (foreign_currency IS NULL)),
    CHECK (foreign_amount IS NULL OR account_id IS NOT NULL),
    UNIQUE (transaction_id, position)
) STRICT;
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
    source TEXT NOT NULL CHECK (source IN ('manual', 'changeover')),
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

CREATE TABLE split_tags (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    split_id BLOB NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    tag_id BLOB NOT NULL REFERENCES tags (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (split_id, tag_id)
) STRICT;
CREATE INDEX split_tags_by_tag ON split_tags (tag_id);

CREATE TABLE attachments (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    link TEXT NOT NULL CHECK (link <> ''),
    title TEXT,
    UNIQUE (transaction_id, position)
) STRICT;

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

CREATE TABLE settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    rounding_mode TEXT NOT NULL CHECK (rounding_mode IN (
        'half_away_from_zero', 'half_even', 'half_toward_zero',
        'toward_zero', 'away_from_zero', 'floor', 'ceiling'
    ))
) STRICT;

INSERT INTO settings (id, rounding_mode) VALUES (1, 'half_away_from_zero');

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
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance',
        'add_institution', 'add_payee', 'add_tag'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1)),
    file_name TEXT
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

CREATE TABLE changed_institutions (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    bank_code TEXT,
    contact TEXT,
    address TEXT,
    phone TEXT,
    PRIMARY KEY (change_id, id, side)
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
    stated_balance INTEGER,
    stated_on TEXT,
    number TEXT,
    notes TEXT,
    institution_id BLOB,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_payees (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    address TEXT,
    email TEXT,
    phone TEXT,
    reference TEXT,
    recognition TEXT,
    key_words TEXT,
    recognition_ignores_case INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_tags (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_transactions (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    booking_date TEXT,
    payee TEXT,
    currency TEXT,
    note TEXT,
    entered_on TEXT,
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
    original_amount INTEGER,
    original_currency TEXT,
    own_date TEXT,
    own_payee TEXT,
    note TEXT,
    number TEXT,
    status TEXT,
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

CREATE TABLE changed_split_tags (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    split_id BLOB,
    tag_id BLOB,
    position INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;

CREATE TABLE changed_attachments (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    transaction_id BLOB,
    position INTEGER,
    link TEXT,
    title TEXT,
    PRIMARY KEY (change_id, id, side)
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
";

/// Layout 17 (M4.4a): bank links (Q-086, D-148). `bank_links` says which
/// bank account an account is filled from, by IBAN or by bank code and
/// account number, and the day it was last fetched; it holds no login data
/// (D-139). Linking, unlinking and fetching are user actions of their own,
/// so `changes` is rebuilt with `link_bank_account`, `unlink_bank_account`
/// and `fetch_statement` in its list of actions; its rows stay as they were,
/// and the `changed_` tables refer to it by name and keep their rows too.
const LAYOUT_17: &str = "
CREATE TABLE bank_links (
    account_id BLOB PRIMARY KEY NOT NULL REFERENCES accounts (id) ON DELETE CASCADE
        CHECK (length(account_id) = 16),
    iban TEXT CHECK (iban IS NULL OR (iban <> '' AND iban NOT GLOB '*[^A-Z0-9]*')),
    bank_code TEXT CHECK (bank_code IS NULL OR bank_code <> ''),
    number TEXT CHECK (number IS NULL OR number <> ''),
    last_fetched_on TEXT
        CHECK (last_fetched_on IS NULL
               OR last_fetched_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    CHECK (iban IS NOT NULL OR (bank_code IS NOT NULL AND number IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX bank_links_by_iban ON bank_links (iban) WHERE iban IS NOT NULL;
CREATE UNIQUE INDEX bank_links_by_number ON bank_links (bank_code, number)
    WHERE number IS NOT NULL;
CREATE TABLE changed_bank_links (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    account_id BLOB NOT NULL,
    iban TEXT,
    bank_code TEXT,
    number TEXT,
    last_fetched_on TEXT,
    PRIMARY KEY (change_id, account_id, side)
) STRICT;
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance',
        'add_institution', 'add_payee', 'add_tag',
        'link_bank_account', 'unlink_bank_account', 'fetch_statement'
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

/// Layout 18 (M5.5a): changes an AI prepared are labelled in the history
/// (D-198). A change set the user confirmed is applied as one user action of
/// its own, so `changes` is rebuilt with `apply_ai_changes` in its list of
/// actions, as layout 17 did; its rows stay as they were, and the
/// `changed_` tables refer to it by name and keep their rows too.
const LAYOUT_18: &str = "
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance',
        'add_institution', 'add_payee', 'add_tag',
        'link_bank_account', 'unlink_bank_account', 'fetch_statement',
        'apply_ai_changes'
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

/// Layout 19 (grid step 4, D-253): an AI may delete a category that nothing
/// uses once the user confirms it, so `changes` is rebuilt with
/// `remove_category` in its list of actions, as layouts 17 and 18 did; its
/// rows stay as they were, and the `changed_` tables keep theirs too.
const LAYOUT_19: &str = "
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance',
        'add_institution', 'add_payee', 'add_tag',
        'link_bank_account', 'unlink_bank_account', 'fetch_statement',
        'apply_ai_changes', 'remove_category'
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

/// Layout 20 (M6a.L, D-278): saved views and dashboards. `views` keeps each
/// document as JSON text with the few things a list needs copied out of it:
/// its kind, name, place and version. The book does not look inside the
/// document any further (the reporting layer does, D-281). Adding, editing,
/// renaming, moving and removing one are user actions of their own, so
/// `changes` is rebuilt with `add_view`, `edit_view`, `rename_view`,
/// `move_view` and `remove_view` in its list of actions, as layouts 17 to 19
/// did; its rows stay as they were, and the `changed_` tables keep theirs.
const LAYOUT_20: &str = "
CREATE TABLE views (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    kind TEXT NOT NULL CHECK (kind IN ('view', 'dashboard')),
    name TEXT NOT NULL CHECK (name <> '' AND name = trim(name) AND length(name) <= 200),
    position INTEGER NOT NULL CHECK (position >= 0),
    format_version INTEGER NOT NULL CHECK (format_version >= 1),
    document TEXT NOT NULL
        CHECK (json_valid(document) AND length(CAST(document AS BLOB)) <= 1048576)
) STRICT;
CREATE UNIQUE INDEX views_by_name ON views (kind, lower(name));
CREATE TABLE changed_views (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    kind TEXT,
    name TEXT,
    position INTEGER,
    format_version INTEGER,
    document TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance',
        'add_institution', 'add_payee', 'add_tag',
        'link_bank_account', 'unlink_bank_account', 'fetch_statement',
        'apply_ai_changes', 'remove_category',
        'add_view', 'edit_view', 'rename_view', 'move_view', 'remove_view'
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

/// Layout 21 (M6b.L, D-290): category groups and hidden categories, kept
/// apart from the transactions and the category tree (Q-221). Nothing in
/// these tables is ever written onto a transaction or a category; they only
/// refer to categories by ID. `report_groups` holds each group with its name,
/// kind and place in the list; `report_group_members` its members, each a
/// category or another group; `report_hidden` the categories the user hid
/// from charts. Removing a category or a group removes the rows that name it
/// in the same change (ON DELETE CASCADE), so its Undo brings them back. The
/// five new user actions rebuild `changes` as layouts 17 to 20 did; its rows
/// stay as they were.
const LAYOUT_21: &str = "
CREATE TABLE report_groups (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL CHECK (name <> '' AND name = trim(name) AND length(name) <= 200),
    kind TEXT NOT NULL CHECK (kind IN ('expense', 'income')),
    position INTEGER NOT NULL CHECK (position >= 0)
) STRICT;
CREATE UNIQUE INDEX report_groups_by_name ON report_groups (lower(name));
CREATE TABLE report_group_members (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    group_id BLOB NOT NULL REFERENCES report_groups (id) ON DELETE CASCADE,
    category_id BLOB REFERENCES accounts (id) ON DELETE CASCADE,
    member_group_id BLOB REFERENCES report_groups (id) ON DELETE CASCADE
        CHECK (member_group_id IS NULL OR member_group_id <> group_id),
    position INTEGER NOT NULL CHECK (position >= 0),
    CHECK ((category_id IS NULL) <> (member_group_id IS NULL))
) STRICT;
CREATE INDEX report_group_members_in_group ON report_group_members (group_id, position);
CREATE UNIQUE INDEX report_group_members_by_category
    ON report_group_members (category_id, group_id);
CREATE UNIQUE INDEX report_group_members_by_group
    ON report_group_members (member_group_id, group_id);
CREATE TABLE report_hidden (
    category_id BLOB PRIMARY KEY NOT NULL CHECK (length(category_id) = 16)
        REFERENCES accounts (id) ON DELETE CASCADE
) STRICT;
CREATE TABLE changed_report_groups (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    kind TEXT,
    position INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_report_group_members (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    group_id BLOB,
    category_id BLOB,
    member_group_id BLOB,
    position INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_report_hidden (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    category_id BLOB NOT NULL,
    PRIMARY KEY (change_id, category_id, side)
) STRICT;
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance',
        'add_institution', 'add_payee', 'add_tag',
        'link_bank_account', 'unlink_bank_account', 'fetch_statement',
        'apply_ai_changes', 'remove_category',
        'add_view', 'edit_view', 'rename_view', 'move_view', 'remove_view',
        'add_group', 'edit_group', 'move_group', 'remove_group',
        'set_hidden_categories'
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

/// Layout 22 (M6c.L, D-344, D-345): budgets, an amount the user plans to
/// spend on one spending category in each of a budget's periods. Nothing in
/// these tables is ever written onto a transaction or a category. `budgets`
/// holds each budget with its category, its kind of period and its days;
/// `budget_amounts` what it plans from one of its periods on, so a change
/// never rewrites an earlier period (D-345). A category that has a budget
/// can't be removed (the foreign key has no cascade, and the store says so
/// first with `store.category-in-use`); removing a budget removes its
/// amounts in the same change (ON DELETE CASCADE), so its Undo brings them
/// back. The three new user actions rebuild `changes` as layouts 17 to 21
/// did; its rows stay as they were.
const LAYOUT_22: &str = "
CREATE TABLE budgets (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    category_id BLOB NOT NULL REFERENCES accounts (id),
    every TEXT NOT NULL CHECK (every IN ('month', 'quarter', 'year', 'range')),
    first_day TEXT NOT NULL
        CHECK (first_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    last_day TEXT
        CHECK (last_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    CHECK (last_day IS NULL OR last_day >= first_day),
    CHECK (every <> 'range' OR last_day IS NOT NULL)
) STRICT;
CREATE INDEX budgets_by_category ON budgets (category_id, first_day);
CREATE TABLE budget_amounts (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    budget_id BLOB NOT NULL REFERENCES budgets (id) ON DELETE CASCADE,
    from_day TEXT NOT NULL
        CHECK (from_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    amount INTEGER NOT NULL CHECK (amount > 0),
    carry_over INTEGER NOT NULL CHECK (carry_over IN (0, 1))
) STRICT;
CREATE UNIQUE INDEX budget_amounts_by_day ON budget_amounts (budget_id, from_day);
CREATE TABLE changed_budgets (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    category_id BLOB,
    every TEXT,
    first_day TEXT,
    last_day TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_budget_amounts (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    budget_id BLOB,
    from_day TEXT,
    amount INTEGER,
    carry_over INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE new_changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN (
        'set_rounding_mode', 'use_currency', 'add_account', 'rename_account',
        'close_account', 'reopen_account', 'add_category', 'rename_category',
        'move_category', 'add_rate', 'remove_rate', 'add_transaction',
        'edit_transaction', 'remove_transaction', 'join_transfer',
        'add_rule', 'edit_rule', 'move_rule', 'remove_rule', 'run_rules',
        'import_statement', 'correct_starting_balance',
        'add_institution', 'add_payee', 'add_tag',
        'link_bank_account', 'unlink_bank_account', 'fetch_statement',
        'apply_ai_changes', 'remove_category',
        'add_view', 'edit_view', 'rename_view', 'move_view', 'remove_view',
        'add_group', 'edit_group', 'move_group', 'remove_group',
        'set_hidden_categories',
        'add_budget', 'edit_budget', 'remove_budget'
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
    let done = match from {
        0 => 0,
        known if known >= first_version => {
            usize::try_from(known - first_version + 1).expect("not negative")
        }
        _ => return Err(StoreError::UnsupportedLayout { found: from }),
    };
    let pending = migrations
        .get(done..)
        .ok_or(StoreError::UnsupportedLayout { found: from })?;
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
mod development_layouts;

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

    /// Made-up data in the tables every layout has: a checking account, a
    /// category and one purchase. Returns the checking account.
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

    /// Writes a book at layout `version` to `path`, as the KassenSturz with
    /// `migrations` did, with [`sample_data`] in it.
    fn book_at(path: &Path, migrations: &[&str], version: i32) -> AccountId {
        let mut conn = Connection::open(path).expect("book created");
        let done = usize::try_from(version - FIRST_VERSION + 1).expect("a known layout");
        migrate_with(&mut conn, &migrations[..done], FIRST_VERSION, 0).expect("old layout");
        sample_data(&conn)
    }

    /// Writes a book at the earlier layout `version` to `path`, as an older
    /// KassenSturz did, with [`sample_data`] in it.
    fn old_book(path: &Path, version: i32) -> AccountId {
        book_at(path, &MIGRATIONS, version)
    }

    /// The migrations a KassenSturz after this one may have: the current
    /// ones and one more, which adds a table. The tests of an update use
    /// these, so the update is tried before that layout exists (D-135).
    const WITH_A_LATER_LAYOUT: [&str; MIGRATIONS.len() + 1] = {
        let mut migrations = [""; MIGRATIONS.len() + 1];
        let mut index = 0;
        while index < MIGRATIONS.len() {
            migrations[index] = MIGRATIONS[index];
            index += 1;
        }
        migrations[MIGRATIONS.len()] = "CREATE TABLE notes (text TEXT NOT NULL) STRICT;";
        migrations
    };

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

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("rows counted")
    }

    /// A change made at the old layout, as an older KassenSturz recorded
    /// it: the checking account was renamed. Returns the change's ID.
    fn recorded_rename(path: &Path, checking: AccountId) -> [u8; 16] {
        let conn = Connection::open(path).expect("book opened");
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
        change
    }

    #[test]
    fn a_new_book_gets_every_migration() {
        let book = Book::create_in_memory().expect("book created");
        assert_eq!(pragma(&book.conn, "user_version"), VERSION);
        assert_eq!(pragma(&book.conn, "application_id"), APPLICATION_ID);
        assert_eq!(pragma(&book.conn, "foreign_keys"), 1);
    }

    #[test]
    // Every layout since 0.1 (AGENTS.md §9).
    fn books_with_every_earlier_layout_are_updated_after_a_backup() {
        let current = schema(&Book::create_in_memory().expect("book created").conn);
        for version in FIRST_VERSION..VERSION {
            let folder = TempFolder::new();
            let path = folder.join("Household.ksz");
            let checking = old_book(&path, version);
            recorded_rename(&path, checking);
            let computer = computer(&folder);

            let book = Book::open(&path, &computer).expect("book updated");
            check_update(book, checking, &current, version);

            let backups = files_in(&computer.backup_folder);
            assert_eq!(backups.len(), 1, "layout {version}");
            let backup = Connection::open(&backups[0]).expect("backup opened");
            assert_eq!(pragma(&backup, "user_version"), version);

            // Once updated, the book opens without another backup.
            drop(Book::open(&path, &computer).expect("book opened again"));
            assert_eq!(files_in(&computer.backup_folder).len(), 1);
        }
    }

    #[test]
    fn a_book_with_an_earlier_layout_is_updated_after_a_backup() {
        // As the KassenSturz with the next layout will do it, so the update
        // is tried although every book has the current layout (D-135).
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        let checking = old_book(&path, VERSION);
        recorded_rename(&path, checking);
        let computer = computer(&folder);
        let later = Book::create_in_memory().expect("book created");
        later
            .conn
            .execute_batch(WITH_A_LATER_LAYOUT[MIGRATIONS.len()])
            .expect("later layout");
        let current = schema(&later.conn);
        drop(later);

        let book = Book::open_updated_by(&path, &computer, &WITH_A_LATER_LAYOUT, FIRST_VERSION)
            .expect("book updated");
        assert_eq!(pragma(&book.conn, "user_version"), VERSION + 1);
        check_update(book, checking, &current, VERSION);

        // The backup is the book as it was before the update.
        let backups = files_in(&computer.backup_folder);
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(&backups[0]).expect("backup opened");
        assert_eq!(pragma(&backup, "user_version"), VERSION);
        assert_eq!(pragma(&backup, "application_id"), APPLICATION_ID);
        assert_eq!(count(&backup, "splits"), 2);
        assert_eq!(count(&backup, "changes"), 1);
        assert!(!schema(&backup).iter().any(|(_, name, _)| name == "notes"));
        drop(backup);

        // Once updated, the book opens without another backup, and not at
        // all with the KassenSturz that doesn't know the new layout.
        let again = Book::open_updated_by(&path, &computer, &WITH_A_LATER_LAYOUT, FIRST_VERSION);
        drop(again.expect("book opened again"));
        assert_eq!(files_in(&computer.backup_folder).len(), 1);
        assert!(matches!(
            Book::open(&path, &computer),
            Err(StoreError::UnsupportedLayout { found }) if found == VERSION + 1
        ));
    }

    /// What every update keeps: the schema is the current one, the data and
    /// the change history are what the old book had, and the update itself
    /// is no change the user could undo.
    fn check_update(
        mut book: Book,
        checking: AccountId,
        current: &[(String, String, Option<String>)],
        version: i32,
    ) {
        assert_eq!(pragma(&book.conn, "foreign_keys"), 1);
        assert_eq!(schema(&book.conn), current, "layout {version}");
        assert_eq!(
            book.balance(checking).expect("balance"),
            Money::from_minor_units(-5320, EUR)
        );
        let register = book.register(checking).expect("register");
        assert_eq!(register.len(), 1, "layout {version}");
        assert_eq!(
            book.rounding_mode().expect("rounding mode"),
            RoundingMode::HalfAwayFromZero
        );
        assert_eq!(
            book.history_retention().expect("retention"),
            HistoryRetention::Days(30)
        );
        let history = book.history().expect("history");
        assert_eq!(history.len(), 1, "layout {version}");
        assert_eq!(history[0].action, crate::Action::RenameAccount);
        book.undo().expect("undone");
        assert_eq!(
            book.account(checking).expect("account").name,
            "Main account"
        );
        // The updated `changed_` tables record what the current layout has,
        // so a change made after the update can be undone too.
        let stated = crate::StatedBalance {
            amount: Money::from_minor_units(-5320, EUR),
            on: Date::constant(2026, 10, 5),
        };
        book.set_stated_balance(checking, Some(&stated))
            .expect("noted");
        assert_eq!(book.stated_balance(checking).expect("read"), Some(stated));
        book.undo().expect("undone");
        assert_eq!(book.stated_balance(checking).expect("read"), None);
        // So does a table a later layout added (layout 17).
        let link = kassensturz_core::BankLink {
            account: checking,
            iban: Some("DE89370400440532013000".into()),
            bank_code: None,
            number: None,
            last_fetched_on: Some(Date::constant(2026, 10, 23)),
        };
        book.link_bank_account(&link).expect("linked");
        assert_eq!(book.bank_link(checking).expect("read"), Some(link));
        book.undo().expect("undone");
        assert_eq!(book.bank_link(checking).expect("read"), None);
        // And an action a later layout added (layout 18): AI changes the
        // user confirmed are recorded and undone as one change.
        book.as_one_action(crate::Action::ApplyAiChanges, |book| {
            book.rename_account(checking, "Household account")
        })
        .expect("applied");
        assert_eq!(
            book.change_to_undo()
                .expect("history")
                .map(|change| change.action),
            Some(crate::Action::ApplyAiChanges)
        );
        book.undo().expect("undone");
        assert_eq!(
            book.account(checking).expect("account").name,
            "Main account"
        );
        // And one layout 19 added: a category nothing uses can be removed,
        // and the removal undone.
        let unused = AccountId::generate();
        book.add_account(&kassensturz_core::Account {
            id: unused,
            name: "Unused".into(),
            kind: kassensturz_core::AccountKind::Expense,
            currency: crate::tests::EUR,
            account_type: None,
            closed: false,
            parent: None,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("category added");
        book.remove_category(unused).expect("removed");
        assert!(book.account(unused).is_err());
        book.undo().expect("undone");
        assert!(book.account(unused).is_ok());
    }

    #[test]
    fn a_book_is_not_updated_without_a_backup() {
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        old_book(&path, VERSION);
        let before = fs::read(&path).expect("book read");
        // The backup folder can't be made: a file is in its place.
        let computer = computer(&folder);
        fs::write(&computer.backup_folder, "in the way").expect("file written");
        assert!(matches!(
            Book::open_updated_by(&path, &computer, &WITH_A_LATER_LAYOUT, FIRST_VERSION),
            Err(StoreError::BackupFailed(_))
        ));
        assert_eq!(fs::read(&path).expect("book read"), before);
        assert!(!lock_path(&path).exists());
    }

    #[test]
    fn a_failed_update_leaves_the_book_as_it_was() {
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        old_book(&path, VERSION);
        let before = fs::read(&path).expect("book read");
        let mut conn = Connection::open(&path).expect("book opened");
        // The next migration fails halfway through.
        let mut migrations = WITH_A_LATER_LAYOUT;
        migrations[MIGRATIONS.len()] =
            "CREATE TABLE notes (text TEXT NOT NULL) STRICT; CREATE TABLE notes;";
        assert!(migrate_with(&mut conn, &migrations, FIRST_VERSION, VERSION).is_err());
        drop(conn);
        assert_eq!(fs::read(&path).expect("book read"), before);
    }

    #[test]
    fn an_update_that_breaks_a_reference_is_refused() {
        let mut conn = Connection::open_in_memory().expect("book created");
        migrate_with(&mut conn, &MIGRATIONS, FIRST_VERSION, 0).expect("current layout");
        // A currency the book doesn't use.
        let mut migrations = WITH_A_LATER_LAYOUT;
        migrations[MIGRATIONS.len()] = "INSERT INTO accounts (id, name, kind, currency)
                                        VALUES (zeroblob(16), 'Savings', 'asset', 'CHF');";
        assert!(matches!(
            migrate_with(&mut conn, &migrations, FIRST_VERSION, VERSION),
            Err(StoreError::Corrupt(Corruption::BrokenReferences))
        ));
        assert_eq!(count(&conn, "accounts"), 0);
        assert_eq!(pragma(&conn, "user_version"), VERSION);
    }

    #[test]
    fn rebuilding_a_table_keeps_the_rows_that_refer_to_it() {
        let mut conn = Connection::open_in_memory().expect("book created");
        migrate_with(&mut conn, &MIGRATIONS, FIRST_VERSION, 0).expect("current layout");
        sample_data(&conn);
        // As a system SQLite may have them, and the bundled one does.
        conn.pragma_update(None, "foreign_keys", true)
            .expect("foreign keys on");
        // SQLite's way to change a table: make a new one and replace the old
        // one with it (sqlite.org/lang_altertable.html). With foreign keys
        // on, dropping the old table would delete the parts of every
        // transaction.
        let mut migrations = WITH_A_LATER_LAYOUT;
        migrations[MIGRATIONS.len()] = "
            CREATE TABLE new_transactions (
                id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
                booking_date TEXT NOT NULL,
                payee TEXT,
                currency TEXT NOT NULL REFERENCES currencies (code),
                note TEXT,
                entered_on TEXT
            ) STRICT;
            INSERT INTO new_transactions (id, booking_date, payee, currency, note, entered_on)
                SELECT id, booking_date, payee, currency, note, entered_on FROM transactions;
            DROP TABLE transactions;
            ALTER TABLE new_transactions RENAME TO transactions;
        ";
        migrate_with(&mut conn, &migrations, FIRST_VERSION, VERSION).expect("table rebuilt");
        assert_eq!(count(&conn, "splits"), 2);
    }

    #[test]
    fn layouts_without_their_migrations_are_refused() {
        let mut conn = Connection::open_in_memory().expect("database opened");
        for found in [-1, FIRST_VERSION - 1, VERSION + 1] {
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
