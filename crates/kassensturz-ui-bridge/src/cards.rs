// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Key figure cards of the open book (M6a.5, D-285): income, expenses, what
//! is left and the savings rate of a period, each against the period before
//! (or the same days a year earlier), and since M6c.2 what you have, owe and
//! are worth at the end of a period (D-348). No Qt in this file:
//! `cards_model.rs` hands the result to QML.
//!
//! Every figure is made by `kassensturz-report` and reaches QML as text
//! (D-065). The only numbers are the per-mille of the savings-rate progress
//! bar, which place a mark and are never read back.

use kassensturz_core::{CurrencyCode, Money};
use kassensturz_report::{
    ACCOUNT_BALANCES, Change, Compare, Filter, INCOME_EXPENSE_BY_MONTH, IncomeExpenseChange,
    Percent, ReportError, Source, TextFormat, Worth, account_balances, compare_income_expense,
    comparison_span,
};
use kassensturz_store::Book;

use crate::charts::{AppText, ChartsError, period};
use crate::format::Locale;

/// What the user asked for, by the fixed names QML uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// The dataset the cards are made from: `income_expense_by_month` or
    /// `account_balances`. Empty means the first.
    pub dataset: String,
    /// `this-month`, `last-month`, `last-12-months`, `this-year`,
    /// `last-year` or `today`.
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

    fn of(order: core::cmp::Ordering) -> Self {
        match order {
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
    /// `income`, `expenses`, `net` or `savings-rate`; `net_worth`, `assets`
    /// or `liabilities` for the balances.
    pub measure: &'static str,
    /// The currency code of the amounts; empty for no currency.
    pub currency: String,
    /// The figure; empty if it has no value (a savings rate without income).
    pub value: String,
    /// The figure with all its decimal places, also when the user chose to
    /// see fewer (D-002); the same as `value` for the savings rate.
    pub exact_value: String,
    /// Whether the figure is below zero, e.g. more went out than came in.
    pub negative: bool,
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
    /// For what you are worth: the figure without its sign, so a screen can
    /// say "You owe" instead of a minus sign (D-067); empty for the others.
    pub magnitude: String,
    /// `magnitude` with all its decimal places (D-002).
    pub exact_magnitude: String,
}

/// The cards of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardSet {
    /// The currency code.
    pub currency: String,
    /// In the order income, expenses, net, savings rate.
    pub cards: Vec<Card>,
}

fn amount_card<L: Locale>(
    measure: &'static str,
    change: &Change,
    currency: &str,
    text: &AppText<'_, L>,
) -> Card {
    Card {
        measure,
        currency: currency.to_owned(),
        value: text.amount(change.current),
        exact_value: text.exact(change.current),
        negative: change.current.is_negative(),
        previous: text.amount(change.previous),
        change: text.amount(change.amount),
        change_percent: change.percent.map_or_else(String::new, |p| text.percent(p)),
        direction: Direction::of(change.amount.minor_units().cmp(&0)),
        permille: None,
        magnitude: String::new(),
        exact_magnitude: String::new(),
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
        (Some(now), Some(before)) => Direction::of(now.cmp(&before)),
        _ => Direction::None,
    };
    Card {
        measure: "savings-rate",
        currency: currency.to_owned(),
        value: shown(now),
        exact_value: shown(now),
        negative: now.is_some_and(|p| p.tenths() < 0),
        previous: shown(before),
        change: String::new(),
        change_percent: String::new(),
        direction,
        // Tenths of a percent are per-mille; the bar only shows 0 to 100 %.
        permille: now.map(|p| i32::try_from(p.tenths().clamp(0, 1000)).unwrap_or(0)),
        magnitude: String::new(),
        exact_magnitude: String::new(),
    }
}

fn set<L: Locale>(change: &IncomeExpenseChange, text: &AppText<'_, L>) -> CardSet {
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

/// The figures of `worth` for one card; nothing at all in `currency` is zero.
fn worth_of(worths: &[Worth], currency: CurrencyCode) -> Worth {
    worths
        .iter()
        .find(|worth| worth.currency == currency)
        .copied()
        .unwrap_or(Worth {
            currency,
            assets: Money::zero(currency),
            liabilities: Money::zero(currency),
            net_worth: Money::zero(currency),
        })
}

/// What you have, owe and are worth at the end of `period` against the end
/// of the period `compare` names, one set of cards per currency in either
/// period, in the order of the currency codes (D-348, D-370). `source` must
/// have been read with balances for both periods.
fn balance_cards<L: Locale>(
    source: &Source,
    period: kassensturz_report::Period,
    compare: Compare,
    text: &AppText<'_, L>,
) -> Result<Vec<CardSet>, ChartsError> {
    let earlier = compare.earlier(period)?;
    let worths = |period| -> Result<Vec<Worth>, ReportError> {
        Ok(account_balances(source, period, &Filter::default(), None)?
            .results
            .iter()
            .map(|result| result.worth)
            .collect())
    };
    let (now, then) = (worths(period)?, worths(earlier)?);
    let mut currencies: Vec<CurrencyCode> = now
        .iter()
        .chain(&then)
        .map(|worth| worth.currency)
        .collect();
    currencies.sort_unstable();
    currencies.dedup();
    let mut sets = Vec::with_capacity(currencies.len());
    for currency in currencies {
        let (now, then) = (worth_of(&now, currency), worth_of(&then, currency));
        let code = currency.to_string();
        let mut worth = amount_card(
            "net_worth",
            &Change::new(then.net_worth, now.net_worth)?,
            &code,
            text,
        );
        // A debt reads "You owe", not a minus sign (D-067): the screen
        // writes the words around the figure without its sign.
        if now.net_worth.is_negative() {
            let owed = Money::zero(currency)
                .checked_sub(now.net_worth)
                .map_err(|_| ReportError::Store(kassensturz_store::StoreError::AmountOverflow))?;
            worth.magnitude = text.amount(owed);
            worth.exact_magnitude = text.exact(owed);
        } else {
            worth.magnitude.clone_from(&worth.value);
            worth.exact_magnitude.clone_from(&worth.exact_value);
        }
        sets.push(CardSet {
            cards: vec![
                worth,
                amount_card(
                    "assets",
                    &Change::new(then.assets, now.assets)?,
                    &code,
                    text,
                ),
                amount_card(
                    "liabilities",
                    &Change::new(then.liabilities, now.liabilities)?,
                    &code,
                    text,
                ),
            ],
            currency: code,
        });
    }
    Ok(sets)
}

/// The cards of every currency the period or the one before has money in,
/// in the order of the currency codes. Empty means there was nothing to show.
pub fn cards(
    book: &Book,
    locale: &impl Locale,
    request: &Request,
) -> Result<Vec<CardSet>, ChartsError> {
    let period = period(book, &request.period, locale.today())?;
    let compare = Compare::from_id(&request.compare).ok_or(ChartsError::UnknownName("compare"))?;
    let span = comparison_span(period, compare)?;
    let text = AppText::new(book, locale);
    match request.dataset.as_str() {
        "" | INCOME_EXPENSE_BY_MONTH => {
            let source = Source::read(book, span)?;
            let (_, changes) =
                compare_income_expense(&source, period, &Filter::default(), compare)?;
            Ok(changes.iter().map(|change| set(change, &text)).collect())
        }
        ACCOUNT_BALANCES => {
            let source = Source::read_with_balances(book, span)?;
            balance_cards(&source, period, compare, &text)
        }
        _ => Err(ChartsError::UnknownName("dataset")),
    }
}
