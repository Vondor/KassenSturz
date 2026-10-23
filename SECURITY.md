# Security policy

KassenSturz keeps a household's money records and, from M4 on, reads balances and statements from banks. A security problem in it can expose someone's finances, so we want to hear about it privately first. How we think about attacks is written down in [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md).

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Report it privately through GitHub's private vulnerability reporting (D-056 in `docs/DECISIONS.md`):

1. Open <https://github.com/Vondor/KassenSturz/security/advisories/new> (or the repository's **Security** tab, then **Report a vulnerability**).
2. Describe what you found, how to reproduce it, and what an attacker could do with it.
3. Send it. Only you and the maintainer can see the report; you can discuss it there and see when a fix is ready.

You need a GitHub account for this. If you can't use one, open a public issue that only says you have a security report and asks for a private way to send it, with **no details** in it.

### Never send real secrets or real financial data

Not in a report, not in an attachment, not in a log, not in a screenshot (AGENTS.md §3.1 rule 2, D-055):

- no banking PIN, TAN, login name, password or chip-card data,
- no content of your system keychain,
- no real bank statements, balances, IBANs or account numbers,
- no unredacted logs of a bank connection.

Show the problem with made-up data instead. For IBANs, use the example IBANs published in SWIFT's IBAN Registry, such as `DE89370400440532013000`. If a real secret was exposed by the problem you found, change it with your bank first, then report the problem without it.

## What to report

Anything that lets someone read, change or lose a user's data or secrets without the user meaning it, for example:

- a banking PIN, TAN or keychain entry that ends up on disk, in a log, in an error message or anywhere else it shouldn't be,
- KassenSturz opening a network connection the user didn't start, or sending data anywhere but to their own bank,
- an imported file or a bank's data that makes KassenSturz crash, run code, show wrong amounts or change other records,
- a way around the "AI off" switch or the MCP server's read-only mode (AGENTS.md §8),
- wrong handling of certificates on bank connections,
- problems in the packages we build (Flatpak and others), including the libraries we bundle.

Problems in AqBanking, Gwenhywfar or another library are welcome too. If the problem is in the library itself rather than in how KassenSturz uses it, we'll help you reach its developers, or you can report it to them directly.

## What happens next

KassenSturz is a small volunteer project, so these are aims, not promises:

- We confirm we got your report within **7 days**.
- We tell you whether we see it as a vulnerability, and how we plan to fix it, within **30 days**.
- We fix it in private, publish the fix in a release, and then publish a GitHub security advisory that describes the problem. We credit you there unless you prefer not to be named.
- Please give us **90 days** from your report before you talk about it publicly, or tell us if you need a different timeline. If a problem is already being exploited, we move faster and may publish sooner.

## Supported versions

KassenSturz has no release yet. Until 1.0 (builds before it are numbered `0.YYMMDD.N`), only the newest release and the `main` branch get security fixes. Once 1.0 is out, this section says which versions are supported.

## For contributors and AI agents

- Read `docs/THREAT_MODEL.md` before you change code that touches the keychain, bank connections, unsafe code, network access or AI.
- Every such change updates the threat model's table in the same pull request, and is a human gate (AGENTS.md §3.2).
- Never paste real bank data, PINs, TANs, keychain content or bank logs into an issue, a pull request, a commit, a test or a prompt (AGENTS.md §3.1 rules 2, 3 and 7).
