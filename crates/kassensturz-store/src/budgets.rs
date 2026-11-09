// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Budgets (D-344, D-345, D-639): an amount the user plans to spend on one
//! or more spending categories in each of a budget's periods, optionally
//! counting only what was paid from chosen accounts of the user's.
//!
//! A budget refers to its categories and accounts by ID, and nothing here
//! ever writes to a transaction, a category or an account. The amount and the carry-over switch are
//! dated rows, so a change applies from the start of one period on and an
//! earlier period keeps what it had (D-345). The book checks what keeps a
//! budget's figures meaningful when it writes: spending categories in one
//! currency, the user's own accounts, days that fit the kind of period, a
//! category's money in one budget at a time, amounts above zero in the
//! categories' currency, and the size limits. How a budget
//! counts is the reporting layer's job (D-346, D-347).

use std::collections::HashMap;

use kassensturz_core::{
    Account, AccountId, AccountKind, Budget, BudgetAmount, BudgetEvery, BudgetId, Date,
};
use kassensturz_core::{CurrencyCode, Money};
use rusqlite::params;

use crate::error::StoreError;
use crate::{Action, Book, currency_from_text, date_from_text, date_to_text};

/// The most budgets a book keeps, so a hostile copied book can't make the
/// reports slow (D-345, T-47).
const MAX_BUDGETS: usize = 1_000;
/// The most amounts one budget keeps (D-345, T-47).
const MAX_AMOUNTS: usize = 1_200;
/// The most categories one budget counts (D-639, T-47).
const MAX_CATEGORIES: usize = 100;
/// The most accounts one budget counts from (D-639, T-47).
const MAX_ACCOUNTS: usize = 100;
/// The most characters of a budget's name (D-664): enough for "Holiday in
/// Italy with the children", short enough for a bar's title. The table's
/// CHECK says the same.
const MAX_NAME_CHARS: usize = 100;
/// The most calendar months a range may touch: 100 years, as for a report's
/// period (`MAX_MONTHS`, D-281).
const MAX_RANGE_MONTHS: i64 = 1_200;
/// The largest amount a budget plans for one period, in minor units: as
/// much as 1,200 periods (100 years of months) of it still add up in a
/// `Money` (D-360). Far above any household's budget.
const MAX_AMOUNT: i64 = i64::MAX / MAX_AMOUNTS as i64;

/// The `every` column of a budget; a salary month is a month whose
/// `salary_day` is set (layout 26, D-651).
fn every_to_text(every: BudgetEvery) -> &'static str {
    match every {
        BudgetEvery::Month | BudgetEvery::SalaryMonth { .. } => "month",
        BudgetEvery::Quarter => "quarter",
        BudgetEvery::Year => "year",
        BudgetEvery::Range => "range",
    }
}

fn every_from_text(text: &str, salary_day: Option<i64>) -> BudgetEvery {
    // The table's CHECKs allow nothing else: a salary day only on a month,
    // and only 2 to 31.
    match text {
        "quarter" => BudgetEvery::Quarter,
        "year" => BudgetEvery::Year,
        "range" => BudgetEvery::Range,
        _ => salary_day
            .and_then(|day| i8::try_from(day).ok())
            .and_then(BudgetEvery::salary_month)
            .unwrap_or(BudgetEvery::Month),
    }
}

/// A budget's name as the book keeps it (D-664): without spaces around it,
/// `None` when nothing is left, so an empty name means "no name". A name
/// longer than [`MAX_NAME_CHARS`] or with a line break or another control
/// character is refused ([`StoreError::BudgetName`]), never shortened.
pub fn checked_budget_name(name: Option<&str>) -> Result<Option<String>, StoreError> {
    let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
        return Ok(None);
    };
    let refused = |reason| StoreError::BudgetName {
        reason,
        max_chars: MAX_NAME_CHARS,
    };
    if name.chars().any(char::is_control) {
        return Err(refused("line-break"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(refused("too-long"));
    }
    Ok(Some(name.to_owned()))
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
            "SELECT id, every, first_day, last_day, salary_day, name FROM budgets ORDER BY id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, [u8; 16]>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut budgets = Vec::with_capacity(rows.len());
        for (id, every, first_day, last_day, salary_day, name) in rows {
            budgets.push(Budget {
                id: BudgetId::from_bytes(id),
                name,
                categories: Vec::new(),
                accounts: Vec::new(),
                every: every_from_text(&every, salary_day),
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
        // The budget's currency is its first category's; the book keeps
        // every category of a budget in one currency.
        let mut currencies = HashMap::new();
        let mut statement = self.conn.prepare_cached(
            "SELECT budget_categories.budget_id, budget_categories.category_id, accounts.currency
             FROM budget_categories JOIN accounts ON accounts.id = budget_categories.category_id
             ORDER BY budget_categories.budget_id, budget_categories.position",
        )?;
        let categories = statement
            .query_map([], |row| {
                Ok((
                    BudgetId::from_bytes(row.get(0)?),
                    AccountId::from_bytes(row.get(1)?),
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (budget, category, currency) in categories {
            if let Some(&index) = place.get(&budget) {
                if budgets[index].categories.is_empty() {
                    currencies.insert(budget, currency_from_text(&currency)?);
                }
                budgets[index].categories.push(category);
            }
        }
        let mut statement = self.conn.prepare_cached(
            "SELECT budget_id, account_id FROM budget_accounts ORDER BY budget_id, position",
        )?;
        let accounts = statement
            .query_map([], |row| {
                Ok((
                    BudgetId::from_bytes(row.get(0)?),
                    AccountId::from_bytes(row.get(1)?),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (budget, account) in accounts {
            if let Some(&index) = place.get(&budget) {
                budgets[index].accounts.push(account);
            }
        }
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

    /// Makes a new budget for `categories`, counting only what was paid from
    /// `accounts` (none: every account), planning `amount` for each period
    /// from `first_day` on; the user action [`Action::AddBudget`] (D-344,
    /// D-639).
    ///
    /// The categories must be in the book ([`StoreError::UnknownAccount`]),
    /// spending categories ([`StoreError::BudgetNotSpending`]), at least one,
    /// each once and all in one currency ([`StoreError::BudgetChoice`]:
    /// `no-category`, `category-twice`, `currencies`); the accounts are the
    /// user's own accounts, each once (`not-own-account`, `account-twice`).
    /// A budget counts at most 100 categories and 100 accounts
    /// ([`StoreError::TooManyBudgets`]). A month,
    /// quarter, year or salary-month budget starts on the first day of one
    /// and, if it has a last day, ends on the last day of one (a salary
    /// month on a day that is no day of a month has none, D-651); a range needs a last day and
    /// touches at most 1,200 calendar months; the last day is never before
    /// the first ([`StoreError::BudgetPeriod`]: `not-period-start`,
    /// `not-period-end`, `no-last-day`, `too-long`, `ends-before-starts`).
    /// The amount is above zero, not absurdly large (D-360) and in
    /// the categories' currency ([`StoreError::BudgetAmount`]: `zero`,
    /// `too-large`, `currency`), and a range doesn't carry over
    /// (`carry-over-on-range`). No other budget counts one of its categories
    /// from one of its accounts on any of these days
    /// ([`StoreError::BudgetOverlap`], [`Budget::clashes_with`]), and a book
    /// keeps at most 1,000 budgets ([`StoreError::TooManyBudgets`]).
    #[expect(
        clippy::too_many_arguments,
        reason = "a budget is made from all of these at once, each checked here"
    )]
    pub fn add_budget(
        &mut self,
        categories: &[AccountId],
        accounts: &[AccountId],
        every: BudgetEvery,
        first_day: Date,
        last_day: Option<Date>,
        amount: Money,
        carry_over: bool,
    ) -> Result<BudgetId, StoreError> {
        self.add_named_budget(
            None, categories, accounts, every, first_day, last_day, amount, carry_over,
        )
    }

    /// As [`Book::add_budget`], with the name the user gave it, if any
    /// (D-664): the rules of [`Book::set_budget_name`] apply, and the budget
    /// and its name are one user action.
    #[expect(
        clippy::too_many_arguments,
        reason = "a budget is made from all of these at once, each checked here"
    )]
    pub fn add_named_budget(
        &mut self,
        name: Option<&str>,
        categories: &[AccountId],
        accounts: &[AccountId],
        every: BudgetEvery,
        first_day: Date,
        last_day: Option<Date>,
        amount: Money,
        carry_over: bool,
    ) -> Result<BudgetId, StoreError> {
        let budget = Budget {
            id: BudgetId::generate(),
            name: checked_budget_name(name)?,
            categories: categories.to_vec(),
            accounts: accounts.to_vec(),
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
        let currency = self.budget_currency(&budget)?;
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
    /// that no other budget counts the same money on any of the budget's
    /// days ([`StoreError::BudgetOverlap`]).
    pub fn set_budget_last_day(
        &mut self,
        id: BudgetId,
        last_day: Option<Date>,
    ) -> Result<(), StoreError> {
        let budget = self.budget(id)?;
        check_days(budget.every, budget.first_day, last_day)?;
        self.check_overlap(&Budget {
            last_day,
            ..budget.clone()
        })?;
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

    /// Gives the budget `id` the name `name`, or takes its name away with
    /// `None` or an empty name, so the screens call it by its categories
    /// again; the user action [`Action::EditBudget`] (D-664). Spaces around
    /// the name are left out. A name has at most 100 characters and fits on
    /// one line ([`StoreError::BudgetName`]: `too-long`, `line-break`).
    /// Names don't need to differ: a name is a label, the budget is still
    /// known by its ID. Nothing that stays the same is recorded.
    pub fn set_budget_name(&mut self, id: BudgetId, name: Option<&str>) -> Result<(), StoreError> {
        let name = checked_budget_name(name)?;
        self.budget(id)?;
        self.as_one_action(Action::EditBudget, |book| {
            book.conn.execute(
                "UPDATE budgets SET name = ?2 WHERE id = ?1 AND name IS NOT ?2",
                params![id.to_bytes(), name],
            )?;
            Ok(())
        })
    }

    /// Deletes the budget `id` with all its amounts and so with its past
    /// results; the user action [`Action::RemoveBudget`]. Its categories,
    /// accounts and
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

    /// The currency of `budget`'s amounts: its first category's, which all
    /// its categories share.
    fn budget_currency(&self, budget: &Budget) -> Result<CurrencyCode, StoreError> {
        let first = budget.categories.first().ok_or(StoreError::BudgetChoice {
            reason: "no-category",
        })?;
        Ok(self.account(*first)?.currency)
    }

    /// Refuses categories and accounts a budget can't count (see
    /// [`Book::add_budget`]) and returns the categories' one currency.
    fn check_choice(&self, budget: &Budget) -> Result<CurrencyCode, StoreError> {
        let choice = |reason| StoreError::BudgetChoice { reason };
        let too_many = |limit, max| StoreError::TooManyBudgets { limit, max };
        if budget.categories.len() > MAX_CATEGORIES {
            return Err(too_many("categories", MAX_CATEGORIES));
        }
        if budget.accounts.len() > MAX_ACCOUNTS {
            return Err(too_many("accounts", MAX_ACCOUNTS));
        }
        let mut currency = None;
        for (index, &id) in budget.categories.iter().enumerate() {
            if budget.categories[..index].contains(&id) {
                return Err(choice("category-twice"));
            }
            let category: Account = self.account(id)?;
            if category.kind != AccountKind::Expense {
                return Err(StoreError::BudgetNotSpending(id));
            }
            match currency {
                None => currency = Some(category.currency),
                Some(first) if first != category.currency => return Err(choice("currencies")),
                Some(_) => {}
            }
        }
        for (index, &id) in budget.accounts.iter().enumerate() {
            if budget.accounts[..index].contains(&id) {
                return Err(choice("account-twice"));
            }
            let account = self.account(id)?;
            if !matches!(account.kind, AccountKind::Asset | AccountKind::Liability) {
                return Err(choice("not-own-account"));
            }
        }
        currency.ok_or(choice("no-category"))
    }

    /// Checks `budget` and writes it with its amounts, categories and
    /// accounts.
    fn insert_budget(&self, budget: &Budget) -> Result<(), StoreError> {
        let name = checked_budget_name(budget.name.as_deref())?;
        let currency = self.check_choice(budget)?;
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
            check_row(budget, row, currency)?;
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
        self.check_overlap(budget)?;
        self.conn.execute(
            "INSERT INTO budgets (id, every, first_day, last_day, salary_day, name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                budget.id.to_bytes(),
                every_to_text(budget.every),
                date_to_text(budget.first_day)?,
                budget.last_day.map(date_to_text).transpose()?,
                budget.every.salary_day(),
                name,
            ],
        )?;
        for (position, category) in budget.categories.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO budget_categories (id, budget_id, category_id, position)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    uuid::Uuid::now_v7().into_bytes(),
                    budget.id.to_bytes(),
                    category.to_bytes(),
                    i64::try_from(position).unwrap_or(i64::MAX),
                ],
            )?;
        }
        for (position, account) in budget.accounts.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO budget_accounts (id, budget_id, account_id, position)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    uuid::Uuid::now_v7().into_bytes(),
                    budget.id.to_bytes(),
                    account.to_bytes(),
                    i64::try_from(position).unwrap_or(i64::MAX),
                ],
            )?;
        }
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

    /// Refuses `budget` if another budget of the book counts the same money
    /// on one of its days (D-344, D-639, [`Budget::clashes_with`]). Only the
    /// budgets that share a category with it are read, by the index.
    fn check_overlap(&self, budget: &Budget) -> Result<(), StoreError> {
        let mut sharing = self.conn.prepare_cached(
            "SELECT budget_id FROM budget_categories WHERE category_id = ?1 AND budget_id <> ?2",
        )?;
        let mut others: Vec<BudgetId> = Vec::new();
        for category in &budget.categories {
            let found = sharing
                .query_map(params![category.to_bytes(), budget.id.to_bytes()], |row| {
                    Ok(BudgetId::from_bytes(row.get(0)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for id in found {
                if !others.contains(&id) {
                    others.push(id);
                }
            }
        }
        for id in others {
            let other = self.budget_without_amounts(id)?;
            if let Some(category) = budget.clashes_with(&other) {
                return Err(StoreError::BudgetOverlap { category });
            }
        }
        Ok(())
    }

    /// The budget `id` with its days, categories and accounts, but none of
    /// its amounts.
    fn budget_without_amounts(&self, id: BudgetId) -> Result<Budget, StoreError> {
        let (every, first_day, last_day, salary_day, name) = self.conn.query_row(
            "SELECT every, first_day, last_day, salary_day, name FROM budgets WHERE id = ?1",
            [id.to_bytes()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )?;
        let ids = |sql: &str| -> Result<Vec<AccountId>, StoreError> {
            let mut statement = self.conn.prepare_cached(sql)?;
            let found = statement
                .query_map([id.to_bytes()], |row| {
                    Ok(AccountId::from_bytes(row.get(0)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(found)
        };
        Ok(Budget {
            id,
            name,
            categories: ids(
                "SELECT category_id FROM budget_categories WHERE budget_id = ?1 ORDER BY position",
            )?,
            accounts: ids(
                "SELECT account_id FROM budget_accounts WHERE budget_id = ?1 ORDER BY position",
            )?,
            every: every_from_text(&every, salary_day),
            first_day: date_from_text(&first_day)?,
            last_day: last_day.as_deref().map(date_from_text).transpose()?,
            amounts: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests;
