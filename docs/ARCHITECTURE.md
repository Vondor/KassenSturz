# Architecture

Headless core, thin UIs, everything extensible through one plugin API.

```
kassensturz/
├── CMakeLists.txt                  # builds and installs the app; drives Cargo (D-063)
├── Cargo.toml                      # workspace root
├── crates/
│   ├── kassensturz-core/           # domain model + logic; no UI, no I/O specifics
│   │   ├── money                   # 64-bit integer minor units + ISO 4217 code; amount × ratio and amount × or ÷ rate, rounded once (D-027, D-051)
│   │   ├── rounding                # rounding modes; the book stores which one it uses (D-027, D-078)
│   │   ├── account                 # accounts (incl. credit cards), categories, payees
│   │   ├── transaction             # double-entry transactions + splits (Parts): no category yet, foreign amounts, what a bank delivered; the remainder (D-083)
│   │   ├── transfer                # Transfer matching between the user's own accounts, and joining both sides into one transaction (D-021, D-087)
│   │   ├── currency                # ISO 4217 codes and KassenSturz's own ISO 4217 table, read from data/ (D-062, D-077); a currency with its decimal places in a book
│   │   ├── decimal                 # exchange rates, prices and quantities as fixed-point decimals, rust_decimal kept private (D-051, D-079)
│   │   ├── rates                   # exchange rates on a date: the rows of the book's price table, entered by hand (D-028)
│   │   ├── budget                  # basic budgets: a monthly limit per category (D-029)
│   │   ├── rules                   # auto-categorization: conditions, actions, Rule tree with sub-Rules
│   │   └── events                  # event bus (TransactionAdded, ...)
│   ├── kassensturz-store/          # all SQLite code: store, migrations, backups, change history — see D-016, D-032, D-033, D-072
│   ├── kassensturz-import/         # CSV, OFX, MT940, CAMT.053, KMyMoney (.kmy / SQLite)
│   ├── kassensturz-bank/           # read-only bank connections over AqBanking, fake backend for tests — see D-053, D-060
│   ├── kassensturz-aqbanking-sys/  # raw AqBanking/Gwenhywfar declarations (bindgen, committed, loaded at runtime) — see D-060
│   ├── kassensturz-plugin-api/     # stable, versioned plugin interface (after 1.0, D-071)
│   ├── kassensturz-plugin-host/    # loads and sandboxes WASM plugins (D-007; after 1.0, D-071)
│   ├── kassensturz-platform/       # keychain, config paths, notifications behind traits
│   ├── kassensturz-charts/         # ChartProvider interface + implementations
│   ├── kassensturz-ai/             # LLM provider abstraction (OpenAI-compatible adapter in v1), consent, redaction
│   ├── kassensturz-mcp/            # local MCP server exposing the core (read-only by default)
│   ├── kassensturz-receipts/       # post-1.0: OCR + transaction-matching for photographed/scanned receipts
│   ├── kassensturz-mobile-bridge/  # post-1.0: Rust↔Kotlin FFI exposing core+receipts to the native Android app — see D-019, Q-018
│   ├── kassensturz-sync/           # post-1.0: shared-host ↔ thin-client access for multi-user (Member-scoped) — see D-022, Q-021/Q-022
│   ├── kassensturz-cli/            # CLI front end for testing the core and the store: English only, errors as codes, not installed (D-085)
│   └── kassensturz-ui-bridge/      # the desktop app: cxx-qt bindings exposing the core to QML
│       ├── qml/                    # Kirigami UI: overview, register, reports, settings, plugins
│       └── cpp/                    # small C++ helpers, e.g. Qt's locale data for formatting (D-065)
├── data/                           # desktop file and AppStream metadata
├── po/                             # translations, one folder per language (D-034)
├── plugins/examples/               # sample importer, report, automation (after 1.0)
├── packaging/                      # flatpak/ (manifest and its offline crate list), windows/, macos/
├── tools/                          # sdk.sh: runs commands in the build environment (D-063); update-currency-table.py (D-077)
├── docs/
└── .github/
```

## Build and development

- CMake (with KDE's Extra CMake Modules) drives Cargo: Cargo builds the Rust crates, CMake installs the desktop file, AppStream data, icons and translations (D-063).
- Development, CI and the Flatpak use one environment, the KDE Flatpak SDK with the Rust extension; Flatpak builds use vendored Cargo dependencies (D-063).
- `tools/sdk.sh <command>` runs any command in that environment, e.g. `tools/sdk.sh cargo test --workspace`; CI uses the same script. Versions are pinned in D-073.
- CMake runs `cargo build --locked` as a custom target, as KDE's guide does, and installs the program it builds. QML files live in `crates/kassensturz-ui-bridge/qml/` because cxx-qt-build registers QML files by their path inside the crate.
- The Flatpak manifest (`packaging/flatpak/`) builds offline from `cargo-sources.json`, which is regenerated from `Cargo.lock` whenever that changes (`README.md`, "Building").
- The first code is a walking skeleton through every layer, kept as real code (D-068).

## Dependency rules

- `kassensturz-core` depends on **no other workspace crate**.
- `kassensturz-store`, `kassensturz-import`, `kassensturz-bank`, `kassensturz-charts`, `kassensturz-ai`, `kassensturz-plugin-host` depend on `kassensturz-core`.
- Only `kassensturz-store` touches the book's database; every other crate calls its functions (D-072). Reading another program's files, such as KMyMoney's SQLite format in `kassensturz-import`, isn't affected.
- Only `kassensturz-bank` uses `kassensturz-aqbanking-sys`; no other crate calls AqBanking's C code (D-060).
- Front ends (`kassensturz-cli`, `kassensturz-ui-bridge`, `kassensturz-mcp`) depend on core, store and plugin host. **Core never knows about a UI.**
- OS-specific behavior lives only in `kassensturz-platform` behind traits.
- AI code lives only in `kassensturz-ai` and `kassensturz-mcp`. Front ends use them only behind one build feature (`ai`, on by default), so KassenSturz can be built without any AI code (D-039). `kassensturz-core` never depends on AI code.
- Built-in importers and exporters implement the native Rust interface that the plugin API mirrors (D-069).

## Plugin model

- The WASM plugin host comes after 1.0 (D-071). Until then, built-in importers and exporters implement native Rust traits that the plugin API will mirror (D-069).
- Extension points: Importer, Exporter, Report, Automation.
- Versioned API. Manifest declares name, version, target API version, permissions.
- Narrow, permission-checked data access. No raw SQL access.
- WASM only, for sandboxing and cross-platform portability. Out-of-process (JSON-RPC) plugins are not part of v1: they can't be sandboxed, so the permission model couldn't be enforced for them (D-007).
- Users add a downloaded plugin file, in KassenSturz's plugin package format (defined with the plugin host), in the **Plugins** tab, which shows the permissions it requests. No plugin store in v1: a store run by the project would be a hosted service (AGENTS.md §3.3).
- The Importer part of the plugin interface is defined during M1–M2 as a native Rust trait: built-in importers implement it directly, and the WASM plugin API mirrors it once it exists (D-069, D-071).

## Transactions and Parts (splits)

- A transaction can be broken down into multiple categorized **Parts** (UI term for `Split`, see `docs/GLOSSARY.md`) at any time, not only at import — see D-017.
- Editing Parts auto-calculates one remaining Part's amount as "total minus what's already entered." Saving is rejected unless all Parts sum exactly to the transaction's original amount — enforced in `kassensturz-core`, not just in the UI.
- Manual entry only in this decision; deriving Parts from a scanned receipt (OCR) is taken up separately in D-018.
- A Part in another currency keeps its original amount and currency plus its value in the transaction's currency (the rate used, stored as a fixed-point decimal, D-051), so every transaction adds up to zero in its own currency (D-043). Every Part is in its account's currency; when an amount was converted with a rate from the price table, the Part keeps a copy of that rate (D-083).
- A Part can have no category yet: an import adds the bank's entry plus such a Part, which a Rule or the user then categorizes (D-083).
- What a bank or file delivered is kept with the Part booked to the account the statement is for, so a Transfer matched from two statements keeps both (D-083). That Part's account and amount never change; the other Parts, the payee and the date can.
- A transaction is changed by replacing it as a whole, with the same ID (`Book::update_transaction`), and checked like a new one. Changing or removing it can be undone like every change (D-032, D-086).

## Command-line tool

- `kassensturz-cli` is a front end like the app, so it uses only the public functions of `kassensturz-core` and `kassensturz-store` (D-072, D-085). It is for developers and testers: it isn't installed by the Flatpak or CMake and isn't translated (Q-040).
- It was the first front end to open book files, so it supplies the computer's name and the backup folder (`Computer`, D-080) from options or a best guess, until `kassensturz-platform` takes that over.
- Amounts go in and out as plain text (`-12.50`) with the currency's decimal places, never rounded; errors are written as their code and parameters (D-084).
- Run it with `tools/sdk.sh cargo run --bin kassensturz-cli -- help`.

## Multiple accounts and Transfers

- Multiple accounts live side by side in one book (D-003); the UI shows an overview across all of them, not one account at a time — see M3 in `docs/ROADMAP.md`.
- When import sees a debit in one of the user's accounts and a matching credit in another account **already in the same book**, it proposes merging them into one double-entry **Transfer** (both splits are real asset/liability accounts, no income/expense category) instead of recording them as two unrelated transactions. See D-021.
- Matching: amount (same currency) or a plausible exchange rate for cross-currency transfers (D-004) + date-proximity + both accounts already present in the book. External IBANs are never candidates.
- Date window: up to 3 business days between the two sides; same currency only for now (D-030; cross-currency tolerances are Q-020).
- Always a suggestion the user confirms, every time, in v1; a "trust this pair of accounts" option may come later (D-030).
- The heuristic is `kassensturz_core::find_transfers`; `Book::transfer_suggestions` runs it on the transactions whose other part has no category yet, and `Book::join_transfer` turns a confirmed pair into one Transfer: the outgoing transaction gets the incoming account's part, with what its bank delivered, and the incoming transaction goes (D-087).
- Reports exclude confirmed Transfers from income/expense totals — a direct consequence of the existing double-entry model (§5), not a separate reporting rule.

## Import pipeline

- Steps run in this order: duplicate check, then Transfer matching, then Rules (D-030).
- Duplicates: a transaction whose bank-provided transaction ID is already in the book is skipped automatically and counted in the import summary; uncertain cases are shown to the user to decide.
- Both booking date and value date are stored, with the Part the bank delivered; the transaction's booking date is the one shown, and the user can change it (D-083).
- What a bank or file delivers (other party name and IBAN, remittance information, bank transaction ID) is stored unchanged next to the payee and category the user can edit, so Rules can be re-run and duplicates recognized later (D-067).
- Only booked transactions are imported in v1; pending ones (not yet booked by the bank) are skipped (D-030).
- Bank connections through AqBanking only read balances and booked transactions; KassenSturz never sends payments (D-053). AqBanking comes from the system in distro packages and is bundled in the Flatpak, Windows and macOS builds; KassenSturz detects it at runtime and works without it. Bank setup, PIN and TAN entry use Gwenhywfar's Qt 6 dialogs (D-054). Logins: PIN/TAN or a bank chip card; no key files, so no EBICS (D-059). The code lives in `kassensturz-bank`, on top of the raw `kassensturz-aqbanking-sys` declarations (D-060). The PIN is asked each time unless the user chooses to remember it until KassenSturz closes (memory only) or to save it in the system keychain through `kassensturz-platform` (D-057). Tests run against a fake bank backend (D-055). Live FinTS connections need KassenSturz's own FinTS product registration, passed to AqBanking as `fintsRegistrationKey`; never another program's number (D-058).
- The KMyMoney importer is built clean-room, from sample files and public documentation only (D-044). It skips data KassenSturz can't hold yet (investments, schedules, loans, …) and reports what it skipped (D-029).

## Multi-user support (post-1.0)

- New concept **Member** (a person using KassenSturz), kept distinct from financial **Account** — see `docs/GLOSSARY.md`. See D-022.
- Each Account has an owner Member and can be explicitly shared with other Members; private to its creator by default.
- Visibility scoping (which Accounts a Member can see/use) is enforced **once**, in `kassensturz-core`/`kassensturz-store` — not reimplemented per feature. Rules, Parts, Transfers and receipt matching all inherit it automatically: a shared Account's imported bills can be completed from either Member's phone (D-019) because that Account is in both of their visible sets.
- Topology: a **shared host with thin clients**. One household-owned device (desktop or home server/NAS) runs the authoritative `kassensturz-store`; other desktops and phones connect over the **local network** via `kassensturz-sync` as thin clients, rather than each holding their own full copy.
- Not a project-run hosted service (§3.3 non-goal) — the host is infrastructure the household itself owns and runs.
- Requires network access for thin clients to reach the host; must stay opt-in and off by default (hard rule 3.1.4) — a single-user, single-device install has zero network exposure unless multi-user is deliberately turned on.
- SQLite's weak concurrent-write story may force one of D-016's planned non-SQLite backends for the host role; not decided (Q-021). Transport/protocol (Q-022) and Member authentication strength (Q-023, a visibility feature, not a security boundary) are also open.

## Rules (auto-categorization)

- Modeled on email-client filters (Thunderbird/Outlook), not KMyMoney's name-only matching. See D-014.
- A Rule = conditions (field + operator + value, matched ALL or ANY) + actions (v1: set category, set payee, make it a Transfer — D-031).
- Fields, extensible as KassenSturz learns more about a transaction: Other party name, Other party IBAN, Purpose (remittance information), Payee, Amount (exact, a range, money in or out), Booking date, Currency and the user's account (D-088). A new text field is one more `TextField` entry.
- Regex is an explicit opt-in per condition, backed by Rust's `regex` crate for guaranteed linear-time (ReDoS-safe) matching. See D-015.
- Rules form a tree. A sub-Rule is only evaluated if its parent matched; a matching sub-Rule's action overrides the parent's. Between top-level Rules, the first match in the user's order wins (D-031).
- Applied automatically to newly imported transactions, after the duplicate check and Transfer matching (D-030); a manual "run Rules" action applies them to existing transactions that have no category yet. A Rule books a transaction once; when the user changes a Rule, they can have it book again what it booked (D-088). It never changes a category set by hand or a transaction broken into Parts (D-031), and it can be undone through the change history (D-032).
- Rule suggestion: when the user has given transactions from the same payee the same category by hand several times, KassenSturz offers to create a Rule. Plain code that works with AI off; the user confirms (D-071).
- `kassensturz_core::RuleSet` checks a tree of Rules, compiles its patterns once and evaluates a transaction's facts (`RuleFacts`). `kassensturz-store` keeps the tree in the book (layout 9) and runs it: `Book::preview_rules` changes nothing, `Book::run_rules` is one user action. A run books only a transaction with two parts, one for the user's account and one without a category yet, and marks the part it booked in `split_rules`; `Book::update_rule_and_rebook` changes a Rule and books again what it and its sub-Rules booked, in the same user action (`Book::preview_rebook` shows that first). Changing a transaction by hand drops the mark, so manual edits win (D-031, D-088).
- Deterministic and code-driven — entirely separate from the AI model below (§8 of AGENTS.md does not apply to Rules).

## Receipt capture (post-1.0)

- One shared pipeline in `kassensturz-receipts` behind two entry points: a future KassenSturz Android companion app (camera capture) and a desktop-only function that imports an existing scan/photo file — see D-018.
- Pipeline: OCR extraction → deterministic matching heuristic (amount + date-proximity + fuzzy payee name) against already-imported transactions → proposes a Parts breakdown (D-017) from the receipt's line items.
- The match and the proposed Parts are always a suggestion; nothing is written to a transaction without user confirmation.
- Must work fully with AI off (D-013). An optional AI-assisted extraction path through `kassensturz-ai` may help on messy receipts once AI is on, but is never required.
- Phone-to-desktop transfer is not a project-run hosted service (§3.3 non-goal); see Q-015 for the local-network-pairing vs. file-drop choice.
- The Android app is a **separate native app** (D-019), not the Qt 6/Kirigami stack via Qt for Android. It reuses `kassensturz-core`/`kassensturz-receipts` through `kassensturz-mobile-bridge` (a Rust↔Kotlin FFI binding, e.g. UniFFI — not pinned yet) rather than reimplementing money-handling or matching logic natively. Exact UI framework and FFI approach are open (Q-018); adding Android to the Portability principle's platform list (AGENTS.md §2) still needs its own maintainer sign-off when this is actually scheduled.

## AI model

- `kassensturz-ai` defines a provider trait. v1 adapter: OpenAI-compatible HTTP only (Ollama, llama.cpp server, LM Studio, vLLM, and cloud services offering the same API); dedicated cloud adapters may follow later (D-035).
- Built-in AI features, all on demand and used through a chat panel in the app (D-047): category suggestions, answers to questions and summaries the user asks for (every figure computed by code: the AI calls KassenSturz's functions, or gets a small precomputed overview if the model can't call functions, D-052), and changes the AI prepares, such as new categories, Rules or charts. Proposals that turn recurring patterns into Rules or saved reports come with AI memory, after 1.0 (D-071).
- A consent layer decides what data may leave the machine, per provider, with preview and redaction. A model on another computer, even one at home, counts as leaving the machine (D-035).
- Changes proposed by AI are represented as reviewable change sets and applied only after user confirmation. Writes requested through `kassensturz-mcp` are confirmed in the KassenSturz window (D-035).
- `kassensturz-mcp` serves the core to the user's own AI tools outside KassenSturz (e.g. Claude Desktop, LM Studio), over stdio only: the outside tool starts it, and no network port is opened (D-050). Read-only by default; write tools can be switched on separately, and every write is confirmed in the KassenSturz window (D-035). The chat panel doesn't use it.
- A global AI-off switch is a kill switch: `kassensturz-mcp` refuses to start while AI is off, and `kassensturz-ai` runs no provider code. The UI layer hides (not disables) every AI-related element in that state, except one "AI features" toggle in Settings. See AGENTS.md §8, D-013.

## AI memory (post-1.0)

- Moved to after 1.0 (D-071). Until then, the chat learns about the book through KassenSturz's functions and the small overview (D-052), computed fresh each time.
- `kassensturz-ai` keeps its own local "memory" (see D-023), analogous to this repo's own AGENTS.md/agent-memory setup — but for interactions with the user, not with code.
- Two parts: a maintained **context document** about this book's own setup, and a **pattern memory** of recurring interactions (e.g. "this payee is often categorized as X"). Both are rows in `kassensturz-store` (D-016), not separate files, so they stay inside the one-file book.
- Purpose: let AI **propose promoting a repeated pattern into a Rule (D-014) or a saved report** — never a parallel source of authority. Never caches balances or totals; those are always recomputed by code.
- Passive/on-demand only: consulted while the user is actively using AI, no unprompted notifications in v1 (Q-024).
- Follows every existing AI rule: off entirely when the AI off-switch is on (D-013); any of it sent to a provider goes through the same consent/preview/redaction flow as other context (§8); treated as data, never instructions, like any other untrusted text.
- User can view, edit or delete entries, individually or all at once, any time.
- Once Members exist (D-022), scoped the same way Account visibility already is — inherited, not reimplemented.
- Whether `kassensturz-mcp` exposes any of this to external AI tools is open (Q-025); retention/frequency tuning is open (Q-026).

## User-facing text

- `kassensturz-core` never produces user-facing text: it returns structured error codes (with parameters), and the UI owns every string (D-034). Every error of core and store turns into an `ErrorReport`: a stable code, typed parameters and its cause. `docs/ERROR_CODES.md` lists every code and suggests an English message for it (D-084).
- Translations use KDE's KI18n (gettext). v1 ships in English (source language) and German.
- Numbers are shown in the user's regional format by default (100,00 € in Germany, $100.00 in the US); the user can change the format and the number of decimal places shown. Display settings live in the app's settings, not in the book, and never change stored amounts or calculations (D-002).
- Amounts reach QML only as text: Rust formats them from the exact value, with separators and currency symbol from Qt's locale data, and parses what the user types. Sorting and totals happen in Rust models; floating point appears only as chart drawing coordinates (D-065). How the pieces are taken from Qt: D-075.
- QML gets its translations through KI18n's `KI18nContext`, created in QML, so the Rust program links no KDE Frameworks library (D-074).
- Plain-language terms come from `docs/GLOSSARY.md`; for example, the other side of a transaction is shown as "From"/"To" and is called "Other party" in the Rule editor.

## Data and portability

- One SQLite file per book in v1, managed by the app; "Open another book…" exists for power users. SQLite runs with the rollback journal, not WAL, so no lasting side files sit next to the book (D-033).
- Book files use the extension `.ksz` and carry the SQLite application ID `KSZB`, so KassenSturz recognizes its own books whatever they're called (D-037). The store checks it before SQLite opens a file, so another program's SQLite file is never changed (D-080).
- A book is open in only one place at a time: a lock file next to it (computer name, time) travels with sync tools, so a second computer shows a clear message instead of editing its own copy. SQLite's locks only work within one computer (D-038). Opening the book anyway after that warning takes over the lock only if it is still the one the user was warned about (D-080). A book on a network drive opens with a clear warning (D-038). Sharing a book between people comes after 1.0 (D-022).
- Versioned migrations, run in one transaction, with an automatic backup before every migration, kept in KassenSturz's own data folder (the last 10 of each book, D-033, D-080). Before 0.1, development migrations are merged into one starting layout (D-045).
- Every change is recorded in a change history inside the book: undo/redo and an audit trail (D-032). It stores the before and after values of every row a user action touched; undo works after a restart, and the user sets how long the history is kept: 30 days by default, or forever; 0 stores none, though undo still works while the book is open. "Erase permanently" actions (wiping AI memory, Clean up history) also remove data from the history (D-066).
- `kassensturz-store` records the history itself, with temporary triggers on its own connection to the book, and refuses a change outside a user action, so nothing escapes it. Each public function that changes a book is one user action; a front end groups several calls into one with `Book::as_one_action`. Undo and redo check that the book still holds what the history says and that the result keeps every reference and rule of the book; otherwise they change nothing (D-086).
- Everything in a book is identified by a UUIDv7, not a running number (D-067).
- Each book stores the decimal places of every currency it uses, taken from KassenSturz's own ISO 4217 table when the currency is first used and fixed from then on (D-027, D-062). The table is a data file in `kassensturz-core`, written by `tools/update-currency-table.py` from the codes Debian's iso-codes lists, with the decimal places kept in the table itself (D-062, D-077). A withdrawn currency, such as the Deutsche Mark, isn't in it; the importers convert it into the currency that replaced it, keeping the original amounts (D-077).
- Exchange rates live in the book's dated price table, one per pair of currencies, direction and day, entered by hand in v1 (D-028). A conversion uses the newest rate on or before its date, multiplies by it or divides by it for the other direction, computes exactly in 128 bits and rounds once with the book's rounding mode (D-051, D-079).
- The rounding mode of a book's calculations is stored in the book, so every computer calculates the same way; changing it affects only later calculations (D-027, D-078).
- Export: CSV plus a complete export of the book in an open format (D-029). It is the stable way to move data.
- The book's layout is documented in `docs/BOOK_FORMAT.md` so other programs can read books. It can change in any version, and writing into a book from outside isn't supported (D-046).
- Paths and keychain access go through `kassensturz-platform`, so Plasma, GNOME, Windows and macOS conventions are handled in one place. Data and config folders are named after the app ID `io.github.vondor.KassenSturz` (D-049). The store doesn't look these up itself: a front end passes it the computer's name and the backup folder when it opens a book (`Computer`, D-080).
- All database code for the book lives in `kassensturz-store`; no other crate touches the book's database directly. SQLite is the only database until 1.0. When another one (PostgreSQL, MySQL/MariaDB, …) is scheduled after 1.0, a storage trait is extracted from the existing code, without changing `kassensturz-core`. See D-016, D-072, Q-012.
