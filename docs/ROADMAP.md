# Roadmap

Status values: `todo` · `doing` · `done`. Agents update the status of what they work on.

| Milestone | Scope | Status |
|---|---|---|
| **M0 Bootstrap** | Cargo workspace, CI (Linux first), formatting/lint, SPDX header checks, `docs/THIRD_PARTY.md`, `docs/GLOSSARY.md` | todo |
| **M1 Core** | `Money` + rounding modes + property tests, currencies and rates, accounts (multiple per book, D-021), double-entry transactions and splits (editable Parts after the fact, auto-remainder calculation, must-balance validation per D-017), Transfer-matching heuristic between the user's own accounts (D-021), SQLite store with migrations, `kashflow-cli`; auto-categorization Rule engine (conditions, actions, Rule tree with sub-Rules, regex via `regex` crate) per D-014/D-015 | todo |
| **M2 Import & bank connections** | CSV, KMyMoney (`.kmy` and SQLite), CAMT.053, MT940, OFX; golden-file tests with synthetic data; direct bank connections via **AqBanking** (HBCI/FinTS, EBICS) once Q-009 is resolved; Rules applied automatically to newly imported transactions; Transfer matches proposed automatically on import (D-021) | todo |
| **M3 UI MVP** | Kirigami UI: first-run flow, accounts shown side by side (D-021), transaction register, simple overview excluding Transfers from income/expense. Plain-language review of all strings; Rule editor (plain-text matching by default, regex as an advanced opt-in per condition); "create Rule from this transaction" shortcut; "break down this purchase" editor for Parts (D-017); Transfer-match confirmation prompt | todo |
| **M4 Plugins** | Plugin API v1, manifest and permissions, WASM host, one example plugin per extension point | todo |
| **M5 AI** | `kashflow-ai` provider abstraction (local + cloud), consent and redaction, change-set review UI, read-only `kashflow-mcp`; hard AI-off switch (hidden UI, no port bound) per D-013; AI memory — book context document + pattern memory stored in `kashflow-store`, viewable/editable/deletable by the user, used to propose new Rules or saved reports (D-023) | todo |
| **M6 Charts and reports** | `ChartProvider`, Qt-based charts, basic reports; LibreOffice chart prototype (Q-005) | todo |
| **M7 Packaging** | Flatpak (Plasma + GNOME), Windows, macOS; CI for all four platforms | todo |
| **Later (post-1.0)** | Budgeting extras, optional at-rest encryption (Q-007), additional SQL storage backends (PostgreSQL, MySQL/MariaDB, ...) behind the existing storage trait (D-016, Q-012), receipt capture and auto-matching via `kashflow-receipts` plus a separate native Android companion app via `kashflow-mobile-bridge` (D-018, D-019, Q-015/Q-017/Q-018), multi-user support — Members, per-Account sharing, shared-host/thin-client sync via `kashflow-sync` (D-022, Q-021/Q-022/Q-023) | todo |

## How to pick work

1. Take the lowest-numbered milestone that still has open items.
2. Do not start a milestone that depends on unfinished decisions listed in `docs/DECISIONS.md`. Use the safe default there and mark it.
3. Cross-platform CI should exist before M3 grows large, so problems on lower-priority platforms show up early.
