# The warnings tile and a reserve target for an account (US4.9)

Design of User Sprint 4 item US4.9, written by thread C in Phase 2 before its build. It answers two wishes with one tile:

- **#726:** a tile on the Overview that tells the user when a limit is crossed: a budget over (or under) a share of its amount, an account balance under an amount, and later other figures.
- **#519 point 3** (US1.11): an account can have a **reserve target**, "this account should always hold N months of budget X", checked at the month's lowest point, and the app warns when it falls short. Points 1 and 2 of #519 (budgets over several categories and limited to chosen accounts) are already built (D-639).

They are designed together because a reserve target is one kind of warning: "the account is under N times what this budget plans". One tile, one list of rules, one way to read them.

Release stage: **Alpha** (D-274). The rules live in the tile's settings in the dashboard document, so **the book format is not touched**: no layout change, no migration, not a **[format]** item, and no human gate. The document `VERSION` stays as it is: an older build shows the tile as a placeholder (D-278, D-504).

**What stays true (D-342):** the start screen is complete without it. The tile is not in the built-in Overview or a template; the user adds it under *Edit layout › Add › Notes*, and it shows nothing alarming until the user adds a rule. **Numbers come from code (AGENTS.md §8):** every figure and every comparison is worked out in Rust with exact `Money`, compared as integer minor units; amounts reach QML only as text (D-065). **Nothing leaves the app:** no notification, no sound, no network, no AI (§3.1 rule 4, §8).

## 1. The tile (D-721)

| | |
|---|---|
| Type id | `notice.alarms` |
| Name | *Warnings* (German *Warnungen*); in the Add menu under *Notes*, next to the other notices |
| Kind | a page widget with a bridge object of its own (as the budget bars, `BookBudgets`); it names no dataset (`shows_no_data()` is true) |
| AI | in `LIBRARY`, **not** in `ALL`: the AI does not propose it and never sees its rules (no AI data sharing gate) |
| Sizes | default 6 × 3, smallest 3 × 2 (`grid.rs`) |
| Properties | `alarms` (the list of rules, §2), `title` (empty: *Warnings*) |
| Period | **today**, always. The page's period header and filter bar don't change it: a warning is about where the user stands now |
| Debug log | the detail word `notice_alarms` (`vocabulary.rs`); rules, names and amounts are never logged |

**What it shows**, in this order:

1. **No rule yet:** *No warnings set yet. Add one in this tile's settings: for example "tell me when the shared account falls under 500".* In edit mode the settings button is next to it.
2. **Rules, none triggered:** a green tick and *All fine. None of your 3 warnings applies today.* (plural form by count).
3. **Rules, some triggered:** one line per triggered rule, in the order of the list, each a plain sentence (§3), with a warning icon. Below them, small: *2 more warnings set, all fine.* when some rules are not triggered.
4. A rule whose budget or account was **removed from the book** shows a line of its own: *A warning is about a budget that no longer exists. Change or remove it in this tile's settings.* It is never dropped silently.
5. A rule that **can't be compared** (the account and the budget have different currencies, or the budget plans nothing this period) shows why in one line (§3), so the user never reads "all fine" for a rule that could not be checked.

A triggered budget line can be clicked and opens the Budgets page; account lines are plain text (opening an account's register from a tile is a possible follow-up). Keyboard: the lines are in the tab order; each has a screen-reader name equal to its sentence.

## 2. The rules (D-722)

`props.alarms` is a list of at most **20 rules**. Each rule is an object:

| Field | For | Value |
|---|---|---|
| `reason` | every rule | one word of §3 |
| `account` | account rules | an account's ID as text (one of the user's own accounts, open or closed) |
| `budget` | budget rules and the reserve | a budget's ID as text |
| `amount` | `account_below` | the limit as a plain decimal, a dot before the minor digits, an optional minus (`"500.00"`, `"-1200"`); at most 20 characters. It is read in the account's currency with the digits the book stores for it (D-027), so it never changes scale |
| `share` | budget rules | a whole percent, 1 to 1,000 (default 100 for over, 80 for under) |
| `times` | `account_reserve` | a whole number, 1 to 24 (default 1) |
| `lowest` | account rules | yes or no: compare the lowest point of this month (§4) instead of today's balance. Default: no for `account_below`, yes for `account_reserve` |

- The document is untrusted input (D-281): `Component::from_value` checks every field (known `reason`, the fields that reason needs, ID length at most 64, the ranges and the amount pattern), and a document with a bad rule is refused like any other bad property. Unknown extra fields are refused.
- **Why in the tile and not in the book:** no book-format change, no migration from Beta on, no gate; and a rule is a way of looking at the book, like the budget bars' *Which budgets*. The cost: two tiles have two separate lists. If testers want one list for the whole book, moving it into the book is a later **[format]** step.
- **A dashboard file** (D-441, D-432) names no IDs of the book, so it leaves `alarms` out, as it leaves out *Which budgets* (D-642). A tile opened from a file starts with no rules (D-726).
- **Copy and paste of a tile** within the same book keeps the rules.

## 3. The four reasons and how each compares (D-723)

All comparisons are on exact integer minor units of one currency. "Share" comparisons multiply both sides, never divide: `spent × 100` against `available × share`, in 128-bit integers, so nothing overflows or rounds.

| Reason | Triggered when | The sentence (English; amounts as the app shows them) |
|---|---|---|
| `budget_over` | in the budget's period that contains today, `spent × 100 > available × share` | share 100: *Food is over by 20.00 € this month.* Other shares: *Food has used 340.00 € of 400.00 € this month, more than 80 %.* |
| `budget_under` | in the budget's **last finished** period, `spent × 100 < available × share` | *Savings used only 120.00 € of 300.00 € last month, less than 80 %.* |
| `account_below` | the balance (today's, or this month's lowest, §4) `< amount` | *Shared account holds 420.00 €, under your limit of 500.00 €.* With `lowest`: *Shared account went down to 180.00 € this month, under your limit of 500.00 €.* |
| `account_reserve` | the balance (this month's lowest by default, §4) `< times × planned` | *Shared account went down to 900.00 € this month, under its reserve of 1 month of Household costs (1,900.00 €).* |

- **`available`** is what the budget bars show as available: the planned amount plus what was carried in (D-347). `spent` is the bars' spent (refunds reduce it, D-639 accounts and categories apply). The budget rules read `budget_utilization` with the grouping `budget`, so the tile and the bars always agree.
- **"over" is strict**: exactly 100 % used is not over, as the bars say. **"under" is strict** too.
- **`budget_under` looks at the period that ended last**, because a period that is still running is nearly always "under" at its start, which would make the tile cry wolf every first of the month. A budget with no finished period yet (it started this period, or it is a `Range` that has not ended) is not triggered and is not counted as "can't compare". A budget that has **ended** (D-710) is checked on its last period once, then stays as it is.
- **A budget with nothing to spend** this period (available zero or less, after overspending was carried in): `budget_over` is triggered whenever the bars say *over*, that is as soon as `spent` is more than `available` (*Food has nothing left to spend this month, and 12.00 € were spent.*); `budget_under` is not triggered. (A budget's planned amount is never zero; the book refuses that.)
- **The words "this month" / "last month"** follow the budget's own period: *this quarter*, *this year*, *this salary month*, *in this period* for a range.
- **No advice** (§3.1 rule 11): the sentences say what is, never what to do with the money.

The default share of `budget_under` is 80 so a new rule makes sense without editing; the default of `budget_over` is 100 ("over budget").

## 4. The lowest point of the month (D-724)

`lowest` means: the smallest of the account's **end-of-day balances** from the last day of the month before to today (the start of the month and every day of it so far). Several bookings on one day count only as that day's end, so a salary and a rent booked the same day don't show a dip that never was. When several days tie, the sentence names the first of them (the day it went down to that). The month is the **calendar month** that contains today; on the first of a month it is the end of the month before and today.

- **Why the lowest point and not the balance now:** #519 asks whether the reserve holds "after fixed costs go out, before contributions arrive". The balance now would say "fine" right after the salary and hide the dip of the last week. Only what has been booked counts: planned payments in the future are the forecast of #725 (after Gold), not this.
- Liability accounts: the balance is as booked, a debt is negative (as in `account_balances`); "under −1,000" means more than 1,000 owed. The rule editor says so for an account the user owes on.

## 5. The reserve target (D-725)

A reserve is **N times what a budget plans for the period that contains today** (`times × planned`, not `available`: carried money is not part of the target). It is checked with `account_reserve` against the account's balance, by default at this month's lowest point.

- **N times a budget, not N months:** the multiplication is exact with no rounding. For a monthly budget, *1 time* reads *1 month*; for a quarterly one *1 quarter*, and so on, so the sentence matches what the user set.
- **Different currencies:** an account in CHF and a budget in EUR are not converted (rates are entered by hand, D-028, and a target should not move with a rate). The line says *Can't check the reserve of Savings: the account is in CHF and Household costs is in EUR.* and the rule is not counted as fine.
- **A plain "N" amount reserve** is just `account_below` with `lowest` on, so it needs no reason of its own.
- **Where the user finds it:** the tile's rule editor names it *Keep a reserve on an account*. The account's own page gets no new field in this step; a link "Set a reserve warning" there is a possible follow-up if testers look for it there.

## 6. The rule editor

In the properties panel, the field *Warnings* lists each rule as its short sentence (*Shared account under 500.00 €*, *Food over 80 %*), with *Change* and *Remove* buttons, and an *Add a warning…* button. Add and Change open one small dialog (`AlarmRuleDialog.qml`):

1. **What should we watch?** four radio buttons, each with one line of help:
   - *A budget is used up past a share*: "For example, tell me when Food has used more than 80 % this month."
   - *A budget was used less than a share*: "For example, tell me when less than 80 % of Savings was put away last month."
   - *An account falls under an amount*: "For example, tell me when the shared account is under 500."
   - *Keep a reserve on an account*: "The account should always hold, say, one month of a budget. Checked at the month's lowest point."
2. **Which budget / account?** a list of the book's budgets or own accounts (closed ones marked, as in *Which budgets*).
3. **The value:** *Share (%)* spin box, *Amount* field (typed as the user writes amounts; Rust parses it with the account's currency and locale and stores it in the plain form of §2), or *How many periods of which budget* (spin box and budget list). For account rules a check box *Check the lowest point of this month, not just today* with one line of help.
4. *Save* stays disabled until every needed field is valid; a wrong amount says what is wrong (*Type an amount such as 500 or 500,00.*).

The bridge validates every rule again when it is saved; QML never computes or compares an amount.

## 7. The build (for the step that follows this design)

Following `docs/ADDING_A_WIDGET.md`:

- **`kassensturz-report`:** `ComponentType::Alarms` (`notice.alarms`), `Kind::Alarms` with an `AlarmRule` setting (checked in `component.rs`), sizes in `grid.rs`; a new `alarms.rs` with `evaluate(source, rules, today) -> Vec<AlarmResult>` (state *triggered / fine / gone / can't compare*, the figures as `Money`, the period kind), using `budget_utilization` and `balances_at`. `dashboard_file.rs` leaves `alarms` out.
- **`kassensturz-core`:** `notice_alarms` in `DETAILS`.
- **`kassensturz-ui-bridge`:** `alarms.rs` (open book → result rows with text figures and the parsed rule summaries) and `alarms_model.rs` (`BookAlarms` qobject, `show(rules)`, `refresh()`), a `setAlarms` and an amount check in the properties model, preview in `properties.rs`.
- **QML:** `AlarmsBox` in `GridItem.qml`, `AlarmRuleDialog.qml`, the *Warnings* field in `PropertiesPanel.qml`, `GridNames.qml`, `AddMenu.qml`, `TemplatePicture.qml`, `build.rs`, `tools/check-panel.sh`, a smoke test `AlarmsSmoke.qml` with a fake `BookAlarms`.
- **Tests:** each reason triggered and not, at the exact limit (strict), share and reserve with large amounts (no overflow), the lowest point with several bookings on one day and on the first of a month, a removed budget or account, different currencies, a budget with no finished period, an ended budget; the rule list read from and written to a document and refused with each bad field; the dashboard file leaving the rules out; the bridge objects read back the way QML reads them (AGENTS.md §9.4); the figures checked against the real `budget_utilization` output (seam). X3 does not apply: no importer writes a rule; the rules live only in dashboard documents, and their limits are tested on the document reader and on the currencies' stored digits.
- **Text:** English and German entries in `po/de`; `docs/GLOSSARY.md` gets *Warning* and *Reserve*.

## 8. Not in this step

- A notification outside the tile (system tray, a pop-up at start): needs its own design; nothing leaves the app (#726 open question, kept off).
- More reasons (a category's spending in a period, uncategorised transactions above a count): the reason list is a fixed word list, so adding one later is one row in §3 and one radio button.
- Warning levels (warning and alarm) per rule: one value per rule keeps the editor simple; a second rule with a higher value gives the same.
- The forecast after planned payments (#725, after Gold): a later reason could use it.
- The AI seeing or proposing warnings (a human gate, later if wanted).
