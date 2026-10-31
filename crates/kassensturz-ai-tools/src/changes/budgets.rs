// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Budgets the AI prepares (M6c.11, D-358, D-394, D-395).
//!
//! A model names a spending category, how often the budget repeats and
//! either an amount the user named in the chat or what the amount is
//! **based on**: the last full period, or the average of the last 3, 6 or
//! 12 full periods, rounded up to a step. With `based_on` the amount is
//! computed here, from the same counting `budget_utilization` uses for a
//! stored budget, never by the model (AGENTS.md §8), and the review page
//! says how it came about. An edit changes the amount or the carry over
//! from the current period on, so the past stays as it was (D-345). The
//! book changes only when the user confirms the set, through the store's
//! own budget operations and their checks.

use kassensturz_core::{
    Account, AccountId, AccountKind, Budget, BudgetAmount, BudgetEvery, BudgetId, Date, Money,
};
use kassensturz_report::{BudgetGrouping, Filter, Period, ReportError, Source, budget_utilization};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::Object;
use super::transactions::parse_amount;
use crate::error::ToolError;

/// The most budgets one set may prepare, new and changed together (D-358).
pub(super) const MAX_BUDGETS: usize = 50;

/// The steps an amount based on spending is rounded up to, in whole units
/// of the currency (D-358).
pub(super) const ROUND_TO: [u32; 5] = [1, 5, 10, 50, 100];

/// How the amount of a prepared budget came about, for the review page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmountBasis {
    /// How many full periods were counted: 1, 3, 6 or 12.
    pub periods: u8,
    /// The first day of the first period counted.
    pub first_day: Date,
    /// The last day of the last period counted, the day before the
    /// current period starts.
    pub last_day: Date,
    /// What the category spent in those periods, everything inside it
    /// counted once (D-291), exact.
    pub spent: Money,
    /// `spent` per period, rounded half away from zero to the currency's
    /// smallest unit: shown to the user, never stored.
    pub average: Money,
    /// The step the amount was rounded up to, in whole units of the
    /// currency (one of 1, 5, 10, 50, 100).
    pub round_to: u32,
}

/// A budget the AI prepares, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewBudget {
    /// The spending category.
    pub category: AccountId,
    /// Its path, e.g. `["Food", "Groceries"]`.
    pub path: Vec<String>,
    /// How often it repeats.
    pub every: BudgetEvery,
    /// The first day of its first period: the current one.
    pub first_day: Date,
    /// The last day of its first period.
    pub first_period_end: Date,
    /// What it plans for each period.
    pub amount: Money,
    /// Whether what is left (or overspent) moves into the next period.
    pub carry_over: bool,
    /// How the amount was computed, or `None` for one the user named.
    pub basis: Option<AmountBasis>,
}

/// A budget whose amount or carry over the AI changes from one period on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditBudget {
    /// The budget.
    pub budget: BudgetId,
    /// Its spending category.
    pub category: AccountId,
    /// The category's path.
    pub path: Vec<String>,
    /// How often it repeats.
    pub every: BudgetEvery,
    /// The first day of the period the change applies from: the current
    /// period, or the budget's first one if it starts later.
    pub from_day: Date,
    /// The last day of that period.
    pub from_period_end: Date,
    /// What applies in that period now.
    pub before: BudgetAmount,
    /// The amount from then on.
    pub amount: Money,
    /// The carry over from then on.
    pub carry_over: bool,
    /// How the amount was computed, or `None` for one the user named or
    /// one left as it is.
    pub basis: Option<AmountBasis>,
    /// The budget as it was checked, so a change in between is caught.
    pub(super) reviewed: Budget,
}

/// The word of `every` in a call.
pub fn every_word(every: BudgetEvery) -> &'static str {
    match every {
        BudgetEvery::Month => "month",
        BudgetEvery::Quarter => "quarter",
        BudgetEvery::Year => "year",
        BudgetEvery::Range => "range",
    }
}

/// The repeating kind of period a call names. A range is not offered: it
/// has days of its own that the model would have to pick (D-394).
fn every_of(item: &Object<'_>) -> Result<BudgetEvery, ToolError> {
    match item.required_string("every")? {
        "month" => Ok(BudgetEvery::Month),
        "quarter" => Ok(BudgetEvery::Quarter),
        "year" => Ok(BudgetEvery::Year),
        _ => Err(ToolError::BadArgument { argument: "every" }),
    }
}

/// How many full periods `based_on` counts.
fn periods_of(word: &str) -> Result<u8, ToolError> {
    match word {
        "last_period" => Ok(1),
        "average_3_periods" => Ok(3),
        "average_6_periods" => Ok(6),
        "average_12_periods" => Ok(12),
        _ => Err(ToolError::BadArgument {
            argument: "based_on",
        }),
    }
}

/// The step of `round_to`, 1 when the call names none. Only with
/// `based_on`: an amount the user named is kept exactly.
fn round_to_of(item: &Object<'_>, based: bool) -> Result<u32, ToolError> {
    let bad = ToolError::BadArgument {
        argument: "round_to",
    };
    match item.get("round_to") {
        None => Ok(1),
        Some(_) if !based => Err(bad),
        Some(value) => value
            .as_u64()
            .and_then(|step| u32::try_from(step).ok())
            .filter(|step| ROUND_TO.contains(step))
            .ok_or(bad),
    }
}

/// The current period of a repeating `every` on `today`.
fn current_period(every: BudgetEvery, today: Date) -> Result<(Date, Date), ToolError> {
    match (every.period_start(today), every.period_end(today)) {
        (Some(first), Some(last)) => Ok((first, last)),
        _ => Err(ToolError::BadArgument { argument: "every" }),
    }
}

/// A spending category of the book, with its path.
fn spending_category(book: &Book, id: AccountId) -> Result<(Account, Vec<String>), ToolError> {
    let entry = book
        .categories()?
        .into_iter()
        .find(|entry| entry.category.id == id)
        .ok_or(ToolError::UnknownCategory)?;
    if entry.category.kind != AccountKind::Expense {
        return Err(ToolError::BudgetRefused(StoreError::BudgetNotSpending(id)));
    }
    Ok((entry.category, entry.path))
}

/// What a report error means for a call: the book's own errors as they
/// are, anything else as periods the book can't count.
fn from_report(error: ReportError) -> ToolError {
    match error {
        ReportError::Store(error) => ToolError::Store(error),
        _ => ToolError::BadArgument {
            argument: "based_on",
        },
    }
}

/// The amount of `periods` full periods of `every` before the one that
/// starts on `current`, for `category`, rounded up to `round_to` whole
/// units (D-395).
///
/// The spending is counted as a budget counts it, by `budget_utilization`
/// with a budget that exists only here, so the figure is the same the
/// Budgets page would show for those periods. The rule: the total spent,
/// divided by the number of periods, rounded **up** to the next multiple of
/// the step (an exact division stays as it is). It is refused with
/// `based_on` when the book's first transaction is after the first period
/// counted (the book doesn't reach back that far, so the average would be
/// too low), and when nothing was spent in them.
fn amount_based_on(
    book: &Book,
    category: &Account,
    every: BudgetEvery,
    current: Date,
    periods: u8,
    round_to: u32,
) -> Result<(Money, AmountBasis), ToolError> {
    let bad = || ToolError::BadArgument {
        argument: "based_on",
    };
    let last_day = current.yesterday().map_err(|_| bad())?;
    let mut first_day = current;
    for _ in 0..periods {
        let before = first_day.yesterday().map_err(|_| bad())?;
        first_day = every.period_start(before).ok_or_else(bad)?;
    }
    let first_period_end = every.period_end(first_day).ok_or_else(bad)?;
    let earliest = book.booking_date_range()?.map(|(first, _)| first);
    if earliest.is_none_or(|earliest| earliest > first_period_end) {
        return Err(bad());
    }
    let currency = category.currency;
    let probe = Budget {
        id: BudgetId::generate(),
        category: category.id,
        every,
        first_day,
        last_day: Some(last_day),
        amounts: vec![BudgetAmount {
            from_day: first_day,
            amount: Money::from_minor_units(1, currency),
            carry_over: false,
        }],
    };
    let period = Period::new(first_day, last_day).map_err(from_report)?;
    let source = Source::read(book, period)
        .map_err(from_report)?
        .with_budgets(vec![probe]);
    let rows = budget_utilization(
        &source,
        period,
        last_day,
        BudgetGrouping::Period,
        &Filter::default(),
    )
    .map_err(from_report)?;
    let counted: Vec<Money> = rows
        .iter()
        .flat_map(|row| row.periods.iter().map(|shown| shown.spent))
        .collect();
    if counted.len() != usize::from(periods) {
        return Err(bad());
    }
    let spent = Money::checked_sum(currency, counted).map_err(|_| bad())?;
    let digits = book.currency_minor_digits(currency)?;
    let unit = 10_i128.checked_pow(u32::from(digits)).ok_or_else(bad)?;
    let step = unit * i128::from(round_to);
    let total = i128::from(spent.minor_units());
    let count = i128::from(periods);
    if total <= 0 {
        return Err(bad());
    }
    // Rounded up to the step: the smallest multiple of `step` that is at
    // least total / count.
    let amount = (total + count * step - 1) / (count * step) * step;
    // Shown only: half away from zero, total is above zero here.
    let average = (2 * total + count) / (2 * count);
    let money = |units: i128| {
        i64::try_from(units)
            .map(|units| Money::from_minor_units(units, currency))
            .map_err(|_| {
                ToolError::BudgetRefused(StoreError::BudgetAmount {
                    reason: "too-large",
                })
            })
    };
    Ok((
        money(amount)?,
        AmountBasis {
            periods,
            first_day,
            last_day,
            spent,
            average: money(average)?,
            round_to,
        },
    ))
}

/// The amount of an item: `amount` exactly as the user named it, or
/// computed from `based_on`; `None` when it names neither (allowed only in
/// an edit, which then keeps the amount).
fn amount_of(
    book: &Book,
    item: &Object<'_>,
    category: &Account,
    every: BudgetEvery,
    current: Date,
) -> Result<Option<(Money, Option<AmountBasis>)>, ToolError> {
    let named = item.string("amount")?;
    let based = item.string("based_on")?;
    let round_to = round_to_of(item, based.is_some())?;
    match (named, based) {
        (Some(_), Some(_)) => Err(ToolError::BadArgument {
            argument: "based_on",
        }),
        (Some(text), None) => {
            let digits = book.currency_minor_digits(category.currency)?;
            let amount = parse_amount(text, category.currency, digits)?;
            if amount.is_negative() {
                return Err(ToolError::BadArgument { argument: "amount" });
            }
            Ok(Some((amount, None)))
        }
        (None, Some(word)) => {
            if !every.repeats() {
                return Err(ToolError::BadArgument {
                    argument: "based_on",
                });
            }
            let periods = periods_of(word)?;
            let (amount, basis) =
                amount_based_on(book, category, every, current, periods, round_to)?;
            Ok(Some((amount, Some(basis))))
        }
        (None, None) => Ok(None),
    }
}

/// Refuses an amount the store would refuse (D-358): the same check as a
/// budget made by hand, so the model hears it at once.
fn check_amount(amount: Money) -> Result<(), ToolError> {
    if amount.minor_units() <= 0 {
        return Err(ToolError::BudgetRefused(StoreError::BudgetAmount {
            reason: "zero",
        }));
    }
    Ok(())
}

/// A new budget from an item of `new_budgets`, checked against `book` and
/// the budgets the set already prepares, on `today`.
pub(super) fn new_budget(
    book: &Book,
    value: &Value,
    today: Date,
    budgets: &[Budget],
    prepared: &mut Vec<AccountId>,
) -> Result<NewBudget, ToolError> {
    let item = Object::read(
        value,
        &[
            "category_id",
            "every",
            "amount",
            "based_on",
            "round_to",
            "carry_over",
        ],
        "new_budgets",
    )?;
    let id = item.id("category_id")?.ok_or(ToolError::BadArgument {
        argument: "category_id",
    })?;
    let (category, path) = spending_category(book, id)?;
    let every = every_of(&item)?;
    let (first_day, first_period_end) = current_period(every, today)?;
    // One budget per category at a time (D-344): this one keeps going
    // from the current period on.
    let overlaps = budgets.iter().any(|budget| {
        budget.category == id && budget.last_day.is_none_or(|last| last >= first_day)
    });
    if overlaps {
        return Err(ToolError::BudgetRefused(StoreError::BudgetOverlap {
            category: id,
        }));
    }
    if prepared.contains(&id) {
        return Err(ToolError::BadArgument {
            argument: "category_id",
        });
    }
    let (amount, basis) = amount_of(book, &item, &category, every, first_day)?
        .ok_or(ToolError::BadArgument { argument: "amount" })?;
    check_amount(amount)?;
    prepared.push(id);
    Ok(NewBudget {
        category: id,
        path,
        every,
        first_day,
        first_period_end,
        amount,
        carry_over: item.boolean("carry_over")?.unwrap_or(false),
        basis,
    })
}

/// A change of a budget from an item of `edit_budgets`, from the current
/// period on (D-358): the past stays.
pub(super) fn edit_budget(
    book: &Book,
    value: &Value,
    today: Date,
    budgets: &[Budget],
    edited: &mut Vec<BudgetId>,
) -> Result<EditBudget, ToolError> {
    let item = Object::read(
        value,
        &["budget_id", "amount", "based_on", "round_to", "carry_over"],
        "edit_budgets",
    )?;
    let id = item
        .uuid("budget_id")?
        .map(BudgetId::from_bytes)
        .ok_or(ToolError::BadArgument {
            argument: "budget_id",
        })?;
    let budget = budgets
        .iter()
        .find(|budget| budget.id == id)
        .cloned()
        .ok_or(ToolError::BudgetRefused(StoreError::UnknownBudget(id)))?;
    if edited.contains(&id) {
        return Err(ToolError::BadArgument {
            argument: "budget_id",
        });
    }
    // From the period that contains today, or the first one if the budget
    // starts later; an ended budget has no period left to change.
    let day = today.max(budget.first_day);
    let (from_day, from_period_end) =
        budget
            .period_containing(day)
            .ok_or(ToolError::BudgetRefused(StoreError::BudgetPeriod {
                reason: "amount-after-end",
            }))?;
    let before = *budget
        .amount_for(from_day)
        .ok_or(ToolError::Store(StoreError::UnknownBudget(id)))?;
    let (category, path) = spending_category(book, budget.category)?;
    let carry_over = item.boolean("carry_over")?;
    let amount = amount_of(book, &item, &category, budget.every, from_day)?;
    if amount.is_none() && carry_over.is_none() {
        return Err(ToolError::BadArgument { argument: "amount" });
    }
    let (amount, basis) = amount.unwrap_or((before.amount, None));
    check_amount(amount)?;
    let carry_over = carry_over.unwrap_or(before.carry_over);
    if carry_over && !budget.every.repeats() {
        return Err(ToolError::BudgetRefused(StoreError::BudgetPeriod {
            reason: "carry-over-on-range",
        }));
    }
    edited.push(id);
    Ok(EditBudget {
        budget: id,
        category: budget.category,
        path,
        every: budget.every,
        from_day,
        from_period_end,
        before,
        amount,
        carry_over,
        basis,
        reviewed: budget,
    })
}

/// Makes the new budget in `book`, with the store's checks.
pub(super) fn apply_new_budget(book: &mut Book, new: &NewBudget) -> Result<(), StoreError> {
    book.add_budget(
        new.category,
        new.every,
        new.first_day,
        None,
        new.amount,
        new.carry_over,
    )
    .map(|_| ())
}

/// Changes the budget in `book` from its period on, if it is still as the
/// user reviewed it.
pub(super) fn apply_edit_budget(
    book: &mut Book,
    edit: &EditBudget,
) -> Result<(), super::ChangeError> {
    match book.budget(edit.budget) {
        Ok(stored) if stored == edit.reviewed => {}
        Ok(_) | Err(StoreError::UnknownBudget(_)) => return Err(super::ChangeError::OutOfDate),
        Err(other) => return Err(other.into()),
    }
    book.edit_budget_amount(edit.budget, edit.from_day, edit.amount, edit.carry_over)?;
    Ok(())
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
