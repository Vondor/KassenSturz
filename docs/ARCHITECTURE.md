# Architecture

Headless core, thin UIs, everything extensible through one plugin API.

```
ledger/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── ledger-core/            # domain model + logic; no UI, no I/O specifics
│   │   ├── money               # integer minor units + ISO 4217, rounding modes
│   │   ├── account             # accounts, categories, payees
│   │   ├── transaction         # double-entry transactions + splits
│   │   ├── currency            # currencies, exchange rates, prices
│   │   ├── budget
│   │   └── events              # event bus (TransactionAdded, ...)
│   ├── ledger-store/           # SQLite persistence, migrations, backups
│   ├── ledger-import/          # CSV, OFX, MT940, CAMT.053, KMyMoney (.kmy / SQLite)
│   ├── ledger-plugin-api/      # stable, versioned plugin interface
│   ├── ledger-plugin-host/     # loads and sandboxes plugins (WASM / JSON-RPC)
│   ├── ledger-platform/        # keychain, config paths, notifications behind traits
│   ├── ledger-charts/          # ChartProvider interface + implementations
│   ├── ledger-ai/              # LLM provider abstraction (local + cloud), consent, redaction
│   ├── ledger-mcp/             # local MCP server exposing the core (read-only by default)
│   ├── ledger-cli/             # CLI front end (testing, scripting)
│   └── ledger-ui-bridge/       # cxx-qt bindings exposing the core to QML
├── ui/qml/                     # Kirigami UI: overview, register, reports, settings
├── plugins/examples/           # sample importer, report, automation
├── packaging/                  # flatpak/, windows/, macos/
├── docs/
└── .github/
```

## Dependency rules

- `ledger-core` depends on **no other workspace crate**.
- `ledger-store`, `ledger-import`, `ledger-charts`, `ledger-ai`, `ledger-plugin-host` depend on `ledger-core`.
- Front ends (`ledger-cli`, `ledger-ui-bridge`, `ledger-mcp`) depend on core, store and plugin host. **Core never knows about a UI.**
- OS-specific behavior lives only in `ledger-platform` behind traits.
- Built-in importers and exporters use the public plugin API.

## Plugin model

- Extension points: Importer, Exporter, Report, Automation.
- Versioned API. Manifest declares name, version, target API version, permissions.
- Narrow, permission-checked data access. No raw SQL access.
- WASM for sandboxing and cross-platform portability. JSON-RPC for plugins that need full OS access or other languages.

## AI model

- `ledger-ai` defines a provider trait. Adapters: OpenAI-compatible HTTP (Ollama, llama.cpp server, LM Studio, vLLM), plus cloud providers.
- A consent layer decides what data may leave the machine, per provider, with preview and redaction.
- Changes proposed by AI are represented as reviewable change sets and applied only after user confirmation.
- `ledger-mcp` serves the core to the user's own AI tools. Read-only by default.

## Data and portability

- One SQLite file per user "book". Versioned migrations with automatic backup before every migration.
- Paths and keychain access go through `ledger-platform`, so Plasma, GNOME, Windows and macOS conventions are handled in one place.
