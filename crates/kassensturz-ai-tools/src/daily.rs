// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daily_balances` (US7, #1036, D-794, T-23): one account's balance at the
//! end of each day of a period, with its two exponential averages, MACD,
//! the signal line and the histogram, all from
//! [`kassensturz_report::daily_balance`], the figures the daily balance
//! chart draws. The model quotes them; it never works them out (D-052).

use kassensturz_core::{AccountId, Date};
use kassensturz_report::{
    Ema, Filter, Period as Days, ReportError, Source, daily_balance, daily_days,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

use crate::error::ToolError;
use crate::read::{Names, Period};
use crate::writer::Writer;

/// The most days one call answers: half a year and a few days more, so
/// "the last 6 months" fits in one call and the result stays small enough
/// for a small model.
pub(crate) const MAX_DAILY_ROWS: usize = 200;

/// The shortest and longest length each average may span, in days.
pub(crate) const EMA_DAYS: std::ops::RangeInclusive<u32> = 2..=200;

/// The columns of each row, in order.
const COLUMNS: [&str; 7] = [
    "day",
    "balance",
    "average_short",
    "average_long",
    "macd",
    "signal",
    "histogram",
];

/// The daily balance of `account` from `period.from` to `period.to` (up to
/// today), averaged over `lengths`.
pub(crate) fn daily_balances(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
    account: AccountId,
    period: Period,
    lengths: Ema,
    today: Date,
) -> Result<Value, ToolError> {
    let own = names.own_account(account)?;
    let asked = Days::new(period.from, period.to).map_err(|_| ToolError::BadPeriod)?;
    let shown = daily_days(asked, today).map_err(too_long)?;
    let mut result = json!({
        "account": w.text(&own.name),
        "currency": own.currency.as_str(),
        "from": period.from.to_string(),
        "to": period.to.to_string(),
        "lengths_in_days": {
            "average_short": lengths.fast,
            "average_long": lengths.slow,
            "signal": lengths.signal,
        },
        "columns": COLUMNS,
    });
    let Some(shown) = shown else {
        // The whole period lies after today: no day has a balance yet.
        result["rows"] = json!([]);
        result["after_today"] = Value::Bool(true);
        return Ok(result);
    };
    let mut days = 1;
    let mut day = shown.first();
    while day < shown.last() && days <= MAX_DAILY_ROWS {
        day = day.tomorrow().map_err(|_| ToolError::BadPeriod)?;
        days += 1;
    }
    if days > MAX_DAILY_ROWS {
        return Err(ToolError::BadArgument { argument: "to" });
    }
    let source = Source::read_with_balances(book, lengths.read_span(shown).map_err(too_long)?)
        .map_err(too_long)?;
    let filter = Filter {
        accounts: Some(vec![account]),
        categories: None,
        tags: None,
    };
    let data = daily_balance(&source, asked, today, &filter, lengths).map_err(too_long)?;
    let mut rows = Vec::new();
    let mut starts_late = false;
    for currency in data {
        starts_late |= currency.starts_late;
        for row in currency.rows {
            rows.push(json!([
                row.day.to_string(),
                w.amount(row.balance)?,
                w.amount(row.fast)?,
                w.amount(row.slow)?,
                w.amount(row.macd)?,
                w.amount(row.signal)?,
                w.amount(row.gap)?,
            ]));
        }
    }
    if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
        result["first_day"] = first[0].clone();
        result["last_day"] = last[0].clone();
    }
    if starts_late {
        // The account's first entry is inside the period: no balance before.
        result["starts_on_first_entry"] = Value::Bool(true);
    }
    result["rows"] = Value::Array(rows);
    Ok(result)
}

/// What a report error means for a call: the book's own errors as they
/// are, a period the book can't count as a period that is too long.
fn too_long(error: ReportError) -> ToolError {
    match error {
        ReportError::Store(error) => ToolError::Store(error),
        _ => ToolError::BadArgument { argument: "to" },
    }
}
