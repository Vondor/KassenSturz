# Complete export format

The complete export holds a whole KassenSturz book in one JSON file (D-029, D-100). It is the stable way to move a book's data: into a new book, to another program, or from a development build to 0.1 (D-045, D-046). Unlike the book file itself (`docs/BOOK_FORMAT.md`), this format is meant to be read and written by other programs, so it changes only with a new [version](#versions).

**Status: version 2** (M2.9b). Version 1 (M2.8) is read as [described below](#versions).

In the command-line tool, `export FILE --format book` writes it and `create --from FILE` makes a new book from it. In code, `BookExport` (ID `book`) in `kassensturz-export` writes it, `read_book` reads it, and `export_book` and `fill_book` in `kassensturz-pipeline` connect it to a book.

## What is in it

Everything the book holds, with its IDs: the settings, the currencies with their decimal places, the accounts and categories, the price table, the Rules, the transactions with all their parts, what a bank or file delivered for a part, the amount a part had in a withdrawn currency before it was converted, and which parts a Rule booked.

Left out is the **change history** (D-032, D-066). It belongs to the book file, not to its data, so a book made from the file starts with an empty history, as a new book does. How long it keeps its history is a setting and does come along.

## The file

- One JSON object (RFC 8259), encoded as UTF-8 without a byte order mark. The extension is `.json`.
- KassenSturz writes the keys in the order this document shows them, two spaces per level, and puts an object or list on one line when nothing inside it is a non-empty object or list. The file ends with a line break. The same book always gives the same file, byte for byte. Other programs don't have to write it like this: any valid JSON with the right keys reads.
- A file is at most 512 MiB (`MAX_FILE_BYTES`); a larger one is refused before it is read.

## Conventions

- **IDs** are UUIDs in their usual form with hyphens, e.g. `019a0000-0000-7000-8000-000000000001`. KassenSturz writes small letters and reads either. Every ID is unique within its kind (accounts, rates, Rules, transactions, parts) across the whole file.
- **Dates** are ISO 8601 calendar dates, exactly `YYYY-MM-DD`, without time or time zone (D-061).
- **Currencies** are ISO 4217 codes, three capital letters, e.g. `EUR`.
- **Amounts** are JSON strings, never JSON numbers, so no program reads them as floating point (AGENTS.md §3.1 rule 1): an optional `-`, digits, and optionally a `.` and at most as many digits as the currency has decimal places in the file's [`currencies`](#currencies). KassenSturz writes exactly that many, e.g. `"-12.50"` for EUR and `"1500"` for JPY, and reads fewer, e.g. `"-12.5"`. A `+`, an exponent, spaces or digit grouping are refused. Positive amounts flow into an account, negative ones out of it.
- **Exchange rates** are JSON strings of digits with an optional `.` and more digits, e.g. `"1.0850"`, kept with all their decimal places (D-051).
- **Words** such as an account's kind are fixed English words from the lists below. They are keys, not text for users, and the same in every language.
- **Text** (names, payees, delivered text) is kept exactly as it is, including line breaks and leading or trailing spaces.
- **Empty values:** a key whose value can be empty is written with `null`. When reading, leaving such a key out means the same as `null`. A key that this document doesn't list is refused, so a mistyped key never loses data silently.

## Versions

`version` is a whole number. KassenSturz reads every version up to its own and refuses a file from a newer one (`export.newer-version`), since it can't know what that file holds. Any change to what a file can hold, even a new key, gets a new version, which this document describes, together with how an older version is read.

| Version | What changed |
|---|---|
| 1 | The first version (M2.8, D-100). |
| 2 | A part can have an `original` amount, from before it was converted out of a withdrawn currency, and a rate's `source` can be `changeover` (M2.9b, D-104). |

A version 1 file is read as version 2 without them: a part with an `original` key or a rate from `changeover` in it is refused (`export.unknown-value`, `export.bad-value`).

## The top level

```json
{
  "format": "kassensturz-book",
  "version": 1,
  "settings": {...},
  "currencies": [...],
  "accounts": [...],
  "rates": [...],
  "rules": [...],
  "transactions": [...]
}
```

| Key | Value |
|---|---|
| `format` | Always `"kassensturz-book"`. A JSON file without it isn't a complete export (`export.not-a-book`). |
| `version` | The [version](#versions) of the format: `2`. |
| `settings` | The book's [settings](#settings). |
| `currencies` | The [currencies](#currencies) the book uses. |
| `accounts` | The [accounts and categories](#accounts-and-categories). |
| `rates` | The [price table](#rates). |
| `rules` | The [Rules](#rules), as a tree. |
| `transactions` | The [transactions](#transactions) with their parts. |

All of them are required.

### Settings

| Key | Value |
|---|---|
| `rounding_mode` | How the book's calculations round (D-027): `half_away_from_zero`, `half_even`, `half_toward_zero`, `toward_zero`, `away_from_zero`, `floor` or `ceiling`. |
| `history_keep_days` | For how many days the book keeps its change history (D-066), a whole number from 0; `null` for forever. |

### Currencies

One object per currency the book uses, sorted by code. Every amount in the file is in one of them.

| Key | Value |
|---|---|
| `code` | The ISO 4217 code. |
| `minor_digits` | How many decimal places the book stores for it (D-027), 0 to 18. The amounts in the file are written with these, and a new book made from the file keeps them, even if KassenSturz's own currency table says otherwise. |

### Accounts and categories

One object per account and category, including the internal account for starting balances. KassenSturz writes the accounts sorted by name, then the internal account, then the categories as a tree, each after the one it sits inside. A file may list them in any order.

| Key | Value |
|---|---|
| `id` | Its ID. |
| `name` | Its name. Names are unique, ignoring capitals: among the accounts, and among the categories inside the same category. |
| `kind` | `asset` or `liability` for an account, `income` or `expense` for a category, `equity` for the internal account for starting balances. |
| `currency` | Its currency. |
| `type` | For an account: `checking`, `savings`, `cash` or `credit_card`. `null` for the others. |
| `closed` | `true` if the user closed it. Only an account whose balance is zero can be closed. |
| `parent` | For a category inside another one: that category's ID. Otherwise `null`. |

### Rates

The price table (D-028): one object per exchange rate, sorted by its two currencies and its date.

| Key | Value |
|---|---|
| `id` | Its ID. |
| `date` | The day it applies to. |
| `from`, `to` | The two currencies: one unit of `from` is worth `rate` units of `to`. |
| `rate` | The exchange rate. |
| `source` | Where it comes from: `manual`, entered by the user (D-028), or, only for the rate of a part's `original` amount, `changeover`, the fixed rate a withdrawn currency was replaced at (D-104). A book's price table holds no `changeover` rate (`store.changeover-rate`). |

### Rules

The auto-categorization Rules (D-014, D-088) as a tree, in the order they are tried.

| Key | Value |
|---|---|
| `id` | Its ID. |
| `name` | Its name. |
| `match` | `all` if every condition must match, `any` if one is enough. |
| `conditions` | Its [conditions](#conditions). |
| `category` | The category it books to, or `null`. |
| `transfer_account` | The account it makes a Transfer to, or `null`. A Rule has at most one of `category` and `transfer_account`. |
| `payee` | The payee it sets, or `null`. |
| `sub_rules` | The Rules inside it, in their order. May be left out when empty. |

Rules sit at most 32 levels deep (`MAX_RULE_DEPTH`); deeper ones are refused when writing and when reading (`export.rules-too-deep`).

#### Conditions

Each condition is an object with `field`, `operator` and the keys that `field` and `operator` need:

| `field` | `operator` | Other keys |
|---|---|---|
| `other_party_name`, `other_party_iban`, `remittance_information` or `payee` | `contains`, `equals`, `starts_with`, `ends_with` or `matches` (a regular expression) | `text`: what to look for; `case_sensitive`: `true` or `false` |
| `amount` | `equals` | `amount` and `currency` |
| `amount` | `between` | `min`, `max` and `currency` |
| `amount` | `money_in` or `money_out` | none |
| `booking_date` | `between` | `first` and `last`, dates |
| `currency` | `is` | `currency` |
| `account` | `is` | `account`: an account's ID |

### Transactions

One object per transaction, sorted by booking date and ID.

| Key | Value |
|---|---|
| `id` | Its ID. |
| `date` | The booking date. |
| `payee` | Who was paid or paid, or `null`. |
| `currency` | The currency the transaction balances in (D-043). |
| `parts` | Its [parts](#parts): their amounts add up to zero. |

#### Parts

| Key | Value |
|---|---|
| `id` | Its ID. |
| `account` | The account or category it books to, or `null` for a part without a category yet. |
| `amount` | Its amount, in the transaction's currency. |
| `foreign` | For a part whose account is in another currency: an object with `amount`, the original amount, `currency`, the account's currency, and `rate`, the rate used, as an object like those in [`rates`](#rates), or `null` if none was recorded. Otherwise `null`. |
| `original` | For a part converted from a withdrawn currency, such as the German mark (D-077, D-104): an object with `amount`, the amount it had, `currency`, the withdrawn currency, which is listed in [`currencies`](#currencies), and `rate`, the fixed rate it was converted with, as an object like those in [`rates`](#rates): one unit of the currency that replaced it, in the part's `currency` or in its `foreign` amount's, costs `rate` units of the withdrawn one. A part with one has no `rate` in its `foreign` amount. Otherwise `null`. Version 2 and later. |
| `delivered` | What a bank or file delivered for it (D-083): an object with `bank_id`, `booking_date`, `value_date`, `other_party_name`, `other_party_iban` and `remittance_information`, each of them text or a date, or `null`. Otherwise `null`. |
| `booked_by_rule` | If a Rule booked it and may book it again when the user changes that Rule (D-088): an object with `rule`, the Rule's ID, or `null` if that Rule was removed since. Otherwise `null`. |

## Reading it back

A file is untrusted (AGENTS.md §8), so it is read strictly, and nothing is read from a file with a mistake. The checks run in this order, and the error names the place in the file, e.g. `transactions[3].parts[0].amount`, counting from 0 (`docs/ERROR_CODES.md`):

1. The size (`export.too-large`), whether it is JSON at all (`export.not-json`, with line and column), whether it is a complete export (`export.not-a-book`), and its version (`export.newer-version`).
2. The format: every key known (`export.unknown-value`), every required one there (`export.missing-value`), every value of the right kind (`export.bad-value`), every ID unique (`export.duplicate`), every amount in a listed currency (`export.unlisted-currency`), and every transaction balanced (`export.invalid-transaction`).
3. The book: the data goes into a new, empty book through the same functions the app uses, so every check they make applies too, e.g. that a category's parent exists, names are unique, and a closed account's balance is zero (the `store.` codes). If anything is refused, the new book stays empty and is removed again.

If a JSON object has the same key twice, the last one counts, as most JSON libraries do.

## Example

A book with one account, one category and one transaction:

```json
{
  "format": "kassensturz-book",
  "version": 2,
  "settings": {"rounding_mode": "half_away_from_zero", "history_keep_days": 30},
  "currencies": [
    {"code": "EUR", "minor_digits": 2}
  ],
  "accounts": [
    {"id": "019a0000-0000-7000-8000-000000000001", "name": "Checking account", "kind": "asset", "currency": "EUR", "type": "checking", "closed": false, "parent": null},
    {"id": "019a0000-0000-7000-8000-000000000002", "name": "Food", "kind": "expense", "currency": "EUR", "type": null, "closed": false, "parent": null}
  ],
  "rates": [],
  "rules": [],
  "transactions": [
    {
      "id": "019a0000-0000-7000-8000-000000000003",
      "date": "2026-10-05",
      "payee": "Bakery",
      "currency": "EUR",
      "parts": [
        {"id": "019a0000-0000-7000-8000-000000000004", "account": "019a0000-0000-7000-8000-000000000001", "amount": "-4.50", "foreign": null, "original": null, "delivered": null, "booked_by_rule": null},
        {"id": "019a0000-0000-7000-8000-000000000005", "account": "019a0000-0000-7000-8000-000000000002", "amount": "4.50", "foreign": null, "original": null, "delivered": null, "booked_by_rule": null}
      ]
    }
  ]
}
```

A longer example with every kind of value is the test file `crates/kassensturz-export/tests/golden/book/sample.json`.
