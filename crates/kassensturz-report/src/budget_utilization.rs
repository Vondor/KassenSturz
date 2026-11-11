// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `budget_utilization`: how much of each budget is spent and left (D-346,
//! D-347, D-378 to D-380).
//!
//! A budget is a way of looking at the spending and nothing else: this
//! dataset only reads the book's parts and budgets, and never changes a
//! booking, a balance or a category total (principle 1). Every amount is an
//! exact [`Money`] sum with overflow checks; the one division is the share
//! used, rounded once, half away from zero, to one decimal place of a
//! percent, whatever the book's rounding setting says (D-285). The words
//! the user reads are planned, spent, left, over and carried, never advice
//! (AGENTS.md §3.1 rule 11).

use std::collections::{HashMap, HashSet};

use kassensturz_core::Transaction;
use kassensturz_core::{AccountId, Budget, BudgetEvery, BudgetId, CurrencyCode, Date, Money};
use kassensturz_store::{Corruption, StoreError};

use crate::metrics::percent_of;
use crate::{Filter, Percent, Period, ReportError, Source, TextFormat, TextTable};

/// The dataset's ID, as dashboards and the AI name it (D-346).
pub const BUDGET_UTILIZATION: &str = "budget_utilization";

/// How the budgets' periods are shown (the dataset's groupings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetGrouping {
    /// `budget`: each budget with the period that contains the day asked
    /// for, as it stands on that day.
    Budget,
    /// `period`: each budget with every one of its periods that the shown
    /// period touches, up to the day asked for, for a chart over time.
    Period,
}

impl BudgetGrouping {
    /// The fixed name dashboards and the AI use.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::Period => "period",
        }
    }

    /// The grouping with this fixed name.
    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Budget, Self::Period]
            .into_iter()
            .find(|grouping| grouping.id() == id)
    }
}

/// What the user reads next to a period's figures, by a fixed word the UI
/// translates (D-034), with the amount it names. None of them is advice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetNote {
    /// `over`: more was spent than was available, by this positive amount
    /// (e.g. *37.50 € over*).
    Over(Money),
    /// `nothing-to-spend`: nothing was available in the period (zero or
    /// less, after overspending was carried in), so the share used has no
    /// value (*Nothing left to spend in this period*), never 0 % or
    /// infinity.
    NothingToSpend,
    /// `carried-in`: this much came from the period before, below zero
    /// after overspending (e.g. *Carried from last month: 12 €*).
    CarriedIn(Money),
    /// `counts-changed`: the budget counts other categories or accounts
    /// from this period on than in the period before (D-689), so its
    /// figures are not those of the old choice (*Counts other categories
    /// from this month on*).
    CountsChanged,
}

impl BudgetNote {
    /// The fixed word the UI looks the sentence up by.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Over(_) => "over",
            Self::NothingToSpend => "nothing-to-spend",
            Self::CarriedIn(_) => "carried-in",
            Self::CountsChanged => "counts-changed",
        }
    }
}

/// One period of a budget, as it stands on the day asked for. Always
/// `available = planned + carried_in` and `left = available − spent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetPeriod {
    /// The period's first day.
    pub first_day: Date,
    /// The period's last day.
    pub last_day: Date,
    /// The last day whose spending is counted: the period's last day, or
    /// the day asked for if that is earlier.
    pub counted_to: Date,
    /// What the budget plans for this period, from the amount that applies
    /// from its first day (D-345).
    pub planned: Money,
    /// What the period before left (negative after overspending) if it had
    /// carry over on, else zero (D-347).
    pub carried_in: Money,
    /// `planned` plus `carried_in`.
    pub available: Money,
    /// What was booked to the budget's categories and every category inside
    /// them (each once, D-291) from the first day to `counted_to`, in the
    /// categories' currency, from the budget's accounts if it has any
    /// (D-639); a refund reduces it and can make it negative.
    pub spent: Money,
    /// `available` less `spent`; below zero the period is over by that
    /// much.
    pub left: Money,
    /// `spent` ÷ `available` in percent, rounded once, half away from zero,
    /// to one decimal place (D-285); over 100 % as it is. `None` when
    /// `available` is zero or less.
    pub used: Option<Percent>,
    /// Whether what is left at the end of this period goes into the next.
    pub carries_over: bool,
    /// Whether the budget counts other categories or accounts from this
    /// period on than in the one before (D-689).
    pub counts_changed: bool,
}

impl BudgetPeriod {
    /// How much more was spent than was available, as a positive amount;
    /// `None` unless `left` is below zero.
    pub fn over(&self) -> Option<Money> {
        if self.left.is_negative() {
            self.left.checked_neg().ok()
        } else {
            None
        }
    }

    /// The plain-word notes for this period, in the order they are shown.
    pub fn notes(&self) -> Vec<BudgetNote> {
        let mut notes = Vec::new();
        if let Some(over) = self.over() {
            notes.push(BudgetNote::Over(over));
        }
        if self.used.is_none() {
            notes.push(BudgetNote::NothingToSpend);
        }
        if !self.carried_in.is_zero() {
            notes.push(BudgetNote::CarriedIn(self.carried_in));
        }
        if self.counts_changed {
            notes.push(BudgetNote::CountsChanged);
        }
        notes
    }
}

/// One budget with the periods shown of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetUtilization {
    /// The budget.
    pub budget: BudgetId,
    /// The name the user gave it, or `None`: the screens then call it by
    /// its categories (D-664).
    pub name: Option<String>,
    /// Its spending categories in the period asked for, in the order the
    /// user chose them (D-639, D-689).
    pub categories: Vec<AccountId>,
    /// Each category's names from the top-level category down, e.g.
    /// `["Food", "Groceries"]`, in the order of `categories`.
    pub paths: Vec<Vec<String>>,
    /// The user's own accounts it counts from in the period asked for, or
    /// empty for all (D-639, D-689).
    pub accounts: Vec<AccountId>,
    /// How long one of its periods is.
    pub every: BudgetEvery,
    /// The currency of every amount: the categories'.
    pub currency: CurrencyCode,
    /// The periods shown, oldest first: one with the grouping `budget`.
    pub periods: Vec<BudgetPeriod>,
}

fn overflow<E>(_: E) -> ReportError {
    ReportError::Store(StoreError::AmountOverflow)
}

/// The day a budget's figures are asked for: the shown period's last day,
/// or today if that is earlier (D-346).
fn asked_day(period: Period, today: Date) -> Date {
    period.last().min(today)
}

/// The first day of the first period of `budget` whose figures are needed
/// for `period` on `asked`, or `None` if none is: for a budget that carries
/// over in any period, its first day, where the carried chain starts
/// (D-347); otherwise the start of its period that contains the shown
/// period's first day (or the day asked for, if that is earlier).
fn first_needed(budget: &Budget, period: Period, asked: Date) -> Option<Date> {
    if budget.first_day > asked {
        return None;
    }
    if budget.amounts.iter().any(|amount| amount.carry_over) {
        return Some(budget.first_day);
    }
    let from = period.first().min(asked).max(budget.first_day);
    budget.period_containing(from).map(|(start, _)| start)
}

/// The days a `Source` must cover for the budgets' figures of `period` on
/// `today`: from the first day any budget needs (or the period's first day,
/// if earlier) to the period's last day.
pub(crate) fn days_needed(
    budgets: &[Budget],
    period: Period,
    today: Date,
) -> Result<Period, ReportError> {
    let asked = asked_day(period, today);
    let first = budgets
        .iter()
        .filter_map(|budget| first_needed(budget, period, asked))
        .fold(period.first(), Date::min);
    Period::new(first, period.last())
}

/// The periods of `budget` from the one starting on `start` up to the one
/// that contains `asked`, oldest first.
fn periods_from(budget: &Budget, start: Date, asked: Date) -> Vec<(Date, Date)> {
    let mut periods = Vec::new();
    if !budget.every.repeats() {
        let last = budget.last_day.unwrap_or(budget.first_day);
        periods.push((budget.first_day, last));
        return periods;
    }
    let mut first = start;
    // At most one period per day up to `asked`: a book's budget has periods
    // of a month or longer, and the `Source` spans at most 100 years, but a
    // bound keeps a wrong budget from looping.
    while first <= asked && budget.last_day.is_none_or(|last| first <= last) {
        let Some(end) = budget.every.period_end(first) else {
            break;
        };
        let last = budget.last_day.map_or(end, |last| last.min(end));
        periods.push((first, last));
        match last.tomorrow() {
            Ok(next) if next > first => first = next,
            _ => break,
        }
    }
    periods
}

/// The spending one budget counts, by day.
struct Spending {
    /// Each part's day and amount in the budget's currency, by day.
    parts: Vec<(Date, Money)>,
}

impl Spending {
    /// What was spent from `first` to `last`, both included.
    fn between(
        &self,
        currency: CurrencyCode,
        first: Date,
        last: Date,
    ) -> Result<Money, ReportError> {
        let from = self.parts.partition_point(|(day, _)| *day < first);
        let to = self.parts.partition_point(|(day, _)| *day <= last);
        Money::checked_sum(
            currency,
            self.parts[from..to.max(from)]
                .iter()
                .map(|(_, amount)| *amount),
        )
        .map_err(overflow)
    }
}

/// Whether `transaction` counts for a budget limited to `accounts`: it
/// has a part on one of them (none chosen: every transaction). A
/// transaction counts as a whole, as with the filters (D-350, D-639).
fn from_accounts(transaction: &Transaction, accounts: &[AccountId]) -> bool {
    accounts.is_empty()
        || transaction.splits().iter().any(|split| {
            split
                .account
                .is_some_and(|account| accounts.contains(&account))
        })
}

/// Where a category's parts count: the budget's index, the first and last
/// day of the set that counts the category, and that set's accounts.
type Counter<'a> = (usize, Date, Date, &'a [AccountId]);

/// The parts each of `budgets` counts: on each day, those booked to the
/// categories of its set for that day and the categories inside them, each
/// category once per budget however often it is reached (D-291), from the
/// set's accounts if it has any (D-639, D-689), in its currency as booked
/// (D-043). A part in another currency than the budget's that has no
/// amount in it is a broken book, not a figure to guess.
fn spending(
    source: &Source,
    budgets: &[(&Budget, CurrencyCode)],
    covers: Period,
) -> Result<Vec<Spending>, ReportError> {
    // Which budgets each category counts towards, on which days and from
    // which accounts: a budget on a category and another inside it are
    // separate figures (D-344), and each set of a budget counts on its own
    // days (D-689), which never overlap.
    let mut towards: HashMap<AccountId, Vec<Counter<'_>>> = HashMap::new();
    for (index, (budget, _)) in budgets.iter().enumerate() {
        for (set, from, until) in budget.counts_with_days() {
            let roots: HashSet<AccountId> = set.categories.iter().copied().collect();
            for category in source.inside(&roots).into_keys() {
                towards
                    .entry(category)
                    .or_default()
                    .push((index, from, until, &set.accounts));
            }
        }
    }
    let mut spent: Vec<Spending> = budgets
        .iter()
        .map(|_| Spending { parts: Vec::new() })
        .collect();
    for transaction in source.transactions() {
        // By the transaction's booking date, as every dataset (D-122,
        // D-108).
        let day = transaction.booking_date();
        if !covers.contains(day) {
            continue;
        }
        for split in transaction.splits() {
            let Some(category) = split.account else {
                continue;
            };
            let Some(counted) = towards.get(&category) else {
                continue;
            };
            let amount = split
                .foreign
                .as_ref()
                .map_or(split.amount, |foreign| foreign.amount);
            if amount.is_zero() {
                continue;
            }
            for &(index, from, until, accounts) in counted {
                if day < from || day > until || !from_accounts(transaction, accounts) {
                    continue;
                }
                let (_, currency) = budgets[index];
                if currency != amount.currency() {
                    return Err(ReportError::Store(StoreError::Corrupt(
                        Corruption::CurrencyMismatch {
                            transaction: transaction.id(),
                        },
                    )));
                }
                spent[index].parts.push((day, amount));
            }
        }
    }
    for spending in &mut spent {
        spending.parts.sort_by_key(|(day, _)| *day);
    }
    Ok(spent)
}

/// How much of each budget is spent and left in `period`, as it stands on
/// `today` (D-346, D-347).
///
/// The day asked for is `period`'s last day, or `today` if that is earlier.
/// With [`BudgetGrouping::Budget`] each budget shows the period that
/// contains that day, and a budget that has not started or has ended by
/// then is left out; with [`BudgetGrouping::Period`] each budget shows every
/// one of its periods that `period` touches, up to that day. The budgets
/// come in the order of the category tree (by their first category), a
/// category's budgets by their first day. `only` keeps the budgets it names
/// (the bars' *Which budgets*, D-642), `None` all of them.
/// `filter.categories` keeps the budgets whose set for the day asked for
/// has a category that is one of them or inside one (D-689); `filter.accounts` does not apply, since a budget
/// counts the accounts it names itself, or every account (D-350, D-639:
/// the shared filters do not apply accounts to it, and say so).
///
/// The `Source` must be read with [`Source::read_with_budgets`] for
/// `period` and `today`, else this fails with
/// [`ReportError::PeriodNotRead`].
pub fn budget_utilization(
    source: &Source,
    period: Period,
    today: Date,
    grouping: BudgetGrouping,
    filter: &Filter,
    only: Option<&[BudgetId]>,
) -> Result<Vec<BudgetUtilization>, ReportError> {
    source.require(period)?;
    let budgets = source.budgets()?;
    let asked = asked_day(period, today);
    let covers = source.covers();

    let chosen: Option<HashSet<AccountId>> = filter.categories.as_ref().map(|chosen| {
        source
            .inside(&chosen.iter().copied().collect())
            .into_keys()
            .collect()
    });
    let mut shown: Vec<(&Budget, CurrencyCode)> = Vec::new();
    let mut starts: Vec<Date> = Vec::new();
    for budget in budgets {
        if only.is_some_and(|only| !only.contains(&budget.id)) {
            continue;
        }
        let Some(asked_set) = budget.counts_on(asked) else {
            return Err(StoreError::Corrupt(Corruption::BrokenReferences).into());
        };
        if chosen.as_ref().is_some_and(|chosen| {
            !asked_set
                .categories
                .iter()
                .any(|category| chosen.contains(category))
        }) {
            continue;
        }
        let Some(start) = first_needed(budget, period, asked) else {
            continue;
        };
        if start < covers.first() || asked > covers.last() {
            return Err(ReportError::PeriodNotRead);
        }
        // The book keeps every category of a budget, in every set, in its
        // first one's currency.
        let first = asked_set
            .categories
            .first()
            .ok_or(StoreError::Corrupt(Corruption::BrokenReferences))?;
        let category = source
            .accounts()
            .get(first)
            .ok_or(StoreError::UnknownAccount(*first))?;
        shown.push((budget, category.currency));
        starts.push(start);
    }
    let spent = spending(source, &shown, covers)?;
    let position: HashMap<AccountId, usize> = source
        .tree()
        .iter()
        .enumerate()
        .map(|(at, entry)| (entry.category.id, at))
        .collect();

    let mut results = Vec::new();
    for (((budget, currency), start), spending) in shown.iter().zip(starts).zip(&spent) {
        let (budget, currency) = (*budget, *currency);
        let mut carried = Money::zero(currency);
        let mut periods = Vec::new();
        for (first_day, last_day) in periods_from(budget, start, asked) {
            let amount = budget
                .amount_for(first_day)
                .ok_or(StoreError::Corrupt(Corruption::BrokenReferences))?;
            let counted_to = last_day.min(asked);
            let spent = spending.between(currency, first_day, counted_to)?;
            let available = amount.amount.checked_add(carried).map_err(overflow)?;
            let left = available.checked_sub(spent).map_err(overflow)?;
            let used = if available.minor_units() > 0 {
                percent_of(spent, available)?
            } else {
                None
            };
            periods.push(BudgetPeriod {
                first_day,
                last_day,
                counted_to,
                planned: amount.amount,
                carried_in: carried,
                available,
                spent,
                left,
                used,
                carries_over: amount.carry_over,
                // Sets start on the first day of a period (D-689).
                counts_changed: budget.counts.windows(2).any(|pair| {
                    pair[1].from_day == first_day
                        && (pair[0].categories != pair[1].categories
                            || pair[0].accounts != pair[1].accounts)
                }),
            });
            carried = if amount.carry_over && budget.every.repeats() {
                left
            } else {
                Money::zero(currency)
            };
        }
        let periods: Vec<BudgetPeriod> = match grouping {
            BudgetGrouping::Budget => periods
                .into_iter()
                .filter(|shown| shown.first_day <= asked && asked <= shown.last_day)
                .collect(),
            BudgetGrouping::Period => periods
                .into_iter()
                .filter(|shown| shown.last_day >= period.first() && shown.first_day <= asked)
                .collect(),
        };
        if periods.is_empty() {
            continue;
        }
        let asked_set = budget
            .counts_on(asked)
            .ok_or(StoreError::Corrupt(Corruption::BrokenReferences))?;
        let paths = asked_set
            .categories
            .iter()
            .map(|category| {
                source
                    .tree()
                    .get(position.get(category).copied().unwrap_or(usize::MAX))
                    .map(|entry| entry.path.clone())
                    .unwrap_or_default()
            })
            .collect();
        results.push(BudgetUtilization {
            budget: budget.id,
            name: budget.name.clone(),
            categories: asked_set.categories.clone(),
            paths,
            accounts: asked_set.accounts.clone(),
            every: budget.every,
            currency,
            periods,
        });
    }
    results.sort_by_key(|result| {
        (
            result
                .categories
                .first()
                .and_then(|first| position.get(first))
                .copied()
                .unwrap_or(usize::MAX),
            result.periods.first().map(|shown| shown.first_day),
        )
    });
    Ok(results)
}

impl BudgetUtilization {
    /// The budget's categories as one text: each path joined by
    /// `separator`, the categories by a comma, e.g. `Home › Rent,
    /// Electricity`.
    pub fn categories_text(&self, separator: &str) -> String {
        self.paths
            .iter()
            .map(|path| path.join(separator))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// What the screens call the budget: the name the user gave it, or its
    /// categories as [`BudgetUtilization::categories_text`] writes them
    /// (D-664).
    pub fn label(&self, separator: &str) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.categories_text(separator))
    }

    /// The result as a table of text with the columns `period`, `planned`,
    /// `carried_in`, `available`, `spent`, `left` and `used`. A share used
    /// without a value is an empty cell; the component says why in words
    /// ([`BudgetNote::NothingToSpend`]).
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec![
                "period",
                "planned",
                "carried_in",
                "available",
                "spent",
                "left",
                "used",
            ],
            rows: self
                .periods
                .iter()
                .map(|shown| {
                    vec![
                        match self.every {
                            BudgetEvery::Month => format.month(shown.first_day),
                            _ => format.days(shown.first_day, shown.last_day),
                        },
                        format.amount(shown.planned),
                        format.amount(shown.carried_in),
                        format.amount(shown.available),
                        format.amount(shown.spent),
                        format.amount(shown.left),
                        shown
                            .used
                            .map(|used| format.percent(used))
                            .unwrap_or_default(),
                    ]
                })
                .collect(),
        }
    }
}
