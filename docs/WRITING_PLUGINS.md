# Write your own plugin

This guide takes you from nothing to a plugin that KassenSturz imports from. It uses a real, working example: `examples/plugins/simple-bank-text/` reads the text statements of an invented "Example Bank". The rules behind it are in [`PLUGINS.md`](PLUGINS.md); a reference like "PLUGINS.md §8" means its section 8.

A plugin is a **WebAssembly module**: one `.wasm` file with no access to your files, network or clock unless KassenSturz hands it something (D-850). The user adds the file in **Settings › Plugins**, sees what it asks for, and confirms.

## 1. Pick what your plugin adds

| You want to… | Extension point | Needs code? | Example in `examples/plugins/` |
|---|---|---|---|
| Read a bank's file KassenSturz doesn't know | `importer` | yes | `simple-bank-text` |
| Write the data to another file format | `exporter` | yes | none yet; same shape (PLUGINS.md §8) |
| Add a table to the Statistics page | `report` | yes | none yet (PLUGINS.md §10) |
| Suggest categories after an import | `automation` | yes | none yet (PLUGINS.md §10) |
| Change the look | `theme` | no, data only | `green-theme` |
| Change how much fits on screen | `ui_density` | no, data only | `compact-layout` |
| Talk to an AI service that isn't OpenAI-compatible | `ai_provider` | yes | none yet (PLUGINS.md §12) |

Data-only plugins are a manifest and a JSON file packed by `tools/make-data-plugin.py`; the green theme shows how (PLUGINS.md §9). The rest of this guide is about plugins with code, using the importer.

## 2. What you need

- Rust with the WebAssembly target: `rustup target add wasm32-unknown-unknown`.
- This repository, for the example to copy.

Any language that compiles to WebAssembly works, as long as the module has **no imports** (see section 6) and the exports of section 3 below. The example is Rust without the standard library (`#![no_std]`), which keeps the file at 15 KB and makes sure nothing sneaks in an import.

## 3. The shape every plugin has

Copy `examples/plugins/simple-bank-text/` and rename it. These parts are the same in every plugin with code:

1. **A manifest**, in the custom section `kassensturz-manifest` (section 4, and PLUGINS.md §5). The example keeps it in `manifest.json` and includes the file's bytes into the module, so the file and the plugin can't drift apart.
2. **Memory.** The module exports `memory` and `ks_alloc(len) -> address`. Before each call KassenSturz asks `ks_alloc` for room and copies the input there. The example's allocator starts again from the beginning on each `ks_alloc`, since the host has copied the last answer out by then, so the plugin can't run out of memory by being called often.
3. **Calls.** Each one is `(address, length) -> i64`: it gets the input's address and length, and returns the answer's address in the high 32 bits and its length in the low 32 bits. The names depend on the extension point; for an importer they are `ks_importer_info`, `ks_importer_recognize` and `ks_importer_read` (section 5).
4. **A panic handler that traps.** A trap stops the plugin; KassenSturz says so and doesn't call it again until it is loaded anew ("one strike", PLUGINS.md §6).

Everything that goes in and out is UTF-8 JSON, except that an importer's input is the file's raw bytes.

## 4. The manifest

```json
{
  "manifest_version": 1,
  "id": "io.github.vondor.kassensturz.example.simple-bank-text",
  "name": "Simple bank text",
  "version": "1.0.0",
  "api": "1.0",
  "extension_points": ["importer"],
  "permissions": [],
  "description": "Reads the text statements of the invented \"Example Bank\".",
  "author": "KassenSturz contributors",
  "license": "CC0-1.0"
}
```

Choose an ID that is yours (reverse DNS, lowercase), and **ask only for the permissions you need**. An importer or exporter needs none: it gets only the file the user picked. A permission your extension points may not have refuses the manifest (PLUGINS.md §3). The reader is strict: an unknown key, a wrong type or a text with odd characters refuses the plugin, so test the file early.

## 5. An importer, step by step

The Example Bank's file looks like this (`sample.txt`):

```text
EXAMPLE-BANK STATEMENT
Account: DE89370400440532013000
Currency: EUR
Opening: 2026-10-31 1000.00
2026-11-01 | Example Bakery | Bread | -23.45
2026-11-02 | Example Employer | Pay for October | 2500.00
2026-11-02 | Example Landlord | Rent | -450.00
Closing: 2026-11-02 3026.55
```

**`ks_importer_info`** says which file name extensions the file dialog offers: `{"extensions": ["txt"]}`.

**`ks_importer_recognize`** gets the file's first bytes (at most 4096) and answers `{"recognition": "yes"}`, `"maybe"` or `"no"`. Say "yes" only when you are sure: the first plugin that is sure wins, and a wrong "yes" takes a file away from the importer that should have read it. The example checks for its header line. The user can always pick your importer by hand anyway.

**`ks_importer_read`** gets the whole file and answers `{"statements": [...]}`, or `{"error": "malformed", "line": 5}` for a file it can't read, with the line (counted from 1) if you know it. Refuse rather than guess: KassenSturz shows the line to the user.

A statement has the account's `iban`, the `currency`, optional opening and closing balances and the `entries` (PLUGINS.md §8). The example's `read` function parses the file line by line and writes the JSON by hand; with a JSON library you would build the same thing.

### Money is text, never a number

This is the one rule that matters most (AGENTS.md §3.1 rule 1; PLUGINS.md §8). Amounts in an answer are **whole minor units written as text**: `"-2345"` for −23.45 EUR. Never a JSON number, and never a floating-point type in your own code. The example never converts "23.45" to a number at all: it checks the digits, glues the whole part and the two decimals together as text, and strips leading zeros.

Two things follow:

- **Know your currency's decimals.** "10.00" is `"1000"` in euros but `"10"` in a currency without decimals. The example knows four currencies, all with two decimals, and refuses any other, answering `malformed` with the line of the currency, instead of guessing.
- **Zero has no sign.** `"-0"` is refused; write `"0"`.

### Text from the file is data

A payee or memo can hold quotes, backslashes or text that looks like JSON or instructions. Escape it when you write your answer (the example's `string` function) and refuse control characters. KassenSturz treats everything a plugin returns as untrusted too (D-852): it reads the answer strictly and shows texts as plain text.

## 6. Build it

```sh
cd examples/plugins/simple-bank-text
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/simple_bank_text.wasm simple-bank-text.wasm
```

The example's `Cargo.toml` builds a `cdylib` with `panic = "abort"`, `lto`, `opt-level = "s"` and `strip = true`. It holds an empty `[workspace]` table, so Cargo treats the folder as a project of its own, not as part of KassenSturz's workspace.

Things that make KassenSturz refuse the module (the codes are in `docs/ERROR_CODES.md`):

- **Any import.** Standard-library code that needs the operating system (files, time, threads, `println!`) imports functions the plugin doesn't get. Use `no_std`, or check the module with `wasm-objdump -x` or any WebAssembly tool for an import section.
- **A missing export** (`memory`, `ks_alloc`, the calls your extension point needs).
- **A bigger memory or longer run than the limits**: 64 MiB of memory, about a billion instructions per call, 16 MiB in and out per call (PLUGINS.md §6).
- **An unknown custom section** whose name starts with `kassensturz-`.

## 7. Try it

1. In KassenSturz, open **Settings › Plugins** and choose `simple-bank-text.wasm`. The page shows its name, what it adds and the permissions it asks for (none). Confirm.
2. Open **Import** and choose `sample.txt`. The preview says which plugin read the file. Nothing is saved until you confirm the preview, as with any import.
3. Check the amounts and the balances against `sample.txt`.

In the repository, the same check runs as a test: `cargo test -p kassensturz-plugin-formats example`. It loads the plugin file that is checked in, imports the sample through the same `import` function as every built-in importer, checks that the opening balance plus the entries equals the closing balance, and checks the error lines and the escaping. Write a test like it for your plugin, using **synthetic data only**: invented names and IBANs from SWIFT's IBAN Registry, never a real statement (AGENTS.md §3.1 rule 2).

## 8. Versions, permissions and updates

- **Your plugin's version** (`version`) is yours; a user who adds a newer one with the same ID is shown both versions and confirms the replacement (PLUGINS.md §7).
- **The API version** (`api`) is the one you built against. A newer minor version of KassenSturz still runs it; a major step is announced as a decision first (PLUGINS.md §1). Plugins made for a newer KassenSturz than the user's say so.
- **Asking for more later:** the permissions are named words. A new one is added in a minor API step, and the page that adds your plugin always lists what it asks for, so an update that asks for more is visible to the user before they confirm.

## 9. Before you share it

- Test with a file with an odd line, an empty file, one with a very long line and one with quotes in a name. The plugin must answer `malformed` or an error, never trap.
- Keep the plugin free of anything that identifies a person or a bank's customers: examples with invented names only.
- Put a license in the manifest (`license`, an SPDX expression). The examples in this repository are CC0-1.0, free to copy.
- Say in your description what the plugin reads and that it needs no permissions. Users see it in the Plugins tab.

## 10. The other extension points

They follow the same shape; their calls and answers are in the reference:

- **Exporter:** `ks_exporter_info`, `ks_exporter_write` (PLUGINS.md §8).
- **Report** and **automation:** `ks_report_info`, `ks_report_run` and `ks_automation_after_import`; both ask for `read_book` (PLUGINS.md §10).
- **Theme** and **UI density:** data only (PLUGINS.md §9 and §11).
- **AI provider:** only translates; KassenSturz sends the request and keeps the key (PLUGINS.md §12).
