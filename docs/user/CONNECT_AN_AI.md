# Connect an AI

KassenSturz can work with an AI in two different ways. Pick the one that fits what you already have. AI is off until you switch on **AI features** in Settings, and everything below only works while it is on.

| You have | Use | Where you talk to the AI |
|---|---|---|
| An OpenAI API key | **Route 1:** the chat inside KassenSturz | KassenSturz's chat sidebar |
| A program on your own computer: Ollama or LM Studio | **Route 1:** the chat inside KassenSturz | KassenSturz's chat sidebar |
| A Claude subscription (Claude Desktop or Claude Code), or another AI tool that speaks MCP | **Route 2:** your own AI tool reads your book | The AI tool's own window |

**The difference in one sentence:** in route 1, KassenSturz sends your questions to a provider you chose; in route 2, an AI tool you already use asks KassenSturz for what it needs, and KassenSturz can't see or limit what that tool does with it.

## Route 1: the chat inside KassenSturz

Open **Settings → AI provider and privacy…**. Type the provider's **address**, choose how its **key** is kept, and press **Save**. The address is checked while you type: a green line says it looks right, and a red one says what is wrong, for example a missing `https://` or a space; **Save** waits until the address is right. The example under the field is only a hint and is never put into the field. If you don't know the model's name, leave it empty: after saving, KassenSturz asks the provider which models it offers and you pick one from the **Model** list, then press **Save** again. Models that are only for searching (their name has "embed" in it) are shown but can't be chosen, since they can't answer in the chat. **Enter another name…** at the end of the list lets you type a name yourself; if the provider names no models at all, the field stays a plain text field and you type the name. **Test connection** checks the address and the key at any time, and once a model is chosen it also asks that model to call a made-up test function, so you see whether it can use KassenSturz's functions. It sends nothing from your book; **Show what the test sends** shows the test question exactly.

**A provider on another computer** (even one in your home) is set up on the same screen: under the key, **What leaves this computer** lists what is sent and what never is. For an `http://` address a warning says the connection isn't encrypted. One box, **I have read this and agree**, agrees to both, and **Save and test** then saves, agrees and lists the models in one step. Without the box you can still save, but nothing is sent until you agree. Before each question goes, the chat still shows exactly what it sends.

### OpenAI (with an API key)

- Address: `https://api.openai.com/v1`
- Key: create an API key in your account at OpenAI and paste it into KassenSturz.
- **A ChatGPT subscription is not an API key and doesn't work here.** The API is billed separately from ChatGPT Plus or Pro. You pay only for what you use.
- OpenAI runs on another computer, so the settings show what is sent and ask for your agreement on the same screen.

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

### A program on another computer at home (only `http://`)

LM Studio, Ollama and similar programs can also run on another computer, for example a server in your home, and then offer only an unencrypted `http://` address such as `http://192.168.0.10:1234/v1`. KassenSturz refuses such an address at first, because anyone on the way could read what is sent. If you know the computer and trust the network, tick **Allow unencrypted connections** under the address, or press the button of the same name in the message that refused the address; the box appears only for an `http://` address on another computer. An address typed without `http://` or `https://`, such as `192.168.0.10:1234/v1`, gets `http://` added in front. Untick it and press **Save** to stop using the unencrypted connection: KassenSturz then sends nothing more to that address, even though it stays in the field, until you enter an `https://` address or tick the box again.

- KassenSturz then says plainly that the connection isn't secure. For an address in a home network (such as `192.168.…`, `10.…` or a name ending in `.local`) it shows a warning; for an address that may be on the Internet it shows a stronger one. Don't use an unencrypted connection over the Internet.
- Everything else stays the same: the computer is still another computer, so KassenSturz shows what is sent and asks for your agreement before the first question.
- An `https://` address is always the safer choice, and a program on this computer (`localhost`) never needs the box.

**Set the context length of a local model to 32768 or more; 65536 is better (D-944).** KassenSturz's request alone is about 9,500 tokens (most of it the descriptions of its functions), and it grows with the conversation: a long chat, or a question that needs a figure for each month of a year, can fill 32768. At 8192 almost every question fails with a message that the context is too small. LM Studio starts models with only 4096, which is too little for the instructions, the functions and a conversation: the model then silently loses the start or the end of it and answers an old question. In LM Studio it is the model's setting **Context Length** when you load it; in Ollama it is `num_ctx`. KassenSturz shows a warning under an answer when the provider counted far fewer tokens than it sent. A small local model may answer less well than an online one. Whatever the model says, the numbers in your book come from KassenSturz, not from the AI.

### Choosing a local model

A local model has to be able to **call functions** ("tool use"). That is how it asks KassenSturz for your balances and totals instead of guessing them. In LM Studio the model list shows a **Tool use** badge; if the badge is missing, the model can't read your book and answers only from a short overview, so pick another file of the same model (for example the Q4_K_M one) or another model.

What we found when we tested local models on a made-up household book, asking ten fixed questions (totals, search, a balance, categories, adding a category, the biggest expenses, a Rule, renaming a category, a German question, and one KassenSturz can't do):

- **Context length 32768 or more, 65536 recommended**, and a model of **at least about 4 billion parameters**.
- **A graphics card with 8 GB is enough for Qwen3.5 9B** (Q4_K_M): it fits on the card completely, at any of these context lengths, and answered 12 to 13 of 13 everyday questions correctly in a longer test. A typical question takes 20 to 90 seconds on such a card.
- **Leave the model's thinking on in LM Studio.** Turning it off (`enable_thinking`, `/no_think`) did not stop the thinking there and made the answers worse.
- **A model larger than the graphics card's memory** (for example Qwen3.6 35B A3B on an 8 GB card) runs mostly from the computer's main memory. It answered more precisely, but sometimes thought for half an hour without answering. Prefer a model that fits on the card.
- **Gemma 4 E4B and Qwen3.5 9B** (both the Q4_K_M files in LM Studio) answered all ten questions correctly in the last round. The much larger **Qwen3.6 35B A3B** was almost as good and needs a lot of memory.
- **Model size, in the longer night run on a 16 GB graphics card:** a model of about 9 billion parameters answered the everyday questions well, with few refused calls. A model of about 4 billion parameters could answer too, but had 29 to 36 refused calls in 73 questions: it sent a category's ID where a Rule's belongs, or made a category its own parent. KassenSturz refuses those calls and says why, so nothing wrong is saved, but the answers take longer and sometimes need a second try. If you can, pick a model of about 9 billion parameters or more that fits on your graphics card.
- Smaller models, or models that run out of memory and answer slowly, tend to skip the functions and answer from memory. Those answers can look right and be wrong.
- The same model can do better or worse from one run to the next, so don't treat this as a promise.

Whatever the model says, check numbers that matter against KassenSturz's own screens. Changes the AI proposes never happen without your **Confirm**.

### If the chat shows an error

The message says what happened and what to do. When the settings can fix it, it has a button **Open the AI settings**.

## Route 2: your own AI tool reads your book (MCP)

This works with Claude Desktop or Claude Code on a Claude subscription, with no API key, and with other tools that speak the Model Context Protocol (MCP). KassenSturz comes with a small program, `kassensturz-mcp`, which the AI tool starts by itself. It talks to the tool only through a direct pipe: no network port is opened.

1. In KassenSturz, switch on **AI features** in Settings.
2. Tell your AI tool how to start the program. The command depends on how you installed KassenSturz; use the one section below that fits. In every case the program takes no arguments except the Flatpak's, and the AI tool starts it by itself.

   **Snap Store (Linux)**

   - The command is `kassensturz.mcp`, with no arguments.
   - Claude Desktop: open **Settings → Developer → Edit Config** and add this under `"mcpServers"`, then restart Claude Desktop:
     ```json
     "kassensturz": {
       "command": "kassensturz.mcp"
     }
     ```
   - Claude Code: run `claude mcp add kassensturz -- kassensturz.mcp`
   - The Snap needs access to the folder of your book. It has it for your home folder and, once you connect it, for removable drives. Connect or disconnect them on KassenSturz's permissions page in KDE Discover, or with `snap connections kassensturz`.

   **Microsoft Store (Windows)**

   - The command is `kassensturz-mcp`, with no arguments. Windows finds it by itself, and it stays the same after updates.
   - Claude Desktop: use `"command": "kassensturz-mcp"` and no `"args"`, as in the Snap example above.
   - Claude Code: run `claude mcp add kassensturz -- kassensturz-mcp`
   - This works from the Store version that adds this command; before that, connecting your own AI tool is not supported there.

   **Flatpak (Linux)**

   - The command is `flatpak`, with the arguments `run`, `--command=kassensturz-mcp` and `io.github.vondor.KassenSturz`.
   - Claude Desktop: add this under `"mcpServers"` and restart it:
     ```json
     "kassensturz": {
       "command": "flatpak",
       "args": ["run", "--command=kassensturz-mcp", "io.github.vondor.KassenSturz"]
     }
     ```
   - Claude Code: run `claude mcp add kassensturz -- flatpak run --command=kassensturz-mcp io.github.vondor.KassenSturz`

   **From your distribution, or built from source (Linux)**

   - The command is `kassensturz-mcp`, with no arguments. Use it as in the Snap examples above, with `kassensturz-mcp` in place of `kassensturz.mcp`.
3. Ask your AI tool about your accounts. It can read accounts, categories, balances, totals per category, transactions and Rules. It never sees PINs, bank logins or account numbers.

It reads the book KassenSturz opened last. KassenSturz may stay open, and when you open another book there, the AI reads that one from its next answer on.

### Letting it prepare changes

By default your AI tool can do nothing: under **What the AI may do** on the same page you switch on what it may read and change, row by row. To let it propose changes, turn on **Settings → AI provider and privacy… → Your own AI tools → Let my AI tools prepare changes**. In the same grid you switch on which kinds of change it may propose: new categories, new Rules, putting transactions into categories, renaming and moving categories, changing and reordering Rules, and deleting Rules. A change of these boxes counts the next time your AI tool asks for its list of functions, so quit it and start it again to see it.

**Then quit your AI tool completely and start it again.** The tool asks KassenSturz for its list of functions only when it starts, so the functions for preparing changes appear after the restart. The same goes for switching the setting off.

Nothing changes without you: the proposal appears in KassenSturz's chat sidebar, you read it, and only **Confirm and make these changes** applies it (Undo takes it back). **KassenSturz must be open** with the same book; if it isn't, your AI tool hears so after about 10 seconds.

### Where your data goes

The answers go to your AI tool, and from there to whatever model that tool uses. That is up to the tool you chose.

**Fixed notes for the AI** are folded away at the bottom of the AI settings, under **Advanced: fixed notes for the AI**. They are for things that apply to every question, such as "Answer briefly" or "Salary comes on the 28th", and go with every question you ask in the chat. Ask the questions themselves in the chat, not there.

For a slower walk through every step, written for people who have never set up an AI, see [Using an AI with KassenSturz](en/ai.md) (German: [Eine KI mit KassenSturz benutzen](de/ki.md)).
