// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Budgets (D-344, D-345; layout 22): an amount the user plans to spend on
//! one spending category in each of a budget's periods.
//!
//! A budget refers to its category by ID, and nothing here ever writes to a
//! transaction or a category. The amount and the carry-over switch are
//! dated rows, so a change applies from the start of one period on and an
//! earlier period keeps what it had (D-345). The book checks what keeps a
//! budget's figures meaningful when it writes: a spending category, days
//! that fit the kind of period, one budget per category at a time, amounts
//! above zero in the category's currency, and the size limits. How a budget
//! counts is the reporting layer's job (D-346, D-347).

use std::collections::HashMap;

use kassensturz_core::{AccountId, AccountKind, Budget, BudgetAmount, BudgetEvery, BudgetId, Date};
use kassensturz_core::{CurrencyCode, Money};
use rusqlite::params;

use crate::error::StoreError;
use crate::{Action, Book, currency_from_text, date_from_text, date_to_text};

/// The most budgets a book keeps, so a hostile copied book can't make the
/// reports slow (D-345, T-47).
const MAX_BUDGETS: usize = 1_000;
/// The most amounts one budget keeps (D-345, T-47).
const MAX_AMOUNTS: usize = 1_200;
/// The most calendar months a range may touch: 100 years, as for a report's
/// period (`MAX_MONTHS`, D-281).
const MAX_RANGE_MONTHS: i64 = 1_200;
/// The largest amount a budget plans for one period, in minor units: as
/// much as 1,200 periods (100 years of months) of it still add up in a
/// `Money` (D-360). Far above any household's budget.
const MAX_AMOUNT: i64 = i64::MAX / MAX_AMOUNTS as i64;

fn every_to_text(every: BudgetEvery) -> &'static str {
    match every {
        BudgetEvery::Month => "month",
        BudgetEvery::Quarter => "quarter",
        BudgetEvery::Year => "year",
        BudgetEvery::Range => "range",
    }
}

fn every_from_text(text: &str) -> BudgetEvery {
    // The table's CHECK allows nothing else.
    match text {
        "quarter" => BudgetEvery::Quarter,
        "year" => BudgetEvery::Year,
        "range" => BudgetEvery::Range,
        _ => BudgetEvery::Month,
    }
}

/// Whether two budgets' days overlap; a last day of `None` means the
/// budget keeps going.
fn days_overlap(
    first: Date,
    last: Option<Date>,
    other_first: Date,
    other_last: Option<Date>,
) -> bool {
    first <= other_last.unwrap_or(Date::MAX) && other_first <= last.unwrap_or(Date::MAX)
}

/// How many calendar months the days from `first` to `last` touch.
fn months_touched(first: Date, last: Date) -> i64 {
    i64::from(last.year() - first.year()) * 12 + i64::from(last.month()) - i64::from(first.month())
        + 1
}

/// Refuses days that don't fit the kind of period; see [`Book::add_budget`].
fn check_days(every: BudgetEvery, first: Date, last: Option<Date>) -> Result<(), StoreError> {
    let period = |reason| StoreError::BudgetPeriod { reason };
    if !every.is_period_start(first) {
        return Err(period("not-period-start"));
    }
    match last {
        None if every == BudgetEvery::Range => Err(period("no-last-day")),
        None => Ok(()),
        Some(last) if last < first => Err(period("ends-before-starts")),
        Some(last) if !every.is_period_end(last) => Err(period("not-period-end")),
        Some(last)
            if every == BudgetEvery::Range && months_touched(first, last) > MAX_RANGE_MONTHS =>
        {
            Err(period("too-long"))
        }
        Some(_) => Ok(()),
    }
}

/// Refuses an amount that isn't above zero, is too large or isn't in the
/// category's currency.
fn check_amount(amount: Money, currency: CurrencyCode) -> Result<(), StoreError> {
    let refused = |reason| StoreError::BudgetAmount { reason };
    if amount.currency() != currency {
        Err(refused("currency"))
    } else if amount.minor_units() <= 0 {
        Err(refused("zero"))
    } else if amount.minor_units() > MAX_AMOUNT {
        Err(refused("too-large"))
    } else {
        Ok(())
    }
}

/// Refuses an amount row whose day is not the start of one of the budget's
/// periods, or that would carry over in a range.
fn check_row(
    budget: &Budget,
    row: &BudgetAmount,
    currency: CurrencyCode,
) -> Result<(), StoreError> {
    let period = |reason| StoreError::BudgetPeriod { reason };
    check_amount(row.amount, currency)?;
    if row.from_day < budget.first_day {
        return Err(period("amount-before-start"));
    }
    if budget.last_day.is_some_and(|last| row.from_day > last) {
        return Err(period("amount-after-end"));
    }
    let starts_period = if budget.every.repeats() {
        budget.every.is_period_start(row.from_day)
    } else {
        row.from_day == budget.first_day
    };
    if !starts_period {
        return Err(period("not-period-start"));
    }
    if row.carry_over && !budget.every.repeats() {
        return Err(period("carry-over-on-range"));
    }
    Ok(())
}

impl Book {
    /// Every budget, in the order they were made (their IDs), each with its
    /// amounts, oldest first.
    pub fn budgets(&self) -> Result<Vec<Budget>, StoreError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT budgets.id, budgets.category_id, budgets.every, budgets.first_day,
                    budgets.last_day, accounts.currency
             FROM budgets JOIN accounts ON accounts.id = budgets.category_id
             ORDER BY budgets.id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, [u8; 16]>(0)?,
                    row.get::<_, [u8; 16]>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut budgets = Vec::with_capacity(rows.len());
        let mut currencies = HashMap::new();
        for (id, category, every, first_day, last_day, currency) in rows {
            let id = BudgetId::from_bytes(id);
            currencies.insert(id, currency_from_text(&currency)?);
            budgets.push(Budget {
                id,
                category: AccountId::from_bytes(category),
                every: every_from_text(&every),
                first_day: date_from_text(&first_day)?,
                last_day: last_day.as_deref().map(date_from_text).transpose()?,
                amounts: Vec::new(),
            });
        }
        let place: HashMap<BudgetId, usize> = budgets
            .iter()
            .enumerate()
            .map(|(index, budget)| (budget.id, index))
            .collect();
        let mut statement = self.conn.prepare_cached(
            "SELECT budget_id, from_day, amount, carry_over FROM budget_amounts
             ORDER BY budget_id, from_day",
        )?;
        let amounts = statement
            .query_map([], |row| {
                Ok((
                    BudgetId::from_bytes(row.get(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (budget, from_day, amount, carry_over) in amounts {
            // The foreign key makes sure the budget is there.
            if let (Some(&index), Some(&currency)) = (place.get(&budget), currencies.get(&budget)) {
                budgets[index].amounts.push(BudgetAmount {
                    from_day: date_from_text(&from_day)?,
                    amount: Money::from_minor_units(amount, currency),
                    carry_over,
                });
            }
        }
        Ok(budgets)
    }

    /// The budget `id` ([`StoreError::UnknownBudget`] if there is none).
    pub fn budget(&self, id: BudgetId) -> Result<Budget, StoreError> {
        self.budgets()?
            .into_iter()
            .find(|budget| budget.id == id)
            .ok_or(StoreError::UnknownBudget(id))
    }

    /// Makes a new budget for `category`, planning `amount` for each period
    /// from `first_day` on; the user action [`Action::AddBudget`] (D-344).
    ///
    /// The category must be in the book ([`StoreError::UnknownAccount`]) and
    /// a spending category ([`StoreError::BudgetNotSpending`]). A month,
    /// quarter or year budget starts on the first day of one and, if it has
    /// a last day, ends on the last day of one; a range needs a last day and
    /// touches at most 1,200 calendar months; the last day is never before
    /// the first ([`StoreError::BudgetPeriod`]: `not-period-start`,
    /// `not-period-end`, `no-last-day`, `too-long`, `ends-before-starts`).
    /// The amount is above zero, not absurdly large (D-360) and in
    /// the category's currency ([`StoreError::BudgetAmount`]: `zero`,
    /// `too-large`, `currency`), and a range doesn't carry over
    /// (`carry-over-on-range`). The category has no other budget for any of
    /// these days ([`StoreError::BudgetOverlap`]), and a book keeps at most
    /// 1,000 budgets ([`StoreError::TooManyBudgets`]).
    pub fn add_budget(
        &mut self,
        category: AccountId,
        every: BudgetEvery,
        first_day: Date,
        last_day: Option<Date>,
        amount: Money,
        carry_over: bool,
    ) -> Result<BudgetId, StoreError> {
        let budget = Budget {
            id: BudgetId::generate(),
            category,
            every,
            first_day,
            last_day,
            amounts: vec![BudgetAmount {
                from_day: first_day,
                amount,
                carry_over,
            }],
        };
        self.as_one_action(Action::AddBudget, |book| book.insert_budget(&budget))?;
        Ok(budget.id)
    }

    /// Plans `amount` with `carry_over` for the budget `id` from the period
    /// that starts on `from_day` on; the user action [`Action::EditBudget`]
    /// (D-345). Earlier periods keep what they had, and what was planned for
    /// later periods is replaced, so "from this period on" means what it
    /// says. To correct a mistake from the start, `from_day` is the budget's
    /// first day.
    ///
    /// `from_day` is the first day of one of the budget's periods
    /// ([`StoreError::BudgetPeriod`]: `not-period-start`,
    /// `amount-before-start`, `amount-after-end`); the amount follows the
    /// rules of [`Book::add_budget`]. Nothing that stays the same is
    /// recorded.
    pub fn edit_budget_amount(
        &mut self,
        id: BudgetId,
        from_day: Date,
        amount: Money,
        carry_over: bool,
    ) -> Result<(), StoreError> {
        let budget = self.budget(id)?;
        let currency = self.account(budget.category)?.currency;
        let row = BudgetAmount {
            from_day,
            amount,
            carry_over,
        };
        check_row(&budget, &row, currency)?;
        let earlier = budget
            .amounts
            .iter()
            .filter(|kept| kept.from_day < from_day)
            .count();
        if earlier + 1 > MAX_AMOUNTS {
            return Err(StoreError::TooManyBudgets {
                limit: "amounts",
                max: MAX_AMOUNTS,
            });
        }
        let same_day = budget.amounts.iter().find(|old| old.from_day == from_day);
        self.as_one_action(Action::EditBudget, |book| {
            let day = date_to_text(from_day)?;
            book.conn.execute(
                "DELETE FROM budget_amounts WHERE budget_id = ?1 AND from_day > ?2",
                params![id.to_bytes(), day],
            )?;
            match same_day {
                Some(old) if *old == row => {}
                Some(_) => {
                    book.conn.execute(
                        "UPDATE budget_amounts SET amount = ?3, carry_over = ?4
                         WHERE budget_id = ?1 AND from_day = ?2",
                        params![id.to_bytes(), day, amount.minor_units(), carry_over],
                    )?;
                }
                None => book.insert_amount(id, &row)?,
            }
            Ok(())
        })
    }

    /// Gives the budget `id` a new last day, or none so it keeps going; the
    /// user action [`Action::EditBudget`] (D-345). *Stop this budget* sets
    /// it to the end of the current period, and the past stays. What was
    /// planned for periods after the new last day goes, in the same change,
    /// so Undo brings it back.
    ///
    /// The last day follows the rules of [`Book::add_budget`], including
    /// that the category has no other budget for any of the budget's days
    /// ([`StoreError::BudgetOverlap`]).
    pub fn set_budget_last_day(
        &mut self,
        id: BudgetId,
        last_day: Option<Date>,
    ) -> Result<(), StoreError> {
        let budget = self.budget(id)?;
        check_days(budget.every, budget.first_day, last_day)?;
        self.check_overlap(budget.category, Some(id), budget.first_day, last_day)?;
        self.as_one_action(Action::EditBudget, |book| {
            let last = last_day.map(date_to_text).transpose()?;
            if let Some(last) = &last {
                book.conn.execute(
                    "DELETE FROM budget_amounts WHERE budget_id = ?1 AND from_day > ?2",
                    params![id.to_bytes(), last],
                )?;
            }
            book.conn.execute(
                "UPDATE budgets SET last_day = ?2 WHERE id = ?1 AND last_day IS NOT ?2",
                params![id.to_bytes(), last],
            )?;
            Ok(())
        })
    }

    /// Deletes the budget `id` with all its amounts and so with its past
    /// results; the user action [`Action::RemoveBudget`]. Its category and
    /// every transaction stay as they are, and one Undo brings the budget
    /// back (D-032).
    pub fn remove_budget(&mut self, id: BudgetId) -> Result<(), StoreError> {
        self.budget(id)?;
        self.as_one_action(Action::RemoveBudget, |book| {
            book.conn
                .execute("DELETE FROM budgets WHERE id = ?1", [id.to_bytes()])?;
            Ok(())
        })
    }

    /// Puts back budgets with the IDs and amounts they had, e.g. from a
    /// complete export (D-029). The checks of [`Book::add_budget`] apply to
    /// each budget and each of its amounts, and a budget keeps at most 1,200
    /// amounts. Its amounts are given oldest first, each day once
    /// (`amount-twice`), and the first starts on the budget's first day
    /// (`first-amount-missing`).
    ///
    /// # Panics
    ///
    /// Outside [`Book::fill_new`].
    pub fn restore_budgets(&mut self, budgets: &[Budget]) -> Result<(), StoreError> {
        assert!(
            self.filling,
            "only a new book being filled takes budgets this way"
        );
        self.as_one_action(Action::AddBudget, |book| {
            for budget in budgets {
                book.insert_budget(budget)?;
            }
            Ok(())
        })
    }

    /// Checks `budget` and writes it with its amounts.
    fn insert_budget(&self, budget: &Budget) -> Result<(), StoreError> {
        let category = self.account(budget.category)?;
        if category.kind != AccountKind::Expense {
            return Err(StoreError::BudgetNotSpending(budget.category));
        }
        check_days(budget.every, budget.first_day, budget.last_day)?;
        if budget.amounts.len() > MAX_AMOUNTS {
            return Err(StoreError::TooManyBudgets {
                limit: "amounts",
                max: MAX_AMOUNTS,
            });
        }
        let period = |reason| StoreError::BudgetPeriod { reason };
        match budget.amounts.first() {
            Some(first) if first.from_day == budget.first_day => {}
            Some(first) if first.from_day < budget.first_day => {
                return Err(period("amount-before-start"));
            }
            _ => return Err(period("first-amount-missing")),
        }
        for (row, next) in budget.amounts.iter().zip(&budget.amounts[1..]) {
            if next.from_day <= row.from_day {
                return Err(period("amount-twice"));
            }
        }
        for row in &budget.amounts {
            check_row(budget, row, category.currency)?;
        }
        let count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM budgets", [], |row| row.get(0))?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_BUDGETS {
            return Err(StoreError::TooManyBudgets {
                limit: "budgets",
                max: MAX_BUDGETS,
            });
        }
        self.check_overlap(budget.category, None, budget.first_day, budget.last_day)?;
        self.conn.execute(
            "INSERT INTO budgets (id, category_id, every, first_day, last_day)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                budget.id.to_bytes(),
                budget.category.to_bytes(),
                every_to_text(budget.every),
                date_to_text(budget.first_day)?,
                budget.last_day.map(date_to_text).transpose()?,
            ],
        )?;
        for row in &budget.amounts {
            self.insert_amount(budget.id, row)?;
        }
        Ok(())
    }

    fn insert_amount(&self, budget: BudgetId, row: &BudgetAmount) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO budget_amounts (id, budget_id, from_day, amount, carry_over)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                uuid::Uuid::now_v7().into_bytes(),
                budget.to_bytes(),
                date_to_text(row.from_day)?,
                row.amount.minor_units(),
                row.carry_over
            ],
        )?;
        Ok(())
    }

    /// Refuses days of `category` that another of its budgets (not `own`)
    /// already covers (D-344).
    fn check_overlap(
        &self,
        category: AccountId,
        own: Option<BudgetId>,
        first: Date,
        last: Option<Date>,
    ) -> Result<(), StoreError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT id, first_day, last_day FROM budgets WHERE category_id = ?1")?;
        let others = statement
            .query_map([category.to_bytes()], |row| {
                Ok((
                    BudgetId::from_bytes(row.get(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, other_first, other_last) in others {
            if Some(id) == own {
                continue;
            }
            let other_first = date_from_text(&other_first)?;
            let other_last = other_last.as_deref().map(date_from_text).transpose()?;
            if days_overlap(first, last, other_first, other_last) {
                return Err(StoreError::BudgetOverlap { category });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
