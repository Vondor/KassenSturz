# Contributing

Thanks for looking at KassenSturz. The real contributing guide is **[AGENTS.md](AGENTS.md)** —
it's written for AI coding agents, but every rule in it (mission, boundaries, domain rules,
how to work here) applies to human contributors too. This file only adds the short,
GitHub-specific checklist; it doesn't duplicate AGENTS.md's rules.

## Before you open a pull request

1. Read `AGENTS.md`, then `docs/DECISIONS.md`, `docs/ROADMAP.md` and `docs/ARCHITECTURE.md`.
2. Pick one small, single-purpose piece of work, ideally one roadmap item.
3. Use [Conventional Commits](https://www.conventionalcommits.org/) and explain **why**, not just what.
4. **Sign off every commit under the DCO**: `git commit -s` (see [D-040](docs/DECISIONS.md)).
   A pull request whose commits aren't signed off will fail CI and can't be merged. The sign-off has to
   be by the commit's author or committer, which `git commit -s` takes care of. If an AI agent made your
   commits, it won't sign them off for you: review them, then sign them off with the `git rebase --signoff`
   command that the failing check prints.
   (The maintainer's own pull requests are the exception for agent commits, see D-040.)
5. Add tests for anything in `kassensturz-core` or other logic crates (AGENTS.md §9.4).
6. Run formatting, lint and tests locally before opening the pull request, in the same
   environment CI uses (see "Building" in [README.md](README.md)):
   `tools/sdk.sh cargo fmt --all`, `tools/sdk.sh cargo clippy --workspace --all-targets -- -D warnings`
   and `tools/sdk.sh cargo test --workspace`.
7. Make sure every new source file carries an SPDX header
   (`SPDX-License-Identifier: GPL-3.0-or-later`, `SPDX-FileCopyrightText: <year> KassenSturz contributors`)
   — checked by `reuse lint` in CI (D-024). CMake files use `BSD-2-Clause` instead; documentation and small
   configuration files are covered by `REUSE.toml` (D-064).

## What needs the maintainer's sign-off first

Some changes are proposed by anyone but **decided only by the maintainer** — see AGENTS.md §3.2.
That includes dependency changes, schema migrations, anything touching keychain/crypto/bank
connections/AI data sharing, and merging to `main` itself. If you're unsure whether your change
falls into one of these, say so in the pull request description and ask.

## Reporting a security issue

See [SECURITY.md](SECURITY.md) once it exists (scheduled before the bank-connection milestone, D-056).
Until then, please open a private report via GitHub's security advisories rather than a public issue.

## License

By contributing, you agree your contribution is licensed under the license of the files it changes:
GPL-3.0-or-later for code (see [LICENSE](LICENSE)), CC-BY-SA-4.0 for documentation, BSD-2-Clause for
CMake files and CC0-1.0 for small configuration files (D-064), and that your DCO sign-off (above) is accurate.
