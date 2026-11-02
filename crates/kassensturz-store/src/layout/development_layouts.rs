// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The 23 development layouts of the Alpha, as the migrations in
//! `layout.rs` built them until layout 24 merged them into the starting
//! layout of the first Beta (D-045, D-536). Layouts 1 to 15 (M0 to M3) had
//! been merged into layout 16 once before (D-135); both merges are proved
//! here at once. They prove that the merged layout is table for table what
//! they built together, and nothing else: no KassenSturz runs them anymore,
//! and this file goes once the first Beta is tagged.

use rusqlite::Connection;

use super::{FIRST_VERSION, MIGRATIONS, migrate_with};

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

/// Layout 15 (D-132): what a book another program kept holds besides
/// accounts and transactions, so a KMyMoney book comes over without losing
/// it. New tables: `institutions` (the banks accounts are at), `payees`
/// (who the user pays, with address and how the other program recognized
/// them), `tags` and `split_tags` (which parts carry which tag). New
/// columns: an account's number, notes and institution; a transaction's
/// note and the day it was entered; a part's note, cheque number and
/// whether it was checked against a statement. `accounts` is rebuilt so an
/// account, not only a category, can sit inside another one of its kind,
/// as another program's account groups do (D-081); its rows and index stay
/// as they were. Every book's rows get none of the new data. Each recorded
/// table's `changed_` table gets the same columns, and the new tables get
/// `changed_` tables of their own. Adding an institution, a payee or a tag
/// becomes a user action, so `changes` is rebuilt with the three in its
/// list of actions, as in layout 14; its rows stay as they were.
const LAYOUT_15: &str = "
CREATE TABLE institutions (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL,
    bank_code TEXT,
    contact TEXT,
    address TEXT,
    phone TEXT
) STRICT;

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

CREATE TABLE new_accounts (
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
INSERT INTO new_accounts
    (id, name, kind, currency, account_type, closed, parent_id, stated_balance, stated_on)
    SELECT id, name, kind, currency, account_type, closed, parent_id, stated_balance, stated_on
    FROM accounts;
DROP TABLE accounts;
ALTER TABLE new_accounts RENAME TO accounts;
CREATE INDEX accounts_by_parent ON accounts (parent_id);
CREATE INDEX accounts_by_institution ON accounts (institution_id);

ALTER TABLE transactions ADD COLUMN note TEXT;
ALTER TABLE transactions ADD COLUMN entered_on TEXT
    CHECK (entered_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');

ALTER TABLE splits ADD COLUMN note TEXT;
ALTER TABLE splits ADD COLUMN number TEXT;
ALTER TABLE splits ADD COLUMN status TEXT CHECK (status IN ('cleared', 'reconciled'));

ALTER TABLE changed_accounts ADD COLUMN number TEXT;
ALTER TABLE changed_accounts ADD COLUMN notes TEXT;
ALTER TABLE changed_accounts ADD COLUMN institution_id BLOB;
ALTER TABLE changed_transactions ADD COLUMN note TEXT;
ALTER TABLE changed_transactions ADD COLUMN entered_on TEXT;
ALTER TABLE changed_splits ADD COLUMN note TEXT;
ALTER TABLE changed_splits ADD COLUMN number TEXT;
ALTER TABLE changed_splits ADD COLUMN status TEXT;

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

CREATE TABLE changed_split_tags (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    split_id BLOB,
    tag_id BLOB,
    position INTEGER,
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
        'add_institution', 'add_payee', 'add_tag'
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

/// Layout 16 (M3.14, D-135): layouts 1 to 15 merged into one, the
/// starting layout until layout 24 merged it with layouts 17 to 23.
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

/// Layout 23 (M6c.B, D-375): what a bank or file delivered stays as it was
/// delivered, also against a slip in KassenSturz or another program writing
/// to the book. The user's own changes (payee, date, note, category) live
/// in other columns, on top of the original, as before (D-067).
///
/// The triggers refuse four things. A row of `split_imports` never changes.
/// It, or a part that has one, is deleted only while `originals_unlocked`
/// holds its one row, which KassenSturz writes and deletes again in the same
/// database transaction, only around removing whole transactions the user
/// removes, undo and redo, and the edits that write a transaction's parts
/// again unchanged (`originals.rs`); it never stays in a saved book. Such a
/// part keeps its account and its amount in the account's currency, and its
/// transaction keeps its currency while the part has no amount of its own in
/// another currency. A program that writes the guard row itself can still
/// delete originals: the triggers stop mistakes, not someone set on it.
///
/// A later migration that rebuilds `splits`, `split_imports` or
/// `transactions` drops these triggers with the table and makes them again.
const LAYOUT_23: &str = "
CREATE TABLE originals_unlocked (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1)
) STRICT;
CREATE TRIGGER original_never_changes BEFORE UPDATE ON split_imports
BEGIN
    SELECT RAISE(ABORT, 'KassenSturz keeps what a bank or file delivered: original changed');
END;
CREATE TRIGGER original_kept BEFORE DELETE ON split_imports
WHEN NOT EXISTS (SELECT 1 FROM originals_unlocked)
BEGIN
    SELECT RAISE(ABORT, 'KassenSturz keeps what a bank or file delivered: original removed');
END;
CREATE TRIGGER original_part_kept BEFORE DELETE ON splits
WHEN EXISTS (SELECT 1 FROM split_imports WHERE split_id = OLD.id)
    AND NOT EXISTS (SELECT 1 FROM originals_unlocked)
BEGIN
    SELECT RAISE(ABORT, 'KassenSturz keeps what a bank or file delivered: original removed');
END;
CREATE TRIGGER original_part_unchanged
BEFORE UPDATE OF account_id, amount, foreign_amount, foreign_currency ON splits
WHEN EXISTS (SELECT 1 FROM split_imports WHERE split_id = OLD.id)
    AND (NEW.account_id IS NOT OLD.account_id
        OR coalesce(NEW.foreign_amount, NEW.amount) IS NOT coalesce(OLD.foreign_amount, OLD.amount)
        OR NEW.foreign_currency IS NOT OLD.foreign_currency)
BEGIN
    SELECT RAISE(ABORT, 'KassenSturz keeps what a bank or file delivered: original changed');
END;
CREATE TRIGGER original_currency_kept BEFORE UPDATE OF currency ON transactions
WHEN NEW.currency IS NOT OLD.currency
    AND EXISTS (
        SELECT 1 FROM splits AS s JOIN split_imports AS i ON i.split_id = s.id
        WHERE s.transaction_id = OLD.id AND s.foreign_amount IS NULL
    )
    AND NOT EXISTS (SELECT 1 FROM originals_unlocked)
BEGIN
    SELECT RAISE(ABORT, 'KassenSturz keeps what a bank or file delivered: original changed');
END;
";

/// The development layouts a book could have been built with, oldest first.
/// A book a build of M0 to M3 wrote ran layouts 1 to 15 and then, once
/// updated, 17 to 23; one made after M3.14 ran layout 16, which merged 1 to
/// 15, and then 17 to 23. Both ways end in the same layout 23.
const BUILT_SINCE_M0: [&str; 22] = [
    LAYOUT_1, LAYOUT_2, LAYOUT_3, LAYOUT_4, LAYOUT_5, LAYOUT_6, LAYOUT_7, LAYOUT_8, LAYOUT_9,
    LAYOUT_10, LAYOUT_11, LAYOUT_12, LAYOUT_13, LAYOUT_14, LAYOUT_15, LAYOUT_17, LAYOUT_18,
    LAYOUT_19, LAYOUT_20, LAYOUT_21, LAYOUT_22, LAYOUT_23,
];
const BUILT_SINCE_M3: [&str; 8] = [
    LAYOUT_16, LAYOUT_17, LAYOUT_18, LAYOUT_19, LAYOUT_20, LAYOUT_21, LAYOUT_22, LAYOUT_23,
];

/// A schema as SQLite keeps it, with the SQL of each table and index spelt
/// the same way whether it was written in one go or changed afterwards: no
/// quotes around names, which `ALTER TABLE … RENAME` adds, and spaces only
/// where two words meet.
fn schema(conn: &Connection) -> Vec<(String, String, String, Option<String>)> {
    let mut statement = conn
        .prepare("SELECT type, name, tbl_name, sql FROM sqlite_schema ORDER BY type, name")
        .expect("schema read");
    statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, Option<String>>(3)?
                    .map(|sql| spelt_one_way(&sql)),
            ))
        })
        .expect("schema read")
        .collect::<Result<_, _>>()
        .expect("schema read")
}

fn spelt_one_way(sql: &str) -> String {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut spelt = String::with_capacity(sql.len());
    let mut pending_space = false;
    for c in sql.chars().filter(|&c| c != '"') {
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && is_word(c) && spelt.chars().last().is_some_and(is_word) {
            spelt.push(' ');
        }
        pending_space = false;
        spelt.push(c);
    }
    spelt
}

fn row(conn: &Connection, sql: &str) -> (i64, String) {
    conn.query_row(sql, [], |row| {
        Ok((
            row.get(0)?,
            row.get::<_, rusqlite::types::Value>(1)
                .map(|value| format!("{value:?}"))?,
        ))
    })
    .expect("row read")
}

/// A database built with the development `layouts`, run in one go as
/// `migrate_with` ran them.
fn built_with(layouts: &[&str]) -> Connection {
    let mut old = Connection::open_in_memory().expect("database opened");
    old.pragma_update(None, "foreign_keys", false)
        .expect("foreign keys off");
    let transaction = old.transaction().expect("transaction started");
    for layout in layouts {
        transaction
            .execute_batch(layout)
            .expect("development layout");
    }
    transaction.commit().expect("committed");
    old
}

#[test]
fn the_starting_layout_is_what_the_development_layouts_built() {
    // The starting layout, as a new book gets it.
    let mut new = Connection::open_in_memory().expect("database opened");
    migrate_with(&mut new, &MIGRATIONS[..1], FIRST_VERSION, 0).expect("starting layout");
    assert_eq!(
        new.pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
            .expect("version read"),
        FIRST_VERSION
    );
    let new_schema = schema(&new);
    assert!(!new_schema.is_empty());

    for (since, layouts) in [("M0", &BUILT_SINCE_M0[..]), ("M3", &BUILT_SINCE_M3[..])] {
        let old = built_with(layouts);
        let old_schema = schema(&old);
        for (was, is) in old_schema.iter().zip(&new_schema) {
            assert_eq!(was, is, "built since {since}");
        }
        assert_eq!(old_schema.len(), new_schema.len(), "built since {since}");
        // Both start with the same settings.
        for sql in [
            "SELECT id, rounding_mode FROM settings",
            "SELECT id, keep_days FROM history_settings",
        ] {
            let (was, is) = (row(&old, sql), row(&new, sql));
            assert_eq!(was, is, "{sql}, built since {since}");
            assert_eq!(was.0, 1);
        }
    }
}
