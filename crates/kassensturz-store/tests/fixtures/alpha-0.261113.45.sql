BEGIN TRANSACTION;
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
CREATE TABLE views (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    kind TEXT NOT NULL CHECK (kind IN ('view', 'dashboard')),
    name TEXT NOT NULL CHECK (name <> '' AND name = trim(name) AND length(name) <= 200),
    position INTEGER NOT NULL CHECK (position >= 0),
    format_version INTEGER NOT NULL CHECK (format_version >= 1),
    document TEXT NOT NULL
        CHECK (json_valid(document) AND length(CAST(document AS BLOB)) <= 1048576)
) STRICT;
CREATE TABLE report_groups (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL CHECK (name <> '' AND name = trim(name) AND length(name) <= 200),
    kind TEXT NOT NULL CHECK (kind IN ('expense', 'income')),
    position INTEGER NOT NULL CHECK (position >= 0)
) STRICT;
CREATE TABLE report_group_members (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    group_id BLOB NOT NULL REFERENCES report_groups (id) ON DELETE CASCADE,
    category_id BLOB REFERENCES accounts (id) ON DELETE CASCADE,
    member_group_id BLOB REFERENCES report_groups (id) ON DELETE CASCADE
        CHECK (member_group_id IS NULL OR member_group_id <> group_id),
    position INTEGER NOT NULL CHECK (position >= 0),
    CHECK ((category_id IS NULL) <> (member_group_id IS NULL))
) STRICT;
CREATE TABLE report_hidden (
    category_id BLOB PRIMARY KEY NOT NULL CHECK (length(category_id) = 16)
        REFERENCES accounts (id) ON DELETE CASCADE
) STRICT;
CREATE TABLE budget_amounts (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    budget_id BLOB NOT NULL REFERENCES budgets (id) ON DELETE CASCADE,
    from_day TEXT NOT NULL
        CHECK (from_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    amount INTEGER NOT NULL CHECK (amount > 0),
    carry_over INTEGER NOT NULL CHECK (carry_over IN (0, 1))
) STRICT;
CREATE TABLE originals_unlocked (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1)
) STRICT;
CREATE TABLE settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    rounding_mode TEXT NOT NULL CHECK (rounding_mode IN (
        'half_away_from_zero', 'half_even', 'half_toward_zero',
        'toward_zero', 'away_from_zero', 'floor', 'ceiling'
    ))
) STRICT;
CREATE TABLE history_settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    keep_days INTEGER CHECK (keep_days BETWEEN 0 AND 4294967295)
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
CREATE TABLE "budgets" (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    every TEXT NOT NULL CHECK (every IN ('month', 'quarter', 'year', 'range')),
    first_day TEXT NOT NULL
        CHECK (first_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    last_day TEXT
        CHECK (last_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'), salary_day INTEGER
    CHECK (salary_day IS NULL OR (every = 'month' AND salary_day BETWEEN 2 AND 31)), name TEXT
    CHECK (name IS NULL OR length(name) BETWEEN 1 AND 100),
    CHECK (last_day IS NULL OR last_day >= first_day),
    CHECK (every <> 'range' OR last_day IS NOT NULL)
) STRICT;
CREATE TABLE "changed_budgets" (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    every TEXT,
    first_day TEXT,
    last_day TEXT, salary_day INTEGER, name TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_budget_categories (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    budget_id BLOB,
    category_id BLOB,
    position INTEGER, from_day TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_budget_accounts (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    budget_id BLOB,
    account_id BLOB,
    position INTEGER, from_day TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE "budget_categories" (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    budget_id BLOB NOT NULL REFERENCES budgets (id) ON DELETE CASCADE,
    category_id BLOB NOT NULL REFERENCES accounts (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    from_day TEXT NOT NULL
        CHECK (from_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')
) STRICT;
CREATE TABLE "budget_accounts" (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    budget_id BLOB NOT NULL REFERENCES budgets (id) ON DELETE CASCADE,
    account_id BLOB NOT NULL REFERENCES accounts (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    from_day TEXT NOT NULL
        CHECK (from_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')
) STRICT;
CREATE TABLE transaction_tags (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    tag_id BLOB NOT NULL REFERENCES tags (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (transaction_id, tag_id)
) STRICT;
CREATE TABLE changed_transaction_tags (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    transaction_id BLOB,
    tag_id BLOB,
    position INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE recurring_moves (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    from_account_id BLOB NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    to_account_id BLOB NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    amount INTEGER NOT NULL CHECK (amount > 0),
    currency TEXT NOT NULL REFERENCES currencies (code),
    day_of_month INTEGER NOT NULL CHECK (day_of_month BETWEEN 1 AND 31),
    from_party_iban TEXT,
    from_party_name TEXT,
    created_on TEXT NOT NULL
        CHECK (created_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    CHECK (from_account_id <> to_account_id)
) STRICT;
CREATE TABLE changed_recurring_moves (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    from_account_id BLOB,
    to_account_id BLOB,
    amount INTEGER,
    currency TEXT,
    day_of_month INTEGER,
    from_party_iban TEXT,
    from_party_name TEXT,
    created_on TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE "changes" (
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
        'add_budget', 'edit_budget', 'remove_budget',
        'change_account_type',
        'rename_tag', 'remove_tag', 'merge_tags',
        'add_recurring_move', 'remove_recurring_move', 'join_recurring_moves',
        'join_transfers'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1)),
    file_name TEXT
) STRICT;
CREATE TABLE unchecked_ai_rules (
    rule_id BLOB PRIMARY KEY NOT NULL REFERENCES rules (id) ON DELETE CASCADE
) STRICT;
INSERT INTO "currencies" VALUES('EUR',2);
INSERT INTO "currencies" VALUES('USD',2);
INSERT INTO "accounts" VALUES(X'01a1dc33c7d974b7857c9827313d8b24','Checking account','asset','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "accounts" VALUES(X'01a1dc33c7dc74e2871f8387e5acc889','Groceries','expense','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "accounts" VALUES(X'01a1dc33c7de71a68f36ef49986ec9c4','Salary','income','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "accounts" VALUES(X'01a1dc33c7e177f0b100052e5118150c','Dollar account','asset','USD',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "transactions" VALUES(X'01a1dc33c7e472a38da3325c52a123dc','2026-03-03','Corner shop','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01a1dc33c7e67544b2f5509459eccc0b','2026-03-09','Corner shop','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01a1dc33c7e97398972e48fe5c9e5f40','2026-03-17','Corner shop','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01a1dc33c7eb739ebf239aa4e2b6547a','2026-03-28','Employer','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01a1dc33c7ed74a882c0db7e6f6dcb05','2026-03-20','To the dollar account','EUR',NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7e472a38da3325ab404a6b0',X'01a1dc33c7e472a38da3325c52a123dc',0,X'01a1dc33c7d974b7857c9827313d8b24',-1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7e472a38da3325b708b94aa',X'01a1dc33c7e472a38da3325c52a123dc',1,X'01a1dc33c7dc74e2871f8387e5acc889',1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7e67544b2f55092355effe7',X'01a1dc33c7e67544b2f5509459eccc0b',0,X'01a1dc33c7d974b7857c9827313d8b24',-4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7e67544b2f55093af8eaa8a',X'01a1dc33c7e67544b2f5509459eccc0b',1,X'01a1dc33c7dc74e2871f8387e5acc889',4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7e97398972e48fc2b682391',X'01a1dc33c7e97398972e48fe5c9e5f40',0,X'01a1dc33c7d974b7857c9827313d8b24',-899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7e97398972e48fdf8cbc912',X'01a1dc33c7e97398972e48fe5c9e5f40',1,X'01a1dc33c7dc74e2871f8387e5acc889',899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7eb739ebf239aa21f626a55',X'01a1dc33c7eb739ebf239aa4e2b6547a',0,X'01a1dc33c7d974b7857c9827313d8b24',250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7eb739ebf239aa35569fafd',X'01a1dc33c7eb739ebf239aa4e2b6547a',1,X'01a1dc33c7de71a68f36ef49986ec9c4',-250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7ed74a882c0db7f72dae60f',X'01a1dc33c7ed74a882c0db7e6f6dcb05',0,X'01a1dc33c7d974b7857c9827313d8b24',-100000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01a1dc33c7ed74a882c0db7d422e02dc',X'01a1dc33c7ed74a882c0db7e6f6dcb05',1,X'01a1dc33c7e177f0b100052e5118150c',100000,108560,'USD',NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "split_rates" VALUES(X'01a1dc33c7ed74a882c0db7d422e02dc',X'01a1dc33c7ed74a882c0db7cea5bb1aa','2026-03-20','EUR','USD',10856,4,'manual');
INSERT INTO "settings" VALUES(1,'half_away_from_zero');
INSERT INTO "history_settings" VALUES(1,30);
INSERT INTO "changed_currencies" VALUES(X'01a1dc33c7d277e9b20510c2e6e2023b','after','EUR',2);
INSERT INTO "changed_currencies" VALUES(X'01a1dc33c7d775a58752b2dfdc60b769','after','USD',2);
INSERT INTO "changed_accounts" VALUES(X'01a1dc33c7da72e696099a5b9849230f','after',X'01a1dc33c7d974b7857c9827313d8b24','Checking account','asset','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_accounts" VALUES(X'01a1dc33c7dd72d391e5dcd426c27e21','after',X'01a1dc33c7dc74e2871f8387e5acc889','Groceries','expense','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_accounts" VALUES(X'01a1dc33c7df70269f5620bb8cffde9c','after',X'01a1dc33c7de71a68f36ef49986ec9c4','Salary','income','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_accounts" VALUES(X'01a1dc33c7e27374ae21a6e5420f3a78','after',X'01a1dc33c7e177f0b100052e5118150c','Dollar account','asset','USD',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01a1dc33c7e574ce88be60188019ceb8','after',X'01a1dc33c7e472a38da3325c52a123dc','2026-03-03','Corner shop','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01a1dc33c7e7747280b84477f7f871af','after',X'01a1dc33c7e67544b2f5509459eccc0b','2026-03-09','Corner shop','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01a1dc33c7e97398972e48ff2c83701d','after',X'01a1dc33c7e97398972e48fe5c9e5f40','2026-03-17','Corner shop','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01a1dc33c7eb739ebf239aa5af1577cb','after',X'01a1dc33c7eb739ebf239aa4e2b6547a','2026-03-28','Employer','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01a1dc33c7ee7644af2982d36f68944a','after',X'01a1dc33c7ed74a882c0db7e6f6dcb05','2026-03-20','To the dollar account','EUR',NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7e574ce88be60188019ceb8','after',X'01a1dc33c7e472a38da3325ab404a6b0',X'01a1dc33c7e472a38da3325c52a123dc',0,X'01a1dc33c7d974b7857c9827313d8b24',-1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7e574ce88be60188019ceb8','after',X'01a1dc33c7e472a38da3325b708b94aa',X'01a1dc33c7e472a38da3325c52a123dc',1,X'01a1dc33c7dc74e2871f8387e5acc889',1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7e7747280b84477f7f871af','after',X'01a1dc33c7e67544b2f55092355effe7',X'01a1dc33c7e67544b2f5509459eccc0b',0,X'01a1dc33c7d974b7857c9827313d8b24',-4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7e7747280b84477f7f871af','after',X'01a1dc33c7e67544b2f55093af8eaa8a',X'01a1dc33c7e67544b2f5509459eccc0b',1,X'01a1dc33c7dc74e2871f8387e5acc889',4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7e97398972e48ff2c83701d','after',X'01a1dc33c7e97398972e48fc2b682391',X'01a1dc33c7e97398972e48fe5c9e5f40',0,X'01a1dc33c7d974b7857c9827313d8b24',-899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7e97398972e48ff2c83701d','after',X'01a1dc33c7e97398972e48fdf8cbc912',X'01a1dc33c7e97398972e48fe5c9e5f40',1,X'01a1dc33c7dc74e2871f8387e5acc889',899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7eb739ebf239aa5af1577cb','after',X'01a1dc33c7eb739ebf239aa21f626a55',X'01a1dc33c7eb739ebf239aa4e2b6547a',0,X'01a1dc33c7d974b7857c9827313d8b24',250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7eb739ebf239aa5af1577cb','after',X'01a1dc33c7eb739ebf239aa35569fafd',X'01a1dc33c7eb739ebf239aa4e2b6547a',1,X'01a1dc33c7de71a68f36ef49986ec9c4',-250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7ee7644af2982d36f68944a','after',X'01a1dc33c7ed74a882c0db7f72dae60f',X'01a1dc33c7ed74a882c0db7e6f6dcb05',0,X'01a1dc33c7d974b7857c9827313d8b24',-100000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01a1dc33c7ee7644af2982d36f68944a','after',X'01a1dc33c7ed74a882c0db7d422e02dc',X'01a1dc33c7ed74a882c0db7e6f6dcb05',1,X'01a1dc33c7e177f0b100052e5118150c',100000,108560,'USD',NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_split_rates" VALUES(X'01a1dc33c7ee7644af2982d36f68944a','after',X'01a1dc33c7ed74a882c0db7d422e02dc',X'01a1dc33c7ed74a882c0db7cea5bb1aa','2026-03-20','EUR','USD',10856,4,'manual');
INSERT INTO "changes" VALUES(X'01a1dc33c7d277e9b20510c2e6e2023b',1,'use_currency','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7d775a58752b2dfdc60b769',2,'use_currency','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7da72e696099a5b9849230f',3,'add_account','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7dd72d391e5dcd426c27e21',4,'add_category','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7df70269f5620bb8cffde9c',5,'add_category','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7e27374ae21a6e5420f3a78',6,'add_account','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7e574ce88be60188019ceb8',7,'add_transaction','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7e7747280b84477f7f871af',8,'add_transaction','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7e97398972e48ff2c83701d',9,'add_transaction','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7eb739ebf239aa5af1577cb',10,'add_transaction','2026-11-14T22:35:43Z',0,NULL);
INSERT INTO "changes" VALUES(X'01a1dc33c7ee7644af2982d36f68944a',11,'add_transaction','2026-11-14T22:35:43Z',0,NULL);
CREATE INDEX accounts_by_parent ON accounts (parent_id);
CREATE INDEX accounts_by_institution ON accounts (institution_id);
CREATE INDEX splits_by_account ON splits (account_id);
CREATE INDEX split_tags_by_tag ON split_tags (tag_id);
CREATE INDEX rules_by_parent ON rules (parent_id);
CREATE INDEX split_rules_by_rule ON split_rules (rule_id);
CREATE UNIQUE INDEX bank_links_by_iban ON bank_links (iban) WHERE iban IS NOT NULL;
CREATE UNIQUE INDEX bank_links_by_number ON bank_links (bank_code, number)
    WHERE number IS NOT NULL;
CREATE UNIQUE INDEX views_by_name ON views (kind, lower(name));
CREATE UNIQUE INDEX report_groups_by_name ON report_groups (lower(name));
CREATE INDEX report_group_members_in_group ON report_group_members (group_id, position);
CREATE UNIQUE INDEX report_group_members_by_category
    ON report_group_members (category_id, group_id);
CREATE UNIQUE INDEX report_group_members_by_group
    ON report_group_members (member_group_id, group_id);
CREATE UNIQUE INDEX budget_amounts_by_day ON budget_amounts (budget_id, from_day);
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
CREATE INDEX budgets_by_first_day ON budgets (first_day);
CREATE UNIQUE INDEX budget_categories_in_set
    ON budget_categories (budget_id, from_day, category_id);
CREATE INDEX budget_categories_by_category ON budget_categories (category_id);
CREATE UNIQUE INDEX budget_accounts_in_set ON budget_accounts (budget_id, from_day, account_id);
CREATE INDEX budget_accounts_by_account ON budget_accounts (account_id);
CREATE INDEX transaction_tags_by_tag ON transaction_tags (tag_id);
CREATE UNIQUE INDEX recurring_moves_once ON recurring_moves (
    from_account_id, to_account_id, amount, currency,
    coalesce(from_party_iban, ''), coalesce(from_party_name, '')
);
CREATE INDEX recurring_moves_by_to_account ON recurring_moves (to_account_id);
COMMIT;
