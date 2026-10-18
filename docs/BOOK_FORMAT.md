# Book format

How a KassenSturz household book is stored, so other programs can read it (D-046). This is documentation, not a promise: the layout can change in any version, and writing into a book from outside KassenSturz isn't supported, because it would bypass KassenSturz's checks. The stable way to move data is the complete export (D-029). Every change to the layout updates this document.

**Status: development layout 1, from the M0 walking skeleton (D-068).** Until 0.1 the layout may change freely; before 0.1 all development changes are merged into one starting layout, and books from development builds aren't opened by 0.1 (D-045). M1 adds the application ID (D-037), backups (D-033), the lock file (D-038), the change history (D-032, D-066) and much more.

## The file

- One SQLite database per book (D-033), with the extension `.ksz` (D-037).
- `PRAGMA user_version` holds the layout version: currently `1`. A file with `0` isn't treated as a book.
- The journal mode is the classic rollback journal (`DELETE`), never WAL, so no lasting side files sit next to the book (D-033).
- Foreign keys are enforced (`PRAGMA foreign_keys = ON` on every connection).

## Conventions

- Every table is `STRICT`: SQLite refuses a value of the wrong type instead of converting it.
- **IDs** are UUIDs, version 7 (D-067), stored as their 16 bytes in a `BLOB`.
- **Amounts** are `INTEGER`s in the currency's minor units, e.g. `5320` for €53.20, never `REAL` (AGENTS.md §3.1 rule 1). Positive amounts flow into an account, negative ones out of it.
- **Dates** are ISO 8601 calendar dates as `TEXT`, e.g. `2026-10-05`, without time or time zone (D-061).
- **Currencies** are ISO 4217 codes as `TEXT`, e.g. `EUR`.

## Tables

### `currencies`

The currencies the book uses. A currency's decimal places are fixed when it is first used, so an update to KassenSturz's currency table can never rescale stored amounts (D-027).

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

## Reading a book

An account's balance is the sum of `splits.amount` for its `account_id`. Its register joins `splits` with `transactions` and orders by `booking_date`, then `id`. Add amounts in 64-bit integers or wider, never as floating point.
