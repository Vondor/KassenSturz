# Book format

How a KassenSturz household book is stored, so other programs can read it (D-046). This is documentation, not a promise: the layout can change in any version, and writing into a book from outside KassenSturz isn't supported, because it would bypass KassenSturz's checks. The stable way to move data is the complete export (D-029), whose format is described in `docs/EXPORT_FORMAT.md` (D-100). Every change to the layout updates this document.

**Status: layout 33** (US7, #795, D-769): layout 24, the starting layout of the first Beta (M6e.M, D-045, D-536), with nine migrations after it. The 23 development layouts of the Alpha were merged into it: every table a book has is built in one go, table for table what layouts 1 to 23 built together. A book a development build wrote (layouts 1 to 23) isn't opened by the Beta but turned away with `store.development-layout` and left unchanged; its data moves over through the complete export made with that build (`docs/EXPORT_FORMAT.md`), read into a new book. From the first Beta on, every change to the layout is a migration of its own, run after a backup, with a test that updates a book from every earlier layout since layout 24 (see [Migrations](#migrations), AGENTS.md §3.1 rule 8), and this document is updated with each.

## The file

- One SQLite database per book (D-033), with the extension `.ksz` (D-037).
- `PRAGMA application_id` is `0x4B535A42` (1263753794), the letters `KSZB` for KassenSturz book (D-037, D-080). KassenSturz reads it, and SQLite's header, from the file's first bytes before SQLite opens the file. Only a file with both is opened, so another program's SQLite file is never changed, not even its journal mode.
- `PRAGMA user_version` holds the layout version: currently `25`. Versions 1 to 23 belong to books from development builds of the Alpha, which KassenSturz turns away (D-045, D-536).
- The journal mode is the classic rollback journal (`DELETE`), never WAL, so no lasting side files sit next to the book (D-033). KassenSturz writes with `PRAGMA synchronous = EXTRA`, which also syncs the folder once the journal is deleted, so a save is on the disk when it returns, also after a power cut (B-278). Another program writing to a book should do the same. While the book is open, its [lock file](#the-lock-file) sits next to it.
- Foreign keys are enforced (`PRAGMA foreign_keys = ON` on every connection).
- Deleted content is overwritten (`PRAGMA secure_delete = ON` on every connection), so what KassenSturz deletes, such as a change history the user cleans up, is gone from the file too, though not from copies of it such as backups (D-066, D-086).
- **The layout is checked at every open (B-245).** Before anything in the book runs, KassenSturz compares its `sqlite_schema` (every table, index, trigger and view, with its SQL) with what its own migrations build for that layout version. A book with anything more, less or different, e.g. an index or a trigger another program added, or one of the book's own triggers dropped, is refused with `store.corrupt.unexpected-layout` and left unchanged; so is reading it through `kassensturz-mcp`. Every connection also runs with `trusted_schema` off and SQLite's defensive mode on. Another program may read a book, but must not add anything to its layout.

## Conventions

- Every table is `STRICT`: SQLite refuses a value of the wrong type instead of converting it.
- **IDs** are UUIDs, version 7 (D-067), stored as their 16 bytes in a `BLOB`.
- **Amounts** are `INTEGER`s in the currency's minor units, e.g. `5320` for €53.20, never `REAL` (AGENTS.md §3.1 rule 1). Positive amounts flow into an account, negative ones out of it.
- **Dates** are ISO 8601 calendar dates as `TEXT`, e.g. `2026-10-05`, without time or time zone (D-061).
- **Currencies** are ISO 4217 codes as `TEXT`, e.g. `EUR`.
- **Exchange rates** are fixed-point decimals in two `INTEGER` columns: their digits and their number of decimal places, e.g. `10856` and `4` for 1.0856 (D-051). The decimal places are kept as entered, so 1.0850 is `10850` and `4`. Never `REAL`.

## Tables

### `settings`

The book's settings: exactly one row. They are stored in the book, not in the app, so every computer calculates the same way.

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER, primary key | Always `1` |
| `rounding_mode` | TEXT | How calculations round a result that falls between two minor units, e.g. a third of €1.00 (D-027). One of `half_away_from_zero` (the default: 2.5 → 3, −2.5 → −3), `half_even` (2.5 → 2, 3.5 → 4), `half_toward_zero` (2.5 → 2), `toward_zero` (2.9 → 2), `away_from_zero` (2.1 → 3), `floor` (−2.1 → −3) or `ceiling` (2.1 → 3). Changing it never changes stored amounts, only later calculations |

### `currencies`

The currencies the book uses. A currency's decimal places are taken from KassenSturz's ISO 4217 table when the book first uses it (D-062) and stay fixed from then on, so an update to that table can never rescale stored amounts (D-027).

A withdrawn currency such as the German mark (DEM) is listed here too, with the decimal places it had while it was current, once a part keeps an [original amount](#splits) in it (D-104). No account can be in one, except one the book had in a currency withdrawn later (D-104, Q-068).

| Column | Type | Meaning |
|---|---|---|
| `code` | TEXT, primary key | ISO 4217 code: three capital letters |
| `minor_digits` | INTEGER, 0–18 | Decimal places: 2 for EUR, 0 for JPY |

### `institutions`

The banks the user's accounts are held at (D-132). Nothing checks the texts; they are kept as the user or an imported file gave them.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT | The bank's name, never empty. Two banks may share a name |
| `bank_code` | TEXT, may be NULL | Its bank code, sort code or BIC |
| `contact` | TEXT, may be NULL | The user's contact there |
| `address` | TEXT, may be NULL | Its address, lines separated by line breaks |
| `phone` | TEXT, may be NULL | Its phone number |

### `accounts`

Accounts and categories (`docs/GLOSSARY.md`).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT | The name the user gave it |
| `kind` | TEXT | `asset` or `liability` (shown as accounts), `income` or `expense` (shown as categories), `equity` (internal, for starting balances) |
| `currency` | TEXT → `currencies.code` | The currency of its amounts |
| `account_type` | TEXT, may be NULL | What sort of account it is, if known: `checking`, `savings`, `cash` or `credit_card`. Only accounts have one, never categories. `checking`, `savings` and `cash` need `kind` `asset`, `credit_card` needs `liability` (D-081). The user can change it later; a change to or from `credit_card` changes `kind` with it, and nothing else of the account or its parts (D-625) |
| `closed` | INTEGER, 0 or 1 | `1` if the user closed the account: its history stays, but it takes no new transactions and is hidden from the lists. Only an account whose balance is zero can be closed |
| `parent_id` | BLOB (16 bytes) → `accounts.id`, may be NULL | For a category: the category it sits inside, e.g. Groceries inside Food. Also for an account: the account it was grouped under, as a KMyMoney book groups them (D-132); the lists still show every account on its own. NULL for a top-level category or account. The `equity` account never has one, and no row is its own parent |
| `stated_balance` | INTEGER, may be NULL | The balance the user stated for the account, taken from the bank, in minor units of `currency`: what the account should hold at the end of `stated_on`. For a debt it is negative, like the account's parts. Set when the user adds an account with a balance or corrects its starting balance, NULL when they never did. Only `asset` and `liability` accounts can have one (D-111, D-117) |
| `stated_on` | TEXT, may be NULL | The day the stated balance was for, ISO 8601. NULL exactly when `stated_balance` is |
| `number` | TEXT, may be NULL | The account's number or IBAN at its bank, as the user or an imported file gave it. Only `asset` and `liability` accounts can have one (D-132) |
| `notes` | TEXT, may be NULL | What the user noted about the account or category |
| `institution_id` | BLOB (16 bytes) → `institutions.id`, may be NULL | The bank the account is held at. Only `asset` and `liability` accounts can have one |

Indexes: `accounts_by_parent` on `accounts (parent_id)`, `accounts_by_institution` on `accounts (institution_id)`.

**The category tree** (D-082): categories are the accounts of kind `income` or `expense`, and `parent_id` links each to the one it sits inside, to any depth. A category sits inside a category of its own kind and currency, so a category's total can add up the ones inside it. A tree has no loops. Siblings, meaning the categories of one kind inside the same category (or all top-level ones), never share a name, ignoring capitals and spaces around it. KassenSturz checks all of this when it writes and again when it reads the tree; the database itself only refuses a parent that doesn't exist, that is the row itself, or that is given to the `equity` account.

**Account groups** (D-132): an account's `parent_id` names an account of the same kind (`asset` under `asset`, `liability` under `liability`), in any currency, with no loops. KassenSturz checks this when it writes. A group adds nothing up and hides nothing: the account lists show every account on its own.

Account names are never empty. No two accounts (kind `asset` or `liability`) share a name, ignoring capitals and spaces around it, closed ones included. KassenSturz checks this when it writes, not the database (D-081).

### `bank_links`

Which bank account an account is filled from, and the day it was last fetched (Q-086, D-148). A link names the bank account so that KassenSturz finds it again in whatever bank login lists it, on this computer or another: by IBAN, or by the bank's code and the number the bank gives when there is no IBAN, as for a credit card. The book holds **no login data**: not the login name, not the PIN, nothing from the keychain. The bank login lives in AqBanking's folder on each computer (D-139), so a book opened on another computer only needs the login set up again there.

| Column | Type | Meaning |
|---|---|---|
| `account_id` | BLOB → `accounts.id`, primary key | The account; deleting it deletes this row. One link per account, and only an `asset` or `liability` account has one, which KassenSturz checks when it writes |
| `iban` | TEXT, may be NULL | The bank account's IBAN, without spaces and in capitals (letters and digits only); NULL if it has none |
| `bank_code` | TEXT, may be NULL | The bank's code in its country, e.g. a German bank code, as the login knows it. Never empty |
| `number` | TEXT, may be NULL | The number the bank lists the account by when it has no IBAN, e.g. a card number as the bank shows it. Never empty |
| `last_fetched_on` | TEXT, may be NULL | The day the account was last fetched from the bank, ISO 8601, by the clock of the computer that fetched it. NULL before the first fetch |

A row has an `iban`, or a `bank_code` with a `number`, or all three; the database checks this. No two accounts are linked to the same bank account: `iban` is unique, and so is `bank_code` with `number` (the unique indexes `bank_links_by_iban` and `bank_links_by_number`, which skip NULLs).

### `views`

The saved views and dashboards (D-278, D-279): JSON documents the user made in the Statistics workbench or on a dashboard. A *view* is one saved chart or table; a *dashboard* is several components on a grid. The built-in default dashboard is not stored here until the user edits it or makes a second dashboard; every stored dashboard is one of the dashboards the Overview switches between, in the order of `position` (D-436). A dashboard's document may also carry `template`, the word of the template it was made from (D-439), and `"default": true` on the one the book opens at (D-448); both are additive document keys, so no column changes. The book keeps the document as text and copies out the few things a list needs; it does not look inside the document any further, which the reporting layer does (D-281). A document is data: it holds no program, and nothing in it is run.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `kind` | TEXT | `view` or `dashboard`; the document's `kind` |
| `name` | TEXT | The document's `name`: 1 to 200 characters, no spaces around it. Unique among the documents of the same kind, ignoring capitals (the unique index `views_by_name`, which folds ASCII capitals only; KassenSturz checks all letters when it writes) |
| `position` | INTEGER, from 0 | The place in the list of its kind. KassenSturz keeps each kind numbered 0, 1, 2, … without gaps; the database does not check this |
| `format_version` | INTEGER, 1 or more | The document's `version`, so a list can be read without parsing the document |
| `document` | TEXT | The JSON document: valid JSON (`json_valid`), at most 1 MiB. It is an object with `format` (`kassensturz-dashboard`), `version` (a whole number), `kind` and `name`; KassenSturz checks those four when it writes, the database checks only that it is valid JSON and not too big |

`kind`, `name` and `format_version` are copies of what the document says. KassenSturz writes them together, so they agree; a program that reads the table should trust the document. A document with a `version` newer than the program knows is kept as it is, and refused only when it is shown (`report.unsupported-document-version`).

### `report_groups`

The user's category groups, "My groups" (D-290): categories put together under a name of the user's own, e.g. "Fixed costs", so a chart can show them as one slice, bar or line. A group is a way of looking at the figures and nothing else: it is kept apart from the transactions and the category tree (Q-221), refers to categories only by ID, and is never written onto a transaction or a category. How a group counts is the reports' business (D-291): a category with everything inside it, each category once.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT | 1 to 200 characters, no spaces around it. Unique among the groups ignoring capitals (the unique index `report_groups_by_name`, which folds ASCII capitals only; KassenSturz checks all letters when it writes). KassenSturz also refuses a category's name when the group gets its name, so a formula's name stays clear (D-292); a category made later may share it |
| `kind` | TEXT | `expense` or `income`: every member is a category or group of this kind, so the group's figure is money spent or money received. KassenSturz checks this when it writes |
| `position` | INTEGER, from 0 | The place in the list. KassenSturz keeps the groups numbered 0, 1, 2, … without gaps; the database does not check this |

KassenSturz also checks when it writes that no group ends up inside itself, directly or through other groups, that groups go at most four levels deep (a group, a group inside it, and so on), and that a book has at most 200 groups with at most 500 members each. The database checks none of this.

### `report_group_members`

The members of each group, in their order (D-290).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7, so the change history can name the row |
| `group_id` | BLOB → `report_groups.id` | The group. Removing the group removes its rows (ON DELETE CASCADE) |
| `category_id` | BLOB → `accounts.id`, may be NULL | A category in the group. Removing the category removes the row in the same change, so its undo brings the row back (ON DELETE CASCADE) |
| `member_group_id` | BLOB → `report_groups.id`, may be NULL | A group inside the group, never the group itself. Removing that group removes the row (ON DELETE CASCADE) |
| `position` | INTEGER, from 0 | The order in the group. KassenSturz numbers the members 0, 1, 2, … when the group is edited; a category or group removed since may leave a gap, so a reader goes by the order, not the numbers |

Exactly one of `category_id` and `member_group_id` is set (a CHECK). A category or a group is in a group once: the unique indexes `report_group_members_by_category` (`category_id`, `group_id`) and `report_group_members_by_group` (`member_group_id`, `group_id`); `report_group_members_in_group` (`group_id`, `position`) reads a group's members in order.

### `report_hidden`

The categories the user hid from charts (D-290, D-291), e.g. Rent in a spending pie. A hidden category leaves only the places where it would be shown on its own; it stays in its groups and formulas still count it (Q-225), and the key figures never change.

| Column | Type | Meaning |
|---|---|---|
| `category_id` | BLOB → `accounts.id`, primary key | A hidden category. Removing the category removes the row in the same change (ON DELETE CASCADE). KassenSturz checks when it writes that it is a category, not an account |

### `budgets`

The user's budgets (D-344, D-345, D-639): an amount the user plans to spend on one or more spending categories in each of a budget's periods, optionally only what was paid from some of the user's accounts. A budget is a way of looking at the spending and nothing else: it refers to its categories and accounts by ID (in [`budget_categories`](#budget_categories) and [`budget_accounts`](#budget_accounts)) and is never written onto a transaction, a category or an account. How a budget counts is the reports' business (D-346, D-347): each of its categories with everything inside it, each category once, and with accounts chosen only the transactions with a part on one of them (D-350). Its amounts are in its categories' currency, which is the same for all of them.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `every` | TEXT | `month`, `quarter` or `year` (calendar months, quarters or years; it repeats) or `range` (one stretch of days; it does not repeat) |
| `name` | TEXT, may be NULL | From layout 28 (M6-B203, D-664): the name the user gave the budget, e.g. "Holiday", which the screens and the AI show instead of its categories. 1 to 100 characters (a CHECK); KassenSturz also writes it without spaces around it and refuses a line break or another control character (`store.budget-name`). NULL: no name, and the screens call the budget by its categories, as for every budget of an older layout. Names may repeat |
| `salary_day` | INTEGER, may be NULL | From layout 26 (D-651): for `month` only, the day 2 to 31 each month starts on, a salary month that runs to the day before it in the next month; in a month without that day it starts on the month's last day, as the salary month of the reports does (D-645). NULL: calendar months. A CHECK allows it only for `month` and only from 2 to 31 |
| `first_day` | TEXT | ISO 8601, the first day of its first period. For `month`, `quarter` and `year` the first day of one (for a salary month the day it starts on); KassenSturz checks this when it writes |
| `last_day` | TEXT, may be NULL | ISO 8601, not before `first_day` (a CHECK). NULL: the budget keeps going, never for `range` (a CHECK). For `month`, `quarter` and `year` the last day of one; a `range` touches at most 1,200 calendar months. KassenSturz checks both when it writes |

`budgets_by_first_day` (`first_day`) lists them by start. KassenSturz checks when it writes that two budgets never count the same money on the same day: two budgets whose days overlap share no category, or count from accounts none of which they share (no accounts chosen counts every account; `store.budget-overlap`); and that a book has at most 1,000 budgets (`store.budget-too-many`). The database checks neither.

### `budget_categories`

The spending categories a budget counts (D-639), from layout 25, in sets by day from layout 29 (D-689, D-690): a budget's sets are the distinct `from_day`s of its rows, oldest first, and each counts from its day until the next set's (or the budget's end), so an earlier period keeps counting what it counted. The first set starts on the budget's `first_day`, each later one on the first day of one of its periods and not after its `last_day`, at most 1,200 sets (`store.budget-period`, `store.budget-too-many`). A set has at least one category and at most 100, each once, all in the budget's one currency; KassenSturz checks this when it writes (`store.budget-choice`, `store.budget-too-many`), and that each is a category for money spent (`store.budget-not-spending`). The user changes what a budget counts from the current period on, never earlier (`store.budget-period` `before-current-period`).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7; for a budget made before layout 25, the budget's own ID |
| `budget_id` | BLOB → `budgets.id` | The budget. Removing the budget removes its rows in the same change, so its undo brings them back (ON DELETE CASCADE) |
| `category_id` | BLOB → `accounts.id` | The spending category. The database refuses removing a category that a budget counts (no cascade), and KassenSturz says so first (`store.category-in-use`, `budgets`) |
| `position` | INTEGER, 0 or more | The order the user chose them in, from 0, within its set; the first one names the budget's place in lists, and the first one of the first set its currency |
| `from_day` | TEXT, ISO 8601 date | From layout 29: the first day of the set the row belongs to. The update gave every row its budget's `first_day` |

The unique index `budget_categories_in_set` (`budget_id`, `from_day`, `category_id`) keeps each category once in a set; `budget_categories_by_category` (`category_id`) finds a category's budgets. A category in any set of a budget, an earlier one included, counts as used (`store.category-in-use`, `budgets`).

### `budget_accounts`

The user's own accounts a budget counts from (D-639), from layout 25, per set from layout 29 (D-690): a set's accounts are the rows with its `from_day`. No row for a set: it counts every account. At most 100 a set, each once, and only accounts, not categories; KassenSturz checks this when it writes (`store.budget-choice`, `store.budget-too-many`).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `budget_id` | BLOB → `budgets.id` | The budget (ON DELETE CASCADE, as above) |
| `account_id` | BLOB → `accounts.id` | One of the user's own accounts. Closing the account keeps the row |
| `position` | INTEGER, 0 or more | The order the user chose them in, from 0, within its set |
| `from_day` | TEXT, ISO 8601 date | From layout 29: the first day of the set the row belongs to, one of the budget's `budget_categories` days |

The unique index `budget_accounts_in_set` (`budget_id`, `from_day`, `account_id`) keeps each account once in a set; `budget_accounts_by_account` (`account_id`) finds an account's budgets.

### `budget_amounts`

What each budget plans, dated so a change never rewrites an earlier period (D-345). A row applies from its period on, until the budget's next row.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7, so the change history can name the row |
| `budget_id` | BLOB → `budgets.id` | The budget. Removing the budget removes its rows in the same change, so its undo brings them back (ON DELETE CASCADE) |
| `from_day` | TEXT | ISO 8601, the first day of one of the budget's periods, not before `first_day` and not after `last_day`; for `range` always `first_day`. KassenSturz checks this when it writes |
| `amount` | INTEGER, above 0 | Minor units of the category's currency (D-027). KassenSturz also refuses an amount so large that 1,200 periods of it would not add up in a 64-bit amount (D-360) |
| `carry_over` | INTEGER, 0 or 1 | `1`: what is left at the end of each period from `from_day` on, or what was spent beyond the amount, goes into the next one (Q-266). Always `0` for `range`; KassenSturz checks this when it writes |

The unique index `budget_amounts_by_day` (`budget_id`, `from_day`) keeps one row per period start. Every budget has a row with `from_day` = `first_day`, and at most 1,200 rows; KassenSturz checks both when it writes. A change of the amount from a period on removes the rows of later periods, and stopping a budget removes the rows after its new last day, each in the same change.

### `transactions`

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `booking_date` | TEXT | ISO 8601 date the transaction is shown with: the date the bank booked it, or the user entered. The user can change it; what the bank delivered is kept in [`split_imports`](#split_imports) (D-030, D-083) |
| `payee` | TEXT, may be NULL | Who the money came from or went to, as the user wants it shown |
| `currency` | TEXT → `currencies.code` | The currency the transaction counts in: the `amount` of every part is in it |
| `note` | TEXT, may be NULL | What the user noted about the whole transaction, never empty text (D-132) |
| `entered_on` | TEXT, may be NULL | The ISO 8601 date the transaction was entered into the book, if known, e.g. from a KMyMoney book; KassenSturz itself doesn't set it yet |

### `splits`

The parts of each transaction (shown to users as "Parts", D-017). A transaction's parts add up to exactly zero in its currency; KassenSturz checks this when it writes a transaction and again when it reads one.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `transaction_id` | BLOB → `transactions.id` | The transaction; deleting it deletes its parts |
| `position` | INTEGER, from 0 | The order the parts were entered in; unique per transaction |
| `account_id` | BLOB → `accounts.id`, may be NULL | The account or category the amount is booked to. NULL for a part that has no category yet, e.g. what an imported purchase was for until a Rule or the user picks one (D-017, D-083) |
| `amount` | INTEGER | Minor units in the transaction's currency |
| `foreign_amount` | INTEGER, may be NULL | For a part booked to an account in another currency than the transaction's: its amount in minor units of that currency, e.g. `10856` for 108.56 USD in a transaction in euros (D-043). It has the same sign as `amount`, unless one of them is zero. NULL otherwise |
| `foreign_currency` | TEXT → `currencies.code`, may be NULL | The currency of `foreign_amount`: the account's currency, never the transaction's. Set exactly when `foreign_amount` is |
| `original_amount` | INTEGER, may be NULL | For a part converted out of a withdrawn currency, e.g. from a book another program kept in German marks: the amount it had in that currency, in its minor units, e.g. `-4841` for -48.41 DEM (D-077, D-104). It has the same sign as the amount it was converted into, unless one of them is zero. NULL otherwise |
| `original_currency` | TEXT → `currencies.code`, may be NULL | The withdrawn currency of `original_amount`, never the transaction's or `foreign_currency`. Set exactly when `original_amount` is, which the database checks |
| `own_date` | TEXT, may be NULL | The ISO 8601 date the part's account shows it with, if not the transaction's `booking_date`: on the second side of a joined Transfer, the day that account's bank booked it (D-108). NULL otherwise |
| `own_payee` | TEXT, may be NULL | The payee the part's account shows, if not the transaction's `payee`: on the second side of a joined Transfer, the one that side had (D-108). NULL otherwise |
| `note` | TEXT, may be NULL | What the user noted about this part. What a bank or file delivered for it stays in [`split_imports`](#split_imports) (D-132) |
| `number` | TEXT, may be NULL | A cheque or document number |
| `status` | TEXT, may be NULL | `cleared` if the part was seen on the bank's side, `reconciled` if it was checked against a statement, NULL if neither is known |

A part with a `foreign_amount` always has an `account_id`. Every part is in its account's currency: `foreign_currency` if it has one, otherwise the transaction's currency. KassenSturz checks this, not the database.

A part with an `original_amount` always has a row in [`split_rates`](#split_rates) with the fixed rate it was converted at, from the currency it was converted into (the transaction's, or `foreign_currency`) to `original_currency`. Its `foreign_amount`, if it has one, has no rate of its own. KassenSturz never changes a part's original amount, its rate or the amount converted from it, nor gives an existing part one.

Index: `splits_by_account` on `splits (account_id)`, for an account's register and balance.

### `split_rates`

The exchange rate a part in another currency was converted with, when one amount was calculated from the other with a rate from the [price table](#prices) (D-043). It is a copy: the part keeps it when the rate in the price table is changed or removed. A part whose two amounts were both given, e.g. by the bank or the user, has no row here: their ratio is the rate.

For a part with an `original_amount`, the row is the fixed rate of the changeover instead, e.g. 1 EUR = 1.95583 DEM from 1999-01-01, with the source `changeover` (D-104). That rate is never in the price table.

| Column | Type | Meaning |
|---|---|---|
| `split_id` | BLOB → `splits.id`, primary key | The part; deleting it deletes this row |
| `rate_id` | BLOB (16 bytes) | The ID the rate had in the price table. It may no longer be there |
| `date` | TEXT | The ISO 8601 date the rate applies to |
| `from_currency` | TEXT → `currencies.code` | As in `prices` |
| `to_currency` | TEXT → `currencies.code` | As in `prices`. The two currencies are the part's two currencies, in either order |
| `rate_digits` | INTEGER, 1 to 999999999999999999 | As in `prices` |
| `rate_decimal_places` | INTEGER, 0–18 | As in `prices` |
| `source` | TEXT | `manual` as in `prices`, or `changeover` for the fixed rate a withdrawn currency was replaced at (D-104) |

KassenSturz converts with the newest rate on or before the date it converts for, usually the transaction's, in either direction, multiplying by a rate from the amount's currency and dividing by one the other way; on a day with a rate in each direction, the one from the amount's currency counts. It rounds once, in the book's rounding mode (D-027, D-083).

### `split_imports`

What a bank or file delivered for a part, exactly as received (D-067): the original. Only a part booked to the account the statement is for has a row, so a Transfer matched from two statements keeps what both delivered (D-021). The user can't change it, nor that part's account and amount; what the user changes (the payee, the dates, notes, categories, the part's own payee) is stored in the other tables, on top of it (D-375).

From layout 23 the book itself keeps it so, with these triggers, whatever program writes to it (D-375):

| Trigger | Refuses |
|---|---|
| `original_never_changes` | Any `UPDATE` of a row of `split_imports` |
| `original_kept` | Deleting a row of `split_imports` while [`originals_unlocked`](#originals_unlocked) is empty |
| `original_part_kept` | Deleting a part that has a row here while `originals_unlocked` is empty; deleting its transaction deletes its parts, so this covers that too |
| `original_part_unchanged` | Changing such a part's `account_id`, its amount in the account (`coalesce(foreign_amount, amount)`) or its `foreign_currency` |
| `original_currency_kept` | Changing the `currency` of a transaction with such a part that has no `foreign_amount`, while `originals_unlocked` is empty |

Each refuses with an SQLite error whose message starts with `KassenSturz keeps what a bank or file delivered`, and nothing is changed; KassenSturz reports it as `store.delivered-kept`. The amount of such a part in the transaction's currency (`amount` next to a `foreign_amount`) may still change, e.g. for a new rate.

| Column | Type | Meaning |
|---|---|---|
| `split_id` | BLOB → `splits.id`, primary key | The part; deleting it deletes this row. Its `account_id` is never NULL |
| `bank_id` | TEXT, may be NULL | The bank's own ID for this entry of the statement, used to recognize it when it's delivered again (D-030) |
| `booking_date` | TEXT, may be NULL | The ISO 8601 date the bank booked it |
| `value_date` | TEXT, may be NULL | The ISO 8601 date the money counts from or until, for interest (German: Wertstellung, D-030) |
| `other_party_name` | TEXT, may be NULL | Who paid, or who was paid |
| `other_party_iban` | TEXT, may be NULL | Their IBAN, or whatever account number the bank gave. Not checked |
| `remittance_information` | TEXT, may be NULL | The text the payment came with (German: Verwendungszweck) |

### `prices`

The book's price table: exchange rates, each for one day (D-028). In v1 they are entered by hand. Security prices will join this table when securities come, after 1.0 (D-029).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `date` | TEXT | ISO 8601 date the rate applies to |
| `from_currency` | TEXT → `currencies.code` | The currency one unit of which is priced |
| `to_currency` | TEXT → `currencies.code` | The currency the price is in; never the same as `from_currency` |
| `rate_digits` | INTEGER, 1 to 999999999999999999 | The rate's digits: `10856` for 1.0856. Rates are always above zero |
| `rate_decimal_places` | INTEGER, 0–18 | The rate's decimal places: `4` for 1.0856 |
| `source` | TEXT | Where the rate came from: `manual` (entered by hand) is the only source so far |

One unit of `from_currency` costs `rate_digits` × 10^−`rate_decimal_places` units of `to_currency`. There is at most one rate per `from_currency`, `to_currency` and `date` (a unique index); the other direction is a separate rate, and converting that way divides by the rate instead.

### `rules`

The user's Rules for categorizing transactions, like an email client's filters (D-014, D-031, D-088). They form a tree: a sub-Rule is checked only when its parent matched.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `parent_id` | BLOB → `rules.id`, may be NULL | The Rule it is a sub-Rule of; NULL for a top-level Rule. Never the Rule itself. Deleting a Rule deletes its sub-Rules |
| `position` | INTEGER, from 0 | Its place among its siblings: the top-level Rules, or the parent's sub-Rules. The first one that matches decides (D-031). KassenSturz keeps them numbered 0, 1, 2, … |
| `name` | TEXT | The name the user gave it. Never empty, and never the same as a sibling's, ignoring capitals and spaces around it |
| `match_mode` | TEXT | `all` if every condition must hold, `any` if one is enough |
| `category_id` | BLOB → `accounts.id`, may be NULL | The category it books to: an `income` or `expense` account |
| `transfer_account_id` | BLOB → `accounts.id`, may be NULL | The account it books to instead, which makes the transaction a Transfer: an `asset` or `liability` account. At most one of the two is set; with neither, the Rule leaves the category alone, e.g. to its sub-Rules |
| `payee` | TEXT, may be NULL | The payee it sets. Never empty |

Index: `rules_by_parent` on `rules (parent_id)`.

The tree has no loops and every parent exists; KassenSturz checks this when it writes and again when it reads the Rules.

### `rule_conditions`

The conditions of each Rule, at least one per Rule. Which other columns are set depends on `field` and `operator`; all others are NULL.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `rule_id` | BLOB → `rules.id` | The Rule; deleting it deletes its conditions |
| `position` | INTEGER, from 0 | The order the user entered them in; unique per Rule |
| `field` | TEXT | What is tested: `other_party_name`, `other_party_iban`, `remittance_information` (what the bank delivered for the part booked to the user's account, see [`split_imports`](#split_imports)), `payee` (the transaction's), `amount`, `booking_date`, `currency` (the transaction's) or `account` (the user's account the money moves in or out of) |
| `operator` | TEXT | For the four text fields: `contains`, `equals`, `starts_with`, `ends_with` or `matches` (a regular expression in the syntax of Rust's `regex` crate, D-015). For `amount`: `equals`, `between`, `money_in` or `money_out`. For `booking_date`: `between`. For `currency` and `account`: `is` |
| `text_value` | TEXT, may be NULL | For a text field: the text or pattern. Never empty |
| `case_sensitive` | INTEGER, 0 or 1, may be NULL | For a text field: `1` if capitals count, `0` if not (the default) |
| `amount_min` | INTEGER, may be NULL | For `amount` `equals` or `between`: the lowest amount that matches, in minor units of `currency`. As everywhere, positive for money coming into the account, negative for money going out |
| `amount_max` | INTEGER, may be NULL | The highest amount that matches, never below `amount_min`. Equal to it for `equals` |
| `currency` | TEXT → `currencies.code`, may be NULL | For `amount` `equals` or `between`: the amounts' currency. For `currency`: the currency tested |
| `date_first` | TEXT, may be NULL | For `booking_date`: the first ISO 8601 date that matches |
| `date_last` | TEXT, may be NULL | For `booking_date`: the last one, never before `date_first` |
| `account_id` | BLOB → `accounts.id`, may be NULL | For `account`: an `asset` or `liability` account |

A text field that a transaction doesn't have, such as the purpose of one entered by hand, fails every condition on it. IBANs are compared without their spaces.

### `attachments`

Links to the documents that belong to a transaction, e.g. a receipt (D-132): the book keeps the link only, never the file, so it stays small and the file stays where the user put it. For the receipts a later companion app may capture (D-018); nothing writes them yet but the complete export.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7, so the change history can name the row |
| `transaction_id` | BLOB → `transactions.id` | The transaction; deleting it deletes this row |
| `position` | INTEGER, from 0 | The order of the transaction's attachments; unique per transaction |
| `link` | TEXT | A path on this computer or a URL, as the user or another program gave it; never empty |
| `title` | TEXT, may be NULL | What the user calls it |

### `payees`

The people and shops money goes to or comes from, with what the user or an imported file knows about them beyond the name (D-132). A transaction names its payee by text (`transactions.payee`), not by a row here, so the book works without any.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT | The name, never empty. No two payees share a name, ignoring capitals and spaces around it; KassenSturz checks this when it writes |
| `address` | TEXT, may be NULL | Their address, lines separated by line breaks |
| `email` | TEXT, may be NULL | Their e-mail address |
| `phone` | TEXT, may be NULL | Their phone number |
| `reference` | TEXT, may be NULL | The user's customer or contract number with them |
| `recognition` | TEXT | How an imported transaction is recognized as theirs: `never`, `name` (the delivered name is this name) or `key_words` (the delivered text contains one of the key words). Kept from a KMyMoney book; KassenSturz's Rules do the recognizing (D-088) |
| `key_words` | TEXT, may be NULL | The key words, one per line. Set exactly when `recognition` is `key_words`, which the database checks |
| `recognition_ignores_case` | INTEGER, 0 or 1 | `1` if recognition ignores capitals |

### `tags`

The tags transactions and parts can carry, e.g. "Holiday 2026" across several categories (D-132, D-712). A book has at most 1,000; KassenSturz checks this when it writes (`store.tag-too-many`, `book`).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT | The name, never empty. No two tags share a name, ignoring capitals and spaces around it; KassenSturz checks this when it writes. From layout 30 KassenSturz writes it without spaces around it, on one line and at most 60 characters long (`store.tag-name`) |

### `split_tags`

Which tags each part carries (D-132).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7, so the change history can name the row |
| `split_id` | BLOB → `splits.id` | The part; deleting it deletes this row |
| `tag_id` | BLOB → `tags.id` | The tag. A part carries a tag at most once, which the database checks |
| `position` | INTEGER, from 0 | The order of the part's tags |

Index: `split_tags_by_tag` on `split_tags (tag_id)`. A part carries at most 20 tags of its own (`store.tag-too-many`, `part`); it also counts for the tags of its whole transaction ([`transaction_tags`](#transaction_tags)), which are never copied onto it (D-713).

### `transaction_tags`

From layout 30: which tags a whole transaction carries (D-712). Every part of the transaction counts for them too, the transaction's tags first and then the part's own, each once; this is worked out when read, never stored on the parts (D-713). At most 20 per transaction (`store.tag-too-many`, `transaction`).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7, so the change history can name the row |
| `transaction_id` | BLOB → `transactions.id` | The transaction; deleting it deletes this row |
| `tag_id` | BLOB → `tags.id` | The tag. A transaction carries a tag at most once, which the database checks |
| `position` | INTEGER, from 0 | The order of the transaction's tags |

Index: `transaction_tags_by_tag` on `transaction_tags (tag_id)`.

### `recurring_moves`

From layout 31: the monthly moves between the user's own accounts that the user asked to be joined by themselves (D-749, `docs/RECURRING_MOVES.md`). A move only recognises the two sides statements bring and joins them into one Transfer; it creates no transaction.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `from_account_id` | BLOB → `accounts.id` | The account the money leaves, one of the user's own; deleting it deletes this row |
| `to_account_id` | BLOB → `accounts.id` | The account the money arrives at, another of the user's own; deleting it deletes this row |
| `amount` | INTEGER, more than 0 | The exact amount that leaves, in minor units of `currency` |
| `currency` | TEXT → `currencies.code` | The currency of the amount and of both accounts; KassenSturz checks this when it writes |
| `day_of_month` | INTEGER, 1 to 31 | The day of the month the paying side was booked on when the user remembered the move. In a shorter month it counts as the last day, on a Saturday or Sunday as the Monday after |
| `from_party_iban` | TEXT, may be NULL | The other party's IBAN or account number the bank named on the paying side, as delivered (D-067) |
| `from_party_name` | TEXT, may be NULL | The other party's name the bank named on the paying side, as delivered |
| `created_on` | TEXT | The ISO 8601 date the user remembered it |

The two accounts differ, which the database checks. Indexes: `recurring_moves_once`, unique, on the accounts, the amount, the currency and both party columns with NULL counted as an empty text, so the same move is remembered once; `recurring_moves_by_to_account` on `recurring_moves (to_account_id)`.

### `schedules`

From layout 33: the planned payments, money the user expects to leave or arrive again and again, such as the rent, a subscription or the salary (D-769, `docs/SCHEDULES.md`). A plan never moves money: imports match the entries a bank delivers to its payments, and it creates an entry only when the user books a payment by hand.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT, 1 to 60 characters | What the user calls it; KassenSturz writes it without spaces around it |
| `account_id` | BLOB → `accounts.id` | The user's own account the money leaves or arrives in; deleting it deletes this row. While the account is closed, the plan is expected nowhere |
| `amount` | INTEGER, not 0 | The expected amount in minor units of `currency`: negative when money leaves, positive when it arrives. Its size is at most 43,506,471,871,956 minor units: the limit for sums in [Reading a book](#reading-a-book) shared by 200 plans, each due every week for the 5 years a forecast looks at (B-269) |
| `currency` | TEXT → `currencies.code` | The account's currency; KassenSturz checks this when it writes |
| `tolerance` | INTEGER, 0 or more and less than the amount's size | How far an entry's amount may be from `amount`, in minor units, and still be a payment of the plan |
| `category_id` | BLOB → `accounts.id`, may be NULL | The category in the same currency that a matched entry gets where its part has none yet; a category a plan uses can't be removed (`store.category-in-use`, `schedules`) |
| `party_iban` | TEXT, 1 to 140 characters, may be NULL | The other party's IBAN or account number an entry must name, spaces and capitals ignored |
| `party_name` | TEXT, 1 to 140 characters, may be NULL | Without an IBAN, the other party's name an entry must name, spaces and capitals ignored; with neither, any party fits |
| `unit` | TEXT | `week`, `month` or `year` |
| `every` | INTEGER, 1 to 52 | How many units lie between two payments |
| `anchor` | TEXT | The ISO 8601 date of the first payment. Every payment is counted from it, in a shorter month on its last day; one counted on a Saturday or Sunday is due on the Monday after |
| `track_from` | TEXT, not before `anchor` | Payments counted before this date are not expected |
| `ends_on` | TEXT, may be NULL, not before `anchor` | The last day a payment may be counted on; NULL: no end |
| `paused` | INTEGER, 0 or 1 | 1: not expected, matched or counted for now |

Indexes: `schedules_by_account` on `schedules (account_id)`, `schedules_by_category` on `schedules (category_id)`.

### `schedule_occurrences`

From layout 33: what was decided about a payment of a plan (D-769). A payment without a row is open: upcoming, due or overdue, worked out from the dates on the day it is read.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `schedule_id` | BLOB → `schedules.id` | The plan; deleting it deletes this row |
| `due_on` | TEXT | The ISO 8601 date the payment is counted on, before a weekend moves it |
| `status` | TEXT | `matched` (an entry is this payment) or `skipped` (the user said it won't come) |
| `transaction_id` | BLOB → `transactions.id`, may be NULL | The matched entry, set exactly when `status` is `matched`; deleting the entry deletes this row, so the payment is open again |

Indexes: `schedule_occurrences_once`, unique, on `(schedule_id, due_on)`, so each payment is decided once; `schedule_occurrences_by_transaction`, unique, on `transaction_id`, so an entry is the payment of one plan at most.

### `split_rules`

Marks the parts a Rule booked (D-088). A run of the Rules books a transaction that has two parts, one booked to one of the user's accounts and the other without a category yet. A Rule books a transaction once: a part with a row here is booked again only when the user changes the Rule the row names, or one of that Rule's parents, and asks for what it booked to follow (Q-044). When the user changes or joins such a transaction, KassenSturz writes its parts again without this row, so the Rules leave it alone (D-031).

| Column | Type | Meaning |
|---|---|---|
| `split_id` | BLOB → `splits.id`, primary key | The part the Rule booked, or left without a category while it set the payee; deleting the part deletes this row |
| `rule_id` | BLOB → `rules.id`, may be NULL | The Rule that decided: the deepest sub-Rule that matched. Set to NULL when that Rule is removed |

Index: `split_rules_by_rule` on `split_rules (rule_id)`.

### `history_settings`

How long the book keeps its [change history](#the-change-history): exactly one row (D-066). Like the settings, it is stored in the book, so every computer keeps the same history.

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER, primary key | Always `1` |
| `keep_days` | INTEGER, 0 to 4294967295, may be NULL | For how many days changes are kept: `30` for a new book and for books from earlier layouts. NULL keeps them forever. `0` keeps no history in the book: KassenSturz keeps it in memory while the book is open |

### `originals_unlocked`

The guard of the triggers on [`split_imports`](#split_imports) (D-375). Empty in every saved book. KassenSturz writes its one row, and deletes it again in the same database transaction, only around removing whole transactions the user chose to remove (after a warning that the original goes too), around undo and redo, and around editing a transaction or joining a Transfer, which write a transaction's parts again and are checked afterwards to have given every original back unchanged, on the same part. The history doesn't record it. A program writing the row itself can delete originals: the triggers stop mistakes, not someone set on it.

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER, primary key | Always `1` |

### `unchecked_ai_rules`

From layout 32 (B-249): the Rules the AI made or changed, after the user confirmed the change set, that the user hasn't opened since. The Rules page says "Made by the AI, not checked by you yet" for each until the user opens it once in the editor, which deletes its row. Deleting the Rule deletes its row too. It is a mark for the user's eyes, not the book's data, so the history doesn't record it: undoing the change set takes the Rule away and with it the mark, and redoing it brings the Rule back without one.

| Column | Type | Meaning |
|---|---|---|
| `rule_id` | BLOB, primary key | The [Rule](#rules) |

### `changes`

The change history (D-032, D-066): one row per user action, e.g. adding an account or changing a transaction.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `sequence` | INTEGER, from 1, unique | The order the changes were made in. Undo and redo follow it, not `made_at`, since a computer's clock can be wrong |
| `action` | TEXT | What the user did: `set_rounding_mode`, `use_currency`, `add_account`, `rename_account`, `close_account`, `reopen_account`, `add_category`, `rename_category`, `move_category`, `add_rate`, `remove_rate`, `add_transaction`, `edit_transaction`, `remove_transaction`, `join_transfer` (joining the two sides of a Transfer into one transaction, D-087), `add_rule`, `edit_rule`, `move_rule`, `remove_rule`, `run_rules` (running the Rules on the book's transactions, D-088), `import_statement` (importing bank statements into one account, with the Rules run on what it added, D-092), `correct_starting_balance` (correcting an account's starting balance so it holds what the bank said on a day, and noting that balance on the account, D-117), `add_institution`, `add_payee` or `add_tag` (adding a bank, a payee or a tag, D-132), or `link_bank_account`, `unlink_bank_account` (linking an account to the bank account it is filled from, or removing that link) or `fetch_statement` (fetching an account's statement from its bank, importing it, and noting the day on the link, D-148), `apply_ai_changes` (applying changes an AI prepared, after the user confirmed them, as one change, D-198), `remove_category` (removing a category that nothing uses, D-253), `add_view`, `edit_view`, `rename_view`, `move_view` and `remove_view` (saving, changing, renaming, moving and deleting a saved view or dashboard, D-278), or `add_group`, `edit_group`, `move_group` and `remove_group` (making a category group, changing its name or members, moving it in the list and removing it, D-290) and `set_hidden_categories` (changing which categories charts hide, D-291), `add_budget`, `edit_budget` (a budget's amount from one period on, its carry over or its last day, D-345, from layout 28 its name, D-664, and from layout 29 what it counts from one period on, D-690), `remove_budget`, and from layout 27 `change_account_type` (giving an account another type, and with it another kind when it becomes or stops being a credit card, D-625), and from layout 30 `rename_tag`, `remove_tag` (taking a tag off every transaction and part and out of the book) and `merge_tags` (giving everything with one tag another one instead, D-712), and from layout 31 `add_recurring_move` and `remove_recurring_move` (remembering a monthly move to be joined by itself, and stopping that), `join_recurring_moves` (joining, after an import, the pairs that fit remembered moves, as an action of its own so its Undo leaves the imported entries) and `join_transfers` (joining several suggested Transfers at once, D-749), and from layout 33 `add_schedule`, `edit_schedule`, `remove_schedule` and `pause_schedule` (planning a payment, changing it, removing it, and pausing it or letting it go on), `match_schedules` (matching, after an import, the entries that fit planned payments, as an action of its own so its Undo leaves the imported entries), `match_schedule_occurrence`, `skip_schedule_occurrence` and `unskip_schedule_occurrence` (the user saying which entry a payment is, that it won't come, or that it will after all) and `book_schedule_occurrence` (booking a payment by hand as a new entry, D-769) |
| `made_at` | TEXT | When, ISO 8601 in UTC, in whole seconds, e.g. `2026-10-19T11:04:02Z`, by the clock of the computer it was made on |
| `undone` | INTEGER, 0 or 1 | `1` if it is undone, so redo can make it again |
| `file_name` | TEXT, may be NULL | The name of the file the change read, without its folder, e.g. `october.xml` for an import of that statement file, so undo can say which import it takes back (D-109). NULL for changes that read no file |

### `changed_settings`, `changed_currencies` and the other `changed_` tables

For each table that holds the book's data (`settings`, `currencies`, `institutions`, `accounts`, `payees`, `tags`, `transactions`, `splits`, `split_tags`, `transaction_tags` (from layout 30), `attachments`, `split_rates`, `split_imports`, `prices`, `rules`, `rule_conditions` and `split_rules`, `bank_links`, `views`, `report_groups`, `report_group_members`, `report_hidden`, `budgets`, `budget_categories`, `budget_accounts` and `budget_amounts`, `recurring_moves` (from layout 31), and `schedules` and `schedule_occurrences` (from layout 33)), a table named `changed_` plus its name, e.g. `changed_accounts`, holds copies of the rows each change touched.

| Column | Type | Meaning |
|---|---|---|
| `change_id` | BLOB → `changes.id` | The change; deleting it deletes these rows |
| `side` | TEXT | `before`: the row's values before the change; `after`: its values after it |
| *every column of the table, in its order* | its type, may be NULL | The row's values. The column that identifies a row (`id`, `code` or `split_id`) is never NULL |

Primary key: `change_id`, the column that identifies the row, and `side`.

A row the change added has only an `after` copy, a row it removed only a `before` copy. A row it changed has both, and they differ: a row changed and then changed back within the same action isn't recorded. The copies have no checks of their own; the table's checks apply when they are written back.

## Reading a book

An account's balance is the sum of its parts' amounts in its currency, `coalesce(foreign_amount, amount)` for its `account_id`. Its register joins `splits` with `transactions` and shows each part with `coalesce(own_date, booking_date)` and `coalesce(own_payee, payee)`, ordered by that date, then the transaction's `id` (D-108). Add amounts in 64-bit integers or wider, never as floating point. KassenSturz keeps every sum in 64 bits: it refuses a change, an import or a book whose parts in one currency, taken without their signs (`amount`, `foreign_amount` and `original_amount` each in its own currency), would add up to more than a quarter of the largest 64-bit integer, i.e. 2,305,843,009,213,693,951 minor units (B-269). No account, category or report total can then leave 64 bits. Parts with a NULL `account_id` count in no account's balance; they are the amounts that have no category yet. The rate from one currency into another on a day is the row with the latest `date` on or before that day.

## The change history

Every change KassenSturz makes to a book's data is a user action, recorded as one row of `changes` in the same transaction (D-066, D-086). KassenSturz records it itself, with temporary triggers on its own connection to the book, which aren't stored in the book, and refuses any change outside a user action, so none escapes the history. Changes made by another program aren't recorded.

- **Undo** writes the `before` copies of the newest change that isn't undone back into their tables: the rows it touched get their old values back, rows it added are removed and rows it removed come back. **Redo** writes the `after` copies of the oldest undone change. Changes are undone newest first and redone oldest first, so the book is always as it was at some moment, also after a restart or on another computer. A new change deletes the undone ones: they can't be redone anymore.
- Before writing back, KassenSturz checks that the book holds exactly what the other copies say, and afterwards that every reference and every check of the book still holds. If not, e.g. because another program changed the book, nothing is changed and the change can't be undone or redone (`store.corrupt.history-mismatch`); cleaning up the history removes it.
- **How long it is kept:** `history_settings.keep_days`. When the book is opened, when a change is made and when the setting changes, the changes made longer ago are deleted, but only those the book was changed after on at least that many other days (D-780, B-274), so a clock set years ahead or a book left alone for months never loses its last days of use; they go from the oldest change up to the newest one that is that old, so the history has no gaps even if a clock was wrong. With `0`, the book holds no history: KassenSturz keeps it in memory, where it is lost when the book is closed. **Clean up history** deletes all of it (D-066), the names of the files the changes read included.
- A migration that changes one of the data tables changes its `changed_` table the same way and converts the copies kept there, so changes made before an update can still be undone (D-066).
- Undo and redo can take away or bring back what a bank or file delivered, since the history kept it: they write back with [`originals_unlocked`](#originals_unlocked) open (D-375). How long that is possible is what the removal warning tells the user.

## Migrations

Every book is built by the same migrations, oldest first: a new book runs all of them on an empty database, and a book with an older layout runs the ones it lacks (D-045). The first migration builds layout 24, the starting layout of the first Beta, in which the 23 development layouts of the Alpha were merged (D-536); KassenSturz's own tests prove that it builds, table for table, what those layouts built together, both in a book that started with layout 1 and in one that started with layout 16, the earlier merge of layouts 1 to 15 (D-135). Layout 25 (US1.10, D-640) lets a budget count several categories and only some accounts: it moves each budget's category into [`budget_categories`](#budget_categories) (the row takes the budget's own ID) and adds an empty [`budget_accounts`](#budget_accounts), the budgets' change history included, so Undo and Redo of a change made before the update still work. Layout 26 (D-651) lets a monthly budget start on a salary day: it adds the column `salary_day` to `budgets` and to its change history, empty for every budget the book has, so they stay calendar months. Layout 27 (D-625) lets the user change an account's type: it rebuilds `changes` with the action `change_account_type` added to its list and keeps every row, so Undo and Redo of a change made before the update still work. Layout 28 (M6-B203, D-664) lets a budget have a name: it adds the column `name` to `budgets` and to its change history, empty for every budget the book has, so each is still called by its categories until the user names it. Layout 29 (US3.2, D-690) lets what a budget counts change from a period on: it rebuilds `budget_categories` and `budget_accounts` with the column `from_day`, which every row gets as its budget's `first_day`, so every budget has one set and counts what it counted; their change-history tables get the column too, filled with the budget's first day (from the change's own copy of the budget for one removed since), so Undo and Redo of a change made before the update still work. Layout 30 (US4.8, D-712) lets a tag sit on a whole transaction: it adds [`transaction_tags`](#transaction_tags) and its change history `changed_transaction_tags`, and rebuilds `changes` with the actions `rename_tag`, `remove_tag` and `merge_tags` added to its list, keeping every row; nothing a book of layout 29 holds changes. Layout 31 (US6, #784, D-749) lets the user have monthly moves between their own accounts joined by themselves: it adds [`recurring_moves`](#recurring_moves) and its change history `changed_recurring_moves`, and rebuilds `changes` with the actions `add_recurring_move`, `remove_recurring_move`, `join_recurring_moves` and `join_transfers` added to its list, keeping every row; nothing a book of layout 30 holds changes. Layout 32 (B-249) marks the Rules the AI made or changed until the user opens them: it adds [`unchecked_ai_rules`](#unchecked_ai_rules), empty, so no Rule a book of layout 31 holds starts marked. Layout 33 (US7, #795, D-769) lets the user plan payments that repeat: it adds [`schedules`](#schedules) and [`schedule_occurrences`](#schedule_occurrences), empty, with their change histories `changed_schedules` and `changed_schedule_occurrences`, and rebuilds `changes` with the actions `add_schedule`, `edit_schedule`, `remove_schedule`, `pause_schedule`, `match_schedules`, `match_schedule_occurrence`, `skip_schedule_occurrence`, `unskip_schedule_occurrence` and `book_schedule_occurrence` added to its list, keeping every row; nothing a book of layout 32 holds changes. Each later one gets its own number, runs after a [backup](#backups) of the book (AGENTS.md §3.1 rule 8) and comes with a test that updates a book from every earlier layout since layout 24. The migrations run in one transaction, which also sets the application ID and `user_version`, so an update either finishes or leaves the book exactly as it was. Foreign keys are off while they run, so a migration can rebuild a table without deleting the rows that refer to it; every reference is checked before the transaction is committed. A migration that rebuilds `splits`, `split_imports` or `transactions` makes the triggers on them again. A book with a newer layout is turned away (`store.unsupported-layout`), and so is a book a development build wrote, with layout 1 to 23 (`store.development-layout`): KassenSturz doesn't update it, since the development migrations are gone; the way over is the complete export made with that build, read into a new book. A migration isn't a change in the [change history](#the-change-history): the backup made before it is the way back.

## Backups

Before a migration, KassenSturz copies the whole book into a folder in its own data folder (D-033), with SQLite's `VACUUM INTO`. It also makes one when the book opens and the newest backup there is more than 30 days old or there is none (B-255), so a state a month back can still be found after a harm noticed late, such as a Rule that books wrongly on every import, outlasted the change history. A backup is itself a book, at the old layout, so restoring one means opening it (D-037).

- **Name:** the book's name and the time in UTC, e.g. `Household 2026-10-19 11-04-02 UTC.ksz` for `Household.ksz`. A second backup in the same second is `Household 2026-10-19 11-04-02 UTC 2.ksz`, then `… 3.ksz`, so a backup never replaces another.
- **Complete or not there:** the copy is written as `<name>.part` (until #806 `<name>.partial`), synced to the disk and then renamed. If making it fails, the book isn't updated. A `.part` file left behind by a crash is an incomplete copy and can be deleted.
- **Kept:** the last 10 backups of each book name, plus the oldest backup of each of the 12 newest earlier months that have one (B-255), so with the monthly backup a year of states stays. Making a new one removes the oldest beyond that, never the new one, and leaves every other file alone. Books with the same name in different folders share them (D-080).

**Copies in a folder the user chose (#806).** The user can choose another folder in Settings › Backups, for example on a USB stick or in a folder a sync app uploads. The folder is kept in the app's settings (`[Backups]` in `kassensturzrc`: `CopiesFolder`, `CopiesFolderShownAs`, `CopiesFolderShownIsRealPath`, `CopiesKeep`), not in the book, so it changes no book layout. Copies there have the backups' names and are written the same way, as `<name>.part` first, which sync tools such as Nextcloud leave out, so half a copy never travels. When a book opens, the folder gets the newest backup of that book if it lacks it and it is newer than every copy there; otherwise, if no copy there is from the last 24 hours, a new copy of the book as it is. The folder keeps the last 3, 5, 10 (the default) or 20 copies of each book name, chosen by the user. The folder is never made by KassenSturz: a missing folder (a stick that isn't plugged in), a full disk or a folder that can't be written to is said in Settings, and the book opens as always.

**Restoring.** Settings › Backups › *Restore from backup…* lists the backups of the open book in both folders, newest first, a copy with the same name in both once. Restoring one checks it (KassenSturz's application ID, a layout this KassenSturz opens, `PRAGMA quick_check`), copies it next to the book as `<book>.restore.part`, makes a backup of the book as it is now, closes the book, renames the copy over the book and opens it again. Either the old book or the whole backup is in place, never half of one, and the book as it was is the newest backup.

**Before moving** a book into another folder (D-731), KassenSturz makes one more backup in the same folder, named apart from the ten above: `Household 2026-11-13 03-04-05 UTC vor dem Verschieben.ksz`, then `… vor dem Verschieben 2.ksz` in the same second (builds before 0.261113.31 wrote `… before moving.ksz`; those are still listed and kept the same way). The last 3 of these are kept for each book name; the two kinds never remove each other's. *Restore from backup…* lists them with the others; they are never copied to a folder the user chose and don't count among its copies. While a move runs, its copy sits in the new folder as `<name>.ksz.part` until it is checked and renamed (`docs/BOOK_FOLDER.md` §4); a `.part` file left behind by a crash is removed at the next start.

## The lock file

While a book is open, a lock file sits next to it: the book's file name plus `.lock`, e.g. `Household.ksz.lock` (D-038). Sync tools carry it to the other computers along with the book, so they can tell that the book is open. It is UTF-8 text, one `key=value` per line:

| Key | Value |
|---|---|
| `computer` | The name of the computer the book is open on: one line of at most 255 characters |
| `since` | When it was opened there: ISO 8601 in UTC, in whole seconds, e.g. `2026-10-19T11:04:02Z` |
| `token` | A UUIDv7 that tells this lock apart from any other |

KassenSturz creates the lock file only if there is none, and removes it when it closes the book, but only while it still holds its own token. Any lock file, even an empty or damaged one, means the book is open somewhere. After a warning, the user can open the book anyway: KassenSturz then replaces the lock file, but only if it is still exactly the one the user was warned about (D-080). A program that reads books can use the lock file to tell whether a book is open somewhere.
