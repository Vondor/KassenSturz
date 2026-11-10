# Budget history: changing what a budget counts (US3.2 design)

Design for US3.2 (#607, US1.10a; D-689). Today a budget's categories and accounts are fixed once it is made (D-639); the only way to change them is to stop the budget and make a new one. This design lets the user change them **from the current period on**, while every earlier period keeps showing what it showed before (D-345, principle 1, the mission's "conservative with the data"). The build step follows after this design is merged; it is a book layout change (**[format]**), so it ships with an automatic backup and a migration test (AGENTS.md §3.1 rule 8).

## 1. What changes for the user

- On the **Change the budget** sheet, the categories (*Category:*, *Also count:*) and the accounts (*Only count money paid from some accounts*) can be changed, like the amount and the carry over today.
- **One "from" for everything.** The sheet's existing *From this period on:* chooser says from which period the whole change applies: amount, carry over, categories and accounts together, as one change of the history and one Undo.
- **Not before the current period.** When the categories or accounts change, the chooser offers the current period and later ones only. Earlier periods are not offered, and the sheet says so in one sentence: *Earlier periods keep counting what they counted, so their results don't change.* (For a change of only the amount or the carry over, the chooser stays as it is today, D-345, including earlier periods to correct a mistake.) The safest reading of the approved item (Q-474) is "from the current period on"; a user who wants different past results can still delete the budget and make a new one, after the plain warning Delete already gives.
- A budget that has **not started yet** changes as a whole: there is no past to keep.
- A budget that **has ended** can't change what it counts (there is no current or later period). It is refused with `store.budget-period` `amount-after-end`, the reason an amount change gets today.
- A budget **for a stretch of days** has one period. When that period is the current one, a change applies to all of it (its result is still being counted, nothing in the past changes); once it has ended, it is refused as above.
- **The budget page shows the history.** Under the budget's categories, one short line per earlier set: *Until 31 Oct 2026 it counted Food, Rent.* (and *only money paid from Checking*, when accounts were chosen). So a user never wonders why an old period shows other figures, or why a category that is in an earlier set can't be deleted (see §4).
- **Carry over continues.** What is left at the end of the period before a change moves into the first period with the new set, as before: it is the same budget. A user who wants a fresh start stops the budget and makes a new one.
- **The salary day stays fixed** (D-651), as does *How often*: those change the periods themselves, so the past periods would no longer be the same periods. Stopping and making a new budget stays the way for them.

## 2. Model (`kassensturz-core`)

A budget gets a dated list of **what it counts**, oldest first, alongside its dated amounts:

```rust
pub struct BudgetCounts {
    /// The first day of the period this set applies from.
    pub from_day: Date,
    /// The spending categories, in the order the user chose them (D-639).
    pub categories: Vec<AccountId>,
    /// The user's own accounts it counts from, empty for every account.
    pub accounts: Vec<AccountId>,
}
```

`Budget::categories` and `Budget::accounts` are replaced by `counts: Vec<BudgetCounts>`. Rules, checked in the store when it writes:

- The first entry starts on the budget's `first_day`; each `from_day` is the first day of one of the budget's periods and later than the one before. A set applies from its `from_day` until the day before the next set's `from_day` (or the budget's last day).
- `Budget::counts_for(period_start)` gives the set of the period that starts on that day: the last whose `from_day` is that day or earlier. This is the same lookup `amount_for` does for amounts.
- Every set follows the D-639 rules: 1 to 100 spending categories, each once, all **in the budget's one currency** (a set in another currency is refused with `store.budget-choice` `currencies`, since the amounts are in it); 0 to 100 own accounts, each once.
- **Overlaps (D-344, D-639):** `Budget::clashes_with` compares each set's days with each set's days of the other budget, and reports the first category that two overlapping sets count from the same accounts. The overlap rule doesn't change; it just applies per set.
- A budget keeps at most 1,200 sets, the limit of its amount rows (T-47).

## 3. Counting (`kassensturz-report`)

- `budget_utilization` counts each period with **the set of that period** (`counts_for(period.first_day)`): its categories with everything inside them, each category once (D-291), from its accounts if it has any. Past periods count with their old set, so their spent, left and carried figures are exactly what they were (D-345).
- A row's `categories`, `paths` and `accounts` are those of the set of the period asked for, which is the one the bars and the list show.
- A period whose set differs from the period before gets the note `counts-changed` (a fixed word, as `over` or `carried-in`), so a screen can say so in words.
- The filter `filter.categories` keeps a budget when the set of the period asked for has one of the categories.
- **Tests (property and unit):** a change of the set from a period on never changes an earlier period's spent, left or carried amount; the carry chain across a change equals a step-by-step sum; a set change and an amount change from the same period count together.

## 4. Book layout 29 (`kassensturz-store`)

`budget_categories` and `budget_accounts` get a **`from_day`** column (TEXT, ISO 8601, NOT NULL): the set a row belongs to. A budget's sets are the distinct `from_day`s of its `budget_categories` rows (every set has at least one category); a set's accounts are the `budget_accounts` rows with the same `from_day` (none: every account). The unique indexes become (`budget_id`, `from_day`, `category_id`) and (`budget_id`, `from_day`, `account_id`). The `changed_budget_categories` and `changed_budget_accounts` history tables get the same column, so the change history records it and Undo brings it back (D-066).

- **Migration (after the automatic backup):** the tables are rebuilt as layout 25 did, and every row gets its budget's `first_day`, so every budget has exactly one set and counts what it counted before. The history copies get the `first_day` of their budget (from `budgets`, or from the change's own copy in `changed_budgets` for a budget that has been removed since), so a change made before the update can still be undone and redone.
- **Store API:** one new operation, recorded as the existing action `edit_budget`:

  `Book::edit_budget(id, from_day, change, today)` applies from the period that starts on `from_day` on, in one change of the history: a new amount and carry over (as `edit_budget_amount` does today) and/or a new set of categories and accounts. A set change is refused with `store.budget-period` and a new reason **`before-current-period`** when `from_day` is earlier than the period that contains `today` (or than the budget's first day, if it starts later); the store gets `today` for this, as the AI tools do (D-394). As with amounts, "from this period on" means what it says: the sets planned for later periods are replaced. Nothing that stays the same is recorded. `edit_budget_amount` stays for amount-only changes.
- **Category removal:** a category in any set of a budget, earlier ones included, still can't be removed (`store.category-in-use` `budgets`), since the earlier periods count it. The budget page's history line (§1) tells the user where it is.
- `docs/BOOK_FORMAT.md` gets the new columns, and `layout.rs`' test opens a layout-28 book with budgets (one with accounts, one with history entries), upgrades it, checks every budget's single set, and undoes and redoes a change made before the update.

**Format and stage.** The book is still Alpha (D-274), but layouts 24 to 28 are real migrations with an automatic backup and a test, and this one is too. The layout number is the next free one when the build merges; if another step merges a layout first, the build takes the next number (the renumber rule of US1.10).

## 5. Complete export, version 12

A budget's `categories` and `accounts` become **`counts`**, a list oldest first of `{"from_day", "categories", "accounts"}`, the way `amounts` is a dated list; the first `from_day` is the budget's `first_day`. A file of version 11 or older reads each budget as one set from its first day; a version 11 or older file with `counts`, or a version 12 file with `categories` or `accounts` on a budget, is refused (`export.unknown-value`). `docs/EXPORT_FORMAT.md` gets the rows.

## 6. The AI (`kassensturz-ai-tools`)

- `edit_budgets` takes `category_ids` and `account_ids` (the whole new list, as in `new_budgets`, D-641). The change applies from the period that contains today on, never earlier (D-394, D-345), through the same store operation, so the same refusals reach the model (`store.budget-choice`, `store.budget-overlap`, `ai-tools.unknown-category`, `ai-tools.unknown-account`).
- The review card shows the categories (and accounts) before and after, with the sentence of §1 that earlier periods keep counting what they counted.
- `list_budgets` gives the set of the current period, as today; it does not list earlier sets (the model doesn't need them to prepare a change, and the reply stays short).
- Budgets › Change already covers it in the permission grid: a change of what a budget counts is a change of the budget (D-252). No new cell.

## 7. What it doesn't do

- No change of *How often*, the salary day or the first day (§1).
- No change of an earlier period's set: the past is never recounted by a set change.
- No automatic set change when a category is moved or renamed: as today, moving a category into another one changes what that outer category's budget counts, for every period, because every report reads the category tree as it is now (D-345, §1.2 of `REPORT_DESIGNER_M6C.md`). That rule is unchanged.

## 8. Steps

1. This design (docs only; D-689).
2. **The build** (one PR, strong model, high effort): core model and `clashes_with`, layout 29 with the migration and its test, the store operation, `budget_utilization` and its property tests, export version 12, the Change sheet and the history line on the budget page (with the German entries), `edit_budgets`' two lists and the review card, and the docs (`BOOK_FORMAT.md`, `EXPORT_FORMAT.md`, `ERROR_CODES.md` for `before-current-period`, `GLOSSARY.md`, `THREAT_MODEL.md` T-50). Its decision row records what was built (D-690). If the PR grows too large to review, the AI part (§6) splits off into a small PR of its own after it.
