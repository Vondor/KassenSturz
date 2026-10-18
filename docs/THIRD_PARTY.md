# Third-Party Dependencies and Code

Tracks every third-party dependency (license, purpose, compatibility) and every piece of reused code, per AGENTS.md §3.1 rules 5 and 6. Nothing is added to `Cargo.toml`, or copied from another project, until its row here says so.

## Dependencies

| Name | Used for | License | Compatible with GPL-3.0-or-later? | Status |
|---|---|---|---|---|
| AqBanking | Direct bank connections (D-012 in `docs/DECISIONS.md`): FinTS/HBCI, including credit card statements (FinTS job DKKKU), EBICS, OFX Direct Connect, PayPal | `GPL-2.0-only OR GPL-3.0-only`. COPYING in the 6.9.2 release tarball: "AqBanking is licensed under the GPLv2 or GPLv3"; contributions from a listed group of authors are under the Modified BSD license, and the source files defer to COPYING. Exception: the headers of the ERI2 importer point to a COPYING file in its own folder that the tarball doesn't contain | Yes, under the GPL-3.0 option. That option has no "or later", so a build that includes AqBanking is distributable as a whole under GPL-3.0 only (D-001). Leave the ERI2 importer out of the build (KassenSturz has its own importers) or clarify its license upstream | License verified 2026-10-18 (see the log below); not yet added: the binding approach and the maintainer's approval are still pending (Q-009) |
| Gwenhywfar | Support library that AqBanking requires | `LGPL-2.1-or-later` (source headers: "version 2.1 of the License, or (at your option) any later version"), plus an extra permission to distribute it with OpenSSL; one file is public domain. The earlier note "dual GPL-2.0-only / GPL-3.0-only", taken from distro metadata, was wrong for Gwenhywfar | Yes | License verified 2026-10-18 (see the log below); not yet added (Q-009) |
| `regex` (rust-lang) | Regex matching for Rule conditions (see D-014/D-015 in `docs/DECISIONS.md`) | Dual MIT / Apache-2.0 | Yes — permissive, no conflict | Planned, not yet added as a Cargo dependency |

## Verification log

| Date | Checked | Source | SHA-256 of the file checked |
|---|---|---|---|
| 2026-10-18 | AqBanking 6.9.2 (released 2026-05-22): COPYING and the license notes in the source files | `https://www.aquamaniac.de/rdm/attachments/download/654/aqbanking-6.9.2.tar.gz` | `244b5f7a139f829928f5cdc3f5f7488517b3e8aa63625a92741c3efc1892bb3f` |
| 2026-10-18 | Gwenhywfar 5.14.1 (released 2025-12-07): COPYING and the license notes in the source files | `https://www.aquamaniac.de/rdm/attachments/download/630/gwenhywfar-5.14.1.tar.gz` | `8916feaa99cb954f963f2cba8dd2dffe57cacf7f284daf00eab071aad6fe2ab3` |

The GPG signatures (the `.asc` files next to each tarball) were not checked. Also checked on 2026-10-18: crates.io has no AqBanking crate, so KassenSturz needs its own binding (Q-009).

## Reused code (AGENTS.md §3.1 rule 5)

None yet. Any code adapted from KMyMoney, GnuCash, LibreOffice or another project must be recorded here with its origin, license, and the maintainer's approval, before it lands in this repo.
