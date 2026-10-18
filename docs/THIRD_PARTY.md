# Third-Party Dependencies and Code

Tracks every third-party dependency (license, purpose, compatibility) and every piece of reused code, per AGENTS.md §3.1 rules 5 and 6. Nothing is added to `Cargo.toml`, or copied from another project, until its row here says so. Direct dependencies are approved one by one; indirect and test-only dependencies are covered by the license allowlist below (D-026 in `docs/DECISIONS.md`).

## Dependencies

| Name | Used for | License | Compatible with GPL-3.0-or-later? | Status |
|---|---|---|---|---|
| AqBanking | Direct bank connections (D-012 in `docs/DECISIONS.md`): FinTS/HBCI with PIN/TAN or chip card, including credit card statements (FinTS job DKKKU); no key files and so no EBICS (D-059); OFX Direct Connect and PayPal only if their credentials can stay in the keychain. Read-only: balances and statements, never payments (D-053) | `GPL-2.0-only OR GPL-3.0-only`. COPYING in the 6.9.2 release tarball: "AqBanking is licensed under the GPLv2 or GPLv3"; contributions from a listed group of authors are under the Modified BSD license, and the source files defer to COPYING. Exception: the headers of the ERI2 importer point to a COPYING file in its own folder that the tarball doesn't contain | Yes, under the GPL-3.0 option. That option has no "or later", so a build that includes AqBanking is distributable as a whole under GPL-3.0 only (D-001). Leave the ERI2 importer out of the build (KassenSturz has its own importers) or clarify its license upstream | License verified 2026-10-18 (see the log below); approved through D-012 and D-054; binding decided (D-060); integrated in M2 |
| Gwenhywfar | Support library that AqBanking requires, including its Qt 6 dialog library `libgwengui-qt6` for bank setup, PIN and TAN entry (D-054) | `LGPL-2.1-or-later` (source headers: "version 2.1 of the License, or (at your option) any later version"), plus an extra permission to distribute it with OpenSSL; one file is public domain. The Qt dialog code defers to the same COPYING (LGPL). The earlier note "dual GPL-2.0-only / GPL-3.0-only", taken from distro metadata, was wrong for Gwenhywfar | Yes | License verified 2026-10-18 (see the log below); approved through D-012 and D-054; integrated in M2 |
| `regex` (rust-lang) | Regex matching for Rule conditions (see D-014/D-015 in `docs/DECISIONS.md`) | Dual MIT / Apache-2.0 | Yes — permissive, no conflict | Planned, not yet added as a Cargo dependency |
| Qt 6 | UI toolkit (D-005); Qt Charts or Qt Graphs for the first `ChartProvider` (D-008) | Most modules: LGPL-3.0 (or commercial). Some modules are GPL-3.0 only (or commercial), among them **Qt Charts and Qt Graphs** (doc.qt.io, checked 2026-10-18) | Yes. A build that includes Qt Charts or Qt Graphs is distributable as a whole under GPL-3.0 only (D-001) | Proposed, awaiting maintainer approval; versions per D-025 (LTS preferred) |
| Kirigami (KDE Frameworks 6) | QML UI components (D-005) | LGPL-2.0-or-later (develop.kde.org) | Yes | Proposed, awaiting maintainer approval |
| KI18n (KDE Frameworks 6) | Translations with gettext (D-034) | Upstream `LICENSES/` holds LGPL-2.0-or-later, LGPL-2.1-only, LGPL-3.0-only, LicenseRef-KDE-Accepted-LGPL, BSD-3-Clause, CC0-1.0 and ODbL-1.0 | Yes for the LGPL, BSD and CC0 parts. ODbL-1.0 is a database license: check what it covers before approving | Proposed, awaiting maintainer approval |
| `cxx-qt` (KDAB) | Rust ↔ Qt/QML binding (D-005) | MIT OR Apache-2.0 | Yes | Proposed, awaiting maintainer approval; newest release 0.10.0 (2026-08-24) |
| `rusqlite` | SQLite access from Rust (D-005, D-016); SQLite itself is public domain | MIT | Yes | Proposed, awaiting maintainer approval; newest release 0.40.2 (2026-08-08); bundled or system SQLite to be decided when it is added |
| `wasmtime` (Bytecode Alliance) | Host for WASM plugins (D-007) | Apache-2.0 WITH LLVM-exception | Yes (Apache-2.0 is compatible with GPL-3.0) | Proposed, awaiting maintainer approval; newest release 49.0.3 (2026-10-14) |
| `rust_decimal` | Exact fixed-point decimals for exchange rates, prices and quantities (D-051) | MIT | Yes | **Approved by the maintainer** (Q-027, 2026-10-18); not yet added; newest release 1.43.0 (2026-09-02), version pinned when it is added in M1 |
| `bindgen` (rust-lang) | Developer tool that generates the AqBanking/Gwenhywfar declarations once; the output is committed (D-060) | BSD-3-Clause | Yes | Developer tool only, not a build dependency; newest release 0.73.2 (2026-09-08) |
| `libloading` | Loading AqBanking at runtime, so KassenSturz works without it (D-054, D-060) | ISC | Yes | Proposed; the maintainer approves it when M2 starts; newest release 0.9.0 (2025-11-05) |

## License allowlist for indirect and test-only dependencies (D-026)

*Proposed, awaiting the maintainer's approval.* CI (e.g. cargo-deny) fails if an indirect or test-only Rust crate uses a license outside this list; anything else needs individual approval, like a direct dependency. Changing the list is itself a dependency change (AGENTS.md §3.2).

`MIT` · `Apache-2.0` · `Apache-2.0 WITH LLVM-exception` · `BSD-2-Clause` · `BSD-3-Clause` · `ISC` · `Zlib` · `0BSD` · `BSL-1.0` · `CC0-1.0` · `Unlicense` · `Unicode-3.0` · `MPL-2.0` (unless a file is marked "Incompatible With Secondary Licenses")

Copyleft licenses (GPL, LGPL, AGPL) are deliberately left off: each such crate is approved individually, because some of them (for example GPL-3.0-only) remove the "or later" option for the whole build (D-001). The list covers Rust crates only; C and C++ libraries such as Qt, KDE Frameworks, AqBanking and Gwenhywfar always get their own row above.

## Verification log

| Date | Checked | Source | SHA-256 of the file checked |
|---|---|---|---|
| 2026-10-18 | AqBanking 6.9.2 (released 2026-05-22): COPYING and the license notes in the source files | `https://www.aquamaniac.de/rdm/attachments/download/654/aqbanking-6.9.2.tar.gz` | `244b5f7a139f829928f5cdc3f5f7488517b3e8aa63625a92741c3efc1892bb3f` |
| 2026-10-18 | Gwenhywfar 5.14.1 (released 2025-12-07): COPYING and the license notes in the source files | `https://www.aquamaniac.de/rdm/attachments/download/630/gwenhywfar-5.14.1.tar.gz` | `8916feaa99cb954f963f2cba8dd2dffe57cacf7f284daf00eab071aad6fe2ab3` |

The GPG signatures (the `.asc` files next to each tarball) were not checked. Also checked on 2026-10-18: crates.io has no AqBanking crate, so KassenSturz needs its own binding (D-060).

## Reused code (AGENTS.md §3.1 rule 5)

None yet. Any code adapted from KMyMoney, GnuCash, LibreOffice or another project must be recorded here with its origin, license, and the maintainer's approval, before it lands in this repo.
