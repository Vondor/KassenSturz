# Tags: free labels on transactions and parts (US4.8 design)

Design for US4.8 (US3 backlog 2; D-712 to D-714). Today the book holds tags per part (`tags`, `split_tags`, layout 15, D-132). The KMyMoney import fills them and the complete export keeps them, but no screen shows or edits them. This design adds tags on the whole transaction, settles how a sum by tag counts, and sets the scope of the first build. The build is a book layout change (**[format]**, layout 30), so it ships as a migration with an automatic backup and a migration test (AGENTS.md §3.1 rule 8; layout 24 on, `docs/BOOK_FORMAT.md`).

A tag is a free label next to the one category, e.g. "Tax" or "Holiday 2026". It never moves money and never changes a sum or a balance. A transaction can carry several tags, and so can each of its parts (the items of a bill). Putting one transaction into two categories stays impossible, because it would count the money twice.

## 1. What the user sees

- **In the transaction form:** a *Tags* field under the note. It shows the chosen tags as small removable chips and a picker. Typing in the picker filters the tag list; a name that doesn't exist yet offers *Create "Holiday 2026"*, so a tag is made on the spot. The whole field works with the keyboard: Tab into it, type, Up and Down to choose, Enter to take, Backspace on an empty field removes the last chip.
- **On the Parts page** (a bill broken into parts): each part has the same field for its own tags. A short line says *Tags on the whole entry also count for every part.*
- **In the form of an entry with part tags:** the whole-entry field stays the one that is edited. A read-only line, *Some parts also have the tags: Tax*, points to the Parts page. A tag is never moved between whole and part without the user's action.
- **In an account's register:** each line shows its tags after the payee (the entry's tags and its parts' tags, each once, in the user's order). The filter row gets a *Tag* chooser next to the text filter; the lines kept are those with the tag on the entry or on any of its parts. The register adds no sum by tag in this build (§5).
- **Managing tags:** *Settings › Tags* lists every tag with how many entries and parts use it, and has *Rename*, *Merge into another tag…* and *Delete*. Delete and Merge say in plain words how many entries and parts they touch, and that Undo takes it back (AGENTS.md §1).
- Words: **Tag** (German *Schlagwort*), see `docs/GLOSSARY.md`. No other term.

## 2. Storage (layout 30, D-712)

A new table beside `split_tags`, built the same way:

```sql
CREATE TABLE transaction_tags (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    transaction_id BLOB NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    tag_id BLOB NOT NULL REFERENCES tags (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (transaction_id, tag_id)
) STRICT;
CREATE INDEX transaction_tags_by_tag ON transaction_tags (tag_id);
```

- **Why a second table, not a copy of the tag onto every part.** A tag on the whole entry is one fact. Copying it onto every part stores it many times, and it drifts: add a part and the copy is missing, remove the tagged part and the entry loses its tag, and the history can't tell "the user tagged the entry" from "the user tagged three parts". One row per fact also keeps the Rust side simple: `Transaction` gets `tags: Vec<TagId>` next to `Split::tags`, written and read like them (`transactions.rs`, `split_tags` read at ~502).
- **The migration only adds** `transaction_tags` and `changed_transaction_tags` and an index; no row of an existing table changes, so the backup before it is the safety net and the test (a layout 29 book with part tags opens at 30 with the same tags and sums) is small.
- **`changed_transaction_tags`**, the change-history twin, with the columns of `changed_split_tags` (`change_id`, `side`, `id`, `transaction_id`, `tag_id`, `position`). The store's table list in `history.rs` gets a `Recorded` entry with key `id`, like `split_tags`. Without it, a change to the new table would not be recorded and Undo would lose it.
- **New actions** (a new `Action` needs the `changes.action` CHECK rebuilt, as the earlier migrations that added one did): `RenameTag`, `DeleteTag`, `MergeTags`. Changing the tags of one transaction is part of the existing change-transaction action; creating a tag inline while saving is one action with the save (`Book::as_one_action`), so one Undo takes back the entry and the new tag. The history entry for a delete or merge records every `split_tags` and `transaction_tags` row it removed or moved, before and after (D-086).

## 3. How a part takes on the entry's tags (D-713)

**Rule: the effective tags of a part are its own tags plus the entry's tags, computed when read, never copied.** The user's order is the entry's tags first, then the part's own, each tag once.

- Nothing is stored for "the part has the entry's tag", so nothing can disagree. Adding a part, removing a part or breaking an entry into Parts needs no tag work: a new part starts with no own tags and still counts for the entry's tags (D-017). A removed part takes only its own tags with it (the `split_tags` rows go by `ON DELETE CASCADE`; the history keeps them for Undo).
- **Changing an entry writes it again** (`update_transaction`, D-707): the entry's tags and each part's own tags are written as the form holds them, and Undo restores the earlier rows exactly.
- **Joining a Transfer (D-087, D-706):** the joined entry gets the tags of both sides, the outgoing side's first, each once (like notes and attachments in D-706). The part tags already merge in `join_transfer` (`transfer.rs`, ~346). Nothing is lost, and the join is one action with one Undo. A Transfer taken apart again by Undo gets each side's tags back from the history.
- **Alternative not taken:** copying the entry's tags onto its parts when saved (simpler reads, but drifts as above), and "a tag on a part marks the whole entry" (then a one-item tag would count the whole bill).

## 4. Sums by tag: which amounts count (D-714)

A sum by tag counts amounts of **category-side parts**, the same parts a sum by category counts, and each part once. In the real model (`kassensturz-core` `Split`, `AccountKind`):

- An entry's parts add up to zero in its currency. Parts booked to an **Asset or Liability** account are the user's own accounts (the *account side*). Parts booked to **Income or Expense**, or to no category yet (`account: None`), are the *category side*. An **Equity** part (starting balance) is never counted.
- **Counted parts** of a tag: every category-side part whose effective tags contain the tag. For an expense of 90.00 split into 60.00 Food and 30.00 Gifts with the entry tagged "Holiday", both parts are effective-tagged and the sum is 90.00, once. If only the 30.00 Gifts part has "Holiday", the sum is 30.00.
- **The account side is never counted.** The 90.00 taken from the checking account carries the entry's tag too (for the register's filter), but a sum by tag skips it. This is why a tag on the whole entry is not counted once per side.
- **A Transfer has two account sides and no category side.** It is found by its tag in a register, and a sum by tag leaves it out, like a sum by category does (money moved between own accounts is neither spent nor earned). Its page, which can't change a Transfer otherwise, has the tag field with its own **Save tags** button (Q-535), the same whole-transaction tags the AI's `tag_transactions` could already set; both sides show them, and they count the Transfer once (one transaction, no part).
- **Signs and kinds:** an Expense part counts as money out and an Income part as money in, as category sums show them. A refund on an expense category lowers it. Two tags on one part count that part once under each tag; there is no total "over all tags", because adding sums of different tags would count it twice. Reports say so in one sentence (US5).
- **Multi-currency:** a category-side part's amount is in the entry's currency (D-043). A sum by tag is made per currency and never mixes them. A part's `foreign` and `original` amounts stay as they are; tags read neither and change neither (AGENTS.md §5). Exact integers, no rounding.
- **Register filter:** a line matches when the entry or any part has the tag. The line's amount is what it did to this account, as always.

## 5. Names and limits

| Rule | Value |
|---|---|
| Name | 1 to 60 characters after trimming, no line break. Unique ignoring capitals and spaces around, as today (`store.duplicate-tag-name`). |
| Tags on one entry | at most 20 |
| Own tags on one part | at most 20 |
| Tags in a book | at most 1,000 |

A refused write says what to do next (AGENTS.md §6), with its own code: `store.tag-name-length`, `store.too-many-tags`, `store.too-many-tags-in-book`. The core returns codes, not text (D-034).

- **Rename:** changes the name only; IDs stay, so nothing tagged changes. The unique rule applies (a clash is refused with the existing tag's name).
- **Delete:** removes the tag from every entry and part that carries it, after a plain count and warning. Nothing else changes, no amount moves, one Undo (history keeps the removed rows). A tagged part stays.
- **Merge A into B:** every use of A becomes B; where an entry or part already has B, the duplicate row is dropped. One action, one Undo. A is then gone.
- **The importer checked against the limits.** Golden files with tags: `kmy/features.kmy` and `kmy/payee-rules.kmy` (one tag, "Holiday") and `kmy/reports-anon.kmy` and `kmy-sqlite/reports-anon.sqlite` (two tags, "Mission" and "Shared hab", 83 tagged parts, at most one tag per part). The longest name is 10 characters and no part has more than one tag, so all fit. The other KMyMoney files (`kassensturz-sample.sqlite`, `space-colony-v12/v15.sqlite`) have no tags; `purpose-tags.sta` and the CSV settings only contain the word "tag" as bank text, not tags. For real files the limits never refuse an import (D-715): the importer cuts a name to 60 characters, keeping it unique ("Name (2)", the number counted in the 60), turns a line break into a space, keeps the first 20 tags of a part and the first 1,000 tags of a book; the tags left out are counted on the import's existing line *tags that no transaction could be given* (`SkippedData::Tags`). The test `tags_beyond_what_a_book_keeps_are_cut_or_counted_never_refused` covers a name of over 90 characters, a second one equal once cut, one with a line break and a part with 25 tags. Joining a Transfer whose two sides together carry more than 20 tags on the whole is refused (`store.tag-too-many`, `transaction`) rather than dropping one.

## 6. First build scope (US4.8)

Done in the build, after US4.3 (both change `TransactionPage.qml`). Nothing is added to a dashboard in this build, so D-452 is untouched:

1. Layout 30 migration, backup and test; `Transaction::tags`; store writes and reads for both tables; tag rename, delete, merge; history table and actions; all with tests where steps meet (an imported book's tags shown and edited; undo of delete and merge; a joined Transfer).
2. Bridge: tags as text lists in the transaction and parts objects, with read-back tests the way QML reads them (AGENTS.md §9.4). Amounts stay text (D-065); counts shown in *Settings › Tags* are formatted by Rust.
3. QML: a reusable `TagField.qml` (chips and picker with inline create) used by `TransactionPage.qml` and `PartsPage.qml`; the tag chips and *Tag* filter in `RegisterPage.qml`; a tags page in Settings. German strings via `tools/update-translations.sh`.
4. Docs: `docs/BOOK_FORMAT.md` (layout 30), `docs/EXPORT_FORMAT.md`, `docs/GLOSSARY.md`, `docs/ARCHITECTURE.md`, handbook page.

**Built in three pull requests:** part 1 (#740) the layout, store, import and export; part 2 the bridge, `TagField.qml` in the transaction form (a tag typed new is made with the save, in one Undo), tags on the register's lines, search by tag name and the *Tag* chooser, which keeps a line when the entry or any of its parts has the tag; part 3 the Parts page's own tags and *Settings › Tags* (rename, merge, remove after the count, one Undo).

**Not in this build (US5):** tag filter in reports, tiles and the filter bar; sums and charts by tag; a tag column in tables; AI tools by tag (`list_tags`, tags in `search_transactions` and in change sets, which are human-gate work); a transactions tile filter by tag (US4.7). Merging and tagging by Rules is not planned.

**What US5 needs from this layout (so no second layout change):** (a) both tag tables carry an index by tag, so "all parts with tag X" is a quick read; (b) one store read that returns, per category-side part, its amount, currency, date, category and effective tag IDs (own plus the entry's), which the report crate turns into a series by tag; (c) the counting rule of §4, written once in `kassensturz-report` and reused by tiles, charts and the AI; (d) no tag colour: chart colours come from the palette (D-589); a colour would be an additive migration if ever wanted.

## 7. Import and export

- **KMyMoney import** keeps filling part tags as today (`kmy/mapping.rs`, `imported_book.rs`); it leaves entry tags empty, since KMyMoney tags belong to splits. Names and counts follow §5.
- **Complete export version 13** (`docs/EXPORT_FORMAT.md`): a transaction gets `tags`, the IDs of the tags it carries, in order, each once, may be left out when empty. Part `tags` and the `tags` list stay as in version 4. An older file reads as version 13 with no entry tags; a version 12 or older file with a transaction `tags` key is refused (`export.unknown-value`), as for earlier versions. Reading a file checks the limits of §5 (`export.bad-value`), so an export of a book that fits always reads back.
- The book file itself is documented in `docs/BOOK_FORMAT.md` (layout 30, `transaction_tags`).

## 8. The maintainer's answers

- **Q-493, where a tag chosen in the simple form goes:** on the whole entry (R). A tag on a part is an addition to the entry's tags, so a search or filter by either the entry's tag or a part's own tag finds the transaction (§1, register filter).
- **Q-494, Transfers in sums by tag:** left out (R), as §4 says.
- **Q-495, removing a tag that is in use:** removed everywhere after a plain count and warning, with one Undo (R), as §5 says.

## 9. Sums by tag in tiles and Statistics (US5.2, step a)

The counting rule of §4 is written once, in `kassensturz-report`: `Filter::tags` is applied in `Source::parts`, which every dataset of income and spending, every formula and every Statistics column already reads (D-714).

- **Where the user chooses it:** a chart, a table, a key figure card and a progress bar have *Only count these tags* in their Properties (`params.tags`, `Kind::Tags`, at most 200 IDs). Nothing ticked counts all money, as before. Ticked, the component counts only the parts that carry one of the tags, on the part or on the whole entry, so it shows the sum by tag. A part with two of the ticked tags counts once.
- **Where it doesn't apply:** the balances and the budgets are not made of parts, so they take no tags (`Takes::tags` is false). A key figure of what the accounts hold with tags ticked says *Not filtered by tag* instead of a figure that only looks filtered. The candles, the custom chart, the transactions tile, the budget bars, *What you are worth* and *Where the money came from* have no tag list.
- **The marks:** the grey line says *Own filter: tags* and *Filtered to tags: Holiday*. A tag removed since is noted as gone; when every ticked tag is gone the component draws nothing and says so.
- **The filter bar has no tags** in this step: tags are a component's own choice only, so a page never narrows by a tag without showing it on the tile.
- **A dashboard file** writes the list empty (`docs/DASHBOARD_FILE.md`), as for the budget bars.
- **The AI** neither sees nor sets `params.tags`; a chart it changes keeps the user's tags. Its own tag tools are §10.

## 10. Tags and the AI (US5.2, step b, D-744)

- **Reading,** under Transactions › Read: `list_tags` gives every tag with its ID and how many transactions carry it; `search_transactions` takes a `tag_id` (the transaction carries it on the whole or on a part) and every row names its tags (`tags`, `tag_ids`); `category_totals` takes a `tag_id` and then counts by the rule of §4, the same check as the tiles (`kassensturz_report::carries_tag`), so the AI's sum by tag and the dashboard's agree. A tag name is the user's own word and reaches the model as data, never as an instruction.
- **Changing,** behind its own cell *Tags on transactions › Change* (`AllowTagTransactions`), off by default and keeping Transactions › Read on: `propose_changes` takes `tag_transactions`, each item a `transaction_id` with `add_tag_ids` and `remove_tag_ids`. Only tags of the book, by ID: the AI never makes a new tag. Only the tags on the whole transaction change (as the simple form sets them); a part's own tags stay, so a tag the user put on one item of a bill is never moved or removed. Refused: nothing to change, a tag added that the whole entry already has or removed that it doesn't, more than 20 tags after it, the same transaction twice in a set, more than 100 transactions.
- **Review and confirm:** each transaction is a card with its day, payee, account and amount and the tag names put on and taken off. On confirm each transaction must be exactly as reviewed, otherwise nothing of the set is applied (`ai-changes.out-of-date`); the set is one Undo.
