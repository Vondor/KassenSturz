# Error codes

KassenSturz's Rust crates never produce user-facing text (D-034). Every error in `kassensturz-core` and `kassensturz-store` turns into an error report (`kassensturz_core::ErrorReport`, through the `Report` trait):

- a **code**, such as `store.account-closed`;
- named, typed **parameters**, such as the account that is closed;
- for an error caused by another one, that **cause**, e.g. why the backup before an update failed.

The UI keeps one translated message per code and fills in the parameters (D-084).

This list is the reference for that. A test in `kassensturz-store` (`src/error/tests.rs`) checks it against the code, so every code, parameter and keyword below exists, and every error is listed. An error added in a later step gets its row here in the same pull request.

## Rules

- **Codes are stable.** A code keeps its meaning. One that is no longer needed is retired, never given to another error. Before 0.1, codes can still change, like the book's layout (D-045).
- **Codes** are lowercase words joined by hyphens, in parts separated by dots. The first part says where the error comes from: `money`, `decimal`, `currency`, `transaction` (core) or `store`. Damage found in a book starts with `store.corrupt`.
- **Parameters** are listed in the order the report gives them. A name ending in `?` is optional: the report leaves it out when it isn't known.
- **Parameter types** (`ErrorParam`):
  - Amounts (`Money`), currencies, dates and timestamps stay typed. The UI formats them for the user's locale (D-002, D-065) and shows timestamps in the user's time zone.
  - IDs of accounts, transactions, parts and rates stay IDs. The UI shows the account's name, or the transaction's date and payee.
  - A **keyword** is one word from the list in the third column. The UI translates it like a code.
  - **Text** comes from a book or a file, e.g. a damaged date or the name of the computer a book is open on. It is data, may contain anything and is never part of a message. The UI shows it as plain text, never as markup or instructions (AGENTS.md §8).
- **Messages.** The fourth column is a suggested English message in the UI's words (`docs/GLOSSARY.md`, AGENTS.md §6: say what happened and what to do next). It isn't translated from here. The UI writes its own message in its source, where KI18n's tools extract it (D-034). Placeholders in braces are parameters.
- **Details** such as SQLite's error text are for logs only. They are in the error's `Debug` output, never in a report.

## Money and numbers (`kassensturz-core`)

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `money.currency-mismatch` | `expected`, `found` | | Amounts in {expected} and {found} can't be added up without an exchange rate. Enter a rate for the two currencies. |
| `money.overflow` | | | The amount is too large for KassenSturz to work with. Check the amount for extra digits. |
| `money.division-by-zero` | | | An amount can't be split into zero parts. Choose at least one part. |
| `decimal.malformed` | | | This isn't a number KassenSturz can read. Type digits with at most one decimal separator, such as 1.0856. |
| `decimal.too-many-digits` | `max` | | The number has more than {max} digits. Leave out some decimal places. |
| `decimal.too-many-decimal-places` | `max` | | The number has more than {max} decimal places. Leave out some of them. |
| `decimal.not-positive` | | | An exchange rate has to be more than zero. |
| `decimal.negative` | | | A price can't be below zero. |
| `currency.malformed-code` | | | A currency code is three capital letters, such as EUR or USD. |

## Transactions (`kassensturz-core`)

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `transaction.too-few-splits` | `found` | | A transaction needs to say where the money comes from and where it goes. Add an account or a category. |
| `transaction.mixed-currencies` | `expected`, `found` | | The amounts of a transaction have to be in one currency, here {expected}. For the part in {found}, give its amount in {expected} as well. |
| `transaction.duplicate-split-id` | `split` | | Two parts of the transaction are the same. Remove one of them. |
| `transaction.unbalanced` | `total` | | The parts don't add up: {total} is left over. Change an amount, or let KassenSturz put the rest into one part. |
| `transaction.overflow` | | | The amounts of the transaction add up to more than KassenSturz can work with. Check them for extra digits. |
| `transaction.unknown-split` | `split` | | This part isn't in the transaction anymore. Close the transaction and open it again. |
| `transaction.imported-without-account` | `split` | | Details from the bank belong to the part for the account the statement is for. Choose that account for the part. |
| `transaction.foreign-without-account` | `split` | | A part with an amount in another currency needs an account or category in that currency. Choose one, or remove the other amount. |
| `transaction.foreign-in-transaction-currency` | `split` | | The part's second amount is in the transaction's own currency. Remove it, or enter it in the account's currency. |
| `transaction.foreign-sign-mismatch` | `split` | | The part's two amounts point in different directions: one is money in, the other money out. Check the sign of one of them. |
| `transaction.rate-currency-mismatch` | `split` | | The exchange rate used for this part isn't between its two currencies. Choose a rate for these currencies. |

## Books (`kassensturz-store`)

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `store.already-exists` | | | There is already a file with this name. Choose another name or another folder. Nothing was changed. |
| `store.not-found` | | | There is no book at this place. It may have been moved or renamed. Choose it again. |
| `store.not-a-book` | | | This file isn't a KassenSturz book. It wasn't changed. Choose another file, or import it if it comes from another program. |
| `store.unsupported-layout` | `found`, `current` | | This book was made by a newer KassenSturz, or by a test version. Update KassenSturz to open it. |
| `store.locked` | `computer?`, `since?` | | This book is open on {computer} since {since}, or wasn't closed properly there. Close it there first. If that computer crashed, you can open it here anyway. |
| `store.backup-failed` | | | KassenSturz couldn't make a backup before updating this book, so it left the book as it was. The detail below says why. Free up space or check the backup folder, then try again. |
| `store.journal-mode-refused` | `mode` | | The book's file couldn't be set up safely, so KassenSturz didn't open it. Copy the book to a local folder and open it from there. |
| `store.unlisted-currency` | `currency` | | KassenSturz doesn't know the currency {currency}. Choose a current currency. |
| `store.unknown-currency` | `currency` | | The currency {currency} isn't used in this book yet. Add it first. |
| `store.unknown-account` | `account` | | This account or category isn't in the book anymore. It may have just been removed. |
| `store.empty-account-name` | | | Give the account a name. |
| `store.duplicate-account-name` | `existing` | | {existing} already has this name. Choose another one, so you can tell them apart. |
| `store.account-type-mismatch` | `account_type`, `kind` | | {account_type} doesn't fit this kind of account. Choose another type. |
| `store.not-an-account` | `account` | | {account} is a category, not an account. Choose an account. |
| `store.account-closed` | `account` | | {account} is closed. Open it again to add or change its transactions. |
| `store.account-not-empty` | `account`, `balance` | | {account} still has {balance}. Move the money to another account first, then close it. |
| `store.not-a-category` | `account` | | {account} is an account, not a category. Only categories sit inside other categories. |
| `store.parent-not-a-category` | `parent` | | {parent} isn't a category. Choose a category to put this one inside. |
| `store.parent-kind-mismatch` | `parent`, `parent_kind`, `kind` | | Income categories go inside income, spending categories inside spending. Choose another category. |
| `store.parent-currency-mismatch` | `parent`, `parent_currency`, `currency` | | {parent} counts in {parent_currency}, this category in {currency}. Choose a category in the same currency. |
| `store.category-inside-itself` | `account` | | {account} can't go inside itself or one of its own subcategories. Choose another place. |
| `store.unknown-transaction` | `transaction` | | This transaction isn't in the book anymore. It may have just been removed. |
| `store.transaction-id-in-use` | `transaction` | | This transaction is in the book already. |
| `store.split-id-in-use` | `split` | | This part belongs to another transaction already. |
| `store.split-currency-mismatch` | `account`, `account_currency`, `split_currency` | | {account} counts in {account_currency}, but this part's amount is in {split_currency}. Enter the amount in {account_currency} as well. |
| `store.imported-split-changed` | `split` | | This part came from your bank or a file, so its account, amount and details stay as they were delivered. You can change the other parts. |
| `store.unknown-rate` | `rate` | | This exchange rate isn't in the book anymore. It may have just been removed. |
| `store.rate-within-one-currency` | `currency` | | An exchange rate is between two different currencies. Choose another currency than {currency}. |
| `store.no-rate` | `from`, `to`, `date` | | There is no exchange rate from {from} to {to} on or before {date}. Enter one. |
| `store.duplicate-rate` | `existing` | | There is already a rate for these currencies on this day. Change that one instead. |
| `store.date-out-of-range` | `date` | | KassenSturz can't store {date}. Choose a date in the year 1 or later. |
| `store.amount-overflow` | | | The amounts add up to more than KassenSturz can work with. Check them for extra digits. |
| `store.io` | `kind` | `not-found`, `permission-denied`, `already-exists`, `read-only`, `storage-full`, `quota-exceeded`, `file-too-large`, `other` | The file couldn't be read or written. Not found: it was moved or removed. Permission denied or read-only: choose a folder you can write to. Storage full or quota exceeded: free up space. File too large: the drive can't hold files this big. |
| `store.database` | | | Something went wrong reading or writing the book. Try again. If it keeps happening, restore a backup. |

## Damaged books (`kassensturz-store`)

Data in the book breaks one of KassenSturz's rules, for example because another program changed it (D-046). The UI can show one message for all of these: "This book is damaged. Restore a backup." It can add the detail below for support.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `store.corrupt.bad-account-kind` | `found` | | An account has a kind that doesn't exist. |
| `store.corrupt.bad-account-type` | `found` | | An account has a type that doesn't exist. |
| `store.corrupt.account-type-mismatch` | `account` | | {account}'s type doesn't fit its kind. |
| `store.corrupt.bad-category-tree` | `account` | | {account} sits in the wrong place in the category tree. |
| `store.corrupt.bad-currency-code` | `found` | | A currency code isn't three capital letters. |
| `store.corrupt.bad-date` | `found` | | A date isn't a valid calendar date. |
| `store.corrupt.bad-minor-digits` | `found` | | A currency has an impossible number of decimal places. |
| `store.corrupt.bad-rate` | `rate` | | An exchange rate has too many digits or decimal places. |
| `store.corrupt.bad-rate-source` | `found` | | An exchange rate comes from a source that doesn't exist. |
| `store.corrupt.missing-settings` | | | The book's settings are missing. |
| `store.corrupt.bad-rounding-mode` | `found` | | The book's rounding setting doesn't exist. |
| `store.corrupt.currency-mismatch` | `transaction` | | A part of {transaction} is in another currency than its account. |
| `store.corrupt.bad-split` | `split` | | A part of a transaction is incomplete. |
| `store.corrupt.invalid-transaction` | `transaction` | | {transaction} doesn't add up. The detail below says why. |
| `store.corrupt.broken-references` | | | Something in the book refers to an account or a transaction that isn't there. |
| `store.corrupt.bad-action` | `found` | | The change history names a change that doesn't exist. |
| `store.corrupt.bad-timestamp` | `found` | | A time in the change history isn't a valid time. |
| `store.corrupt.bad-history-retention` | `found` | | The book keeps its change history for an impossible number of days. |
| `store.corrupt.history-mismatch` | | | The change history doesn't fit what is in the book, e.g. because another program changed the book, so the change can't be undone or redone. Nothing was changed; Clean up history removes the history that doesn't fit. |
