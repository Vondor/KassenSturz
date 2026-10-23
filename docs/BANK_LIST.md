# The bank list: how agents complete it and how the app shows what is missing

KassenSturz finds a bank from the IBAN the user types and then asks only what that bank's login needs (D-144). The bank's details come from two sources, in this order (D-145):

1. **The hand-checked list** in this repository: `crates/kassensturz-bank/src/banks.rs`, `HAND_CHECKED`. Corrections and additions a person confirmed in a test round. An entry here wins over the external source entirely.
2. **The external source**: AqBanking's bank information, read by the real backend (M4.5). It covers most German banks but can be out of date: a moved server, a bank that merged, a login kind it doesn't know.

This document is the "Agent Info" the maintainer asked for (item 94 of the Proposals list): how a finding from a test round becomes an entry, and what the app shows while a bank isn't fully known. The maintainer can test only two banks, so the list grows one confirmed entry at a time.

## What the app shows when something is missing

Every gap is an explicit state (`kassensturz_bank::Setup`), shown in plain language with what to do next (AGENTS.md §6). The screens (M4.6) and the wording (M4.9) build on these states; the words below are the suggested English.

| State | When | What the user sees, and can do |
|---|---|---|
| `Ready` | The bank, its server and its login kinds are known | The bank's name is shown for confirmation; the app asks the login kind's questions (`LoginKind::questions`): login name, a customer ID only where the bank has one, PIN; or the chip card. |
| `NoFints` | The IBAN is from another country than Germany | "Banks in {country} don't offer the connection KassenSturz uses (FinTS). You can import a file from your bank instead." No bank lookup happens. |
| `UnknownBank` | No source knows the bank code | "KassenSturz doesn't know a bank with the code {code}. You can enter its name and server yourself, or import a file instead." The fallback is the manual setup (Gwenhywfar's dialog, Q-035). The code goes into a bug report so the list can be completed. |
| `NoServer` | The bank is known, but no FinTS server address is | "{bank} is known, but not how to reach it. If your bank offers FinTS (also called HBCI), enter the server address from your bank's website, or import a file instead." |
| `UnknownLoginKind` | The bank and its server are known, but not how customers log in | "{bank} is known, but not how you log in there. Choose: login name and PIN, or chip card." The user's choice goes into the report. |

Errors while connecting are the `bank.` codes in `docs/ERROR_CODES.md`, each with what to do next.

## How a test round completes the list

The maintainer runs the real bank test on their own computer and reports **in words** what happened: the state shown, the bank code, which server address and login kind worked, and the error codes seen (D-055). No agent ever sees bank traffic, logs, PINs, TANs or login names, and the report must not contain them either.

An agent then adds or corrects an entry:

1. **Take only what the maintainer confirmed worked.** Never fill a server address or login kind from memory, a search result or another program's bank list. If the maintainer's report lacks a value, leave it `None`, which the app shows as "no server" or "unknown login type", and ask through the coordinator.
2. **Write the whole entry**, since it replaces the external source's record completely:

   ```rust
   BankEntry {
       country: "DE",
       code: "12345678",                       // the bank code from the IBAN, eight digits
       name: "Example Bank",                    // as its customers know it
       bic: Some("EXAMDEFFXXX"),                // or None
       server: Some("https://fints.example-bank.example/"), // https only (threat T-12), or None
       login_kinds: Some(&[LoginKind::PinTan { customer_id: CustomerId::SameAsLoginName }]), // or None
       checked: "2026-11-05, the maintainer's test round 1: login and fetch worked",
   },
   ```

   `customer_id` says whether the bank gives a customer ID apart from the login name (`Separate`), uses the login name for both (`SameAsLoginName`), or whether that isn't known (`Unknown`, asked as optional).
3. **Nothing personal goes in**: no login name, customer ID, account number, IBAN of a person, PIN or TAN, and no fragment of a bank's log (AGENTS.md §3.1 rules 2 and 3). The server address and login kinds are the same for every customer of the bank, so they are fine.
4. **Run the tests.** `cargo test -p kassensturz-bank banks` checks the format: a two-letter country, an eight-digit German code, an `https://` server, non-empty `checked`, no duplicate codes.
5. **Record it**: a line in the pull request's description naming the test round, and the bug the Issue Tracker opened for the missing bank, closed by the pull request.

When the external source and the maintainer's finding differ, the list entry stands, and a note in `docs/THIRD_PARTY.md`'s verification log or an upstream report to AqBanking is a follow-up, not part of the entry.

## Showing the list's state to agents

Where a report shows a bank code and an `UnknownBank`, `NoServer` or `UnknownLoginKind` state, the Issue Tracker files it as a bug of kind "bank list" against the step that owns this document (M4.3), with the state, the code and what the maintainer confirmed. The fix is one entry in `banks.rs` and nothing else.
