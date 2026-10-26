# Connect an AI

KassenSturz can work with an AI in two different ways. Pick the one that fits what you already have. AI is off until you switch on **AI features** in Settings, and everything below only works while it is on.

| You have | Use | Where you talk to the AI |
|---|---|---|
| An OpenAI API key | **Route 1:** the chat inside KassenSturz | KassenSturz's chat sidebar |
| A program on your own computer: Ollama or LM Studio | **Route 1:** the chat inside KassenSturz | KassenSturz's chat sidebar |
| A Claude subscription (Claude Desktop or Claude Code), or another AI tool that speaks MCP | **Route 2:** your own AI tool reads your book | The AI tool's own window |

**The difference in one sentence:** in route 1, KassenSturz sends your questions to a provider you chose; in route 2, an AI tool you already use asks KassenSturz for what it needs, and KassenSturz can't see or limit what that tool does with it.

## Route 1: the chat inside KassenSturz

Open **Settings → AI provider and privacy…**. Type the provider's **address**, choose how its **key** is kept, and press **Save**. If you don't know the model's name, leave it empty: after saving, KassenSturz asks the provider which models it offers and you pick one from the list, then press **Save** again. **Test connection** checks the address and the key at any time; it sends nothing from your book.

### OpenAI (with an API key)

- Address: `https://api.openai.com/v1`
- Key: create an API key in your account at OpenAI and paste it into KassenSturz.
- **A ChatGPT subscription is not an API key and doesn't work here.** The API is billed separately from ChatGPT Plus or Pro. You pay only for what you use.
- OpenAI runs on another computer, so KassenSturz shows what is sent and asks for your agreement before the first question.

### Ollama (on this computer)

- Install and start Ollama, and download a model with `ollama pull <model>`.
- Address: `http://localhost:11434/v1`
- No key needed: choose "The provider needs no key".
- Nothing leaves your computer.

### LM Studio (on this computer)

- In LM Studio, load a model and start the local server (the **Developer** tab).
- Address: `http://localhost:1234/v1`. **The `/v1` at the end is needed.** Without it LM Studio answers with an error; KassenSturz notices this and offers a button, **Add /v1 to the address**.
- No key needed.
- Nothing leaves your computer.

A small local model may answer less well than an online one. Whatever the model says, the numbers in your book come from KassenSturz, not from the AI.

### If the chat shows an error

The message says what happened and what to do. When the settings can fix it, it has a button **Open the AI settings**.

## Route 2: your own AI tool reads your book (MCP)

This works with Claude Desktop or Claude Code on a Claude subscription, with no API key, and with other tools that speak the Model Context Protocol (MCP). KassenSturz comes with a small program, `kassensturz-mcp`, which the AI tool starts by itself. It talks to the tool only through a direct pipe: no network port is opened.

1. In KassenSturz, switch on **AI features** in Settings.
2. Tell your AI tool how to start the program:
   - **Claude Desktop:** open **Settings → Developer → Edit Config**. This opens `claude_desktop_config.json`. Add this under `"mcpServers"` and restart Claude Desktop:
     ```json
     "kassensturz": {
       "command": "flatpak",
       "args": ["run", "--command=kassensturz-mcp", "io.github.vondor.KassenSturz"]
     }
     ```
   - **Claude Code:** run `claude mcp add kassensturz -- flatpak run --command=kassensturz-mcp io.github.vondor.KassenSturz`
   - **KassenSturz from your distribution instead of the Flatpak:** the command is `kassensturz-mcp`, with no arguments.
3. Ask your AI tool about your accounts. It can read accounts, categories, balances, totals per category, transactions and Rules. It never sees PINs, bank logins or account numbers.

It reads the book KassenSturz opened last. KassenSturz may stay open.

### Letting it prepare changes

By default your AI tool can only read. To let it propose changes (new categories, new Rules, a category for transactions), turn on **Settings → AI provider and privacy… → Your own AI tools → Let my AI tools prepare changes**.

Nothing changes without you: the proposal appears in KassenSturz's chat sidebar, you read it, and only **Confirm and make these changes** applies it (Undo takes it back). **KassenSturz must be open** with the same book; if it isn't, your AI tool hears so after about 10 seconds.

### Where your data goes

The answers go to your AI tool, and from there to whatever model that tool uses. That is up to the tool you chose.

See also [Notes for the user guide](NOTES.md) for details of each step.
