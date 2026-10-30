# Connect an AI

KassenSturz can work with an AI in two different ways. Pick the one that fits what you already have. AI is off until you switch on **AI features** in Settings, and everything below only works while it is on.

| You have | Use | Where you talk to the AI |
|---|---|---|
| An OpenAI API key | **Route 1:** the chat inside KassenSturz | KassenSturz's chat sidebar |
| A program on your own computer: Ollama or LM Studio | **Route 1:** the chat inside KassenSturz | KassenSturz's chat sidebar |
| A Claude subscription (Claude Desktop or Claude Code), or another AI tool that speaks MCP | **Route 2:** your own AI tool reads your book | The AI tool's own window |

**The difference in one sentence:** in route 1, KassenSturz sends your questions to a provider you chose; in route 2, an AI tool you already use asks KassenSturz for what it needs, and KassenSturz can't see or limit what that tool does with it.

## Route 1: the chat inside KassenSturz

Open **Settings → AI provider and privacy…**. Type the provider's **address**, choose how its **key** is kept, and press **Save**. If you don't know the model's name, leave it empty: after saving, KassenSturz asks the provider which models it offers and you pick one from the list, then press **Save** again. **Test connection** checks the address and the key at any time, and once a model is chosen it also asks that model to call a made-up test function, so you see whether it can use KassenSturz's functions. It sends nothing from your book; **Show what the test sends** shows the test question exactly.

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

**Set the context length of a local model to 16000 or more.** LM Studio starts models with only 4096, which is too little for the instructions, the functions and a conversation: the model then silently loses the start or the end of it and answers an old question. In LM Studio it is the model's setting **Context Length** when you load it; in Ollama it is `num_ctx`. KassenSturz shows a warning under an answer when the provider counted far fewer tokens than it sent. A small local model may answer less well than an online one. Whatever the model says, the numbers in your book come from KassenSturz, not from the AI.

### Choosing a local model

A local model has to be able to **call functions** ("tool use"). That is how it asks KassenSturz for your balances and totals instead of guessing them. In LM Studio the model list shows a **Tool use** badge; if the badge is missing, the model can't read your book and answers only from a short overview, so pick another file of the same model (for example the Q4_K_M one) or another model.

What we found when we tested local models on a made-up household book, asking ten fixed questions (totals, search, a balance, categories, adding a category, the biggest expenses, a Rule, renaming a category, a German question, and one KassenSturz can't do):

- **Context length 16000 or more**, and a model of **at least about 4 billion parameters**.
- **Gemma 4 E4B and Qwen3.5 9B** (both the Q4_K_M files in LM Studio) answered all ten questions correctly in the last round. The much larger **Qwen3.6 35B A3B** was almost as good and needs a lot of memory.
- Smaller models, or models that run out of memory and answer slowly, tend to skip the functions and answer from memory. Those answers can look right and be wrong.
- The same model can do better or worse from one run to the next, so don't treat this as a promise.

Whatever the model says, check numbers that matter against KassenSturz's own screens. Changes the AI proposes never happen without your **Confirm**.

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

By default your AI tool can do nothing: under **What the AI may do** on the same page you switch on what it may read and change, row by row. To let it propose changes, turn on **Settings → AI provider and privacy… → Your own AI tools → Let my AI tools prepare changes**. In the same grid you switch on which kinds of change it may propose: new categories, new Rules, putting transactions into categories, renaming and moving categories, changing and reordering Rules, and deleting Rules. A change of these boxes counts the next time your AI tool asks for its list of functions, so quit it and start it again to see it.

**Then quit your AI tool completely and start it again.** The tool asks KassenSturz for its list of functions only when it starts, so the functions for preparing changes appear after the restart. The same goes for switching the setting off.

Nothing changes without you: the proposal appears in KassenSturz's chat sidebar, you read it, and only **Confirm and make these changes** applies it (Undo takes it back). **KassenSturz must be open** with the same book; if it isn't, your AI tool hears so after about 10 seconds.

### Where your data goes

The answers go to your AI tool, and from there to whatever model that tool uses. That is up to the tool you chose.

See also [Notes for the user guide](NOTES.md) for details of each step.
