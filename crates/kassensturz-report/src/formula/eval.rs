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

use std::collections::{BTreeMap, HashMap, HashSet};

use jiff::ToSpan;
use kassensturz_core::{
    AccountId, AccountKind, CurrencyCode, GroupKind, GroupMember, Money, RoundingMode,
};

use super::exact::{Exact, TooLarge};
use super::lex::Comparison;
use super::parse::{Expr, Function, Kind, Name, Node, Operator, Range, Tree, Words, side_word};
use super::{Counted, FormulaResult, FormulaValue, Term};
use crate::source::Kind as Side;
use crate::{Filter, Overlap, Percent, Period, ReportError, Source, category_over_time};
use kassensturz_store::StoreError;

/// What a name of a formula stands for once it is looked up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Target {
    /// A category or a group, with everything inside it.
    Member(GroupMember),
    /// Everything of one side, also what is not sorted into a category
    /// yet (D-653).
    All(Side),
}

/// What a name in `COUNT` counts transactions by.
enum Counting {
    /// The transactions with a part in one of these categories.
    Categories(HashSet<AccountId>),
    /// The transactions with a part of this side, sorted or not.
    All(Side),
}

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
            // Counted back from the figure's last day, as every word is.
            Words::Months { count, complete } => {
                crate::Rolling::Months { count, complete }.period(day)
            }
        },
    }
}

/// The categories and groups `text` fits: a group's name, a category's path
/// written with `>` (its last parts are enough), or a category's name,
/// ignoring capitals; each with its side and its path as a formula writes
/// it. With a `side` only the income or only the spending ones fit (D-653).
pub(super) fn fits(
    source: &Source,
    text: &str,
    side: Option<Side>,
) -> Vec<(GroupMember, Side, String)> {
    let whole = text.trim().to_lowercase();
    let parts: Vec<String> = whole
        .split('>')
        .map(|part| part.trim().to_owned())
        .collect();
    let mut found: Vec<(GroupMember, Side, String)> = Vec::new();
    for group in source.groups() {
        let kind = match group.kind {
            GroupKind::Income => Side::Income,
            GroupKind::Expense => Side::Spending,
        };
        if group.name.to_lowercase() == whole && side.is_none_or(|side| side == kind) {
            found.push((GroupMember::Group(group.id), kind, group.name.clone()));
        }
    }
    for entry in source.tree() {
        let kind = match entry.category.kind {
            AccountKind::Income => Side::Income,
            _ => Side::Spending,
        };
        if side.is_some_and(|side| side != kind) {
            continue;
        }
        let path: Vec<String> = entry.path.iter().map(|part| part.to_lowercase()).collect();
        let by_path = parts.len() > 1 && path.ends_with(&parts);
        let by_name = path.last() == Some(&whole);
        if by_path || by_name {
            found.push((
                GroupMember::Category(entry.category.id),
                kind,
                entry.path.join(" > "),
            ));
        }
    }
    found
}

/// A path as a formula names it, in single quotes, with the side in front
/// when `with_side` (D-653).
pub(super) fn written_path(side: Side, path: &str, with_side: bool) -> String {
    let quoted = format!("'{}'", path.replace('\'', "''"));
    if with_side {
        format!("{}.{quoted}", side_word(side))
    } else {
        quoted
    }
}

/// What `name` stands for. Two that fit are refused with the ways to write
/// instead, never guessed (T-42): with the side in front when an income and
/// a spending category share the name, or when the formula gave one.
fn resolve(source: &Source, name: &Name) -> Result<Target, ReportError> {
    if name.all
        && let Some(side) = name.side
    {
        return Ok(Target::All(side));
    }
    let found = fits(source, &name.text, name.side);
    match found.as_slice() {
        [] => Err(ReportError::FormulaUnknownName {
            position: name.at,
            name: name.shown(),
        }),
        [(member, _, _)] => Ok(Target::Member(*member)),
        _ => {
            let by_kind = name.side.is_some() || found.iter().any(|fit| fit.1 != found[0].1);
            Err(ReportError::FormulaAmbiguousName {
                position: name.at,
                name: name.shown(),
                paths: found
                    .iter()
                    .map(|(_, side, path)| written_path(*side, path, by_kind))
                    .collect::<Vec<_>>()
                    .join(", "),
                by: if by_kind { "kind" } else { "path" },
            })
        }
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
    counts: Vec<(Period, Counting)>,
    /// Per period of a name in `COUNT`: each transaction's categories and
    /// sides.
    transactions: HashMap<Period, Vec<(usize, Option<AccountId>, Side)>>,
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
                    let (period, counting) = &self.counts[index];
                    for &(transaction, category, side) in &self.transactions[period] {
                        let hit = match counting {
                            Counting::Categories(categories) => {
                                category.is_some_and(|category| categories.contains(&category))
                            }
                            Counting::All(of) => *of == side,
                        };
                        if hit {
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
    let mut members: Vec<Target> = Vec::with_capacity(tree.names.len());
    let mut periods = Vec::with_capacity(tree.names.len());
    for name in &tree.names {
        members.push(resolve(source, name)?);
        periods.push(period_of(name.range, period)?);
    }

    // One dataset per period, with every name of that period.
    let mut figures: HashMap<(Period, Target), Vec<Money>> = HashMap::new();
    let mut asked: Vec<(Period, Vec<Target>)> = Vec::new();
    for (&member, &period) in members.iter().zip(&periods) {
        match asked.iter_mut().find(|(asked, _)| *asked == period) {
            Some((_, members)) => members.push(member),
            None => asked.push((period, vec![member])),
        }
    }
    for (period, chosen) in &asked {
        let categories: Vec<GroupMember> = chosen
            .iter()
            .filter_map(|target| match target {
                Target::Member(member) => Some(*member),
                Target::All(_) => None,
            })
            .collect();
        if !categories.is_empty() {
            for result in
                category_over_time(source, *period, filter, &categories, Overlap::Allowed)?
            {
                for (series, total) in result.series.iter().zip(&result.totals) {
                    if let Some(of) = series.of
                        && !total.is_zero()
                    {
                        figures
                            .entry((*period, Target::Member(of)))
                            .or_default()
                            .push(*total);
                    }
                }
            }
        }
        for side in [Side::Income, Side::Spending] {
            if !chosen.contains(&Target::All(side)) {
                continue;
            }
            // Every part of the side, with the money not sorted into a
            // category yet (D-653).
            let mut totals: BTreeMap<CurrencyCode, Money> = BTreeMap::new();
            for part in source.parts(*period, filter)? {
                if part.kind != side {
                    continue;
                }
                let total = totals
                    .entry(part.currency)
                    .or_insert_with(|| Money::zero(part.currency));
                *total = total
                    .checked_add(part.amount)
                    .map_err(|_| ReportError::Store(StoreError::AmountOverflow))?;
            }
            for total in totals.into_values().filter(|total| !total.is_zero()) {
                figures
                    .entry((*period, Target::All(side)))
                    .or_default()
                    .push(total);
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
                        first: first_name.shown(),
                        first_currency: first,
                        second: name.shown(),
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
        let counting = match member {
            Target::All(side) if name.only_counted => Counting::All(side),
            Target::Member(member) if name.only_counted => {
                Counting::Categories(source.counted(member).into_iter().collect())
            }
            _ => Counting::Categories(HashSet::new()),
        };
        counts.push((period, counting));
        if name.only_counted && !transactions.contains_key(&period) {
            let parts = source.parts(period, filter)?;
            transactions.insert(
                period,
                parts
                    .iter()
                    .map(|part| (part.transaction, part.category, part.kind))
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
    for (&target, &period) in members.iter().zip(&periods) {
        let term = match target {
            Target::Member(member) => Term::Member(member),
            Target::All(Side::Income) => Term::AllIncome,
            Target::All(Side::Spending) => Term::AllSpending,
        };
        if counted
            .iter()
            .any(|known| known.of == term && known.period == period)
        {
            continue;
        }
        let Target::Member(member) = target else {
            // Everything of a side: no path, nothing inside to list.
            counted.push(Counted {
                of: term,
                path: Vec::new(),
                inside: Vec::new(),
                period,
            });
            continue;
        };
        let path = source.path_of(member).unwrap_or_default();
        let inside = source
            .counted(member)
            .into_iter()
            .map(GroupMember::Category)
            .filter(|&category| category != member)
            .filter_map(|category| source.path_of(category)?.pop())
            .collect();
        counted.push(Counted {
            of: term,
            path,
            inside,
            period,
        });
    }
    Ok(FormulaResult { value, counted })
}
