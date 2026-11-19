<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Budgets: Pause and Resume (BS2, #1241)

Design for #1241 (D-909, D-917). Thread G, step C3. The build follows after this note and after the layout-37 pull request (C2) has merged, because both touch the budgets screen.

## 1. The problem

The budget card has a button **Stop** (German *Beenden*). One click ends the budget after the current period. Testers took it for "leave this page", nothing says what it does, and there is no way back except making a new budget. Vondor decided (D-909): replace it with a **Pause / Resume** pair. The past results are kept, and a resumed budget runs again from today.

## 2. What the user sees

- **Pause** (German *Pausieren*) on a running budget opens a short question first, never a one-click change: *Pause the budget for Groceries? It counts until 30 Nov 2026 and stops after that. Its past results stay. You can resume it later.* Buttons: *Pause* and *Cancel*. The notice after it offers Undo, like every change.
- A paused budget's card says *Paused: it stopped counting on 30 Nov 2026* in place of today's *Ended*, and shows **Resume** (German *Fortsetzen*) instead of Pause.
- **Resume**, asked in the same way, says what happens, depending on the case:
  - *The budget is still inside its last period* (its last day is still ahead): it simply counts on; nothing else changes.
  - *The budget has stopped counting* (its last day is past): *Resume the budget for Groceries from 1 Dec 2026? It starts again with the same categories and amount. The months in between are not counted, and what was left over before is not carried into it. The old results stay under the paused budget.*
- Change and Delete stay as they are (Delete already warns and has Undo).

## 3. No format change (answers Q-684)

D-917 allows a stored paused state if it fits better. It does not need one. Resuming the *same* budget across a gap would make the months in between count as periods (planned amount, nothing spent, left-over carried on), which are wrong figures (principle 1). Keeping a list of pauses inside the budget would need new report rules and a new layout. So:

- **Pause** is what *Stop* does today: it sets the budget's last day to the end of the current period (`stop_budget`, undoable).
- **Resume, last day still ahead:** clears the last day (`set_budget_last_day(None)`), one change.
- **Resume, last day past:** makes a **new budget from the start of the period that contains today**, copying name, categories, accounts, amount, how often (and salary day) and carry-over setting from the paused budget's latest set, in one change (`add_budget`). The old budget and its history stay untouched. If another budget counts the same categories and accounts today, the store's usual overlap rule refuses it with its plain message.

So **layout 37 stays exactly #1154's** (D-893, D-917); no migration comes from this step.

## 4. Build (C3, after C2)

- `kassensturz-ui-bridge` `budgets.rs`: `resume_budget(book, id, locale)` doing the two cases of section 3, and the row's new fields `canResume` and `canPause` in place of `canStop`; `budgets_model.rs` exposes `pauseBudget` and `resumeBudget`.
- `BudgetsPage.qml`: the *Stop* button becomes *Pause*, with the confirmation dialog; *Resume* with its own; the card text. `ActionLog` names `BudgetsPage_pause`, `BudgetsPage_resume` and the dialogs' buttons; German and Polish texts.
- Tests: a budget paused and resumed in the same period is the same budget with no last day; a budget resumed after its last day gives a new budget with the old one's history unchanged and no figure of a month in between; resuming into an overlap is refused; Undo brings back each state; the bridge object is read back the way QML does (AGENTS.md §9.4); a smoke test for the two dialogs.

## 5. Decision

D-909 and D-917 already cover the choice. The "no stored paused state" reading of D-917 is the one proposed here; if Vondor prefers a stored pause list, it joins the layout-37 pull request before C2 starts.
