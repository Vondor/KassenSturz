// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Works a formula out from the reporting layer's datasets (D-292).
//!
//! The names are looked up first, then each name's figure is read from
//! [`category_over_time`] (one call per period, all names of that period
//! together), then the tree is worked out with exact values. A division by
//! zero has no value and a figure too large to work out exactly is refused;
//! both end at the first `IFERROR` around them, or make the formula's
//! result an error.

use std::collections::{HashMap, HashSet};

use jiff::ToSpan;
use kassensturz_core::{AccountId, CurrencyCode, GroupMember, Money, RoundingMode};

use super::exact::{Exact, TooLarge};
use super::lex::Comparison;
use super::parse::{Expr, Function, Kind, Name, Node, Operator, Range, Tree, Words};
use super::{Counted, FormulaResult, FormulaValue};
use crate::{Filter, Overlap, Percent, Period, ReportError, Source, category_over_time};

/// The period a name counts: the figure's own, the words counted from the
/// figure's period, or the dates it gives. The words count from the
/// figure's last day: on a figure for March 2026 `"last month"` is
/// February 2026 (D-318).
pub(crate) fn period_of(range: Option<Range>, figure: Period) -> Result<Period, ReportError> {
    let bad = |_| ReportError::BadPeriod;
    let day = figure.last();
    let quarter = || {
        let first = day
            .first_of_month()
            .checked_sub(i32::from((day.month() - 1) % 3).months())
            .map_err(bad)?;
        Ok::<_, ReportError>(first)
    };
    match range {
        None => Ok(figure),
        Some(Range::Dates(period)) => Ok(period),
        Some(Range::Words(words)) => match words {
            Words::ThisMonth => Period::this_month(day),
            Words::LastMonth => Period::last_month(day),
            Words::ThisQuarter => {
                let first = quarter()?;
                let last = first.checked_add(2.months()).map_err(bad)?.last_of_month();
                Period::new(first, last)
            }
            Words::LastQuarter => {
                let first = quarter()?.checked_sub(3.months()).map_err(bad)?;
                let last = first.checked_add(2.months()).map_err(bad)?.last_of_month();
                Period::new(first, last)
            }
            Words::ThisYear => Period::this_year(day),
            Words::LastYear => Period::last_year(day),
            Words::Last12Months => Period::last_12_months(day),
        },
    }
}

/// The category or group `text` names: a group's name, a category's path
/// written with `>` (its last parts are enough), or a category's name,
/// ignoring capitals. Two that fit are refused with the paths to write
/// instead, never guessed (T-42).
fn resolve(source: &Source, name: &Name) -> Result<GroupMember, ReportError> {
    let whole = name.text.trim().to_lowercase();
    let parts: Vec<String> = whole
        .split('>')
        .map(|part| part.trim().to_owned())
        .collect();
    let mut found: Vec<(GroupMember, String)> = Vec::new();
    for group in source.groups() {
        if group.name.to_lowercase() == whole {
            found.push((GroupMember::Group(group.id), group.name.clone()));
        }
    }
    for entry in source.tree() {
        let path: Vec<String> = entry.path.iter().map(|part| part.to_lowercase()).collect();
        let by_path = parts.len() > 1 && path.ends_with(&parts);
        let by_name = path.last() == Some(&whole);
        if by_path || by_name {
            found.push((
                GroupMember::Category(entry.category.id),
                entry.path.join(" > "),
            ));
        }
    }
    match found.as_slice() {
        [] => Err(ReportError::FormulaUnknownName {
            position: name.at,
            name: name.text.clone(),
        }),
        [(member, _)] => Ok(*member),
        _ => Err(ReportError::FormulaAmbiguousName {
            position: name.at,
            name: name.text.clone(),
            paths: found
                .iter()
                .map(|(_, path)| format!("'{}'", path.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(", "),
        }),
    }
}

/// Why a part has no figure.
enum Fail {
    /// A division by zero.
    NoValue(usize),
    /// Too large to work out exactly.
    TooLarge(usize),
    /// A mistake `IFERROR` doesn't hide.
    Mistake(ReportError),
}

/// A worked-out part: an amount or a number (the kinds were checked
/// before), or a yes/no.
#[derive(Clone, Copy)]
enum Value {
    Figure(Exact),
    YesNo(bool),
}

impl Value {
    fn figure(self) -> Exact {
        match self {
            Self::Figure(value) => value,
            // The kinds were checked before anything was read.
            Self::YesNo(yes) => Exact::whole(i64::from(yes)),
        }
    }

    fn yes(self) -> bool {
        match self {
            Self::YesNo(yes) => yes,
            Self::Figure(value) => !value.is_zero(),
        }
    }
}

struct Evaluator {
    /// Each name's figure, in the formula's currency.
    figures: Vec<Exact>,
    /// Each name's period, and the categories it counts if it is in
    /// `COUNT`.
    counts: Vec<(Period, HashSet<AccountId>)>,
    /// Per period of a name in `COUNT`: each transaction's categories.
    transactions: HashMap<Period, Vec<(usize, Option<AccountId>)>>,
}

impl Evaluator {
    fn eval(&self, expr: &Expr) -> Result<Value, Fail> {
        let large = |_: TooLarge| Fail::TooLarge(expr.at);
        let figure = |expr| self.eval(expr).map(Value::figure);
        Ok(match &expr.node {
            Node::Number(value) => Value::Figure(*value),
            Node::Name(index) => Value::Figure(self.figures[*index]),
            Node::Negate(inside) => Value::Figure(figure(inside)?.neg().map_err(large)?),
            Node::Percent(inside) => Value::Figure(figure(inside)?.percent().map_err(large)?),
            Node::Binary(operator, left, right) => {
                let (left, right) = (figure(left)?, figure(right)?);
                Value::Figure(
                    match operator {
                        Operator::Add => left.add(right),
                        Operator::Subtract => left.sub(right),
                        Operator::Multiply => left.mul(right),
                        Operator::Divide => match left.div(right).map_err(large)? {
                            Some(value) => Ok(value),
                            None => return Err(Fail::NoValue(expr.at)),
                        },
                    }
                    .map_err(large)?,
                )
            }
            Node::Compare(comparison, left, right) => {
                let order = figure(left)?.cmp(&figure(right)?);
                Value::YesNo(match comparison {
                    Comparison::Equal => order.is_eq(),
                    Comparison::NotEqual => order.is_ne(),
                    Comparison::Less => order.is_lt(),
                    Comparison::Greater => order.is_gt(),
                    Comparison::LessOrEqual => order.is_le(),
                    Comparison::GreaterOrEqual => order.is_ge(),
                })
            }
            Node::Call(function, arguments) => self.call(*function, arguments, expr.at)?,
        })
    }

    fn call(&self, function: Function, arguments: &[Expr], at: usize) -> Result<Value, Fail> {
        let large = |_: TooLarge| Fail::TooLarge(at);
        let figure = |expr| self.eval(expr).map(Value::figure);
        let figures = || arguments.iter().map(figure).collect::<Result<Vec<_>, _>>();
        let ratio =
            |part: Exact, whole: Exact| part.div(whole).map_err(large)?.ok_or(Fail::NoValue(at));
        Ok(match function {
            Function::Sum | Function::Average => {
                let values = figures()?;
                let mut sum = Exact::ZERO;
                for value in &values {
                    sum = sum.add(*value).map_err(large)?;
                }
                if function == Function::Average {
                    // At least one argument; at most MAX_FORMULA_ARGUMENTS.
                    sum = ratio(sum, Exact::whole(values.len() as i64))?;
                }
                Value::Figure(sum)
            }
            Function::Min | Function::Max => {
                let values = figures()?;
                let chosen = if function == Function::Min {
                    values.into_iter().min()
                } else {
                    values.into_iter().max()
                };
                Value::Figure(chosen.unwrap_or(Exact::ZERO))
            }
            Function::Count => {
                let mut counted = HashSet::new();
                for argument in arguments {
                    let Node::Name(index) = argument.node else {
                        continue;
                    };
                    let (period, categories) = &self.counts[index];
                    for &(transaction, category) in &self.transactions[period] {
                        if category.is_some_and(|category| categories.contains(&category)) {
                            counted.insert(transaction);
                        }
                    }
                }
                Value::Figure(Exact::whole(counted.len() as i64))
            }
            Function::Abs => Value::Figure(figure(&arguments[0])?.abs().map_err(large)?),
            Function::Sign => Value::Figure(Exact::whole(figure(&arguments[0])?.signum())),
            Function::Int => Value::Figure(
                figure(&arguments[0])?
                    .round(0, RoundingMode::Floor)
                    .map_err(large)?,
            ),
            Function::Round | Function::RoundUp | Function::RoundDown => {
                let value = figure(&arguments[0])?;
                let places = match arguments.get(1) {
                    None => 0,
                    Some(places) => figure(places)?
                        .as_whole()
                        .filter(|places| (-9..=10).contains(places))
                        .ok_or_else(|| {
                            Fail::Mistake(ReportError::FormulaArguments {
                                position: places.at,
                                function: function.name(),
                                reason: "places",
                            })
                        })?,
                };
                let mode = match function {
                    Function::RoundUp => RoundingMode::AwayFromZero,
                    Function::RoundDown => RoundingMode::TowardZero,
                    _ => RoundingMode::HalfAwayFromZero,
                };
                // Within -9..=10.
                Value::Figure(value.round(places as i32, mode).map_err(large)?)
            }
            Function::If => {
                if self.eval(&arguments[0])?.yes() {
                    self.eval(&arguments[1])?
                } else {
                    self.eval(&arguments[2])?
                }
            }
            Function::IfError => match self.eval(&arguments[0]) {
                Err(Fail::NoValue(_) | Fail::TooLarge(_)) => self.eval(&arguments[1])?,
                other => other?,
            },
            Function::And | Function::Or => {
                // Every argument is worked out, as spreadsheets do, so a
                // mistake in a later one isn't hidden by an earlier answer.
                let mut answers = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    answers.push(self.eval(argument)?.yes());
                }
                Value::YesNo(if function == Function::And {
                    answers.into_iter().all(|yes| yes)
                } else {
                    answers.into_iter().any(|yes| yes)
                })
            }
            Function::Not => Value::YesNo(!self.eval(&arguments[0])?.yes()),
            Function::PercentOf => {
                Value::Figure(ratio(figure(&arguments[0])?, figure(&arguments[1])?)?)
            }
            Function::Change => {
                // As the cards count it: the change over the size of the
                // earlier figure, so a rise is positive also from a
                // negative one (D-285).
                let new = figure(&arguments[0])?;
                let old = figure(&arguments[1])?;
                let change = new.sub(old).map_err(large)?;
                Value::Figure(ratio(change, old.abs().map_err(large)?)?)
            }
        })
    }
}

/// See [`Formula::evaluate`](super::Formula::evaluate).
pub(crate) fn evaluate(
    tree: &Tree,
    source: &Source,
    period: Period,
    filter: &Filter,
    fallback: CurrencyCode,
) -> Result<FormulaResult, ReportError> {
    // Every name is looked up before any figure is read.
    let mut members = Vec::with_capacity(tree.names.len());
    let mut periods = Vec::with_capacity(tree.names.len());
    for name in &tree.names {
        members.push(resolve(source, name)?);
        periods.push(period_of(name.range, period)?);
    }

    // One dataset per period, with every name of that period.
    let mut figures: HashMap<(Period, GroupMember), Vec<Money>> = HashMap::new();
    let mut asked: Vec<(Period, Vec<GroupMember>)> = Vec::new();
    for (&member, &period) in members.iter().zip(&periods) {
        match asked.iter_mut().find(|(asked, _)| *asked == period) {
            Some((_, members)) => members.push(member),
            None => asked.push((period, vec![member])),
        }
    }
    for (period, chosen) in &asked {
        for result in category_over_time(source, *period, filter, chosen, Overlap::Allowed)? {
            for (series, total) in result.series.iter().zip(&result.totals) {
                if !total.is_zero() {
                    figures
                        .entry((*period, series.of))
                        .or_default()
                        .push(*total);
                }
            }
        }
    }

    // One currency for the whole formula, from the names it counts money of.
    let mut currency: Option<(CurrencyCode, &Name)> = None;
    for ((name, &member), &period) in tree.names.iter().zip(&members).zip(&periods) {
        if name.only_counted {
            continue;
        }
        for money in figures.get(&(period, member)).into_iter().flatten() {
            match currency {
                None => currency = Some((money.currency(), name)),
                Some((first, first_name)) if first != money.currency() => {
                    return Err(ReportError::FormulaMixedCurrencies {
                        position: name.at,
                        first: first_name.text.clone(),
                        first_currency: first,
                        second: name.text.clone(),
                        second_currency: money.currency(),
                    });
                }
                Some(_) => {}
            }
        }
    }
    let currency = currency.map_or(fallback, |(currency, _)| currency);
    let digits = u32::from(source.minor_digits(currency));
    let too_large = |at| ReportError::FormulaTooLarge { position: at };

    let mut values = Vec::with_capacity(tree.names.len());
    let mut counts = Vec::with_capacity(tree.names.len());
    let mut transactions = HashMap::new();
    for ((name, &member), &period) in tree.names.iter().zip(&members).zip(&periods) {
        let minor_units = figures
            .get(&(period, member))
            .and_then(|totals| totals.iter().find(|money| money.currency() == currency))
            .map_or(0, |money| money.minor_units());
        values.push(Exact::new(i128::from(minor_units), digits).map_err(|_| too_large(name.at))?);
        let categories = if name.only_counted {
            source.counted(member).into_iter().collect()
        } else {
            HashSet::new()
        };
        counts.push((period, categories));
        if name.only_counted && !transactions.contains_key(&period) {
            let parts = source.parts(period, filter)?;
            transactions.insert(
                period,
                parts
                    .iter()
                    .map(|part| (part.transaction, part.category))
                    .collect(),
            );
        }
    }

    let evaluator = Evaluator {
        figures: values,
        counts,
        transactions,
    };
    let result = match evaluator.eval(&tree.root) {
        Ok(value) => value.figure(),
        Err(Fail::NoValue(at)) => return Err(ReportError::FormulaNoValue { position: at }),
        Err(Fail::TooLarge(at)) => return Err(too_large(at)),
        Err(Fail::Mistake(error)) => return Err(error),
    };
    let whole = |places, mode| {
        result
            .scaled(places, mode)
            .ok()
            .and_then(|scaled| i64::try_from(scaled).ok())
            .ok_or(too_large(1))
    };
    let value = match tree.kind {
        Kind::Amount => FormulaValue::Amount(Money::from_minor_units(
            whole(digits, source.rounding())?,
            currency,
        )),
        Kind::Number { percent: true } => FormulaValue::Percent(Percent::from_tenths(whole(
            3,
            RoundingMode::HalfAwayFromZero,
        )?)),
        Kind::Number { percent: false } | Kind::YesNo => FormulaValue::Number {
            hundredths: whole(2, RoundingMode::HalfAwayFromZero)?,
        },
    };

    let mut counted: Vec<Counted> = Vec::new();
    for (&member, &period) in members.iter().zip(&periods) {
        if counted
            .iter()
            .any(|known| known.of == member && known.period == period)
        {
            continue;
        }
        counted.push(Counted {
            of: member,
            path: source.path_of(member).unwrap_or_default(),
            period,
        });
    }
    Ok(FormulaResult { value, counted })
}
