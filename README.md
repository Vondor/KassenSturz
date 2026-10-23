# KassenSturz

A free, open-source personal finance app for the desktop, **simpler than KMyMoney and GnuCash**, made for everyday people who have never taken an economics class.

## Goals

- Simple, plain-language interface with progressive disclosure
- Correct money handling with multi-currency support and European standards
- Local-first and private. Your data stays on your machine
- Open plugin system (WASM)
- Native AI connection, including locally running models, with the user always in control
- First-class on **KDE Plasma**, then **GNOME**, **Windows**, **macOS**
- Built the KDE way, with the goal of becoming an official KDE project

## Status

Pre-alpha. The core and the book file (milestones M1 and M2 in [`docs/ROADMAP.md`](docs/ROADMAP.md)) and the first usable app (M3) are in place; the first release, 0.1, is not tagged yet. Bank connections (M4) and AI features (M5) come later.

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

Building without Flatpak works too, with CMake, Extra CMake Modules, Qt 6.11, KDE Frameworks 6.29 and Rust 1.98 or newer: `cmake -B build/cmake && cmake --build build/cmake`. CI also builds it on Kubuntu 26.10 (D-094) and Fedora 44 (D-097) from the distributions' packages.

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

## License

Code: GPL-3.0-or-later, see [`LICENSE`](LICENSE). Documentation: CC-BY-SA-4.0. CMake files: BSD-2-Clause. Small configuration files and templates: CC0-1.0. [`REUSE.toml`](REUSE.toml) and the [`LICENSES/`](LICENSES/) folder say which license covers which file (D-064 in [`docs/DECISIONS.md`](docs/DECISIONS.md)).
