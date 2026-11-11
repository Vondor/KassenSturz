# Per-bar figures: comparison bars and a formula per bar (US3.1)

Design of User Sprint 3 item US3.1, written by thread A in Phase 2 before its build (D-687; its row stays apart from D-685's so parallel pull requests don't touch neighbouring lines, D-577). It answers two wishes from US1 Phase 3 with one idea, **each bar can carry its own figure**:

- **#587:** on a spending chart by category, each category's figure next to a comparison (the period before, the same period last year, or the average of 3, 6 or 12 periods before), as two bars per category. It is the same in the app and in `propose_view`.
- **#562:** a bar chart whose bars are figures the user writes, each as a formula. For example, last month's salary next to this month's money out.

They are designed together because both put more than one figure source into one bar chart. Two separate "per bar" settings would confuse the user. The design keeps them as **two settings that never meet in one chart**: a *comparison* belongs to the rows of spending by category, and a *formula per bar* is a column of the columns the app already has (D-339, D-341).

Release stage: **Alpha** (D-274). There is no book layout change and no document version change. New choice words of an existing property of the `views` documents need no migration in Alpha (D-045). An older build refuses a chart that uses them as it refuses any unknown word. The book format is not touched, so this is not a **[format]** item.

**What stays true (D-342):** the app is complete without either. A chart has no comparison until the user picks one, and columns stay where the Statistic Builder already has them. **Numbers come from code (AGENTS.md §8):** every figure is worked out by `kassensturz-report`. Amounts reach QML only as text (D-065).

## 1. Comparison bars on spending by category (D-687 part 1; #587)

### 1.1 The setting

`params.compare` (the property number cards have since M6a.5, D-285) is added to **bar charts and tables of `spending_by_category`**, for every grouping (each category, top-level category, My groups). Its words:

| Word | The comparison figure of a category | Name in the legend and the table |
|---|---|---|
| `none` (default) | none: the chart is as today | — |
| `previous_period` | its figure in the period right before (`Period::before`: the same number of months, the salary month before, or the same number of days) | *Period before* |
| `same_last_year` | its figure in the same days a year earlier (`Period::year_before`) | *Same period last year* |
| `average-3`, `average-6`, `average-12` | the average of its figures in the 3, 6 or 12 periods right before, each as long as the chart's period (`before` taken N times) | *Average of the 3 periods before* (6, 12) |

- **Not on** pies, donuts and stacked bars: a stacked bar of spending is one bar split into its categories (D-666), so there is no place for a second bar. The schema doesn't offer it there. The AI's call is refused with `only_for_show_as: ["bar", "table"]` (D-353).
- **Number cards and progress bars** keep their two words. `average-N` on a card is a possible later wish, not part of US3.1.
- **Period limit:** the chart's period and its comparison periods together are at most `MAX_MONTHS` (1,200 months). A 12-month chart with `average-12` reads 13 years. A longer span is refused like any period that is too long (`report.period-too-long`, existing code).

### 1.2 The figures

- The comparison is counted **exactly as the chart's own figure**: the same accounts and categories filter, hidden categories, "money not sorted yet" line, refunds and currency rules. It is also counted with the same `average` (D-648). If the chart shows each category per month, the comparison is per month too.
- **An average of N periods** adds the N periods' exact amounts and divides **once** with the book's rounding mode (D-027). The rounded figures are never added up, as `Average::of` does today.
- **Which rows:** the categories the chart shows for its own period, in their order (largest first), then any category that had spending only in the comparison, with 0 for this period, ordered by its comparison. So a category you stopped spending on still shows. Groups and "only these" lists apply to both sides the same way.
- **One result per currency**, as today: each currency's chart compares within its own currency.
- **The total** under the chart stays the period's own total. The table gets the comparison total in its last row too.

### 1.3 Months before the first booking (the question of #587)

The same approach as D-668: **the figures stay, and the chart says so.** Today a chart average divides by every month of the period (D-648), and D-668 added a note instead of changing that.

- An average of N periods divides by N. When some of the N periods end before the first transaction that the chart's filter keeps (found as `counted_early` finds it), they count as zero. A grey note under the chart then says: *"3 of the 6 periods this comparison averages are before the first transaction and count as zero."* (`compare-early`, with `counted` and `before`).
- For `previous_period` and `same_last_year`, a comparison period that ends before that first transaction gets *"The period compared is before the first transaction."* That way an empty comparison bar is not read as "spent nothing".
- *Hide the grey filter notes* (D-663) hides these like the other grey notes.
- **Not chosen:** starting the average at the first booking. That would change the figures without the user seeing why, and it would differ from D-648 and the budgets' `based_on` (D-395).

### 1.4 Drawing

- **Bars:** two series. Series 0 is `spending` (this period). Series 1 is the comparison, keyed by its word (`previous_period`, `same_last_year`, `average_3`, `average_6`, `average_12`, as built in US3.1a, so the screen names it without a second field), with the second palette colour and its second mark, so colour is never the only difference (M3). The axis, the legend and the orientation rules (D-679: rows when names are long) work for two series as they do for income and expenses today. The legend names series 1 with the word from the table in 1.1.
- **Tooltips and screen readers:** each bar speaks its category, its side (this period or the comparison's name) and its amount.
- **Table:** the columns are *Category*, *Amount* (or *Per month* with an average), the comparison's name, *Change* and *Change in %* (`Change::new`; the percent is empty when the comparison is zero). As built, the change is two columns, so no sentence has to join an amount and a percent in each language, and the table has no total row: the total under the chart stays the period's own.
- **The properties panel:** a *Compare with* choice under *Average*, only for bar and table of spending and not for a chart made of columns, with the six choices of 1.1. *None* is first.

### 1.5 Where it lives in code (for the build)

- `kassensturz-report`: `schema.rs` adds `COMPARE_SPENDING` (`params.compare`, the words of 1.1) to `BAR_CHART` and `TABLE`. Other datasets get `none` alone (`Property::words`, as `average`), and stacked bars have their own list without it. As built, a new `compared.rs` holds `CompareWith` (`Earlier(Compare)` or `Average(n)`), so `Compare` stays the cards' two words; `CompareWith::divisor` gives what the comparison's exact sum is divided by once. `spending_compared(source, period, filter, level, shown, compare)` gives the rows with both exact figures, and `compared_early` the note of 1.3. `Asked` carries the comparison to the bridge's `Scope`, as it does `average`. It is tested against two calls of `spending_by_category_shown` (tests where steps meet).
- `kassensturz-charts`: `spending_by_category_compared` builds the two-series model and the five-column table.
- `kassensturz-ui-bridge`: `charts.rs` reads `params.compare` into the request and reads the source over the whole span. The notes of 1.3 are added as `NoteView`s, read back as QML reads them. `ChartView.qml` names series 1, and `PropertiesPanel` shows the choice. New strings are translated (`po/de`).
- AI: in `propose_view`, `compare` is allowed for show_as `bar` and `table` with data `spending_by_category`, and the `average-N` words only there (`what_suits` lists the words per show_as). `list_views` reports it. The answer has no `computed`: a chart of many figures is read with the read functions (D-685).

## 2. A formula per bar: the AI gets the columns the app already has (D-687 part 2; #562)

**Corrected after the design merged (#667):** the app already draws a bar per formula. A bar chart or table can be made of named columns (`params.columns`, D-339), and since D-341 a column can be a formula whose `@` gives it its own period. So the example of #562 already works in the Statistic Builder today, as two columns *Salary last month* `=Salary@"last month"` and *Money out this month* `=Spending.ALL`. A new dataset `own_figures` would have been a second way to do the same thing, so it is dropped. What is missing is that the AI can't prepare columns, and that the way to a formula column is hard to find.

### 2.1 The AI prepares columns

- `propose_view` takes **`columns`**: a list of 1 to 12 objects (`MAX_COLUMNS`). Each is either `{"name": text, "formula": text}` or `{"name": text, "categories": [IDs]}`, never both. Names must not be empty and must differ ignoring capitals, as the board demands (D-339).
- It is allowed for show_as bar, stacked_bar, line, area and table with data `spending_by_category` (the datasets and kinds that take `params.columns` today; the build reads the schema for the exact list). When columns are given, they are the bars, as in the app.
- It is **offered only with Categories › Read**, as `formula` is (D-650). Each formula is checked by `check_formula`. A mistake names `columns` and the column's place: `ai-tools.bad-formula` with `argument: "columns"`, `item` and `position`. A formula that gives a percent or a plain number is refused as the app refuses it (`report.column-formula-not-amount`, as a bad-formula reason `kinds`).
- `MAX_VIEW_CALL_BYTES` rises from 8 KiB to 16 KiB, so 12 columns with formulas still fit.
- `compare` (section 1) and `columns` don't go together: the comparison is for the dataset's own rows. A call with both is refused as `bad-argument compare`, and the schema refuses it in the app too.
- `list_views` reports a chart's columns (names, and formulas or members only as the grid lets the model read them, D-681). The answer of `propose_view` has no `computed`, because a chart of many figures is read with the read functions (D-685).

### 2.2 Easier to find in the app

- The Statistic Builder's board already turns an empty column into a formula column (D-341). The build adds one line of help under *Add a column*: *"A column can also be a formula, e.g. =Salary@"last month", with its own period."* This adds no new control.

### 2.3 Where it lives in code (for the build)

- `ai-tools/src/changes/view_call.rs` (the argument, its schema and checks), `changes/views.rs` (the item key `columns`, set with `Setting::Columns`), `list_views`, the Statistic Builder's board QML (the help line), `po/de`. No change to `kassensturz-report` or `kassensturz-charts`.

## 3. Build order and the conflict map

US3.1's build is **two PRs on thread A, in this order**, because both touch the same files:

1. **US3.1a, comparison bars (section 1).** Files: `report/src/schema.rs` (its own entries only, US3 conflict map), `report/src/metrics.rs`, `report/src/spending.rs`, `charts/src/datasets.rs`, `ui-bridge/src/charts.rs`, `ChartView.qml`, `PropertiesPanel`, `ai-tools/src/changes/view_call.rs`, `po/de`.
2. **US3.1b, columns for the AI (section 2)**, after 1a merges. Files: `ai-tools/src/changes/view_call.rs`, `changes/views.rs`, the board QML, `po/de`.

Thread B (budgets) also edits `schema.rs` with its own entries. A's build merges first, as the conflict map says. Each PR adds only its own German entries and its own roadmap line (D-577).

**Tests** (AGENTS.md §9.4): the report figures of both sides against `spending_by_category_shown` for each period, and the average of N against the exact sum. Notes are tested on a synthetic book whose first transaction is late. The AI's columns are checked against the same chart built by hand with `params.columns`. The bridge maps are read back as QML reads them. The AI calls cover the refusals per show_as and the oversized call.

## 4. Choices made here, open to the maintainer

These are the safest reversible defaults (AGENTS.md §10). Each can change before or during the build without touching stored books:

1. **An average compares with periods as long as the chart's own,** not always with months. A year chart then compares with the average year. *Why:* it is the rule `Period::before` already follows for cards, so one word means one thing everywhere.
2. **Months before the first booking count as zero, with a note** (1.3), as D-668 does. *Why:* figures never change behind the user's back.
3. **A comparison and columns are never in one chart.** *Why:* a chart then has one plain-words meaning, and the everyday-user test (§6) stays easy.
4. **#562 reuses the columns of D-339 and D-341,** with no new dataset. *Why:* one way to make a bar per formula, already tested, instead of two.
