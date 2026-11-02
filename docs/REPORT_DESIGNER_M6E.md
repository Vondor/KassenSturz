# The report designer, part 5: custom charts from a Vega-Lite subset (M6e)

Design of the M6e Sprint, written in step M6e.0. It is a **delta** to `docs/REPORT_DESIGNER.md` section 5 (the subset, D-280, and the limits, D-281). Those two are not reopened here. This file adds what the build steps need: the chart model's Rust shape, the fields a spec can name, where a custom chart lives in the dashboard document, how a pasted spec and an opened file reach the parser, the error codes and the steps. Decisions D-502 to D-509. The maintainer's answers applied here: Q-353 = B, so all seven datasets of the catalog can be named, with the choices a spec cannot carry made in the properties panel (section 2); and Q-352 = A: in M6e a spec comes only from the user, pasted or opened from a file, and the AI does not write specs. The AI keeps the listed `propose_view` choices (D-282), so the AI surface and the permission grid stay as they are. Q-357 = A: the formula separator setting stays in the backlog, and M6e is custom charts only. D-510 records that formulas follow LibreOffice Calc's behaviour, which is outside this design. The steps are in `docs/ROADMAP.md` ("M6 steps", rows M6e.\*).

Release stage: **Alpha, last part** (D-274). **No new layout and no new document version:** a custom chart is one more component type in the existing `views` documents (section 3). M6e.M, the last step of Phase 2, then merges the development migrations into one starting layout (D-045).

**What stays true (D-342):** the app is complete without custom charts. Nothing in this design appears on a page unless the user adds a custom chart from the Add menu.

Words: a **spec** is the JSON text that describes one chart in the Vega-Lite language. The UI calls it a *chart description* (section 7).

## 1. The chart model (D-502; M6e.1a)

The model lives in the new module `kassensturz_report::spec`, in `crates/kassensturz-report/src/spec/`. M6e.1a writes these types with their constructors and unit tests, and nothing that reads JSON. The parser (M6e.1) is the only thing that builds a `ChartSpec` from text, and the renderer (M6e.2) is the only thing that reads one against the book. All fields are private and are read through accessors, so a value that breaks a rule below cannot be built.

```rust
// spec/model.rs
/// The version of the subset this program reads (`"kassensturz": {"vl_subset": 1}`, D-280).
pub const VL_SUBSET: u32 = 1;

pub struct ChartSpec {
    dataset: &'static SpecDataset,      // from `data.name`, section 2
    title: Option<SpecText>,            // `title`, a string only
    description: Option<SpecText>,      // `description`
    size: Option<ChartSize>,            // `width`, `height`
    palette: Vec<PaletteName>,          // `config.range.category`, at most 8
    filters: Vec<FieldFilter>,          // top-level `transform`, at most 8 together with the layers'
    layers: Vec<Layer>,                 // 1 (no `layer`) to 4 (D-281)
}

pub struct Layer {
    mark: Mark,
    encodings: Vec<Encoding>,           // at most 8 over the whole spec, tooltip entries counted one by one
    filters: Vec<FieldFilter>,          // the layer's own `transform`
}

pub struct Mark { kind: MarkKind, tooltip: bool, point: bool, inner_radius: Option<u16> }
pub enum MarkKind { Line, Bar, Area, Arc, Point, Text, Rule }

pub struct Encoding {
    channel: Channel,
    field: &'static SpecField,          // a field of the spec's dataset, section 2
    kind: FieldType,                    // `type`; must suit the field, section 2
    title: Option<SpecText>,
    aggregate: Option<Aggregate>,
    time_unit: Option<TimeUnit>,        // only on a date field
    sort: Option<Sort>,
}
pub enum Channel { X, Y, Color, Size, Theta, Text, Tooltip }
pub enum FieldType { Nominal, Ordinal, Quantitative, Temporal }
pub enum Aggregate { Sum, Mean, Min, Max, Count }
pub enum TimeUnit { Year, YearMonth, YearMonthDate }
pub enum Sort { Ascending, Descending, ByField(&'static SpecField) }

pub struct FieldFilter { field: &'static SpecField, test: FilterTest }
pub enum FilterTest {
    Equal(SpecValue),
    OneOf(Vec<SpecValue>),              // at most 50 values
    Range { from: Option<SpecValue>, to: Option<SpecValue> },  // both ends included; JSON `null` = open
}
pub enum SpecValue { Text(SpecText), Day(Date), Number(SpecNumber) }

/// A number from a spec: digits and decimal places, read from the text,
/// never through floating point (D-506). 12.50 is digits 1250, places 2.
pub struct SpecNumber { digits: i64, places: u8 }      // places <= 18

/// Text from a spec: at most 200 characters (D-281), one line, control
/// characters refused. It is data, always shown as plain text (T-08, T-21).
pub struct SpecText(String);

pub struct ChartSize { width: Option<u16>, height: Option<u16> }   // each 100 to 2000
pub enum PaletteName { /* the eight names of ChartPalette.qml, by its fixed words */ }
```

Rules the constructors enforce, so the parser and the tests share them:

- **Layers:** 1 to 4. An `Arc` layer stands alone (a pie has no axis to share). The other layers share one category axis: every layer that has `x` names the same field with the same `type` and time unit, and every `y` is an amount or a count.
- **Channels per mark:**

  | Mark | Needs | May have |
  |---|---|---|
  | `Line`, `Bar`, `Area`, `Point` | `x` (date, nominal or ordinal) and `y` (quantitative) | `color` (nominal: one series per value), `tooltip`; `size` only on `Point` |
  | `Arc` | `theta` (quantitative) and `color` (nominal: one slice per value) | `tooltip`; `inner_radius` > 0 makes it a donut |
  | `Text` | `text`, and the `x` and `y` of the layer below it | `color` |
  | `Rule` | `y` only (a level line, for example an average) or `x` only (a line at one date) | `color` |

  A channel the mark does not take is an unsupported value (section 5), not an error.
- **Stacking:** a `Bar` or `Area` with `color` stacks, as Vega-Lite does by default. `stack` is not in the subset.
- A **horizontal** bar chart (a quantitative `x` with a nominal `y`) is not in the subset v1. It is listed as unsupported with the reason `horizontal`.
- **No floating point anywhere** in the model (AGENTS.md §3.1 rule 1). Drawing coordinates are made later, in the chart components, as they are today (D-065).

## 2. The fields a spec can name (D-503, Q-353 = B; M6e.1a)

`data.name` is a dataset id from the catalog, and `field` is one of that dataset's **spec fields**. A spec sees a dataset as a flat table, one row per entry of its result. The table is in the new `spec/fields.rs` (`SPEC_DATASETS: &[SpecDataset]`), which names the catalog's ids but does not change `catalog.rs`.

```rust
pub struct SpecDataset { id: &'static str, fields: &'static [SpecField] }
pub struct SpecField { name: &'static str, value: FieldValue }
pub enum FieldValue { Text, Day, Amount, Percent, YesNo }
```

| Dataset | Fields (kind) |
|---|---|
| `spending_by_category` | `category` (text: the path joined with " › ", empty for money not sorted yet), `top_level` (text), `total` (amount), `currency` (text) |
| `income_expense_by_month` | `month` (day: the month's first day), `income`, `expenses`, `net` (amounts), `currency` |
| `account_balances` | `account` (text: the account's name), `owed` (yes/no), `closed` (yes/no), `balance` (amount), `currency` |
| `net_worth_over_time` | `month` (day), `assets`, `liabilities`, `net_worth` (amounts), `currency` |
| `cash_flow_by_month` | `month` (day), `start`, `money_in`, `money_out`, `net_change`, `end` (amounts), `currency` |
| `category_over_time` | `month` (day), `series` (text: the chosen category's path joined with " › ", or the group's name), `total` (amount), `currency`. One row per month and chosen series |
| `budget_utilization` | `category` (text: the budget's category path), `every` (text: the budget's period word), `first_day`, `last_day` (days), `planned`, `carried_in`, `available`, `spent`, `left` (amounts), `used` (percent, empty when nothing was available), `carries_over` (yes/no), `currency`. One row per budget and period |

- **Which `type` suits which field:** `quantitative` only for an amount or a percent, or with `aggregate: count` for any field. A percent takes no `sum` and no `mean` (`report.spec-invalid`, `bad-aggregate`), because adding ratios gives no meaningful figure; `min` and `max` are exact. `temporal` only for a day. `nominal` and `ordinal` for text, yes/no and day. Any other pairing is `report.spec-invalid` with the reason `type-mismatch`.
- **Choices a spec cannot carry come from the properties panel (Q-353 = B).** `category_over_time` needs the chosen series and `budget_utilization` the chosen budgets. The custom chart takes them as the built-in components of those datasets do: `params.series` (categories and My groups, as `chart.line` of `category_over_time` has) and `params.categories` (the categories whose budgets are shown, empty for all, as `budget.progress` has). They are shown only while the spec names that dataset. Changing the spec to another dataset drops the choice the new one does not take, in the same edit, so nothing unused is stored. Without a chosen series the chart shows the same "choose what to show" state as the built-in chart. Both datasets read My groups, so, like the built-in components, a custom chart of them is never offered to the AI (D-291, D-358), which already holds for every custom chart (section 3).
- **Money stays exact.** An amount field holds `Money`. `sum`, `min` and `max` are exact and overflow-checked. `mean` is the exact sum divided by the count, rounded to minor units with the book's rounding setting (D-027). `count` is a plain number. **Different currencies are never added:** the renderer makes one chart per currency, as every built-in chart does (`ChartSet`). The `currency` field can be shown or filtered, but no aggregate crosses it.
- **The page's filters apply** (period, accounts, categories, D-350, D-475), as they do for the built-in components of the same dataset (`Takes`). A spec's own `filter` narrows further and never widens.
- **Without `aggregate`,** two rows that land on the same place (the same `x` or slice, and the same `color`) are added up, and the chart's table says so. This is how the existing components take one value per place.
- **Limits at drawing time (D-281):** at most 10,000 rows used after the filters, and at most 50 series or slices. Over either limit gives `report.spec-too-big` (section 5), never a partial chart.

## 3. Where a custom chart lives (D-504; M6e.3)

A custom chart is a component of a new type, **`chart.custom`** (`ComponentType::CustomChart`), inside the dashboard document of `docs/REPORT_DESIGNER.md` section 4. It also works in a view.

```json
{
  "id": "c7",
  "type": "chart.custom",
  "dataset": "income_expense_by_month",
  "params": {},
  "props": {
    "spec": "{\"data\":{\"name\":\"income_expense_by_month\"},\"mark\":\"bar\",\"encoding\":{\"x\":{\"field\":\"month\",\"type\":\"temporal\",\"timeUnit\":\"yearmonth\"},\"y\":{\"field\":\"net\",\"type\":\"quantitative\"}}}",
    "partial": false
  },
  "frame": { "col": 0, "row": 8, "cols": 6, "rows": 4 }
}
```

- **`props.spec`** holds the user's text exactly as pasted or read, as a JSON string, at most 32 KiB. It is stored as a string, not as an object, so the document's nesting limit (depth 8) and the spec's (depth 10) never meet, and the user gets back exactly what they wrote, spacing included. Nothing the user wrote is lost or rewritten.
- **`props.partial`** (default `false`) records that the user chose to draw the chart without the parts that are not supported (D-508).
- **`params`** holds `series` or `categories` only when the spec names `category_over_time` or `budget_utilization` (section 2), checked like any other component's params; otherwise it is empty.
- **`dataset`** is written from the spec's `data.name` whenever the spec is saved. `Component::from_value` runs the parser on `props.spec` (`spec::read`, section 4), so every reader of a document checks the spec: the book, a dashboard file (M6d.4, T-51) and the grid editor. If `dataset` and the spec do not agree, the result is `report.document-invalid` with the reason `spec-dataset` and the path `components[n].dataset`. A spec that is refused is `report.document-invalid` with the reason `bad-spec`, and its own code is kept for the properties panel.
- **No new document version.** The plan proposed raising it to 4. That is not done: D-278 already keeps a component of an unknown type as a placeholder, so an older KassenSturz still opens the dashboard and shows "This component isn't available in this version" for the custom chart alone. A raised version would make the older program refuse the whole dashboard. Nothing else in the document changes.
- **Dashboard files** (D-441) carry the component as it is. A spec names fields and the values of its filters, never IDs, so it needs no name mapping (D-432). Filter values such as a category's path are plain text from the user.
- **Not for the AI.** `CustomChart` is in `ComponentType::LIBRARY` (the Add menu) and **not** in `ComponentType::ALL`, so `propose_view` cannot pick it (Q-352 = A, D-282). The AI tools never get a spec's text: it is the user's free text, and it may hold words aimed at a model (T-38, AGENTS.md §8 on untrusted text).
- **Undo:** adding, changing and removing a custom chart are grid-editor edits like any other, one history entry each when the user leaves edit mode (D-315).

## 4. How a spec reaches the parser (D-505, D-506; M6e.1, M6e.3)

There is one way in, for a pasted spec and an opened file alike:

```rust
/// Reads a spec. Checks run in this order, and each check runs before any work of the next one.
pub fn read(text: &str) -> Result<Read, ReportError>;

pub struct Read {
    /// The chart, or `None` when what is left after the unsupported parts can't be drawn.
    chart: Option<ChartSpec>,
    /// Every unsupported part, in the order of the text (section 5). Empty for a spec that is fully supported.
    unsupported: Vec<Unsupported>,
}
pub struct Unsupported { path: String, reason: &'static str }
```

1. **Size:** more than 32 KiB of text gives `report.spec-too-big` with `limit` = `bytes`, before anything is parsed.
2. **Shape:** the existing raw scan of the document reader (`document::scan`, which finds `too-deep`, `duplicate-key` and `not-json` without building anything) runs with depth 10. A duplicate key is refused because `serde_json` would keep only the last one, so the checks and the drawing could see different specs.
3. **Parse:** `serde_json::from_str` into a `Value`. Its own recursion limit is never what stops a deep spec; step 2 has already stopped it.
4. **Walk:** the tree is walked key by key against the subset. A key the subset does not have becomes an `Unsupported` entry, never dropped silently. A key the subset has, but with a wrong value, is `report.spec-invalid` with its reason and path. The counts (layers 4, encodings 8, filters 8, `oneOf` 50, labels 200 characters) are `report.spec-too-big` with their `limit` word. They are checked when each one is reached, so a huge array costs one pass.

**Numbers are never floating point (D-506).** `serde_json` keeps a number with a fraction as `f64`, so the parser never reads one: it uses only `as_i64` and `as_u64`, and `is_f64` to refuse. Where a spec needs a number:
- `width`, `height` and `innerRadius` are whole numbers.
- An amount in a filter is a whole number of currency units (`"range": [0, 500]`) or a decimal written as a string (`"equal": "12.50"`), read into `SpecNumber` from its text. A JSON number with a fraction or an exponent is `report.spec-invalid` with the reason `fraction-number`, and the words say to write it in quotes. The renderer compares a `SpecNumber` with `Money` exactly, scaling to the currency's decimal places (D-027) without rounding.
- Dates in filters are ISO 8601 strings (`"2026-01-01"`). Vega-Lite's date-time objects are unsupported values.

**Pasting (M6e.3).** The properties panel of a custom chart has a multi-line field for the spec. The bridge calls `spec::read` on each change, after a short pause in typing, and returns a map that QML reads (AGENTS.md §9.4, *bridge objects are read back*): `ok`, `code`, `params`, `unsupported` (a list of `{path, reason}`), `dataset` and `canDraw`. Applying stores the text through the grid editor.

**Opening a file (M6e.3).** *Open a chart description…* in the same panel shows the system's file dialog (the portal under Flatpak), filtered to `*.vl.json` and `*.json` plus all files. The bridge:
- takes only a local file (`file:` URL); any other URL is refused before anything is read;
- reads at most 32 KiB + 1 byte with `Read::take`, so a huge or endless file costs bounded memory and time; more than 32 KiB is `report.spec-too-big` (`bytes`);
- accepts UTF-8 only, with a leading byte-order mark stripped; anything else is `report.spec-invalid` with `not-utf8`;
- puts the text into the spec field, so the user sees what was read, and the same check runs as for a paste.

The file is not remembered. Only its text is kept, in `props.spec`. Nothing is fetched: `$schema` is checked as text and never opened (D-280), and `data.url` is unsupported (section 5).

## 5. Error codes and the "not supported here" list (D-507, D-508)

**New codes** (rows in `docs/ERROR_CODES.md` in the step that raises each, as before):

| Code | Parameters | Keywords | Raised by |
|---|---|---|---|
| `report.spec-too-big` | `limit` | `bytes`, `depth`, `layers`, `encodings`, `filters`, `values`, `label`, `rows`, `series` | M6e.1 (all but the last two), M6e.2 (`rows`, `series`) |
| `report.spec-invalid` | `reason`, `path` | `not-json`, `not-utf8`, `not-an-object`, `duplicate-key`, `bad-schema`, `missing`, `wrong-type`, `bad-choice`, `unknown-dataset`, `unknown-field`, `type-mismatch`, `bad-channel-for-mark`, `layers-differ`, `bad-aggregate`, `fraction-number`, `bad-number`, `bad-date`, `bad-text`, `nothing-to-draw` | M6e.1; `not-utf8` M6e.3 |
| `report.spec-unsupported` | `count` | | M6e.3: shown when a stored spec has unsupported parts and `partial` is `false` |

`depth` uses the existing scan's `too-deep` as `report.spec-too-big` (`depth`), so a deep spec gives one clear code. The `document-invalid` reasons `bad-spec` and `spec-dataset` (section 3) are new keywords of an existing code. `path` is JSON-path text from the spec, shown as plain text and never part of a message (D-034).

**The unsupported list.** `Unsupported.reason` is one of these words. Each has its own sentence in the UI, and the path is shown in front of it, for example *`transform[1].calculate`: calculations aren't supported here*:

| Reason | For |
|---|---|
| `data-source` | `data.url`, `data.values`, `data.sequence`, `data.format`. The sentence says that data comes only from your book |
| `expression` | an expression string: `filter` as a string, `calculate`, a `value` or `condition` with an expression |
| `transform` | other transforms (`lookup`, `window`, `joinaggregate`, `fold`, `pivot`, `aggregate` as a transform, `bin`, …) |
| `mark` | another mark (`boxplot`, `geoshape`, `image`, `trail`, …) |
| `composition` | `facet`, `repeat`, `concat`, `hconcat`, `vconcat`, `params`, `selection` |
| `channel` | another channel, or a channel the mark does not take (section 1) |
| `horizontal` | a quantitative `x` with a nominal `y` |
| `option` | a key the subset does not have inside an allowed part (`scale`, `axis`, `legend`, `stack`, `bin`, `datum`, other `config`, `view`, `padding`, …) |
| `value` | an allowed key with a value outside the subset (`aggregate: median`, `timeUnit: quarter`, a date-time object) |

**Drawing with unsupported parts (D-508).** As `docs/REPORT_DESIGNER.md` section 5 says, the chart is not made, or is made from the supported part only if the user chooses that:
- With an empty list, the chart is drawn.
- With entries, the panel shows the list and draws nothing. If what is left can be drawn (`canDraw`: `data.name`, a mark and its needed channels are left), it offers **Draw it without these**, which sets `partial` to `true`. The list stays visible in the panel whenever the chart is selected, so nothing is hidden.
- A stored chart whose spec has entries while `partial` is `false` (for example from a dashboard file) shows the placeholder with `report.spec-unsupported` and its `count` instead of drawing.
- Without `data.name` (for example only `data.url`), nothing can be drawn and `Read.chart` is `None`, with the `data-source` entry and `spec-invalid` `missing` for `data.name`.

## 6. Steps and who owns which files (D-509)

The final table is in `docs/ROADMAP.md` (rows M6e.\*). Corrections to the planning agent's conflict map (`m6e-parallel-map.md` in the Agent-Teams repository):
- **M6e.1a** writes `R/spec/mod.rs`, `R/spec/model.rs` and `R/spec/fields.rs` (the spec datasets of section 2) plus `R/lib.rs`. It does not touch `catalog.rs`.
- **M6e.3** does **not** change `document.rs` (no version raise, section 3). It owns `schema.rs` and `component.rs` (the new type, its props and the `series` and `categories` params that depend on the spec's dataset), `U/src/properties.rs` and the panel's QML, `AddMenu.qml`, and the file open in `U`.
- **M6e.2** reads `R/spec/fields.rs` and the dataset functions, including `category_over_time` with its chosen series and `budget_utilization` with its chosen categories (read-only), and writes `R/spec/render.rs`, which turns a `ChartSpec` and the book's results into the existing `kassensturz-charts::ChartModel`, one per currency. It adds the marks the components lack (`point`, `text`, `rule`, layers) in `ChartView.qml`, and `MarkKind` to `ChartKind` where needed in `kassensturz-charts` (an additive enum change, which the map should list).
- **Seam tests (AGENTS.md §9.4):** whichever of M6e.1 and M6e.2 merges second adds a test that runs real `spec::read` output through the renderer. M6e.4 runs its hostile cases through the real UI path (the bridge's check, then the panel's map).

## 7. Words (glossary)

- **Custom chart**, German *Eigenes Diagramm*: a chart you describe yourself in a short text, for charts the built-in ones do not offer. Internal: the component `chart.custom`.
- **Chart description**, German *Diagrammbeschreibung*: the text that describes a custom chart, in the Vega-Lite language (a subset of it), pasted or opened from a file. Internal: the spec, `props.spec`.
- **Not supported here**, German *Hier nicht unterstützt*: a part of a chart description that KassenSturz does not read, listed with where it is. Internal: `spec::Unsupported`.
