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
   A pull request whose commits aren't signed off will fail CI and can't be merged.
5. Add tests for anything in `kassensturz-core` or other logic crates (AGENTS.md §9.4).
6. Run formatting, lint and tests locally before opening the pull request.
7. Make sure every new source file carries an SPDX header
   (`SPDX-License-Identifier: GPL-3.0-or-later`, `SPDX-FileCopyrightText: <year> KassenSturz contributors`)
   — checked by `reuse lint` in CI (D-024).

## What needs the maintainer's sign-off first

Some changes are proposed by anyone but **decided only by the maintainer** — see AGENTS.md §3.2.
That includes dependency changes, schema migrations, anything touching keychain/crypto/bank
connections/AI data sharing, and merging to `main` itself. If you're unsure whether your change
falls into one of these, say so in the pull request description and ask.

## Reporting a security issue

See [SECURITY.md](SECURITY.md) once it exists (scheduled before the bank-connection milestone, D-056).
Until then, please open a private report via GitHub's security advisories rather than a public issue.

## License

By contributing, you agree your contribution is licensed under GPL-3.0-or-later (see [LICENSE](LICENSE)),
and that your DCO sign-off (above) is accurate.
