# 22. What comes next

*An overview in plain words: what KassenSturz can already do, what is still to come before version 1.0, and what is planned only after that. This is a plan, not a promise: order and content can change when testers find something more important.*

## Where we are

Since 21 November 2026 there is a **stable release** of KassenSturz. It is meant for everyday use with your own book. Next to it there are **beta releases**: they are newer, can have more errors and are there for trying things out (page 2).

KassenSturz as a whole stays in its **Beta** stage until version 1.0 is declared; until then every version number starts with `0.`. We know of no error that loses data. Testers keep trying the app with made-up books, and what they find decides what is built next.

Before the stable release, the code that deals with money, passwords, bank connections and the layout of the files was checked thoroughly once more. Since then the file format of the book no longer changes, except with an automatic backup and a clean carry-over of the old books.

## Already there

- Accounts, categories, transactions, transfers, undo and history
- Reading in bank statements; on Linux also the direct bank connection, for reading only
- Rules that sort transactions by themselves
- Budgets, planned payments (which you can also pause) and statistics with dashboards
- Tags, also as a filter on the dashboards
- A guided setup for new books; it is optional, and every step can be skipped
- Plugins: your own additions for importers, exporters, reports, automations, themes and AI providers. Every plugin shows beforehand what it may do.
- AI help, which is off until you switch it on (including reading, attaching and filing bills from folders you choose, both in the built-in chat and via MCP)

How all of this works is in the other pages of this [handbook](README.md).

## Before version 1.0

- Fixing the errors that testers and users find
- The bank connection for Windows is intended but has no date yet; until then you read in files there
- A signed Mac version, as soon as the signing is paid for; until then there is an experimental download

Only the developer declares version 1.0.

## After version 1.0

This is planned but not started:

- Securities and a portfolio, later cryptocurrencies
- Loans, planned from the payments that really happened
- An importer for GnuCash books
- Backups in your own cloud and optional encryption of the book
- A companion app for Android for taking photos of receipts

## What KassenSturz is not meant to become

No business accounting, no tax return, no investment advice, no service that stores your data in a cloud run by the project, no ads and no sending of payments. Your data stays on your computer.

## Have your say

News and plans are in the [Announcements](https://github.com/Vondor/KassenSturz/discussions/categories/announcements), and wishes and ideas are collected in the [Ideas](https://github.com/Vondor/KassenSturz/discussions/categories/ideas). There you can comment and show with a thumbs-up what matters most to you. Please keep reporting errors as an issue (see [When something goes wrong](19-problems.md)).

Please do not write real account details, names or account numbers there.

## If something looks different

This plan changes more often than the rest of the handbook. The exact state of each step is in the [project's roadmap](../../ROADMAP.md).
