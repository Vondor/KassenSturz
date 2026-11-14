# Security log: how attacks are run and recorded

For agents and developers. The plain-language page for users and testers is [`SECURITY_OVERVIEW.de.md`](SECURITY_OVERVIEW.de.md) (German, for testers) with its twin [`SECURITY_OVERVIEW.en.md`](SECURITY_OVERVIEW.en.md). The threat model is [`THREAT_MODEL.md`](THREAT_MODEL.md); private vulnerability reports go through [`SECURITY.md`](../SECURITY.md).

## Where attacks are recorded

- **Automated attack suite** in the tests (for example `crates/*/tests/attack_*.rs`, PR #882): runs in CI on every change. A failing attack that is not fixed yet stays in the suite as `#[ignore]` with its B number, never deleted (AGENTS.md §3.1 rule 12).
- **Working attack log** of the maintainer's agent team: one row per attack with date, round, class, input or technique, result (pass or B number) and fix PR. It lives in the team's shared project folder (`rogue/attack-log.md`), outside this public repository, because its rows describe open weaknesses in detail before they are fixed.
- **Findings** become GitHub issues with a B number from the coordinator; the fix PR names the B number in its title or body.
- **This repository** keeps the public summary: the "What we found and fixed" table on both overview pages.

## Rules for every attack round

1. **Synthetic books only**, in an isolated test machine or CI. Never a real book, real bank data or a secret (AGENTS.md §3.1 rule 2).
2. **Every attack round appends to the working attack log**, also when everything passes. A round that is not logged did not happen. Rows that only say "pass" matter too: they show what was tried.
3. **Every finding gets a B number** from the coordinator (never "next free" picked by the thread) and an issue.
4. **Every fix is attacked again** from its own PR build, through its own path and its neighbours, and the result is logged.
5. **When a fix merges, its row goes into the public table** of both overview pages (plain words, B number, PR), in the fix PR itself or in a follow-up docs PR. Describe what *could happen* to a user, not how to trigger it.
6. **Public text never carries** exploit recipes, details of unfixed weaknesses, hostnames, addresses or anything about the maintainer's own machines or setup. Open items are named only in general terms in the "can't do yet" section.
7. **New threats** found in a round also get a row in `THREAT_MODEL.md` (and the pull request that adds it follows that file's rules).

## Rounds so far

| When | What | Where the detail is |
|---|---|---|
| US6 | Security review and the automated attack suite (book, AI tools, MCP, chat, consent, exports) | PR #882; fixes #883, #884, #885, #888, #897, #900 |
| US7 Phase 2 | Eight rogue rounds in the test VM: hostile imports, money, crashes and power cuts, two writers, look-alike names, prompt injection with four local models, AI off, upgrades, sync clients | Working attack log; fixes listed on the overview pages |
