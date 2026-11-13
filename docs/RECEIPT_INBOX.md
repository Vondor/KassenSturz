# The receipt inbox (US7, #333 design)

**Status:** design for US7 (#333), making D-751 and D-752 Decided (rows on the `docs/us7-p2` ledger branch). Nothing here is built yet; the build is a later step (strong, high) after the steps that also change the AI chat and tool files (#808, #852) have merged. Stage: Alpha, so no migration (D-274), and nothing here changes the book's layout, so it is not [format].

Human gate: **AI data sharing** (the inbox folder's path and the file names go to the AI the user chose; the AI proposes moves). The gate was given for US5.13 (#333's comment of 2026-11-12); this design does not widen what leaves the machine (see §6).

All examples are invented: the shop "Papierladen Nord", the invoice numbers, the accounts and the amounts.

## 1. What is new, and what is not

US5.1, US5.12 and US5.13 (`docs/AI_SPLITS.md`, D-737 to D-739) already let the user's own AI, outside KassenSturz over MCP, read a bill, find its transaction, split it and attach the bill as a **link** to a file in a *bill folder*. KassenSturz never moves that file. That stays exactly as it is.

The inbox adds, for people who want it:

1. **Two folders**, an *inbox* and an *archive*, each chosen with the system's folder chooser (the portal in the Flatpak), through the shared helper `choose_folder` and `FolderGrant` of `docs/BOOK_FOLDER.md`.
2. **A move on success.** When the user confirms the change set, KassenSturz moves each bill from the inbox into the archive and links it at its new place. An empty inbox means everything is filed.
3. **Code chooses the place**, never the AI.
4. **Undo** puts the file back into the inbox and removes the link.
5. **On the transaction:** *Open bill* and *Show in folder*.

No OCR engine and no PDF reader in KassenSturz (D-751): the user's own AI reads the document. The in-app chat reading bills stays after Gold with #803. So the inbox, like the bill folder, is **MCP only**.

**Why a separate tool and cell, not a mode of `attach_bills`:** D-739 promises that attaching a bill never moves a file. A user who switched *Attach bills* on in US5 must not get moves by accident. So moving has its own tool (`file_bills`) and its own cell (*Transactions › File bills from the inbox*, `Permission::FileBills`, off by default). Both folders are new settings keys, so nothing moves until the user has chosen two folders and switched the cell on.

## 2. What the user sees

*Settings › AI › Your own AI tools* (hidden while AI is off, D-013) gets, under *Bill folder*:

- **Inbox for new bills**: *Choose folder…*, the real path, *Remove*.
- **Archive for filed bills**: the same.
- One sentence: "Drop bills into the inbox. Your AI reads them, and when you confirm, KassenSturz moves each one into the archive, sorted by account and category. Undo puts it back."

The two folders are refused, with a plain sentence and no dead end (`settings.inbox-archive-nested`), when they are the same folder, when one lies inside the other (the archive would be read as an inbox again), or when one is KassenSturz's own data folder or the book's folder. Paths are compared after resolving links.

**The check card** (the confirmation in the KassenSturz window, D-035) shows per bill:

> File bill: RE-0042.pdf (from your inbox) → Papierladen Nord, 24.50 EUR, 05 Nov 2026
> Goes to: `Archive/Checking account/Groceries/261105_RE-0042.pdf`
> The bill says 24.50 EUR · your bank booked 24.50 EUR ✓

and above the list one line: "3 files will be moved from *Inbox* to *Archive*. Each is copied and checked first; the original is removed only after your book is saved. Undo puts them back." Every figure on the card is formatted in Rust (D-065).

A mismatch is shown, never hidden: "The bill says 23.50 EUR, your bank booked 24.50 EUR." That row is **not ticked** (§4); the user ticks it on purpose if a tip or a part-payment explains it.

**On the transaction page** the bill list (US5.13) gets, per bill, **Open bill** (the system's handler; the OpenURI portal in the Flatpak) and **Show in folder** (the file manager's *show item* through the portal, otherwise the containing folder is opened), also with AI off, since the links are the user's own data. *Remove link* removes the link only and says so: the file stays where it is.

A bill that is no longer in its place: *"This bill isn't where it was: <path>. Put it back there, or remove the link."* (`bill.missing`, as today).

## 3. The tools (MCP only)

- `inbox` (read): the inbox folder and its files that are filable: regular files, type `pdf`, `png`, `jpg` or `jpeg`, at most 200 listed, each with name, size and modified date. Offered only when both folders are set and the cell is on, so no path goes to a model the user didn't allow. Subfolders are not entered; a link in the inbox is not followed and not listed.
- `propose_changes` gets `file_bills` (at most 50 per set), each `{ "transaction_id", "file", "title"?, "bill_total"?, "bill_currency"? }`:
  - `file` is a path inside the inbox; it must exist, be a regular file inside the inbox after resolving `..` and links (`ai-tools.bill-outside-folder`), have one of the four types (`ai-tools.bill-type`), and be at most 50 MB, 500 MB per set (`ai-tools.bill-too-big`). Missing: `ai-tools.bill-not-found`. A folder not set: `ai-tools.no-inbox`, `ai-tools.no-archive`. A file already linked to any transaction: `ai-tools.bill-already-filed`; the same file twice in one set: `ai-tools.duplicate`.
  - `title` is optional text, used as the link's title and as the file name stem; the default is the file's own stem. It is data: sanitised (§5), never run.
  - `bill_total` is exact decimal text of the figure the AI read, `bill_currency` an ISO 4217 code (D-751, §4). Both optional; without them the card says "the AI did not give a total, check the bill yourself".
  - **No destination field exists.** The archive path is built by code from the transaction (§5). Text inside a document cannot choose where a file goes.
  - It combines with `split_transactions` in the same set: one confirmation, one Undo for the split, the link and the move.
- `attach_bills` and `bill_folder` stay as D-739 has them.

## 4. Every figure is checked in code (D-751)

KassenSturz reads no bill, so the figures come from the AI and are treated like any figure an AI proposes: as a claim that code checks.

1. **Parts** (D-737, unchanged): the parts of a split add up exactly to the bank total in integer minor units, before the card and again before the write.
2. **Bill total against the bank:** `bill_total` is parsed with the book's exact parser for the given currency (no floating point, §3.1 rule 1; ISO 4217 decimals; more places than the currency has is a refusal, `ai-tools.amount`), then compared in minor units with the transaction's amount as booked on the account (absolute value, the bank's sign is not the bill's). Equal: a green tick on the card. Different, or in another currency than the transaction: a plain warning, the row **starts unticked**, and nothing is filed unless the user ticks it. The AI cannot tick it; the confirmation is the user's.
3. **Never silently corrected:** a mismatch is never "fixed" by changing the transaction or a part. The user's amount is the bank's.

Dates are shown on the card but not checked: a bill's date and the booking date legitimately differ.

## 5. The path, built by code

`<archive>/<account>/<category>/<YYMMDD>_<name>.<ext>`

- **account:** the transaction's account name. **category:** the category's own name (not the whole tree); for a split, the category of the part with the largest amount (the first on a tie); a part without a category gives `Not sorted yet`. **YYMMDD:** the transaction's date (a calendar date, D-061). **name:** the sanitised title, or the file's own stem. **ext:** the original extension, lower case, one of the four types.
- **Sanitising**, for every component, one function with property tests: Unicode NFC; control characters, `/ \ : * ? " < > |` and a leading or trailing dot or space removed; Windows' reserved names (`CON`, `NUL`, `COM1` …) get a trailing `_`; empty becomes `bill`; at most 80 characters per component, and the whole path under 240 so it works on Windows. The result never contains `..` and never starts with a separator, so it cannot leave the archive. After building it, the real path of its parent is checked to lie inside the archive.
- **Never overwrite:** if the name is taken, compared without regard to case so it also works on Windows and macOS, a number is added: `261105_RE-0042-2.pdf`.
- Missing account or category folders are created. A closed account is no reason to refuse: it keeps its transactions and their bills.

## 6. The safe move

For each file, in this order (the move is the platform's helper `move_file_safely`, which also serves the book's own move in BOOK_FOLDER and #647 later):

1. **Fingerprint.** When the AI proposes, the file's size and modified time are recorded in the change set. Before the move they are read again; a change means "this bill changed after your AI read it" (`bill.changed`) and the file stays.
2. **Copy** to `<final>.part` in the archive (a name sync tools leave out by default), then `fsync`.
3. **Verify:** read both files back and compare them byte for byte. No hash and no new dependency are needed. A difference removes the copy and stops: `bill.move-failed` with reason `verify`.
4. **Into place** with a rename that refuses to replace a file (`RENAME_NOREPLACE`, `renamex_np`/`RENAME_EXCL`, `MoveFileEx` without replace; the same wrapper as `docs/BOOK_FOLDER.md`), then `fsync` of the folder.
5. **Save the book:** the link (`attachments` row: the archive path and the title) is written inside the same `ApplyAiChanges` action as the rest of the set. If the save fails, the archived copy is removed and the original is untouched.
6. **Remove the original** from the inbox, only now. If that fails, the bill is filed and linked, a copy is left in the inbox, and the page says so with the path: "The bill is filed. KassenSturz couldn't remove the copy in your inbox: <path>."

Copy, verify and remove, not a rename, because the inbox and the archive can be on different drives and in the Flatpak are two separate portal grants, where a rename across folders may not exist.

**A journal for crashes.** Per computer, next to `kassensturzrc`, KassenSturz keeps `receipt-moves.jsonl`: one line per file, written before step 2 (`from`, `to`, `fingerprint`, the history action it belongs to once step 5 has run). At start-up and whenever the inbox page opens, each line left unfinished is settled by **facts, never by guessing, and never by deleting the last copy**:

| What is found | What happens |
|---|---|
| Original in the inbox, a copy in the archive, no link in the book | The copy is removed (the original is the only real one) |
| Link in the book, original still in the inbox, identical copy in the archive | The original is removed |
| Link in the book, original gone, file in the archive | Done, nothing to do |
| Original gone, no link, file in the archive | The archive file stays; a note says "A bill is in your archive without a link: <path>" |
| Original and archive copy both gone | Nothing to restore; the line is dropped and the user is told |

## 7. Undo, Redo and the link stay consistent

The book's Undo works on the book. The file lives outside the book. The rule: **Undo of a change set that moved files moves them back; the journal names which ones.**

- **Undo:** the bridge finds the journal lines of that history action. For each: move the file from the archive back into the inbox with the safe move of §6 (if the inbox name is taken, a number is added), then undo the book's change (the link goes with it). If a file can't be moved back (it was moved or deleted in the archive by the user, or the inbox is gone), the page says why, **nothing is undone for that file**, and offers **"Undo anyway, and leave the file where it is"**, a deliberate step: the link goes, the file stays, and the page says where it is. Empty archive folders left behind are removed if empty.
- **Redo:** the reverse: the same safe move into the archive path recorded in the journal, then the book's redo restores the link. If the inbox file is gone, Redo refuses and says so.
- **A later change** to the transaction (another category, another date) does not rename the archived file. The link stays right; the folder name records where the bill was filed, not where the transaction is now. Deleting the transaction removes its links and leaves the file in the archive; it is not moved back, because that is no Undo.
- **Another computer:** the journal is per computer. An Undo of a filed bill on a computer without its journal line removes the link only and says "the bill stays in the archive: <path>".
- **The history window** (M3.8) says "Filed 3 bills from the inbox" for the action, so the user sees what Undo will do before pressing it.

Step 0 of the build checks how the store's history names an action (an id that survives a restart) so the journal can key on it; if it doesn't, the journal keys on the link's path, which is just as exact.

## 8. Errors: no dead end

Every message says what happened and what to do next; the core returns codes only (D-034). New codes for `docs/ERROR_CODES.md`: `ai-tools.no-inbox`, `ai-tools.no-archive`, `ai-tools.bill-too-big`, `ai-tools.bill-already-filed`, `bill.changed`, `bill.move-failed` (reasons `copy`, `verify`, `exists`, `no-space`, `no-permission`, `archive-gone`, `inbox-gone`) and `settings.inbox-archive-nested`; `ai-tools.bill-outside-folder`, `bill-type`, `bill-not-found`, `duplicate`, `amount` are reused. A failure of one file never stops the others: each file is a step of its own, the card lists which ones were filed and which stayed in the inbox, and the book's part of the set is applied only for the files that moved (the user confirmed the set, so the rows of failed files are dropped, not guessed).

## 9. Threats and gates

New row **T-62 extended** (a row of its own is added at build time):

| Threat | Answer |
|---|---|
| A bill's text or a file name tells the AI to file it into some other folder, or to link the wrong transaction | No destination field; code builds the path from the book's own names; the card shows both ends; the user confirms |
| A path or a link that leads out of the inbox, `..`, an absolute path, a link in the inbox pointing at another file | Resolved before use, regular files only, links not followed (T-62) |
| The archive path leaves the archive (a category named `../../`) | Sanitised components, real-path check of the parent |
| A file is replaced after the AI read it | Fingerprint at proposal, re-read before copy |
| A move that loses the only copy | Copy, verify, link saved, only then remove; the journal never deletes the last copy |
| A figure the AI read is wrong | Checked in code against the bank amount; mismatch unticked (§4) |
| A user with *Attach bills* on gets moves they never agreed to | Separate cell, off by default, two new folder settings |
| AI off | `inbox`, `file_bills`, both folder settings and the cell are hidden; the transaction page's *Open bill* and *Show in folder* stay (the user's own links) |

**What leaves the machine** is unchanged from D-739: the folder's path, the file names and, when the AI reads a file itself, the file, all to the AI the user chose and none of it by KassenSturz. KassenSturz reads no file's content except to copy and compare bytes.

**Flatpak:** no new filesystem permission. **Step 0 of the build, in a real Flatpak (the same open point as #647 and BOOK_FOLDER step 0):** (a) a file can be read, copied out of and removed from an inbox granted by the folder portal, (b) a file can be created, renamed without replacing and `fsync`ed in a separate archive grant, (c) the real paths come back from `GetHostPaths`, (d) *Show in folder* works through the portal. If (a) or (b) fails the build stops and the question goes to the maintainer; it must not fall back to a broad permission.

## 10. Build order and tests

1. **Core, no UI** (`kassensturz-core` and `kassensturz-platform`): the path builder and sanitiser, `move_file_safely`, the journal and its settling table, `Permission::FileBills`. Tests: a property test that any title, account and category name gives a path inside the archive, within the length limits and free of reserved names; the collision rule (also by case); the move with a failure injected at every step (no space, kill between steps, verify difference, remove refused) leaving a state the settling table repairs; no symlink is followed out; a file changed between proposal and confirm stays.
2. **Tools and card** (`kassensturz-ai-tools`, `kassensturz-mcp`): `inbox`, `file_bills`, the card rows, mismatch unticked. Tests: figures from the AI against the bank in exact minor units for currencies with 0, 2 and 3 places; no float anywhere; a split and a bill in one set and one Undo; **a transaction from every importer's golden output** (CAMT, MT940, OFX, CSV, KMyMoney) filed with a bill (the seam and X3); the MCP golden file; prompt-injection text in a file name and in `title` ends up only as sanitised text.
3. **Settings and transaction page** (`kassensturz-ui-bridge`, QML): the two folder rows, the nesting refusals, *Show in folder*, the history line. Tests: **the bridge's maps read back the way QML does** (settings row, check card rows, inbox status); German strings through `tools/update-translations.sh` and `tools/check-translations.sh`; AI off hides everything new.
4. **Undo and Redo** with files: round trip, "Undo anyway", another computer, a deleted transaction. A fix step adds the test for its neighbour path (Q-559).

Docs at build: the new codes in `docs/ERROR_CODES.md`, the keys in the settings documentation, `docs/GLOSSARY.md` ("inbox", "archive"), `docs/THREAT_MODEL.md`, `docs/USER_GUIDE` text for the German user guide. No new direct dependency (§3.1 rule 6) and no book change.

## 11. Decisions

- **D-751 (Decided):** no OCR engine; the user's own AI reads bills and receipts; code checks every figure the AI gives against the bank transaction in exact minor units, and a mismatch is shown and never filed without the user's tick.
- **D-752 (Decided):** the receipt inbox moves a bill from the inbox into an archive folder once its change set is confirmed. Refined here: its own tool and cell (`file_bills`, *File bills from the inbox*) so D-739's "never moves a bill" stays true for the bill folder; both folders through the folder chooser; the path built by code from the book's names; copy, verify, link saved, then remove; never overwrites; a per-computer journal settles a crash by facts and never deletes the last copy; Undo moves the files back, with "Undo anyway" as the deliberate way out.
