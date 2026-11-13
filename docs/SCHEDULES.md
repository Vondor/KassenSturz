# Planned payments and the forecast

**Status:** design for US7 (#795 and #725, one design because the forecast reads the schedules), decisions D-769 (planned payments) and D-770 (forecast) in `docs/DECISIONS.md`, both **Proposed**: the defaults in [9](#9-defaults-the-maintainer-may-change) stand unless the maintainer changes them. Nothing is built yet. This pull request says `Refs #795` and `Refs #725`; the build pull requests close them (Q-538). It applies D-029 (it moves "scheduled and recurring transactions" from "after 1.0" into the 1.0 set, as the US7 prompt says), D-030, D-031, D-053, D-065, D-083, D-087, D-749 and AGENTS.md §3.1 rules 1, 8 and 9.

All names, amounts and banks below are invented.

## 1. The problem and what exists

A household pays rent on the 1st, an insurance once a year and a gym fee on the 25th. Today KassenSturz only knows what already happened. The person cannot see, on the start screen, how much of the balance is really theirs this month, and has to remember what is still to come. Budgets (D-345) come closest, but they are a limit per category the person types in; they know nothing about a payment that is due on a given day.

What exists and what this design reuses:

- `docs/RECURRING_MOVES.md` (D-749) recognises a *repeating move between the user's own accounts* once its first pair was joined. It creates nothing and is not a plan. Its `expected_date` function (day of month, cut back to the month's length, moved forward to Monday) and the 3-banking-day window (D-030) are reused here.
- The import pipeline (D-092) puts statement entries in the book. The KMyMoney importer skips schedules and counts what it skipped (D-029); that stays (see [8](#8-not-here)).
- Balances, categories, Undo and the change history (D-086) are the book's; a schedule is a *plan beside them*.

## 2. What the user sees

Plain words only: the UI says **planned payment**, never "schedule", "recurring transaction" or "accrual" (`docs/GLOSSARY.md` gets the rows in 7).

1. **Make a payment repeat.** In the register, an entry has **Plan this again…**. It opens the form below with everything filled from the entry (account, amount, category, other party, how often: monthly on its day). Saving makes the entry the first *paid* occurrence. This is the main path for someone who has never heard of a schedule.
2. **Accounts › Planned payments…** (next to *Monthly moves…*) lists each plan: "*Rent*, 850,00 € out of *Checking account*, every month on the 1st, next: Mon 2 Nov". Buttons: **Edit**, **Pause**, **Remove**, **New planned payment**. Removing says: "Entries already matched stay as they are. KassenSturz just stops expecting this payment." It is one history entry and Undo brings it back.
3. **The form.** *What is it for* (name, at most 60 characters) · *Account* · *Money goes out / comes in* and *Amount* · *Category* (optional) · *How often*: every 1, 2, 3, 6 or 12 months, every year, or every N weeks · *First due on* · *Ends* (never, or on a date) · *The amount may vary by up to* (optional, for a bill that is not always the same; default none) · *Paid to / received from* (optional, name or IBAN as the bank shows it, filled from the entry). A line under the form shows the next three dates in words: "Next: Mon 2 Nov, Wed 1 Dec, Fri 1 Jan", so nobody has to understand "the 1st that is a Sunday".
4. **What it does with the bank.** After an import or a bank fetch, an entry that fits a due planned payment is *matched* to it. The summary has a new part, **"Planned payments found"**: "*Rent* 850,00 € on 2 Nov: matched". One button, **"Undo these matches"**, takes them back; the entries stay. If the entry has no category yet, the plan's category is put on it (a Rule or the user's own choice always wins, D-031) and the line says so.
5. **When nothing came.** A payment whose day plus 3 banking days has passed with no fitting entry shows as **"Did this happen?"** on the Planned payments page and on the start screen: **Match an entry…** (pick one from the account), **It was paid** is *not* offered (nothing is claimed without an entry), **Book it** (creates the entry from the plan, for an account the person keeps by hand: the amount can be changed first, and it is one history entry) and **Skip this time**. Nothing is booked, matched or skipped by itself.
6. **Start screen.** A tile **Left this month** (the forecast, [5](#5-the-forecast-725-d-770)) and a tile **Coming up** (the next planned payments in 30 days). With no plan yet, the tile says "Nothing planned yet" with one button, *Plan a payment*; it shows no number (an unset tile is `unknown`, D-734). New books' default Overview gets both (D-342); a book's existing dashboard document is not changed, the tiles are in the tile picker.
7. **Pause.** A paused plan is not expected, matched or counted. Closing an account does the same to its plans; reopening it brings them back.

KassenSturz **never sends a payment** and a plan never starts one (D-053, AGENTS.md §3.3).

## 3. D-769: how it works

### 3.1 A planned payment

New in `kassensturz-core`: `Schedule` (the code word is `schedule`; the UI word is *planned payment*).

| Field | Meaning |
|---|---|
| `id` | UUIDv7 |
| `name` | 1 to 60 characters, trimmed, one line |
| `account` | one of the user's accounts, `asset` or `liability` (D-081), open |
| `amount`, `currency` | signed minor units (negative = money out), never zero; the currency is the account's (D-027) |
| `tolerance` | minor units, 0 or more, less than `abs(amount)`; default 0. A matching entry may differ by at most this much, with the same sign |
| `category` | optional, an existing category |
| `party_iban`, `party_name` | optional, what the bank names as the other party; text as delivered (D-067), kept up to 140 characters (a longer text is cut on a character boundary, never refused, so "Plan this again…" works on every importer's entries; both sides are cut the same way when compared) |
| `unit`, `every` | `week`, `month` or `year`; `every` 1 to 52 (a quarter is every 3 months, half a year every 6) |
| `anchor` | the first due date as the pattern is counted from, a calendar date (`jiff::civil::Date`, D-061) |
| `track_from` | the first date an occurrence counts; occurrences before it never exist, so a plan made today is not "overdue" for last year |
| `ends_on` | optional last date |
| `paused` | yes or no |

Not stored: the occurrences. They are **computed**, so a plan never goes stale and costs no rows.

### 3.2 When a payment is due

`due_dates(schedule, from, to)` is a pure function in `kassensturz-core`, tested on the rows below. The *k*-th date is counted **from the anchor, never from the previous date** (so 31 Jan, 28 Feb, 31 Mar, not a drift to the 28th): `week`: anchor + 7·every·k days; `month`: the anchor's day in the month anchor + every·k, cut back to that month's last day; `year`: the same with 12·every months. Each date is then moved **forward to Monday** when it falls on a weekend (banks book on banking days; public holidays are not considered, as in D-030 and D-749). It stops at `ends_on`, never before `track_from`, never more than 5 years after `from` (a hard limit against runaway loops) and yields at most 1000 dates per call.

| Plan | Date | Why |
|---|---|---|
| monthly, anchor 1 Oct 2026 | Mon 2 Nov | the 1st is a Sunday |
| monthly, anchor 31 Jan 2027 | Sun 28 Feb, so Mon 1 Mar; then Wed 31 Mar | cut back, then moved; no drift |
| yearly, anchor 29 Feb 2028 | Mon 28 Feb 2029; Tue 29 Feb 2032 | cut back in a common year |
| every 2 weeks, anchor Fri 6 Nov 2026 | 6 Nov, 20 Nov, 4 Dec | 14 days from the anchor |
| monthly, anchor 28 Nov 2026 (a Saturday) | Mon 30 Nov | moved forward, may cross a month end |

An occurrence belongs to the month of its date **after** the move.

### 3.3 Occurrence state (stored only when it was decided)

Table `schedule_occurrences` holds one row per occurrence that was **matched** or **skipped**: `schedule_id`, `due_on` (the unmoved counted date, so editing the weekend rule later never orphans a row), `status` (`matched` or `skipped`), `transaction_id` (set exactly when matched). An occurrence with no row is **open**. Its *state* for a given `today` is decided in Rust, in this order of words (fixed English words, translated in QML, D-034):

| State | Means |
|---|---|
| `matched`, `skipped` | a row exists |
| `upcoming` | open, the (moved) date is after today |
| `due` | open, the date is today or at most 3 banking days ago (the bank may still deliver it) |
| `overdue` | open, more than 3 banking days ago |

Removing the matched transaction removes the row (`ON DELETE CASCADE`), so the occurrence is open again; the history records that (it is one action with the removal and Undo restores both).

### 3.4 Which entry fits (matching)

An open occurrence fits an entry when, **all** of:

1. the entry is a transaction with a part in the plan's account, in the plan's currency, whose amount has the plan's sign and differs from `amount` by at most `tolerance` (exact when 0, AGENTS.md §3.1 rule 1);
2. the entry's booking date is at most 3 banking days from the occurrence's moved date, before or after (`business_days_apart`, D-030);
3. the other party is compatible, exactly as `docs/RECURRING_MOVES.md` 3.3 point 2 says (a remembered IBAN must equal, spaces and capitals ignored; else a remembered name must equal; with neither, any party fits, a *Transfer* side never does);
4. the entry is not a side of a Transfer or of a pending suggestion (D-087, D-691), and is not already matched to another occurrence (a unique index on `transaction_id`).

Two entries that fit the same occurrence equally well, or two occurrences that fit the same entry (a weekly plan and a monthly one of the same amount), are **not matched**; both stay open and are listed "needs you". Never the order, IDs or time of import decide (as in D-749). A match **never changes an amount, a date or a Part**. It adds the plan's category only to an entry whose part has no category after the Rules ran; the user's hand-set categories and Rules win (D-031).

### 3.5 Where it runs: the pipeline

D-030's order becomes: **duplicate check, remembered moves (D-749), Transfer matching, Rules, then planned payments.** A new `match_schedules(book, &mut report)` in `kassensturz-pipeline` runs after the Rules, in **one action of its own**, `match_schedules`, so Undo takes the matches back and leaves the entries. It looks at every open occurrence with a date up to 3 banking days from today (late files for old months match too) against the account's unmatched entries, so it is idempotent: running it twice matches nothing new. Rules go first on purpose: the Rule order is what people already trust, and a plan then only fills what is still open. The file import, the bank fetch and the command line all call it (D-092). The bridge calls it after its own `as_one_action(ImportStatement)`, like `join_recurring_moves` (US6 build PR 1).

Manual paths, each one action: `match_schedule_occurrence(schedule, due_on, transaction)` (the user picks), `skip_schedule_occurrence`, `unskip_schedule_occurrence` and `book_schedule_occurrence(schedule, due_on, date, amount)` (the one place a plan creates an entry, only on the user's click; it creates a transaction like any hand-made one: the account's part with the amount, one part without category or with the plan's category).

### 3.6 Where it is stored: layout 33 **[format]**

Layout 33 adds two tables and rebuilds `changes`:

- `schedules` (columns as 3.1; CHECKs: `amount <> 0`, `every` 1 to 52, `unit` in the three words, `tolerance >= 0 AND tolerance < abs(amount)`, `ends_on IS NULL OR ends_on >= anchor`, `track_from >= anchor`; foreign key `account_id` to `accounts` (`ON DELETE CASCADE`), `category_id` to `categories` (`ON DELETE RESTRICT`; removing a category that a plan uses is refused with `category.used-by-schedule`, as for budgets));
- `schedule_occurrences` (primary key `schedule_id, due_on`; foreign keys `ON DELETE CASCADE` to the schedule and to the transaction; CHECK `(status = 'matched') = (transaction_id IS NOT NULL)`; a unique index on `transaction_id`);
- their history twins `changed_schedules`, `changed_schedule_occurrences`, both in `RECORDED`; `changes.action` gains `add_schedule`, `edit_schedule`, `remove_schedule`, `pause_schedule` (also resume), `match_schedules`, `match_schedule_occurrence`, `skip_schedule_occurrence`, `unskip_schedule_occurrence`, `book_schedule_occurrence` in the CHECK (rebuilt, as for tags D-712 and moves D-749), `Action::ALL`, `action_to_text`, the CLI's `action_text`.

The migration only adds tables and rebuilds a CHECK, so it changes no existing row, like layouts 25 to 32; it runs after the automatic backup and has a migration test from layouts 24, 31 and 32. `docs/BOOK_FORMAT.md` (the table, `RECORDED` list, migration paragraph) and `docs/EXPORT_FORMAT.md` are updated with it: **the complete export (open format) and its import carry the plans and their decided occurrences**, with a round-trip test, because the book's data may never be locked in. A book from before layout 33 has no plans. **This is a [format] item: build PR 1 merges before the stage Go.** It adds no limit on existing data, but because it copies bank text into `party_name` and `party_iban`, the limit test (X3) runs "Plan this again…" on every importer's golden entries (CAMT.053, MT940, OFX, CSV, KMyMoney).

Limits: at most 200 plans per book (`schedule.too-many`), names 60 characters. Error codes (never text, D-034) are added to `docs/ERROR_CODES.md`: `schedule.name`, `schedule.amount-zero`, `schedule.every-range`, `schedule.end-before-start`, `schedule.tolerance-range`, `schedule.account-closed`, `schedule.currency-differs`, `schedule.too-many`, `schedule.occurrence-unknown`, `schedule.entry-taken`, `schedule.entry-not-fitting`, `category.used-by-schedule`.

### 3.7 AI, MCP, command line, Rules

- **No AI or MCP change in this design.** Creating a plan is consent for the future, so only the user gives it, in the window (AGENTS.md §3.1 rule 9, §8), as D-749 decided for moves. Reading plans or the forecast in the chat is a *new read function*, which is AI data sharing and a human gate; it is its own item (see [8](#8-not-here)). So no AI gate is carried here.
- **Command line:** `schedule list`, `schedule add ACCOUNT AMOUNT NAME --every N --unit month --from DATE [--category C] [--until DATE] [--tolerance X]`, `schedule edit|pause|resume|remove ID`, `schedule match ID DUE TRANSACTION`, `schedule skip ID DUE`, `schedule book ID DUE [--date D --amount X]`, `forecast [--months N]`; `import` prints the matches. The behaviour is testable without the app.
- **Bridge (QML):** `plannedPayments` (a list of maps: `id`, `name`, `account`, `amount` as text (D-065), `every`, `unit`, `nextDue`, `state`, `paused`), `plannedOccurrences(from, to)`, and the calls above; each map read back the way QML reads it (AGENTS.md §9.4).

## 4. Tests

- **Core:** `due_dates` on the rows of 3.2, the year and leap borders, `ends_on`, `track_from`, the 5-year and 1000-date limits; property test that dates are strictly increasing, never on a weekend and never before `track_from`; matching with tolerance, sign, window, IBAN, name, neither; two equal candidates are not matched; a Transfer side never matches; a hand-set category is never replaced; a Rule-set one neither.
- **Store:** layout 33 migration from 24, 31 and 32 (the existing layout loop covers each later rebuild); every table in `RECORDED` and its `changed_` twin (`the_history_records_every_table_with_every_column`); Undo and Redo of each new action, including removing a matched transaction and Undoing it; `category.used-by-schedule`; a schedule on a closed account is neither matched nor counted; complete export and import round trip; writes outside an action refused (use `as_another_program` to tamper).
- **Where steps meet (AGENTS.md §9.4):** the real CAMT.053 and MT940 importers' output for an account with rent, a gym fee and a salary plan and a "salary" Rule: import month 1, month 2, a late file for month 1 after month 2, and the plans match once each; balances equal the statements; Undo of the import and of the matches each leave the other. A bank fetch through the fake bank (D-055) does the same. The Rule's category stays when a plan also has one.
- **Forecast:** the worked example in 5.4 as a golden test, and the property that `left` plus the outgoing sum equals the balance for any generated set.
- **UI bridge:** read-back of every new map; German translations filled in (`tools/update-translations.sh`, `check-translations.sh`).
- Synthetic books only.

## 5. The forecast (#725, D-770)

### 5.1 What it says

"How much is really left this month, once the planned payments still to come are taken off." It is **only** the balance and the plans: it never guesses from past spending and never gives advice (AGENTS.md §3.1 rule 11). Under the number the tile says: "Based on your planned payments only. Spending that is not planned is not included."

### 5.2 The figures, all from code (D-052, D-065)

`forecast_month(book, accounts, today)` in `kassensturz-core` returns exact `Money` values (checked add, no rounding):

- `balance_now`: the booked balance of the chosen accounts at the end of today, by booking date (pending entries and future-dated ones are not in it);
- `out_due`: the sum of the **open** outgoing occurrences whose moved date is from today to the end of the calendar month, plus `due` ones from the last 3 banking days (the bank may still deliver them);
- `in_due`: the same for incoming ones;
- **`left = balance_now − out_due`**. Planned income that has **not arrived** is not added to `left`: the figure may be too low, never too high (principle 1, "a wrong balance is the worst bug");
- `expected_end = left + in_due`, shown as a second line only when `in_due` is not zero: "At month end, with the 2.200,00 € you expect: 4.379,11 €";
- `overdue`: the open occurrences that are `overdue`, **not counted** in any figure and listed as "Did this happen?" (2.5); the tile shows their number so the figure is never silently too high or too low;
- `months(n)`: the same projection for the next *n* months (up to 12): per month the start balance, planned in, planned out and end balance, each end the next start.

Sums are over **one currency** at a time: the tile uses the book's main currency and says "N accounts in other currencies are not included" (D-043: no conversion without a rate). A plan on a paused plan or a closed account counts for nothing. The default accounts are the open checking and cash accounts; the tile's settings choose others. A credit card is not in the default, because its bill is paid from the checking account and would be counted twice.

### 5.3 Where it shows

- The **Left this month** tile: one data widget `forecast.month` (kpi, with the state word from Rust: `to-do` when something is overdue, `unknown` with no plan, else `fine`; status icon at 1 × 1, STATUS_TILES D-734).
- The Planned payments page has a **month by month** table (`months`) and a catalog dataset `forecast.months` so the user can add a chart; chart coordinates are the only floats and are computed from exact values, never read back (AGENTS.md §5).
- The CLI: `forecast`.

### 5.4 Worked example (becomes the golden test)

Today is Fri 13 Nov 2026; Checking account balance 2.340,00 €. Plans: *Rent* −850,00 € monthly on the 1st (paid, matched Mon 2 Nov), *Insurance* −96,00 € yearly on 20 Nov, *Gym* −29,90 € monthly on the 25th, *Phone* −34,99 € monthly on the 28th (Sat, so Mon 30 Nov), *Salary* +2.200,00 € monthly on the 27th.

- `out_due` = 96,00 + 29,90 + 34,99 = **160,89 €**; `in_due` = 2.200,00 €.
- `left` = 2.340,00 − 160,89 = **2.179,11 €**; `expected_end` = 2.179,11 + 2.200,00 = **4.379,11 €**.
- December: start 4.379,11; out: rent 850,00 (Tue 1 Dec), gym 29,90 (Fri 25 Dec), phone 34,99 (Mon 28 Dec) = 914,89; in: salary on Sun 27 Dec, so Mon 28 Dec, 2.200,00; end **5.664,22 €**.

## 6. Relation to other items

- **#784 monthly moves (D-749)** are not plans and are **not** in the forecast: money moving between two chosen accounts cancels out when both are in the set, and with a single account the tile says "Moves to your other accounts are not counted". The alternative (count remembered moves from their expected date) needs the "already joined this month" test and is a follow-up (9, item 5).
- **Budgets (D-345)** stay separate: a budget's "left" and the forecast are never added or merged, so one payment is not counted twice. A later item may let a plan count into a budget.
- **Rules (D-031)** are unchanged; a plan only fills an empty category.

## 7. Glossary and docs the build updates

`docs/GLOSSARY.md` rows *(proposed)*: **Planned payment** (a payment you expect on a day, repeating: rent, insurance. It is only a plan: it moves no money and sends nothing; schedule), **Coming up**, **Left this month** (balance now minus the planned payments still due), **Did this happen?** (a planned payment whose day passed without a matching entry). Also `docs/BOOK_FORMAT.md`, `docs/EXPORT_FORMAT.md`, `docs/ERROR_CODES.md`, `docs/ARCHITECTURE.md` (pipeline order), the user guide.

## 8. Not here

- A plan that **sends** a payment or a direct debit (D-053, never).
- A plan that **books by itself** on the due day: only the user's click books (2.5).
- Plans between the user's own accounts, plans in another currency than the account's, loans with interest and an amortisation plan (version 3 of #725, after Gold), amounts that follow a formula.
- The **KMyMoney importer reading schedules** (the importer keeps skipping and counting them, D-029): a follow-up for the Issue Tracker, slot Beta or after Gold; it needs golden files and a clean-room reading (D-044).
- **AI functions** for plans and the forecast (read-only, so the chat can answer "what is left this month"): a follow-up with its own AI-data human gate, after the chat is known to work with the new data.
- Public holidays (as D-030), salary-day months for the forecast (it uses calendar months; the budgets' `salary_day`, D-651, is a follow-up).

## 9. Defaults the maintainer may change

1. A plan is **never matched, booked or skipped by itself** except the match to an entry the bank delivered (and that is undoable in one click). The alternative "book it on the due day" would put numbers in the book that nobody confirmed.
2. **`left` does not count planned income that has not arrived**, a second line does (5.2). The alternative (one figure with income) can overstate what the person may spend.
3. Dates that fall on a weekend move **forward to Monday** for every plan, as in D-749. Salary that a bank books the Friday before still matches (window 3 banking days); it only lands in the next month when the 1st is a weekend.
4. **Rules run before plans**, and a plan only fills an empty category (3.5).
5. Remembered moves are **not** in the forecast (6).
6. The forecast uses **calendar months** and the main currency.
7. The default Overview of **new** books gets the two tiles; existing dashboards stay as they are.
8. Plans have no payee link; "Paid to" is a hint for matching and the payee name when booking.

## 10. Build plan

| PR | Content | Tier |
|---|---|---|
| 1 | **[format]** Core `Schedule`, `due_dates`, matching, layout 33, store and history, export and import, pipeline step, `forecast_month` / `months`, command line, `docs/BOOK_FORMAT.md`, `docs/EXPORT_FORMAT.md`, `docs/ERROR_CODES.md` | strong, high (writes the book, computes money) |
| 2 | UI: the form, *Plan this again…*, Planned payments page, import summary part, *Did this happen?*, the two tiles and their datasets, translations, glossary | standard, high |

PR 1 merges before the stage Go. Neither is expected to carry a human gate beyond what the maintainer's review of the layout needs (no dependency, no keychain, no bank code, no AI data); the build threads tick the list honestly. This design pull request is docs only and carries none.
