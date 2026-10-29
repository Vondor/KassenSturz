// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Key figure cards of the open book (M6a.5, D-285): income, expenses, what
//! is left and the savings rate of a period, each against the period before
//! (or the same days a year earlier). No Qt in this file: `cards_model.rs`
//! hands the result to QML.
//!
//! Every figure is made by `kassensturz-report` and reaches QML as text
//! (D-065). The only numbers are the per-mille of the savings-rate progress
//! bar, which place a mark and are never read back.

use kassensturz_report::{
    Change, Compare, Filter, IncomeExpenseChange, Percent, Source, TextFormat,
    compare_income_expense, comparison_span,
};
use kassensturz_store::Book;

use crate::charts::{AppText, ChartsError, period};
use crate::format::Locale;

/// What the user asked for, by the fixed names QML uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// `this-month`, `last-month`, `last-12-months`, `this-year` or
    /// `last-year`.
    pub period: String,
    /// `previous_period` or `same_last_year` (the ids of `Compare`).
    pub compare: String,
}

/// Which way a figure moved against the earlier period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// More than before.
    Up,
    /// Less than before.
    Down,
    /// The same.
    Same,
    /// There is no earlier figure to compare with.
    None,
}

impl Direction {
    /// The fixed name QML switches on.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Same => "same",
            Self::None => "none",
        }
    }

    fn of(amount: i64) -> Self {
        match amount.cmp(&0) {
            core::cmp::Ordering::Greater => Self::Up,
            core::cmp::Ordering::Less => Self::Down,
            core::cmp::Ordering::Equal => Self::Same,
        }
    }
}

/// One card. Texts are empty where a card has nothing to say, e.g. the
/// change in percent when the earlier figure was zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    /// `income`, `expenses`, `net` or `savings-rate`.
    pub measure: &'static str,
    /// The currency code of the amounts; empty for no currency.
    pub currency: String,
    /// The figure; empty if it has no value (a savings rate without income).
    pub value: String,
    /// The same figure in the earlier period; empty if it has no value.
    pub previous: String,
    /// The change in the figure; empty for the savings rate.
    pub change: String,
    /// The change as a percentage of the earlier figure; empty if that was
    /// zero.
    pub change_percent: String,
    /// The way the figure moved.
    pub direction: Direction,
    /// For the savings rate: 0 to 1000, how much of the income is left, for
    /// a progress bar. `None` for the other cards and for no value.
    pub permille: Option<i32>,
}

/// The cards of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardSet {
    /// The currency code.
    pub currency: String,
    /// In the order income, expenses, net, savings rate.
    pub cards: Vec<Card>,
}

fn amount_card(
    measure: &'static str,
    change: &Change,
    currency: &str,
    text: &impl TextFormat,
) -> Card {
    Card {
        measure,
        currency: currency.to_owned(),
        value: text.amount(change.current),
        previous: text.amount(change.previous),
        change: text.amount(change.amount),
        change_percent: change.percent.map_or_else(String::new, |p| text.percent(p)),
        direction: Direction::of(change.amount.minor_units()),
        permille: None,
    }
}

fn rate_card(
    now: Option<Percent>,
    before: Option<Percent>,
    currency: &str,
    text: &impl TextFormat,
) -> Card {
    let shown = |rate: Option<Percent>| rate.map_or_else(String::new, |p| text.percent(p));
    let direction = match (now, before) {
        (Some(now), Some(before)) => Direction::of(now.tenths() - before.tenths()),
        _ => Direction::None,
    };
    Card {
        measure: "savings-rate",
        currency: currency.to_owned(),
        value: shown(now),
        previous: shown(before),
        change: String::new(),
        change_percent: String::new(),
        direction,
        // Tenths of a percent are per-mille; the bar only shows 0 to 100 %.
        permille: now.map(|p| i32::try_from(p.tenths().clamp(0, 1000)).unwrap_or(0)),
    }
}

fn set(change: &IncomeExpenseChange, text: &impl TextFormat) -> CardSet {
    let currency = change.currency.to_string();
    CardSet {
        cards: vec![
            amount_card("income", &change.income, &currency, text),
            amount_card("expenses", &change.expenses, &currency, text),
            amount_card("net", &change.net, &currency, text),
            rate_card(
                change.savings_rate,
                change.previous_savings_rate,
                &currency,
                text,
            ),
        ],
        currency,
    }
}

/// The cards of every currency the period or the one before has money in,
/// in the order of the currency codes. Empty means there was nothing to show.
pub fn cards(
    book: &Book,
    locale: &impl Locale,
    request: &Request,
) -> Result<Vec<CardSet>, ChartsError> {
    let period = period(&request.period, locale.today())?;
    let compare = Compare::from_id(&request.compare).ok_or(ChartsError::UnknownName("compare"))?;
    let source = Source::read(book, comparison_span(period, compare)?)?;
    let (_, changes) = compare_income_expense(&source, period, &Filter::default(), compare)?;
    let text = AppText::new(book, locale);
    Ok(changes.iter().map(|change| set(change, &text)).collect())
}
