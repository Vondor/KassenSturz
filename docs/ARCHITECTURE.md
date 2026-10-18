# Architecture

Headless core, thin UIs, everything extensible through one plugin API.

```
kassensturz/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── kassensturz-core/            # domain model + logic; no UI, no I/O specifics
│   │   ├── money               # integer minor units + ISO 4217, rounding modes
│   │   ├── account             # accounts, categories, payees
│   │   ├── transaction         # double-entry transactions + splits
│   │   ├── currency            # currencies, exchange rates, prices
│   │   ├── budget
│   │   ├── rules               # auto-categorization: conditions, actions, Rule tree with sub-Rules
│   │   └── events              # event bus (TransactionAdded, ...)
│   ├── kassensturz-store/           # storage trait + SQLite impl (v1), migrations, backups — see D-016
│   ├── kassensturz-import/          # CSV, OFX, MT940, CAMT.053, KMyMoney (.kmy / SQLite)
│   ├── kassensturz-plugin-api/      # stable, versioned plugin interface
│   ├── kassensturz-plugin-host/     # loads and sandboxes plugins (WASM / JSON-RPC)
│   ├── kassensturz-platform/        # keychain, config paths, notifications behind traits
│   ├── kassensturz-charts/          # ChartProvider interface + implementations
│   ├── kassensturz-ai/              # LLM provider abstraction (local + cloud), consent, redaction
│   ├── kassensturz-mcp/             # local MCP server exposing the core (read-only by default)
│   ├── kassensturz-receipts/        # post-1.0: OCR + transaction-matching for photographed/scanned receipts
│   ├── kassensturz-mobile-bridge/   # post-1.0: Rust↔Kotlin FFI exposing core+receipts to the native Android app — see D-019, Q-018
│   ├── kassensturz-sync/            # post-1.0: shared-host ↔ thin-client access for multi-user (Member-scoped) — see D-022, Q-021/Q-022
│   ├── kassensturz-cli/             # CLI front end (testing, scripting)
│   └── kassensturz-ui-bridge/       # cxx-qt bindings exposing the core to QML
├── ui/qml/                     # Kirigami UI: overview, register, reports, settings
├── plugins/examples/           # sample importer, report, automation
├── packaging/                  # flatpak/, windows/, macos/
├── docs/
└── .github/
```

## Dependency rules

- `kassensturz-core` depends on **no other workspace crate**.
- `kassensturz-store`, `kassensturz-import`, `kassensturz-charts`, `kassensturz-ai`, `kassensturz-plugin-host` depend on `kassensturz-core`.
- Front ends (`kassensturz-cli`, `kassensturz-ui-bridge`, `kassensturz-mcp`) depend on core, store and plugin host. **Core never knows about a UI.**
- OS-specific behavior lives only in `kassensturz-platform` behind traits.
- Built-in importers and exporters use the public plugin API.

## Plugin model

- Extension points: Importer, Exporter, Report, Automation.
- Versioned API. Manifest declares name, version, target API version, permissions.
- Narrow, permission-checked data access. No raw SQL access.
- WASM for sandboxing and cross-platform portability. JSON-RPC for plugins that need full OS access or other languages.

## Transactions and Parts (splits)

- A transaction can be broken down into multiple categorized **Parts** (UI term for `Split`, see `docs/GLOSSARY.md`) at any time, not only at import — see D-017.
- Editing Parts auto-calculates one remaining Part's amount as "total minus what's already entered." Saving is rejected unless all Parts sum exactly to the transaction's original amount — enforced in `kassensturz-core`, not just in the UI.
- Manual entry only in this decision; deriving Parts from a scanned receipt (OCR) is taken up separately in D-018.

## Multiple accounts and Transfers

- Multiple accounts live side by side in one book (D-003); the UI shows an overview across all of them, not one account at a time — see M3 in `docs/ROADMAP.md`.
- When import sees a debit in one of the user's accounts and a matching credit in another account **already in the same book**, it proposes merging them into one double-entry **Transfer** (both splits are real asset/liability accounts, no income/expense category) instead of recording them as two unrelated transactions. See D-021.
- Matching: amount (same currency) or a plausible exchange rate for cross-currency transfers (D-004) + date-proximity + both accounts already present in the book. External IBANs are never candidates.
- Always a suggestion the user confirms, never a silent merge (Q-019 covers whether a "trust this pair" setting should reduce that friction later; Q-020 covers the exact tolerances).
- Reports exclude confirmed Transfers from income/expense totals — a direct consequence of the existing double-entry model (§5), not a separate reporting rule.

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
- A Rule = conditions (field + operator + value, matched ALL or ANY) + actions (set category, set payee — see Q-011).
- Field registry is extensible, not hardcoded: at minimum Sender/Payee name, Sender IBAN, Amount, Remittance information, Date, Currency.
- Regex is an explicit opt-in per condition, backed by Rust's `regex` crate for guaranteed linear-time (ReDoS-safe) matching. See D-015.
- Rules form a tree. A sub-Rule is only evaluated if its parent matched; a matching sub-Rule's action overrides the parent's. See Q-010 for ordering between top-level Rules.
- Applied automatically to newly imported transactions; a manual "re-run Rules" action can re-apply them to existing transactions, always undoable.
- Deterministic and code-driven — entirely separate from the AI model below (§8 of AGENTS.md does not apply to Rules).

## Receipt capture (post-1.0)

- One shared pipeline in `kassensturz-receipts` behind two entry points: a future KassenSturz Android companion app (camera capture) and a desktop-only function that imports an existing scan/photo file — see D-018.
- Pipeline: OCR extraction → deterministic matching heuristic (amount + date-proximity + fuzzy payee name) against already-imported transactions → proposes a Parts breakdown (D-017) from the receipt's line items.
- The match and the proposed Parts are always a suggestion; nothing is written to a transaction without user confirmation.
- Must work fully with AI off (D-013). An optional AI-assisted extraction path through `kassensturz-ai` may help on messy receipts once AI is on, but is never required.
- Phone-to-desktop transfer is not a project-run hosted service (§3.3 non-goal); see Q-015 for the local-network-pairing vs. file-drop choice.
- The Android app is a **separate native app** (D-019), not the Qt 6/Kirigami stack via Qt for Android. It reuses `kassensturz-core`/`kassensturz-receipts` through `kassensturz-mobile-bridge` (a Rust↔Kotlin FFI binding, e.g. UniFFI — not pinned yet) rather than reimplementing money-handling or matching logic natively. Exact UI framework and FFI approach are open (Q-018); adding Android to the Portability principle's platform list (AGENTS.md §2) still needs its own maintainer sign-off when this is actually scheduled.

## AI model

- `kassensturz-ai` defines a provider trait. Adapters: OpenAI-compatible HTTP (Ollama, llama.cpp server, LM Studio, vLLM), plus cloud providers.
- A consent layer decides what data may leave the machine, per provider, with preview and redaction.
- Changes proposed by AI are represented as reviewable change sets and applied only after user confirmation.
- `kassensturz-mcp` serves the core to the user's own AI tools. Read-only by default.
- A global AI-off switch is a kill switch: `kassensturz-mcp` does not bind its listening port at all while AI is off, and `kassensturz-ai` runs no provider code. The UI layer hides (not disables) every AI-related element in that state. See AGENTS.md §8, D-013.

## AI memory

- `kassensturz-ai` keeps its own local "memory" (see D-023), analogous to this repo's own AGENTS.md/agent-memory setup — but for interactions with the user, not with code.
- Two parts: a maintained **context document** about this book's own setup, and a **pattern memory** of recurring interactions (e.g. "this payee is often categorized as X"). Both are rows in `kassensturz-store` (D-016), not separate files, so they stay inside the one-file book.
- Purpose: let AI **propose promoting a repeated pattern into a Rule (D-014) or a saved report** — never a parallel source of authority. Never caches balances or totals; those are always recomputed by code.
- Passive/on-demand only: consulted while the user is actively using AI, no unprompted notifications in v1 (Q-024).
- Follows every existing AI rule: off entirely when the AI off-switch is on (D-013); any of it sent to a provider goes through the same consent/preview/redaction flow as other context (§8); treated as data, never instructions, like any other untrusted text.
- User can view, edit or delete entries, individually or all at once, any time.
- Once Members exist (D-022), scoped the same way Account visibility already is — inherited, not reimplemented.
- Whether `kassensturz-mcp` exposes any of this to external AI tools is open (Q-025); retention/frequency tuning is open (Q-026).

## Data and portability

- One SQLite file per user "book" in v1. Versioned migrations with automatic backup before every migration.
- Paths and keychain access go through `kassensturz-platform`, so Plasma, GNOME, Windows and macOS conventions are handled in one place.
- `kassensturz-store` exposes a backend-agnostic storage trait; SQLite is the only implementation for v1. Post-1.0, PostgreSQL, MySQL/MariaDB and other well-known SQL engines may be added as further implementations without changing `kassensturz-core`. See D-016, Q-012.
