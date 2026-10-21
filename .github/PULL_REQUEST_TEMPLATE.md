## What and why

<!-- What does this change, and why? Link the roadmap item or issue if there is one. -->

## Definition of done (AGENTS.md §9.5)

- [ ] Builds in CI
- [ ] Formatted and lint-clean
- [ ] Tests pass (unit / property / golden-file / migration, as applicable)
- [ ] Docs updated (`docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, etc. as relevant)
- [ ] `docs/DECISIONS.md` updated if this makes or changes a decision
- [ ] No `TODO` without an issue reference
- [ ] SPDX header on every new source file, checked by `reuse lint`
- [ ] Every commit is signed off under the DCO (`git commit -s`, see D-040)

## Human gates (AGENTS.md §3.2)

Does this PR touch any of the following? If yes, say so explicitly — these need the
maintainer's explicit approval, not just a review. Keep the whole list and tick what applies:

- [ ] Adding/changing a direct dependency, or the license allowlist (D-026)
- [ ] A database schema change affecting existing user data
- [ ] A breaking change to the plugin API
- [ ] Keychain, cryptography, bank connections, or AI data sharing
- [ ] Code adapted from another project (AGENTS.md §3.1 rule 5)
- [ ] No human gate applies
