# The docs folder: a map

Start here instead of reading the big files in full (D-960). Each file below has one line saying what it is for and how far to trust it. Read only the files your task needs, and only the part you need.

## How to read this folder

- **The code is the truth.** Where a document and the code disagree, the code shows what the app does; the document is then the bug, so fix it in your pull request.
- **Decisions: search, don't read.** [`DECISIONS.md`](DECISIONS.md) is the ledger of every decision (D-nnn) and open question (Q-nnn), about 1 MB. Look up the numbers your task names, for example `grep -n '^| D-753 ' docs/DECISIONS.md`. A later row can revise an earlier one, so also search for the number in later rows (`grep -n 'D-071' docs/DECISIONS.md`) before you rely on it.
- **Roadmap: read the newest Sprint.** [`ROADMAP.md`](ROADMAP.md) holds the milestone table at the top and one block per Sprint below; the last block is the current work. The older blocks are history.
- **Design records** tell why a feature was built the way it is. Their first lines say whether it was built; the body below can be older than the code.

## The rules and the plan

| File | What it is |
|---|---|
| [`DECISIONS.md`](DECISIONS.md) | The ledger: every decision and open question, with who decided it. Search by number (above). |
| [`ROADMAP.md`](ROADMAP.md) | Milestones, Sprints and their steps with status and pull request. |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | The crates, how they depend on each other, and the main design of each area. |
| [`GLOSSARY.md`](GLOSSARY.md) | The plain words the app uses for accounting terms (AGENTS.md §6). |
| [`ERROR_CODES.md`](ERROR_CODES.md) | Every error code the core returns, with the message shown to the user. Each new error needs a row. |
| [`THIRD_PARTY.md`](THIRD_PARTY.md) | Every dependency with its license, and the license allowlist CI checks (D-026). |
| [`INDEPENDENT_REVIEW.md`](INDEPENDENT_REVIEW.md) | When a pull request needs a review by an agent that did not write it, and the checklist (D-786). |
| [`TRANSLATING.md`](TRANSLATING.md) | How a volunteer checks or starts a translation (AGENTS.md §11). |

## Formats other programs can read

| File | What it is |
|---|---|
| [`BOOK_FORMAT.md`](BOOK_FORMAT.md) | The book's SQLite layout, updated with every migration (D-046). |
| [`EXPORT_FORMAT.md`](EXPORT_FORMAT.md) | The complete export, a format for other programs. |
| [`DASHBOARD_FILE.md`](DASHBOARD_FILE.md) | The file that holds one dashboard, to share it without any money in it. |

## Security

| File | What it is |
|---|---|
| [`THREAT_MODEL.md`](THREAT_MODEL.md) | What we protect, from whom, and what stops each threat (rows T-nn). |
| [`SECURITY_LOG.md`](SECURITY_LOG.md) | How attacks are run and recorded. |
| [`REGRESSION_TESTS.md`](REGRESSION_TESTS.md) | One test per attack finding; a test checks that every name listed here still exists. |
| [`FUZZING.md`](FUZZING.md) | The fuzz targets for the file readers and how to run them. |
| [`SECURITY_OVERVIEW.en.md`](SECURITY_OVERVIEW.en.md), [`.de`](SECURITY_OVERVIEW.de.md) | The plain-language safety page for users and testers. |
| [`PRIVACY.md`](PRIVACY.md), [`.de`](PRIVACY.de.md) | The privacy policy. |

## Building, packaging and releasing

| File | What it is |
|---|---|
| [`RELEASING.md`](RELEASING.md) | How a beta or stable release is tagged and published; only the maintainer tags. |
| [`SNAP.md`](SNAP.md) | The snap: how it is built, its channels and where it keeps the book. |
| [`WINDOWS.md`](WINDOWS.md) | The Windows packages, the Microsoft Store and signing. |
| [`MACOS.md`](MACOS.md), [`.de`](MACOS.de.md) | The experimental, untested Mac download. |
| [`CODE_SIGNING_POLICY.md`](CODE_SIGNING_POLICY.md) | What is signed and who may approve it. |
| [`FINTS_PRODUCT_ID.md`](FINTS_PRODUCT_ID.md), [`.de`](FINTS_PRODUCT_ID.de.md) | How the FinTS registration number is handled, for other open-source projects. |
| [`BANK_LIST.md`](BANK_LIST.md) | How the hand-checked bank list grows and what the app shows when a bank is not fully known. |

## Extending the app

| File | What it is |
|---|---|
| [`PLUGINS.md`](PLUGINS.md) | The plugin API reference: manifest, permissions, extension points. |
| [`WRITING_PLUGINS.md`](WRITING_PLUGINS.md) | A step-by-step guide to a first plugin. |
| [`ADDING_A_WIDGET.md`](ADDING_A_WIDGET.md) | Every place a new dashboard widget type touches. |

## Design records (built; why it works this way)

| File | Feature |
|---|---|
| [`REPORT_DESIGNER.md`](REPORT_DESIGNER.md) | The reporting layer, charts and the dashboard document (M6a). Follow-ups: [`_M6B`](REPORT_DESIGNER_M6B.md) grid, groups and formulas; [`_M6C`](REPORT_DESIGNER_M6C.md) budgets and filters; [`_M6D`](REPORT_DESIGNER_M6D.md) several dashboards and templates; [`_M6E`](REPORT_DESIGNER_M6E.md) custom charts; [`_US3`](REPORT_DESIGNER_US3.md) per-bar figures; [`_US4_ALARMS`](REPORT_DESIGNER_US4_ALARMS.md) the warnings tile; [`_US4_TRANSACTIONS`](REPORT_DESIGNER_US4_TRANSACTIONS.md) the transactions tile. |
| [`STATUS_TILES.md`](STATUS_TILES.md) | Tiles that shrink to a status icon. |
| [`MONEY_FLOW.md`](MONEY_FLOW.md) | The money-flow chart. |
| [`TREND_LINES.md`](TREND_LINES.md), [`DAILY_BALANCE.md`](DAILY_BALANCE.md) | Moving averages, MACD and the daily balance. |
| [`BUDGET_HISTORY.md`](BUDGET_HISTORY.md), [`BUDGET_PAUSE.md`](BUDGET_PAUSE.md) | Changing what a budget counts; pausing and resuming a budget. |
| [`SCHEDULES.md`](SCHEDULES.md) | Planned payments and the forecast. |
| [`RECURRING_MOVES.md`](RECURRING_MOVES.md) | Monthly moves between the user's own accounts. |
| [`TAGS.md`](TAGS.md) | Tags on transactions and parts. |
| [`BOOK_FOLDER.md`](BOOK_FOLDER.md) | Choosing and moving the book's folder, per install type. |
| [`AI_SPLITS.md`](AI_SPLITS.md), [`RECEIPT_INBOX.md`](RECEIPT_INBOX.md) | The AI splits a transaction and files its bill; the receipt inbox. |
| [`SETUP_WIZARD.md`](SETUP_WIZARD.md), [`GUIDED_SETUP.md`](GUIDED_SETUP.md) | The first setup wizard and the optional guided setup; pictures in `setup-wizard/`. |

## For testers and users

| Folder or file | What it is |
|---|---|
| [`INSTALL.md`](INSTALL.md), [`.de`](INSTALL.de.md) | Installing on each system. |
| [`testing/`](testing/README.md) | The tester kit: guides, install sheets, step log, interview guide. German is what testers get (D-553). |
| [`user/`](user/) | The handbook, mirrored to the GitHub Wiki by `tools/wiki-sync.py`. |
| `branding/` | The publisher's logo; not covered by the project's licenses. |

## History (kept for reference, not current)

- `TESTING_GUIDE_M6D.md`, `TESTING_GUIDE_M6E.md`, `TESTING_GUIDE_M7A.md`: click paths from the Alpha Sprints.
- `REPORT_DESIGNER_M6B_DELTA.md`: a study of a grid editor's behaviour; its outcome is D-311 and D-312.
- `proposals/`: the background of two early choices, the date library (D-061) and the number format (D-051).
