# Using an AI with KassenSturz

*This page is for people who would like to ask an AI about their household book and have never set one up. It explains every word and every step. You don't need any of this: KassenSturz is complete without AI, and the rest of this handbook never uses it.*

## What this is, in plain words

- An **AI** here means a program you can ask questions in ordinary sentences, such as "How much did I spend on groceries last month?", and that answers in sentences.
- The part that does the answering is called a **model**. A model is one large file. There are many models, some better than others.
- A **provider** is whatever runs the model for you. It can be a program on your own computer or a service on the Internet.
- KassenSturz reaches the provider at an **address**, a line of text much like the address of a web page.
- A **key** is a kind of password some Internet services give you. A program on your own computer needs none.

In KassenSturz, the AI is **off** until you switch it on. While it is off, nothing about it is shown and nothing of it runs.

## What the AI can and can't do

- It can answer questions about your book and prepare changes, for example suggest categories or Rules.
- **It changes nothing by itself.** Every change is shown to you first, and only your click on **Confirm and make these changes** makes it. **Undo** takes it back.
- **The numbers come from KassenSturz, not from the AI.** The AI asks KassenSturz for your totals and balances. Still, check a number that matters on KassenSturz's own screens.
- It gives no tax, legal or investment advice you should rely on.
- It never sees your banking PIN or your bank login.

## Which way is right for you?

| You would like | Take | What leaves your computer |
|---|---|---|
| To keep everything on your own computer | **Way 1:** a program on your computer | Nothing |
| To use a paid Internet service you have a key for | **Way 2:** an Internet provider | Your questions and the parts of your book needed to answer them; KassenSturz shows you exactly what, and asks first |
| To use an AI program you already have, such as a desktop AI app that can use "MCP" | **Way 3:** your own AI tool | Whatever that tool sends on; KassenSturz can't see or limit that |

If you are unsure, take way 1.

## Way 1: an AI on your own computer

You need a fairly new computer. A graphics card with 8 GB of memory or more makes the answers come in seconds instead of minutes. If you don't know what your computer has, simply try: nothing can break.

This way uses **LM Studio**, a program that downloads models and runs them on your computer. It can be downloaded at no cost. (Ollama is another such program; its address is in the [longer guide](../CONNECT_AN_AI.md).)

### Step 1: install LM Studio

1. Open your web browser, the program you use to look at web pages.
2. Type `lmstudio.ai` into the address line at the top and press Enter.
3. The page offers a download for your system (Windows, Linux or Mac). Click it and wait until the file has arrived in your **Downloads** folder.
4. Open the file and follow what the screen says, as with any program you install.
5. Start **LM Studio**.

### Step 2: get a model

1. In LM Studio, look for the place where you search for models. It may offer you a first model by itself when it starts.
2. Choose a model that carries the badge **Tool use**. This matters: only such a model can ask KassenSturz for your figures. A model without it answers only from a short overview.
3. In our tests on a made-up household book, models of about 9 billion parameters (written "9B" in the name) worked well and fit on a graphics card with 8 GB. Smaller ones made more mistakes; much larger ones were very slow.
4. Download the model. It is several gigabytes, so this takes a while.

### Step 3: load the model with enough working memory

When you load the model in LM Studio, it shows a setting **Context Length**. This is how much text the model can keep in mind at once. Set it to **32768 or more**; 65536 is better if your computer manages it. With the small value LM Studio starts with, the AI loses the thread and answers wrongly or not at all.

### Step 4: start LM Studio's local server

A "server" here is only the part of LM Studio that lets other programs on your computer, such as KassenSturz, talk to the model. In LM Studio, open the **Developer** tab and start the server there. Leave LM Studio open while you use the AI in KassenSturz.

### Step 5: tell KassenSturz where the AI is

1. In KassenSturz, open **Settings** in the menu on the left.
2. Switch on **AI features**.
3. Choose **AI provider and privacy…**.
4. Under **Address**, type exactly: `http://localhost:1234/v1`
   "localhost" means "this computer". A green line says the address looks right; a red one says what is wrong.
5. For the key, choose **The provider needs no key**.
6. Press **Save**. KassenSturz asks LM Studio which models it has; pick yours under **Model** and press **Save** again.
7. Press **Test connection**. It sends nothing from your book and tells you whether the model can use KassenSturz's functions.

If you answered **Yes, I have an AI on this computer** at the end of the guided setup (page 3), you are already on this page at point 4.

### Step 6: say what the AI may do, and ask

On the same page, under **What the AI may do**, every permission starts switched off: the AI reads and changes nothing in your book until you allow it, row by row. Switch on what you are comfortable with. Reading is enough for questions.

Then choose **Ask AI** at the top of the window and type your question. A question usually takes 20 to 90 seconds on a computer with a suitable graphics card.

## Way 2: a provider on the Internet

Some companies run models for you and charge for each use. You get an address and a key from them. In **Settings › AI provider and privacy…** you type the address, paste the key and press **Save**.

Because the provider is on another computer, the page lists under **What leaves this computer** what is sent and what never is. Nothing is sent until you tick that you have read it and agree, and before each question the chat shows again exactly what it sends.

Two things people often get wrong:

- A monthly subscription to a chat website is usually **not** a key and doesn't work here. A key is sold separately and billed by use.
- A model on another computer in your own home counts as "another computer" too. KassenSturz asks the same way.

The addresses for common providers are in the [longer guide](../CONNECT_AN_AI.md).

## Way 3: your own AI tool reads your book

If you already use an AI program on your computer that can use "MCP" (a way for AI programs to ask other programs for information), it can ask KassenSturz for what it needs. This takes a few lines in that program's settings. The command is `kassensturz.mcp` for the Snap Store version, `kassensturz-mcp` for the Microsoft Store version, and `flatpak run --command=kassensturz-mcp io.github.vondor.KassenSturz` for the Flatpak; the [longer guide](../CONNECT_AN_AI.md) has the lines for each. At first it may do nothing; you allow reading, and if you like preparing changes, in **What the AI may do**. Every change it prepares still waits for your click in KassenSturz.

## Switching it off again

**Settings**, then switch off **AI features**. Everything about AI disappears from KassenSturz again and nothing of it runs.

**If something looks different:** if the chat shows an error, the message says what happened and what to do; often it has a button **Open the AI settings**. If the message says the model's working memory is too small, go back to step 3.
