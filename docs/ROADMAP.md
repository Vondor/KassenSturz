# Roadmap

Status values: `todo` · `doing` · `done`. Agents update the status of what they work on.

| Milestone | Scope | Status |
|---|---|---|
| **M0 Bootstrap** | Cargo workspace, CI (Linux first), formatting/lint, SPDX header checks, `docs/THIRD_PARTY.md`, `docs/GLOSSARY.md` | todo |
| **M1 Core** | `Money` + rounding modes + property tests, currencies and rates, accounts, double-entry transactions and splits, SQLite store with migrations, `ledger-cli` | todo |
| **M2 Import** | CSV, KMyMoney (`.kmy` and SQLite), CAMT.053, MT940, OFX; golden-file tests with synthetic data | todo |
| **M3 UI MVP** | Kirigami UI: first-run flow, accounts, transaction register, simple overview. Plain-language review of all strings | todo |
| **M4 Plugins** | Plugin API v1, manifest and permissions, WASM host, one example plugin per extension point | todo |
| **M5 AI** | `ledger-ai` provider abstraction (local + cloud), consent and redaction, change-set review UI, read-only `ledger-mcp` | todo |
| **M6 Charts and reports** | `ChartProvider`, Qt-based charts, basic reports; LibreOffice chart prototype (Q-005) | todo |
| **M7 Packaging** | Flatpak (Plasma + GNOME), Windows, macOS; CI for all four platforms | todo |
| **Later** | FinTS / bank connections, budgeting extras, optional at-rest encryption (Q-007) | todo |

## Rules for picking work

1. Take the lowest-numbered milestone that still has open items.
2. Do not start a milestone that depends on unfinished decisions listed in `docs/DECISIONS.md`. Use the safe default there and mark it.
3. Cross-platform CI should exist before M3 grows large, so problems on lower-priority platforms show up early.
