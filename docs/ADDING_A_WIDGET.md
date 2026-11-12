# Adding a widget type

For developers and AI agents who want the Overview to offer a new kind of widget (D-534). In the code a widget is called a **component**, and its kind is a `ComponentType`. This guide lists every place a new kind touches and the rules that apply. The design behind it is in `docs/REPORT_DESIGNER.md` section 4 and the later `REPORT_DESIGNER_M6*.md` files.

Read `AGENTS.md` first. A new widget type is a normal step: one single-purpose pull request, tests, translations and docs (AGENTS.md §9).

## 0. Do you need a new type?

Many wishes need no code. A user can already make these widgets in the app (*Anordnung bearbeiten › Hinzufügen*, English *Edit layout › Add*):

- **Eigenes Diagramm** (*Custom chart*, M6e): a chart described in a subset of Vega-Lite, pasted or opened from a file (`docs/REPORT_DESIGNER_M6E.md`).
- **A figure card or progress bar with a formula** (*Formel*, Calc-style, D-322, D-510; `docs/REPORT_DESIGNER_M6B.md`).
- **Gespeicherte Ansichten** (*Saved views*): any chart made in the *Statistik-Baukasten* (*Statistic Builder*).

Add a type only when none of these can show it, for example a widget that does something (the *Update all accounts* button) or draws data in a way no chart can.

Plugins can't add widgets either. The WASM plugin host comes after Gold (D-071), and its extension points are Importer, Exporter, Report and Automation (AGENTS.md §7).

## 1. Choose what kind of widget it is

There are two kinds. Pick one before you write code, because it decides most of the steps below.

| Kind | What fills it | `shows_no_data()` | Examples |
|---|---|---|---|
| **Data widget** | A dataset from the catalog (`crates/kassensturz-report/src/catalog.rs`), asked for by the page with the component's dataset, params and period | `false` | `chart.bar`, `kpi.card`, `budget.progress`, `chart.custom` |
| **Page widget** | The page or a bridge object of its own; the document names no dataset | `true` | `card.net_worth`, `bank.update_all`, `page.tabs`, `text.heading` |

## 2. The worked examples

Read the diff of the pull request closest to your widget. Each one shows the whole path from the type to the screen:

- **#354**, dashboard tabs (`page.tabs`): a page widget with no settings.
- **#335**, Update all accounts (`bank.update_all`): a page widget with its own bridge object, QML file and smoke test.
- **#349**, custom chart (`chart.custom`): a data widget with its own properties and a checked text property.

## 3. The steps, in order

### 3.1 The model (`crates/kassensturz-report`)

1. **`src/schema.rs`**: add a variant to `ComponentType` with a doc comment that says what it shows and which decision it comes from. Then:
   - `id()`: its fixed type id, `<group>.<name>` (for example `card.savings`). Documents store this id; never change it once it has been released.
   - `LIBRARY`: add it so the Add menu can offer it.
   - `ALL`: add it **only** if the AI may propose it in `propose_view` (D-282). Adding a type here changes what the AI sees, which is the human gate "AI data sharing" (AGENTS.md §3.2). Free text the user writes (like a chart description) never goes to the AI (Q-352, T-38).
   - `shows_no_data()`, `suit()` and `properties()`. A property needs an entry in a property table (see `CHART`, `UPDATE_ALL`, `CUSTOM_CHART` in the same file) with its kind and allowed values. Its label is looked up by key in the app, so the table holds no user-facing text (D-034).
2. **`src/grid.rs`**: `smallest_size()` and `default_size()` (D-287, D-296). The grid is 12 columns wide.
3. **Data widgets only:** add the new `suit()` word to the `suits` of each dataset in `src/catalog.rs` that fits. A new dataset is a step of its own, with exact numbers (`Money`, never floating point, AGENTS.md §3.1 rule 1) and tests.
4. **`src/templates.rs`** and **`src/default_overview.rs`**: only if the widget belongs in a template or in the built-in Overview.
5. **`src/document.rs`**: usually no change. Every match on `ComponentType` is exhaustive, so `cargo build` lists every other place that needs your variant. Do **not** raise the document `VERSION` for a new type: an older KassenSturz shows a component of a type it doesn't know as a placeholder and still opens the rest of the dashboard (D-278, D-504). Raise it only when older dashboards must be rewritten as they are read (D-476, D-530).
6. A document is untrusted input (D-281). Every new property has a length or range limit, is checked in `Component::from_value`, and its text is shown as plain text, never as markup.

### 3.2 The fixed names (`crates/kassensturz-core`)

- **`src/vocabulary.rs`**: add the type id with the dot written as an underscore (for example `card_savings`) to `DETAILS`, in the widget block. The debug log writes only names from this list (D-173, D-227); a bridge test fails without it.

### 3.3 The bridge (`crates/kassensturz-ui-bridge`, Rust)

- **`src/properties.rs`**, `preview()`: what the properties panel's preview draws for the type.
- **`src/grid_editor.rs`** and **`src/grid_editor_model.rs`**: only if the cell needs a field the existing ones don't carry. Every field you add to a `QVariant` map has a test that reads it back the way QML does (AGENTS.md §9.4).
- **A page widget with its own logic:** a bridge object of its own (as `src/update_all.rs` and `src/bank_connection_model.rs` in #335), registered in `src/main.rs`.
- **Numbers reach QML only as text** (D-065). Rust formats every amount; sums and sorting stay in Rust.

### 3.4 The screen (`crates/kassensturz-ui-bridge/qml`)

1. **`GridItem.qml`**: add the type to the `sourceComponent` choice of the `content` loader and add the `Component` that draws it. A data widget that looks like an existing chart, card or table can use `figuresBox` and needs no new branch.
2. **`GridNames.qml`**: its name in the Add menu and in tooltips, with an `i18nc` context that says what it is (`"@item:inlistbox a part of the layout: …"`).
3. **`AddMenu.qml`**: add the type id to one of the groups (*Charts*, *Figures and tables*, *Bank*, *Layout*, *Notes*).
4. **`TemplatePicture.qml`**: the icon of the type in template pictures, if the default icon doesn't fit.
5. **A new QML file:** add it to the list in **`build.rs`** (cxx-qt registers QML files by their path) and to the module lists in **`tools/check-panel.sh`**.
6. **A smoke test** in `qml-tests/` (for example `PageWidgetsSmoke.qml`, `UpdateAllSmoke.qml`), run by `tools/check-panel.sh`. Fakes for bridge objects live in `qml-tests/fakes/`.
7. **Accessibility:** keyboard reachable, a screen-reader name, enough contrast (AGENTS.md §6). Hidden edit-mode parts use a `Loader`, not `visible: false` (`docs/ARCHITECTURE.md`, "Opening the Overview").
8. **AI:** an AI-related widget is hidden while AI is off, not greyed out (AGENTS.md §8, D-013), and the build without AI (D-039) still has to work; `tools/check-no-ai.sh` checks its dependencies.

### 3.5 Text and translations

- No user-facing text in Rust; the core returns codes (D-034). New error codes go into `docs/ERROR_CODES.md`.
- Run `tools/update-translations.sh` and `tools/check-translations.sh` (they need gettext, for example in the Qt container) and fill in the German entries in `po/de/kassensturz.po`. When other open pull requests also touch `po/de`, add only your new entries by msgid instead of regenerating the whole file.
- Plain words: no accounting terms in names or help (AGENTS.md §6, `docs/GLOSSARY.md`).

### 3.6 Docs

- A row in `docs/DECISIONS.md` for every choice you made (the coordinator hands out the number).
- The design doc of the current milestone (`docs/REPORT_DESIGNER_M6*.md`) gets a short section on the type, its properties and its sizes.
- `docs/DASHBOARD_FILE.md` if the type needs anything new in a dashboard file (D-441, D-432: a file names fields and values, never IDs of the book).
- The testing guide of the milestone gets the steps to try it by clicking.

## 4. Checks before the first push

From AGENTS.md §9.4, in the shared SDK environment (`tools/sdk.sh …`, `docs/ARCHITECTURE.md`, "Build and development"):

```sh
tools/sdk.sh cargo fmt --all --check
tools/sdk.sh cargo clippy --workspace --all-targets -- -D warnings
tools/sdk.sh cargo test --workspace
tools/check-panel.sh            # QML smoke tests, in the Qt container
tools/check-translations.sh
```

Tests that already cover every type tell you what you missed: in `crates/kassensturz-ui-bridge/src/grid_editor.rs`, `every_library_kind_has_a_debug_log_detail` checks `DETAILS`, and `the_library_offers_the_twenty_four_components_and_adds_each_with_its_defaults` adds every `LIBRARY` kind (update its count and name, and the length of the `LIBRARY` array, when you add one).

## 5. Human gates to check

Most widget types touch none. Tick the gate in the pull request when the widget:

- is added to `ComponentType::ALL` or shows anything to the AI (AI data sharing),
- reads from or starts a bank connection (bank connections),
- stores secrets (keychain),
- needs a new crate (dependencies).
