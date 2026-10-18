# Architecture

Headless core, thin UIs, everything extensible through one plugin API.

```
kassensturz/
├── Cargo.toml                      # workspace root
├── crates/
│   ├── kassensturz-core/           # domain model + logic; no UI, no I/O specifics
│   │   ├── money                   # 64-bit integer minor units + ISO 4217, rounding modes (D-027)
│   │   ├── account                 # accounts (incl. credit cards), categories, payees
│   │   ├── transaction             # double-entry transactions + splits, booking and value date
│   │   ├── currency                # currencies (decimal places stored per book), exchange rates, prices
│   │   ├── budget                  # basic budgets: a monthly limit per category (D-029)
│   │   ├── rules                   # auto-categorization: conditions, actions, Rule tree with sub-Rules
│   │   └── events                  # event bus (TransactionAdded, ...)
│   ├── kassensturz-store/          # storage trait + SQLite impl (v1), migrations, backups, change history — see D-016, D-032, D-033
│   ├── kassensturz-import/         # CSV, OFX, MT940, CAMT.053, KMyMoney (.kmy / SQLite)
│   ├── kassensturz-bank/           # read-only bank connections over AqBanking, fake backend for tests — see D-053, D-060
│   ├── kassensturz-aqbanking-sys/  # raw AqBanking/Gwenhywfar declarations (bindgen, committed, loaded at runtime) — see D-060
│   ├── kassensturz-plugin-api/     # stable, versioned plugin interface
│   ├── kassensturz-plugin-host/    # loads and sandboxes WASM plugins (D-007)
│   ├── kassensturz-platform/       # keychain, config paths, notifications behind traits
│   ├── kassensturz-charts/         # ChartProvider interface + implementations
│   ├── kassensturz-ai/             # LLM provider abstraction (OpenAI-compatible adapter in v1), consent, redaction
│   ├── kassensturz-mcp/            # local MCP server exposing the core (read-only by default)
│   ├── kassensturz-receipts/       # post-1.0: OCR + transaction-matching for photographed/scanned receipts
│   ├── kassensturz-mobile-bridge/  # post-1.0: Rust↔Kotlin FFI exposing core+receipts to the native Android app — see D-019, Q-018
│   ├── kassensturz-sync/           # post-1.0: shared-host ↔ thin-client access for multi-user (Member-scoped) — see D-022, Q-021/Q-022
│   ├── kassensturz-cli/            # CLI front end (testing, scripting)
│   └── kassensturz-ui-bridge/      # cxx-qt bindings exposing the core to QML
├── ui/qml/                         # Kirigami UI: overview, register, reports, settings, plugins
├── plugins/examples/               # sample importer, report, automation
├── packaging/                      # flatpak/, windows/, macos/
├── docs/
└── .github/
```

## Dependency rules

- `kassensturz-core` depends on **no other workspace crate**.
- `kassensturz-store`, `kassensturz-import`, `kassensturz-bank`, `kassensturz-charts`, `kassensturz-ai`, `kassensturz-plugin-host` depend on `kassensturz-core`.
- Only `kassensturz-bank` uses `kassensturz-aqbanking-sys`; no other crate calls AqBanking's C code (D-060).
- Front ends (`kassensturz-cli`, `kassensturz-ui-bridge`, `kassensturz-mcp`) depend on core, store and plugin host. **Core never knows about a UI.**
- OS-specific behavior lives only in `kassensturz-platform` behind traits.
- AI code lives only in `kassensturz-ai` and `kassensturz-mcp`. Front ends use them only behind one build feature (`ai`, on by default), so KassenSturz can be built without any AI code (D-039). `kassensturz-core` never depends on AI code.
- Built-in importers and exporters use the public plugin API.

## Plugin model

- Extension points: Importer, Exporter, Report, Automation.
- Versioned API. Manifest declares name, version, target API version, permissions.
- Narrow, permission-checked data access. No raw SQL access.
- WASM only, for sandboxing and cross-platform portability. Out-of-process (JSON-RPC) plugins are not part of v1: they can't be sandboxed, so the permission model couldn't be enforced for them (D-007).
- Users add a downloaded plugin file, in KassenSturz's plugin package format (defined in M4), in the **Plugins** tab, which shows the permissions it requests. No plugin store in v1: a store run by the project would be a hosted service (AGENTS.md §3.3).
- The Importer part of the plugin interface is defined during M1–M2, so the built-in importers use it from the start.

## Transactions and Parts (splits)

- A transaction can be broken down into multiple categorized **Parts** (UI term for `Split`, see `docs/GLOSSARY.md`) at any time, not only at import — see D-017.
- Editing Parts auto-calculates one remaining Part's amount as "total minus what's already entered." Saving is rejected unless all Parts sum exactly to the transaction's original amount — enforced in `kassensturz-core`, not just in the UI.
- Manual entry only in this decision; deriving Parts from a scanned receipt (OCR) is taken up separately in D-018.
- A Part in another currency keeps its original amount and currency plus its value in the transaction's currency (the rate used, stored as a fixed-point decimal, D-051), so every transaction adds up to zero in its own currency (D-043).

## Multiple accounts and Transfers

- Multiple accounts live side by side in one book (D-003); the UI shows an overview across all of them, not one account at a time — see M3 in `docs/ROADMAP.md`.
- When import sees a debit in one of the user's accounts and a matching credit in another account **already in the same book**, it proposes merging them into one double-entry **Transfer** (both splits are real asset/liability accounts, no income/expense category) instead of recording them as two unrelated transactions. See D-021.
- Matching: amount (same currency) or a plausible exchange rate for cross-currency transfers (D-004) + date-proximity + both accounts already present in the book. External IBANs are never candidates.
- Date window: up to 3 business days between the two sides; same currency only for now (D-030; cross-currency tolerances are Q-020).
- Always a suggestion the user confirms, every time, in v1; a "trust this pair of accounts" option may come later (D-030).
- Reports exclude confirmed Transfers from income/expense totals — a direct consequence of the existing double-entry model (§5), not a separate reporting rule.

## Import pipeline

- Steps run in this order: duplicate check, then Transfer matching, then Rules (D-030).
- Duplicates: a transaction whose bank-provided transaction ID is already in the book is skipped automatically and counted in the import summary; uncertain cases are shown to the user to decide.
- Both booking date and value date are stored; the booking date is the one shown.
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
- Field registry is extensible, not hardcoded: at minimum Other party name, Other party IBAN, Amount, Remittance information, Date, Currency.
- Regex is an explicit opt-in per condition, backed by Rust's `regex` crate for guaranteed linear-time (ReDoS-safe) matching. See D-015.
- Rules form a tree. A sub-Rule is only evaluated if its parent matched; a matching sub-Rule's action overrides the parent's. Between top-level Rules, the first match in the user's order wins (D-031).
- Applied automatically to newly imported transactions, after the duplicate check and Transfer matching (D-030); a manual "re-run Rules" action can re-apply them to existing transactions. It never changes a category set by hand or a transaction broken into Parts (D-031), and it can be undone through the change history (D-032).
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
- Built-in AI features (D-023), all on demand and used through a chat panel in the app (D-047): category suggestions, answers to questions and summaries the user asks for (every figure computed by code: the AI calls KassenSturz's functions, or gets a small precomputed overview if the model can't call functions, D-052), changes the AI prepares, such as new categories, Rules or charts, and proposals to turn recurring patterns into Rules or saved reports.
- A consent layer decides what data may leave the machine, per provider, with preview and redaction. A model on another computer, even one at home, counts as leaving the machine (D-035).
- Changes proposed by AI are represented as reviewable change sets and applied only after user confirmation. Writes requested through `kassensturz-mcp` are confirmed in the KassenSturz window (D-035).
- `kassensturz-mcp` serves the core to the user's own AI tools outside KassenSturz (e.g. Claude Desktop, LM Studio), over stdio only: the outside tool starts it, and no network port is opened (D-050). Read-only by default; write tools can be switched on separately, and every write is confirmed in the KassenSturz window (D-035). The chat panel doesn't use it.
- A global AI-off switch is a kill switch: `kassensturz-mcp` refuses to start while AI is off, and `kassensturz-ai` runs no provider code. The UI layer hides (not disables) every AI-related element in that state, except one "AI features" toggle in Settings. See AGENTS.md §8, D-013.

## AI memory

- `kassensturz-ai` keeps its own local "memory" (see D-023), analogous to this repo's own AGENTS.md/agent-memory setup — but for interactions with the user, not with code.
- Two parts: a maintained **context document** about this book's own setup, and a **pattern memory** of recurring interactions (e.g. "this payee is often categorized as X"). Both are rows in `kassensturz-store` (D-016), not separate files, so they stay inside the one-file book.
- Purpose: let AI **propose promoting a repeated pattern into a Rule (D-014) or a saved report** — never a parallel source of authority. Never caches balances or totals; those are always recomputed by code.
- Passive/on-demand only: consulted while the user is actively using AI, no unprompted notifications in v1 (Q-024).
- Follows every existing AI rule: off entirely when the AI off-switch is on (D-013); any of it sent to a provider goes through the same consent/preview/redaction flow as other context (§8); treated as data, never instructions, like any other untrusted text.
- User can view, edit or delete entries, individually or all at once, any time.
- Once Members exist (D-022), scoped the same way Account visibility already is — inherited, not reimplemented.
- Whether `kassensturz-mcp` exposes any of this to external AI tools is open (Q-025); retention/frequency tuning is open (Q-026).

## User-facing text

- `kassensturz-core` never produces user-facing text: it returns structured error codes (with parameters), and the UI owns every string (D-034).
- Translations use KDE's KI18n (gettext). v1 ships in English (source language) and German.
- Numbers are shown in the user's regional format by default (100,00 € in Germany, $100.00 in the US); the user can change the format and the number of decimal places shown. Display settings live in the app's settings, not in the book, and never change stored amounts or calculations (D-002).
- Plain-language terms come from `docs/GLOSSARY.md`; for example, the other side of a transaction is shown as "From"/"To" and is called "Other party" in the Rule editor.

## Data and portability

- One SQLite file per book in v1, managed by the app; "Open another book…" exists for power users. SQLite runs with the rollback journal, not WAL, so no lasting side files sit next to the book (D-033).
- Book files use the extension `.ksz` and carry a SQLite application ID, so KassenSturz recognizes its own books whatever they're called (D-037).
- A book is open in only one place at a time: a lock file next to it (computer name, time) travels with sync tools, so a second computer shows a clear message instead of editing its own copy. SQLite's locks only work within one computer (D-038). A book on a network drive opens with a clear warning (D-038). Sharing a book between people comes after 1.0 (D-022).
- Versioned migrations with an automatic backup before every migration, kept in KassenSturz's own data folder (the last 10, D-033). Before 0.1, development migrations are merged into one starting layout (D-045).
- Every change is recorded in a change history inside the book: undo/redo and an audit trail (D-032).
- Each book stores the decimal places of every currency it uses, fixed when the currency is first used (D-027).
- Export: CSV plus a complete export of the book in an open format (D-029). It is the stable way to move data.
- The book's layout is documented in `docs/BOOK_FORMAT.md` (from M1) so other programs can read books. It can change in any version, and writing into a book from outside isn't supported (D-046).
- Paths and keychain access go through `kassensturz-platform`, so Plasma, GNOME, Windows and macOS conventions are handled in one place. Data and config folders are named after the app ID `io.github.vondor.KassenSturz` (D-049).
- `kassensturz-store` exposes a backend-agnostic storage trait; SQLite is the only implementation for v1. Post-1.0, PostgreSQL, MySQL/MariaDB and other well-known SQL engines may be added as further implementations without changing `kassensturz-core`. See D-016, Q-012.
