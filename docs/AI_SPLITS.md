# The AI splits a transaction and attaches its bill (US5.1, US5.12, US5.13 design)

Design for #717 (D-737 to D-739). Today an AI can sort a transaction into one category (`categorize`) and change its fields (`edit_transactions`, D-257), but it cannot break a transaction into Parts, it sees a split only as a list of category names, and it has no way to tie a bill to the transaction it paid for. This design adds those three things for the AI the user chose, behind permission cells that start off (D-252), with every change on a check card the user confirms (AGENTS.md §8).

All examples are invented: the shop "Papierladen Nord", the invoice numbers and the amounts.

**Not [format].** Nothing here changes the book's layout or needs a migration:

- A part's **name** is the part note the book already holds (`splits.note`, D-132). It is the text a user writes about one part ("what the money was for in their own words"), which is what a name per part is.
- A part **not yet assigned** is a part without a category (`splits.account_id` NULL, *Not sorted yet*), which the book and the Parts page already have (D-083, D-118).
- An attached bill is a row of `attachments` (a link and a title per transaction, D-132), which the book has had since the starting layout, with its change history `changed_attachments`. Nothing wrote it until now except the complete export, which already carries it.
- The bill folder is a setting of this computer (`kassensturzrc`), not of the book, because a path means nothing on another computer. Two new permission cells and the folder are new keys; a settings file without them reads as before (cells off, no folder), so no settings migration is needed.

## 1. The split (US5.1, D-737)

### What the user sees

The AI says, for example: *"The debit of €19.99 to Papierladen Nord pays two invoices. I prepared a split."* The check card shows:

| | |
|---|---|
| **Papierladen Nord**, 3 Nov 2026, Checking account | **−€19.99** (from the bank) |
| Office supplies · Invoice RE-0042 printer paper | −€12.50 |
| *Not sorted yet* · Invoice RE-0043 | −€7.49 |
| **The parts add up to the bank total.** | ✓ |

Confirm writes it; one Undo takes the whole set back (the set is one `apply_ai_changes` action, D-198). The transaction page then shows the parts as any split; the Parts page edits them as before.

### The tool

`propose_changes` gets a list `split_transactions` (at most 100 items):

```json
"split_transactions": [{
  "transaction_id": "…",
  "parts": [
    {"category_id": "…", "amount": "-12.50", "name": "Invoice RE-0042 printer paper"},
    {"category_id": null, "amount": "-7.49", "name": "Invoice RE-0043"}
  ]
}]
```

- `amount` is exact decimal text in the transaction's currency, **as seen from the account** (money out negative), the same sign as `search_transactions` shows and `edit_transactions` takes. It is read by the existing `parse_amount`: never rounded, never guessed (D-065).
- `category_id: null` (or leaving it out) books the part to *Not sorted yet*. No new kind of category.
- `name` is optional, at most 512 characters like a note (D-703), trimmed; empty means none.
- **One part removes the split:** the transaction goes back to one part in the account and one other part. "Removing a split works the same way" (#717).
- A transaction may appear once per set, and not also in `categorize` or `edit_transactions` of the same set (`ai-tools.duplicate`).

### The check, in Rust, in integer minor units

1. The transaction has **exactly one part in an open account of the user**, in the transaction's currency, and every other part is a category or *Not sorted yet*, in that currency, with no foreign amount, no rate and no amount from a replaced currency. Otherwise `ai-tools.not-editable` with reason `transfer`, `other-currency` or `split` as today (M6-B215).
2. The other parts carry nothing the user would lose: no tags, number or status (D-132, D-714). Otherwise `ai-tools.not-editable`, reason `part-details`, so the user splits by hand on the Parts page. A tag on the whole transaction stays and counts for every new part (D-713).
3. 1 to 50 parts (`ai-tools.too-many-parts`); every `category_id` is an open category in the transaction's currency (`ai-tools.unknown-category`).
4. **The parts add up to the bank total:** the sum of the parts' minor units, added with `Money`'s checked addition, equals the account part's amount exactly. Otherwise `ai-tools.parts-dont-add-up` with `total`, `sum` and `difference`, written as decimal text so the model can correct itself. There is no automatic rest: a model that wants one writes a *Not sorted yet* part with the difference.
5. The account part is **never changed**: not its amount, date, payee or what the bank delivered (D-067). The other parts are replaced by the new ones; the transaction still balances because of step 4, and `Transaction::new` checks it again.
6. The set is checked again when it is applied (`ai-changes.out-of-date` if the transaction changed since the check card, D-199).

A Rule's mark on a replaced part goes with it; the part is now sorted by the AI and the user, like any edit.

### Permission

A new cell **Transactions › Split** (`Permission::SplitTransactions`), its own cell like *Join Transfers* (D-693): a user who allowed sorting never allowed breaking a payment into parts. Off by default; switching it on also switches *Transactions › Read* on. The field locks apply (D-252): a locked **Category** or **Amount** refuses the whole split (`ai-tools.field-locked`, the parts are new amounts and categories); a locked **Note** refuses names, and a split without names still works.

### Parts in `search_transactions` (#717 item 4)

- A row whose transaction has **two or more parts besides the account's** gets a `parts` list, in the stored order: `{"category", "category_id", "amount", "name"}`, with `"unsorted": true` instead of a category for *Not sorted yet*, and `amount` as seen from the account. The existing `categories` and `category_ids` stay for older prompts.
- `min_amount` and `max_amount` match a row when the account amount **or one part's amount** is in range; such a row says `"matched_part": n` (1-based), so "find the €7.49" finds the split. `text` already matches part notes, i.e. names.
- The `parts` list grows the tool answer; the chat scenario test on token counts is checked with it (US2 lesson).

## 2. Matching a bill by its reference (US5.12, D-738)

A bill names an order or invoice number; the bank text ("Purpose", D-083) of the payment usually carries it. `search_transactions` gets an argument `reference`:

- It matches the bank text, the transaction's note and the part names. Both sides are compared with **letters and digits only, ignoring case**, so `RE-2026/0042` finds `re 2026 0042`. The reference must have at least 4 letters or digits (`ai-tools.reference-too-short`), so a short number never matches half the book.
- With `reference`, the answer says `"matched_by": "reference"`. With no match it says `"found": false` and, in `what_to_do` (the result's `note` is the data notice, D-187), a model-facing note: *"No transaction carries this reference. Report the bill as not found. Do not match it by amount or date."* The tool description says the same. KassenSturz never offers a guess by amount for a reference that matched nothing (#717 item 5).
- **Several bills, one debit:** each reference finds the same transaction. The model then proposes one split with a part per bill, named by its invoice number, and attaches each bill (§3).
- Read-only, under *Transactions › Read*; nothing is stored.

## 3. The bill folder and the attached bill (US5.13, D-739)

**The path (Q-506):** KassenSturz runs no OCR and reads no PDF. The user's own AI tool, outside KassenSturz over MCP, reads the bill itself; KassenSturz only names the folder and stores the link. So no bill text passes through KassenSturz. The in-app chat reading PDFs is a new data-sharing path with a PDF dependency and stays after Gold with #333; in US5 the chat does not get the tools of this section.

### The setting

*Settings › AI › Your own AI tools* (hidden while AI is off, D-013) gets **Bill folder**: a *Choose folder…* button and the chosen path, with *Remove*. It is stored per computer as `[AI] BillFolder` in `kassensturzrc`.

- The button opens the system's folder chooser. In the Flatpak this is the **file chooser portal**, which grants the app that one folder; the manifest gets **no new filesystem permission**. This keeps the portal route open for choosing the book's folder (#515, US5.3), and this design decides nothing for it.
- The path is stored as the chooser returns it (in the Flatpak a document-portal path, which the outside AI can read too, since that mount is on the host).

### The tools (MCP only)

- `bill_folder` (read): the folder and the file types taken (`pdf`, `png`, `jpg`, `jpeg`). Offered only when a folder is set and the cell below is on, so a path never goes to a model the user didn't allow.
- `propose_changes` gets `attach_bills` (at most 100): `{"transaction_id", "file", "title"}`. `file` is a path inside the bill folder; `title` is optional (default: the file name).
  - The file must exist, be a regular file **inside the bill folder** after resolving `..` and links (`ai-tools.bill-outside-folder`), and have one of the four types (`ai-tools.bill-type`); a missing file is `ai-tools.bill-not-found`, no folder set is `ai-tools.no-bill-folder`.
  - The same file on the same transaction twice is `ai-tools.duplicate`. A transaction keeps every bill it has; one more is added after them.
  - Applied in the same set as a split, so one Undo takes back the split and its bills.
- New cell **Transactions › Attach bills** (`Permission::AttachBills`), off by default, switches *Read* on.

### What the user sees

- The check card: *"Attach bill: RE-0042.pdf (from your bill folder)"* under the transaction.
- **The transaction page** lists its bills with **Open bill** and **Remove link**. Open goes through the system (in the Flatpak the OpenURI portal), so KassenSturz never runs the file. Remove link takes away the link only; **KassenSturz never moves, changes or deletes a bill file**, and says so next to the button. This list is not an AI element: it shows with AI off too, as the links are the user's data.
- A bill that isn't in its place anymore: *"This bill isn't where it was: <path>. Put it back there, or remove the link."* (`bill.missing`, no dead end).

## 4. Threats and gates

- **Human gate:** AI data sharing. The parts, their names and the bill folder's path go to the AI the user chose; the AI writes parts and links. Each behind its own cell, off by default, and confirmed on the check card.
- **Untrusted text:** bill text, invoice numbers and part names are data. A name is stored as text and shown as text, never run.
- **A link the AI writes can't point anywhere:** only inside the folder the user chose, only four document types, and opening goes through the system's handler for that type.
- **Money:** the split is checked in integer minor units before the card and again before the write; the account's own amount never changes.

## 5. Build order and tests

1. **US5.1, the split** (strong, high; after this design and US5.2 step b, which also changes `changes.rs` and `read.rs`): `split_transactions`, the cell, `parts` and `matched_part` in `search_transactions`, the check card rows (Rust formats every amount, D-065; the bridge map read back like QML does), error codes in `docs/ERROR_CODES.md`, German strings. Tests: property test that any accepted split balances and leaves the account part's amount unchanged; refusal per rule above; a split and Undo restores the transaction exactly; **a split of a transaction from every importer's golden output** (CAMT, MT940, OFX, CSV, KMyMoney), the seam and X3; the MCP golden file.
2. **US5.12, the reference** (standard, high): `reference`, `found: false`, the description. Tests with bank texts in golden files.
3. **US5.13, the bill** (standard, high): the setting, `bill_folder`, `attach_bills`, the transaction page list, the cell. Tests: paths outside the folder (`..`, a link out), wrong types, a missing file, attach plus split in one set and one Undo, the complete export carrying the link.

## 6. Decisions

- **D-737:** the AI splits a transaction through `split_transactions`, behind its own cell, with names as part notes and *Not sorted yet* as a part without category; the account part never changes; the parts must add up exactly in minor units; one part removes the split; `search_transactions` returns `parts` and matches a part's amount.
- **D-738:** `search_transactions` takes `reference`, compared on letters and digits ignoring case, at least 4 of them; no match says "not found" and is never guessed by amount.
- **D-739:** a per-computer bill folder chosen through the system's folder chooser (the portal in the Flatpak, no new permission); over MCP only, `bill_folder` and `attach_bills` behind their own cell store a link inside that folder in `attachments`; the transaction page opens and unlinks bills and never touches the files.
