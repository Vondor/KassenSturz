# The report designer: design for M6

Written in M6.0 (Sprint M6a, Phase 0). It holds what the M6 steps build on: the chart library check, the reporting layer, the dashboard document and its table, the Vega-Lite subset for custom charts, and the risks. Decisions are D-275 to D-284 in `docs/DECISIONS.md`; they are **Proposed** until the maintainer approves them. Release stage: Alpha (no migrations, the book format may change; the dashboard document still has a version field from day one).

Nothing here changes how money is stored or computed. Charts show numbers that Rust computed exactly (D-065); the only floating-point numbers are the drawing coordinates a chart needs, made from exact values and never read back (AGENTS.md §5).

## 1. Chart library check (D-275)

**Question:** which library draws the charts behind the `ChartProvider` interface (D-008, D-071)? Checked in a scratch container of the Qt image (Ubuntu 26.10: Qt 6.11.2, KDE Frameworks 6.29) with one scene per library. Licenses come from the packages' copyright files (Debian's, which list the files' headers) and the upstream sources named below; nothing was copied from them.

| | KQuickCharts (KDE Frameworks) | Qt Charts | Qt Graphs (2D) | Own QML Canvas layer |
|---|---|---|---|---|
| License | `LGPL-3.0-or-later` with the KDE e.V. option (the copyright file says `LGPL-3+KDEeV`); `src/shaders/sdf.glsl` MIT; tests BSD-2-Clause | `GPL-3.0-or-later` (Qt GPL module) | `GPL-3.0-only` (Qt GPL module) | ours, `GPL-3.0-or-later` |
| With GPL-3.0-or-later | Yes, as a dynamically loaded QML plugin; keeps "or later" | Yes, but the whole build is then GPL-3.0 only (D-001) | Same | Yes |
| Chart types | `LineChart` (also filled: area; `stacked`), `BarChart` (`stacked`), `PieChart` (`thickness` gives a donut) | line, bar, pie, area, and more | line, bar, pie, and more (QML `GraphsView`) | whatever we draw |
| Axes, grid, labels | **None built in**: a chart item draws the data only; `Legend`, `LegendDelegate`, `LineChartControl` and `PieChartControl` in `org.kde.quickcharts.controls`. We draw axis labels and grid lines ourselves | built in | built in (rendered in the scene: axes, grid, labels) | ours |
| Rendering | GPU, signed distance fields. **Needs an OpenGL/RHI backend**: under Qt's `offscreen` platform with the software backend the chart items stay **blank** (our usual screenshot way); under Xvfb with Mesa llvmpipe (`LIBGL_ALWAYS_SOFTWARE=1 QSG_RHI_BACKEND=opengl`) they draw | needs `QApplication` (QGraphicsView based); in the scratch run under `qmltestrunner` (a `QGuiApplication`) it crashed with a segmentation fault, not investigated; deprecated since Qt 6.10, Qt's docs point new projects to Qt Graphs (doc.qt.io, Qt Charts index) | rendered fine in the same scratch run | CPU or GPU, our choice |
| Accessibility | none built in: we add `Accessible` roles to the container and a text table for each chart (D-283) | same | same | ours |
| Maintenance | KDE Frameworks, released every month; in KDE's Flatpak runtime (below) | Qt, but deprecated | Qt, newer | us |
| Fit with goals | KDE-official stack (D-041), LGPL | GPL-3-only | GPL-3-only | most work |

**Recommendation (D-275):** use **KQuickCharts**, behind `ChartProvider`, as a QML module the app imports at run time. It is the only candidate that keeps GPL-3.0-or-later whole and is part of the KDE stack that we want to join (D-041). Its missing axes are not a loss: every axis label and number on a chart is text that Rust formats (D-065), so we draw the labels from our own text anyway. Qt Graphs stays the fallback if KQuickCharts turns out to be unfit, and Qt Charts is not used (deprecated). Both are already covered by the approval of Qt 6 (D-061), so a fallback needs no new approval.

**Build proof.**
- *KDE Flatpak runtime 6.11 (the app's runtime and CI's SDK):* KQuickCharts is a QML plugin, not something the app links against, so the build needs nothing; the app only needs the module at run time. This PR adds one CI step to the existing "Rust format, lints and tests (KDE SDK)" job that looks for `org/kde/quickcharts/qmldir` in the SDK and prints its version, and fails if it is missing (the runtime ships the SDK's QML modules too). The result of that step is the proof; if it fails, the fallback below applies and M6a.3 starts by vendoring.
- *CMake:* nothing to do for the same reason; M6a.3 adds an `import org.kde.quickcharts` smoke test to the Qt test container.
- *Windows:* **not proven here.** The Windows CI job builds no Qt (M7 does, D-214), and KDE's metadata lists Windows among KQuickCharts' platforms, but nothing was built. M7's Windows step must build or bundle it (KF6 libraries build on Windows with CMake); if that fails, the fallback applies there. Risk: low impact, because the chart code sits behind `ChartProvider`.
- *Fallback:* a small own QML `Canvas`/`Shape` layer for line, bar, area and pie. About 600 lines of QML; decided at M6a.3 only if the check above fails.

**What the dependency means for tests:** screenshots and QML tests of charts need Xvfb with Mesa (`xvfb`, `libgl1-mesa-dri` in the Qt container image); the `offscreen` platform draws them blank. The Qt container for M6 gets those two packages in M6a.3. Data-level tests (what a chart is given) need no GPU.

**Dependency approval (human gate, AGENTS.md §3.1 rule 6, §3.2):** KQuickCharts, license verified above, maintained (KDE Frameworks release 6.29 in the scratch image; `master` needs Qt 6.10 and KF 6.32 according to its `CMakeLists.txt`). Recorded in `docs/THIRD_PARTY.md` as **proposed**; the maintainer decides (Q-214).

## 2. The `ChartProvider` boundary (D-276)

A chart is described by plain data the provider gets and draws:

- a **chart model** built in Rust from a dataset: series with their names, colors (from the palette, see D-283), and points. Each point has a **label as text** (already formatted for the user's locale, exact), and a **drawing value** (`f64`, converted from the exact minor units only inside the adapter, refused when it would lose exactness above 2^53) used only to place the mark;
- axes as text: tick labels, titles, the unit; the provider draws no number it formatted itself;
- a **text alternative**: the same series as a table (period or category, value as text), which is also what the screen reader reads;
- interactions return indices (the series and point under the pointer), never values; the UI asks the model for that point's text.

The chart model types are headless, in `kassensturz-charts` (no Qt). `kassensturz-ui-bridge` holds the QML-facing models and a read-back test for each object (AGENTS.md §9.4); the QML picks the provider. Nothing in `kassensturz-report` knows about Qt.

## 3. The reporting layer, `kassensturz-report` (D-277)

A new headless crate between the book and the components (ARCHITECTURE.md: depends on `kassensturz-core` and read access to the store; no UI, no AI, no network).

**Datasets** are named, versioned, read-only functions of the book and a request `{dataset, period, filters, grouping}`. Catalog of M6a (more in M6c):

| Dataset id | Rows | Used by |
|---|---|---|
| `spending_by_category` | category path, total | pie/donut, bar, table |
| `income_expense_by_month` | month, income, expenses, net | line, stacked bar, KPI |
| `category_over_time` | month, category, total | stacked bar, area |
| `balance_over_time` | date, account (or all), balance | line, area |
| `account_balances` | account, balance | table, KPI |
| `net_worth_over_time` (M6c) | month, assets, liabilities, net | line, KPI |

**Rules, so a wrong figure cannot appear (principle 1):**
- Totals are sums of `Money` in minor units with overflow checks; a total is never made from displayed (rounded) values.
- A dataset has **one currency**. A book with several currencies gives one result per currency; amounts of different currencies are never added together. Showing converted totals needs the recorded rate of each part (D-043) and says which; it is its own later step, not part of M6a.
- Transfers between the user's own accounts are not spending and not income (they have no category); this is stated in each dataset's description in the catalog, and tested against the real Transfer made by the importer and by the transfer detection (tests where steps meet).
- Periods are calendar-date ranges (`jiff::civil::Date`, D-061): this month, last month, last 12 months, this year, last year, a custom range. "Month" means the calendar month.
- **Calculated metrics** (M6a.2): net = income − expenses; savings rate = net ÷ income as a percentage; period comparison = change in amount and in percent. A ratio is the one place a division happens: it uses the exact decimal type (`rust_decimal` behind our own types, D-051) and an explicit rounding rule, round half away from zero to one decimal place, written in the dataset's description. A division by zero gives "no value" and the card says so in words; it never shows 0 % or infinity.
- Amounts, rates and ratios leave the crate as **text** (D-065). Sorting and totals happen here.
- Every dataset has a **description in plain words** (glossary rules, AGENTS.md §6), what it counts and what it leaves out, shown in the properties panel and given to the AI as the only description of it.

**What the AI sees (D-282):** dataset ids, their descriptions, the allowed groupings and chart types as enumerated choices, and category names only when the AI permission grid allows reading categories. Amounts only if the grid allows reading transactions, and then only what the existing read functions give (D-186). A model never receives a dashboard's computed numbers to "check": it proposes, the code computes.

## 4. The dashboard document and layout 20 (D-278, D-279)

**One document type, two kinds.** A *view* is one saved chart or table the user made in the Statistics workbench; a *dashboard* is several components on a grid. Both are JSON documents in one table of the book.

```json
{
  "format": "kassensturz-dashboard",
  "version": 1,
  "kind": "dashboard",
  "name": "Overview",
  "filters": { "period": { "kind": "last_months", "count": 12 } },
  "components": [
    {
      "id": "c1",
      "type": "chart.line",
      "dataset": "income_expense_by_month",
      "params": { "grouping": "month" },
      "props": { "title": "Income and expenses", "legend": true },
      "frame": { "col": 0, "row": 0, "cols": 6, "rows": 4 }
    }
  ]
}
```

- `format` and `version` identify it. Reading refuses a document whose version is newer than the program knows (`report.unsupported-document-version`); older versions are read by the same code (from the first Beta on, version changes are migrations with a backup and a test, `release-stages.md`; during Alpha the book format may change).
- `kind` is `dashboard` or `view`. A view has exactly one component and no `frame`.
- `components[].type` is one of an **enumerated list** (`chart.line`, `chart.bar`, `chart.stacked_bar`, `chart.area`, `chart.pie`, `chart.donut`, `kpi.card`, `progress.bar`, `table.rows`; more with later steps). An unknown type is shown as a placeholder "This component isn't available in this version", the rest of the dashboard still opens.
- `dataset` is an id from the catalog (section 3), never a query or a path. `params` and `props` are checked against the component type's **property schema** (the same schema drives the properties panel, M6a.6, and is what the AI is allowed to fill).
- `frame` is grid cells (the grid editor of M6b adds drag, resize and snapping; M6a uses a fixed layout).
- There are no URLs, no scripts, no expressions, no file paths anywhere in a document.
- Limits, checked before anything is shown: 1 MiB for the document, nesting depth 8, 100 components, 200 characters for a name or title, 50 series per chart, 10,000 rows per dataset result. A document over a limit is refused with `report.document-too-big`; the others are `report.document-invalid` (with the JSON path as text, never as part of a message).
- Everything that is text in a document (name, title, labels, category names that come with the data) is **data**: shown as plain text, never as markup or as an instruction (T-08, T-21).

**Layout 20** (the fifth migration; first M6 layout step, M6a.L, its own small pull request; Alpha: a new book is built by all migrations, an older Alpha book is refused as before):

`views` table, STRICT:

| Column | Type | Meaning |
|---|---|---|
| `id` | BLOB (16 bytes), primary key | UUIDv7 |
| `kind` | TEXT | `view` or `dashboard`; checked |
| `name` | TEXT | Never empty; unique among documents of the same kind, ignoring capitals and spaces around it |
| `position` | INTEGER, from 0 | The order in the lists; the book keeps them numbered 0, 1, 2, … |
| `format_version` | INTEGER, ≥ 1 | Copy of the document's `version`, so a list can be read without parsing |
| `document` | TEXT | The JSON document, valid JSON (`json_valid`), at most 1 MiB |

Plus `changed_views` (like every other table with the change history, D-032, D-066) and the new `changes.action` values `add_view`, `edit_view`, `rename_view`, `move_view`, `remove_view`; `changes` is rebuilt for it as in layouts 17 to 19. Documents are saved with the book, copied with it, backed up with it, and part of the complete export (`docs/EXPORT_FORMAT.md` gets a `views` list in M6a.L). The preset default dashboard is **not** stored: until the user edits it, the Overview is generated from the built-in default document (restore default = delete the stored copy, M6d).

**Undo.** The Statistics workbench and saving a view go through the book's change history (undoable like any other change). The grid editor of M6b keeps its own in-memory command stack while editing, and writes one history entry when the user leaves edit mode.

## 5. Vega-Lite subset for custom charts (D-280, D-281)

For M6e: a user or an AI may describe a chart as a small Vega-Lite specification. We read it with **our own parser** (`serde_json` only) into our own chart model and draw it with the same components. **No Vega or Vega-Lite code is used or shipped**, so no dependency is added.

**License.** Vega-Lite is `BSD-3-Clause` (`package.json` of vega/vega-lite 6.5.0, checked 2026-10-29), which is GPL-3.0-compatible. We implement from the public language description (the specification and its JSON schema, studied, not copied); the subset is therefore our own work, and compatible spellings are used so that a user can paste a simple spec from the Vega-Lite documentation. If we ever copy a file of it (a schema, an example), the license text, origin and copyright go into `docs/THIRD_PARTY.md` first (AGENTS.md §3.1 rule 5).

**The subset (v1):**

| Part | Allowed | Everything else |
|---|---|---|
| `$schema` | absent, or a string that starts with `https://vega.github.io/schema/vega-lite/`; never fetched | refused |
| `data` | `{ "name": "<dataset id>" }`, one of the catalog | `url`, `values`, `sequence`, `format`: refused ("not supported here": data comes only from your book) |
| `mark` | `line`, `bar`, `area`, `arc`, `point`, `text`, `rule`; as a string or an object with `type` and `tooltip`, `innerRadius`, `point` | other marks (`boxplot`, `geoshape`, `image`, `trail`, …) |
| `encoding` | channels `x`, `y`, `color`, `size`, `theta`, `text`, `tooltip`; each `field` (a field of the dataset), `type` (`nominal`, `ordinal`, `quantitative`, `temporal`), `title`, `aggregate` (`sum`, `mean`, `min`, `max`, `count`), `timeUnit` (`year`, `yearmonth`, `yearmonthdate`), `sort` (`ascending`, `descending`, or a field name) | other channels, `bin`, `scale` with expressions, `axis.format` (d3 format strings), `condition`, `datum`, `value` as expression |
| `transform` | `filter` as an **object** (`field` with `equal`, `oneOf` or `range`), at most 8 | `filter` as an **expression string**, `calculate`, `lookup`, `window`, `joinaggregate`, `fold`, `pivot`, … (no expression language) |
| composition | `layer` of at most 4 single-view specs; `title`, `description` | `facet`, `repeat`, `concat`, `hconcat`, `vconcat`, `params`, `selection` |
| style | `width`, `height` (numbers within 100 to 2000), `config.range.category` (a color list from the palette names only) | other `config`, `view`, `padding` |

An unsupported key is **never dropped silently**: the import lists each one as plain text with its path ("`transform[1].calculate` isn't supported here") and the chart is not made, or is made from the supported part only if the user chooses that. Spellings and the supported subset are versioned (`"kassensturz": { "vl_subset": 1 }` in our saved form).

**Limits (D-281), checked before rendering:** spec text at most 32 KiB, nesting depth 10, at most 4 layers, 8 encoding fields, 8 filters, 10,000 data rows used, 50 series, labels at most 200 characters. Hostile specs (deep nesting, huge arrays, an `url` or an expression string, invalid UTF-8, repeated keys) are refused with `report.spec-too-big` or `report.spec-unsupported` and cost bounded time and memory; tests (M6e) use fuzz-style cases. All text in a spec is shown as plain text. The same checks apply to a spec the AI proposed.

## 6. AI and the designer (D-282)

M6a.9 adds the first, small tool and M6c the full one. Both are the same shape as the existing change sets (D-047, D-199):

- a single function `propose_view` with **enumerated** arguments: dataset id, measure, grouping, period, comparison, chart type (`auto` allowed); no free text except the title (200 characters, plain text);
- the proposal is validated against the same property schema as a hand-made component; it is shown as a **preview** (the real chart, built from the real exact numbers by our code) and saved as a view only when the user confirms (`Confirmed::by_the_user`);
- numbers on the preview come from `kassensturz-report`; the model never supplies a figure (§8 "Numbers come from code");
- with AI switched off, nothing of this is visible or reachable (D-013); the tool is off by default and has its own cell in the permission grid when it is added (D-252); labelled as AI-made in the history.

## 7. Accessibility and colors (D-283)

- Every chart has a **text alternative**: a table of the same values, opened by a "Show as table" control, and an accessible name and description built from the title and the chart's summary (largest category, trend). The table is the screen-reader path; the drawing is marked decorative.
- Keyboard: a chart is a focus stop; arrow keys move between points and the text of the focused point is spoken; no information is only in color or only on hover.
- The default palette uses a colour-blind-safe set (8 colours), enough contrast against light and dark, and a different mark or pattern for the first four series; the user's system theme is followed.
- Text scales with the user's font size; nothing is a fixed-size picture.

## 8. Risks and their rows in the threat model

New rows T-37 to T-40 in `docs/THREAT_MODEL.md`: hostile dashboard or view documents, hostile Vega-Lite specs, AI-made views, and the KQuickCharts dependency (supply chain). Planned error codes (they go into `docs/ERROR_CODES.md` with the code that raises them, as a test there compares the file with the code): `report.unsupported-document-version`, `report.document-invalid`, `report.document-too-big`, `report.unknown-dataset`, `report.unknown-component`, `report.spec-unsupported`, `report.spec-too-big`, `report.no-value` (a ratio with no income), `store.view-name-taken`, `store.unknown-view`.

## 9. The M6 steps

The final table is in `docs/ROADMAP.md` (row "M6 steps"). The Sprints: M6a = M6.0 + the reporting layer, built-in charts, properties panel, Statistics workbench, Overview rebuilt from components, first AI chart; M6b the grid editor; M6c financial components, budgets, shared filters and the AI chart builder; M6d several dashboards, templates, import and export, KMyMoney report import; M6e Vega-Lite custom charts (D-284).

## 10. Questions for the maintainer (through the coordinator)

- **Q-214** Approve KQuickCharts as a dependency (D-275)? Recommended: yes (R).
- **Q-215** What does the default Overview contain? Proposal: balance and net worth KPI cards, income and expenses of the last 12 months (line), spending by category this month (donut), the five largest categories (table).
- **Q-216** Budget model for M6c? Proposal: a monthly limit per category (D-029), shown as a progress bar (R).
