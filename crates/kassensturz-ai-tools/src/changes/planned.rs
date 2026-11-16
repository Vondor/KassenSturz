// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Planned payments the AI prepares and changes (US7, B-357, D-814).
//!
//! A model may prepare the plans the *New planned payment* dialog makes, so
//! a user who asks it to plan the rent and the salary for the forecast
//! (D-770) gets them without typing each one: a name, the user's account,
//! whether the money goes out or comes in, the amount, how often (every so
//! many weeks, months or years), the first due day, and optionally the
//! last day, how much a payment may differ, the category a matching entry
//! gets, and the other party's name. It may also change a plan's name,
//! amount, how often it comes, its end, its category, how much a payment
//! may differ and whether it is paused.
//!
//! **The amount is never the model's own figure** (AGENTS.md §8, as for
//! budgets, D-358): it is either one the user named (`amount`) or the
//! amount an entry of the book was booked with
//! (`amount_from_transaction_id`), which KassenSturz reads itself.
//!
//! Each plan is checked as the book checks one made by hand
//! ([`Book::check_schedule`]), so the model hears the store's own refusal
//! at once. Nothing changes until the user confirms the set in the
//! window; the plans are then made or changed inside the set's one
//! `apply_ai_changes` entry, so one Undo takes the whole set back. A plan
//! changed or removed since it was reviewed makes the set out of date
//! (`ai-changes.out-of-date`), and nothing of it is applied.
//!
//! The IBAN a plan remembers never reaches the model (T-23, D-186): a
//! model names the other party only by name, and a change of a plan keeps
//! whatever party it remembers.

use std::collections::HashSet;

use kassensturz_core::{
    AccountId, Date, MAX_SCHEDULE_EVERY, MAX_SCHEDULES, Money, Schedule, ScheduleId, ScheduleUnit,
    TransactionId, due_dates, due_window_end,
};
use kassensturz_store::{Book, NewSchedule, StoreError};
use serde_json::Value;

use super::transactions::{free_text, parse_amount, parse_date};
use super::{ChangeError, Object};
use crate::error::ToolError;

/// The list of `propose_changes` with new plans.
pub(super) const NEW_LIST: &str = "new_plans";

/// The list of `propose_changes` with changed plans.
pub(super) const EDIT_LIST: &str = "edit_plans";

/// The most plans one set may make and change together, so the user can
/// still review them one by one.
pub(super) const MAX_PLANNED: usize = 20;

/// The keys an item of [`NEW_LIST`] takes.
const NEW_KEYS: &[&str] = &[
    "name",
    "account_id",
    "direction",
    "amount",
    "amount_from_transaction_id",
    "tolerance",
    "category_id",
    "every",
    "unit",
    "first_due",
    "ends",
    "paid_to",
    "received_from",
];

/// The keys an item of [`EDIT_LIST`] takes.
const EDIT_KEYS: &[&str] = &[
    "plan_id",
    "name",
    "amount",
    "amount_from_transaction_id",
    "tolerance",
    "category_id",
    "every",
    "unit",
    "ends",
    "paused",
];

/// A plan the AI prepares, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPlan {
    /// The plan as the book will make it.
    pub plan: NewSchedule,
    /// The name of its account.
    pub account_name: String,
    /// The path of its category, if it has one.
    pub category: Option<Vec<String>>,
    /// The entry its amount was taken from, if the model named one.
    pub amount_from: Option<TransactionId>,
}

/// A plan the AI changes, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditPlan {
    /// The plan as it is now, as the user reviewed it.
    pub before: Schedule,
    /// The plan as it will be.
    pub after: Schedule,
    /// The name of its account.
    pub account_name: String,
    /// The path of its category now, if it has one.
    pub category_before: Option<Vec<String>>,
    /// The path of its category after the change, if it has one.
    pub category_after: Option<Vec<String>>,
    /// The entry its new amount was taken from, if the model named one.
    pub amount_from: Option<TransactionId>,
}

impl NewPlan {
    /// The plan as a stored one would be, for its due dates.
    fn as_schedule(&self) -> Schedule {
        let plan = &self.plan;
        Schedule {
            id: ScheduleId::from_bytes([0; 16]),
            name: plan.name.clone(),
            account: plan.account,
            amount: plan.amount,
            tolerance: plan.tolerance,
            category: plan.category,
            party_iban: plan.party_iban.clone(),
            party_name: plan.party_name.clone(),
            unit: plan.unit,
            every: plan.every,
            anchor: plan.anchor,
            track_from: plan.track_from,
            ends_on: plan.ends_on,
            paused: false,
        }
    }

    /// Up to `count` due days on or after `from`, as the dialog shows the
    /// next three, so nobody has to work out "the 1st that is a Sunday".
    pub fn next_due(&self, from: Date, count: usize) -> Vec<Date> {
        next_due_of(&self.as_schedule(), from, count)
    }
}

impl EditPlan {
    /// Up to `count` due days of the changed plan on or after `from`.
    pub fn next_due(&self, from: Date, count: usize) -> Vec<Date> {
        next_due_of(&self.after, from, count)
    }
}

/// Up to `count` due days of `plan` on or after `from` (or its first due
/// day, if that is later), within the years [`due_dates`] looks at.
fn next_due_of(plan: &Schedule, from: Date, count: usize) -> Vec<Date> {
    let from = from.max(plan.anchor);
    due_dates(plan, from, due_window_end(from))
        .into_iter()
        .take(count)
        .map(|due| due.date)
        .collect()
}

/// The name of an item: one line of text, made harmless as every text the
/// model writes is (T-08); the book checks its length.
fn name_of(item: &Object<'_>) -> Result<Option<String>, ToolError> {
    match item.string("name")? {
        None => Ok(None),
        Some(text) => free_text(text, "name")?
            .map(Some)
            .ok_or(ToolError::BadArgument { argument: "name" }),
    }
}

/// A day of `argument`, `YYYY-MM-DD`.
fn day_of(item: &Object<'_>, argument: &'static str) -> Result<Option<Date>, ToolError> {
    item.string(argument)?
        .map(|text| parse_date(text).map_err(|_| ToolError::BadArgument { argument }))
        .transpose()
}

/// `every` of an item: a whole number from 1 to 52.
fn every_of(item: &Object<'_>) -> Result<Option<u8>, ToolError> {
    let bad = ToolError::BadArgument { argument: "every" };
    item.get("every")
        .map(|value| {
            value
                .as_u64()
                .and_then(|every| u8::try_from(every).ok())
                .filter(|every| (1..=MAX_SCHEDULE_EVERY).contains(every))
                .ok_or(bad)
        })
        .transpose()
}

/// `unit` of an item: week, month or year.
fn unit_of(item: &Object<'_>) -> Result<Option<ScheduleUnit>, ToolError> {
    item.string("unit")?
        .map(|word| {
            ScheduleUnit::from_word(word).ok_or(ToolError::BadArgument { argument: "unit" })
        })
        .transpose()
}

/// A size of money of `argument` in `account`'s currency: exact decimal
/// text with a dot, without a sign; `zero` allows 0.
fn size_of(
    book: &Book,
    item: &Object<'_>,
    argument: &'static str,
    currency: kassensturz_core::CurrencyCode,
    zero: bool,
) -> Result<Option<Money>, ToolError> {
    let Some(text) = item.string(argument)? else {
        return Ok(None);
    };
    let bad = || ToolError::BadArgument { argument };
    // A size, never a sign: whether the money goes out or comes in is the
    // plan's direction.
    if !text.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(bad());
    }
    let digits = book.currency_minor_digits(currency)?;
    let size = parse_amount(text, currency, digits).map_err(|_| bad())?;
    if size.is_negative() || (size.is_zero() && !zero) {
        return Err(bad());
    }
    Ok(Some(size))
}

/// The amount of an item, with the sign of `outgoing`: one the user named
/// (`amount`) or the amount the entry `amount_from_transaction_id` was
/// booked with in `account`, never both. `None` when it names neither.
fn amount_of(
    book: &Book,
    item: &Object<'_>,
    account: AccountId,
    currency: kassensturz_core::CurrencyCode,
    outgoing: bool,
) -> Result<Option<(Money, Option<TransactionId>)>, ToolError> {
    let named = size_of(book, item, "amount", currency, false)?;
    let from = item
        .uuid("amount_from_transaction_id")?
        .map(TransactionId::from_bytes);
    let signed = |size: Money| -> Result<Money, ToolError> {
        if outgoing {
            size.checked_neg()
                .map_err(|_| ToolError::BadArgument { argument: "amount" })
        } else {
            Ok(size)
        }
    };
    match (named, from) {
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(ToolError::BadArgument { argument: "amount" }),
        (Some(size), None) => Ok(Some((signed(size)?, None))),
        (None, Some(id)) => {
            let bad = || ToolError::BadArgument {
                argument: "amount_from_transaction_id",
            };
            // As "Plan this again…" reads an entry: its one part in the
            // account, in the account's currency, not zero.
            let entry = match book.plan_from_entry(id, account, "") {
                Err(StoreError::UnknownTransaction(_)) => {
                    return Err(ToolError::UnknownTransaction);
                }
                other => other?.ok_or_else(bad)?,
            };
            if entry.amount.currency() != currency || entry.amount.is_negative() != outgoing {
                return Err(bad());
            }
            Ok(Some((entry.amount, Some(id))))
        }
    }
}

/// The category of an item: an open category of the book in `currency`.
fn category_of(
    book: &Book,
    item: &Object<'_>,
    currency: kassensturz_core::CurrencyCode,
) -> Result<Option<AccountId>, ToolError> {
    let Some(id) = item.id("category_id")? else {
        return Ok(None);
    };
    let category = match book.account(id) {
        Ok(category) if category.kind.is_category() && !category.closed => category,
        Ok(_) | Err(StoreError::UnknownAccount(_)) => return Err(ToolError::UnknownCategory),
        Err(error) => return Err(error.into()),
    };
    if category.currency != currency {
        return Err(ToolError::OtherCurrency);
    }
    Ok(Some(id))
}

/// The path of `category`, e.g. `["Home", "Rent"]`.
fn path_of(book: &Book, category: Option<AccountId>) -> Result<Option<Vec<String>>, ToolError> {
    let Some(category) = category else {
        return Ok(None);
    };
    Ok(book
        .categories()?
        .into_iter()
        .find(|entry| entry.category.id == category)
        .map(|entry| entry.path))
}

/// The book's own refusal of `plan`, for the model (`store.schedule-…`).
fn check(book: &Book, plan: &Schedule) -> Result<(), ToolError> {
    match book.check_schedule(plan) {
        Ok(()) => Ok(()),
        Err(StoreError::UnknownAccount(_) | StoreError::NotAnAccount(_)) => {
            Err(ToolError::UnknownAccount)
        }
        Err(error @ StoreError::Database(_)) => Err(error.into()),
        Err(error) => Err(ToolError::PlanRefused(error)),
    }
}

/// Checks one item of [`NEW_LIST`] against `book`. `planned` counts the
/// plans the set makes so far, so the book never holds more than
/// [`MAX_SCHEDULES`].
pub(super) fn new_plan(
    book: &Book,
    value: &Value,
    planned: &mut usize,
) -> Result<NewPlan, ToolError> {
    let item = Object::read(value, NEW_KEYS, NEW_LIST)?;
    let name = name_of(&item)?.ok_or(ToolError::BadArgument { argument: "name" })?;
    let account_id = item.id("account_id")?.ok_or(ToolError::BadArgument {
        argument: "account_id",
    })?;
    let account = match book.account(account_id) {
        Ok(account) if account.kind.is_account() => account,
        Ok(_) | Err(StoreError::UnknownAccount(_)) => return Err(ToolError::UnknownAccount),
        Err(error) => return Err(error.into()),
    };
    let outgoing = match item.required_string("direction")? {
        "out" => true,
        "in" => false,
        _ => {
            return Err(ToolError::BadArgument {
                argument: "direction",
            });
        }
    };
    // The other party's name, under the key of its direction, as
    // `list_plans` writes it.
    let (party_key, wrong_key) = if outgoing {
        ("paid_to", "received_from")
    } else {
        ("received_from", "paid_to")
    };
    if item.get(wrong_key).is_some() {
        return Err(ToolError::BadArgument {
            argument: wrong_key,
        });
    }
    let party_name = item
        .string(party_key)?
        .map(|text| free_text(text, "payee"))
        .transpose()?
        .flatten();
    let (amount, amount_from) = amount_of(book, &item, account.id, account.currency, outgoing)?
        .ok_or(ToolError::BadArgument { argument: "amount" })?;
    let tolerance = size_of(book, &item, "tolerance", account.currency, true)?
        .unwrap_or_else(|| Money::zero(account.currency));
    let category = category_of(book, &item, account.currency)?;
    let unit = unit_of(&item)?.ok_or(ToolError::BadArgument { argument: "unit" })?;
    let every = every_of(&item)?.unwrap_or(1);
    let anchor = day_of(&item, "first_due")?.ok_or(ToolError::BadArgument {
        argument: "first_due",
    })?;
    let ends_on = day_of(&item, "ends")?;
    // As the dialog makes it: payments are counted from the first due day.
    let plan = NewSchedule {
        name,
        account: account.id,
        amount,
        tolerance,
        category,
        party_iban: None,
        party_name,
        unit,
        every,
        anchor,
        track_from: anchor,
        ends_on,
    };
    let new = NewPlan {
        account_name: account.name,
        category: path_of(book, category)?,
        amount_from,
        plan,
    };
    check(book, &new.as_schedule())?;
    let stored = book.schedules()?.len();
    if stored + *planned >= MAX_SCHEDULES {
        return Err(ToolError::PlanRefused(StoreError::TooManySchedules {
            max: MAX_SCHEDULES,
        }));
    }
    *planned += 1;
    Ok(new)
}

/// Checks one item of [`EDIT_LIST`] against `book`: a plan of the book,
/// at least one change, each plan once in the set (`seen`).
pub(super) fn edit_plan(
    book: &Book,
    value: &Value,
    seen: &mut HashSet<ScheduleId>,
) -> Result<EditPlan, ToolError> {
    let item = Object::read(value, EDIT_KEYS, EDIT_LIST)?;
    let id = item
        .uuid("plan_id")?
        .map(ScheduleId::from_bytes)
        .ok_or(ToolError::BadArgument {
            argument: "plan_id",
        })?;
    let before = match book.schedule(id) {
        Err(StoreError::UnknownSchedule(_)) => return Err(ToolError::UnknownPlan),
        other => other?,
    };
    let currency = before.amount.currency();
    let mut after = before.clone();
    if let Some(name) = name_of(&item)? {
        after.name = name;
    }
    let amount = amount_of(book, &item, before.account, currency, before.is_outgoing())?;
    let amount_from = amount.and_then(|(_, from)| from);
    if let Some((amount, _)) = amount {
        after.amount = amount;
    }
    if let Some(tolerance) = size_of(book, &item, "tolerance", currency, true)? {
        after.tolerance = tolerance;
    }
    if let Some(category) = category_of(book, &item, currency)? {
        after.category = Some(category);
    }
    if let Some(every) = every_of(&item)? {
        after.every = every;
    }
    if let Some(unit) = unit_of(&item)? {
        after.unit = unit;
    }
    // An empty text takes the end away; a day sets it.
    match item.string("ends")? {
        None => {}
        Some("") => after.ends_on = None,
        Some(_) => after.ends_on = day_of(&item, "ends")?,
    }
    if let Some(paused) = item.boolean("paused")? {
        after.paused = paused;
    }
    if after == before {
        return Err(ToolError::BadArgument {
            argument: "plan_id",
        });
    }
    check(book, &after)?;
    if !seen.insert(id) {
        return Err(ToolError::NotChangeable {
            argument: "plan_id",
            reason: "also-changed",
        });
    }
    let account_name = book.account(before.account)?.name;
    Ok(EditPlan {
        category_before: path_of(book, before.category)?,
        category_after: path_of(book, after.category)?,
        account_name,
        amount_from,
        before,
        after,
    })
}

/// Makes the plan the user reviewed, inside the set's user action.
pub(super) fn apply_new(book: &mut Book, new: &NewPlan) -> Result<(), ChangeError> {
    book.add_schedule(&new.plan)?;
    Ok(())
}

/// Changes the plan as the user reviewed it, inside the set's user action:
/// only if it is still as it was, otherwise nothing of the set at all.
pub(super) fn apply_edit(book: &mut Book, edit: &EditPlan) -> Result<(), ChangeError> {
    match book.schedule(edit.before.id) {
        Ok(stored) if stored == edit.before => {}
        Ok(_) | Err(StoreError::UnknownSchedule(_)) => return Err(ChangeError::OutOfDate),
        Err(error) => return Err(error.into()),
    }
    book.edit_schedule(&edit.after)?;
    Ok(())
}
