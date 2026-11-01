# KassenSturz

A free, open-source personal finance app for the desktop, **simpler than KMyMoney and GnuCash**, made for everyday people who have never taken an economics class.

## Goals

- Simple, plain-language interface with progressive disclosure
- Open the app and see what you need: the start screen with your personalised overview is complete on its own, and everything deeper (automatic sorting rules, more details per transaction, AI help) is optional customisation
- Correct money handling with multi-currency support and European standards
- Local-first and private. Your data stays on your machine
- Open plugin system (WASM)
- Native AI connection, including locally running models, with the user always in control
- First-class on **KDE Plasma**, then **GNOME**, **Windows**, **macOS**
- Built the KDE way, with the goal of becoming an official KDE project

## Status

Pre-alpha. The core and the book file (milestones M1 and M2 in [`docs/ROADMAP.md`](docs/ROADMAP.md)) and the first usable app (M3), bank connections (M4) and the AI features (M5) are in place and being tested; no release is tagged yet. Builds are numbered `0.YYMMDD.N` (build date and a counter) until the first shippable release, 1.0. To connect an AI, see [Connect an AI](docs/user/CONNECT_AN_AI.md), which also has the command that lets Claude Desktop or Claude Code read your book.

What the app does today, with all numbers worked out by the core and shown in your regional format:

- **Your books.** KassenSturz keeps your book in its own folder and opens another one on request. It says so when a book is open on another computer or lies on a network drive. It saves backups automatically.
- **Starting.** A first start that asks for your currency and sets up a default set of categories, or a new book made from a file: a KMyMoney book (`.kmy`, or SQLite of layouts 12 to 15) or a whole-book file saved by KassenSturz.
- **Accounts and categories.** Accounts side by side with their balances (in several currencies), debts shown as "You owe", categories as a tree, starting balances, and "Check against my bank" to see why your balance differs from your bank's.
- **Transactions.** A register you can sort, transactions broken down into Parts, Transfers between your own accounts (suggested, and joined once you confirm), and undo, redo and a history of changes.
- **Overview.** One month's money in and out, where it came from and where it went, and what is left over.
- **Import.** Bank files (CSV with a screen for choosing the columns, CAMT.053, MT940, OFX) with a check for duplicates and a check against the statement's balance.
- **Rules.** Rules that sort transactions by what they say, with plain-text matching by default, a suggestion when you keep sorting the same payee by hand, and a preview before a changed Rule sorts again.
- **Saving.** Transactions as CSV, and "Save the whole book" in an open format that KassenSturz can read back.
- **Languages.** English and German.

The book's layout is now one starting layout (layout 16, D-045). A book made by an earlier development build is turned away with a pointer to its complete export, so save one with the old build before you install a newer one. Try the app with made-up numbers only, never your own (AGENTS.md §3.1 rule 2).

## Trying a build from CI

Every pull request and every push to `main` that changes code builds an installable Flatpak bundle, `KassenSturz.flatpak` (D-068). To try it, nothing has to be built on your computer. A pull request that only changes documentation builds none (D-110).

Once per computer, add Flathub and install the KDE runtime the app runs on:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.kde.Platform//6.11
```

Then, for each build you want to try:

1. Open the pull request's **Checks** tab (or the CI run on `main`), choose the run of the **CI** workflow, and download the artifact `KassenSturz-x86_64.flatpak` under **Artifacts** at the bottom of its summary. Downloading needs a GitHub login; GitHub keeps an artifact for 90 days.
2. Unpack the downloaded zip file and install the bundle in it. `--reinstall` replaces a build installed before:

   ```sh
   unzip KassenSturz-x86_64.flatpak.zip
   flatpak install --user --reinstall KassenSturz.flatpak
   flatpak run io.github.vondor.KassenSturz
   ```

The app also appears in the app menu. Books and settings stay in `~/.var/app/io.github.vondor.KassenSturz/` when a build is replaced or removed; until 0.1, a development build may not open a book made by a later one, and a book from before layout 16 is turned away (D-045). Try builds with synthetic data only, never with your own (AGENTS.md §3.1 rule 2). To remove the build: `flatpak uninstall --user io.github.vondor.KassenSturz`.

## Building

KassenSturz is developed, tested and packaged in one environment: the KDE Flatpak SDK with the Rust extension (D-063). Install it from Flathub:

```sh
flatpak install flathub org.kde.Sdk//6.11 org.kde.Platform//6.11 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08 org.flatpak.Builder
```

`tools/sdk.sh` runs any command in that environment, the same way CI does:

```sh
tools/sdk.sh cargo fmt --all
tools/sdk.sh cargo clippy --workspace --all-targets -- -D warnings
tools/sdk.sh cargo test --workspace
tools/sdk.sh cargo run --bin kassensturz    # starts the app
```

`kassensturz-cli`, a command-line tool for trying out the core without the app (D-085), works on a book file of your choice. It is for developers and testers, writes English only and isn't installed with the app:

```sh
export KASSENSTURZ_BOOK=/tmp/Household.ksz
tools/sdk.sh cargo run --bin kassensturz-cli -- help
tools/sdk.sh cargo run --bin kassensturz-cli -- create
tools/sdk.sh cargo run --bin kassensturz-cli -- account add "Checking account" --type checking --currency EUR
tools/sdk.sh cargo run --bin kassensturz-cli -- category add Groceries --kind expense --currency EUR
tools/sdk.sh cargo run --bin kassensturz-cli -- transaction add --date 2026-10-05 --account "Checking account" --amount -53.20 --category Groceries
tools/sdk.sh cargo run --bin kassensturz-cli -- register "Checking account"
tools/sdk.sh cargo run --bin kassensturz-cli -- transfer suggest
tools/sdk.sh cargo run --bin kassensturz-cli -- rule add Supermarket --if "payee contains market" --category Groceries
tools/sdk.sh cargo run --bin kassensturz-cli -- rule run --dry-run
tools/sdk.sh cargo run --bin kassensturz-cli -- undo
```

`cargo run` shows the text in English: the translations are found only once they are installed, as in the Flatpak.

Build and install the Flatpak, offline as on Flathub, and start it (it also appears in the app menu):

```sh
flatpak run org.flatpak.Builder --user --install --force-clean --disable-rofiles-fuse \
    --repo=build/flatpak-repo build/flatpak packaging/flatpak/io.github.vondor.KassenSturz.yml
flatpak run io.github.vondor.KassenSturz
```

When `Cargo.lock` changes, regenerate the Flatpak's crate list with `flatpak-cargo-generator.py` from [flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools) (Python with `aiohttp` and `tomlkit`):

```sh
python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
```

When strings in the UI change, update the translations with `tools/sdk.sh tools/update-translations.sh` and translate what it leaves empty; `tools/sdk.sh tools/check-translations.sh` (run by CI) fails while a translation is missing or left over (D-126).

When Debian's [iso-codes](https://salsa.debian.org/iso-codes-team/iso-codes) has a new release, download it and update KassenSturz's currency table with `python3 tools/update-currency-table.py VERSION iso-codes-VERSION/data/iso_4217.json`. A currency it adds needs its decimal places from ISO 4217, given once as `CODE=DIGITS` after the file (D-077 in [`docs/DECISIONS.md`](docs/DECISIONS.md)).

Building without Flatpak works too, with CMake, Extra CMake Modules, Qt 6.11, KDE Frameworks 6.29 and Rust 1.98 or newer: `cmake -B build/cmake && cmake --build build/cmake`. CI also builds it on Kubuntu 26.10 (D-094) and Fedora 44 (D-097) from the distributions' packages. Bank connections need AqBanking 6.9.2 and Gwenhywfar 5.14.1 or newer at runtime, found by their ABI names (D-138); the dialogs for bank setup, PIN and TAN are built when Gwenhywfar's Qt 6 GUI development package (`libgwengui-qt6-dev` on Kubuntu) is installed, and the app works without any of them.

## Developer mode

For testing, especially with different AI services, KassenSturz can write a debug log (D-227). It is not offered or mentioned anywhere in the app; it starts only when the app is started from a terminal with an environment variable:

```sh
KASSENSTURZ_DEBUG_LOG=1 tools/sdk.sh cargo run --bin kassensturz
flatpak run --env=KASSENSTURZ_DEBUG_LOG=1 io.github.vondor.KassenSturz
```

To read the log with your own AI tool as well (see [Connect an AI](docs/user/CONNECT_AN_AI.md)), start the MCP server with the same variable: `flatpak run --env=KASSENSTURZ_DEBUG_LOG=1 --command=kassensturz-mcp io.github.vondor.KassenSturz`. In the AI tool's MCP settings that is command `flatpak` with the arguments `run`, `--env=KASSENSTURZ_DEBUG_LOG=1`, `--command=kassensturz-mcp`, `io.github.vondor.KassenSturz` (Flatpak takes its options, in any order, before the app's name). The app has to be started with the variable too, as above: it writes the log, and the MCP server lets your AI tool read it.

While it runs, the window title ends in "developer mode" and a banner above every page says so, with buttons to show and to delete the log. The log is `~/.var/app/io.github.vondor.KassenSturz/data/io.github.vondor.KassenSturz/debug-log/current.jsonl` in the Flatpak (`$XDG_DATA_HOME/io.github.vondor.KassenSturz/debug-log/` otherwise), at most about 512 KiB. Each line holds a time (to the millisecond), an event name from a fixed list, a function, tool or screen name from a fixed list and an error code, never an amount, a name, a text from the book or a secret (the transcript below is the exception). While AI is also switched on, the chat and the MCP server (started with the same variable) let the AI read the log with `read_debug_log`. **The chat transcript (D-235):** next to it, `chat.jsonl` (and `chat-older.jsonl` after 4 MiB) holds the chat in full, one JSON line per step, each with a millisecond time and the `turn` shared by everything that follows from one question: `asked` (the question as typed), `request` (the exact body sent to the model, with the system prompt and the book data in it), `reply` (the provider's answer as it came, before it was read, and the error code when it couldn't be, which shows why an `ai.bad-reply` happened), `called` (a function with arguments and result), `answer`, `failed` and the outcome of change sets, written as one plain line of words (what each change does). The outcome of changes one of your own AI tools prepared carries the turn `mcp`, since it follows from no question of the chat. It is what the chat's "What the AI looked at" tab shows, plus the raw replies, and it is the file to send when a model misbehaves. Nothing reads it back, not the app, not the AI and not the issue drafts; "Delete the log" removes it. Anything shaped like an API key or token is blanked, and the provider's key is never part of a request. Your privacy can't be guaranteed while this mode runs: use it with synthetic books only. To end it, close KassenSturz and start it the usual way.

**Timing a screen:** `screen.requested` is written when the app starts making a screen (after a click on a menu entry, an account or a button), `screen.shown` when it is there, and for the register `screen.ready` when its lines were read and drawn. The difference between the times is how long each step took; no names or amounts are logged.

**What the user did:** `ui.action-taken` is written when a handler of a button, menu entry, switch, choice or confirmation runs, and its detail names the action (`RulesPage_addRule`; the names are in `crates/kassensturz-core/src/vocabulary/actions.rs`). A click that is logged here but changes nothing failed after the handler started; a click that is not logged never reached its handler. Every such handler in the QML must say so (`ActionLog.taken("<File>_<what>")` on its first line); `tools/check-action-log.sh` and the test `action_log_audit` fail on one that doesn't.

**A KMyMoney report's period:** `import.period-fixed` is written once for each report whose relative period became fixed days when a book was made from a KMyMoney file (M6d.6a). Its detail is KMyMoney's word for the period (`last30days`) and `days` holds the day of the import, then the first and the last day (`2026-11-01:2026-10-02..2026-11-01`), so the days can be compared with what KMyMoney shows; nothing else of the report is written.

Distributions can leave the log code out with the CMake option `-DKASSENSTURZ_DEBUG_LOG=OFF` (Cargo feature `debug-log`).

### Typing the FinTS registration number

Real banks only talk to programs registered with the German banks' FinTS system (D-058). KassenSturz builds carry no registration number: its terms don't allow the number to be passed on, so it is in no source file, build or setting (D-260). Whoever holds the number and tests against a real bank starts the app with a switch:

```sh
KASSENSTURZ_FINTS_ASK=1 tools/sdk.sh cargo run --bin kassensturz
flatpak run --env=KASSENSTURZ_FINTS_ASK=1 io.github.vondor.KassenSturz
```

The first time KassenSturz is about to contact a bank (setting up a bank login, or fetching), it asks for the 25-character product registration number, in a field that shows dots. It keeps the number in memory until it closes and never saves, shows or logs it; it goes only to the bank, as the program's registration. "Cancel" contacts no bank and asks again next time. A number that isn't 25 letters and digits is asked for again; a number of the right shape that the bank refuses needs KassenSturz to be closed and started again to type it anew. Without the switch the bank screens say that this build isn't registered.

## For AI agents

Read [`AGENTS.md`](AGENTS.md) first. It defines the mission, boundaries and working rules.

## Documents

- [`AGENTS.md`](AGENTS.md): rules for AI agents (read first)
- [`docs/DECISIONS.md`](docs/DECISIONS.md): decisions and open questions
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): repository and module layout
- [`docs/ROADMAP.md`](docs/ROADMAP.md): milestones
- [`docs/GLOSSARY.md`](docs/GLOSSARY.md): plain-language UI terms and what they mean internally
- [`docs/BOOK_FORMAT.md`](docs/BOOK_FORMAT.md): how a household book is stored, for programs that read it
- [`docs/ERROR_CODES.md`](docs/ERROR_CODES.md): every error code with its parameters, for the UI and translators
- [`docs/THIRD_PARTY.md`](docs/THIRD_PARTY.md): third-party dependencies, licenses and reused code
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md): what KassenSturz protects, from whom, and how
- [`SECURITY.md`](SECURITY.md): how to report a vulnerability privately
- [`docs/proposals/`](docs/proposals/): proposals waiting for a decision

## Contributing

Sign off every commit (`git commit -s`) to confirm the [Developer Certificate of Origin](https://developercertificate.org/): you wrote the change, or have the right to submit it under the license of the files it changes. That includes code made with AI tools. See D-040 in [`docs/DECISIONS.md`](docs/DECISIONS.md).

## Publisher

KassenSturz is published by [SerpentSoftware](https://serpentsoftware.de).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/branding/serpentsoftware-mark-dark.svg">
  <img src="docs/branding/serpentsoftware-mark.svg" alt="SerpentSoftware logo" width="64">
</picture>

## License

Code: GPL-3.0-or-later, see [`LICENSE`](LICENSE). The SerpentSoftware logo is the publisher's brand mark and is not covered by these licenses (`LicenseRef-SerpentSoftware-Logo`): copy it unmodified with the project, but remove it if you publish a fork under your own name. Documentation: CC-BY-SA-4.0. CMake files: BSD-2-Clause. Small configuration files and templates: CC0-1.0. [`REUSE.toml`](REUSE.toml) and the [`LICENSES/`](LICENSES/) folder say which license covers which file (D-064 in [`docs/DECISIONS.md`](docs/DECISIONS.md)).
