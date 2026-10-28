# Error codes

KassenSturz's Rust crates never produce user-facing text (D-034). Every error in `kassensturz-core`, `kassensturz-store`, `kassensturz-import`, `kassensturz-export`, `kassensturz-bank`, `kassensturz-platform` and `kassensturz-ai-tools` turns into an error report (`kassensturz_core::ErrorReport`, through the `Report` trait):

- a **code**, such as `store.account-closed`;
- named, typed **parameters**, such as the account that is closed;
- for an error caused by another one, that **cause**, e.g. why the backup before an update failed.

The UI keeps one translated message per code and fills in the parameters (D-084).

The app's own checks of what the user typed, e.g. an amount in the wrong regional format, aren't errors of core or store: their codes start with `input.`, they live in `kassensturz-ui-bridge` and aren't listed here (D-089). The same goes for the app's own problems opening a book, whose codes start with `book.` (D-113).

This list is the reference for that. A test in `kassensturz-store` (`src/error/tests.rs`) checks it against the code of core and store, and one in `kassensturz-import` (`src/error/tests.rs`) against the `import.` codes, one in `kassensturz-export` (`src/error/tests.rs`) against the `export.` codes, and one in `kassensturz-bank` (`src/error/tests.rs`) against the `bank.` codes, and one in `kassensturz-platform` (`src/error/tests.rs`) against the `platform.` codes, and one in `kassensturz-ai` (`src/error/tests.rs`) against the `ai.` codes, and one in `kassensturz-ai-tools` (`src/error.rs`) against the `ai-tools.` codes, and one in `kassensturz-ai-chat` (`src/error.rs`) against the `ai-chat.` codes, all with the same reader of this list (`kassensturz-store/src/error/list.rs`), so every code, parameter and keyword below exists, and every error is listed. An error added in a later step gets its row here in the same pull request.

## Rules

- **Codes are stable.** A code keeps its meaning. One that is no longer needed is retired, never given to another error. Before 0.1, codes can still change, like the book's layout (D-045).
- **Codes** are lowercase words joined by hyphens, in parts separated by dots. The first part says where the error comes from: `money`, `decimal`, `currency`, `transaction`, `transfer`, `rule` (core), `store`, `import`, `export`, `bank`, `platform`, `ai` or `ai-tools`. Damage found in a book starts with `store.corrupt`.
- **Parameters** are listed in the order the report gives them. A name ending in `?` is optional: the report leaves it out when it isn't known.
- **Parameter types** (`ErrorParam`):
  - Amounts (`Money`), currencies, dates and timestamps stay typed. The UI formats them for the user's locale (D-002, D-065) and shows timestamps in the user's time zone.
  - IDs of accounts, transactions, parts, rates and Rules stay IDs. The UI shows the account's or Rule's name, or the transaction's date and payee.
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
| `transaction.original-in-part-currency` | `split` | | The amount this part had before it was converted is in one of the part's own currencies, not in the withdrawn currency it was converted from. The file it came from is damaged; import it again from the original program. |
| `transaction.original-sign-mismatch` | `split` | | The amount this part had before it was converted points in the other direction than its amount now: one is money in, the other money out. The file it came from is damaged; import it again from the original program. |
| `transaction.original-with-foreign-rate` | `split` | | This part was converted from a withdrawn currency and also has an exchange rate for its amount in another currency, but a part keeps only one rate. The file it came from is damaged; import it again from the original program. |

## Transfers (`kassensturz-core`)

Joining the two sides of a Transfer between the user's own accounts into one transaction (D-087). `store.transfer` isn't a code of its own: the store reports these codes as they are.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `transfer.not-a-side` | `transaction` | | {transaction} can't be one side of a Transfer: it needs one part for one of your accounts and one part without a category yet. Choose a transaction from your bank that has no category. |
| `transfer.same-transaction` | `transaction` | | A Transfer needs two transactions: money out of one account and into another. Choose the other side. |
| `transfer.same-account` | `account` | | Both transactions are in {account}. A Transfer moves money between two of your accounts. Choose the other account's transaction. |
| `transfer.amounts-differ` | `first`, `second` | | One side says {first}, the other {second}. The money taken out of one account has to be what arrived in the other. Choose the matching transaction. |

A joined transaction that wouldn't add up is reported with its `transaction` code.

## Auto-categorization Rules (`kassensturz-core`)

A Rule that can't be used as it is (D-014, D-088). `store.rule` isn't a code of its own: the store reports these codes as they are. `condition` counts the Rule's conditions from 1, as the user sees them.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `rule.no-conditions` | `rule` | | {rule} has no conditions, so it would apply to every transaction. Add at least one condition. |
| `rule.empty-text` | `rule`, `condition` | | Condition {condition} of {rule} has no text to look for. Type some, or remove the condition. |
| `rule.bad-pattern` | `rule`, `condition` | | The pattern in condition {condition} of {rule} isn't one KassenSturz can use. Check its brackets and special characters, or look for plain text instead. |
| `rule.amount-range-reversed` | `rule`, `condition` | | In condition {condition} of {rule}, the lowest amount is above the highest. Swap them. For money going out, both are negative, such as −50.00 to −10.00. |
| `rule.amount-range-currencies` | `rule`, `condition` | | The two amounts in condition {condition} of {rule} are in different currencies. Use one currency for both. |
| `rule.date-range-reversed` | `rule`, `condition` | | In condition {condition} of {rule}, the last day is before the first. Swap them. |
| `rule.empty-payee` | `rule` | | {rule} would set an empty payee. Type the payee, or leave that action out. |

## Books (`kassensturz-store`)

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `store.already-exists` | `file` | | There is already a file called {file}. Choose another name or another folder. Nothing was changed. |
| `store.not-found` | | | There is no book at this place. It may have been moved or renamed. Choose it again. |
| `store.book-not-empty` | | | A complete export can only be read into a new, empty book, and this one has something in it already. It wasn't changed. Choose a new book to read the export into. |
| `store.not-a-book` | | | This file isn't a KassenSturz book. It wasn't changed. Choose another file, or import it if it comes from another program. |
| `store.unsupported-layout` | `found`, `current` | | This book was made by a newer KassenSturz. Update KassenSturz to open it. It wasn't changed. |
| `store.older-layout` | `found`, `current` | | This book has the layout of an older KassenSturz. Open it in KassenSturz once, which updates it after a backup; then it can be read here. It wasn't changed. |
| `store.development-layout` | `found`, `current` | | This book was made by a test version of KassenSturz from before 0.1 (D-045). Open it with that version, choose "Save the whole book" there, and start a new book from the saved file. It wasn't changed. |
| `store.locked` | `computer?`, `since?` | | This book is open on {computer} since {since}, or wasn't closed properly there. Close it there first. If that computer crashed, you can open it here anyway. |
| `store.backup-failed` | | | KassenSturz couldn't make a backup before updating this book, so it left the book as it was. The detail below says why. Free up space or check the backup folder, then try again. |
| `store.journal-mode-refused` | `mode` | | The book's file couldn't be set up safely, so KassenSturz didn't open it. Copy the book to a local folder and open it from there. |
| `store.unlisted-currency` | `currency` | | KassenSturz doesn't know the currency {currency}. Choose a current currency. |
| `store.withdrawn-currency` | `currency` | | {currency} isn't used any more: another currency replaced it at a fixed rate. Choose the currency that replaced it. |
| `store.unknown-currency` | `currency` | | The currency {currency} isn't used in this book yet. Add it first. |
| `store.unknown-account` | `account` | | This account or category isn't in the book anymore. It may have just been removed. |
| `store.empty-account-name` | | | Give the account a name. |
| `store.incomplete-bank-link` | `account` | | To link {account} to your bank, KassenSturz needs the bank account's IBAN, or its bank code with its account number. |
| `store.bank-account-already-linked` | `existing` | | {existing} is already linked to this bank account. Remove that link first, or choose another bank account. |
| `store.not-linked` | `account` | | {account} isn't linked to a bank account yet. Link it first, then fetch. |
| `store.duplicate-account-name` | `existing` | | {existing} already has this name. Choose another one, so you can tell them apart. |
| `store.account-type-mismatch` | `account_type`, `kind` | | {account_type} doesn't fit this kind of account. Choose another type. |
| `store.not-an-account` | `account` | | {account} is a category, not an account. Choose an account. |
| `store.stated-balance-currency-mismatch` | `account`, `account_currency`, `currency` | | {account} counts in {account_currency}, the balance you stated is in {currency}. State it in the account's currency. |
| `store.account-closed` | `account` | | {account} is closed. Open it again to add or change its transactions. |
| `store.account-not-empty` | `account`, `balance` | | {account} still has {balance}. Move the money to another account first, then close it. |
| `store.category-in-use` | `category`, `used_by` | `subcategories`, `transactions`, `rules` | {category} can't be deleted because {used_by} use it. A category can only be deleted when nothing uses it. |
| `store.not-a-category` | `account` | | {account} is an account, not a category. Only categories sit inside other categories. |
| `store.parent-not-a-category` | `parent` | | {parent} isn't a category. Choose a category to put this one inside. |
| `store.parent-not-an-account` | `parent` | | {parent} isn't an account. An account can only be grouped under another account. |
| `store.unknown-institution` | `institution` | | This bank isn't in the book anymore. It may have just been removed. |
| `store.unknown-tag` | `tag` | | This tag isn't in the book anymore. It may have just been removed. |
| `store.empty-name` | | | Give it a name. |
| `store.duplicate-payee-name` | `existing` | | {existing} already has this name. Choose another one, so you can tell them apart. |
| `store.duplicate-tag-name` | `existing` | | {existing} already has this name. Choose another one, so you can tell them apart. |
| `store.parent-kind-mismatch` | `parent`, `parent_kind`, `kind` | | Income categories go inside income, spending categories inside spending. Choose another category. |
| `store.parent-currency-mismatch` | `parent`, `parent_currency`, `currency` | | {parent} counts in {parent_currency}, this category in {currency}. Choose a category in the same currency. |
| `store.category-inside-itself` | `account` | | {account} can't go inside itself or one of its own subcategories. Choose another place. |
| `store.unknown-transaction` | `transaction` | | This transaction isn't in the book anymore. It may have just been removed. |
| `store.transaction-id-in-use` | `transaction` | | This transaction is in the book already. |
| `store.split-id-in-use` | `split` | | This part belongs to another transaction already. |
| `store.split-currency-mismatch` | `account`, `account_currency`, `split_currency` | | {account} counts in {account_currency}, but this part's amount is in {split_currency}. Enter the amount in {account_currency} as well. |
| `store.imported-split-changed` | `split` | | This part came from your bank or a file, so its account, amount and details stay as they were delivered. You can change the other parts. |
| `store.original-changed` | `split` | | This part was converted from a currency that isn't used any more, so its amount stays as it was converted, with the amount it had before. You can choose another account or category in the same currency, or change the other parts. |
| `store.unknown-rule` | `rule` | | This Rule isn't in the book anymore. It may have just been removed. |
| `store.rule-id-in-use` | `rule` | | This Rule is in the book already. |
| `store.empty-rule-name` | | | Give the Rule a name. |
| `store.duplicate-rule-name` | `existing` | | {existing} already has this name here. Choose another one, so you can tell them apart. |
| `store.rule-inside-itself` | `rule` | | {rule} can't go inside itself or one of its own sub-Rules. Choose another place. |
| `store.rule-needs-category` | `account` | | {account} isn't a category. Choose a category, or let the Rule make a Transfer to move the money into one of your accounts. |
| `store.transfer-suggested` | `transaction` | | {transaction} looks like one side of a Transfer between your accounts, so the Rules left it alone. Decide on the Transfer first, then run the Rules again. |
| `store.unknown-rate` | `rate` | | This exchange rate isn't in the book anymore. It may have just been removed. |
| `store.rate-within-one-currency` | `currency` | | An exchange rate is between two different currencies. Choose another currency than {currency}. |
| `store.changeover-rate` | | | This is the fixed rate a currency was replaced at. It stays with the amounts converted with it and isn't added to the exchange rates. |
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
| `store.corrupt.half-stated-balance` | `account` | | {account} has a stated balance without its day, or a day without the balance. |
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
| `store.corrupt.bad-part-status` | `found` | | A part of a transaction has a status that doesn't exist. |
| `store.corrupt.bad-payee` | `payee` | | {payee}'s details contradict each other. |
| `store.corrupt.invalid-transaction` | `transaction` | | {transaction} doesn't add up. The detail below says why. |
| `store.corrupt.broken-references` | | | Something in the book refers to an account or a transaction that isn't there. |
| `store.corrupt.bad-action` | `found` | | The change history names a change that doesn't exist. |
| `store.corrupt.bad-timestamp` | `found` | | A time in the change history isn't a valid time. |
| `store.corrupt.bad-history-retention` | `found` | | The book keeps its change history for an impossible number of days. |
| `store.corrupt.bad-rule` | `rule` | | {rule} or one of its conditions has values that don't fit together. |
| `store.corrupt.bad-rule-tree` | `rule` | | {rule} sits in the wrong place in the list of Rules. |
| `store.corrupt.history-mismatch` | | | The change history doesn't fit what is in the book, e.g. because another program changed the book, so the change can't be undone or redone. Nothing was changed; Clean up history removes the history that doesn't fit. |

## Importing files (`kassensturz-import`)

A file from a bank or another program couldn't be read (D-091), or what it holds doesn't fit the account it was to go into (`kassensturz-pipeline`, which adds no codes of its own and passes on those of the store, D-092). Nothing was imported, so the book is unchanged. Where a code has `line?`, it is the line of the file the problem is on, counted from 1, if the importer knows it; the UI adds "on line {line}" to the message. Text in `found` is what the file says, shown as data (AGENTS.md §8).

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `import.io` | `kind` | `not-found`, `permission-denied`, `is-a-directory`, `other` | The file couldn't be read. Not found: it was moved or removed. Permission denied: choose a file you may read. Is a directory: choose a file inside the folder. Nothing was imported. |
| `import.file-too-large` | `max_bytes` | | The file is larger than {max_bytes}, too large to be a statement or a book KassenSturz can read safely. Check that it's the right file. Nothing was imported. |
| `import.too-deeply-nested` | `max`, `line?` | | The file nests more than {max} levels deep, which no bank file does. It may be damaged. Download it again. Nothing was imported. |
| `import.too-many-entries` | `max` | | The file has more than {max} entries. Download a shorter period and import each part. Nothing was imported. |
| `import.text-too-long` | `max_bytes`, `line?` | | A text in the file is longer than {max_bytes}, which no bank file has. It may be damaged. Download it again. Nothing was imported. |
| `import.malformed` | `line?` | | The file isn't in the format chosen, or it is damaged. Check the format, or download the file again. Nothing was imported. |
| `import.bad-amount` | `found`, `line?` | | {found} should be an amount, but it isn't one KassenSturz can read. Check the format, or download the file again. Nothing was imported. |
| `import.bad-date` | `found`, `line?` | | {found} should be a date, but it isn't one KassenSturz can read in the order of day, month and year chosen for the file. Check that order, or download the file again. Nothing was imported. |
| `import.wrong-encoding` | `encoding`, `line?` | `utf-8`, `windows-1252`, `iso-8859-15` | The file has a character that doesn't exist in {encoding}, so it is written in another text encoding. Choose the one your bank names for the file; for German banks it is often Windows-1252. Nothing was imported. |
| `import.column-not-found` | `column`, `line?` | | The file has no column called {column}. Your bank may have changed its file: check which column holds what. Nothing was imported. |
| `import.column-not-unique` | `column`, `line?` | | The file has more than one column called {column}, so KassenSturz can't tell which one is meant. Choose the column by its number. Nothing was imported. |
| `import.bad-settings` | `line?` | | The settings for reading the file are incomplete or contradict each other, e.g. no column for the amount. Check which column holds what; {line} is the line of the settings file. Nothing was imported. |
| `import.too-many-decimal-places` | `found`, `currency`, `line?` | | The amount {found} has more decimal places than {currency} has. KassenSturz never rounds what a bank delivered, so nothing was imported. Check the file's currency. |
| `import.amount-too-large` | `found`, `line?` | | The amount {found} is too large for KassenSturz to work with. The file may be damaged. Nothing was imported. |
| `import.unknown-currency` | `currency`, `line?` | | The file uses the currency {currency}, which KassenSturz doesn't know. Nothing was imported. |
| `import.currency-mismatch` | `expected`, `found` | | The file has an amount in {found} in a statement for an account in {expected}, so its entries can't be added up. Nothing was imported. |
| `import.account-currency-differs` | `account`, `account_currency`, `statement_currency` | | The statement is in {statement_currency}, but {account} counts in {account_currency}. Choose an account in {statement_currency}. Nothing was imported. |
| `import.replaced-currency-differs` | `account`, `account_currency`, `statement_currency`, `successor` | | The statement is in {statement_currency}, which {successor} replaced, but {account} counts in {account_currency}. Choose an account in {successor}: the amounts are converted into it. Nothing was imported. |
| `import.successor-needed` | `currency` | | The file uses {currency}, a currency KassenSturz doesn't know, most likely one that another currency replaced long ago. Say which current currency replaced it, at what rate and from which day, and import the file again. Nothing was imported. |
| `import.successor-not-current` | `currency`, `successor` | | {successor} isn't a currency in use today, so {currency} can't be converted into it. Give the current currency that replaced it in the end, with the rate into that one. Nothing was imported. |
| `import.decimal-places-differ` | `currency`, `book`, `file` | | The file counts {currency} with {file} decimal places, your book with {book}, so its amounts can't be read safely. KassenSturz never moves the decimal point of what a bank delivered, so nothing was imported. |

## Exporting files (`kassensturz-export`)

Data couldn't be written to a file (D-099), or a complete export couldn't be read back (D-100, `docs/EXPORT_FORMAT.md`). The `path` of a reading error names the place in the file, e.g. `transactions[3].parts[0].amount`, counting from 0. `kassensturz-pipeline` gathers the data and fills a new book with what was read, and adds no codes of its own; its store errors keep their `store.` codes.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `export.io` | `kind` | `not-found`, `permission-denied`, `is-a-directory`, `disk-full`, `other` | The file couldn't be written. Not found: the folder doesn't exist, so choose another. Permission denied: choose a folder you may write to. Is a directory: type a file name after the folder. Disk full: free some space or choose another drive. No file was left behind. |
| `export.already-exists` | `file` | | The file {file} exists already. KassenSturz never overwrites a file, so choose another name, or move the old file away first. Nothing was exported. |
| `export.bad-settings` | | | The settings for the file contradict each other, e.g. a comma as both the separator and the decimal mark. Choose a semicolon or a tab as the separator, or a point as the decimal mark. Nothing was exported. |
| `export.currency-mismatch` | `expected`, `found` | | An amount in {found} was to be written for an account in {expected}, so the file would add up wrongly. This is a mistake in KassenSturz; please report it. Nothing was exported. |
| `export.bad-decimal-places` | `currency`, `digits` | | {currency} has {digits} decimal places in this book, more than an amount can have, so it can't be written exactly. This is a mistake in KassenSturz or in the book; please report it. Nothing was exported. |
| `export.wrong-kind` | | | The chosen format can't hold this data, e.g. a whole book as CSV. This is a mistake in KassenSturz; please report it. Nothing was exported. |
| `export.unlisted-currency` | `currency`, `path?` | | An amount is in {currency}, but the book's list of currencies doesn't have it, so its decimal places aren't known. In a complete export being read, {path} says where. Correct the file, or report it if KassenSturz wrote it. Nothing was exported or read. |
| `export.mark-without-part` | `split` | | A part is marked as booked by a Rule, but no transaction has it. This is a mistake in KassenSturz; please report it. Nothing was exported. |
| `export.unlisted-tag` | `split`, `tag` | | A part carries a tag the book doesn't list. This is a mistake in KassenSturz: nothing was exported. |
| `export.rules-too-deep` | `max` | | Rules sit inside each other more than {max} levels deep, more than a complete export can hold. Move some of them up a level and try again. Nothing was exported or read. |
| `export.too-large` | `max` | | The file is larger than a complete export can be ({max} bytes), so it wasn't read. Check that it is the right file. |
| `export.not-json` | `line`, `column` | | This file isn't a complete export: it isn't JSON, or it is cut off, at line {line}, column {column}. Choose the file KassenSturz exported, or export the book again. Nothing was read. |
| `export.not-a-book` | | | This file isn't a complete export of a KassenSturz book. Choose a file that KassenSturz exported with "Export the whole book". Nothing was read. |
| `export.newer-version` | `version` | | This complete export was written by a newer KassenSturz (format version {version}). Update KassenSturz to read it. Nothing was read. |
| `export.missing-value` | `path` | | The complete export lacks {path}, which every export has. The file may have been changed by hand or by another program. Correct it, or export the book again. Nothing was read. |
| `export.unknown-value` | `path` | | The complete export has {path}, which KassenSturz doesn't know, so reading it would leave something out. Remove it, or update KassenSturz if a newer one wrote the file. Nothing was read. |
| `export.bad-value` | `path`, `expected` | `object`, `list`, `text`, `number`, `yes-or-no`, `null`, `id`, `date`, `amount`, `currency`, `rate`, `keyword` | In the complete export, {path} isn't what it should be: an object, a list, text, a whole number in its range, true or false, null (because another value rules it out), an ID, a date like 2026-10-05, an amount like -12.50 with at most the currency's decimal places, a currency code like EUR, an exchange rate like 1.0856, or one of the words the format lists for it (`docs/EXPORT_FORMAT.md`). Correct it, or export the book again. Nothing was read. |
| `export.duplicate` | `path` | | In the complete export, {path} has the same ID as an entry before it, or names a currency twice. Correct it, or export the book again. Nothing was read. |
| `export.invalid-transaction` | `path` | | In the complete export, the transaction at {path} doesn't add up or contradicts itself; the detail below says how. Correct it, or export the book again. Nothing was read. |

## Bank connections (`kassensturz-bank`)

A bank connection couldn't be used, or what the bank delivered couldn't be turned into a statement (D-053, D-141, D-145). Nothing was imported, so the book is unchanged; the fetch can be tried again. No report ever carries a PIN, a TAN or a login name: logins are named by their ID (threat T-02 in `docs/THREAT_MODEL.md`). A bank's own message (`text`) is data the user may read, never an instruction (AGENTS.md §8).

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `bank.not-installed` | | | Bank connections need AqBanking, a free program KassenSturz uses to talk to banks, and it isn't installed on this computer. Install it from your distribution (the Flatpak brings it along), or import a file from your bank instead. |
| `bank.too-old` | `library`, `found`, `minimum` | `aqbanking`, `gwenhywfar` | The installed {library} is version {found}; KassenSturz needs {minimum} or newer. Update it, or import a file from your bank instead. |
| `bank.not-registered` | | | This build of KassenSturz has no registration with the banks' FinTS system yet, so it can't connect to a bank. Import a file from your bank instead. |
| `bank.malformed-iban` | | | This isn't an IBAN. It has two letters, two digits and then up to 30 letters or digits, e.g. DE89 3704 0044 0532 0130 00; spaces don't matter. Check it against what your bank shows. |
| `bank.unknown-login` | `login` | | This bank login no longer exists in KassenSturz. Set it up again. |
| `bank.unknown-account` | `account` | | Your bank doesn't list the bank account this account is linked to for this login anymore. Link the account to another bank account, or connect the bank again in Settings. |
| `bank.no-bank-account` | | | None of your bank logins lists the bank account this account is linked to. Connect the bank again in Settings, or link the account to another bank account. |
| `bank.pin-needed` | | | Your bank asks for your PIN. Type it to continue. |
| `bank.pin-rejected` | | | Your bank says the PIN is wrong. KassenSturz has forgotten the PIN it had, so it won't be sent again. Check it and type it again; after a few wrong PINs a bank blocks the login. |
| `bank.unreachable` | | | Your bank's server couldn't be reached. Check your internet connection and the bank's address, and try again later. |
| `bank.refused` | `text?` | | Your bank refused the request. It says: {text}. Try again later; if it keeps happening, ask your bank. |
| `bank.aborted` | | | The bank's question was closed before it was answered, so nothing was done. Start again when you are ready. |
| `bank.tan-needed` | | | Your bank asks for a TAN, and KassenSturz had no way to ask you for it. Nothing was done. Try again. |
| `bank.tan-method-unsupported` | | | Your bank shows this TAN as a picture (a flicker code, photoTAN or QR code), which KassenSturz can't show yet. Choose a TAN method at your bank that sends the TAN as text or asks you to approve in its app, then try again. |
| `bank.untrusted-server` | | | The bank server's security certificate didn't pass every check, so KassenSturz sent nothing to it. Check your internet connection and the server address; if it keeps happening, ask your bank. |
| `bank.needs-setup-dialog` | | | A chip-card login can't be set up in KassenSturz yet. Use a login with PIN and TAN for this bank, or import a file from your bank instead. |
| `bank.setup-failed` | `text?` | | The bank login couldn't be set up, so nothing of it was kept. The bank software says: {text}. Check the login name and the server address and try again. |
| `bank.folder` | | | The folder KassenSturz keeps its bank settings in couldn't be created or opened. Check that your user folder can be written to. |
| `bank.busy` | | | Another bank connection is still running. Wait for it to finish and try again. |
| `bank.keepassxc` | | | KeePassXC couldn't be reached, or it said no. Start KeePassXC, open and unlock your database, make sure Browser Integration is on, allow KassenSturz when KeePassXC asks, and try again. Until then, type the PIN each time (D-164, D-167: also when choosing KeePassXC in Settings, which then changes nothing). |
| `bank.keychain` | | | The system keychain couldn't be reached, so a saved PIN couldn't be saved, read or removed. Make sure the keychain (for example KWallet) is unlocked, or choose to type the PIN each time. |
| `bank.malformed-amount` | `amount` | | Your bank sent an amount KassenSturz can't read: {amount}. Nothing was imported. If it keeps happening, report it with the amount as shown. |
| `bank.inexact-amount` | `amount` | | Your bank sent an amount with a fraction of the smallest unit ({amount}), which KassenSturz never rounds. Nothing was imported. If it keeps happening, report it with the amount as shown. |
| `bank.amount-too-large` | `amount` | | Your bank sent an amount too large for KassenSturz to work with: {amount}. Nothing was imported. |
| `bank.missing-currency` | | | Your bank didn't say which currency the amounts are in. Nothing was imported. Check the account's currency with your bank. |
| `bank.currency-mismatch` | `expected`, `found` | | Your bank sent amounts in {found} for an account in {expected}. Nothing was imported. Check the account's currency with your bank. |
| `bank.missing-date` | | | Your bank sent a transaction without a date. Nothing was imported. If it keeps happening, report it. |
| `bank.statement` | | | The fetched transactions couldn't be used: see the cause. Nothing was imported. |

## The system keychain (`kassensturz-platform`)

The system keychain couldn't save, read or remove a secret, such as a banking PIN the user chose to save (D-057, D-147). No report carries the secret, the key or the keychain's own text (threat T-02). The front end, or `kassensturz-bank` on its behalf (`bank.keychain`), tells the user to unlock the keychain or to type the PIN each time instead.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `platform.keychain-unavailable` | | | The system keychain (KWallet or GNOME Keyring) couldn't be reached. Make sure it is running and unlocked, or choose to type the PIN each time instead. |
| `platform.keychain-failed` | | | The system keychain answered with an error, so the PIN wasn't saved, read or removed. Try again; if it keeps happening, look at the keychain in its own manager (e.g. KWalletManager), or choose to type the PIN each time instead. |
| `platform.keepassxc-unavailable` | | | KeePassXC couldn't be used to save, read or remove the PIN: it isn't running, its browser integration is off, the database stayed locked, or KassenSturz wasn't allowed in KeePassXC. Start KeePassXC, open and unlock your database, turn on Tools → Settings → Browser Integration, and try again; or type the PIN this time (D-164). |
| `platform.keychain-not-text` | | | What the keychain holds for this login isn't a PIN that KassenSturz saved. Remove the entry in the keychain's manager and save the PIN again, or type it each time. |
| `platform.invalid-secret-key` | | | KassenSturz asked the keychain with a name it can't use. This is a bug in KassenSturz: please report it. |

## AI providers (`kassensturz-ai`)

A request to the AI provider the user set up couldn't be sent, or its answer couldn't be used (D-035, D-175, D-182, D-183). Nothing in the book changed. No report carries the provider's key, a question, an answer or the provider's own text (threats T-25, T-31); the only parameter is an HTTP status.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `ai.off` | | | AI features are switched off, so nothing was sent. Turn them on in Settings → AI features. |
| `ai.no-provider` | | | No AI provider is set up yet: its address or the model is missing. Enter both in Settings → AI features. |
| `ai.bad-address` | | | This address can't be used for an AI provider. It starts with `http://` or `https://`, has the server's name, and has no login name, password, `?` or `#` part, e.g. `http://localhost:11434/v1`. |
| `ai.plain-http` | | | This address is on another computer and starts with `http://`, which isn't encrypted, so KassenSturz won't send anything to it. Use the provider's `https://` address; a server at home needs `https://` too. Only a provider on this computer may use `http://`. |
| `ai.no-consent` | | | The AI provider is on another computer, and you haven't agreed yet to what KassenSturz sends to it, or the provider or the list of what is sent changed since. Nothing was sent. Read what is sent and agree, or use a provider on this computer. |
| `ai.settings-changed` | | | The AI provider's settings changed after this request was prepared, so it wasn't sent. Look at the request again and send it. |
| `ai.request-too-large` | | | This request is too large to send at once. Ask about less at a time. |
| `ai.key-needed` | | | The AI provider needs a key. Type it, or save it in Settings → AI features. |
| `ai.key-rejected` | | | The AI provider refused the key. Check the key in Settings → AI features, and whether it is still valid at the provider. |
| `ai.keychain` | | | The system keychain couldn't be reached, so the provider's key couldn't be saved, read or removed. Make sure the keychain (for example KWallet) is unlocked, or type the key each time. |
| `ai.keepassxc` | | | KeePassXC couldn't be reached, or it said no, so the provider's key couldn't be saved, read or removed. Start KeePassXC, open and unlock your database, make sure Browser Integration is on, and try again; or type the key each time. |
| `ai.unreachable` | | | The AI provider couldn't be reached. Check the address in Settings → AI features, that the provider is running (for a local server such as Ollama or LM Studio), and your internet connection. |
| `ai.untrusted-server` | | | The server's security certificate didn't pass the checks of this computer, so KassenSturz sent nothing to it. Check the address; a server at home needs a certificate this computer trusts. |
| `ai.timeout` | | | The AI provider took too long to answer. Try again; a local model may need a faster computer or a smaller model. |
| `ai.redirect` | | | The AI provider sent KassenSturz to another address, which KassenSturz doesn't follow. Enter the provider's final address in Settings → AI features. |
| `ai.not-found` | | | The AI provider doesn't know this address or this model. Check both in Settings → AI features. |
| `ai.missing-v1` | | | The address needs `/v1` at its end: the provider's answer to the address as typed is an error, and with `/v1` it lists its models. KassenSturz offers the address with `/v1`. |
| `ai.rate-limited` | | | The AI provider asks you to wait before the next request. Try again in a little while. |
| `ai.context-too-small` | | | The AI model's context (its working memory) is too small for this question. In the program that runs the model, load it again with a larger context length (8192 tokens or more; 16384 is better), or ask about less at a time. |
| `ai.provider-failed` | `status` | | The AI provider answered with an error (HTTP {status}). Try again later; if it keeps happening, check the provider's settings. |
| `ai.reply-too-large` | | | The AI provider's answer was too large, so KassenSturz didn't read it. Ask about less at a time. |
| `ai.bad-reply` | | | The AI provider's answer isn't what KassenSturz expects from an OpenAI-compatible provider. Check that the address points to such an interface, often ending in `/v1`. |

## The AI's read functions (`kassensturz-ai-tools`)

A model asked for something from the book that the read functions can't give (D-170, D-186). These codes go back to the model as the function's answer and show in the tool-call log (M5.4); the user sees them only there. No report repeats what the model sent, since a call is untrusted text (threat T-21): a bad argument is named by its fixed name only.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `ai-tools.unknown-function` | | | The AI asked for a function KassenSturz doesn't have, so it got nothing. Nothing to do; ask the question again in other words if the answer is missing. |
| `ai-tools.bad-argument` | `argument` | `arguments`, `include_closed`, `account_id`, `date`, `from`, `to`, `text`, `category_id`, `limit`, `unsorted_only`, `sort`, `min_amount`, `max_amount`, `skip_transfers`, `new_categories`, `key`, `name`, `parent_id`, `income`, `currency`, `new_rules`, `match`, `conditions`, `field`, `test`, `case_sensitive`, `money_in`, `money_out`, `new_category`, `categorize`, `transaction_id`, `delete_categories`, `new_accounts`, `type`, `edit_accounts`, `account_id`, `closed`, `edit_categories`, `new_parent`, `to_top`, `edit_rules`, `rule_id`, `move_rules`, `position`, `delete_rules`, `title`, `what_happened`, `expected`, `steps`, `page`, `id` | The AI asked with a value KassenSturz can't use for {argument}, such as a day that doesn't exist or too many rows, so it got nothing. Nothing to do; the AI can ask again. |
| `ai-tools.unknown-account` | | | The AI asked about an account that isn't one of yours in this book. Nothing to do; the AI can ask for the list of accounts first. |
| `ai-tools.unknown-category` | | | The AI asked about a category that isn't in this book. Nothing to do; the AI can ask for the list of categories first. |
| `ai-tools.bad-period` | | | The AI asked about a period that ends before it starts. Nothing to do; the AI can ask again. |
| `ai-tools.unknown-transaction` | | | The AI prepared a change for a transaction that isn't in this book. Nothing was prepared; the AI can search for the transaction first. |
| `ai-tools.too-many-changes` | | | The AI prepared more changes at once than you could review (at most 100). Nothing was prepared; ask for fewer at a time. |
| `ai-tools.not-allowed` | `argument` | `new_categories`, `edit_categories`, `new_rules`, `edit_rules`, `move_rules`, `delete_rules`, `categorize`, `delete_categories`, `new_accounts`, `edit_accounts`, `list_accounts`, `account_balance`, `category_totals`, `search_transactions`, `list_categories`, `list_rules`, `page` | The AI tried to do something you haven't allowed it ({argument}): to read part of your book, to open a page that shows it, or to prepare a kind of change. Nothing was prepared or read. You choose what the AI may do in the AI settings, under "What the AI may do". |
| `ai-tools.category-in-use` | `used_by` | `subcategories`, `transactions`, `rules` | The AI wanted to delete a category that still has {used_by} in it or on it. Only an empty category can go, so nothing was prepared; move or sort what is in it first, or keep the category. |
| `ai-tools.account-not-empty` | | | The AI wanted to close an account that still holds money. Nothing was prepared; the balance has to be zero first. |
| `ai-tools.name-taken` | | | The AI prepared a new category or Rule with a name that is already used there. Nothing was prepared; the AI can pick another name or use the one you have. |
| `ai-tools.not-categorizable` | | | The AI wanted to give a transaction a category, but it is a transfer between your accounts, a starting balance, or split over several categories, which only you can change. Nothing was prepared. |
| `ai-tools.other-currency` | | | The AI wanted to book a transaction to a category in another currency. Nothing was prepared; the AI can pick a category in the transaction's currency. |
| `ai-tools.unknown-rule` | | | The AI prepared a change for a Rule that isn't in this book. Nothing was prepared; the AI can list the Rules first. |
| `ai-tools.not-changeable` | | | The AI wanted to change a Rule or category in a way KassenSturz doesn't do for it: delete a Rule that has sub-Rules, move a sub-Rule, give a Rule that makes a Transfer another category, or rewrite conditions it can't see in full (such as an IBAN or a pattern). Nothing was prepared; you can make that change yourself. |
| `ai-tools.debug-log-off` | | | The AI wanted to read the debug log, but it is off. Nothing to do; turn on the debug log in Settings if you want the AI to look at it. |
| `ai-tools.issue-book-data` | `field` | `title`, `what_happened`, `expected`, `steps` | The AI's draft of a problem report held something from your book, such as a name, an amount or a day, so KassenSturz didn't show it. Nothing to do; the AI can write it again in general words. |
| `ai-tools.issue-secret` | `field` | `title`, `what_happened`, `expected`, `steps` | The AI's draft of a problem report held something that looks like a password or a key, so KassenSturz didn't show it. Nothing to do; the AI can write it again without it. |
| `ai-tools.book-unreadable` | | | The book couldn't be read to answer the AI. The detail says why; see that message. |

## Applying changes the AI prepared (`kassensturz-ai-tools`)

The user confirmed a change set (D-199), and it couldn't be applied. Nothing of it was: the book is exactly as it was before.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `ai-changes.out-of-date` | | | A transaction in the changes was changed or removed after the AI prepared them, so nothing was changed. Ask the AI to prepare them again. |
| `ai-changes.not-allowed` | | | The changes hold a kind of change that you have turned off in the AI settings since the AI prepared them, so nothing was changed. Turn it on again in the AI settings, or ask the AI to prepare the changes again. |
| `ai-changes.not-applied` | | | The book refused one of the changes, e.g. because a category of that name was added in the meantime, so nothing was changed. The detail says why; ask the AI to prepare them again. |

## Your own AI tools' changes (`kassensturz-mcp`, `kassensturz-ai-tools`)

The user's own AI tool asked `kassensturz-mcp` to prepare changes (D-216, D-217). These codes go back to the AI tool as the tool's answer, never into the book; the user sees them only in that AI tool. Nothing was changed.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `mcp-writes.off` | | | Your AI tools may not prepare changes. Turn on "Let my AI tools prepare changes" in KassenSturz's AI settings if you want them to. |
| `mcp-writes.folder` | | | The folder through which changes reach KassenSturz couldn't be made or used, or this system has none yet (only Linux has one so far). Check that your user can write to KassenSturz's data folder. |
| `mcp-writes.app-not-running` | | | KassenSturz didn't take the changes within 10 seconds: it isn't open, has another book open, or AI or "Let my AI tools prepare changes" is off in it. Open the book in KassenSturz and ask your AI tool again. |
| `mcp-writes.unknown-request` | | | There are no prepared changes with that number (any more): they were decided on and already reported, or are older than an hour. Ask your AI tool to prepare them again if you still want them. |
| `mcp-writes.bad-request` | | | KassenSturz couldn't read what arrived in its folder, so it showed nothing. Ask your AI tool again; if it keeps happening, the AI tool or another program writes into the folder by itself. |
| `mcp-writes.dropped` | | | KassenSturz showed the changes but forgot them before you decided: you started a new conversation, opened another book, switched AI off or closed KassenSturz. Ask your AI tool again. |

## The AI chat (`kassensturz-ai-chat`)

A question in the chat ended without an answer (D-190, D-193). Errors of the provider keep their own `ai.` codes. The conversation is as it was before the question, and the tool-call log shows how far it got.

| Code | Parameters | Keywords | What happened, and what the user can do |
|---|---|---|---|
| `ai-chat.cancelled` | | | You stopped the question, so the AI's answer, if one still comes, is not shown. A request already on its way can't be called back. Ask again when you like. |
| `ai-chat.stopped` | | | You didn't let the request go after seeing what would be sent, so nothing was sent. Ask again, and remove what you don't want to send. |
| `ai-chat.too-many-rounds` | | | The AI kept asking KassenSturz for data without answering, so KassenSturz stopped it. Ask a narrower question, or try another model. |
| `ai-chat.tool-call-as-text` | | | The AI model wrote a function call as plain text instead of making the call, so nothing was done. Ask again, or try another model; this one doesn't work well with KassenSturz's functions. |
| `ai-chat.book-unreadable` | | | The book couldn't be read for the overview a model without functions gets. The detail says why; see that message. |
