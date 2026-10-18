# Third-Party Dependencies and Code

Tracks every third-party dependency (license, purpose, compatibility) and every piece of reused code, per AGENTS.md §3.1 rules 5 and 6. Nothing is added to `Cargo.toml`, or copied from another project, until its row here says so.

## Dependencies

| Name | Used for | License | Compatible with GPL-3.0-or-later? | Status |
|---|---|---|---|---|
| AqBanking (+ Gwenhywfar) | Direct bank connections: HBCI/FinTS, EBICS (see D-012 in `docs/DECISIONS.md`) | Dual GPL-2.0-only / GPL-3.0-only, per distro packaging metadata for the 6.8.x / 5.14.x releases (Nov–Dec 2025) | Yes, if built under the GPLv3 branch of the dual license — **not yet confirmed against the upstream COPYING/README in the release tarball** | Planned, not yet added as a Cargo dependency (blocked on Q-009) |
| `regex` (rust-lang) | Regex matching for Rule conditions (see D-014/D-015 in `docs/DECISIONS.md`) | Dual MIT / Apache-2.0 | Yes — permissive, no conflict | Planned, not yet added as a Cargo dependency |

## Reused code (AGENTS.md §3.1 rule 5)

None yet. Any code adapted from KMyMoney, GnuCash, LibreOffice or another project must be recorded here with its origin, license, and the maintainer's approval, before it lands in this repo.
