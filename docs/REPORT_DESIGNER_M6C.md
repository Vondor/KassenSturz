# The report designer, part 3: budgets, financial figures, shared filters and the AI chart builder (M6c)

Design of the M6c Sprint (D-284), written in step M6c.0. It builds on `docs/REPORT_DESIGNER.md` (the reporting layer, D-277; the AI and the designer, D-282) and `docs/REPORT_DESIGNER_M6B.md` (the grid, groups, formulas, the period box of D-333). Decisions D-344 to D-359, questions Q-264 to Q-268; threat rows T-46 to T-50; the steps are in `docs/ROADMAP.md` ("M6c steps").

Release stage: **Alpha** (D-274). Layout 22 is a development layout: no migration of older books is written, and the development migrations are merged into one starting layout at the end of Alpha (D-045).

**The maintainer's answers this design applies:** Q-216 = D (each budget has its own period, month, quarter, year or a range of days, and its own switch to carry what is left into the next period), Q-264 = A (a budget widget on the Overview, hidden until a budget exists; the rest of the Overview's design is left to the team), Q-265 = B (the AI may prepare budgets in M6c, always as changes the user confirms), Q-266 = A (carry over moves overspending too), Q-267 = A (an own setting wins for its dimension only), Q-268 = A (no My groups for the AI in M6c).

## 1. Budgets (D-344, D-345, D-346, D-347)

### 1.1 What a budget is

A **budget** is an amount the user plans to spend on one spending category in a stretch of time, and nothing else: it never changes a booking, a balance, a category total or any other figure (principle 1). It is a way of looking at the spending, like a group (D-290).

| Part | What it is | Rules |
|---|---|---|
| Category | One **spending** category (kind `expense`). It counts with everything inside it, each category once, as a group does (D-291) | An income category, an account, or money not sorted yet can't have one (`store.budget-not-spending`) |
| Period | `month`, `quarter` or `year` (calendar months, quarters, years; it repeats) or `range` (one stretch of days, from a first to a last day; it does not repeat) | A repeating budget starts on the first day of a month, quarter or year and may end on the last day of one; a range has both days and covers at most 100 years (`MAX_MONTHS`, D-281) |
| Amount | What the user plans to spend in one period, in the **category's own currency**, as `Money` in minor units, more than zero | Up to the largest amount `Money` holds; zero or less is refused (`store.budget-amount`) |
| Carry over | On or off. When on, what is left at the end of a period is added to the next one; what was spent beyond the amount is taken off the next one (Q-266 = A) | Not offered for a range, which has no next period (`store.budget-period`, `carry-over-on-range`) |

Monthly, quarterly and yearly budgets and envelope-style budgeting ("money left in the envelope stays in it") are all settings of this one model, as Q-216 = D asked.

**One budget per category at a time** (D-344): two budgets of the same category whose periods overlap are refused (`store.budget-overlap`). A budget on a category and another on a category inside it are allowed and are separate figures; KassenSturz does not check that the inner ones fit into the outer one, it shows both (a check of that kind would be advice-like and is not needed for correct figures). A later Sprint may allow several budgets per category if users ask for it.

### 1.2 Changes keep the past (D-345)

The amount and the carry-over switch are **dated**: each change applies from the start of one period of the budget on, and earlier periods keep what they had. So a past period's result never changes when the user raises next month's amount, and a carried amount stays what it was (principle 1, mission: nothing is overwritten without the user's say-so).

- The editing screen changes the amount **from the current period on** by default; *From another period…* picks an earlier or later one (to correct a mistake from the start, the user picks the first period).
- *Stop this budget* sets its last day to the end of the current period; the past stays.
- *Delete* removes the budget with all its past results. The screen says so in one plain sentence before it runs, and one Undo brings it back (D-032).
- Every one of these is one entry of the change history (`add_budget`, `edit_budget`, `remove_budget`), so Undo and Redo work as everywhere.

Moving or renaming the category does not touch the budget. Moving a category into another one changes what that outer category's budget counts, from then on and for the past, as every report reads the category tree as it is now. A category that has a budget can't be removed: `store.category-in-use` gains the keyword `budgets` (D-253 removes only unused categories); the user deletes the budget first.

### 1.3 Layout 22 (D-345; built in M6c.L)

Two tables, their `changed_` tables, three history actions and the complete export's next version. M6c.L writes the rows into `docs/BOOK_FORMAT.md` and `docs/EXPORT_FORMAT.md`.

`budgets`:

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `category_id` | BLOB → `accounts.id` | The spending category. The database refuses removing a category that has a budget (no cascade); KassenSturz checks the kind when it writes |
| `every` | TEXT | `month`, `quarter`, `year` or `range` |
| `first_day` | TEXT | ISO 8601. For `month`, `quarter` and `year` the first day of one |
| `last_day` | TEXT, may be NULL | ISO 8601, not before `first_day`. NULL: the budget keeps going (never for `range`). For `month`, `quarter` and `year` the last day of one |

`budget_amounts`:

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7, so the change history can name the row |
| `budget_id` | BLOB → `budgets.id` | Removing the budget removes its rows (ON DELETE CASCADE), in the same change, so Undo brings them back |
| `from_day` | TEXT | ISO 8601, the first day of one of the budget's periods, not before `first_day`. The row counts from that period on, until the next row |
| `amount` | INTEGER, above 0 | Minor units of the category's currency (D-027) |
| `carry_over` | INTEGER, 0 or 1 | Whether what is left at the end of each period from `from_day` on goes into the next. Always 0 for `range` |

A unique index on (`budget_id`, `from_day`); every budget has a row with `from_day` = `first_day` (KassenSturz checks this when it writes). The store refuses two budgets of one category whose days overlap. Limits checked when writing: at most 1,000 budgets in a book and 1,200 amount rows per budget (a hostile copied book can't make the reports slow, T-47).

History actions: `add_budget`, `edit_budget` (amount, carry over, last day; adds, changes or removes `budget_amounts` rows) and `remove_budget`. The complete export goes from version 7 to **version 8** with `budgets` and `budget_amounts` (D-029 complete export). The KMyMoney importer keeps listing budgets as skipped data (D-029); importing them is not part of M6c.

### 1.4 How a budget counts (D-346, D-347; built in M6c.4)

The dataset `budget_utilization` in `kassensturz-report`, a pure function of a `Source`, the budgets and a day:

- **Which period:** for each budget, the period that contains the **day it is asked for**: the last day of the shown period, or today if that is earlier. So "this month" on the Overview shows a monthly budget's month, a quarterly budget's quarter and a yearly budget's year, each as it stands today. A budget that has not started or has ended by then is not in the result. The grouping `period` gives one row for each of the budget's periods that the shown period touches, for a chart over time.
- **Spent:** the parts booked to the category or inside it, each category once, by the transaction's booking date (as every dataset, D-122), with the amount the part has in the **category's currency** (the amount a part in another currency was booked with, D-043; never converted again at a later rate). A refund reduces it and can make it negative. The hide list does not change it (Q-225), and My groups play no part.
- **Available** = the period's amount + what was carried in. **Carried in** = what was left at the end of the period before (its available less its spent, which is negative after overspending) if that period had carry over on; otherwise zero. The chain starts at zero in the budget's first period, so the `Source` is read from the first day of the earliest budget with carry over (one read transaction, as before).
- **Left** = available − spent; below zero the result says *over by* that amount.
- **Used** = spent ÷ available as a `Percent`, rounded **half away from zero to one decimal place**, whatever the book's rounding setting says, the same rule as every ratio (D-285). It has **no value** when available is zero or less, and the component says so in words (*Nothing left to spend in this period*), never 0 % or infinity. Over 100 % is shown as it is (e.g. 112.5 %); a bar is drawn full with the part beyond marked, its drawing values clamped as drawing only (D-065).
- **Exact:** every amount is a `Money` sum with overflow checks; the result keeps exact amounts and leaves the crate as text (D-065). Property tests: available − spent = left for every period, the carried chain equals a step-by-step sum, and a change of an amount from a period on never changes an earlier period's figures.
- **Plain words, no advice** (AGENTS.md §3.1 rule 11): *Planned 400 €*, *Spent 380 €*, *20 € left*, *37.50 € over*, *Carried from last month: 12 €*. The app never says *you should*, *too much* or *save*, and no colour other than the bar's over part suggests judgement.

The catalog entry's description says all of this in plain words for the properties panel and the AI.

**As built in M6c.4 (D-378 to D-380):** the current period counts its spending from its first day to the day asked for (`counted_to`), so a booking dated later in it is not spent yet; grouped by `period`, only the periods that start by that day are shown. `Source::read_with_budgets` reads the budgets and the days they need (back to a carrying budget's first day) in one read. The dataset (`BUDGETS`) and `budget.progress` (properties `categories`, `title`, `period`) join the catalog and the component library in M6c.5, which draws them. Each period carries notes by fixed word (`over`, `nothing-to-spend`, `carried-in`) for the UI's sentences.

### 1.5 The budget screens (D-357; Q-264 = A; built in M6c.5)

- **Budgets** page in the side menu: the budgets with their current period's bar, *Add a budget*, and per budget *Change*, *Stop* and *Delete*. With none yet, the page explains in two sentences what a budget is and offers *Add a budget*.
- **Add or change a budget** sheet: the category (spending categories only), *How often* (Every month, Every quarter, Every year, Once, for some days), the amount, *Carry over what is left* (hidden for Once), and for a change *From this period on* / *From another period…*. The sheet shows what the first period will be (e.g. *1 Nov 2026 to 30 Nov 2026*). Every error has a plain message from `docs/ERROR_CODES.md`.
- A category's own menu gets *Set a budget…*, which opens the sheet with the category filled in.
- The component `budget.progress` (one bar per budget, or the budgets of the chosen categories) is in the component library for every dashboard and for Statistics. **Overview (Q-264):** the built-in Overview layout gets it under the key figures; it draws nothing and takes no room in view mode while the book has no budget, and shows *Set a budget* only in edit mode, so the start screen stays complete without it (D-342).

### 1.6 AI-prepared budgets (D-358; Q-265 = B; built in M6c.11)

The AI may prepare budgets, for example amounts based on past spending; the user always confirms them, and nothing is ever written before that.

- **The one path:** a new list `new_budgets` (and `edit_budgets` for an existing budget) of `propose_changes`, so a prepared budget goes through `propose`, the review page, the user's edits (D-297), `check_allowed` again at the confirm and `ChangeSet::apply` as one `apply_ai_changes` history entry, labelled AI-made (D-198), taken back by one Undo.
- **Numbers come from code (AGENTS.md §8):** an entry names the category, the period kind and, for the amount, either **`based_on`** (an enum: the average of the last 3, 6 or 12 full periods, or the last full period, rounded up to a step from an enum: 1, 5, 10, 50 or 100 of the currency) or **`amount`** (an exact decimal text, only for a figure the user named in the chat; checked like any typed amount). With `based_on` the app computes the amount from `budget_utilization`'s spending, and the review page says how: *Average of the last 6 months: 387.40 €, rounded up to 390 €*. The model never supplies a computed figure. A `based_on` without the full periods it needs is refused (`ai-tools.bad-argument` `based_on`).
- **Permission grid (D-252):** a new row **Budgets** with Read, Add and Change, all off at the start; Delete is *not offered*. Add and Change switch Read on (D-252). Read gives a new read function `list_budgets` (named in T-23, like every function) (category, period kind, the current amount and carry over, and the current period's spent and left as `budget_utilization` computes them; the spent figure also needs the cell `category_totals` needs). With AI off, none of it is visible or reachable (D-013).
- **What the model sees:** the category ids and names only as `list_categories` gives them under Categories › Read; the budgets only under Budgets › Read; never the book's other figures. Budget amounts it reads count as book figures in the number check (D-188).
- **Checks:** the same store checks as a hand-made budget (spending category, no overlap, amount above zero), plus at most 50 budgets in one set; an edit of a budget changes from the current period on only, so the AI can't rewrite the past.

## 2. Financial datasets (D-348, D-349; built in M6c.1)

New datasets in `kassensturz-report`, each a pure function of a `Source`, a `Period` and a `Filter`, with a plain-word catalog description that says what it counts and leaves out.

**Balances on a day:** an account's balance at the end of a day is what `Book::balance_on` gives: every part booked to it up to that day, by the day the bank booked it where it says so, otherwise the transaction's booking date, in the account's currency (a part in another currency counts with its amount in the account's currency, D-043). `Source::read` also reads every account's balance at the end of the day before the period, in the same read transaction, so the datasets need no second read and can't mix two states of the book. Closed accounts count with their history (after closing their balance is zero, so they add nothing).

| Dataset id | Rows | Groupings | Measures |
|---|---|---|---|
| `account_balances` | each account the user has (open, or closed with a balance that is not zero on the day): name, *have* or *owe*, balance on the period's last day | `account`, `account_kind` | `assets`, `liabilities`, `net_worth` |
| `net_worth_over_time` | for each calendar month the period touches, at the end of its last day inside the period: what you have, what you owe, what you are worth | `month` | `assets`, `liabilities`, `net_worth` |
| `cash_flow_by_month` | for each calendar month: balance at the start, money in, money out, starting balances set, balance at the end, of the chosen accounts together | `month` | `money_in`, `money_out`, `net_change` |
| `budget_utilization` (§1.4) | for each budget: period, planned, carried in, available, spent, left, used | `budget`, `period` | `spent`, `left`, `used_percent` |

- **What you have** (`assets`) is the sum of the balances of the accounts of kind `asset` (checking, savings, cash); **what you owe** (`liabilities`) is the debt of the accounts of kind `liability` (credit cards) as a positive amount; **what you are worth** (`net_worth`) is the first less the second. An account counts by its kind, not by the sign of its balance: an overdrawn checking account lowers what you have, a credit card in credit lowers what you owe. Categories and the internal starting-balance account are never in these figures.
- **Cash flow** is the money that went into and out of the chosen accounts, by default all accounts of kind `asset`. A transaction counts with its net amount on the chosen accounts, per currency: above zero is money in, below zero money out, so a move between two chosen accounts counts as nothing and paying the credit card from the checking account counts as money out (the card is not chosen by default). A starting balance set inside the period is its own column, so for every month and currency **start + in − out + starting balances = end** exactly (a property test). The same day rule as the balances applies, so the start and end are the balances above.
- **One result per currency** (as every dataset): amounts in different currencies are never added together.
- **One currency on request (D-349):** `account_balances` and `net_worth_over_time` take an optional `in_currency` (a currency the book uses). Each other currency's total on a row's day is converted with the exchange rate that applies on that day by `Book::convert`'s rule (the newest rate on or before the day, in either direction, rounded once in the book's rounding mode, D-027, D-079); the result lists, per currency, the rate and its date. **A currency without a rate on or before the day is not converted and not guessed:** the result lists it with its own amount, the total is marked as leaving it out, and the component says so in words (*No exchange rate for USD on or before 31 Oct 2026; 1,200.00 USD are not in this total*). The rates come from the book's own price table only, entered by hand (D-028); nothing is fetched.
- `card.net_worth` (D-329) is then filled from `account_balances` instead of the page's own figures, with the same look (M6c.2), so it can follow a period box and the filters.
- The catalog row `balance_over_time` of `docs/REPORT_DESIGNER.md` §3 is not built: one account's balance over time is `net_worth_over_time` with the accounts filter set to that account.

The schema additions (component types, measures, groupings) are made by the step that builds the dataset (M6c.1 and M6c.4), so the component steps (M6c.2, M6c.5) touch only the bridge and QML.

## 3. Shared filters (D-350, D-351; report layer in M6c.6, filter bar in M6c.7)

A **filter** narrows what a dashboard shows: the **period**, the **accounts** and the **categories**. The report layer applies it, not the UI: one function in `kassensturz-report` works out, for each component, the period and `Filter` it is drawn with and the notes it shows, so QML maps nothing (as `properties::preview` does).

**Where a value comes from, per dimension** (period, accounts, categories), first match wins:

1. The component's **own** setting (`props.period` other than `page`, D-289; `params.categories`, D-331; the new `params.accounts`).
2. For the period only: the **period box** the component lies in (D-333).
3. The dashboard's **filter bar** (`filters` of the document).
4. Nothing: all accounts, all categories, the dashboard's default period.

So an own setting wins **for its dimension only**, as an own period already wins over the period box (Q-267 = A): a pie with its own categories still follows the filter bar's period and accounts. The component shows a small *Own filter* mark naming which dimensions are its own.

**What a dataset takes:** each catalog entry lists the dimensions it can honour without making a figure wrong:

| Dataset | Period | Accounts | Categories |
|---|---|---|---|
| `spending_by_category`, `income_expense_by_month`, `category_over_time`, the columns of D-339 | yes | yes (a whole transaction, M6a.1) | yes (the part, M6a.1) |
| `account_balances`, `net_worth_over_time`, `cash_flow_by_month` | yes | yes (which accounts count) | no |
| `budget_utilization` | yes (the day it is asked for, §1.4) | no (a budget counts every account) | yes (which budgets) |

A dimension a dataset does not take is **not applied, and the component says so** under it (*Not filtered by account: a budget counts every account*), never silently. Money the filter leaves out is out of every figure of that component, which carries a *Filtered* mark listing what the filter keeps (the hidden note of D-291 stays as it is: a filter does not unhide a category).

**What the report layer refuses** (`report.filter-invalid`, with a fixed reason): an id in `accounts` that is a category, an id in `categories` that is an account, more than 200 ids in one list, an id twice. An id that is no longer in the book (a removed category) is left out and the filter bar lists it as *something that no longer exists*, so the user can take it out (as D-340); a list left with nothing chooses nothing, and the component says *The filter chooses nothing* instead of drawing zeros.

**The document (D-351):** `filters` gains `accounts` and `categories` next to `period`, and components gain `params.accounts`; the dashboard document goes to **version 2**, so an older build says it was made by a newer KassenSturz instead of a reading error. Version 1 documents are still read (Alpha: no migration, the reader takes both).

**The filter bar (M6c.7):** a row above the Overview's grid (and every dashboard of M6d) with *Period*, *Accounts* and *Categories*, each a button that opens a list with check boxes (the period one the choices of D-332). In view mode a choice is **for now**: it changes what the page shows, is not written into the book and is not a history entry; *Reset filters* goes back to the layout's own. In **edit mode** the bar's choices are saved with the layout when the user presses Done (one `edit_view` entry, one Undo), so a layout can carry its filters. The properties panel gets *Accounts* next to *Categories* and, for each dimension, *Follow the page* or the component's own.

## 4. The AI chart builder (D-352 to D-356)

### 4.1 One function, `propose_view` (D-352; M6c.8)

The full tool replaces the list `new_views` of `propose_changes` (M6a.9): one function of its own, so the model reads one schema built for charts. Its result is the same change-set entry as before (a new view), so the review page, the user's edits (D-297), the confirm, the AI-made label and Undo stay exactly as they are; only how the model asks changes. The MCP request file (D-216) takes the same function. With AI off, it does not exist (D-013); it needs the grid cell Charts › Add (`AllowNewViews`, D-282). Like every function it is named in T-23 of `docs/THREAT_MODEL.md` in the same pull request (a test checks it).

Arguments (JSON object; every value but `title` and the dates is from a fixed list, sent to the model as JSON-schema `enum`s built from the catalog and the schema, so a list in the description can never drift from the code):

| Argument | Required | Values |
|---|---|---|
| `title` | yes | Plain text, one line, 1 to 200 characters; also the view's name (`ai-tools.name-taken` if a view has it) |
| `data` | yes | A catalog dataset id with `for_ai` (§4.3) |
| `show_as` | yes | `auto`, `line`, `area`, `bar`, `stacked_bar`, `pie`, `donut`, `figure`, `progress`, `table`, `budget_bars`; `auto` picks as M6a.9 does, plus a line for `net_worth_over_time`, bars for `cash_flow_by_month` and budget bars for `budget_utilization` |
| `measure` | no | One of the dataset's measures (§2) |
| `group_by` | no | One of the dataset's groupings the AI may use: `category`, `top_level_category`, `month`, `account`, `account_kind`, `budget`, `period` |
| `period` | no | `page`, `this-month`, `last-month`, `last-12-months`, `this-year`, `last-year`, or `{"from": "YYYY-MM-DD", "to": "YYYY-MM-DD"}` with one or both days (D-332 range rules); default: the dataset's own (`last-12-months`, `this-month` for balances and budgets) |
| `compare` | no | `none`, `previous_period`, `same_last_year` (only where the schema allows it) |
| `categories` | no | Up to 12 category ids as `list_categories` gives them; needs Categories › Read |
| `accounts` | no | Up to 20 account ids as `list_accounts` gives them; needs Accounts › Read |
| `in_currency` | no | A currency code the book uses; only `account_balances` and `net_worth_over_time` |

The values go through `Component::new` and `Component::set` and the finished document through `Document::read`, exactly as a hand-made view, so the schema alone decides what fits. One call prepares one view; one answer holds at most 3 (as M6a.9).

### 4.2 Malformed calls (D-353; M6c.8, suite in M6c.10)

A call from a model is untrusted input (T-21, T-46). Checked in this order, before anything is built; the first failure answers:

1. The argument text is at most **4 KiB** (`ai-tools.call-too-big`), valid JSON, an object, nested at most 3 levels.
2. **No key twice** in one object, compared after decoding escapes (`ai-tools.repeated-key`), as `Document::read` does (D-278).
3. **No unknown key** (`ai-tools.unknown-argument`); the model's key is not repeated back to it. `null` for an optional argument counts as left out (small models send it often).
4. Each value has its type and is in its list (`ai-tools.bad-argument` with the argument's fixed name). The answer to the model lists the allowed values from the schema and never repeats the model's own text.
5. Ids the grid does not let it read are `ai-tools.not-allowed`; ids not in the book `ai-tools.unknown-category` or `ai-tools.unknown-account`.
6. The schema's own checks (a pie of a monthly dataset, a grouping on a figure): `ai-tools.bad-argument` naming the argument.

The title is shown as plain text, labelled AI-made, and run through the number check (D-188), so an amount in it that code did not compute is marked like one in a chat answer. In one chat turn the model may correct a refused call **twice**; after that the chat says plainly that it couldn't prepare the chart and offers the Statistic Builder (D-338) with what it understood.

### 4.3 What the model sees (D-354)

The function's description and enums: the dataset ids with their catalog descriptions, measures, groupings and chart kinds; nothing computed. `for_ai` becomes true for `account_balances`, `net_worth_over_time`, `cash_flow_by_month` and `budget_utilization`: naming one sends no figure, the preview is drawn by code. Category and account ids come only through the read functions and the grid (D-252). My groups, the hide list and `category_over_time` stay out (D-306, Q-268 = A). The model never receives a preview's figures to "check".

### 4.4 Describe a chart in plain words (D-355; M6c.9)

In the chat panel, *Describe a chart* (a one-press entry next to the existing ones) puts the hint *e.g. My spending on food each month this year as bars* into the field. The answer shows the prepared chart **as a preview card in the chat**: the real chart from the real figures (`ComponentPreview`, as the review page), the AI-made label, and *Keep*, *Change…* and *Leave out*. *Keep* is the confirm (`Confirmed::by_the_user`) and saves the view under Statistics as one AI-made history entry; *Change…* opens the Statistic Builder on the prepared view, and what the user changes there is the user's version (D-297); *Leave out* drops it. Putting the kept view on the Overview stays the user's own step (*Add to the Overview*, D-326); the AI does not edit layouts in M6c.

### 4.5 Tests (D-356; M6c.10)

- **Fake model** (`KASSENSTURZ_FAKE_AI=1`): a fixed list of plain descriptions, each answered with a fixed call, covering every dataset, `auto`, a range, a comparison, ids, `in_currency` and one refused call it then corrects.
- **Malformed-call suite:** one table-driven test with at least 40 hostile or broken calls (each check of §4.2, giant strings, deep nesting, every enum misspelt, ids of another kind, titles with control characters or direction marks, a prompt-injection title), each with the error code it must give, run in CI with no network.
- **One small local model:** a script under `tools/` run by hand, never in CI, on the Space Colony book: ten fixed descriptions, the share of calls accepted at the first try and after corrections, recorded in the AI questionnaire (`ai-tests/`, the project's shared folder). It is opt-in, uses only a model on the same computer, and sends no book data anywhere else.
- **Seams:** the tests read the real catalog, schema and datasets of M6c.1, M6c.4 and M6c.6, never a hand-built copy, and expectations come from printed output.

## 5. The debug-log event per action handler (D-359; M6c.D)

Every button, menu item and shortcut handler in QML emits a fixed event name to the debug log (terminal only, `KASSENSTURZ_DEBUG_LOG=1`, D-227), through one helper, and a check under `tools/` fails when a handler has none; this catches the silent *does nothing* bugs of M6b (M6-B9, B13, B14). The events carry the handler's fixed name only, never book data. Making *every action handler logs* a rule for all developers is a change to the product's agent rules, a human gate: M6c.D proposes it in its pull request and the maintainer decides.

## 6. Shared QML components (the M6a retro rule)

When two side-by-side steps both add QML components, both briefs carry this list.

- **Planned, with their owner:** M6c.2 adds no chart type of its own (net worth is a line of `ChartView`); it changes `ChartView`/`KpiCard` only through their models and adds `CashFlowTable` only if the shared `DataTable` can't show start and end columns. M6c.5 `BudgetsPage`, `BudgetSheet`, `BudgetBars` (the `budget.progress` component). M6c.7 `FilterBar`, `FilterPicker`. M6c.9 `AiChartCard` (inside the chat). M6c.11 changes `AiChangeReviewPage` and `AiPermissionGrid` only.

## 7. Risks and their rows in the threat model

New rows T-46 to T-50 in `docs/THREAT_MODEL.md`: a hostile `propose_view` call, budgets that show a wrong or misleading figure, a filter that hides money without saying so, a converted total with a guessed rate, and AI-prepared budgets that slip past the user. T-39 now names M6c.8 to M6c.10.

## 8. The M6c steps

The table is in `docs/ROADMAP.md` ("M6 steps", rows M6c.\*), with needs, tier, effort and which steps run side by side.
