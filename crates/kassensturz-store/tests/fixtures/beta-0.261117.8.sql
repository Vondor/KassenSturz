BEGIN TRANSACTION;
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
INSERT INTO "accounts" VALUES(X'01A1EE1CCCB1752F8EA3EDC8E39524F2','Checking account','asset','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "accounts" VALUES(X'01A1EE1CCCB1752F8EA3EDC986F5B690','Groceries','expense','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "accounts" VALUES(X'01A1EE1CCCB1752F8EA3EDCA0F258E28','Salary','income','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "accounts" VALUES(X'01A1EE1CCCB1752F8EA3EDCBF3C900A0','Dollar account','asset','USD',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
CREATE TABLE attachments (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    link TEXT NOT NULL CHECK (link <> ''),
    title TEXT,
    UNIQUE (transaction_id, position)
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
CREATE TABLE book_identity (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    book_id BLOB NOT NULL CHECK (length(book_id) = 16)
) STRICT;
INSERT INTO "book_identity" VALUES(1,X'3FE5F1E36721AEB6D0BB9CF7D741EBCE');
CREATE TABLE "budget_accounts" (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    budget_id BLOB NOT NULL REFERENCES budgets (id) ON DELETE CASCADE,
    account_id BLOB NOT NULL REFERENCES accounts (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    from_day TEXT NOT NULL
        CHECK (from_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')
) STRICT;
CREATE TABLE budget_amounts (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    budget_id BLOB NOT NULL REFERENCES budgets (id) ON DELETE CASCADE,
    from_day TEXT NOT NULL
        CHECK (from_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    amount INTEGER NOT NULL CHECK (amount > 0),
    carry_over INTEGER NOT NULL CHECK (carry_over IN (0, 1))
) STRICT;
CREATE TABLE "budget_categories" (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    budget_id BLOB NOT NULL REFERENCES budgets (id) ON DELETE CASCADE,
    category_id BLOB NOT NULL REFERENCES accounts (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    from_day TEXT NOT NULL
        CHECK (from_day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')
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
INSERT INTO "changed_accounts" VALUES(X'01A1EE1CCCB271739CA4B95238E529D9','after',X'01A1EE1CCCB1752F8EA3EDC8E39524F2','Checking account','asset','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_accounts" VALUES(X'01A1EE1CCCB575EB9390400A6778EAEB','after',X'01A1EE1CCCB1752F8EA3EDC986F5B690','Groceries','expense','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_accounts" VALUES(X'01A1EE1CCCB8739B9976104506700045','after',X'01A1EE1CCCB1752F8EA3EDCA0F258E28','Salary','income','EUR',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_accounts" VALUES(X'01A1EE1CCCBB769F92DDB126C76B7640','after',X'01A1EE1CCCB1752F8EA3EDCBF3C900A0','Dollar account','asset','USD',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL);
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
CREATE TABLE changed_budget_accounts (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    budget_id BLOB,
    account_id BLOB,
    position INTEGER, from_day TEXT,
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
CREATE TABLE changed_budget_categories (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    budget_id BLOB,
    category_id BLOB,
    position INTEGER, from_day TEXT,
    PRIMARY KEY (change_id, id, side)
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
CREATE TABLE changed_currencies (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    code TEXT NOT NULL,
    minor_digits INTEGER,
    PRIMARY KEY (change_id, code, side)
) STRICT;
INSERT INTO "changed_currencies" VALUES(X'01A1EE1CCCAB74B8A8FCF8596E120C65','after','EUR',2);
INSERT INTO "changed_currencies" VALUES(X'01A1EE1CCCB07752A46797000E16EDD8','after','USD',2);
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
INSERT INTO "changed_prices" VALUES(X'01A1EE1CCCCD73A2BADD374F0CA83F81','after',X'01A1EE1CCCCC70F896D38D1ED58FC861','2026-03-20','EUR','USD',10856,4,'manual');
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
CREATE TABLE changed_report_groups (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    kind TEXT,
    position INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_report_hidden (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    category_id BLOB NOT NULL,
    PRIMARY KEY (change_id, category_id, side)
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
CREATE TABLE changed_schedule_occurrences (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    schedule_id BLOB,
    due_on TEXT,
    status TEXT,
    transaction_id BLOB,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_schedules (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    account_id BLOB,
    amount INTEGER,
    currency TEXT,
    tolerance INTEGER,
    category_id BLOB,
    party_iban TEXT,
    party_name TEXT,
    unit TEXT,
    every INTEGER,
    anchor TEXT,
    track_from TEXT,
    ends_on TEXT,
    paused INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;
CREATE TABLE changed_settings (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id INTEGER NOT NULL,
    rounding_mode TEXT,
    PRIMARY KEY (change_id, id, side)
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
INSERT INTO "changed_split_rates" VALUES(X'01A1EE1CCCD076D1ADDB6D89DBF7BF51','after',X'01A1EE1CCCCF7455B07877814FC49236',X'01A1EE1CCCCC70F896D38D1ED58FC861','2026-03-20','EUR','USD',10856,4,'manual');
CREATE TABLE changed_split_rules (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    split_id BLOB NOT NULL,
    rule_id BLOB,
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
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCC075F588AB244E4ADEFA8A','after',X'01A1EE1CCCBF70058FC1D37BAD681FA6',X'01A1EE1CCCBF70058FC1D37A67FEE1C2',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCC075F588AB244E4ADEFA8A','after',X'01A1EE1CCCBF70058FC1D37C2D15C8F9',X'01A1EE1CCCBF70058FC1D37A67FEE1C2',1,X'01A1EE1CCCB1752F8EA3EDC986F5B690',1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCC57343B188C118EC1D1E60','after',X'01A1EE1CCCC27003A10E518F2C5CD517',X'01A1EE1CCCC27003A10E518EA33C77D9',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCC57343B188C118EC1D1E60','after',X'01A1EE1CCCC27003A10E5190C51E09CC',X'01A1EE1CCCC27003A10E518EA33C77D9',1,X'01A1EE1CCCB1752F8EA3EDC986F5B690',4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCC97514AC9F258690EF8A3E','after',X'01A1EE1CCCC877E6856E6AEFF838F748',X'01A1EE1CCCC877E6856E6AEED0BCD58E',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCC97514AC9F258690EF8A3E','after',X'01A1EE1CCCC877E6856E6AF04BC3FDD3',X'01A1EE1CCCC877E6856E6AEED0BCD58E',1,X'01A1EE1CCCB1752F8EA3EDC986F5B690',899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCCB77CAB43EC9CDB4D0A192','after',X'01A1EE1CCCCA76B0811C01F39A1013BB',X'01A1EE1CCCCA76B0811C01F2DEC70AEA',0,X'01A1EE1CCCB1752F8EA3EDCA0F258E28',-250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCCB77CAB43EC9CDB4D0A192','after',X'01A1EE1CCCCA76B0811C01F4696EE612',X'01A1EE1CCCCA76B0811C01F2DEC70AEA',1,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCD076D1ADDB6D89DBF7BF51','after',X'01A1EE1CCCCF7455B07877834C138A46',X'01A1EE1CCCCF7455B07877828B05501F',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-100000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "changed_splits" VALUES(X'01A1EE1CCCD076D1ADDB6D89DBF7BF51','after',X'01A1EE1CCCCF7455B07877814FC49236',X'01A1EE1CCCCF7455B07877828B05501F',1,X'01A1EE1CCCB1752F8EA3EDCBF3C900A0',100000,108560,'USD',NULL,NULL,NULL,NULL,NULL,NULL,NULL);
CREATE TABLE changed_tags (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    name TEXT,
    PRIMARY KEY (change_id, id, side)
) STRICT;
INSERT INTO "changed_tags" VALUES(X'01A1EE1CCCBD734990F4B22FE5AAE97A','after',X'01A1EE1CCCBC75109ECEEDAEDE982B1B','Holiday');
CREATE TABLE changed_transaction_tags (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    id BLOB NOT NULL,
    transaction_id BLOB,
    tag_id BLOB,
    position INTEGER,
    PRIMARY KEY (change_id, id, side)
) STRICT;
INSERT INTO "changed_transaction_tags" VALUES(X'01A1EE1CCCC57343B188C118EC1D1E60','after',X'01A1EE1CCCC372EA8CED50BC92F4F369',X'01A1EE1CCCC27003A10E518EA33C77D9',X'01A1EE1CCCBC75109ECEEDAEDE982B1B',0);
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
INSERT INTO "changed_transactions" VALUES(X'01A1EE1CCCC075F588AB244E4ADEFA8A','after',X'01A1EE1CCCBF70058FC1D37A67FEE1C2','2026-03-03','Corner shop','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01A1EE1CCCC57343B188C118EC1D1E60','after',X'01A1EE1CCCC27003A10E518EA33C77D9','2026-03-09','Corner shop','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01A1EE1CCCC97514AC9F258690EF8A3E','after',X'01A1EE1CCCC877E6856E6AEED0BCD58E','2026-03-17','Corner shop','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01A1EE1CCCCB77CAB43EC9CDB4D0A192','after',X'01A1EE1CCCCA76B0811C01F2DEC70AEA','2026-03-28','Employer','EUR',NULL,NULL);
INSERT INTO "changed_transactions" VALUES(X'01A1EE1CCCD076D1ADDB6D89DBF7BF51','after',X'01A1EE1CCCCF7455B07877828B05501F','2026-03-20','To the dollar account','EUR',NULL,NULL);
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
        'join_transfers',
        'add_schedule', 'edit_schedule', 'remove_schedule', 'pause_schedule',
        'match_schedules', 'match_schedule_occurrence',
        'skip_schedule_occurrence', 'unskip_schedule_occurrence',
        'book_schedule_occurrence'
    )),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1)),
    file_name TEXT
) STRICT;
INSERT INTO "changes" VALUES(X'01A1EE1CCCAB74B8A8FCF8596E120C65',1,'use_currency','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCB07752A46797000E16EDD8',2,'use_currency','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCB271739CA4B95238E529D9',3,'add_account','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCB575EB9390400A6778EAEB',4,'add_category','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCB8739B9976104506700045',5,'add_category','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCBB769F92DDB126C76B7640',6,'add_account','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCBD734990F4B22FE5AAE97A',7,'add_tag','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCC075F588AB244E4ADEFA8A',8,'add_transaction','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCC57343B188C118EC1D1E60',9,'add_transaction','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCC97514AC9F258690EF8A3E',10,'add_transaction','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCCB77CAB43EC9CDB4D0A192',11,'add_transaction','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCCD73A2BADD374F0CA83F81',12,'add_rate','2026-11-18T10:03:47Z',0,NULL);
INSERT INTO "changes" VALUES(X'01A1EE1CCCD076D1ADDB6D89DBF7BF51',13,'add_transaction','2026-11-18T10:03:47Z',0,NULL);
CREATE TABLE currencies (
    code TEXT PRIMARY KEY NOT NULL
        CHECK (length(code) = 3 AND code NOT GLOB '*[^A-Z]*'),
    minor_digits INTEGER NOT NULL CHECK (minor_digits BETWEEN 0 AND 18)
) STRICT;
INSERT INTO "currencies" VALUES('EUR',2);
INSERT INTO "currencies" VALUES('USD',2);
CREATE TABLE history_settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    keep_days INTEGER CHECK (keep_days BETWEEN 0 AND 4294967295)
) STRICT;
INSERT INTO "history_settings" VALUES(1,30);
CREATE TABLE institutions (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL,
    bank_code TEXT,
    contact TEXT,
    address TEXT,
    phone TEXT
) STRICT;
CREATE TABLE originals_unlocked (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1)
) STRICT;
CREATE TABLE parked_ai_rule_marks (
    rule_id BLOB PRIMARY KEY NOT NULL CHECK (length(rule_id) = 16)
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
INSERT INTO "prices" VALUES(X'01A1EE1CCCCC70F896D38D1ED58FC861','2026-03-20','EUR','USD',10856,4,'manual');
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
CREATE TABLE report_group_members (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    group_id BLOB NOT NULL REFERENCES report_groups (id) ON DELETE CASCADE,
    category_id BLOB REFERENCES accounts (id) ON DELETE CASCADE,
    member_group_id BLOB REFERENCES report_groups (id) ON DELETE CASCADE
        CHECK (member_group_id IS NULL OR member_group_id <> group_id),
    position INTEGER NOT NULL CHECK (position >= 0),
    CHECK ((category_id IS NULL) <> (member_group_id IS NULL))
) STRICT;
CREATE TABLE report_groups (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL CHECK (name <> '' AND name = trim(name) AND length(name) <= 200),
    kind TEXT NOT NULL CHECK (kind IN ('expense', 'income')),
    position INTEGER NOT NULL CHECK (position >= 0)
) STRICT;
CREATE TABLE report_hidden (
    category_id BLOB PRIMARY KEY NOT NULL CHECK (length(category_id) = 16)
        REFERENCES accounts (id) ON DELETE CASCADE
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
CREATE TABLE schedule_occurrences (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    schedule_id BLOB NOT NULL REFERENCES schedules (id) ON DELETE CASCADE,
    due_on TEXT NOT NULL
        CHECK (due_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    status TEXT NOT NULL CHECK (status IN ('matched', 'skipped')),
    transaction_id BLOB REFERENCES transactions (id) ON DELETE CASCADE,
    CHECK ((status = 'matched') = (transaction_id IS NOT NULL))
) STRICT;
CREATE TABLE schedules (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 60),
    account_id BLOB NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    amount INTEGER NOT NULL CHECK (amount <> 0),
    currency TEXT NOT NULL REFERENCES currencies (code),
    tolerance INTEGER NOT NULL CHECK (tolerance >= 0 AND tolerance < abs(amount)),
    category_id BLOB REFERENCES accounts (id) ON DELETE RESTRICT,
    party_iban TEXT CHECK (length(party_iban) BETWEEN 1 AND 140),
    party_name TEXT CHECK (length(party_name) BETWEEN 1 AND 140),
    unit TEXT NOT NULL CHECK (unit IN ('week', 'month', 'year')),
    every INTEGER NOT NULL CHECK (every BETWEEN 1 AND 52),
    anchor TEXT NOT NULL
        CHECK (anchor GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    track_from TEXT NOT NULL
        CHECK (track_from GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    ends_on TEXT
        CHECK (ends_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    paused INTEGER NOT NULL CHECK (paused IN (0, 1)),
    CHECK (track_from >= anchor),
    CHECK (ends_on IS NULL OR ends_on >= anchor)
) STRICT;
CREATE TABLE settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    rounding_mode TEXT NOT NULL CHECK (rounding_mode IN (
        'half_away_from_zero', 'half_even', 'half_toward_zero',
        'toward_zero', 'away_from_zero', 'floor', 'ceiling'
    ))
) STRICT;
INSERT INTO "settings" VALUES(1,'half_away_from_zero');
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
INSERT INTO "split_rates" VALUES(X'01A1EE1CCCCF7455B07877814FC49236',X'01A1EE1CCCCC70F896D38D1ED58FC861','2026-03-20','EUR','USD',10856,4,'manual');
CREATE TABLE split_rules (
    split_id BLOB PRIMARY KEY NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    rule_id BLOB REFERENCES rules (id) ON DELETE SET NULL
) STRICT;
CREATE TABLE split_tags (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    split_id BLOB NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    tag_id BLOB NOT NULL REFERENCES tags (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (split_id, tag_id)
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
INSERT INTO "splits" VALUES(X'01A1EE1CCCBF70058FC1D37BAD681FA6',X'01A1EE1CCCBF70058FC1D37A67FEE1C2',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCBF70058FC1D37C2D15C8F9',X'01A1EE1CCCBF70058FC1D37A67FEE1C2',1,X'01A1EE1CCCB1752F8EA3EDC986F5B690',1250,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCC27003A10E518F2C5CD517',X'01A1EE1CCCC27003A10E518EA33C77D9',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCC27003A10E5190C51E09CC',X'01A1EE1CCCC27003A10E518EA33C77D9',1,X'01A1EE1CCCB1752F8EA3EDC986F5B690',4710,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCC877E6856E6AEFF838F748',X'01A1EE1CCCC877E6856E6AEED0BCD58E',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCC877E6856E6AF04BC3FDD3',X'01A1EE1CCCC877E6856E6AEED0BCD58E',1,X'01A1EE1CCCB1752F8EA3EDC986F5B690',899,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCCA76B0811C01F39A1013BB',X'01A1EE1CCCCA76B0811C01F2DEC70AEA',0,X'01A1EE1CCCB1752F8EA3EDCA0F258E28',-250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCCA76B0811C01F4696EE612',X'01A1EE1CCCCA76B0811C01F2DEC70AEA',1,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',250000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCCF7455B07877834C138A46',X'01A1EE1CCCCF7455B07877828B05501F',0,X'01A1EE1CCCB1752F8EA3EDC8E39524F2',-100000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "splits" VALUES(X'01A1EE1CCCCF7455B07877814FC49236',X'01A1EE1CCCCF7455B07877828B05501F',1,X'01A1EE1CCCB1752F8EA3EDCBF3C900A0',100000,108560,'USD',NULL,NULL,NULL,NULL,NULL,NULL,NULL);
CREATE TABLE tags (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    name TEXT NOT NULL
) STRICT;
INSERT INTO "tags" VALUES(X'01A1EE1CCCBC75109ECEEDAEDE982B1B','Holiday');
CREATE TABLE transaction_tags (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    tag_id BLOB NOT NULL REFERENCES tags (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (transaction_id, tag_id)
) STRICT;
INSERT INTO "transaction_tags" VALUES(X'01A1EE1CCCC372EA8CED50BC92F4F369',X'01A1EE1CCCC27003A10E518EA33C77D9',X'01A1EE1CCCBC75109ECEEDAEDE982B1B',0);
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
INSERT INTO "transactions" VALUES(X'01A1EE1CCCBF70058FC1D37A67FEE1C2','2026-03-03','Corner shop','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01A1EE1CCCC27003A10E518EA33C77D9','2026-03-09','Corner shop','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01A1EE1CCCC877E6856E6AEED0BCD58E','2026-03-17','Corner shop','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01A1EE1CCCCA76B0811C01F2DEC70AEA','2026-03-28','Employer','EUR',NULL,NULL);
INSERT INTO "transactions" VALUES(X'01A1EE1CCCCF7455B07877828B05501F','2026-03-20','To the dollar account','EUR',NULL,NULL);
CREATE TABLE unchecked_ai_rules (
    rule_id BLOB PRIMARY KEY NOT NULL REFERENCES rules (id) ON DELETE CASCADE
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
CREATE INDEX schedules_by_account ON schedules (account_id);
CREATE INDEX schedules_by_category ON schedules (category_id);
CREATE UNIQUE INDEX schedule_occurrences_once ON schedule_occurrences (schedule_id, due_on);
CREATE UNIQUE INDEX schedule_occurrences_by_transaction
    ON schedule_occurrences (transaction_id);
COMMIT;
