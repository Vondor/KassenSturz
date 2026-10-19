# Book format

How a KassenSturz household book is stored, so other programs can read it (D-046). This is documentation, not a promise: the layout can change in any version, and writing into a book from outside KassenSturz isn't supported, because it would bypass KassenSturz's checks. The stable way to move data is the complete export (D-029). Every change to the layout updates this document.

**Status: development layout 5.** Layout 1 was the M0 walking skeleton (D-068); layout 2 added the book's settings (M1.1), layout 3 the price table (M1.2) and layout 4 the type and closed flag of accounts (M1.4) and layout 5 the category tree (M1.5). Books with an earlier layout are updated after a backup (see [Migrations](#migrations)). Until 0.1 the layout may change freely; before 0.1 all development changes are merged into one starting layout, and books from development builds aren't opened by 0.1 (D-045). M1 adds the change history (D-032, D-066) and much more.

## The file

- One SQLite database per book (D-033), with the extension `.ksz` (D-037).
- `PRAGMA application_id` is `0x4B535A42` (1263753794), the letters `KSZB` for KassenSturz book (D-037, D-080). KassenSturz reads it, and SQLite's header, from the file's first bytes before SQLite opens the file. Only a file with both is opened, so another program's SQLite file is never changed, not even its journal mode.
- `PRAGMA user_version` holds the layout version: currently `5`.
- The journal mode is the classic rollback journal (`DELETE`), never WAL, so no lasting side files sit next to the book (D-033). While the book is open, its [lock file](#the-lock-file) sits next to it.
- Foreign keys are enforced (`PRAGMA foreign_keys = ON` on every connection).

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

| Column | Type | Meaning |
|---|---|---|
| `code` | TEXT, primary key | ISO 4217 code: three capital letters |
| `minor_digits` | INTEGER, 0–18 | Decimal places: 2 for EUR, 0 for JPY |

### `accounts`

Accounts and categories (`docs/GLOSSARY.md`).

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `name` | TEXT | The name the user gave it |
| `kind` | TEXT | `asset` or `liability` (shown as accounts), `income` or `expense` (shown as categories), `equity` (internal, for starting balances) |
| `currency` | TEXT → `currencies.code` | The currency of its amounts |
| `account_type` | TEXT, may be NULL | What sort of account it is, if known: `checking`, `savings`, `cash` or `credit_card`. Only accounts have one, never categories. `checking`, `savings` and `cash` need `kind` `asset`, `credit_card` needs `liability`. Books from before layout 4 have none (D-081) |
| `closed` | INTEGER, 0 or 1 | `1` if the user closed the account: its history stays, but it takes no new transactions and is hidden from the lists. Only an account whose balance is zero can be closed |
| `parent_id` | BLOB (16 bytes) → `accounts.id`, may be NULL | For a category: the category it sits inside, e.g. Groceries inside Food. NULL for a top-level category and for every account. Only `income` and `expense` accounts can have one, and never themselves |

Index: `accounts_by_parent` on `accounts (parent_id)`.

**The category tree** (D-082): categories are the accounts of kind `income` or `expense`, and `parent_id` links each to the one it sits inside, to any depth. A category sits inside a category of its own kind and currency, so a category's total can add up the ones inside it. A tree has no loops. Siblings, meaning the categories of one kind inside the same category (or all top-level ones), never share a name, ignoring capitals and spaces around it. KassenSturz checks all of this when it writes and again when it reads the tree; the database itself only refuses a parent that doesn't exist, that is the category itself, or that belongs to an account.

Account names are never empty. No two accounts (kind `asset` or `liability`) share a name, ignoring capitals and spaces around it, closed ones included. KassenSturz checks this when it writes, not the database (D-081).

### `transactions`

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `booking_date` | TEXT | ISO 8601 date the bank booked it, or the user entered (D-030) |
| `payee` | TEXT, may be NULL | Who the money came from or went to, as the user wants it shown |
| `currency` | TEXT → `currencies.code` | The currency all its parts are in |

### `splits`

The parts of each transaction (shown to users as "Parts", D-017). A transaction's parts add up to exactly zero; KassenSturz checks this when it writes a transaction and again when it reads one.

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `transaction_id` | BLOB → `transactions.id` | The transaction; deleting it deletes its parts |
| `position` | INTEGER, from 0 | The order the parts were entered in; unique per transaction |
| `account_id` | BLOB → `accounts.id` | The account or category the amount is booked to |
| `amount` | INTEGER | Minor units in the transaction's currency, which in layout 1 is also the account's currency |

Index: `splits_by_account` on `splits (account_id)`, for an account's register and balance.

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

## Reading a book

An account's balance is the sum of `splits.amount` for its `account_id`. Its register joins `splits` with `transactions` and orders by `booking_date`, then `id`. Add amounts in 64-bit integers or wider, never as floating point. The rate from one currency into another on a day is the row with the latest `date` on or before that day.

## Migrations

Every book is built by the same migrations, oldest first: a new book runs all of them on an empty database, and a book with an older layout runs the ones it lacks (D-045). They run in one transaction, which also sets the application ID and `user_version`, so an update either finishes or leaves the book exactly as it was. Foreign keys are off while they run, so a migration can rebuild a table without deleting the rows that refer to it; every reference is checked before the transaction is committed. A book is updated only after a [backup](#backups) of it was made (AGENTS.md §3.1 rule 8). A book with a newer layout, or with a layout older than the first migration KassenSturz still has, is turned away.

## Backups

Before a migration, KassenSturz copies the whole book into a folder in its own data folder (D-033), with SQLite's `VACUUM INTO`. A backup is itself a book, at the old layout, so restoring one means opening it (D-037).

- **Name:** the book's name and the time in UTC, e.g. `Household 2026-10-19 11-04-02 UTC.ksz` for `Household.ksz`. A second backup in the same second is `Household 2026-10-19 11-04-02 UTC 2.ksz`, then `… 3.ksz`, so a backup never replaces another.
- **Complete or not there:** the copy is written as `<name>.partial`, synced to the disk and then renamed. If making it fails, the book isn't updated. A `.partial` file left behind by a crash is an incomplete copy and can be deleted.
- **Kept:** the last 10 backups of each book name. Making a new one removes the oldest beyond that, never the new one, and leaves every other file alone. Books with the same name in different folders share these 10 (D-080).

## The lock file

While a book is open, a lock file sits next to it: the book's file name plus `.lock`, e.g. `Household.ksz.lock` (D-038). Sync tools carry it to the other computers along with the book, so they can tell that the book is open. It is UTF-8 text, one `key=value` per line:

| Key | Value |
|---|---|
| `computer` | The name of the computer the book is open on: one line of at most 255 characters |
| `since` | When it was opened there: ISO 8601 in UTC, in whole seconds, e.g. `2026-10-19T11:04:02Z` |
| `token` | A UUIDv7 that tells this lock apart from any other |

KassenSturz creates the lock file only if there is none, and removes it when it closes the book, but only while it still holds its own token. Any lock file, even an empty or damaged one, means the book is open somewhere. After a warning, the user can open the book anyway: KassenSturz then replaces the lock file, but only if it is still exactly the one the user was warned about (D-080). A program that reads books can use the lock file to tell whether a book is open somewhere.
