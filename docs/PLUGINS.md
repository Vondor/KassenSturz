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

Every plugin carries one manifest: a JSON object of at most 16 KiB. Where it sits inside the plugin file is defined with the package format (*not built yet*, #791 step 2).

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

## 5. Still to come (#791)

The host that loads and sandboxes the modules and the package format (step 2, waiting for the choice of WASM runtime, a new direct dependency), the **Plugins** tab, the calls of each extension point, one example plugin per extension point, and a "write your own plugin" guide.
