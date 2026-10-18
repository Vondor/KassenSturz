# Decisions and Open Questions

Format: ID · Status · Decision · Reason. Agents may *propose* changes; the maintainer decides (AGENTS.md §3.2).
Status values: **Decided** (set by the maintainer) · **Proposed** (agent default, awaiting confirmation) · **Open**.

## Decisions

| ID | Status | Decision |
|---|---|---|
| D-000 | Proposed | Working title is **Ledger**. Crates use the `ledger-` prefix. Rename in one pass when a final name is chosen. |
| D-001 | Decided / Proposed | License is **GPL 3.0**. Proposed SPDX identifier: `GPL-3.0-or-later` (see Q-001). |
| D-002 | Decided | **Money type and rounding rules are user-selectable.** The backend follows European standards: ISO 4217 currencies, integer minor units internally, locale-based display. |
| D-003 | Decided | **Database schema is oriented on KMyMoney** (concepts: accounts, transactions, splits, payees, institutions, securities, prices, schedules). Clean-room: no copied source, no blind copy of table layout. A KMyMoney importer is provided. |
| D-004 | Decided | **Multi-currency is supported from the start.** Original amounts are never discarded; rate, date and source are recorded. |
| D-005 | Proposed | Core in **Rust**; UI in **Qt 6 / QML / Kirigami** via cxx-qt; storage in **SQLite** with migrations. |
| D-006 | Decided | Platform priority: **KDE Plasma → GNOME → Windows → macOS**. |
| D-007 | Proposed | Plugins: **WASM** primary, **out-of-process JSON-RPC** secondary. Versioned API, permission manifest. |
| D-008 | Decided (later) | **LibreOffice chart code** is wanted later. Until then charts sit behind a `ChartProvider` interface with a Qt-based implementation. See Q-005. |
| D-009 | Decided | **Native AI connection**, including **locally running models**. Local-first, off by default, user confirms changes, local MCP server. See AGENTS.md §8. |
| D-010 | Decided | **Mission:** simpler than KMyMoney and GnuCash, for everyday users with no economics background. |
| D-011 | Decided | **Development is AI-driven.** Humans hold the gates listed in AGENTS.md §3.2. |

## Open Questions

| ID | Question | Safe default until answered |
|---|---|---|
| Q-001 | `GPL-3.0-or-later` or `GPL-3.0-only`? "-or-later" is the more common choice in the KDE ecosystem and eases future compatibility. Hard to change once others contribute. | Use `GPL-3.0-or-later` in headers |
| Q-002 | Final project name | Keep "Ledger" |
| Q-003 | Multi-currency balancing: dedicated trading accounts (GnuCash-style) or per-split price/rate (KMyMoney-style)? | Per-split price/rate, with original amounts preserved |
| Q-004 | Default rounding mode for new users | Round half away from zero ("commercial rounding"); user-changeable. Verify against applicable euro-conversion rules before release |
| Q-005 | Feasibility and license handling of reusing LibreOffice chart code (chart2). It is MPL-2.0 (generally combinable with GPL-3.0) but tightly coupled to LibreOffice's UNO framework. Prototype in a separate branch before any commitment. | Qt-based `ChartProvider` |
| Q-006 | License check of KMyMoney source files (GPL-2.0-or-later vs GPL-2.0-only) before any code reuse. GPL-2.0-only code cannot go into a GPL-3.0 project. | No code reuse |
| Q-007 | Optional at-rest encryption of the data file (e.g. SQLCipher)? | Unencrypted SQLite, plus documented OS-level disk encryption |
| Q-008 | Backup, export and sync story (no hosted sync is planned) | Manual backup/export plus automatic local backups before migrations |
