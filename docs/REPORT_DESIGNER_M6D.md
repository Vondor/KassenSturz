# The report designer, part 4: several dashboards, templates, dashboard files, KMyMoney reports and Update all (M6d)

Design of the M6d Sprint, written in step M6d.0. It builds on `docs/REPORT_DESIGNER.md` (the dashboard document and layout 20, D-278, D-279), `docs/REPORT_DESIGNER_M6B.md` (the grid, the document v1, D-289; the editor, D-315; named layouts, D-330) and `docs/REPORT_DESIGNER_M6C.md` (filters and document version 2, D-350, D-351). Decisions D-435 to D-449 (the maintainer's Phase 0 answers are D-431 to D-433; D-448 and D-449 apply his answers to Q-317 and Q-318), questions Q-317 to Q-320; threat rows T-51 to T-55; the steps are in `docs/ROADMAP.md` ("M6 steps", rows M6d.\*).

Release stage: **Alpha** (D-274). **No new layout:** everything below fits the `views` table of layout 20 and the existing history actions (`add_view`, `edit_view`, `rename_view`, `move_view`, `remove_view`), combined with `Book::as_one_action` where one user action takes several calls. The dashboard document stays version 2; its one new key is additive (section 3). A second additive key marks the default dashboard (D-448, section 1.3); nothing is added to the book's settings or to this computer's config.

**The maintainer's answers this design applies:** D-431 (Q-313 = R: the KMyMoney sample reports come from a throw-away book with Space Colony data, as `.kmy` and SQLite; without them M6d.6 moves to the start of M6e), D-432 (Q-314 = R: a dashboard file names categories, groups and accounts, never IDs; what does not match is listed and left out), D-433 (Q-315 = R: the Update all accounts widget is M6d.7, a human gate the maintainer merges).

**What stays true (D-342):** the Overview is still the start screen and the app is complete without anything in this design. A person who never makes a second dashboard sees no new button, tab or menu on the page.

## 1. Several dashboards in `views` (D-436; M6d.1)

### 1.1 What the list is

- The **dashboards** are the book's documents of kind `dashboard`, in the book's order (`position` 0, 1, 2, …). The Overview page shows one of them at a time.
- A book with **no** stored dashboard has exactly one: the built-in Overview (`Document::default_overview`, D-314), not stored, as today. It is the first and only entry of the list.
- A dashboard's identity is its ID, no longer its name. The rule of D-330 that the dashboard named *Overview* is the one the page shows, and every other one is a *saved layout*, ends here: every stored dashboard is a dashboard of the list. A development book from M6c whose saved layouts are stored keeps them, now as dashboards in their stored order (Alpha: nothing is moved or renamed for it).
- Names stay unique among dashboards, ignoring capitals (`store.view-name-taken`), at most 200 characters, plain text.

### 1.2 The actions, each one entry of the change history

| Action | What happens in the book | History |
|---|---|---|
| **New dashboard…** (empty or from a template, section 2) | `add_view` at the end of the list. If the list is still the built-in Overview alone, the Overview is stored first, as it is shown (its document with the name translated into the user's language at that moment), so it keeps its place in front | one entry (`as_one_action`, `add_view`) |
| **Rename…** | `rename_view`. Renaming the built-in Overview stores it first | one entry |
| **Duplicate** | `add_view` of a copy right after it, named *Name (copy)* (then *(copy 2)*, …), with the same `template` key and without the default mark | one entry |
| **Make this the default** | `edit_view` adding `"default": true`, and removing it from the dashboard that had it (D-448); storing the built-in Overview first if it is marked | one entry |
| **Move earlier / Move later** (and drag in the list) | `move_view`; storing the built-in Overview first if it is moved | one entry |
| **Delete…** | `remove_view` after a question that names the dashboard and says Undo brings it back. Deleting the last stored dashboard leaves the built-in Overview, so the page is never empty | one entry |
| **Arrange (Edit layout), Done** | as D-315, on the dashboard that is shown: `add_view` the first time, `edit_view` after | one entry |

- **When nothing is stored.** D-315's rule that Done removes the stored Overview when it equals the built-in one again stays only for a list of one: a book whose only dashboard is unchanged stores nothing, so a later version's better Overview reaches it. With two or more dashboards nothing is removed by Done (their order must be kept).
- **Save this layout as…** (D-330) stays in the arranging mode as **Save as a new dashboard…**, with D-330's behaviour (a copy of what is on the page now, its own history entry, the session goes on). D-330's **Load** and **Delete** in *Saved layouts* go: switching dashboards replaces loading, and deleting is in the dashboard menu.
- **Undo across dashboards.** Undo takes back the last change of the book whatever dashboard is shown; when it changes a dashboard that is not shown, the page switches to it, so the user sees what Undo did.

### 1.3 Switching and the dashboard menu (proposal; the maintainer reacts in Phase 2)

- **One dashboard:** the page looks as today. The page's menu (the existing actions of the Overview's header) gets one more entry, **New dashboard…**.
- **Two or more:** a row of tabs above the filter bar, one per dashboard in list order, with the name as plain text; a narrow window scrolls the row. Keyboard: Ctrl+Page Down and Ctrl+Page Up go to the next and previous dashboard; each tab has the accessible name *Dashboard {name}, {n} of {count}*.
- **The dashboard menu** (built by M6d.1, one entry added by each later step): *New dashboard…* (M6d.1; M6d.2 adds the templates to its dialog), *Rename…*, *Duplicate*, *Move earlier*, *Move later*, *Make this the default*, *Delete…* (M6d.1), *Save to a file…* (M6d.3), *Add from a file…* (M6d.4), *Back to the template…* (M6d.5).
- The page title is the dashboard's name; the sidebar entry stays *Overview*.
- **The default dashboard (D-448, answers Q-317).** Opening a book shows its **default** dashboard. The user marks one with **Make this the default** in the dashboard menu; its tab shows a small home icon (accessible name *{name}, default*). The mark lives **in the book**, in that dashboard's document, as the top-level key `"default": true` (additive, the document stays version 2; no layout change, no setting on this computer). At most one dashboard carries it: marking one is one history entry (`as_one_action`, `edit_view` on the new one and on the one that had it), so Undo takes it back. **When the user never marked one** (or deleted the one marked), the **standard dashboard** is the default and shows the mark: the first dashboard made from the `overview` template (section 3), stored or built-in; if the book has none, the first of the list. Deleting the marked dashboard first says which one will open instead. A book whose documents carry the key twice (an edited export) takes the first in the list; the next marking clears the others. Duplicating a dashboard does not copy the mark, *Back to the template* keeps it, and a dashboard file never carries it (section 4.1). Leaving the page and coming back during the session shows the one last shown; that is not written anywhere.
- **What other pages add to.** *Add a saved view* (D-326) and the AI chart card's button after *Keep* (D-428) add to the dashboard the Overview page shows; the card's button names it (*Add to {name}*). Only the label and the target change; the button is still the user's own click, so D-428's reasoning on the human gate holds.

### 1.4 What M6d.1 changes in code (predicted, for the parallel map)

`grid_editor.rs` (the list instead of `is_active_name`, the rules of 1.2), `grid_editor_model.rs` (the list as a model; `savedLayouts` goes), `OverviewPage.qml` and a new `DashboardTabs.qml` and `DashboardMenu.qml`, `NewDashboardDialog.qml`, `AiChartCard.qml` (label only), `vocabulary/actions.rs` (the new handler names); in `kassensturz-report`, `document.rs` (the `template` key) and a new `templates.rs` with `overview` and `empty` (section 3). No change in `kassensturz-store` or `kassensturz-core`.

## 2. Templates (D-440; M6d.2)

A **template** is a built-in starter dashboard the user picks in *New dashboard…*. It is a constant document in `kassensturz-report` (`templates.rs`, started by M6d.1 with `overview` and `empty`), read by `Document::read` in a test like the built-in Overview. Choosing one stores a copy (section 1.2); later changes to the template in a new version never reach a copy, except through *Back to the template* (section 3).

**No English text in a template.** Titles stay empty, so each component shows its own translated default title, as the built-in Overview does; a heading carries a fixed text key that the bridge translates once, when the dashboard is made, and the result is stored as the user's text. The dashboard's name is the template's translated name (made unique as in 1.2).

**The starter set (proposal; the maintainer reacts in Phase 2):**

| Template (`template` key) | Name shown | Components |
|---|---|---|
| `empty` | *Empty* | none; the page shows the arranging mode with the Add menu open |
| `overview` | *Overview* | the built-in Overview (D-314) |
| `spending` | *Where the money goes* | heading; spending ring by category, this month; categories over the last 12 months as stacked bars; spending table, this month; *Money out* card |
| `money-in-out` | *Money in and out* | the four key figure cards (M6a.5); money in and out by month as bars, last 12 months; *Where your money came from* table (D-336); in and out of your accounts table (`cash_flow_by_month`) |
| `worth` | *What you own* | *What you have*, *What you owe*, *What you are worth* cards; net worth over time as a line, last 12 months; account balances table |
| `budgets` | *Budgets* | heading; budget bars (`budget.progress`, all budgets); spending ring, this month. Offered always; the bars take no room until a budget exists (D-381, D-382), and the dialog says so under the picture while the book has none |
| `accounts` | *Your accounts* | account balances table; the fetch-gap note (`notice.fetch_gap`); the *Update all accounts* widget (M6d.7). Offered only while bank connections can be used (D-054); the widget is added to it by whichever of M6d.2 and M6d.7 merges second |

The *New dashboard* dialog shows each template as a small picture of its grid (the frames as boxes with the component's icon), its name and one line of what it shows; *Empty* is first, *Overview* second. Every template fits the 12-column grid and passes `grid::check`.

## 3. Back to the template (D-439; M6d.5)

- A dashboard made from a template carries the top-level key **`template`** with the template's word (section 2). It is additive, so the document stays version 2 (D-289's rule). M6d.1 adds it to `Document::read`, which refuses a word it does not know, with the words `overview` and `empty` and one function `templates::document(word)` that gives a template's document; M6d.2 adds the other words and their documents to it, so M6d.5 needs only M6d.1 (`report.document-invalid`, reason `bad-choice`, path `template`). The built-in Overview and a stored Overview have `overview`; a dashboard made empty has `empty`; one added from a file keeps the file's word; a duplicate keeps its original's.
- **Back to the template…** replaces the shown dashboard's components and filters with the template's, after a question that says what will change (*The 9 widgets of {name} are replaced by the template's 6. Undo brings them back.*). It is `edit_view` (one entry); the name, place and ID stay. For `overview` it is the built-in Overview, for `empty` an empty page. In a list of one, the rule of 1.2 applies: the stored copy is removed, since it equals the built-in one.
- A dashboard without the key (stored before M6d) offers *Back to the template* only when it is the first dashboard and was the M6c Overview (its name is *Overview*); it is treated as `overview`. Otherwise the entry is not shown.
- This replaces the M6b.3 button *Back to the built-in layout* in the arranging mode by the same function for the shown dashboard.

## 4. The dashboard file (D-441 to D-443; human gate: the format; export in M6d.3, import in M6d.4)

A dashboard file holds one dashboard so it can be given to another book, the user's own or someone else's. It holds **no money**: no amounts, no transactions, no balances, no IBANs, no IDs of the book. It does hold the names of the accounts, categories and groups the dashboard uses, the titles and heading texts, and the text of its formulas.

### 4.1 The format, version 1

```json
{
  "format": "kassensturz-dashboard-file",
  "version": 1,
  "application": "KassenSturz 0.261101.8",
  "references": {
    "ref:1": { "kind": "category", "path": ["Everyday", "Pets"] },
    "ref:2": { "kind": "group", "name": "Fixed costs" },
    "ref:3": { "kind": "account", "name": "Colony Checking" }
  },
  "dashboard": {
    "format": "kassensturz-dashboard",
    "version": 2,
    "kind": "dashboard",
    "name": "Where the money goes",
    "template": "spending",
    "filters": { "period": "last-12-months", "accounts": ["ref:3"] },
    "components": [
      {
        "id": "c1",
        "type": "chart.stacked_bar",
        "dataset": "category_over_time",
        "params": { "series": ["ref:1", "ref:2"] },
        "props": { "period": "page" },
        "frame": { "col": 0, "row": 0, "cols": 12, "rows": 6 }
      }
    ]
  }
}
```

- **Header.** `format` is exactly `kassensturz-dashboard-file`; `version` a whole number from 1; `application` is information only (the program and build that wrote it, plain text up to 100 characters) and is never used to decide anything. A file whose `version` is newer than the program knows is refused (`report.unsupported-dashboard-file-version`).
- **`dashboard`** is a dashboard document exactly as the book keeps it (D-278, D-289, D-351, section 3), with one difference: every place that holds an ID of the book holds a **reference key** instead (`ref:` and a number from 1). These places are the ID lists of the schema (`Kind::Ids`, `Kind::Accounts`: `params.groups`, `params.categories`, `params.accounts`, `params.series`, and the members of each column in `params.columns`, D-339) and `filters.accounts`, `filters.categories`. A reference key must be listed in `references`; a UUID or any other text in these places is refused (reason `id-in-file`), so a file can never point at something in a book by its ID, not even by chance.
- **`references`** maps each key to what it names: `{"kind": "category", "path": [...]}` with the names from the top category down (a path is a list, so no separator inside a name can break it), `{"kind": "group", "name": ...}` or `{"kind": "account", "name": ...}`. Every key listed is used at least once.
- **Formulas** already name categories and groups by name (D-292), so they are written as they are stored (D-295) and need no references.
- **Limits** (checked before anything is shown): the file at most 1 MiB; nesting depth 10 for the whole file (the document's own depth of 8 inside it); 500 references; a path of at most 16 names; every name 1 to 200 characters, plain text without control characters, no spaces around it; the document's own limits (100 components, 50 series, D-281). Duplicate keys anywhere are refused, so are keys the format does not know (strict, like the document). The document in a file never carries the default mark: saving leaves it out, and a file whose document has it is refused (`unknown-property`, path `dashboard.default`), so a file can't change which dashboard opens.
- **Writing** (M6d.3): UTF-8 without a byte order mark, keys in the order shown, two spaces per level, a line break at the end; the same dashboard in the same book gives the same file byte for byte (references numbered in the order they are first met). The suggested file name is the dashboard's name with `.dashboard.json` (characters a file system refuses are replaced by `_`); the chooser filters `*.json`. Reading identifies the file by `format`, never by its name.
- The format is this section until M6d.3 writes it out as `docs/DASHBOARD_FILE.md` with the same content and its tests; a later change raises `version`.

### 4.2 Saving to a file (M6d.3)

- *Save to a file…* in the dashboard menu, then the file chooser. Before the chooser, one line on the page says what the file holds: *The file holds this dashboard's layout and the names of the accounts, categories and groups it uses. It holds no amounts or transactions.*
- An existing file is replaced only after the chooser's own question; a write that fails says so with `export.io` (nothing half-written stays: written to a temporary file next to it and renamed).
- The built-in Overview can be saved too (with `template` `overview`, no references).
- Nothing in the book changes, so there is no history entry.

### 4.3 Adding from a file (M6d.4)

The file is untrusted input (T-51). Reading, in this order: size, JSON with the depth limit and no duplicate keys, the header, `references`, then the document by `Document::read` after the reference keys are resolved. A file that fails any check is refused whole, with the reason; nothing is repaired.

**Resolving names (D-432).** Each reference is looked up in the open book, ignoring capitals as the book compares names:
- a category by its whole path, from the top; never by its last name alone and never by a similar name;
- a group by its name; an account by its name, open accounts first, then closed ones.

A reference that matches nothing is **left out** of every list that held it. Nothing is guessed. A list that was not empty in the file and is empty after leaving out would narrow nothing and so show more than its author meant; for each such list the summary says it in words (*"Spending by category" will show all accounts, because none of its 2 accounts are in this book*).

**The summary page, before anything is added.** It shows the dashboard's name (editable; a taken name is offered as *Name (2)*), how many widgets it has, the names it found in this book (*Uses from this book: accounts Colony Checking; categories Everyday › Pets*), every name it did not find with the widgets that used it, and every widget that now shows more, as above. A formula with a name this book lacks is listed too; it shows its mistake on the widget, as any formula does. A component of a type this version does not know is listed and shown as the placeholder (D-278). Buttons: **Add dashboard** and **Cancel**. Only *Add dashboard* writes: one `add_view` at the end of the list (with the stored built-in Overview first, as in 1.2), one history entry, and the page switches to it.

All names and texts from the file are data, shown as plain text, never as markup or instruction (T-08, T-21). The file is never given to the AI by this step, and opening it starts no network access.

**Round trip.** The second of M6d.3 and M6d.4 to merge adds the test: the real exported file of a Space Colony dashboard, read back into the same book, gives the same document with the same IDs; read into a second synthetic book with some names missing, gives the listed notes.

## 5. KMyMoney reports into saved views (D-444, D-445; M6d.6)

**Clean-room** (AGENTS.md §3.1 rule 5, D-044): written from KMyMoney's public handbook (the report types and the settings of its report configuration dialog) and from the maintainer's checked sample files (D-431). No KMyMoney source is read. The attribute names of the file's report records are read off the sample files by M6d.6, which records them in `docs/THIRD_PARTY.md`'s KMyMoney note and in the golden tests; this section speaks of KMyMoney's settings as its dialog names them.

**Where it runs.** Reports come along when a new book is made from a KMyMoney file (`.kmy` or SQLite, the existing book import, D-044); Q-320 asks about the open book. The file is untrusted, as the rest of it is: the importer's existing limits apply, plus at most 500 reports and the document limits for each view made. A report's name, comment and text filters are data.

**Each report becomes one saved view** (`kind` `view`, one component, Q-213 = A) under Statistics, in the file's order, after the book's accounts and categories exist, so its accounts and categories are mapped by the importer's own mapping of the file's IDs to the new book's (not by name). A name taken by an earlier report becomes *Name (2)*.

### 5.1 The mapping

| KMyMoney setting (report dialog) | Becomes | When it can't |
|---|---|---|
| Report type *Income and expenses* (rows of income and expense categories) with chart type pie or ring | `chart.pie` or `chart.donut` on `spending_by_category` (money out) | income rows only: the money in by category, if the dataset offers it; otherwise listed as not kept |
| same, chart type line, bar or stacked bar, columns by month | `chart.line`, `chart.bar` or `chart.stacked_bar` on `category_over_time` with the report's categories as series, or on `income_expense_by_month` when the rows are only the totals | columns by days, weeks, two months, quarters or years: by month, listed (*shown by month*) |
| same, shown as a table (no chart) | `table.rows` on `spending_by_category` | — |
| Report type *Net worth* (assets and liabilities) | `chart.line` on `net_worth_over_time` with a chart, `table.rows` on `account_balances` without | — |
| *Transactions* (query) reports, investment, tax, budget, forecast, schedule, account and loan information reports | not kept: no saved view lists transactions or these figures yet | listed with their kind |
| Detail *All* / *Top level* | all categories / the top categories only | *Groups* and *Totals*: the totals per income and expense (`income_expense_by_month`), listed |
| Date range *Current month*, *Last month*, *Last 12 months*, *Current year* or *Year to date*, *Last year* | the period words `this-month`, `last-month`, `last-12-months`, `this-year`, `last-year` | — |
| Date range *Custom* (two dates) | the typed range of those days (D-332) | — |
| any other relative range (*Last 3 months*, quarters, fiscal years, *Last 7 days*, …) | the days it covers on the day of the import, as a typed range, listed with the KMyMoney period beside the dates (*"Last 3 months" became 1 Aug 2026 to 31 Oct 2026*) (Q-319 = A) | ranges in the future (*Next …*, forecasts): not kept |
| Account filter, category filter | `filters.accounts`, `filters.categories` of the view, by the importer's mapping | an account the book did not take (investments, D-029): left out, listed |
| Payee, tag, text, amount, state, type and number filters | not kept, listed (*filter by payee left out*) | — |
| *Convert to base currency* | figures in the book's currency, as every view shows them (D-349) | not converting: listed (*shown in the book's currency*) |
| Include scheduled transactions, include transfers, show totals column, chart options (data labels, grid lines, line width), report group, favourite, comment | not kept, each listed once per report | — |

Never guessed: a setting that has no row above is listed as not kept by its dialog name. A report whose type or chart type the file does not name clearly is not kept.

### 5.2 The import summary

The book import's done page (`BookFromFileDonePage`, which lists what the book left out, D-132) gets a section **Reports**: *{n} of your {m} reports are now saved views under Statistics.* Then one line per report, in three groups: **Kept as they were**, **Kept with changes** (each change in plain words, as in the last column above) and **Not kept** (with the kind: *a list of transactions*, *an investment report*, …). `SkippedData::Reports` now counts only the reports not kept.

**A period turned into days is easy to check (Q-319 = A; the maintainer expects it may need debugging).** Its line shows the KMyMoney period by its dialog name beside the first and last day it became and the day of the import, so it can be compared with KMyMoney at a glance. The conversion writes one debug-log event (D-227, `KASSENSTURZ_DEBUG_LOG=1`) per converted period with the period's keyword, the day of the import and the two days, and nothing else (no report name, no account or category). M6d.6 tests every relative period of the sample files with a fixed import day, and one test per period word the dialog offers, with the days printed by the code (AGENTS.md §9.4). Each line's wording is written in QML from keywords (D-034): outcome `kept`, `changed`, `not-kept`; reasons `by-month`, `range-fixed`, `range-future`, `totals-only`, `filter-left-out` (with the filter's kind: `payee`, `tag`, `text`, `amount`, `state`, `type`, `number`), `account-left-out`, `currency`, `options-left-out`, `transactions-list`, `investments`, `tax`, `budget`, `forecast`, `information`, `unclear`.

**Tests.** Golden files on the maintainer's checked sample files (both formats give the same views), each part A report of `proposals/kmymoney-reports-acceptance.md` named in a test, and the figures of each kept view compared with the importer's real book (tests where steps meet, AGENTS.md §9.4).

## 6. The Update all accounts widget (D-446; M6d.7; human gate: bank connections)

**What it is.** A widget for any dashboard: one button, **Update all accounts**, that asks the banks for the new transactions of every linked open account, in turn, with the existing fetch (`bank_connection::fetch`, M4.7, D-153). It is the *Get new transactions* page with every linked account ticked and *Fetch* pressed, placed where the user starts, except that what it fetches is booked at once and only what needs a look is shown (D-449).

- **Component type `bank.update_all`**, no dataset, one property `props.title` (empty: *Update all accounts*). It is in `ComponentType::LIBRARY` (the Add menu, group *Bank*), **not** in `ComponentType::ALL`, so the AI can never pick or place it (D-282) and `propose_view` refuses it as an unknown type. The MCP server gets no function for it. Smallest size 3 × 2 cells.
- **When it shows.** Only while bank connections can be used (D-054) and at least one open account is linked; otherwise it takes no room on the page in view mode, the rule of D-382 here for a widget of any width. In the arranging mode it shows a plain note instead (*Shows a button once an account is linked to a bank*). A build or system without bank support does not list it in the Add menu.
- **Only on the user's click.** No timer, no fetch on opening the book or the app, no repeat, nothing in the background (AGENTS.md §3.1 rule 4). Never payments (D-053). The button is the only way it starts.
- **Order and PINs.** Accounts are fetched one bank login after another, in the order of the first account of each login in the accounts list, and within a login in the accounts list's order, so each bank asks for its PIN once and its TAN or approval in one stretch. PINs follow D-057 (asked, remembered until the app closes, or from the keychain, as the user chose). The PIN, TAN and approval dialogs are the existing `BankDialogs.qml`.
- **When one bank fails.** Cancelling a bank's PIN or TAN skips that bank's accounts (*skipped: you cancelled the PIN*) and goes on with the next bank. An account that fails does not stop the others, unless the same would happen to them (`ends_the_fetch`), as today. A fetch already running (here or on the *Get new transactions* page) disables the button (`bank.busy`).
- **While it runs.** The button becomes a progress line, *Updating 2 of 5: {account name}*, with **Stop**, which stops after the account being fetched; nothing of a stopped account is half kept.
- **Booked straight away (D-449, answers Q-318).** After each account is fetched, its new transactions go through the same import as *Get new transactions* (`statement_import::import`: the Rules, the duplicate check, the bank's closing balance, D-030, D-086, D-117) and are **booked at once**, with no review step and no "to check" state. Each account is one *Imported a statement* entry of the change history, so Undo takes an account's update back. Entries the duplicate check recognises are left out, as on that page. Transfers the import suggests (D-087) are not joined on its own; the two sides stay as booked and can be joined later from the register. A closing balance that doesn't agree with the bank is said on the account's line, as the import says it today.
- **Notices: only two kinds**, for spotting payments the user doesn't recognise (theft, fraud, a mistake). A booked transaction gets a notice when
  1. **No Rule sorted it**: it was booked without a category; or
  2. **Its amount is unusual for its category** (the rule below).
  Everything else is booked silently. The words never say fraud or give advice (AGENTS.md §3.1 rule 11); they say what is unusual and ask the user to check it: *Unusual for Groceries: 412.00 €, usually about 45.00 €. Do you know this payment?*
- **What counts as unusual** (exact, in whole minor units, never floating point, AGENTS.md §3.1 rule 1). Look at the transactions already booked in the same category, in the same currency and the same direction (money out or money in), dated in the 365 days before the new one. If there are fewer than 5, nothing is called unusual (too little to compare with). Otherwise the **usual amount** is their median (the middle one by size; with an even count, the lower of the two middle ones, so no rounding is needed). The new amount is unusual when it is **at least 3 times the usual amount, or at most a third of it** (`amount ≥ 3 × usual` or `3 × amount ≤ usual`). A category's own subcategories count as the category. The function lives in `kassensturz-report` (it reads the book through the `Source`), with property tests: exact results, the same answer whatever the order of the earlier transactions, and no notice below 5 earlier ones. The factor 3, the count 5 and the 365 days are constants named in the code, documented here.
- **The summary**, on the widget until the user presses *Seen* or the session ends (nothing about it is stored): *4 accounts updated, 23 transactions booked. 2 to check.* Then one line per notice (date, payee as plain text, amount, account and its reason), each opening the transaction in the register; at most 20 lines, then *Show all*. Failed and skipped accounts each get one line with the existing words for their error (`bank_connection::error_text`). A transaction booked without a category also stays findable in the register as every uncategorised one is. Below the button, when nothing has run in the session: *Last updated: {the oldest last-fetched day of the linked accounts}*.
- **Logs and data.** The debug log gets the handler's event names only (D-359), never an account name, IBAN, amount or the bank's text. The widget's words from the bank are data, shown as plain text (D-143).
- **Tests.** The fake bank (`KASSENSTURZ_FAKE_BANK=1`) with two logins, one asking a PIN and one failing; a test that the AI's component list and `propose_view` refuse `bank.update_all`; a QML smoke test that the widget is hidden without a link and shows the summary after a fake run; tests with the importer's real output that a fake update books the entries, leaves duplicates out, gives the two kinds of notice and is taken back by Undo; a real-app screenshot (Xvfb).

## 7. Shared QML components (the M6a retro rule)

Planned names and their owners, carried in every M6d brief: M6d.1 `DashboardTabs`, `DashboardMenu`, `NewDashboardDialog`; M6d.2 `TemplatePicture` (inside `NewDashboardDialog`); M6d.3 none (the file chooser of `ExportPage` is reused); M6d.4 `DashboardFilePage` (the summary of 4.3); M6d.6 `KmyReportsSummary` (inside `BookFromFileDonePage`); M6d.7 `UpdateAllWidget`. M6d.4 and M6d.6 do not share a summary component: the first is a preview before adding, the second a report after a finished import.

## 8. Risks and their rows in the threat model

New rows T-51 to T-55 in `docs/THREAT_MODEL.md`: a hostile dashboard file, a dashboard file that tells more than the user meant to share, a name in a file that matches the wrong thing or silently widens a chart, a KMyMoney report that changes its meaning on import, and an Update all that reaches the network without the user or leaks bank data.

## 9. The M6d steps

The table is in `docs/ROADMAP.md` ("M6 steps", rows M6d.\*), with needs, tier, effort and gate. Wave 1 after M6d.0: M6d.1, M6d.3, M6d.4, M6d.6 (with the sample files) and M6d.7; wave 2 after M6d.1: M6d.2, M6d.5 and the menu entries of M6d.3 and M6d.4.

## 10. Questions for the maintainer (through the coordinator)

Each has a safe default that the steps build until it is answered.

- ~~**Q-317: Which dashboard opens when a book opens?**~~ Answered by the maintainer, 2026-11-01: the user can mark one dashboard as the default, and while none is marked a standard dashboard carries the mark (D-448, section 1.3).
- ~~**Q-318: After *Update all accounts*, what happens with the new transactions?**~~ Answered by the maintainer, 2026-11-01: they are booked at once, with a notice only for a transaction no Rule sorted and for an unusual amount (D-449, section 6).
- ~~**Q-319: A KMyMoney report with a relative period that KassenSturz has no word for?**~~ Answered by the maintainer, 2026-11-01: A, the days it covers on the day of the import, made easy to check (section 5.2). The other option was new relative period words.
- **Q-320: Reports from a KMyMoney file into a book that already exists?** A (R): not in M6d; reports come with a new book made from the file. B: also *Add reports from a KMyMoney file* for the open book, matching accounts and categories by name as dashboard files do.
