// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The planned payments and the forecast for a model to read (#920,
//! `docs/SCHEDULES.md`): `list_plans` and `forecast`.
//!
//! Both only read. A plan is made, changed, matched, booked and skipped
//! only by the user, in the window (AGENTS.md §3.1 rule 9, §8, D-749): no
//! change a model prepares reaches a plan. Every figure comes from the
//! store's exact forecast (D-052, D-770), written as decimal text; no IBAN
//! a plan remembers is ever in a result (T-23, D-186), and a plan's name
//! and the other party's name are text of the book, made harmless as every
//! other text is (D-171).

use std::collections::HashMap;

use kassensturz_core::{
    AccountId, CurrencyCode, Date, MAX_FORECAST_MONTHS, Schedule, ScheduleId, salary_month_of,
};
use kassensturz_store::{Book, Corruption, StoreError};
use serde_json::{Map, Value};

use crate::error::ToolError;
use crate::read::Names;
use crate::writer::Writer;

/// The most plans `list_plans` lists; a book holds at most
/// [`kassensturz_core::MAX_SCHEDULES`].
pub(crate) const MAX_PLANS: usize = 200;

/// The most payments `forecast` lists under "Did this happen?".
pub(crate) const MAX_OVERDUE: usize = 50;

/// How many months after the current one `forecast` shows when the call
/// doesn't say, as the Planned payments window does.
pub(crate) const DEFAULT_MONTHS: usize = 3;

/// `list_plans`: every plan the user made, in the order made,
/// with what it is for, its account, its exact amount (negative for money
/// out), how often it comes and when it is next due. Its category only
/// when the user lets the model read categories (`categories`).
pub(crate) fn list_plans(
    book: &Book,
    w: &mut Writer<'_>,
    today: Date,
    categories: bool,
) -> Result<Value, ToolError> {
    let paths: HashMap<AccountId, String> = if categories {
        book.categories()?
            .into_iter()
            .map(|entry| (entry.category.id, entry.path.join(" › ")))
            .collect()
    } else {
        HashMap::new()
    };
    let plans = book.schedules()?;
    let mut rows = Vec::new();
    for plan in plans.iter().take(MAX_PLANS) {
        let mut row = Map::new();
        row.insert("plan_id".into(), Value::String(plan.id.to_string()));
        row.insert("name".into(), w.text(&plan.name));
        row.insert("account_id".into(), Value::String(plan.account.to_string()));
        row.insert(
            "direction".into(),
            Value::from(if plan.is_outgoing() { "out" } else { "in" }),
        );
        row.insert("amount".into(), w.amount(plan.amount)?);
        row.insert("every".into(), Value::from(plan.every));
        row.insert("unit".into(), Value::from(plan.unit.word()));
        row.insert("first_due".into(), Value::String(plan.anchor.to_string()));
        if let Some(ends) = plan.ends_on {
            row.insert("ends".into(), Value::String(ends.to_string()));
        }
        if plan.paused {
            row.insert("paused".into(), Value::Bool(true));
        }
        if let Some(next) = book.next_due(plan.id, today)? {
            row.insert("next_due".into(), Value::String(next.date.to_string()));
        }
        // The other party only by name: an IBAN a plan remembers is never
        // in a result (T-23, D-186).
        if let Some(party) = &plan.party_name {
            let key = if plan.is_outgoing() {
                "paid_to"
            } else {
                "received_from"
            };
            row.insert(key.into(), w.text(party));
        }
        if let Some(category) = plan.category {
            if categories {
                row.insert("category_id".into(), Value::String(category.to_string()));
                let path = paths.get(&category).map(String::as_str).unwrap_or_default();
                row.insert("category".into(), w.text(path));
            } else {
                row.insert("category_not_shown".into(), Value::Bool(true));
            }
        }
        rows.push(Value::Object(row));
    }
    let mut result = Map::new();
    result.insert("today".into(), Value::String(today.to_string()));
    result.insert("plans".into(), Value::Array(rows));
    if plans.len() > MAX_PLANS {
        result.insert("plans_left_out".into(), w.count(plans.len() - MAX_PLANS));
    }
    Ok(Value::Object(result))
}

/// The currency most of `accounts` use, the open ones; a tie goes to the
/// first code in the alphabet. `None` with no open account.
fn forecast_currency(
    names: &Names,
    accounts: &[AccountId],
) -> Result<Option<CurrencyCode>, ToolError> {
    let mut counts: Vec<(CurrencyCode, usize)> = Vec::new();
    for &id in accounts {
        let account = names.own_account(id)?;
        if account.closed {
            continue;
        }
        match counts
            .iter_mut()
            .find(|(code, _)| *code == account.currency)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((account.currency, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.as_str().cmp(b.0.as_str())));
    Ok(counts.first().map(|(code, _)| *code))
}

/// `forecast`: *left this month* and the months after it (D-770) for the
/// user's `chosen` accounts, or, with none chosen, the open checking and
/// cash accounts the overview counts. Accounts in another currency than
/// most of them are left out and named, never converted (D-043).
///
/// Each month starts on day `starts_on` (1 is the calendar month), or,
/// when the call doesn't say, on the day the *Left this month* tile's
/// default "like my budgets" takes (#921, Q-565), so the model's "this
/// month" is the tile's.
pub(crate) fn forecast(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
    chosen: Vec<AccountId>,
    months: usize,
    starts_on: Option<i8>,
    today: Date,
) -> Result<Value, ToolError> {
    let starts_on = match starts_on {
        Some(day) => day,
        None => book.budgets_salary_day(today)?,
    };
    let chosen = if chosen.is_empty() {
        book.default_forecast_accounts()?
    } else {
        chosen
    };
    let mut result = Map::new();
    result.insert("today".into(), Value::String(today.to_string()));
    let Some(currency) = forecast_currency(names, &chosen)? else {
        result.insert("account_ids".into(), Value::Array(Vec::new()));
        result.insert("no_accounts".into(), Value::Bool(true));
        return Ok(Value::Object(result));
    };
    let months = months.min(MAX_FORECAST_MONTHS);
    let found = book.forecast_from_day(&chosen, currency, today, months, starts_on)?;
    let plans: HashMap<ScheduleId, Schedule> = book
        .schedules()?
        .into_iter()
        .map(|plan| (plan.id, plan))
        .collect();
    let ids = |list: &[AccountId]| {
        Value::Array(
            list.iter()
                .map(|id| Value::String(id.to_string()))
                .collect(),
        )
    };
    let figures = &found.forecast;
    result.insert("currency".into(), Value::from(currency.as_str()));
    // The store refused a day outside 1 to 31 above, as it does here.
    let (_, month_ends) = salary_month_of(today, starts_on)
        .ok_or_else(|| StoreError::Corrupt(Corruption::BadDate(today.to_string())))?;
    result.insert("month_starts_on".into(), Value::from(starts_on));
    result.insert("month_ends".into(), Value::String(month_ends.to_string()));
    result.insert("account_ids".into(), ids(&found.accounts));
    if !found.left_out.is_empty() {
        result.insert("left_out_account_ids".into(), ids(&found.left_out));
    }
    result.insert("has_plans".into(), Value::Bool(found.has_plans));
    result.insert("balance_now".into(), w.amount(figures.balance_now)?);
    result.insert("out_due".into(), w.amount(figures.out_due)?);
    result.insert("in_due".into(), w.amount(figures.in_due)?);
    result.insert("left".into(), w.amount(figures.left)?);
    result.insert("expected_end".into(), w.amount(figures.expected_end)?);
    let mut overdue = Vec::new();
    for occurrence in figures.overdue.iter().take(MAX_OVERDUE) {
        let Some(plan) = plans.get(&occurrence.schedule) else {
            return Err(StoreError::UnknownSchedule(occurrence.schedule).into());
        };
        let mut row = Map::new();
        row.insert("plan_id".into(), Value::String(plan.id.to_string()));
        row.insert("name".into(), w.text(&plan.name));
        row.insert("due".into(), Value::String(occurrence.due.date.to_string()));
        row.insert("amount".into(), w.amount(plan.amount)?);
        overdue.push(Value::Object(row));
    }
    result.insert("did_this_happen".into(), Value::Array(overdue));
    if figures.overdue.len() > MAX_OVERDUE {
        result.insert(
            "did_this_happen_left_out".into(),
            w.count(figures.overdue.len() - MAX_OVERDUE),
        );
    }
    let mut rows = Vec::new();
    for month in &figures.months {
        let mut row = Map::new();
        row.insert(
            "month".into(),
            Value::String(format!(
                "{:04}-{:02}",
                month.month.year(),
                month.month.month()
            )),
        );
        row.insert("first_day".into(), Value::String(month.month.to_string()));
        row.insert("last_day".into(), Value::String(month.last.to_string()));
        row.insert("start".into(), w.amount(month.start)?);
        row.insert("planned_in".into(), w.amount(month.planned_in)?);
        row.insert("planned_out".into(), w.amount(month.planned_out)?);
        row.insert("end".into(), w.amount(month.end)?);
        rows.push(Value::Object(row));
    }
    result.insert("months".into(), Value::Array(rows));
    Ok(Value::Object(result))
}
