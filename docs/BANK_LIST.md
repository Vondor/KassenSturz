# The bank list: how agents complete it and how the app shows what is missing

KassenSturz finds a bank from the IBAN the user types and then asks only what that bank's login needs (D-145). The bank's details come from two sources, in this order (D-146):

1. **The hand-checked list** in this repository: `crates/kassensturz-bank/src/banks.rs`, `HAND_CHECKED`. Corrections and additions a person confirmed in a test round. An entry here wins over the external source entirely.
2. **The external source**: AqBanking's bank information, read by the real backend (M4.5). It covers most German banks but can be out of date: a moved server, a bank that merged, a login kind it doesn't know; what it carries and what it lacks is below.

This document is the "Agent Info" the maintainer asked for (item 94 of the Proposals list): how a finding from a test round becomes an entry, and what the app shows while a bank isn't fully known. Only a few banks can be tested by hand in a test round, so the list grows one confirmed entry at a time.

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

## What AqBanking's bank information carries (item 93 C)

The maintainer chose AqBanking's bank information as the default source, with the hand-checked list for corrections (item 93 C, D-149). The real backend reads it through `AB_Banking_GetBankInfo` (`AqBankingDirectory` in `crates/kassensturz-bank/src/aqbanking/mod.rs`). What that data is, read from the German data file of AqBanking 6.9.2 (`banks.data`, see `docs/THIRD_PARTY.md`, 2026-10-23):

| What | Found | What the backend makes of it |
|---|---|---|
| Records | 14,101 for 3,513 bank codes: most codes have several records (branches, and old names marked `-alt-`, 589 of them) | AqBanking answers with the **first record of a code**; the others are never seen. Where the first record has no services but a later one has, the bank shows as `NoServer`: an entry in the hand-checked list is the fix. |
| Name, BIC, place | Name and BIC in most records, the BIC sometimes empty; names percent-escaped (`%28Gf P2%29`) and occasionally not UTF-8 | The name is shown as the bank's, unreadable bytes replaced; an empty BIC is "unknown". |
| Services | 10,753 `HBCI` services, each with an address, a mode (`PINTAN` or `RDH1` to `RDH10`; no `DDV` chip-card service in this data) and a protocol version (`3.0` in 10,470, empty in 281) | **Only `https://` addresses count**: the 3,040 PIN/TAN services have one (at 1,260 bank codes), the key-file services carry a bare host name (`fints2.atruvia.de`) that isn't a usable address. A bank with key-file services only shows as `NoServer`. PIN/TAN gives `LoginKind::PinTan` with the customer ID unknown (the data doesn't say), a `DDV` service would give a chip card; key-file modes give no login kind (D-059). |
| TAN methods, HTTP version, CAMT | Not in the data | AqBanking chooses the TAN method itself after the first contact (Q-088); the hand-checked list notes what worked. |

So for about 1,260 of Germany's bank codes the IBAN-first flow is `Ready` from AqBanking's data alone, for roughly 2,250 the user sees `NoServer` or `UnknownBank` and the list needs an entry. A test round at a bank the data gets wrong is the way to find out which (next section).

## The first entries (item 77)

The first entries of `HAND_CHECKED` are settings reported on 2026-10-23 as working with AqBanking 6.9.2 (no login data was given or taken; who uses which bank is nobody's business and is not recorded anywhere in this repository):

| Bank | Code | What worked in AqBanking | AqBanking's data for the code |
|---|---|---|---|
| Volksbank Darmstadt Mainz | `55190000` (BIC `MVBMDE55XXX`), and `50890000` (BIC `GENODEF1VBD`, the former Volksbank Darmstadt - Südhessen) | Server `https://fints2.atruvia.de/cgi-bin/hbciservlet`, HBCI 3.0, HTTP 1.1, numeric login name, TAN method 946 "SecureGo plus" (version 7), TAN mechanism "text", statements as CAMT preferred | `55190000`: PIN/TAN with that address, after several key-file services. `50890000`: 30 records, none with a service, so `NoServer` without the entry. |
| ING | `50010517` (BIC `INGDDEFFXXX`) | Server `https://fints.ing.de/fints`, HBCI 3.0, HTTP 1.1, login name, TAN method 900 "iTAN" (version 1), TAN mechanism "auto" | PIN/TAN with `https://fints.ing.de/fints/`, protocol version empty. |

At both banks the customer ID is the login name (AqBanking's setup shows the same value as user ID, customer ID and user name), so the entries have `customer_id: CustomerId::SameAsLoginName` and the app doesn't ask for one. Which of the two codes a Volksbank Darmstadt Mainz IBAN carries depends on the account; both are listed. The TAN method and mechanism aren't part of an entry (there is no field for them, Q-088); they are kept here for M4.6.

## How a test round completes the list

A person runs the real bank test on their own computer and reports **in words** what happened: the state shown, the bank code, which server address and login kind worked, and the error codes seen (D-055). No agent ever sees bank traffic, logs, PINs, TANs or login names, and the report must not contain them either. **Nothing in the repository says who banks where**: an entry, a commit message, a pull request or a decision names the bank and the settings, never the person who reported them or that they are a customer there (AGENTS.md §3.1 rule 2).

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

To see what AqBanking's data says for a code without the app, an agent can run the stub-free lookup in a test (`AqBankingBank::open` on a computer with AqBanking installed, then `directory().lookup`), or read the record in the data file (`banks.data`, the first record of the code). An error code the maintainer reports (`bank.setup-failed` with the backend's text, `bank.untrusted-server`, `bank.unreachable`) points at the server address or the certificate, not at the list: check the address against the bank's website before adding an entry.
