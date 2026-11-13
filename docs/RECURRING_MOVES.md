# Monthly moves between my own accounts

**Status:** design for US6 (#784), decision D-749 (Proposed) in `docs/DECISIONS.md`. It amends D-030 ("the user confirms every Transfer match") and builds on D-087, D-088, D-691, D-692 and D-693. Docs only: nothing is built by this pull request. The maintainer decides it on the design pull request.

All names, amounts and banks below are invented.

## 1. The problem and what exists

A household pays three standing orders from "Checking account" to "Savings", "Holiday fund" and "Child's account" on the first banking day of each month. Every statement shows both sides. Today:

- the import only *suggests* the pairs (D-030, D-087), and the user joins each one, every month, again: a join is not remembered;
- a category Rule may sort a side first. The entry then counts as income on the receiving account and is missing from the paying account's spending, so a figure such as "salary minus all spending" is too high (principle 1);
- *Find moves between my accounts…* asks about each pair one at a time (D-692);
- the AI can prepare joins (`join_transfers`, D-693), but that is behind a grid cell that is off by default.

Wish (#784): after the user joined such a pair once, the app joins the later ones by itself, tells the user what it did, and can take it back.

## 2. What the user sees

1. **The offer.** The preview that asks "Join these two entries?" (`TransferPrompt.qml`, D-692) gets one more line with a **tick box, not ticked**: "Next time, join this move by itself when it comes again around the same day of the month." Under it, in plain words: "KassenSturz then joins entries of exactly 100,00 € from *Checking account* to *Savings* without asking, around the 1st. After every import it lists what it joined, and Undo takes the joins back. You can stop it any time in Accounts › Monthly moves." The box is shown when both sides are in different accounts, in one currency, and no remembered move covers them already.
2. **The import.** After an import or a bank fetch, the summary has a new part, **"Joined by themselves"**: one line per pair, "100,00 € from *Checking account* (1 Oct) to *Savings* (1 Oct) · monthly move", with the amounts exactly as the banks sent them. One button, **"Undo these joins"**, takes them all back; the imported entries stay.
3. **Entries that wait.** An entry that fits a remembered move but whose other side has not been imported yet stays *Not sorted yet* instead of being sorted by a Rule, and says so in the register: "Waits for its other side: monthly move *Checking account → Savings*". The user can sort it by hand at any time (manual choices win, D-031).
4. **Join all.** *Find moves between my accounts…* opens with a list of every suggested pair (as D-692's preview shows each one) and two buttons: **"Join all 7"** and **"Go through them one by one"**. One Undo, "Join 7 Transfers", takes all back. After it, moves that look monthly (same two accounts, same amount, same other party, in at least two different months) are offered once as a list with unticked boxes: "These look like monthly moves. Join them by themselves from now on?"
5. **See and remove.** The Accounts page gets **Monthly moves…** next to *Find moves between my accounts…*. It lists each remembered move: "*Checking account → Savings*, 100,00 €, around the 1st, last joined 1 Oct 2026" and a **Remove** button. Removing says: "Moves already joined stay joined. From now on KassenSturz asks about this move again, one by one." Removing is one history entry and can be undone.

Everything else stays as it is. Someone who never ticks the box sees no change except the **Join all** button (progressive disclosure, AGENTS.md §6).

## 3. D-749: how it works

### 3.1 A remembered move

New in the core: `RecurringMove`, one fact per remembered move:

| Field | Meaning |
|---|---|
| `id` | UUIDv7 |
| `from_account`, `to_account` | The user's own accounts the money leaves and arrives at; different accounts, both `asset` or `liability` accounts (D-081) |
| `amount`, `currency` | The exact amount that leaves, in minor units, greater than zero, in the currency of both accounts' entries (a move across currencies is not remembered, Q-020 stays open) |
| `day_of_month` | 1 to 31: the day of the paying entry's booking date when the user ticked the box |
| `from_party_iban`, `from_party_name` | What the bank named as the other party on the paying entry when the user ticked the box, as the bank delivered it (D-067); either may be empty |
| `created_on` | The day it was remembered |

It is not a schedule and creates nothing: it only *recognises* entries that statements bring (scheduled and recurring transactions are #795, US7, and may reuse `expected_date` below).

### 3.2 When an entry fits (the day)

For a month, the **expected date** is `day_of_month`, cut back to the month's last day when the month is shorter (31 in April is the 30th; 29, 30 and 31 in February are the 28th or 29th), and moved forward to Monday when it falls on a Saturday or Sunday, because a bank books a standing order on a banking day (public holidays are not considered, as in D-030). A paying entry fits when its booking date, the one the user sees, is at most **3 business days** before or after an expected date of the previous, the same or the next month (`business_days_apart`, the window D-030 and D-087 already use), and a remembered move joins at most one pair per expected date: the closest to it, then the earlier. The receiving entry is then found as D-087 finds it: at most 3 business days from the paying one, the incoming side may come first.

| Day | Month | Expected date | Why |
|---|---|---|---|
| 1 | October 2026 | Thu 1 Oct | a banking day |
| 1 | November 2026 | Mon 2 Nov | the 1st is a Sunday |
| 31 | April 2027 | Fri 30 Apr | the month has 30 days |
| 31 | February 2027 | Mon 1 Mar | cut back to Sun 28 Feb, moved to Monday |
| 1 | January 2027 | Fri 1 Jan | an entry on Thu 31 Dec fits too (one business day), across the year |

`expected_date(day, year, month)` is a pure function in `kassensturz-core`, tested on these rows.

### 3.3 Which entries pair up (accounts, then the other party, never IDs)

A pair fits a remembered move when:

1. the paying entry is a **side** (D-691: two parts, one in `from_account`, the other open or booked in a category *by a Rule*) with exactly `amount` leaving, and the receiving entry is a side in `to_account` with exactly `amount` arriving, in the same currency. **A side the user sorted by hand never joins by itself** (D-031); it stays for the user to pick (D-692). A side with a note or number that a join would lose (`transfer.part-holds-more`, D-691) is not joined either; it is listed as "needs you".
2. the paying entry's other party is compatible: if the move remembers an IBAN, the entry names the same IBAN (spaces and capitals ignored, as D-672); otherwise, if it remembers a name, the entry names the same name (capitals and spaces ignored, as D-081 compares account names); if the move remembers neither, the entry may name no other party or the receiving account's own IBAN, never another IBAN. So a payment of the same amount to a shop on the same day cannot be taken for the move.
3. the day fits (3.2).

Pairs are made **per remembered move, one by one**. Two entries that fit the same move equally well (same day, same amount, nothing in their other party tells them apart) are **not joined**: both stay suggestions, listed under "needs you". The order, the IDs or the time of import never decide, because they mean nothing to the user and would make the result depend on which file came first. Two remembered moves with the same amount and day from the same account to different accounts (the case in #784) are told apart by the receiving account and the paying entry's other party; two with the same accounts and amount are one move.

### 3.4 The pipeline and D-030

D-030's order becomes: **duplicate check, then the remembered moves, then Transfer matching (suggestions), then Rules.** `kassensturz-pipeline` runs the remembered moves on the entries a run added, and on the other side's entries when a second import brings them, so a pair joins whichever statement comes last. Every front end calls the pipeline (D-092): file import, bank fetch and the command-line tool all join the same way.

D-030 said every Transfer match is confirmed by the user and left room for "trust this pair of accounts" once real use showed how often matches are wrong. **This is that option, narrow and the user's own:** confirming a *remembered move* (the ticked box, after seeing exactly what it means) counts as confirming the later pairs that fit it as 3.2 and 3.3 say, and nothing else. Every other suggestion is asked about, as before. Why this is safe for the money (principle 1): a join keeps both account parts exactly as their banks sent them, amounts, currency, dates and bank IDs, so no balance and no register changes (D-691, `keeping_originals`); only the two *other* parts go, the one that had no category and, for a Rule-sorted side, its category; everything is listed; and one Undo takes it back.

Joins of one run are **one action of their own**, `join_recurring_moves`, after the entries were added, so Undo takes the joins back and leaves the entries. Undo restores both transactions, their categories and the Rule marks (D-691).

### 3.5 Rules skip what fits

A Rule run (import, "Apply now", "Apply all", the AI's `sort_with_rules`) leaves an entry alone when it is a side of a remembered move as 3.3 point 1 and 2 and 3.2 say (the other side may still be missing): the Rule skip reasons gain `fits-monthly-move` next to the existing one for a side of a pending suggestion (`TransferSuggested`, D-691). **No Rule action runs on it**, including *make it a Transfer* (D-031): the user said by remembering the move that the other half comes from that account's own statement, and a Rule that books the other half itself would book it twice. An entry that waits stays *Not sorted yet*, which counts in no category (D-083), so it is neither income nor spending. If the user removes the move, the entry is an ordinary one again and a Rule may sort it.

### 3.6 Join all

`Book::join_transfers(pairs)` (new) joins a list of suggested pairs in **one action**, `join_transfers`, with one Undo. The preview lists every pair as D-692's does and says how many categories go; pairs a join would refuse (`transfer.part-holds-more`) are listed apart with what to change first, the rest join. Only suggestions the store itself returns (open and Rule-sorted sides, D-691) are in the list; a hand-sorted side is never in it. The AI's `join_transfers` (D-693) stays as it is.

### 3.7 Where it is stored: layout 31

A new table `recurring_moves` (one row per move; columns as in 3.1; CHECKs: `amount > 0`, `day_of_month` 1 to 31, accounts differ; foreign keys to `accounts` with `ON DELETE CASCADE`, a unique index on the accounts, amount, currency and `from_party_iban` / `from_party_name`) and its history twin `changed_recurring_moves`; `changes.action` gains `add_recurring_move`, `remove_recurring_move`, `join_recurring_moves`, `join_transfers` (the CHECK is rebuilt, as for tags in D-712). The migration only adds tables and rebuilds a CHECK, so it changes no existing row, like layouts 25 to 30; it runs after the automatic backup, has a migration test from layout 29 and 30 and from a book of layout 24, and `docs/BOOK_FORMAT.md` is updated with it. The stage is Alpha, but layouts 25 to 30 are already migrations since layout 24 (D-536), so this is one too. **[format] item:** the build pull request merges before the stage Go. Books from the first Beta on open as before; a book from before this layout has no remembered moves.

### 3.8 AI, MCP, command line

- **The AI cannot remember or remove a move.** That is consent for the future, so only the user gives it, in the window (AGENTS.md §3.1 rule 9, §8). The AI search keeps marking such entries `possible_transfer` (D-672) and a joined one is a Transfer like any other. No new AI field, so this design has no AI-data-sharing gate; if US6 later adds one, it is a design of its own.
- **Command line:** `transfer monthly list`, `transfer monthly add FROM TO AMOUNT DAY` and `transfer monthly remove ID`, so the behaviour is testable without the app; `import` prints the joined pairs.
- **Bridge (QML):** `monthlyMoves` (a list of maps: `id`, `from`, `to`, `amount` as text (D-065), `day`, `lastJoined`), `rememberMove(a, b)`, `removeMove(id)`, `joinAll()` and its preview, and the import summary's `joined` list; each map read back the way QML reads it (AGENTS.md §9.4).

### 3.9 Not here

Moves across currencies (Q-020); a schedule that creates entries (#795); an amount that varies a little (a move is exact, anything else stays a suggestion); a "turned down" list of pairs (Q-075: still nothing is remembered about a refused pair; the user's tool for it is to leave a pair unjoined or to remove a move); the AI remembering moves.

## 4. Tests

- **Core:** `expected_date` on the rows of 3.2 and the year border; a pair fits across a month end; two equal candidates are not joined; hand-sorted and Rule-sorted sides; a stranger with the same amount is not joined (other party); remembered IBAN, name, neither; one pair per expected date; no join across currencies; property test that a join never changes any account's balance and that Undo restores the book table for table.
- **Store:** layout 31 migration from 24, 29 and 30; history, Undo and redo of `join_recurring_moves`, `join_transfers`, `add_recurring_move` and `remove_recurring_move`; Rules skip a fitting side with the new reason.
- **Where steps meet (AGENTS.md §9.4):** the real CAMT.053 and MT940 importers' output for three accounts with three standing orders and a "salary" Rule that sorts incoming entries as income: import A, then B, then C, and the other way round; the three pairs join, none counts as income or spending, and the balances equal the statements. A bank fetch through the fake bank (D-055) does the same.
- **UI bridge:** read-back of every new map; translations filled in German.
- Synthetic books only.

## 5. Build plan

| PR | Content | Tier |
|---|---|---|
| 1 | Core `RecurringMove`, `expected_date`, matching, layout 31, store and history, pipeline step, Rules skip, command line, `docs/BOOK_FORMAT.md`, `docs/ERROR_CODES.md` | strong, high (moves money between accounts, writes the book) |
| 2 | UI: the offer, the import summary with its Undo, *Monthly moves…*, *Join all*, the entry's "waits" line, translations | standard, high |

PR 1 is the [format] part and merges before the stage Go. Neither is expected to carry a human gate (no dependency, no keychain, no bank code, no AI data); this design pull request does, because it amends D-030.

## 6. Defaults the maintainer may change on the design pull request

1. The tick box is **not ticked** and the user must give consent per move; the alternative "ticked" would lower protection.
2. An entry that waits for its other side stays unsorted with no time limit; the alternative (let a Rule sort it after some days, and join it later by D-691) leaves the interim figures wrong.
3. A remembered move needs no other party to be known, but then refuses entries that name a foreign IBAN.
4. The AI cannot create or remove moves.
