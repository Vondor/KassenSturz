# 17. Saving a copy and exporting

*Your data belongs to you. This page shows how to take it with you as a spreadsheet or as a whole backup, and how to pass on a dashboard.*

## Saving a copy of my data

**Settings › Your data › Save a copy of my data…** offers two ways:

- **For a spreadsheet:** the transactions of the chosen accounts as a file any spreadsheet program opens, for example LibreOffice Calc, one row per transaction and account.
  Under the accounts you can give a **first** and a **last day**; both days are included, and an empty field means all dates. The lines stay exactly as they are, so they add up to an account's balance only if nothing before the first day is left out. On the command line: `export FILE --from 2026-01-01 --until 2026-12-31`.
- **Save the whole book…:** everything in one file (accounts, categories, transactions, Rules, exchange rates, links to bank accounts), from which KassenSturz can make a new book, for example as a backup or on another computer. The history doesn't come along.

KassenSturz **never overwrites** an existing file here. The file holds **no** bank login names and no PIN.

## Making a book from a file

At the first start (**Bring my data from another program**, page 3) or with **Settings › Start a new book…**.

## KassenSturz's own backups

Before it adapts a book to a new version, KassenSturz makes a backup by itself (**Settings › Backups**) and keeps the last ten. A copy of your own from time to time is still wise.

## Passing on dashboards

**Save this dashboard to a file** writes a dashboard into a small file (`.json`); **Add a dashboard from a file** reads it into another book. The file holds **no money**: no amounts, transactions, balances or account numbers, only the names of the accounts and categories the dashboard points to (page 4).

## Your data never leaves your computer by itself

Nothing is sent anywhere without your clear yes. KassenSturz sends no usage data, error reports or update checks.

**If something looks different:** if KassenSturz says it can't read a file, the message says what to do. A file from a very old development version may no longer be readable.
