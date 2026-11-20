# Plugins

KassenSturz can be extended with plugins: sandboxed WebAssembly modules with a versioned API (D-007), part of 1.0 and built during the Beta (#791, D-753). This page is the reference for plugin authors. It grows with each step of #791; the parts marked *not built yet* describe what is planned, not what works today.

## 1. The plugin API version

The plugin API has a version `MAJOR.MINOR`. This KassenSturz offers **1.0** (`kassensturz_plugin_api::API_VERSION`).

- A plugin names the version it targets in its manifest (`api`).
- It runs when the major version is the same and its minor version is the same or lower.
- A higher minor version means *this plugin needs a newer KassenSturz*; another major version is refused.
- A minor step only adds. Anything that would break an existing plugin is a major step, a decision for the maintainer (AGENTS.md §3.2). The API freezes at RC together with the book format (D-753).

## 2. Extension points

| Word in the manifest | What the plugin does | Permissions it may ask for (D-848) |
|---|---|---|
| `importer` | Reads a file the user picked and hands back statements. They go through the same import preview as the built-in importers (D-069). | `read_book` |
| `exporter` | Hands back the bytes of a file, which KassenSturz writes where the user chose. | `read_book` |
| `report` | Computes a report from the book. | `read_book` |
| `automation` | Reacts to events, such as an import, and may propose changes. | `read_book`, `propose_changes` |
| `ai_provider` | Translates for an AI service that isn't OpenAI-compatible (D-754, §12). KassenSturz does the sending. Hidden and never loaded while AI is off (D-013); consent and the preview of what is sent stay in KassenSturz. | `network` |
| `theme` | Colours, accent, font, corner radius, for light and dark (§9). **Data only, no code** (D-756). | none |
| `ui_density` | Turns on switches KassenSturz defines, such as hiding explanation texts. **Data only, no code** (D-756). Nothing that warns before a change to data can be hidden. | none |

A plugin may fill several extension points, but a data-only one (`theme`, `ui_density`) can't be combined with one that runs code.

## 3. Permissions

| Word | What it allows |
|---|---|
| `read_book` | Reading accounts, categories, transactions, plans, budgets and tags through a read-only view. Never the database itself (AGENTS.md §7). |
| `propose_changes` | Handing back a change set. The user reviews and confirms it like a change the AI prepared: one Undo, an entry in the change history. A plugin never writes by itself. |
| `network` | Connecting to other computers. Only an AI provider may ask for it. |

There is no permission for files: an importer gets the bytes of the file the user picked, and an exporter hands back bytes that KassenSturz writes. A permission is granted only if at least one of the plugin's extension points may have it; a manifest that asks for more is refused whole.

## 4. The manifest (D-847)

Every plugin carries one manifest: a JSON object of at most 16 KiB, in the custom section `kassensturz-manifest` of the plugin file (section 5).

```json
{
  "manifest_version": 1,
  "id": "org.example.csv-import",
  "name": "Example CSV import",
  "version": "1.2.3",
  "api": "1.0",
  "extension_points": ["importer"],
  "permissions": ["read_book"],
  "description": "Reads the CSV files of an invented bank.",
  "author": "Example Author",
  "license": "GPL-3.0-or-later",
  "homepage": "https://example.org/csv-import"
}
```

| Key | Required | Rule |
|---|---|---|
| `manifest_version` | yes | The whole number `1`. |
| `id` | yes | Lowercase reverse-DNS: 2 to 8 parts of `a`–`z`, `0`–`9` and `-`, each 1 to 63 characters and not starting or ending with `-`, at most 128 characters. Two plugins with the same ID are the same plugin. |
| `name` | yes | The name the Plugins tab shows, 1 to 80 characters. |
| `version` | yes | The plugin's own version, `MAJOR.MINOR.PATCH`, digits only, no leading zeros. Compared part by part, so 1.10.0 is newer than 1.9.0. |
| `api` | yes | The plugin API version it targets, such as `"1.0"` (section 1). |
| `extension_points` | yes | 1 to 7 words of section 2, none twice. |
| `permissions` | yes | 0 to 3 words of section 3, none twice; `[]` if it needs none. |
| `description` | no | Up to 500 characters; may have line breaks. |
| `author` | no | Up to 80 characters. |
| `license` | no | The plugin's license as an SPDX expression, up to 64 characters. KassenSturz doesn't check it. |
| `homepage` | no | An `https://` address, up to 200 characters. Shown as text; KassenSturz never opens it by itself. |

**The reader is strict.** A key it doesn't know, a value of the wrong type, a word twice in a list, a text that is empty, too long, has a space at either end, or holds control or invisible characters (such as the ones that reverse the direction of text) refuses the whole manifest, and nothing of the plugin is used. The manifest format and the API version are checked first, so a plugin made for a newer KassenSturz says so instead of naming a key this one doesn't know. Every refusal has an error code (`plugin.*` in `docs/ERROR_CODES.md`).

All texts of a manifest are untrusted (AGENTS.md §8): KassenSturz shows them as plain text, never as markup or instructions.

## 5. The plugin file (D-849)

A plugin file is **one WebAssembly module** (binary format version 1), at most 16 MiB. Any file name works; `.wasm` is usual.

- The manifest is the custom section `kassensturz-manifest`, exactly once.
- Other custom sections (such as `name` or `producers`, which compilers write) are ignored. A custom section whose name starts with `kassensturz-` and that this KassenSturz doesn't know refuses the file, so a later API version's data is never silently skipped.
- A **data-only** plugin (`theme`, `ui_density`) is a module without code: no function, code or start section. A theme's data is the custom section `kassensturz-theme` (§9), that of a UI density plugin the custom section `kassensturz-density` (§11).
- KassenSturz reads the manifest without running or compiling anything, so the Plugins tab can show it first.

With a Rust plugin, one way to add the section is a static in the plugin's source:

```rust
#[unsafe(link_section = "kassensturz-manifest")]
pub static MANIFEST: [u8; 123] = *br#"{"manifest_version":1, ...}"#;
```

where the array length is the manifest's length in bytes.

## 6. How KassenSturz runs a plugin (D-850)

The runtime is **wasmi**, an interpreter (Q-633): no compiler comes into the app.

**What the module must export:**

| Export | Kind | What it is |
|---|---|---|
| `memory` | memory | The one memory, through which bytes go in and out. |
| `ks_alloc` | `(len: i32) -> i32` | Gives the host room for `len` bytes and returns the address. |
| the calls of its extension points | `(ptr: i32, len: i32) -> i64` | Each takes the input at `ptr` with `len` bytes, and returns the output's address in the high 32 bits and its length in the low 32 bits. Which names each extension point calls, and what goes in and out, comes with that extension point (*not built yet*). |

A call: KassenSturz calls `ks_alloc(len)`, copies the input there, calls the export, and copies the output out of the plugin's memory. The plugin never sees KassenSturz's memory. Input and output are bytes; the extension points will use UTF-8 JSON, with amounts as text in minor units, never as floating point (AGENTS.md §3.1 rule 1).

**What the plugin gets: nothing it isn't given.**

- **No imports.** API 1.0 offers no host function, so a module that imports anything is refused. Later steps add functions in the module `kassensturz`, each behind a permission of section 3. There is no WASI: no files, clock, randomness or network.
- **Limits.** At most one memory of 64 MiB, one table of 10 000 elements, one instance, and fuel for about a billion instructions per call (also for the module's start function). A plugin that grows its memory past the limit gets `-1` from `memory.grow`; one that runs out of fuel is stopped. At most 16 MiB go in and come out per call.
- **One strike.** After a trap, running out of fuel, or giving no room for its input, the plugin takes no more calls until it is loaded again, since its state can't be trusted.
- **AI off means not loaded.** A plugin with the `ai_provider` extension point is never loaded while "AI features" is off (D-013, D-754).

Every refusal has an error code (`plugin.*` and `plugin-host.*` in `docs/ERROR_CODES.md`).

## 7. Adding a plugin (D-851)

In **Settings › Plugins** the user chooses a downloaded plugin file. KassenSturz reads it (at most 16 MiB), loads it once to check it, without calling anything, and shows what the manifest says: name, version, author, description, what it adds (its extension points) and what it may do (its permissions), plus homepage and license. A plugin that may use the network gets a plain warning. Only after a second click is a copy saved, as `<id>.wasm` in the `plugins` folder of the app's data folder; the downloaded file can then be moved or deleted.

- A plugin whose ID is added already replaces the old copy only after the user confirms it; the page names both versions.
- Removing a plugin deletes only its copy.
- While "AI features" is off, AI provider plugins are not listed and can't be added (D-013).
- A copy that no longer loads, e.g. after an update of KassenSturz, stays in the list with the reason, so it can be removed.

## 8. Importers and exporters (D-852)

A plugin importer or exporter is used exactly like a built-in one (D-069): it shows in the same lists, and what it reads goes through the same checks and the same import preview. Its ID there is the plugin's ID. Everything goes in and out as UTF-8 JSON. **Amounts are whole numbers of minor units written as text**, such as `"-2345"` for −23.45 EUR: an optional `-` and digits, no leading zeros, within 64 bits. Never a JSON number, which many languages read as floating point (AGENTS.md §3.1 rule 1). Dates are `YYYY-MM-DD`; currencies are ISO 4217 codes.

KassenSturz reads every answer strictly: a key it doesn't know, a value of the wrong type, or `null` where a key may only be left out makes the whole answer unusable (`plugin-host.output-invalid`), and nothing of it is used.

**An importer** exports three calls (section 6):

| Call | In | Out |
|---|---|---|
| `ks_importer_info` | nothing | `{"extensions": ["csv"]}`: 0 to 8 file name extensions, lowercase letters and digits, without the dot, none twice. |
| `ks_importer_recognize` | the file's first bytes, at most 4096 | `{"recognition": "yes"}`, `"maybe"` or `"no"`. Any other answer, or a failure, counts as `"no"`; the user can always pick the importer anyway. |
| `ks_importer_read` | the whole file | `{"statements": [...]}`, or `{"error": "malformed", "line": 7}` when the file isn't one it can read (`line`, counted from 1, may be left out). |

A statement:

```json
{
  "account": {"iban": "DE89370400440532013000"},
  "currency": "EUR",
  "opening_balance": {"amount": "100000", "date": "2026-10-31"},
  "closing_balance": {"amount": "97655", "date": "2026-11-02"},
  "entries": [
    {"status": "booked", "booking_date": "2026-11-01", "amount": "-2345",
     "other_party_name": "Example Bakery", "remittance_information": "Bread"}
  ]
}
```

`account` holds `iban` and `number`, each optional. `opening_balance` and `closing_balance` may be left out. An entry needs `status` (`"booked"` or `"pending"`), `booking_date` and `amount`; `bank_id`, `value_date`, `other_party_name`, `other_party_iban` and `remittance_information` may be left out. Every amount is in the statement's currency. A plugin importer reads bank statements only: a whole book from another program is the built-in importers' job. The limits on entries and text that hold for every importer hold for plugins too.

**An exporter** exports two calls:

| Call | In | Out |
|---|---|---|
| `ks_exporter_info` | nothing | `{"extension": "csv"}`: the file name extension it writes. |
| `ks_exporter_write` | the transactions, as below | the bytes of the file, which KassenSturz writes where the user chose. |

What it gets, with every key there and `null` where there is nothing:

```json
{"accounts": [{
  "name": "Checking",
  "currency": "EUR",
  "minor_digits": 2,
  "rows": [{
    "date": "2026-11-01",
    "payee": "Example Bakery",
    "counterpart": {"kind": "category", "name": "Food"},
    "memo": null,
    "note": "Sunday",
    "amount": "-2345",
    "original": {"amount": "-2500", "currency": "USD", "minor_digits": 2}
  }]
}]}
```

`counterpart` is `{"kind": "uncategorized"}`, `{"kind": "category", "name": ...}`, `{"kind": "transfer", "account": ...}` or `{"kind": "parts", "count": ...}`. `original` is the amount in another currency, if there was one. A plugin exporter writes transactions only, never a whole book.

**Where they show.** When the user imports a file from the bank, KassenSturz first tries its own formats. One that is sure of the file wins, and a whole book (such as a KMyMoney file) is refused with a hint. Then the plugin importers are tried in the order of their names: the first one that is sure wins. After that a file named `.csv` or `.txt` goes to the column screen, then a built-in format that may fit is used, then the first plugin that may. The preview says which plugin read the file, and the file dialog offers the plugins' extensions. A plugin exporter shows on the "Save a copy of my data" page under "With a plugin": it writes the accounts and days chosen for the spreadsheet file.

Neither an importer nor an exporter needs a permission for this: it gets only the file or the transactions the user picked (D-848).

## 9. Themes (D-925)

A theme plugin changes how KassenSturz looks, never what it does. It is data only: a module without code whose manifest names `theme`, with its colours as strict JSON in the custom section `kassensturz-theme`, exactly once, at most 4 KiB. A module with that section that isn't a theme plugin is refused.

```json
{
  "theme_version": 1,
  "light": {"background": "#F1F8F1", "text": "#1B2A1C", "accent": "#2E7D32"},
  "dark": {"background": "#102214", "text": "#E2F0E3", "accent": "#81C784"},
  "font": "Example Sans",
  "corner_radius": 8
}
```

- `theme_version` is 1. `light` and `dark` are both needed, each with `background`, `text` and `accent` as `#RRGGBB`. The light look has the lighter background, the dark one the darker.
- **Readable or refused.** `text` and `accent` must each stand out from `background` by at least 4.5 to 1 (WCAG 2.2 level AA for normal text). A theme below that is refused whole, with the colour that failed.
- `font` (optional): a font family, used if the computer has it; otherwise the system's font stays.
- `corner_radius` (optional): 0 to 16, the corner radius in logical pixels of KassenSturz's own boxes, such as the tiles of the start screen.
- Unknown keys, wrong types and other spellings of a colour refuse the theme, as in the manifest.

KassenSturz works out every other colour from these three: the backgrounds of the window and buttons, secondary text, selections, and its own colours for warnings, problems and amounts below zero, each kept at the same contrast. A theme can't change those meanings, and it doesn't change the chart colours, which the user picks in Settings › Look.

**Choosing one.** Settings › Look › "Colors of the app" lists the theme plugins that were added, and the choice is kept as the plugin's ID under `Look/Theme` in the app's settings (D-924). It applies from the next start, with the theme's light or dark colours after the system's setting at that moment. A theme that is removed or can no longer be read leaves KassenSturz's own colours.

**Example.** `examples/plugins/green-theme/` holds the green theme: its `manifest.json` and `theme.json`, and `green-theme.wasm`, packed from them by `tools/make-data-plugin.py`:

```sh
tools/make-data-plugin.py manifest.json kassensturz-theme=theme.json green-theme.wasm
```

## 10. Reports and automations (D-926)

Both read the book, so both must ask for `read_book` in their manifest; an automation also asks for `propose_changes`. Without them KassenSturz doesn't call the plugin (`plugin-host.permission-missing`).

**A report** exports two calls:

| Call | In | Out |
|---|---|---|
| `ks_report_info` | nothing | `{"reports": [{"id": "monthly", "name": "Monthly summary"}]}`: 1 to 20 reports, each ID 1 to 64 lowercase letters, digits, `-` or `_`. |
| `ks_report_run` | the exporter's transactions (§8) of the accounts and days the user chose, plus `"report": "<id>"` | the table, as below. |

```json
{"title": "Monthly summary",
 "columns": [{"label": "Month", "kind": "text"}, {"label": "Left over", "kind": "amount"},
             {"label": "Payments", "kind": "count"}, {"label": "Last day", "kind": "date"}],
 "rows": [["November 2026", {"amount": "-123456", "currency": "EUR", "minor_digits": 2}, "12", "2026-11-30"]]}
```

At most 20 columns and 10,000 rows; every row has one cell per column, or `null` for an empty one. A `text` cell has at most 500 characters, a `count` is a whole number written as text, an amount is in minor units as text with its currency and 0 to 4 decimal places. Titles, names and labels have at most 80 characters. KassenSturz formats the dates and amounts after the user's settings, draws the table with its own components and shows every text as plain text; a report has no charts or markup. The page says that the plugin, not KassenSturz, worked out the figures.

**An automation** exports one call so far, after an import:

| Call | In | Out |
|---|---|---|
| `ks_automation_after_import` | `{"categories": [names], "transactions": [{"id", "date", "account", "payee", "memo", "amount", "currency", "minor_digits", "uncategorized"}]}`, the transactions just imported | `{"suggestions": [{"transaction": "<id>", "category": "<name>"}]}` |

It may suggest only a category the book already has, only for a transaction it was given that has none (`"uncategorized": true`), and each transaction at most once; any other answer is refused whole. Suggestions are only suggestions: KassenSturz lists them for the user to tick, and applies the ticked ones as one change with one Undo.

**Where they show.** The reports of every report plugin are listed on the Statistics page under "From plugins"; one opens a page with a choice of account (or all) and period (this month, the last 3 or 12 months, this year, everything), where the plugin works it out again on each change. After an import, each automation plugin is asked once; the page that says what was imported lists its suggestions, all ticked, with "Use the ticked suggestions" and "Not now". Using them is recorded in the change history as editing transactions, so the book format stays as it is.

## 11. UI density (D-927)

A UI density plugin changes how much fits on the screen, never what KassenSturz shows about the user's money. It is data only: a module without code whose manifest names `ui_density`, with its switches as strict JSON in the custom section `kassensturz-density`, exactly once, at most 1 KiB. A module with that section that isn't a UI density plugin is refused.

```json
{
  "density_version": 1,
  "switches": {"compact_lists": true, "hide_hints": true, "compact_tiles": true}
}
```

- `density_version` is 1, and `switches` is needed. Each switch is `true` or `false`; one left out is off.
- `compact_lists`: tighter rows in the lists of transactions, categories, category groups and Rules.
- `hide_hints`: hides the grey explanations under some settings, such as "Most households never need to change anything here."
- `compact_tiles`: smaller gaps between the tiles of the start screen and the account and figure grids.
- A switch this KassenSturz doesn't define, a wrong type or an unknown key refuses the plugin, as in the manifest.

**Warnings always stay.** `hide_hints` hides only texts the page works without. Warnings, error messages, the questions before a change, and texts about Undo, backups or what a file holds are never such a text, and a test checks it on every build (`density_audit`).

**Choosing one.** Settings › Look › "Layout" lists the layout plugins that were added, and the choice is kept as the plugin's ID under `Look/Density` in the app's settings. It applies at once. A layout plugin that is removed or can no longer be read turns every switch off.

**Example.** `examples/plugins/compact-layout/` holds the "Compact" layout with all three switches on, packed the same way as the green theme:

```sh
tools/make-data-plugin.py manifest.json kassensturz-density=density.json compact-layout.wasm
```

## 12. AI providers (D-754, D-928)

An AI provider plugin lets KassenSturz talk to an AI service whose interface isn't OpenAI-compatible. **It only translates.** It gets no way to reach the network: KassenSturz builds every request in the OpenAI-compatible form, the plugin turns it into its service's form, and KassenSturz sends that, with its own HTTPS client and all its checks, only to the address the user typed in Settings › AI. The answer comes back through the plugin into the OpenAI-compatible form, which KassenSturz reads with the same limits as any answer. The manifest names `ai_provider` and asks for `network`, which the Plugins tab shows as "sends to the address you enter". The plugin is hidden and never loaded while AI is off (D-013).

| Call | In | Out |
|---|---|---|
| `ks_ai_info` | nothing | `{"chat_path": "messages", "models_path": "models", "key_header": "x-api-key", "key_prefix": "", "headers": {"example-version": "2026-01-01"}}` |
| `ks_ai_request` | an OpenAI-compatible chat request (`model`, `messages`, `tools`, …) | the service's request body, a JSON object |
| `ks_ai_reply` | the service's answer to it | an OpenAI-compatible chat answer (`choices[0].message` with `content` and `tool_calls`, `finish_reason`) |
| `ks_ai_models` | the service's list of models | an OpenAI-compatible list (`{"data": [{"id": "..."}]}`) |

- **The route is fixed.** `chat_path` and `models_path` are paths under the user's address: letters, digits and `. _ ~ / -`, with no leading `/`, no `//`, no `.` or `..` part, no query. `key_header` names the header the key goes into, and `key_prefix` (optional, such as `Bearer `) what comes before it. `headers` (optional) adds up to 8 fixed headers. Headers that change where or how a request goes (`Host`, `Content-Type`, `Content-Length`, `Cookie`, `Proxy-…` and the like) are refused, and so is the whole plugin with them.
- **The key never reaches the plugin.** KassenSturz adds it to the named header when it sends, after the preview was built.
- **The preview is what is sent.** The body `ks_ai_request` builds is checked to be JSON, shown in the preview, and sent byte for byte; redacting a phrase builds it again. Consent per address and the statement of what is sent stay as for any provider.
- A plugin that fails, or answers something that isn't JSON, stops the request with `ai.plugin-failed`; a chosen plugin that was removed stops every request before anything is sent (`ai.plugin-missing`). KassenSturz never falls back to another kind of service by itself.

**Choosing one.** Settings › AI shows "Kind of service" once an AI provider plugin was added: OpenAI-compatible, or the plugin. The choice is kept as the plugin's ID under `AI/ProviderPlugin` in the settings of this computer, and applies to the next request. Address, model, key and consent are set up as for any provider.

## 13. Still to come (#791)

One example plugin per extension point, and a "write your own plugin" guide.
