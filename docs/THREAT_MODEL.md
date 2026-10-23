# Threat model

What KassenSturz protects, from whom, and what stops each attack. Written in M4.0, before any bank code exists (D-056), so the steps of M4 build the protections in from the start instead of adding them later. It covers the book, file imports and bank connections. AI comes in M5, which extends this document (AGENTS.md §8 already sets the rules).

**How to keep it current.** Every pull request that touches the keychain, bank connections, unsafe code, network access, the Flatpak's permissions or AI updates the table in [Threats and what stops them](#threats-and-what-stops-them): the "Where" and "Status" columns, and new rows for threats it finds. Such a pull request is a human gate (AGENTS.md §3.2). Vulnerability reports go through [`SECURITY.md`](../SECURITY.md).

Facts about AqBanking and Gwenhywfar below were read in their 6.9.2 and 5.14.1 sources (the versions KassenSturz pins, `docs/THIRD_PARTY.md`); file names are given so the next agent can check them.

## What we protect

| ID | What | Why it matters |
|---|---|---|
| A-1 | **Banking PIN** (and a chip card's PIN) | With the login name it opens the user's online banking. Some banks let a PIN alone read statements without a TAN (D-057) |
| A-2 | **TANs** | One-time codes. A TAN caught in flight could confirm something the user didn't mean. KassenSturz only reads, but the bank doesn't know that |
| A-3 | **Saved PINs in the system keychain** | Saved only if the user chooses (D-057) |
| A-4 | **Bank login data**: bank code, login name, customer ID, bank server address, TAN method | Not secret on its own, but the login name is half of a login. Treated as personal data |
| A-5 | **Fetched balances and statements** | The household's financial life |
| A-6 | **The book** | Everything the household recorded; it may sit in a sync folder (D-033, D-038) |
| A-7 | **AqBanking's own folder**: its settings (A-4, the accounts' IBANs, the bank's parameters) and its message logs (A-5 in plain text) | Lies outside the book, so it's easy to forget |
| A-8 | **Correct numbers** | Principle 1: a fetched transaction must never be changed, counted twice or lost on the way into the book |

The FinTS product registration number (D-058) is **not** a secret: open-source programs carry theirs in the source code. It is protected by D-058's rule that nobody else's number is used, not by hiding it.

## Who and what we defend against

| ID | Attacker or source | Examples |
|---|---|---|
| X-1 | **Data from outside**: the bank's messages, statement files, KMyMoney files | A payee name with HTML in it, a malformed statement, a file built to crash the reader |
| X-2 | **Someone on the network** between KassenSturz and the bank | Public Wi-Fi, a malicious proxy, a fake certificate |
| X-3 | **Someone with a copy of the book or a backup** | A sync server (Nextcloud, Dropbox), a lost USB stick, a shared backup |
| X-4 | **Other user accounts** on the same computer | A shared family computer |
| X-5 | **Other programs running as the same user** | Malware, a browser exploit, another app with broad permissions |
| X-6 | **The supply chain** | A Rust crate, a bundled C library, the Flatpak build |
| X-7 | **Leaks while building and supporting KassenSturz** | A bug report with a real log, a test fixture made from real data, a prompt to an AI agent |

**What we can't stop (X-5).** A program that runs as the user can read the user's files, and can read an unlocked keychain through the Secret Service. KassenSturz limits what such a program can find (no secrets in files, PINs only saved when the user chooses, AqBanking's logs removed), but it can't defend against it. A Flatpak or other sandbox around the other program is what helps there. The same goes for someone with full access to the unlocked computer.

## Where data crosses boundaries

```
 User ─── QML screens (kassensturz-ui-bridge) ──── never sees a PIN or TAN
              │
              │ amounts as text (D-065), bank text as plain text (T-08)
              ▼
     kassensturz-bank (safe Rust) ──── kassensturz-pipeline ──── kassensturz-store ──── book (A-6)
       │      │    PIN modes (D-057), exact amounts (T-10)    import pipeline (D-030)
       │      │
       │      └── kassensturz-platform ──── Secret Service (D-Bus) ──── KWallet / GNOME Keyring (A-3)
       │
       │ only place with unsafe code (D-060)
       ▼
     kassensturz-aqbanking-sys ── loaded at runtime ──► libaqbanking, libgwenhywfar (C)
                                                        │      │
     KassenSturz's small C++ library (Q-035) ──► libgwengui-qt6: setup, PIN and TAN dialogs
                                                        │
                                                        ├── its own folder: settings, message logs (A-7)
                                                        └── TLS (GnuTLS) ──► the bank's FinTS server (X-2)
```

Trust boundaries: everything the bank, a file or AqBanking returns is **untrusted input** (X-1), checked in `kassensturz-bank` before it reaches the pipeline. Everything that leaves the computer leaves through AqBanking, to the bank the user set up, when the user asks for it.

## Threats and what stops them

Status: **planned** (the step named under "Where" builds it), **done** (merged, with a test), **open** (a known gap), **accepted** (a risk we live with, explained).

| ID | Threat | What stops it | Where | Status |
|---|---|---|---|---|
| T-01 | **The PIN is written to disk** by Gwenhywfar or AqBanking. Gwenhywfar can keep passwords in a password-store file (`src/gui/passwdstore.c`, used when the app sets one) and ask to save them for good (`GWEN_GUI_FLAGS_PERMPASSWORDS`) | KassenSturz never gives Gwenhywfar a password store and never sets that flag. Its Gwenhywfar GUI answers every password request itself (`CppGui::getPassword`) with the choices of D-057: ask, remember until the app closes, or the keychain through `kassensturz-platform`. A test runs a fake session and checks that no file in AqBanking's folder contains the test PIN (D-140) | M4.2, M4.5, M4.6 | planned |
| T-02 | **The PIN or a TAN shows up in a log, an error, a panic message or a crash report** | The PIN type has no `Debug` or `Display` that shows its value and is wiped from memory when dropped (`zeroize`, if approved). Errors are codes without values (D-034). The PIN never reaches QML or JavaScript: it is typed into Gwenhywfar's Qt dialog and passed between C++ and Rust only. Gwenhywfar's log messages go through KassenSturz's log hook, which keeps warnings and errors only and writes no file (D-140). KassenSturz sends no crash reports (AGENTS.md §3.1 rule 4) | M4.1, M4.3, M4.6 | planned |
| T-03 | **A PIN stays in memory and lands in a core dump** or swap | Wiped when no longer needed (T-02). Remembering it until the app closes is the user's choice (D-057). A core dump of a crashed KassenSturz may still hold it while it is remembered | M4.3 | accepted: only the operating system can rule this out (locked memory, no core dumps); revisit if a cheap measure appears |
| T-04 | **A saved PIN is read by another program** (X-5) | Saving is opt-in; the default is to ask each time (D-057). Saved PINs can be removed in Settings and go with the bank login. Sandboxed apps need explicit permission to reach the Secret Service | M4.1, M4.6 | accepted for unsandboxed programs (see X-5) |
| T-05 | **A wrong saved PIN locks the bank login**: banks block a login after a few wrong PINs, and an app that retries a stored wrong PIN uses them up | When the bank says the PIN is wrong (Gwenhywfar's `setPasswordStatus` "bad"), KassenSturz forgets the remembered PIN and deletes the saved one at once, and never sends a PIN again without asking the user (D-140) | M4.3, M4.5 | planned |
| T-06 | **AqBanking's message logs keep the user's statements in plain text.** AqBanking 6.9.2 writes every FinTS message it sends and receives to `<its folder>/backends/aqhbci/data/banks/<country>/<bank code>/logs/*.log`, always, with no switch to turn it off (`msglayer/dialog.c`, `msglayer/message.c`). It blanks the PIN and TAN (segments `HNSHA`, `HKPAE`, `DKPAE`), but balances, transactions, IBANs and the login name stay readable | KassenSturz gives AqBanking a folder of its own inside KassenSturz's data folder, readable only by the user, and deletes the `logs` folders after every bank session and at every start, which also catches a crash (D-139). Asking AqBanking's developers for a switch is a follow-up | M4.5 | planned |
| T-07 | **AqBanking's settings with the login data (A-4) and IBANs are read by others** (X-3, X-4), or mixed up with KMyMoney's or GnuCash's | Its own folder (T-06), created readable only by the user, never inside a sync folder and never in the book. KassenSturz never uses the shared `~/.aqbanking` of other programs (D-139) | M4.5 | planned |
| T-08 | **Text from a bank or a file is shown as rich text.** Qt Quick shows a text that looks like HTML as rich text unless told otherwise, and rich text can load images from the web. A payee name like `<img src="https://…">` would make KassenSturz contact a server (privacy leak, tracking) or show something that looks like part of the app. In the M3 screens, several labels show such text without `textFormat: Text.PlainText`, for example the payee names in `TransferPrompt.qml` and the row titles in `TransactionRow.qml`. Today the Flatpak has no network, but bank connections need it (D-142) | Every text that comes from a bank, a file or the user is shown with `textFormat: Text.PlainText`. The QML engine gets a network access factory that refuses every non-local address, so even a missed label can't reach the network (D-143). Both land before the Flatpak gets network access | M4.T, before M4.8 | **open** |
| T-09 | **Malformed bank data crashes KassenSturz or corrupts memory** (X-1) through the C libraries | Unsafe code lives only in `kassensturz-aqbanking-sys` (D-060). `kassensturz-bank` checks every pointer for null, copies every string at once with a length limit, checks that text is valid UTF-8 (with one written rule and a test for text that isn't), and refuses a statement that is too large, the same limits as for files (D-091). Tests feed the safe layer broken data through a stub library | M4.2, M4.5 | planned |
| T-10 | **An amount from the bank is changed on the way in**: AqBanking keeps amounts as exact fractions (`AB_VALUE`, GMP rationals in `types/value.c`) but also offers them as floating point (`AB_Value_GetValueAsDouble`, `AB_Value_toDbFloat`, `AB_Value_fromDouble`) | KassenSturz reads amounts only as the exact fraction (`AB_Value_GetNumDenomString`) and turns them into `Money` only when they fit the currency's minor units exactly; anything else is refused with an error code, never rounded (AGENTS.md §3.1 rule 1, D-141). The floating-point functions are left out of the generated bindings, so no code can call them | M4.2, M4.3, M4.5 | planned |
| T-11 | **A fetched transaction is counted twice or lost** (A-8) | Fetched statements go through the same pipeline as files: duplicate check by the bank's transaction ID, user decisions for unsure cases, booked transactions only (D-030); the balance check against the bank's balance shows any gap (D-029). Tests feed the pipeline the fake bank's real output, not hand-built values (AGENTS.md §9.4) | M4.3, M4.7 | planned |
| T-12 | **A network attacker reads or changes the bank traffic** (X-2) | FinTS with PIN/TAN relies on TLS, which Gwenhywfar does with GnuTLS. KassenSturz's Gwenhywfar GUI accepts certificates that check out and refuses all others without offering "connect anyway" (`checkCert`, D-140), so a fake certificate can't be clicked through. Bank addresses come from the bank list or the user's setup and must start with `https://` | M4.5, M4.6 | planned |
| T-13 | **KassenSturz contacts something other than the user's bank** | Only `kassensturz-bank` opens network connections, only through AqBanking, and only when the user asks to fetch (AGENTS.md §3.1 rule 4). The QML engine refuses remote addresses (T-08). Once the Flatpak has network access (D-142), this rests on the code, so a test checks that nothing but the bank backend asks for the network | M4.5, M4.8 | planned |
| T-14 | **KassenSturz could move money** | It has no code that starts a transfer, standing order or direct debit (D-053). `kassensturz-bank`'s trait has no such function, and the bindings declare only the functions that reading needs: functions that exist only for payments are left out | M4.2, M4.3 | planned |
| T-15 | **Key files outside the keychain** | Only PIN/TAN and chip-card logins are offered (D-059). AqBanking's PayPal backend keeps its credentials in an encrypted file of its own (`aqpaypal/provider_credentials.c`), so it isn't offered either (D-142) | M4.5, M4.6 | planned |
| T-16 | **A malicious or broken library in the bundle** (X-6) | Bundled sources are pinned by SHA-256 and listed in `docs/THIRD_PARTY.md`, checked against two independent copies where possible (the upstream tarball and a distribution's). Crates are checked by cargo-deny (D-026). AqBanking is built with the FinTS backend only, which also leaves out EBICS and its XML signing library (D-142). The bindings are generated by a developer and committed, so a normal build runs no generator (D-060) | M4.2, M4.8 | planned |
| T-17 | **A different AqBanking than the one the bindings were made for** crashes KassenSturz | KassenSturz loads the libraries by their ABI name (`libaqbanking.so.44`, `libgwenhywfar.so.79`, `libgwengui-qt6.so.79`) and checks their version before the first call; an older or unknown one counts as "not installed" and the app says what to install (D-054, D-138) | M4.2, M4.3 | planned |
| T-18 | **Real bank data leaks while building or supporting KassenSturz** (X-7) | Tests use the fake bank and SWIFT's example IBANs only (D-055). The maintainer runs the real-bank test on their own computer, scrubs the log and reports in words; no agent sees bank traffic, logs, PINs or TANs, and no remote session is used for it. `SECURITY.md` asks reporters for made-up data | M4.3, Phase 2 | planned |
| T-19 | **Data in the book is read by someone with a copy** (X-3) | The book holds no secrets: no PIN, no TAN, no login name (D-139, Q-086). It does hold the household's records; protecting a copied file is the user's storage (disk encryption, the sync service). Encrypting the book is not part of v1 | M4.4a if chosen | accepted |
| T-20 | **Bank text tricks a person or an AI** (X-1): a payee name that reads like an instruction | Text from banks and files is data, never instructions (AGENTS.md §8); the AI side is covered in M5. For people: bank text is shown as plain text and clearly as the bank's (T-08) | M5 | planned |

## Rules for code that touches secrets

For the agents of M4.1 to M4.8 and everyone after them:

1. A PIN, TAN or keychain value never goes into a log, an error, a `Debug` or `Display` output, a QML property, a test fixture, a commit or an issue.
2. Secrets live in one type that wipes itself; copies are avoided, and every copy is wiped too.
3. Nothing is written to disk that holds a secret, except through `kassensturz-platform` into the system keychain.
4. Everything from AqBanking, a bank or a file is untrusted input, checked once at the boundary.
5. Amounts are exact: no floating point anywhere between the bank and the book.
6. A step that changes any of this updates this document in the same pull request.

## Not covered yet

- **AI** (M5): what the chat, the providers and the MCP server may see of bank data; prompt injection through payee names and memos; a visible log of the AI's tool calls. M5's Phase 0 extends this document.
- **Windows and macOS** keychains and packages (M7).
- **Plugins** (after 1.0, D-071).
