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

Pre-alpha. The walking skeleton (D-068 in [`docs/DECISIONS.md`](docs/DECISIONS.md)) runs through every layer: one account's transactions, stored in SQLite and shown in a Kirigami window, with every amount formatted in Rust. It shows made-up sample data only and saves nothing.

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

When strings in the UI change, update the translations with `tools/sdk.sh tools/update-translations.sh`.

When SIX publishes a new ISO 4217 list, download list one and update KassenSturz's currency table with `python3 tools/update-currency-table.py list-one.xml` (D-077 in [`docs/DECISIONS.md`](docs/DECISIONS.md)).

Building without Flatpak works too, with CMake, Extra CMake Modules, Qt 6.11, KDE Frameworks 6.30 and Rust 1.98 or newer: `cmake -B build/cmake && cmake --build build/cmake`.

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
- [`docs/proposals/`](docs/proposals/): proposals waiting for a decision

## Contributing

Sign off every commit (`git commit -s`) to confirm the [Developer Certificate of Origin](https://developercertificate.org/): you wrote the change, or have the right to submit it under the license of the files it changes. That includes code made with AI tools. See D-040 in [`docs/DECISIONS.md`](docs/DECISIONS.md).

## License

Code: GPL-3.0-or-later, see [`LICENSE`](LICENSE). Documentation: CC-BY-SA-4.0. CMake files: BSD-2-Clause. Small configuration files and templates: CC0-1.0. [`REUSE.toml`](REUSE.toml) and the [`LICENSES/`](LICENSES/) folder say which license covers which file (D-064 in [`docs/DECISIONS.md`](docs/DECISIONS.md)).
