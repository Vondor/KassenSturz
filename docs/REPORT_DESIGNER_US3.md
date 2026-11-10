# Per-bar figures: comparison bars and your own figures (US3.1)

Design of User Sprint 3 item US3.1, written by thread A in Phase 2 before its build (D-687; its row stays apart from D-685's so parallel pull requests don't touch neighbouring lines, D-577). It answers two wishes from US1 Phase 3 with one idea, **each bar can carry its own figure**:

- **#587:** on a spending chart by category, each category's figure next to a comparison (the period before, the same period last year, or the average of 3, 6 or 12 periods before), as two bars per category. It is the same in the app and in `propose_view`.
- **#562:** a bar chart whose bars are figures the user writes, each as a formula. For example, last month's salary next to this month's money out.

They are designed together because both put more than one figure source into one bar chart. Two separate "per bar" settings would confuse the user. The design keeps them as **two settings on two kinds of data**, so a chart never has both: a *comparison* belongs to spending by category, and *your own figures* is a dataset of its own whose bars are formulas.

Release stage: **Alpha** (D-274). There is no book layout change and no document version change. A new choice word, a new dataset and one new property of the existing `views` documents need no migration in Alpha (D-045). An older build refuses a chart that uses them as it refuses any unknown word. The book format is not touched, so this is not a **[format]** item.

**What stays true (D-342):** the app is complete without either. A chart has no comparison until the user picks one, and *Your own figures* appears only when the user adds it from the Add menu. **Numbers come from code (AGENTS.md §8):** every figure is worked out by `kassensturz-report`. Amounts reach QML only as text (D-065).

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

- **Bars:** two series. Series 0 is `spending` (this period). Series 1 is the new key `comparison`, with the second palette colour and its second mark, so colour is never the only difference (M3). The axis, the legend and the orientation rules (D-679: rows when names are long) work for two series as they do for income and expenses today. The legend names series 1 with the word from the table in 1.1.
- **Tooltips and screen readers:** each bar speaks its category, its side (this period or the comparison's name) and its amount.
- **Table:** the columns are *Category*, *Amount* (or *Per month* with an average), the comparison's name, and *Change*. *Change* is the amount and the percent of the comparison, written like a card's change and empty when the comparison is zero (`Change::new`).
- **The properties panel:** a *Compare with* choice under *Average*, only for bar and table, with the five choices of 1.1. *None* is first.

### 1.5 Where it lives in code (for the build)

- `kassensturz-report`: `schema.rs` adds `COMPARE_SPENDING` (`params.compare`, the words of 1.1) to `BAR_CHART` and `TABLE`. A component check refuses it unless the dataset is `spending_by_category` and the chart is a bar or a table. `Compare` grows `Average(n)` with `average-3`, `average-6`, `average-12`, and `Compare::earlier` stays for the cards' two words. A new `spending_compared(source, period, filter, level, shown, compare, average)` gives the rows with both figures. It is tested against two calls of `spending_by_category_shown` (tests where steps meet).
- `kassensturz-charts`: `spending_by_category_compared` builds the two-series model and the four-column table.
- `kassensturz-ui-bridge`: `charts.rs` reads `params.compare` into the request and reads the source over the whole span. The notes of 1.3 are added as `NoteView`s, read back as QML reads them. `ChartView.qml` names series 1, and `PropertiesPanel` shows the choice. New strings are translated (`po/de`).
- AI: in `propose_view`, `compare` is allowed for show_as `bar` and `table` with data `spending_by_category`, and the `average-N` words only there (`what_suits` lists the words per show_as). `list_views` reports it. The answer has no `computed`: a chart of many figures is read with the read functions (D-685).

## 2. Your own figures: one bar per formula (D-687 part 2; #562)

### 2.1 The dataset and the setting

- A new catalog dataset **`own_figures`**, *"Your own figures: one bar per figure you write as a formula"*. It suits bar and table. It has no grouping and no key figure. It takes the accounts filter as a formula card does: the accounts narrow what every name counts. Categories are named in the formulas themselves, so the component's categories list doesn't apply, as on cards (D-650).
- A new property **`params.bars`** (`Kind::Bars`): a list of **1 to 8 bars**, each `{"label": text, "formula": text}`. A label is plain text of at most 60 characters. A formula is in the stored form of D-292, at most `MAX_FORMULA_CHARS`.
- **Each bar's period:** the chart's period, so a bar without `@` counts the chart's period. **A bar's own period is the formula's `@`** (D-649), for example `=Salary@"last month"`. No new period setting is needed, and the example of #562 is two bars: *Salary last month* `=Salary@"last month"` and *Money out this month* `=Spending.ALL`.

### 2.2 The figures

- Each bar is `Formula::evaluate` over the chart's period with the chart's accounts filter. These are the functions a formula card uses, so the same formula shows the same figure on a card and on a bar.
- **One kind per chart:** the first bar that works decides the kind (an amount in one currency, a percent or a number). A later bar of another kind, or an amount in another currency, is left out of the drawing with a note: *"Bar 3 is a percent, the others are amounts, so it is left out."* The table still lists it with its own figure.
- **A bar that can't be worked out** (a division by zero, an unknown name after a category was removed) is left out with a note naming the bar and the error in words, as the formula card says it (D-650). The others still draw.
- **No total** under the chart: the bars are different things, so a sum would mean nothing.

### 2.3 Drawing and editing

- **Bars:** one series, key `own`, one bar per entry in the order of the list, each named by its label (plain text, the user's own). The orientation rules of D-679 apply, so long labels give rows.
- **Table:** *Bar*, *Figure*, and *Counts* (the categories and periods the formula counted, in the words `FormulaWords.qml` uses).
- **The properties panel:** a *Bars* list. Each row has a name field and the existing formula field (with its suggestions and its mistake shown in words, M6b), plus buttons to move the row up or down and to remove it, and *Add a bar* up to 8. A new chart starts with one empty bar and the hint *"Write a formula for each bar, e.g. =Groceries or =Salary@"last month"."*
- **The Add menu:** *Your own figures* under the charts.

### 2.4 Where it lives in code (for the build)

- `kassensturz-report`: `catalog.rs` adds `own_figures` (with `for_ai: true`). `schema.rs` adds `BARS` and `Kind::Bars` with its checks (1 to 8 entries, the label rules of `TITLE`, each formula parsed in stored form) to `BAR_CHART` and `TABLE`. A component refuses `bars` on another dataset and needs it on `own_figures`. `own_figures(source, period, filter, bars, currency)` gives one result per bar (`Ok` value or the error).
- `kassensturz-charts`: `own_figures` builds the model and the table, with the notes of 2.2.
- `kassensturz-ui-bridge` and QML: the request, the notes, the *Bars* editor, and the strings (`po/de`).
- AI: `propose_view` takes `bars`, a list of 1 to 8 objects `{label, formula}`, only with data `own_figures` and show_as `bar` or `table`. It is offered only with Categories › Read, as `formula` is (D-650). Each formula is checked by `check_formula`, and a mistake names `bars` and the bar's place: `ai-tools.bad-formula` with `argument: "bars"`, `item` and `position`. `MAX_VIEW_CALL_BYTES` rises from 8 KiB to 16 KiB, so eight formulas of the longest length still fit. The answer's `computed` (D-685) gives each bar's figure with its place and label index, under the same permission rule. `list_views` reports the bars.

## 3. Build order and the conflict map

US3.1's build is **two PRs on thread A, in this order**, because both touch the same files:

1. **US3.1a, comparison bars (section 1).** Files: `report/src/schema.rs` (its own entries only, US3 conflict map), `report/src/metrics.rs`, `report/src/spending.rs`, `charts/src/datasets.rs`, `ui-bridge/src/charts.rs`, `ChartView.qml`, `PropertiesPanel`, `ai-tools/src/changes/view_call.rs`, `po/de`.
2. **US3.1b, your own figures (section 2)**, after 1a merges. Files: `report/src/catalog.rs`, `schema.rs`, a new `report/src/own_figures.rs`, `charts/src/datasets.rs`, the bridge, a new `BarsEditor.qml`, `view_call.rs`, `po/de`.

Thread B (budgets) also edits `schema.rs` with its own entries. A's build merges first, as the conflict map says. Each PR adds only its own German entries and its own roadmap line (D-577).

**Tests** (AGENTS.md §9.4): the report figures of both sides against `spending_by_category_shown` for each period, and the average of N against the exact sum. Notes are tested on a synthetic book whose first transaction is late. Each bar is checked against a formula card with the same formula. The bridge maps are read back as QML reads them. The AI calls cover the refusals per show_as and the oversized call.

## 4. Choices made here, open to the maintainer

These are the safest reversible defaults (AGENTS.md §10). Each can change before or during the build without touching stored books:

1. **An average compares with periods as long as the chart's own,** not always with months. A year chart then compares with the average year. *Why:* it is the rule `Period::before` already follows for cards, so one word means one thing everywhere.
2. **Months before the first booking count as zero, with a note** (1.3), as D-668 does. *Why:* figures never change behind the user's back.
3. **Comparison and own figures are two settings on two datasets,** never both in one chart. *Why:* a chart then has one plain-words meaning, and the everyday-user test (§6) stays easy.
4. **At most 8 own bars.** *Why:* that is what one chart can label readably at the default size, and what the AI's call can hold.
