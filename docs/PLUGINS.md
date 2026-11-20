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
| `ai_provider` | Talks to an AI service that isn't OpenAI-compatible (D-754). Hidden and never loaded while AI is off (D-013); consent and the preview of what is sent stay in KassenSturz. | `network` |
| `theme` | Colours, accent, fonts, corner radius. **Data only, no code** (D-756). | none |
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
- A **data-only** plugin (`theme`, `ui_density`) is a module without code: no function, code or start section. Where its data sits comes with those extension points (*not built yet*).
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

## 9. Still to come (#791)

The calls and host functions of the other extension points, the data of themes and UI density, one example plugin per extension point, and a "write your own plugin" guide.
