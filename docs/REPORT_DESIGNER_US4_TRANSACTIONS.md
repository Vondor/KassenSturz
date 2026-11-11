# The transactions tile (US4.7)

Design of User Sprint 4 item 7 (US3 backlog 1), written by thread A in Phase 2 before its build (D-716, D-717). The wish: a tile for the Overview and every dashboard that lists single transactions like an account's list does, for the accounts, categories and days the user chooses, including a rolling "last N days". Today no tile lists single transactions: `table.rows` shows a dataset's totals, and the Overview's "newest transactions" list is fixed to the five newest of all open accounts.

Release stage: **Alpha** (D-274). There is no book layout change and no document version change: the tile is a new component type, and an older build shows it as a placeholder and opens the rest of the dashboard (D-278, D-504). The new period word is a new word of an existing property, which an older build refuses like any unknown word. So this is not a **[format]** item.

**What stays true (D-342):** the app is complete without the tile. It is not in the built-in Overview or in a template; the user adds it with *Edit layout › Add › Figures and tables › Transactions*. **It only shows:** nothing is edited in the tile; a click opens the usual page of that transaction. **Numbers come from code:** every amount is formatted in Rust and reaches QML as text (D-065); the order is worked out in Rust.

The steps follow `docs/ADDING_A_WIDGET.md` (D-534).

## 1. What the tile shows (D-716)

### 1.1 The rows

One row per transaction, **newest first**; on the same day the one added last comes first, as in an account's list (D-067). Each row has:

| Column | What it holds |
|---|---|
| Date | the booking date, in the user's date format |
| Account | the user's account it is listed from; **only shown when the tile can list more than one account** |
| From / To | the name shown (D-034); empty if there is none |
| Category | the category; for a Transfer *Transfer: <other account>*; for a transaction with several Parts *Several Parts*; *Not sorted yet* when it has none |
| Amount | the amount as seen from that account, with the user's number format, the exact amount on hover (D-002), red for money out as in the register |

- **No balance column.** An account's list shows what was left after each entry only in date order and for one account (D-656). A tile mixes accounts and leaves rows out, so a figure beside a row would read wrong.
- **No total line.** The rows can be in different currencies, and a split transaction shown because one Part fits the category filter would count its other Parts too. A sum belongs in a number card, whose figure the report layer works out exactly. A total line can be a later wish with its own rule.
- **Starting balances are left out**, as in the Overview's newest list: they are not something the user paid or received.
- **A Transfer between two accounts the tile lists is one row**, listed from the side with the later date, or from the account the money reached if both have the same date (D-108), as the Overview's newest list does. A Transfer with only one side in the tile's accounts is listed from that side.
- **Each row keeps its account's currency.** Nothing is added up, so no currencies are mixed (AGENTS.md §5).

### 1.2 Which transactions

The tile honours the **three shared filters** like every data tile (D-350, D-351), first match wins: its own setting, for the period the period box it lies in (D-333), the dashboard's filter bar, then nothing.

- **Accounts:** the user's own accounts (bank accounts, cash, cards, debts). Default: all open accounts. A closed account is listed only when it is chosen by name.
- **Categories:** a transaction is listed when **at least one of its Parts** is booked to a chosen category or one of its subcategories, or to a category of a chosen My group. With no category filter every transaction is listed, *Not sorted yet* ones and Transfers included. With a category filter, Transfers and *Not sorted yet* are left out, as they have no category.
- **Period:** every period word the other tiles take (this month, last month, the last N months, a salary month, a typed range, D-332, D-644), plus the new **last N days** (section 2). Default: the page's period.

The grey filter notes and their *Hide the grey filter notes* switch work as on every tile (D-663).

### 1.3 How many

The list scrolls inside the tile. It holds **at most 200 rows** (a dashboard file is untrusted, D-281, and a tile is no place for a year of bookings). When more transactions fit, the last line says *Showing the newest 200 of N.* and *Open the list* takes the user to the full list when the tile lists one account; for several accounts the line only counts. The user can lower the number in *Properties* (**Rows: 10, 20, 50, 100, 200**; default 20).

When nothing fits, the tile says *No transactions in this period.*, or *No transactions in these categories in this period.* with a category filter, never an empty box (AGENTS.md §6, no dead ends).

### 1.4 Opening a transaction

A click on a row, or Enter on the focused row, opens the transaction exactly as from its account's list (`Main.openTransaction(accountId, transactionId)`): the simple page, the Parts page or the Transfer page. After a change, every tile of the dashboard is drawn again as today when the book changes, so the row shows the new state.

### 1.5 Properties

*Properties* (the existing panel) offers:

| Setting | Stored as | Values | Default |
|---|---|---|---|
| Title | `props.title` | text, as every tile | *Transactions* |
| Period | `props.period` | every period word, `page` | `page` |
| Accounts | `params.accounts` | the account list of every data tile | all |
| Categories | `params.categories` | the category and My groups list of every data tile | all |
| Rows | `params.rows` | `10`, `20`, `50`, `100`, `200` | `20` |
| Hide the grey filter notes | `props.hide_filter_notes` | on, off | off |

Every value is checked when a document is read (D-281): an unknown `rows` word makes the component an error tile with its reason, as for every property.

### 1.6 Type, size, name

- Type id **`table.transactions`**, filled by its own bridge object (`BookTransactions`, like the budget bars' `BookBudgets`). Its document names no dataset, as *Where the money came from* (`table.income_sources`), and like that tile it takes all three shared filters (`filter::takes`). So no dataset is added to the catalog, which the AI and the chart descriptions read.
- **Not in `ComponentType::ALL`:** the AI does not propose this tile, and no AI tool reads it. So no human gate (AI data sharing) applies. Letting `propose_view` add it is a possible later wish with its own gate.
- Smallest size **4 × 3**, default **6 × 5** on the 12-column grid (D-287, D-296).
- English name *Transactions*, German *Buchungen* (`docs/GLOSSARY.md`). In the Add menu under *Figures and tables*.
- **Dashboard file:** the type id and its properties, with accounts and categories by name like every tile (D-432, D-441). Nothing new.

### 1.7 Accessibility

Each row is one focusable item whose screen-reader name reads the whole row (*12 Nov 2026, Checking, To Supermarket, Groceries, minus 23.40 €*). Arrow keys move between rows, Enter opens. Money out is told by the sign and the word, not by colour alone.

## 2. The rolling period "last N days" (D-717)

A new rolling period beside *the last N months* and *a salary month* (D-644, D-645):

- **Word** `last-N-days`, N from 1 to **366**, written without a leading zero (as the other rolling words). It covers **today and the N − 1 days before**: `last-7-days` on a Wednesday is the Thursday a week before up to that Wednesday. `last-1-days` is today only.
- It is a period word like any other: every tile, the period box, the filter bar and the formula functions that take a period word read it, because they all go through `Period::named` and `is_period_word`. Charts by month show the one or two months it touches, counted only on its days, as a typed range does today.
- **Comparison periods** (`previous_period`, D-687) of a period of N days are the N days right before, which `Period::before` already does for typed ranges.
- **Choosing it:** the period choice gets *The last days…* below *The last months…*, with a spin box *Days:* (1 to 366, default 7).
- The upper end 366 keeps a whole leap year possible; for longer spans the last N months or a typed range fit better.

## 3. What is built, in one PR

1. `kassensturz-report`: `Rolling::Days` (word, read, period, tests on month and year ends and leap days); `ComponentType::Transactions` with its property table (`rows`), sizes, `LIBRARY`, and `Takes::ALL` in `filter::takes`.
2. `kassensturz-core`: `table_transactions` in `vocabulary::DETAILS`.
3. `kassensturz-ui-bridge` (Rust): `transactions_tile.rs` (the rows, from the store's register and outline reads, filter and order in Rust, at most `rows`) and `transactions_tile_model.rs` (`BookTransactions`, `show(period, scope)`, `view` with `rows`, `shown`, `total`, `accounts` and the filter notes); bridge tests that read the object back as QML does, including a Transfer between two listed accounts, a split with one Part in the filter, two currencies, and the 200-row cut.
4. QML: `TransactionsTile.qml`, its branch in `GridItem.qml`, `GridNames.qml`, `AddMenu.qml`, the *Rows* choice in the properties panel, *The last days…* in `PeriodChoice.qml`; a smoke test in `qml-tests/`.
5. `po/de` entries, `docs/ADDING_A_WIDGET.md` counts, `docs/DASHBOARD_FILE.md` (the new property), the testing guide steps.

## 4. Not part of this item

Editing in the tile, a quick search field, sorting by another column, a total line, the AI proposing the tile, and a "newest N" mode without a period. Each can be a later wish.
