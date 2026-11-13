// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The figure a prepared key figure or progress bar shows, for the answer
//! of `propose_view` (US3.12, D-685).
//!
//! A model that prepares a number card can't see the card, and before this
//! it read the figure again with the read functions to tell the user what
//! it shows. Now the answer has it: KassenSturz works it out here with the
//! same `kassensturz-report` functions the card and the preview use, for
//! the chart exactly as it was prepared (its kind, data, key figure,
//! period, comparison, accounts and formula), and writes it as exact
//! decimal text. Every figure goes into the answer's figures, so the chat's
//! number check passes it (D-052).
//!
//! Charts with bars, lines, slices or a table get nothing here: they show
//! many figures, which the read functions give.

use kassensturz_core::ai_switch::{Permissions, Row};
use kassensturz_core::{CurrencyCode, Date, Money, Report};
use kassensturz_report::{
    ACCOUNT_BALANCES, Asked, CASH_FLOW_BY_MONTH, Compare, Component, ComponentType,
    FORMULA_MEASURE, Filter, Filters, Formula, FormulaValue, INCOME_EXPENSE_BY_MONTH, Level,
    Percent, Period, ReportError, SPENDING_BY_CATEGORY, Setting, Source, account_balances,
    cash_flow_by_month, income_expense_by_month, savings_rate, spending_by_category,
};
use kassensturz_store::Book;
use serde_json::{Map, Value, json};

use super::{Proposal, ProposedChange};
use crate::error::ToolError;
use crate::writer::{Writer, decimal_text};

/// What the answer says about the figures, so the model relays them and
/// never works out one of its own (AGENTS.md §8).
const COMPUTED_NOTE: &str = "computed is what the prepared chart shows, worked out by \
    KassenSturz for its period (from, to); for a formula that is the days it counted, which may be \
    a period its own names give (a formula whose names give different periods lists them in \
    periods instead of from and to); with a comparison, previous is the same figure in \
    the earlier period (previous_from, previous_to). Tell the user these figures as they are \
    and never work out one of your own. If the user changes the chart before saving it, the \
    saved chart shows the figure of the changed one. An empty list: nothing to count in the \
    period.";

/// A figure before it is written.
#[derive(Clone, Copy)]
enum Shown {
    Amount(Money),
    /// A percent; `None` for a savings rate without income.
    Percent(Option<Percent>),
    /// Any other number, in hundredths.
    Number(i64),
}

impl From<FormulaValue> for Shown {
    fn from(value: FormulaValue) -> Self {
        match value {
            FormulaValue::Amount(money) => Self::Amount(money),
            FormulaValue::Percent(percent) => Self::Percent(Some(percent)),
            FormulaValue::Number { hundredths } => Self::Number(hundredths),
        }
    }
}

impl Shown {
    /// `amount`, `percent` or `number`.
    const fn unit(self) -> &'static str {
        match self {
            Self::Amount(_) => "amount",
            Self::Percent(_) => "percent",
            Self::Number(_) => "number",
        }
    }

    /// The figure as exact decimal text, noted in the writer's figures: an
    /// amount with its currency's places, a percent to one place, any other
    /// number to two. `None` for a percent without a value.
    fn text(self, writer: &mut Writer<'_>) -> Result<Option<Value>, ToolError> {
        Ok(match self {
            Self::Amount(money) => Some(writer.amount(money)?),
            Self::Percent(Some(percent)) => {
                writer.figures.add_decimal(percent.tenths(), 1);
                Some(json!(decimal_text(percent.tenths(), 1)))
            }
            Self::Percent(None) => None,
            Self::Number(hundredths) => {
                writer.figures.add_decimal(hundredths, 2);
                Some(json!(decimal_text(hundredths, 2)))
            }
        })
    }
}

/// Puts the figure of the one chart `proposal` prepares into its answer,
/// when it is a key figure or a progress bar: `computed`, a list with one
/// entry per currency (one for a formula), and `computed_note`; or
/// `not_computed` with the error code when the figure can't be worked out
/// for the period (a formula that divides by zero), which the preview
/// tells the user too.
///
/// The figure is a figure of the book, so it reaches the model only when
/// the permission grid lets it read the same figure with a read function
/// (D-252): what you have, owe and are worth with the reading of the
/// accounts (`account_balance`), the money in and out of the accounts with
/// that of the accounts and the transactions, and everything else (income,
/// spending, a formula of categories) with that of the categories and the
/// transactions (`category_totals`). Otherwise `not_shown` names
/// `computed`, as `list_views` names what it leaves out.
pub(super) fn add_figure(
    book: &Book,
    proposal: &mut Proposal,
    allowed: Permissions,
    today: Date,
) -> Result<(), ToolError> {
    let (component, period) = match proposal.changes.changes() {
        [ProposedChange::NewView(new)] => (new.component(), new.period.as_str()),
        [ProposedChange::EditView(edit)] => (edit.component(), edit.period.as_str()),
        _ => return Ok(()),
    };
    if !matches!(
        component.kind(),
        ComponentType::KpiCard | ComponentType::ProgressBar
    ) {
        return Ok(());
    }
    if !readable(component, allowed) {
        if let Value::Object(value) = &mut proposal.output.value {
            value.insert("not_shown".into(), json!(["computed"]));
        }
        return Ok(());
    }
    let mut writer = Writer::new(book);
    let answer = match figures(book, component, period, today, &mut writer) {
        Ok(Some(entries)) => vec![
            ("computed", Value::Array(entries)),
            ("computed_note", json!(COMPUTED_NOTE)),
        ],
        Ok(None) => return Ok(()),
        Err(Failure::Tool(error)) => return Err(error),
        Err(Failure::Report(ReportError::Store(error))) => return Err(ToolError::Store(error)),
        Err(Failure::Report(error)) => vec![("not_computed", json!(error.report().code))],
    };
    if let Value::Object(value) = &mut proposal.output.value {
        for (key, entry) in answer {
            value.insert(key.into(), entry);
        }
    }
    proposal.output.figures.extend(&writer.figures);
    Ok(())
}

/// Whether `allowed` lets a model read the figure `component` shows with
/// a read function (see [`add_figure`]).
fn readable(component: &Component, allowed: Permissions) -> bool {
    let formula = matches!(component.get("measure"), Some(Setting::Word(word)) if word == FORMULA_MEASURE)
        || matches!(component.get("formula"), Some(Setting::Text(text)) if !text.trim().is_empty());
    let rows: &[Row] = match component.dataset().id {
        _ if formula => &[Row::Categories, Row::Transactions],
        ACCOUNT_BALANCES => &[Row::Accounts],
        CASH_FLOW_BY_MONTH => &[Row::Accounts, Row::Transactions],
        _ => &[Row::Categories, Row::Transactions],
    };
    rows.iter().all(|row| allowed.reads(*row))
}

/// Why a figure wasn't worked out.
enum Failure {
    /// The book couldn't be read, or an amount not written.
    Tool(ToolError),
    /// The figures of the period don't make one (a division by zero, two
    /// currencies in one formula).
    Report(ReportError),
}

impl From<ToolError> for Failure {
    fn from(error: ToolError) -> Self {
        Self::Tool(error)
    }
}

impl From<ReportError> for Failure {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

/// The period a saved chart's word names on `today`, as the chart counts
/// it: an open range starts with the book's first booking.
fn period_of(book: &Book, word: &str, today: Date) -> Result<Period, ReportError> {
    let earliest = || {
        book.booking_date_range()
            .ok()
            .flatten()
            .map(|(first, _)| first)
    };
    let earliest = if word.contains("..") {
        earliest()
    } else {
        None
    };
    Period::named(word, today, earliest).ok_or(ReportError::BadPeriod)?
}

/// The entries of `computed` for `component` over the period `word`;
/// `None` for a key figure this version can't work out.
fn figures(
    book: &Book,
    component: &Component,
    word: &str,
    today: Date,
    writer: &mut Writer<'_>,
) -> Result<Option<Vec<Value>>, Failure> {
    let period = period_of(book, word, today)?;
    let text = |key: &str| match component.get(key) {
        Some(Setting::Text(text)) if !text.trim().is_empty() => Some(text.as_str()),
        _ => None,
    };
    let measure = match component.kind() {
        // A bar without a formula shows the share of the income kept, as
        // the preview's bar does.
        ComponentType::ProgressBar if text("formula").is_none() => "savings-rate",
        ComponentType::ProgressBar => FORMULA_MEASURE,
        _ => match component.get("measure") {
            Some(Setting::Word(word)) => word.as_str(),
            _ => component.dataset().measures.first().copied().unwrap_or(""),
        },
    };
    let asked = Asked::new(component, None, &Filters::default());
    if measure == FORMULA_MEASURE {
        let Some(formula) = text("formula") else {
            return Ok(None);
        };
        let target = text("target").filter(|_| component.kind() == ComponentType::ProgressBar);
        let entry = formula_entry(book, asked.as_ref(), formula, target, period, writer)?;
        return Ok(Some(entry.into_iter().collect()));
    }
    let compare = match component.get("compare") {
        Some(Setting::Word(word)) => Compare::from_id(word),
        _ => None,
    };
    let earlier = compare.map(|compare| compare.earlier(period)).transpose()?;
    let span = match earlier {
        Some(earlier) => Period::new(earlier.first().min(period.first()), period.last())?,
        None => period,
    };
    let dataset = component.dataset().id;
    let source = if dataset == ACCOUNT_BALANCES || dataset == CASH_FLOW_BY_MONTH {
        Source::read_with_balances(book, span)?
    } else {
        Source::read(book, span)?
    };
    let Some(filter) = filter_of(asked.as_ref(), &source)? else {
        return Ok(Some(Vec::new()));
    };
    let Some(now) = by_currency(&source, dataset, measure, period, &filter)? else {
        return Ok(None);
    };
    let then = match earlier {
        Some(earlier) => {
            by_currency(&source, dataset, measure, earlier, &filter)?.unwrap_or_default()
        }
        None => Vec::new(),
    };
    let mut currencies: Vec<CurrencyCode> = now
        .iter()
        .chain(&then)
        .map(|(currency, _)| *currency)
        .collect();
    currencies.sort_unstable();
    currencies.dedup();
    // A currency only one of the periods has counts as zero in the other,
    // as on the card.
    let of = |sets: &[(CurrencyCode, Shown)], currency: CurrencyCode| {
        sets.iter().find(|(code, _)| *code == currency).map_or_else(
            || {
                if measure == "savings-rate" {
                    Shown::Percent(None)
                } else {
                    Shown::Amount(Money::zero(currency))
                }
            },
            |(_, shown)| *shown,
        )
    };
    let mut entries = Vec::with_capacity(currencies.len());
    for currency in currencies {
        let shown = of(&now, currency);
        let mut entry = Map::new();
        entry.insert("measure".into(), json!(measure));
        entry.insert("unit".into(), json!(shown.unit()));
        entry.insert("currency".into(), json!(currency.as_str()));
        put(&mut entry, "value", shown, writer)?;
        dates(&mut entry, "", period);
        if let Some(earlier) = earlier {
            put(&mut entry, "previous", of(&then, currency), writer)?;
            dates(&mut entry, "previous_", earlier);
        }
        entries.push(Value::Object(entry));
    }
    Ok(Some(entries))
}

/// The filter `asked` gives on `source`; `None` when it chooses nothing,
/// so the chart shows no figure.
fn filter_of(asked: Option<&Asked>, source: &Source) -> Result<Option<Filter>, ReportError> {
    let Some(asked) = asked else {
        return Ok(Some(Filter::default()));
    };
    let applied = asked.apply(source)?;
    Ok((!applied.chooses_nothing()).then_some(applied.filter))
}

/// Writes `shown` under `key`; a percent without a value gets `no_value`
/// (`no-income`: a savings rate needs income) in its place.
fn put(
    entry: &mut Map<String, Value>,
    key: &str,
    shown: Shown,
    writer: &mut Writer<'_>,
) -> Result<(), ToolError> {
    match shown.text(writer)? {
        Some(text) => entry.insert(key.into(), text),
        None => entry.insert(format!("{key}_missing"), json!("no-income")),
    };
    Ok(())
}

/// The figure `measure` of `dataset` over `period`, one per currency;
/// `None` for a measure this version doesn't work out.
fn by_currency(
    source: &Source,
    dataset: &str,
    measure: &str,
    period: Period,
    filter: &Filter,
) -> Result<Option<Vec<(CurrencyCode, Shown)>>, ReportError> {
    let amounts = |figures: Vec<(CurrencyCode, Option<Money>)>| {
        figures
            .into_iter()
            .map(|(currency, money)| money.map(|money| (currency, Shown::Amount(money))))
            .collect::<Option<Vec<_>>>()
    };
    Ok(match dataset {
        INCOME_EXPENSE_BY_MONTH => {
            let results = income_expense_by_month(source, period, filter)?;
            let mut shown = Vec::with_capacity(results.len());
            for result in &results {
                let figure = match measure {
                    "income" => Shown::Amount(result.income),
                    "expenses" => Shown::Amount(result.expenses),
                    "net" => Shown::Amount(result.net),
                    "savings-rate" => Shown::Percent(savings_rate(result.income, result.net)?),
                    _ => return Ok(None),
                };
                shown.push((result.currency, figure));
            }
            Some(shown)
        }
        ACCOUNT_BALANCES => amounts(
            account_balances(source, period, filter, None)?
                .results
                .iter()
                .map(|result| {
                    let worth = &result.worth;
                    let money = match measure {
                        "net_worth" => Some(worth.net_worth),
                        "assets" => Some(worth.assets),
                        "liabilities" => Some(worth.liabilities),
                        _ => None,
                    };
                    (worth.currency, money)
                })
                .collect(),
        ),
        CASH_FLOW_BY_MONTH => amounts(
            cash_flow_by_month(source, period, filter)?
                .iter()
                .map(|result| {
                    let total = &result.total;
                    let money = match measure {
                        "net_change" => Some(total.net_change),
                        "money_in" => Some(total.money_in),
                        "money_out" => Some(total.money_out),
                        _ => None,
                    };
                    (result.currency, money)
                })
                .collect(),
        ),
        SPENDING_BY_CATEGORY if measure == "total" => Some(
            spending_by_category(source, period, filter, Level::Each)?
                .iter()
                .map(|result| (result.currency, Shown::Amount(result.total)))
                .collect(),
        ),
        _ => None,
    })
}

/// The entry of a formula, with what a full bar stands for (`target`) and
/// how full it is (`share`, a percent) for a progress bar that has one;
/// `None` when its filter chooses nothing.
fn formula_entry(
    book: &Book,
    asked: Option<&Asked>,
    formula: &str,
    target: Option<&str>,
    period: Period,
    writer: &mut Writer<'_>,
) -> Result<Option<Value>, Failure> {
    let work = |text: &str| -> Result<Option<(FormulaValue, Vec<Period>)>, ReportError> {
        let formula = Formula::parse(text)?;
        let source = Source::read(book, formula.span(period)?)?;
        let Some(filter) = filter_of(asked, &source)? else {
            return Ok(None);
        };
        let currency = fallback_currency(book);
        let result = formula.evaluate(&source, period, &filter, currency)?;
        // The periods the formula really counted (#827), once each, in the
        // order it names them: a name may give itself a period of its own.
        let mut counted: Vec<Period> = Vec::new();
        for each in &result.counted {
            if !counted.contains(&each.period) {
                counted.push(each.period);
            }
        }
        Ok(Some((result.value, counted)))
    };
    let Some((value, mut counted)) = work(formula)? else {
        return Ok(None);
    };
    let shown = Shown::from(value);
    let mut entry = Map::new();
    entry.insert("measure".into(), json!(FORMULA_MEASURE));
    entry.insert("unit".into(), json!(shown.unit()));
    if let FormulaValue::Amount(money) = value {
        entry.insert("currency".into(), json!(money.currency().as_str()));
    }
    put(&mut entry, "value", shown, writer)?;
    let whole = match target {
        Some(target) => work(target)?,
        None => None,
    };
    if let Some((_, more)) = &whole {
        for each in more {
            if !counted.contains(each) {
                counted.push(*each);
            }
        }
    }
    // The label names the days the figure was counted for, not the chart's
    // period (#827). A formula without a name counts no days of its own.
    match counted.as_slice() {
        [] => dates(&mut entry, "", period),
        [only] => dates(&mut entry, "", *only),
        several => {
            let periods = several
                .iter()
                .map(|each| {
                    let mut one = Map::new();
                    dates(&mut one, "", *each);
                    Value::Object(one)
                })
                .collect();
            entry.insert("periods".into(), Value::Array(periods));
        }
    }
    if let Some((whole, _)) = whole {
        let fill = value.fill(whole)?;
        put(&mut entry, "target", Shown::from(whole), writer)?;
        put(&mut entry, "share", Shown::Percent(Some(fill)), writer)?;
    }
    Ok(Some(Value::Object(entry)))
}

/// The currency a formula without amounts is counted in: the book's first,
/// as the card counts it.
fn fallback_currency(book: &Book) -> CurrencyCode {
    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a currency code"),
    };
    book.currencies()
        .ok()
        .and_then(|used| used.first().map(|currency| currency.code()))
        .unwrap_or(EUR)
}

/// The first and last day of `period` under `from` and `to`, after
/// `prefix`.
fn dates(entry: &mut Map<String, Value>, prefix: &str, period: Period) {
    entry.insert(format!("{prefix}from"), json!(period.first().to_string()));
    entry.insert(format!("{prefix}to"), json!(period.last().to_string()));
}
