// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Budgets the AI prepares (M6c.11, D-358, D-394, D-395).
//!
//! A model names one or more spending categories, optionally the user's
//! accounts the budget counts from (D-641), how often the budget repeats and
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
    Account, AccountId, AccountKind, Budget, BudgetAmount, BudgetEvery, BudgetId, CurrencyCode,
    Date, Money,
};
use kassensturz_report::{BudgetGrouping, Filter, Period, ReportError, Source, budget_utilization};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::Object;
use super::transactions::parse_amount;
use crate::error::ToolError;

/// The most budgets one set may prepare, new and changed together (D-358).
pub(super) const MAX_BUDGETS: usize = 50;

/// The most categories, and the most accounts, one prepared budget names
/// (D-641); the book itself keeps up to 100 of each.
pub(super) const MAX_IDS: usize = 20;

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
    /// What the budget's categories spent in those periods, from its
    /// accounts if it has any, everything inside them counted once
    /// (D-291, D-641), exact.
    pub spent: Money,
    /// `spent` per period, rounded half away from zero to the currency's
    /// smallest unit: shown to the user, never stored.
    pub average: Money,
    /// The step the amount was rounded up to, in whole units of the
    /// currency (one of 1, 5, 10, 50, 100).
    pub round_to: u32,
    /// The newest full period, first and last day, when it had no spending
    /// and the count moved back one period past it (D-669); `None` when the
    /// periods right before the current one were counted.
    pub left_out: Option<(Date, Date)>,
}

/// A budget the AI prepares, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewBudget {
    /// The name it gets, as the book will keep it, or `None` (D-664).
    pub name: Option<String>,
    /// The spending categories, in the order the call names them.
    pub categories: Vec<AccountId>,
    /// Their paths, e.g. `["Food", "Groceries"]`, in the same order.
    pub paths: Vec<Vec<String>>,
    /// The user's own accounts it counts from, empty for all (D-641).
    pub accounts: Vec<AccountId>,
    /// Their names, in the same order.
    pub account_names: Vec<String>,
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
    /// The name it has now, or `None` (D-664).
    pub name_before: Option<String>,
    /// Its new name as the book will keep it (`Some(None)` takes the name
    /// away), or `None` when the name stays as it is (D-664).
    pub rename: Option<Option<String>>,
    /// Whether the amount or the carry over changes; `false` for a change
    /// of the name only, which then leaves every period's amount as it is.
    pub changes_amount: bool,
    /// Its spending categories.
    pub categories: Vec<AccountId>,
    /// Their paths, in the same order.
    pub paths: Vec<Vec<String>>,
    /// The accounts it counts from, empty for all.
    pub accounts: Vec<AccountId>,
    /// Their names, in the same order.
    pub account_names: Vec<String>,
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

/// The categories' paths in quotes, separated by commas, for the
/// developer mode's log: `"Home › Rent", "Electricity"`.
pub(super) fn paths_text(paths: &[Vec<String>]) -> String {
    paths
        .iter()
        .map(|path| format!("\"{}\"", path.join(" › ")))
        .collect::<Vec<_>>()
        .join(", ")
}

/// ` "Weekly shop"`, a budget's name in quotes after a space, or nothing for
/// none.
pub(super) fn name_text(name: Option<&str>) -> String {
    name.map(|name| format!(" \"{name}\"")).unwrap_or_default()
}

/// The name of an item (D-664): text, checked as the book checks a name
/// the user types, so the model hears a refusal at once. `None` when the
/// item names none; `Some(None)` for an empty one, which means no name.
fn name_of(item: &Object<'_>) -> Result<Option<Option<String>>, ToolError> {
    item.string("name")?
        .map(|name| {
            kassensturz_store::checked_budget_name(Some(name)).map_err(ToolError::BudgetRefused)
        })
        .transpose()
}

/// ` from "Shared account"`, or nothing for every account.
pub(super) fn accounts_text(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let quoted: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
    format!(" from {}", quoted.join(", "))
}

/// The word of `every` in a call.
pub fn every_word(every: BudgetEvery) -> &'static str {
    match every {
        // A salary month is a month that starts on its `salary_day` (D-651).
        BudgetEvery::Month | BudgetEvery::SalaryMonth { .. } => "month",
        BudgetEvery::Quarter => "quarter",
        BudgetEvery::Year => "year",
        BudgetEvery::Range => "range",
    }
}

/// The repeating kind of period a call names. A range is not offered: it
/// has days of its own that the model would have to pick (D-394).
/// A month may start on its `salary_day`, 1 being the calendar month
/// (D-651).
fn every_of(item: &Object<'_>) -> Result<BudgetEvery, ToolError> {
    let every = match item.required_string("every")? {
        "month" => BudgetEvery::Month,
        "quarter" => BudgetEvery::Quarter,
        "year" => BudgetEvery::Year,
        _ => return Err(ToolError::BadArgument { argument: "every" }),
    };
    let bad = ToolError::BadArgument {
        argument: "salary_day",
    };
    match item.get("salary_day") {
        None => Ok(every),
        Some(_) if every != BudgetEvery::Month => Err(bad),
        Some(value) => value
            .as_u64()
            .and_then(|day| i8::try_from(day).ok())
            .and_then(BudgetEvery::salary_month)
            .ok_or(bad),
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

/// Spending categories of the book, with their paths and their one
/// currency: the checks of the book's own budgets (D-639), so the model
/// hears them at once.
fn spending_categories(
    book: &Book,
    ids: &[AccountId],
) -> Result<(CurrencyCode, Vec<Vec<String>>), ToolError> {
    let choice = |reason| ToolError::BudgetRefused(StoreError::BudgetChoice { reason });
    let entries = book.categories()?;
    let mut currency = None;
    let mut paths = Vec::with_capacity(ids.len());
    for (index, &id) in ids.iter().enumerate() {
        if ids[..index].contains(&id) {
            return Err(choice("category-twice"));
        }
        let entry = entries
            .iter()
            .find(|entry| entry.category.id == id)
            .ok_or(ToolError::UnknownCategory)?;
        if entry.category.kind != AccountKind::Expense {
            return Err(ToolError::BudgetRefused(StoreError::BudgetNotSpending(id)));
        }
        match currency {
            None => currency = Some(entry.category.currency),
            Some(first) if first != entry.category.currency => return Err(choice("currencies")),
            Some(_) => {}
        }
        paths.push(entry.path.clone());
    }
    Ok((currency.ok_or(choice("no-category"))?, paths))
}

/// The user's own accounts of the book, with their names.
fn own_accounts(book: &Book, ids: &[AccountId]) -> Result<Vec<String>, ToolError> {
    let mut names = Vec::with_capacity(ids.len());
    for (index, &id) in ids.iter().enumerate() {
        if ids[..index].contains(&id) {
            return Err(ToolError::BudgetRefused(StoreError::BudgetChoice {
                reason: "account-twice",
            }));
        }
        let account: Account = match book.account(id) {
            Ok(account) => account,
            Err(StoreError::UnknownAccount(_)) => return Err(ToolError::UnknownAccount),
            Err(other) => return Err(other.into()),
        };
        if !matches!(account.kind, AccountKind::Asset | AccountKind::Liability) {
            return Err(ToolError::BudgetRefused(StoreError::BudgetChoice {
                reason: "not-own-account",
            }));
        }
        names.push(account.name);
    }
    Ok(names)
}

/// A list of IDs of `name`, at most [`MAX_IDS`]; not given is empty.
fn ids_of(item: &Object<'_>, name: &'static str) -> Result<Vec<AccountId>, ToolError> {
    let bad = ToolError::BadArgument { argument: name };
    let values = item.array(name)?;
    if values.len() > MAX_IDS {
        return Err(bad);
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .and_then(|text| uuid::Uuid::try_parse(text).ok())
                .map(|id| AccountId::from_bytes(id.into_bytes()))
                .ok_or(ToolError::BadArgument { argument: name })
        })
        .collect()
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
/// starts on `current`, for `categories` from `accounts` (none: every
/// account) in `currency`, rounded up to `round_to` whole
/// units (D-395, D-669).
///
/// The spending is counted as a budget counts it, by `budget_utilization`
/// with a budget that exists only here, so the figure is the same the
/// Budgets page would show for those periods. The rule: the total spent,
/// divided by the number of periods, rounded **up** to the next multiple of
/// the step (an exact division stays as it is).
///
/// **A payment that slipped past a period's end (D-669):** when the newest
/// of the full periods has no spending at all and the period before it has
/// some, the periods counted move back by one period, once: a payment
/// booked a few days late then sits in the current period, which is never
/// counted, and counting the empty period would make the amount too low
/// (`last_period` would be refused). The left-out period is kept in the
/// basis so the review card says so. Nothing moves when the period before
/// is empty too (the spending stopped), or when the book doesn't reach
/// back to the moved first period.
///
/// It is refused with `ai-tools.nothing-to-base-on` (M6-B210), with the
/// days looked at, when the book's first transaction is after the first
/// period counted (`book-too-short`: the average would be too low), and
/// when nothing was spent in them (`nothing-spent`).
fn amount_based_on(
    book: &Book,
    counts: &Counts<'_>,
    every: BudgetEvery,
    current: Date,
    periods: u8,
    round_to: u32,
) -> Result<(Money, AmountBasis), ToolError> {
    let bad = || ToolError::BadArgument {
        argument: "based_on",
    };
    // The full periods before the current one, newest first: one more than
    // asked for, the one a slipped payment moves the count back to.
    let mut starts = Vec::with_capacity(usize::from(periods) + 1);
    let mut first_day = current;
    for _ in 0..=periods {
        let before = first_day.yesterday().map_err(|_| bad())?;
        first_day = every.period_start(before).ok_or_else(bad)?;
        starts.push(first_day);
    }
    let earliest = book.booking_date_range()?.map(|(first, _)| first);
    let reaches = |start: Date| -> Result<bool, ToolError> {
        let end = every.period_end(start).ok_or_else(bad)?;
        Ok(earliest.is_some_and(|earliest| earliest <= end))
    };
    let asked = usize::from(periods);
    let last_day = current.yesterday().map_err(|_| bad())?;
    if !reaches(starts[asked - 1])? {
        // Not a wrong word: the book is too young for it (M6-B210).
        return Err(ToolError::NothingToBaseOn {
            reason: "book-too-short",
            from: starts[asked - 1],
            to: last_day,
        });
    }
    let reaches_moved = reaches(starts[asked])?;
    // Read from the oldest start the asked may need, up to the day before
    // the current period; a period before the book's first transaction is
    // read as nothing spent and only counted if the move is allowed.
    let read_from = if reaches_moved {
        starts[asked]
    } else {
        starts[asked - 1]
    };
    // Oldest first, as `budget_utilization` gives them.
    let spent_each = spent_per_period(book, counts, every, read_from, last_day)?;
    let read = if reaches_moved { asked + 1 } else { asked };
    if spent_each.len() != read {
        return Err(bad());
    }
    let newest_empty = spent_each[read - 1].is_zero();
    let before_newest_spent = read >= 2 && spent_each[read - 2].minor_units() > 0;
    let (counted, left_out) = if reaches_moved && newest_empty && before_newest_spent {
        let left_from = starts[0];
        (&spent_each[..asked], Some((left_from, last_day)))
    } else {
        (&spent_each[read - asked..], None)
    };
    let (first_day, last_day) = match left_out {
        Some((left_from, _)) => (starts[asked], left_from.yesterday().map_err(|_| bad())?),
        None => (starts[asked - 1], last_day),
    };
    let currency = counts.currency;
    let spent = Money::checked_sum(currency, counted.iter().copied()).map_err(|_| bad())?;
    let digits = book.currency_minor_digits(currency)?;
    let unit = 10_i128.checked_pow(u32::from(digits)).ok_or_else(bad)?;
    let step = unit * i128::from(round_to);
    let total = i128::from(spent.minor_units());
    let count = i128::from(periods);
    if total <= 0 {
        // Nothing spent in the periods looked at (M6-B210).
        return Err(ToolError::NothingToBaseOn {
            reason: "nothing-spent",
            from: first_day,
            to: last_day,
        });
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
            left_out,
        },
    ))
}

/// What `counts` spent in each full period of `every` from `first_day`
/// (a period's start) to `last_day` (a period's end), oldest first,
/// counted by `budget_utilization` for a budget that exists only here.
fn spent_per_period(
    book: &Book,
    counts: &Counts<'_>,
    every: BudgetEvery,
    first_day: Date,
    last_day: Date,
) -> Result<Vec<Money>, ToolError> {
    let probe = Budget {
        id: BudgetId::generate(),
        name: None,
        categories: counts.categories.to_vec(),
        accounts: counts.accounts.to_vec(),
        every,
        first_day,
        last_day: Some(last_day),
        amounts: vec![BudgetAmount {
            from_day: first_day,
            amount: Money::from_minor_units(1, counts.currency),
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
        None,
    )
    .map_err(from_report)?;
    Ok(rows
        .iter()
        .flat_map(|row| row.periods.iter().map(|shown| shown.spent))
        .collect())
}

/// What a budget counts: its categories and accounts, and their currency.
struct Counts<'a> {
    categories: &'a [AccountId],
    accounts: &'a [AccountId],
    currency: CurrencyCode,
}

/// The amount of an item: `amount` exactly as the user named it, or
/// computed from `based_on`; `None` when it names neither (allowed only in
/// an edit, which then keeps the amount).
fn amount_of(
    book: &Book,
    item: &Object<'_>,
    counts: &Counts<'_>,
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
            let digits = book.currency_minor_digits(counts.currency)?;
            let amount = parse_amount(text, counts.currency, digits)?;
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
            let (amount, basis) = amount_based_on(book, counts, every, current, periods, round_to)?;
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
/// the budgets the set already prepares (`prepared`, as the book will keep
/// them), on `today`. One category is `category_id`, several are
/// `category_ids` (D-641); `account_ids` limits it to the user's own
/// accounts.
pub(super) fn new_budget(
    book: &Book,
    value: &Value,
    today: Date,
    budgets: &[Budget],
    prepared: &mut Vec<Budget>,
) -> Result<NewBudget, ToolError> {
    let item = Object::read(
        value,
        &[
            "name",
            "category_id",
            "category_ids",
            "account_ids",
            "every",
            "salary_day",
            "amount",
            "based_on",
            "round_to",
            "carry_over",
        ],
        "new_budgets",
    )?;
    let one = item.id("category_id")?;
    let several = ids_of(&item, "category_ids")?;
    // The argument that names the categories, for a refusal.
    let named = if one.is_some() {
        "category_id"
    } else {
        "category_ids"
    };
    let categories = match (one, several.is_empty()) {
        (Some(id), true) => vec![id],
        (None, false) => several,
        (Some(_), false) => {
            return Err(ToolError::BadArgument {
                argument: "category_ids",
            });
        }
        (None, true) => {
            return Err(ToolError::BadArgument {
                argument: "category_id",
            });
        }
    };
    let name = name_of(&item)?.flatten();
    let accounts = ids_of(&item, "account_ids")?;
    let (currency, paths) = spending_categories(book, &categories)?;
    let account_names = own_accounts(book, &accounts)?;
    let every = every_of(&item)?;
    let (first_day, first_period_end) = current_period(every, today)?;
    // A category's money is in one budget at a time (D-344, D-639): this
    // one keeps going from the current period on.
    let probe = Budget {
        id: BudgetId::generate(),
        name: None,
        categories: categories.clone(),
        accounts: accounts.clone(),
        every,
        first_day,
        last_day: None,
        amounts: Vec::new(),
    };
    // Refused in the AI's own words, pointing to edit_budgets: the store's
    // code names the category only by its ID (M6-B216).
    if budgets
        .iter()
        .any(|budget| probe.clashes_with(budget).is_some())
    {
        return Err(ToolError::HasBudget);
    }
    if prepared
        .iter()
        .any(|budget| probe.clashes_with(budget).is_some())
    {
        return Err(ToolError::BadArgument { argument: named });
    }
    let counts = Counts {
        categories: &categories,
        accounts: &accounts,
        currency,
    };
    let (amount, basis) = amount_of(book, &item, &counts, every, first_day)?
        .ok_or(ToolError::BadArgument { argument: "amount" })?;
    check_amount(amount)?;
    prepared.push(probe);
    Ok(NewBudget {
        name,
        categories,
        paths,
        accounts,
        account_names,
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
        &[
            "budget_id",
            "name",
            "amount",
            "based_on",
            "round_to",
            "carry_over",
        ],
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
    let (currency, paths) = spending_categories(book, &budget.categories)?;
    let account_names = own_accounts(book, &budget.accounts)?;
    let counts = Counts {
        categories: &budget.categories,
        accounts: &budget.accounts,
        currency,
    };
    let carry_over = item.boolean("carry_over")?;
    let amount = amount_of(book, &item, &counts, budget.every, from_day)?;
    // A new name alone is a change too; the same name again is none.
    let rename = name_of(&item)?.filter(|name| *name != budget.name);
    let changes_amount = amount.is_some() || carry_over.is_some();
    if !changes_amount && rename.is_none() {
        return Err(ToolError::BadArgument {
            argument: if item.get("name").is_some() {
                "name"
            } else {
                "amount"
            },
        });
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
        name_before: budget.name.clone(),
        rename,
        changes_amount,
        categories: budget.categories.clone(),
        paths,
        accounts: budget.accounts.clone(),
        account_names,
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
    book.add_named_budget(
        new.name.as_deref(),
        &new.categories,
        &new.accounts,
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
    if edit.changes_amount {
        book.edit_budget_amount(edit.budget, edit.from_day, edit.amount, edit.carry_over)?;
    }
    if let Some(name) = &edit.rename {
        book.set_budget_name(edit.budget, name.as_deref())?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
