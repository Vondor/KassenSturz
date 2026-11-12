// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Budgets page and the budget bars (M6c.5, D-357, D-381 to D-383): the
//! budgets the user set, how much of each is spent and left, and the changes
//! to them.
//!
//! The figures come from `kassensturz-report` (`budget_utilization`, D-346,
//! D-347); this file only turns them into text and the user's typed amounts
//! and dates into the store's calls (D-065). A budget never changes a
//! booking or a total (principle 1). The book makes every check; this file
//! turns its errors into codes. No Qt in this file: `budgets_model.rs` and
//! `budget_bars_model.rs` hand the result to QML.

use std::collections::HashMap;

use kassensturz_core::{AccountId, Budget, BudgetEvery, BudgetId, CurrencyCode, Date, Report};
use kassensturz_report::{
    BudgetGrouping, BudgetNote, BudgetPeriod, BudgetUtilization, ReportError, Source, TextFormat,
    budget_utilization,
};
use kassensturz_store::{Book, StoreError};

use crate::categories::{CategoryRow, view as category_view};
use crate::charts::{AppText, ChartsError, period};
use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::parse::{AmountInputError, AmountReader};
use crate::scope::{FilterView, Scope};

/// How many periods back a change or a start can reach in the lists of the
/// page: a long-running budget keeps the list short, and its first period is
/// always in it so a mistake can be corrected from the start (D-361).
const PERIODS_BACK: usize = 24;

/// Why a request couldn't be answered or a change wasn't made.
#[derive(Debug)]
pub enum BudgetsError {
    /// A category or budget the screen named isn't in the book any more.
    NoSuchEntry,
    /// A name QML sent isn't one of the fixed names. QML only sends fixed
    /// names, so this is a mistake in the screen.
    UnknownName(&'static str),
    /// The typed amount isn't an amount in the category's currency.
    Amount(AmountInputError, CurrencyCode),
    /// The typed day isn't a date.
    Date,
    /// The report layer refused.
    Report(ReportError),
    /// The book refused it.
    Store(StoreError),
}

impl From<StoreError> for BudgetsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ReportError> for BudgetsError {
    fn from(error: ReportError) -> Self {
        Self::Report(error)
    }
}

impl From<ChartsError> for BudgetsError {
    fn from(error: ChartsError) -> Self {
        match error {
            ChartsError::UnknownName(name) => Self::UnknownName(name),
            ChartsError::Report(error) => Self::Report(error),
            // A period is the only thing asked of the charts crate here.
            ChartsError::Chart(_) | ChartsError::Custom(_) => Self::UnknownName("period"),
        }
    }
}

/// A budget's period word, as QML says it (the keys of D-344); a month
/// from a salary day is `salary-month` (D-651).
pub(crate) const fn every_key(every: BudgetEvery) -> &'static str {
    match every {
        BudgetEvery::Month => "month",
        BudgetEvery::SalaryMonth { .. } => "salary-month",
        BudgetEvery::Quarter => "quarter",
        BudgetEvery::Year => "year",
        BudgetEvery::Range => "range",
    }
}

/// The day a salary month starts on for QML, 0 for any other period.
fn salary_day_of(every: BudgetEvery) -> i32 {
    every.salary_day().map_or(0, i32::from)
}

/// The period of `key`; `salary-month-D` is a salary month from day D,
/// where day 1 is the calendar month, the words of the reports' salary
/// months (D-645, D-651).
fn every_of(key: &str) -> Result<BudgetEvery, BudgetsError> {
    if let Some(day) = key.strip_prefix("salary-month-") {
        return day
            .parse::<i8>()
            .ok()
            .and_then(BudgetEvery::salary_month)
            .ok_or(BudgetsError::UnknownName("every"));
    }
    [
        BudgetEvery::Month,
        BudgetEvery::Quarter,
        BudgetEvery::Year,
        BudgetEvery::Range,
    ]
    .into_iter()
    .find(|every| every_key(*every) == key)
    .ok_or(BudgetsError::UnknownName("every"))
}

/// A note beside a period's figures: the fixed word the screen turns into a
/// sentence (D-380) and the amount it names, empty if it names none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRow {
    /// `over`, `nothing-to-spend`, `carried-in` or `counts-changed`
    /// (D-689).
    pub word: &'static str,
    /// The amount, written for the user, without a minus sign: `negative`
    /// says which way it went.
    pub amount: String,
    /// For `carried-in`: it came from overspending, so it takes away from
    /// what can be spent.
    pub negative: bool,
}

/// One budget's period as a bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    /// The budget's ID as text.
    pub budget: String,
    /// Its categories' IDs as text (D-639).
    pub categories: Vec<String>,
    /// What the screens call it: the name the user gave it, or else its
    /// categories as `category_names` has them (D-664).
    pub name: String,
    /// The name the user gave it; empty for none (D-664).
    pub own_name: String,
    /// Each category's names from the top-level category down, joined, the
    /// categories separated by commas.
    pub category_names: String,
    /// The names of the accounts it counts from, separated by commas;
    /// empty when it counts every account (D-639).
    pub accounts: String,
    /// `month`, `salary-month`, `quarter`, `year` or `range`.
    pub every: &'static str,
    /// The day a `salary-month` starts on, else 0 (D-651).
    pub salary_day: i32,
    /// The category's currency.
    pub currency: String,
    /// The period's first day, as the user writes dates.
    pub first_day: String,
    /// The period's last day.
    pub last_day: String,
    /// The last day whose spending is counted: before the period's last
    /// day while the period is running (D-379).
    pub counted_to: String,
    /// Whether spending is counted only to a day before the period's end.
    pub running: bool,
    /// Whether the budget has not started yet: the bar is empty, nothing is
    /// spent and what it plans is left (D-710).
    pub starts_later: bool,
    /// Whether the budget ended before the day asked for: this is its last
    /// period, complete (D-710).
    pub ended: bool,
    /// What the period plans.
    pub planned: String,
    /// What came from the period before (negative after overspending).
    pub carried_in: String,
    /// Planned plus carried in.
    pub available: String,
    /// What was spent.
    pub spent: String,
    /// Available less spent; below zero it is the amount over.
    pub left: String,
    /// The share used, rounded once (D-285); empty when nothing is
    /// available.
    pub used: String,
    /// How much of the bar is filled, 0 to 1000; never read back as a
    /// figure.
    pub permille: i32,
    /// Whether more was spent than was available.
    pub over: bool,
    /// Whether the bar has any share at all.
    pub has_share: bool,
    /// What is left to say, by fixed words.
    pub notes: Vec<NoteRow>,
}

/// The names of the user's own accounts, by ID, for the screens that name
/// the accounts a budget counts from.
fn account_names(book: &Book) -> Result<HashMap<AccountId, String>, StoreError> {
    Ok(book
        .accounts(true)?
        .into_iter()
        .map(|account| (account.id, account.name))
        .collect())
}

/// The names of `accounts`, separated by commas; empty for none (every
/// account).
fn accounts_text(accounts: &[AccountId], names: &HashMap<AccountId, String>) -> String {
    accounts
        .iter()
        .filter_map(|id| names.get(id).cloned())
        .collect::<Vec<_>>()
        .join(", ")
}

fn bar<L: Locale>(
    shown: &BudgetUtilization,
    period: &BudgetPeriod,
    locale: &L,
    text: &AppText<'_, L>,
    names: &HashMap<AccountId, String>,
) -> Bar {
    let notes = period
        .notes()
        .into_iter()
        .map(|note| {
            let (amount, negative) = match note {
                BudgetNote::Over(amount) => (text.amount(amount), false),
                BudgetNote::CarriedIn(amount) if amount.is_negative() => (
                    amount
                        .checked_neg()
                        .map_or_else(|_| text.amount(amount), |positive| text.amount(positive)),
                    true,
                ),
                BudgetNote::CarriedIn(amount) => (text.amount(amount), false),
                BudgetNote::NothingToSpend | BudgetNote::CountsChanged => (String::new(), false),
            };
            NoteRow {
                word: note.id(),
                amount,
                negative,
            }
        })
        .collect();
    Bar {
        budget: shown.budget.to_string(),
        categories: shown.categories.iter().map(ToString::to_string).collect(),
        name: shown.label(" › "),
        own_name: shown.name.clone().unwrap_or_default(),
        category_names: shown.categories_text(" › "),
        accounts: accounts_text(&shown.accounts, names),
        every: every_key(shown.every),
        salary_day: salary_day_of(shown.every),
        currency: shown.currency.to_string(),
        first_day: locale.short_date(period.first_day),
        last_day: locale.short_date(period.last_day),
        counted_to: locale.short_date(period.counted_to),
        running: !period.starts_later && period.counted_to < period.last_day,
        starts_later: period.starts_later,
        ended: period.ended,
        planned: text.amount(period.planned),
        carried_in: text.amount(period.carried_in),
        available: text.amount(period.available),
        spent: text.amount(period.spent),
        left: text.amount(period.left),
        used: period
            .used
            .map(|used| text.percent(used))
            .unwrap_or_default(),
        // Tenths of a percent are per-mille; the bar shows 0 to 100 %.
        permille: period.used.map_or(0, |used| {
            i32::try_from(used.tenths().clamp(0, 1000)).unwrap_or(0)
        }),
        over: period.left.is_negative(),
        has_share: period.used.is_some(),
        notes,
    }
}

/// The budgets' bars for `period_key` (a fixed period name or a month as
/// `YYYY-MM`) as they stand on the period's last day, or today if that is
/// earlier. `only` are the IDs of categories: only the budgets with one of
/// those or a category inside them (empty: every budget). `budgets` are the
/// IDs of the budgets the tile shows, its *Which budgets* (D-642; empty:
/// every budget); one no longer in the book is left out. A budget that has
/// not started by then has an empty bar of its first period, and one that
/// has ended by then the bar of its last period (D-710).
pub fn bars_filtered(
    book: &Book,
    locale: &impl Locale,
    period_key: &str,
    only: &[String],
    budgets: &[String],
    scope: &Scope,
) -> Result<(Vec<Bar>, FilterView), BudgetsError> {
    let today = locale.today();
    let period = period(book, period_key, today)?;
    let source = Source::read_with_budgets(book, period, today)?;
    // The filter bar's or the component's own categories (M6c.7); budgets
    // count every account, so the accounts are noted and not applied.
    let (mut filter, view) = scope.resolve(book, &source)?;
    if view.nothing {
        return Ok((Vec::new(), view));
    }
    if filter.categories.is_none() && !only.is_empty() {
        let ids: Vec<AccountId> = book
            .categories()?
            .into_iter()
            .map(|entry| entry.category.id)
            .filter(|id| only.contains(&id.to_string()))
            .collect();
        filter.categories = Some(ids);
    }
    let chosen: Option<Vec<BudgetId>> = if budgets.is_empty() {
        None
    } else {
        Some(
            book.budgets()?
                .into_iter()
                .map(|budget| budget.id)
                .filter(|id| budgets.contains(&id.to_string()))
                .collect(),
        )
    };
    let shown = budget_utilization(
        &source,
        period,
        today,
        BudgetGrouping::Budget,
        &filter,
        chosen.as_deref(),
    )?;
    let text = AppText::new(book, locale);
    let names = account_names(book)?;
    let bars = shown
        .iter()
        .flat_map(|budget| {
            budget
                .periods
                .iter()
                .map(|period| bar(budget, period, locale, &text, &names))
        })
        .collect();
    Ok((bars, view))
}

/// As [`bars_filtered`], with nothing narrowed by a filter bar.
pub fn bars(
    book: &Book,
    locale: &impl Locale,
    period_key: &str,
    only: &[String],
) -> Result<Vec<Bar>, BudgetsError> {
    bars_filtered(book, locale, period_key, only, &[], &Scope::default()).map(|(bars, _)| bars)
}

/// One budget to pick in the bars tile's *Which budgets* (D-642).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetChoice {
    /// The budget's ID as text.
    pub id: String,
    /// What the screens call it: the name the user gave it, or its
    /// categories (D-664), then the accounts it counts from, if not all.
    pub name: String,
}

/// Every budget of the book, to pick from, in the order they were made.
pub fn choices(book: &Book) -> Result<Vec<BudgetChoice>, StoreError> {
    let paths: HashMap<AccountId, String> = book
        .categories()?
        .into_iter()
        .map(|entry| (entry.category.id, entry.path.join(" › ")))
        .collect();
    let names = account_names(book)?;
    Ok(book
        .budgets()?
        .into_iter()
        .map(|budget| {
            // What it counts now or, if a change is planned, from then on
            // (D-689): the newest set.
            let (categories, accounts) = budget
                .counts
                .last()
                .map(|set| (set.categories.as_slice(), set.accounts.as_slice()))
                .unwrap_or_default();
            let categories = categories
                .iter()
                .filter_map(|id| paths.get(id).cloned())
                .collect::<Vec<_>>()
                .join(", ");
            let accounts = accounts_text(accounts, &names);
            // Its own name, if the user gave it one (D-664).
            let label = budget.name.unwrap_or(categories);
            BudgetChoice {
                id: budget.id.to_string(),
                // The screen's own words would need QML; the accounts follow
                // in brackets, as a closed account does in the lists.
                name: if accounts.is_empty() {
                    label
                } else {
                    format!("{label} ({accounts})")
                },
            }
        })
        .collect())
}

/// How many budgets the book has.
pub fn count(book: &Book) -> Result<usize, StoreError> {
    Ok(book.budgets()?.len())
}

/// One period the user can start or change a budget from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeriodChoice {
    /// The first day, as ISO 8601 text: what a change is sent with.
    pub day: String,
    /// The first day, as the user writes dates.
    pub first_day: String,
    /// The last day.
    pub last_day: String,
    /// Whether it is the period that contains today.
    pub current: bool,
}

fn choice(
    every: BudgetEvery,
    start: Date,
    today: Date,
    locale: &impl Locale,
    last: Option<Date>,
) -> PeriodChoice {
    let end = last.or_else(|| every.period_end(start)).unwrap_or(start);
    PeriodChoice {
        day: start.to_string(),
        first_day: locale.short_date(start),
        last_day: locale.short_date(end),
        current: start <= today && today <= end,
    }
}

/// The start of the period after the one starting on `start`.
fn next_start(every: BudgetEvery, start: Date) -> Option<Date> {
    every
        .period_end(start)
        .and_then(|end| end.tomorrow().ok())
        .filter(|next| *next > start)
}

/// The start of the period before the one starting on `start`.
fn previous_start(every: BudgetEvery, start: Date) -> Option<Date> {
    start
        .yesterday()
        .ok()
        .and_then(|day| every.period_start(day))
        .filter(|before| *before < start)
}

/// The periods a new budget of this kind can begin with, newest first: the
/// next one, the current one and `PERIODS_BACK` before it (D-357). A range
/// has none: the user types its days.
pub fn start_choices(
    every_key: &str,
    locale: &impl Locale,
) -> Result<Vec<PeriodChoice>, BudgetsError> {
    let every = every_of(every_key)?;
    let today = locale.today();
    let Some(current) = every.period_start(today) else {
        return Ok(Vec::new());
    };
    let mut choices = Vec::new();
    if let Some(next) = next_start(every, current) {
        choices.push(choice(every, next, today, locale, None));
    }
    let mut start = current;
    for _ in 0..=PERIODS_BACK {
        choices.push(choice(every, start, today, locale, None));
        match previous_start(every, start) {
            Some(before) => start = before,
            None => break,
        }
    }
    Ok(choices)
}

/// The periods of `budget` a change can apply from, newest first: from the
/// current (or last) one back, at most `PERIODS_BACK` of them, and always
/// the budget's first, which corrects it from the start (D-361).
fn change_choices(budget: &Budget, locale: &impl Locale) -> Vec<PeriodChoice> {
    let today = locale.today();
    let every = budget.every;
    if !every.repeats() {
        return vec![choice(
            every,
            budget.first_day,
            today,
            locale,
            budget.last_day,
        )];
    }
    // The latest period that exists: today's, or the last one of an ended
    // budget.
    let latest_day = budget.last_day.map_or(today, |last| last.min(today));
    let Some(mut start) = every
        .period_start(latest_day.max(budget.first_day))
        .map(|start| start.max(budget.first_day))
    else {
        return Vec::new();
    };
    let mut choices = Vec::new();
    for _ in 0..=PERIODS_BACK {
        choices.push(choice(every, start, today, locale, None));
        if start <= budget.first_day {
            return choices;
        }
        match previous_start(every, start) {
            Some(before) if before >= budget.first_day => start = before,
            _ => break,
        }
    }
    if choices.last().map(|last| last.day.as_str()) != Some(budget.first_day.to_string().as_str()) {
        choices.push(choice(every, budget.first_day, today, locale, None));
    }
    choices
}

/// One budget as the Budgets page lists it.
#[derive(Debug, PartialEq, Eq)]
pub struct BudgetRow {
    /// The budget's ID as text.
    pub id: String,
    /// Its categories' IDs as text (D-639).
    pub categories: Vec<String>,
    /// What the screens call it: the name the user gave it, or else its
    /// categories as `category_names` has them (D-664).
    pub name: String,
    /// The name the user gave it; empty for none (D-664).
    pub own_name: String,
    /// Each category's names from the top-level category down, joined, the
    /// categories separated by commas.
    pub category_names: String,
    /// The names of the accounts it counts from, separated by commas;
    /// empty when it counts every account.
    pub accounts: String,
    /// The IDs of the accounts it counts from, as text; empty for every
    /// account.
    pub account_ids: Vec<String>,
    /// The first day, ISO 8601, of the earliest period from which a change
    /// of what it counts may apply: the current one, or its first if it
    /// starts later; empty for an ended budget (D-689).
    pub counts_from: String,
    /// What it counted before, and what it is to count later, if that
    /// differs from what it counts now: oldest first (D-689).
    pub history: Vec<CountsLine>,
    /// `month`, `salary-month`, `quarter`, `year` or `range`.
    pub every: &'static str,
    /// The day a `salary-month` starts on, else 0 (D-651).
    pub salary_day: i32,
    /// The categories' currency.
    pub currency: String,
    /// What it plans now, or at its last period if it has ended.
    pub amount: String,
    /// What it plans, as the user types an amount.
    pub amount_input: String,
    /// Whether what is left goes into the next period.
    pub carry_over: bool,
    /// `running`, `upcoming` or `ended`, from today.
    pub state: &'static str,
    /// The first day, as the user writes dates.
    pub first_day: String,
    /// The last day; empty if it keeps going.
    pub last_day: String,
    /// Whether *Stop* does something: it keeps going, and has started.
    pub can_stop: bool,
    /// The periods a change can apply from, newest first.
    pub changes: Vec<PeriodChoice>,
    /// This period's bar; for a budget that starts later its first
    /// period's, empty, and for an ended one its last period's (D-710).
    pub bar: Option<Bar>,
}

/// What a budget counted before the set it counts now, or is to count
/// after it (D-689): the budget page's history line, e.g. *Until 31 Oct
/// 2026 it counted Food, Rent.*
#[derive(Debug, PartialEq, Eq)]
pub struct CountsLine {
    /// Whether it is an earlier set; else a set planned for later.
    pub earlier: bool,
    /// For an earlier set its last day, for a later one its first day, as
    /// the user writes dates.
    pub day: String,
    /// Its categories' paths, separated by commas.
    pub category_names: String,
    /// The names of the accounts it counts from, separated by commas;
    /// empty for every account.
    pub accounts: String,
}

/// Everything the page shows.
#[derive(Debug)]
pub struct BudgetsView {
    /// The budgets, in the order of the category tree.
    pub budgets: Vec<BudgetRow>,
    /// The spending categories a budget can be set on, in tree order.
    pub categories: Vec<CategoryRow>,
    /// The user's own accounts a budget can count from, open ones first,
    /// by name; closed ones are not offered (D-639).
    pub accounts: Vec<(String, String)>,
}

/// The budgets of `book` with the bars of the period that contains today.
pub fn view(book: &Book, locale: &impl Locale) -> Result<BudgetsView, BudgetsError> {
    let today = locale.today();
    let categories: Vec<CategoryRow> = category_view(book)?.rows;
    let bars = bars(book, locale, "this-month", &[])?;
    let text = AppText::new(book, locale);
    let names = account_names(book)?;
    let mut rows = Vec::new();
    let paths_of = |ids: &[AccountId]| {
        ids.iter()
            .filter_map(|id| {
                let id = id.to_string();
                categories.iter().find(|row| row.id == id)
            })
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    for budget in book.budgets()? {
        let started = budget.first_day <= today;
        let ended = budget.last_day.is_some_and(|last| last < today);
        let state = if !started {
            "upcoming"
        } else if ended {
            "ended"
        } else {
            "running"
        };
        // What it plans now: the amount of the period that contains today,
        // else its last (or first) one.
        let day = if !started {
            budget.first_day
        } else if ended {
            budget.last_day.unwrap_or(today)
        } else {
            today
        };
        let start = budget.every.period_start(day).unwrap_or(budget.first_day);
        // What it counts on that day (D-689).
        let Some(set) = budget.counts_on(day) else {
            continue;
        };
        let chosen: Vec<&CategoryRow> = set
            .categories
            .iter()
            .filter_map(|id| {
                let id = id.to_string();
                categories.iter().find(|row| row.id == id)
            })
            .collect();
        let Some(category) = chosen.first() else {
            continue;
        };
        let history = budget
            .counts_with_days()
            .filter(|(other, _, _)| other.from_day != set.from_day)
            .map(|(other, from, until)| {
                let earlier = other.from_day < set.from_day;
                CountsLine {
                    earlier,
                    day: locale.short_date(if earlier { until } else { from }),
                    category_names: paths_of(&other.categories),
                    accounts: accounts_text(&other.accounts, &names),
                }
            })
            .collect();
        // A change of what it counts applies from the current period on, or
        // from the first if it starts later; an ended one has none.
        let counts_from = if today < budget.first_day {
            Some(budget.first_day)
        } else {
            budget.period_containing(today).map(|(start, _)| start)
        };
        let planned = budget
            .amount_for(start)
            .or_else(|| budget.amounts.first())
            .ok_or(StoreError::Corrupt(
                kassensturz_store::Corruption::BrokenReferences,
            ))?;
        let currency = book_currency(book, &budget)?;
        let ids = budget.id.to_string();
        let category_names = chosen
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(BudgetRow {
            id: ids.clone(),
            categories: chosen.iter().map(|row| row.id.clone()).collect(),
            name: budget
                .name
                .clone()
                .unwrap_or_else(|| category_names.clone()),
            own_name: budget.name.clone().unwrap_or_default(),
            category_names,
            accounts: accounts_text(&set.accounts, &names),
            account_ids: set.accounts.iter().map(ToString::to_string).collect(),
            counts_from: counts_from.map_or_else(String::new, |day| day.to_string()),
            history,
            every: every_key(budget.every),
            salary_day: salary_day_of(budget.every),
            currency: category.currency.clone(),
            amount: text.amount(planned.amount),
            amount_input: book
                .currency_minor_digits(currency)
                .map(|digits| input_text(planned.amount, digits, locale))
                .unwrap_or_default(),
            carry_over: planned.carry_over,
            state,
            first_day: locale.short_date(budget.first_day),
            last_day: budget
                .last_day
                .map_or_else(String::new, |d| locale.short_date(d)),
            can_stop: state == "running"
                && budget.every.repeats()
                && budget
                    .period_containing(today)
                    .is_some_and(|(_, end)| budget.last_day.is_none_or(|last| last > end)),
            changes: change_choices(&budget, locale),
            bar: bars.iter().find(|bar| bar.budget == ids).cloned(),
        });
    }
    let categories = categories.into_iter().filter(|row| !row.income).collect();
    let mut accounts: Vec<(String, String)> = book
        .accounts(false)?
        .into_iter()
        .map(|account| (account.id.to_string(), account.name))
        .collect();
    accounts.sort_by_key(|(_, name)| name.to_lowercase());
    Ok(BudgetsView {
        budgets: rows,
        categories,
        accounts,
    })
}

/// The currency of `budget`'s amounts: its first category's, which the book
/// keeps for all of them.
fn book_currency(book: &Book, budget: &Budget) -> Result<CurrencyCode, BudgetsError> {
    let first = budget
        .counts
        .first()
        .and_then(|set| set.categories.first())
        .ok_or(BudgetsError::NoSuchEntry)?;
    Ok(book.account(*first)?.currency)
}

/// The category with this ID. Any category passes: the book itself refuses an
/// income one, in words that name it (`store.budget-not-spending`).
fn category_of(book: &Book, id: &str) -> Result<(AccountId, CurrencyCode), BudgetsError> {
    book.categories()?
        .into_iter()
        .find(|entry| entry.category.id.to_string() == id)
        .map(|entry| (entry.category.id, entry.category.currency))
        .ok_or(BudgetsError::NoSuchEntry)
}

/// A positive `amount` as the field of the sheet holds it: the digits with
/// the locale's decimal point and all the decimal places, with no currency
/// symbol and no grouping, so that reading it back gives the same amount.
fn input_text(amount: kassensturz_core::Money, minor_digits: u8, locale: &impl Locale) -> String {
    let units = amount.minor_units().unsigned_abs();
    if minor_digits == 0 {
        return units.to_string();
    }
    let scale = 10_u64.pow(u32::from(minor_digits));
    format!(
        "{}{}{:0width$}",
        units / scale,
        locale.decimal_point(),
        units % scale,
        width = usize::from(minor_digits)
    )
}

fn budget_of(book: &Book, id: &str) -> Result<Budget, BudgetsError> {
    book.budgets()?
        .into_iter()
        .find(|budget| budget.id.to_string() == id)
        .ok_or(BudgetsError::NoSuchEntry)
}

fn read_amount(
    book: &Book,
    text: &str,
    currency: CurrencyCode,
    locale: &impl Locale,
) -> Result<kassensturz_core::Money, BudgetsError> {
    let format = locale.amount_format(currency);
    let group_separator = locale.group_separator();
    let reader = AmountReader {
        format: &format,
        group_separator: &group_separator,
        currency,
        minor_digits: book.currency_minor_digits(currency)?,
    };
    reader
        .read(text)
        .map_err(|error| BudgetsError::Amount(error, currency))
}

fn read_day(text: &str, locale: &impl Locale) -> Result<Date, BudgetsError> {
    locale.read_date(text.trim()).ok_or(BudgetsError::Date)
}

/// A day sent by QML as ISO 8601 text, from a list this file made.
fn iso_day(text: &str) -> Result<Date, BudgetsError> {
    text.parse().map_err(|_| BudgetsError::UnknownName("day"))
}

/// What the user typed for a new budget.
pub struct NewBudget<'a> {
    /// The name the user gave it, as typed; empty for none (D-664).
    pub name: &'a str,
    /// The spending categories' IDs as text, at least one (D-639).
    pub categories: &'a [String],
    /// The IDs of the user's own accounts it counts from, as text; empty for
    /// every account.
    pub accounts: &'a [String],
    /// `month`, `salary-month-D` (from day D, 1 to 31; D-651), `quarter`,
    /// `year` or `range`.
    pub every: &'a str,
    /// The amount per period, as typed.
    pub amount: &'a str,
    /// Whether what is left goes into the next period.
    pub carry_over: bool,
    /// For a month, quarter or year: the first day of its first period as
    /// ISO 8601 text, one of [`start_choices`]; empty for the current one.
    /// For a range: the first day, as typed.
    pub first_day: &'a str,
    /// For a range: the last day, as typed.
    pub last_day: &'a str,
}

/// Makes a budget.
pub fn add_budget(
    book: &mut Book,
    new: &NewBudget<'_>,
    locale: &impl Locale,
) -> Result<BudgetId, BudgetsError> {
    let mut categories = Vec::with_capacity(new.categories.len());
    for id in new.categories {
        categories.push(category_of(book, id)?);
    }
    // The book refuses categories in more than one currency in its own
    // words; the amount is read in the first one's.
    let Some(&(_, currency)) = categories.first() else {
        return Err(StoreError::BudgetChoice {
            reason: "no-category",
        }
        .into());
    };
    let categories: Vec<AccountId> = categories.into_iter().map(|(id, _)| id).collect();
    let own = book.accounts(true)?;
    let mut accounts = Vec::with_capacity(new.accounts.len());
    for id in new.accounts {
        let account = own
            .iter()
            .find(|account| account.id.to_string() == *id)
            .ok_or(BudgetsError::NoSuchEntry)?;
        accounts.push(account.id);
    }
    let every = every_of(new.every)?;
    let amount = read_amount(book, new.amount, currency, locale)?;
    let (first_day, last_day) = if every.repeats() {
        let first = if new.first_day.is_empty() {
            every
                .period_start(locale.today())
                .ok_or(BudgetsError::UnknownName("every"))?
        } else {
            iso_day(new.first_day)?
        };
        (first, None)
    } else {
        (
            read_day(new.first_day, locale)?,
            Some(read_day(new.last_day, locale)?),
        )
    };
    // A range never carries over (D-344); the sheet hides the switch.
    let carry_over = new.carry_over && every.repeats();
    Ok(book.add_named_budget(
        Some(new.name),
        &categories,
        &accounts,
        every,
        first_day,
        last_day,
        amount,
        carry_over,
    )?)
}

/// What the user changed of a budget on its page.
pub struct BudgetChange<'a> {
    /// Its name, as typed; empty for none (D-664).
    pub name: &'a str,
    /// Whether the amount or the carry over changes. `false` when the user
    /// changed only the name, so no period's amount is touched: saving the
    /// same amount again would still drop what was planned for later
    /// periods (D-345).
    pub changes_amount: bool,
    /// The first day of the period the change applies from, ISO 8601 text,
    /// one of the budget's `changes`.
    pub from_day: &'a str,
    /// The amount from then on, as typed.
    pub amount: &'a str,
    /// Whether what is left goes into the next period from then on.
    pub carry_over: bool,
    /// What it counts from `from_day` on: the spending categories' IDs as
    /// text, at least one, and the IDs of the user's own accounts, empty for
    /// every account (D-689); `None` when the user left them as they are.
    pub counts: Option<(&'a [String], &'a [String])>,
}

/// Saves what the user changed of the budget: its name (D-664), the
/// amount with `carry_over` (D-345, D-361) and what it counts (D-689),
/// from the period that starts on `from_day` on, as one change that one
/// Undo takes back. What it counts changes from the current period on at
/// the earliest (`store.budget-period` `before-current-period`); the same
/// categories and accounts again are no change.
pub fn change_budget(
    book: &mut Book,
    id: &str,
    change: &BudgetChange<'_>,
    locale: &impl Locale,
) -> Result<(), BudgetsError> {
    let budget = budget_of(book, id)?;
    let amount = if change.changes_amount {
        let currency = book_currency(book, &budget)?;
        let amount = read_amount(book, change.amount, currency, locale)?;
        Some((amount, change.carry_over && budget.every.repeats()))
    } else {
        None
    };
    let counts = match change.counts {
        Some((categories, accounts)) => {
            let from = iso_day(change.from_day)?;
            let mut chosen = Vec::with_capacity(categories.len());
            for id in categories {
                chosen.push(category_of(book, id)?.0);
            }
            let own = book.accounts(true)?;
            let mut from_accounts = Vec::with_capacity(accounts.len());
            for id in accounts {
                let account = own
                    .iter()
                    .find(|account| account.id.to_string() == *id)
                    .ok_or(BudgetsError::NoSuchEntry)?;
                from_accounts.push(account.id);
            }
            // The same set again is no change, from any period.
            let same = budget
                .counts_on(from)
                .is_some_and(|set| set.categories == chosen && set.accounts == from_accounts);
            (!same).then_some((chosen, from_accounts))
        }
        None => None,
    };
    if amount.is_none() && counts.is_none() {
        book.set_budget_name(budget.id, Some(change.name))?;
        return Ok(());
    }
    let from = iso_day(change.from_day)?;
    let today = locale.today();
    book.as_one_action(kassensturz_store::Action::EditBudget, |book| {
        book.edit_budget(
            budget.id,
            from,
            amount,
            counts
                .as_ref()
                .map(|(categories, accounts)| (categories.as_slice(), accounts.as_slice())),
            today,
        )?;
        book.set_budget_name(budget.id, Some(change.name))
    })?;
    Ok(())
}

/// Ends the budget after the period that contains today; the past stays
/// (D-345).
pub fn stop_budget(book: &mut Book, id: &str, locale: &impl Locale) -> Result<(), BudgetsError> {
    let budget = budget_of(book, id)?;
    let (_, end) = budget
        .period_containing(locale.today())
        .ok_or(BudgetsError::NoSuchEntry)?;
    Ok(book.set_budget_last_day(budget.id, Some(end))?)
}

/// Deletes the budget with its past results; one Undo brings it back
/// (D-032).
pub fn remove_budget(book: &mut Book, id: &str) -> Result<(), BudgetsError> {
    let budget = budget_of(book, id)?;
    Ok(book.remove_budget(budget.id)?)
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &BudgetsError, locale: &impl Locale) -> ErrorText {
    match error {
        BudgetsError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        // A mistake in the screen, logged and not shown (like the charts').
        BudgetsError::UnknownName(what) => {
            eprintln!("kassensturz: a budget screen sent an unknown {what}");
            ErrorText::plain("input.no-such-entry")
        }
        BudgetsError::Amount(error, currency) => ErrorText::amount(*error, book, *currency, locale),
        BudgetsError::Date => ErrorText {
            code: "input.malformed-date",
            params: vec![("example", locale.short_date(locale.today()))],
        },
        BudgetsError::Report(error) => report_text(book, &error.report(), locale),
        BudgetsError::Store(error) => report_text(book, &error.report(), locale),
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{
        Account, AccountKind, AccountType, Money, Split, Transaction, TransactionId,
    };

    use super::*;
    use crate::categories::{NewCategory, add_category};
    use crate::register::tests::TestLocale;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn day(year: i16, month: i8, day: i8) -> Date {
        Date::new(year, month, day).expect("a date")
    }

    /// A book with a checking account and these categories: Food with
    /// Groceries inside it, Rent, and Salary for money received.
    struct Fixture {
        book: Book,
        checking: AccountId,
        food: String,
        groceries: String,
        rent: String,
        salary: String,
    }

    fn category(book: &mut Book, name: &str, income: bool, parent: &str) -> String {
        let new = NewCategory {
            name,
            income,
            parent,
            currency: "",
        };
        add_category(book, &new, EUR)
            .expect("category added")
            .to_string()
    }

    fn fixture() -> Fixture {
        let mut book = Book::create_in_memory().expect("book");
        book.use_currency(EUR).expect("currency used");
        let checking = AccountId::generate();
        book.add_account(&Account {
            id: checking,
            name: "Checking".into(),
            kind: AccountKind::Asset,
            currency: EUR,
            account_type: Some(AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        let food = category(&mut book, "Food", false, "");
        let groceries = category(&mut book, "Groceries", false, &food);
        let rent = category(&mut book, "Rent", false, "");
        let salary = category(&mut book, "Salary", true, "");
        Fixture {
            book,
            checking,
            food,
            groceries,
            rent,
            salary,
        }
    }

    impl Fixture {
        /// Money out of checking, booked to the category with this ID.
        fn spend(&mut self, category: &str, on: Date, cents: i64) {
            let category: AccountId = self
                .book
                .categories()
                .expect("categories")
                .into_iter()
                .map(|entry| entry.category.id)
                .find(|id| id.to_string() == category)
                .expect("a category");
            let transaction = Transaction::new(
                TransactionId::generate(),
                on,
                None,
                vec![
                    Split::new(self.checking, Money::from_minor_units(-cents, EUR)),
                    Split::new(category, Money::from_minor_units(cents, EUR)),
                ],
            )
            .expect("balanced");
            self.book.add_transaction(&transaction).expect("added");
        }

        fn add(&mut self, category: &str, every: &str, amount: &str, carry: bool, first: &str) {
            let new = NewBudget {
                name: "",
                categories: &[category.to_owned()],
                accounts: &[],
                every,
                amount,
                carry_over: carry,
                first_day: first,
                last_day: "",
            };
            add_budget(&mut self.book, &new, &TestLocale).expect("budget added");
        }

        fn code(&self, error: &BudgetsError) -> (&'static str, Vec<(&'static str, String)>) {
            let shown = error_text(&self.book, error, &TestLocale);
            (shown.code, shown.params)
        }
    }

    /// A change of the amount from the period that starts on `from` on,
    /// without a name.
    fn amount<'a>(from: &'a str, amount: &'a str) -> BudgetChange<'a> {
        BudgetChange {
            name: "",
            changes_amount: true,
            from_day: from,
            amount,
            carry_over: false,
            counts: None,
        }
    }

    fn monthly_food() -> Fixture {
        let mut f = fixture();
        let food = f.food.clone();
        f.add(&food, "month", "400,00", false, "2026-10-01");
        f
    }

    #[test]
    fn a_bar_shows_what_is_spent_and_left_with_the_categories_inside() {
        let mut f = monthly_food();
        let (groceries, food) = (f.groceries.clone(), f.food.clone());
        f.spend(&groceries, day(2026, 10, 5), 12_000);
        f.spend(&food, day(2026, 10, 6), 3_000);
        // Rent is no part of the Food budget; a later day is not spent yet.
        let rent = f.rent.clone();
        f.spend(&rent, day(2026, 10, 7), 80_000);
        f.spend(&groceries, day(2026, 10, 25), 5_000);
        let shown = bars(&f.book, &TestLocale, "this-month", &[]).expect("bars");
        assert_eq!(shown.len(), 1);
        let bar = &shown[0];
        assert_eq!(bar.name, "Food");
        assert_eq!(bar.every, "month");
        assert_eq!(bar.currency, "EUR");
        assert_eq!(bar.planned, "400,00 €");
        assert_eq!(bar.spent, "150,00 €");
        assert_eq!(bar.left, "250,00 €");
        assert_eq!(bar.available, "400,00 €");
        assert_eq!(bar.used, "37,5 %");
        assert_eq!(bar.permille, 375);
        assert!(!bar.over);
        assert!(bar.has_share);
        assert!(bar.notes.is_empty());
        // The month is still running: spending is counted up to today.
        assert_eq!(
            (bar.first_day.as_str(), bar.last_day.as_str()),
            ("2026-10-01", "2026-10-31")
        );
        assert_eq!(bar.counted_to, "2026-10-19");
        assert!(bar.running);
        // In the month before the budget had not started: an empty bar of
        // its first month, all of it left (D-710).
        let september = bars(&f.book, &TestLocale, "2026-09", &[]).expect("bars");
        assert_eq!(september.len(), 1);
        let later = &september[0];
        assert!(later.starts_later && !later.running);
        assert_eq!(
            (later.first_day.as_str(), later.last_day.as_str()),
            ("2026-10-01", "2026-10-31")
        );
        assert_eq!(later.spent, "0,00 €");
        assert_eq!(later.left, "400,00 €");
        assert_eq!(later.used, "0,0 %");
        assert_eq!(later.permille, 0);
        assert!(!bar.starts_later);
    }

    #[test]
    fn spending_beyond_the_amount_is_said_as_an_amount_over_and_the_bar_stays_full() {
        let mut f = monthly_food();
        let food = f.food.clone();
        f.spend(&food, day(2026, 10, 5), 45_000);
        let bar = bars(&f.book, &TestLocale, "this-month", &[])
            .expect("bars")
            .remove(0);
        assert!(bar.over);
        assert_eq!(bar.left, "-50,00 €");
        assert_eq!(bar.used, "112,5 %");
        // The fill stops at the full bar; the figure says how far it went.
        assert_eq!(bar.permille, 1000);
        assert_eq!(
            bar.notes,
            [NoteRow {
                word: "over",
                amount: "50,00 €".into(),
                negative: false
            }]
        );
    }

    #[test]
    fn what_is_carried_in_is_a_note_with_the_way_it_went() {
        let mut f = fixture();
        let food = f.food.clone();
        f.add(&food, "month", "400,00", true, "2026-09-01");
        f.spend(&food, day(2026, 9, 10), 10_000);
        let bar = bars(&f.book, &TestLocale, "this-month", &[])
            .expect("bars")
            .remove(0);
        assert_eq!(bar.carried_in, "300,00 €");
        assert_eq!(bar.available, "700,00 €");
        assert_eq!(
            bar.notes,
            [NoteRow {
                word: "carried-in",
                amount: "300,00 €".into(),
                negative: false
            }]
        );
        // Overspending moves on as less to spend, written without a sign.
        f.spend(&food, day(2026, 9, 11), 40_000);
        let bar = bars(&f.book, &TestLocale, "this-month", &[])
            .expect("bars")
            .remove(0);
        assert_eq!(bar.carried_in, "-100,00 €");
        assert_eq!(bar.available, "300,00 €");
        assert_eq!(
            bar.notes,
            [NoteRow {
                word: "carried-in",
                amount: "100,00 €".into(),
                negative: true
            }]
        );
    }

    #[test]
    fn only_the_chosen_categories_have_bars() {
        let mut f = monthly_food();
        let rent = f.rent.clone();
        f.add(&rent, "month", "800,00", false, "2026-10-01");
        assert_eq!(
            bars(&f.book, &TestLocale, "this-month", &[]).unwrap().len(),
            2
        );
        let only = bars(&f.book, &TestLocale, "this-month", &[f.rent.clone()]).unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name, "Rent");
        // A category inside the chosen one counts with it.
        let groceries = bars(&f.book, &TestLocale, "this-month", &[f.groceries.clone()]).unwrap();
        assert!(groceries.is_empty());
        assert_eq!(count(&f.book).unwrap(), 2);
    }

    #[test]
    fn a_budget_that_cannot_be_set_says_why_in_the_stores_words() {
        let mut f = fixture();
        fn new<'a>(categories: &'a [String], amount: &'a str) -> NewBudget<'a> {
            NewBudget {
                name: "",
                categories,
                accounts: &[],
                every: "month",
                amount,
                carry_over: false,
                first_day: "",
                last_day: "",
            }
        }
        let income = add_budget(
            &mut f.book,
            &new(&[f.salary.clone()], "100,00"),
            &TestLocale,
        )
        .expect_err("income has no budget");
        assert_eq!(f.code(&income).0, "store.budget-not-spending");
        let zero =
            add_budget(&mut f.book, &new(&[f.food.clone()], "0"), &TestLocale).expect_err("zero");
        assert_eq!(
            f.code(&zero),
            ("store.budget-amount", vec![("reason", "zero".to_owned())])
        );
        let text = add_budget(&mut f.book, &new(&[f.food.clone()], "plenty"), &TestLocale)
            .expect_err("not an amount");
        assert_eq!(f.code(&text).0, "input.malformed-amount");
        let unknown = add_budget(&mut f.book, &new(&["nothing".to_owned()], "5"), &TestLocale)
            .expect_err("no such category");
        assert_eq!(f.code(&unknown).0, "input.no-such-entry");
        let food = [f.food.clone()];
        let from_another_start = NewBudget {
            name: "",
            first_day: "2026-10-05",
            ..new(&food, "5")
        };
        let off_start = add_budget(&mut f.book, &from_another_start, &TestLocale)
            .expect_err("not the first day of a month");
        assert_eq!(
            f.code(&off_start),
            (
                "store.budget-period",
                vec![("reason", "not-period-start".to_owned())]
            )
        );
        // Nothing was kept by any of them.
        assert_eq!(count(&f.book).unwrap(), 0);
        // A second budget on the same category overlaps the first.
        f.add(&f.food.clone(), "month", "400,00", false, "");
        let again = add_budget(&mut f.book, &new(&[f.food.clone()], "5"), &TestLocale)
            .expect_err("one at a time");
        assert_eq!(f.code(&again).0, "store.budget-overlap");
    }

    /// A ticked budget that has ended still has a bar: its last period,
    /// complete, marked ended (D-710).
    #[test]
    fn a_budget_that_has_ended_shows_its_last_period() {
        let mut f = fixture();
        let new = NewBudget {
            name: "",
            categories: &[f.food.clone()],
            accounts: &[],
            every: "range",
            amount: "250,00",
            carry_over: false,
            first_day: "2026-09-01",
            last_day: "2026-09-30",
        };
        add_budget(&mut f.book, &new, &TestLocale).expect("a range");
        let food = f.food.clone();
        f.spend(&food, day(2026, 9, 10), 4_000);
        let shown = bars(&f.book, &TestLocale, "this-month", &[]).expect("bars");
        assert_eq!(shown.len(), 1);
        let bar = &shown[0];
        assert!(bar.ended && !bar.starts_later && !bar.running);
        assert_eq!(
            (bar.first_day.as_str(), bar.last_day.as_str()),
            ("2026-09-01", "2026-09-30")
        );
        assert_eq!(bar.spent, "40,00 €");
        assert_eq!(bar.left, "210,00 €");
        let page = view(&f.book, &TestLocale).expect("view");
        assert_eq!(page.budgets[0].state, "ended");
        assert!(page.budgets[0].bar.as_ref().is_some_and(|bar| bar.ended));
    }

    #[test]
    fn a_range_takes_typed_days_and_never_carries_over() {
        let mut f = fixture();
        let new = NewBudget {
            name: "",
            categories: &[f.food.clone()],
            accounts: &[],
            every: "range",
            amount: "250,00",
            carry_over: true,
            first_day: "2026-12-01",
            last_day: "2026-12-24",
        };
        add_budget(&mut f.book, &new, &TestLocale).expect("a range");
        let shown = view(&f.book, &TestLocale).expect("view");
        let row = &shown.budgets[0];
        assert_eq!(row.every, "range");
        assert!(!row.carry_over);
        assert_eq!(row.state, "upcoming");
        assert_eq!(
            (row.first_day.as_str(), row.last_day.as_str()),
            ("2026-12-01", "2026-12-24")
        );
        // Not started yet: an empty bar with all of it left (M6-B237, D-710).
        let bar = row.bar.as_ref().expect("an empty bar");
        assert!(bar.starts_later);
        assert_eq!(
            (bar.first_day.as_str(), bar.last_day.as_str()),
            ("2026-12-01", "2026-12-24")
        );
        assert_eq!(
            (
                bar.spent.as_str(),
                bar.available.as_str(),
                bar.left.as_str()
            ),
            ("0,00 €", "250,00 €", "250,00 €")
        );
        assert!(bar.has_share && bar.notes.is_empty());
        assert!(!row.can_stop);
        assert_eq!(row.changes.len(), 1);
        let no_end = NewBudget {
            name: "",
            last_day: "soon",
            ..new
        };
        let error = add_budget(&mut f.book, &no_end, &TestLocale).expect_err("not a date");
        assert_eq!(
            f.code(&error),
            (
                "input.malformed-date",
                vec![("example", "2026-10-19".to_owned())]
            )
        );
    }

    #[test]
    fn a_change_applies_from_a_period_on_and_earlier_periods_keep_what_they_had() {
        let mut f = fixture();
        let food = f.food.clone();
        f.add(&food, "month", "400,00", false, "2026-08-01");
        let id = f.book.budgets().unwrap()[0].id.to_string();
        change_budget(
            &mut f.book,
            &id,
            &amount("2026-10-01", "500,00"),
            &TestLocale,
        )
        .expect("changed");
        let planned = |key: &str| {
            bars(&f.book, &TestLocale, key, &[]).unwrap()[0]
                .planned
                .clone()
        };
        assert_eq!(planned("2026-09"), "400,00 €");
        assert_eq!(planned("this-month"), "500,00 €");
        // The page offers the periods to change from, newest first, ending
        // with the first one, which corrects a budget from its start.
        let row = &view(&f.book, &TestLocale).unwrap().budgets[0];
        let days: Vec<&str> = row.changes.iter().map(|c| c.day.as_str()).collect();
        assert_eq!(days, ["2026-10-01", "2026-09-01", "2026-08-01"]);
        assert!(row.changes[0].current);
        assert_eq!(row.amount, "500,00 €");
        assert_eq!(row.amount_input, "500,00");
        // A day that is not the start of a period is refused.
        let error = change_budget(&mut f.book, &id, &amount("2026-10-05", "1"), &TestLocale)
            .expect_err("not a period start");
        assert_eq!(f.code(&error).0, "store.budget-period");
        let error = change_budget(
            &mut f.book,
            "nothing",
            &amount("2026-10-01", "1"),
            &TestLocale,
        )
        .expect_err("no such budget");
        assert_eq!(f.code(&error).0, "input.no-such-entry");
    }

    #[test]
    fn stop_ends_after_this_period_and_delete_takes_the_budget_with_it() {
        let mut f = monthly_food();
        let id = f.book.budgets().unwrap()[0].id.to_string();
        let row = view(&f.book, &TestLocale).unwrap().budgets.remove(0);
        assert!(row.can_stop);
        assert_eq!(row.state, "running");
        stop_budget(&mut f.book, &id, &TestLocale).expect("stopped");
        let row = view(&f.book, &TestLocale).unwrap().budgets.remove(0);
        assert!(!row.can_stop);
        assert_eq!(row.last_day, "2026-10-31");
        // The bar of the period it covers stays.
        assert!(row.bar.is_some());
        remove_budget(&mut f.book, &id).expect("removed");
        assert!(view(&f.book, &TestLocale).unwrap().budgets.is_empty());
        let error = remove_budget(&mut f.book, &id).expect_err("already gone");
        assert_eq!(f.code(&error).0, "input.no-such-entry");
    }

    #[test]
    fn the_sheet_offers_the_next_period_the_current_one_and_those_before() {
        let months = start_choices("month", &TestLocale).expect("choices");
        assert_eq!(months.len(), 26);
        assert_eq!(months[0].day, "2026-11-01");
        assert!(!months[0].current);
        assert_eq!(months[1].day, "2026-10-01");
        assert!(months[1].current);
        assert_eq!(months[2].day, "2026-09-01");
        assert_eq!(
            (months[1].first_day.as_str(), months[1].last_day.as_str()),
            ("2026-10-01", "2026-10-31")
        );
        let quarters = start_choices("quarter", &TestLocale).expect("choices");
        assert_eq!(quarters[0].day, "2027-01-01");
        assert_eq!(quarters[1].day, "2026-10-01");
        assert_eq!(quarters[1].last_day, "2026-12-31");
        let years = start_choices("year", &TestLocale).expect("choices");
        assert_eq!(years[1].day, "2026-01-01");
        // A range has no list: the user types its days.
        assert!(
            start_choices("range", &TestLocale)
                .expect("choices")
                .is_empty()
        );
        assert!(matches!(
            start_choices("weekly", &TestLocale),
            Err(BudgetsError::UnknownName("every"))
        ));
    }

    #[test]
    fn a_budget_can_repeat_every_salary_month_from_its_day() {
        // On 19 October the salary month from day 25 runs from 25 September
        // to 24 October (D-651).
        let months = start_choices("salary-month-25", &TestLocale).expect("choices");
        assert_eq!(months[0].day, "2026-10-25");
        assert!(!months[0].current);
        assert_eq!(months[1].day, "2026-09-25");
        assert!(months[1].current);
        assert_eq!(months[1].last_day, "2026-10-24");
        assert_eq!(months[2].day, "2026-08-25");
        // Day 1 is the calendar month; a day outside a month is refused.
        assert_eq!(
            start_choices("salary-month-1", &TestLocale).unwrap()[1].day,
            "2026-10-01"
        );
        for word in [
            "salary-month-0",
            "salary-month-32",
            "salary-month",
            "salary-month-x",
        ] {
            assert!(matches!(
                start_choices(word, &TestLocale),
                Err(BudgetsError::UnknownName("every"))
            ));
        }

        let mut f = fixture();
        let food = f.food.clone();
        f.spend(&food, day(2026, 9, 24), 1_000);
        f.spend(&food, day(2026, 10, 1), 2_000);
        let new = NewBudget {
            name: "",
            categories: &[food],
            accounts: &[],
            every: "salary-month-25",
            amount: "100",
            carry_over: false,
            first_day: "",
            last_day: "",
        };
        add_budget(&mut f.book, &new, &TestLocale).expect("added");
        let row = &view(&f.book, &TestLocale).unwrap().budgets[0];
        assert_eq!((row.every, row.salary_day), ("salary-month", 25));
        assert_eq!(row.first_day, "2026-09-25");
        let bar = row.bar.as_ref().expect("a running bar");
        assert_eq!((bar.every, bar.salary_day), ("salary-month", 25));
        assert_eq!(
            (bar.first_day.as_str(), bar.last_day.as_str()),
            ("2026-09-25", "2026-10-24")
        );
        // 24 September is in the salary month before.
        assert_eq!(bar.spent, "20,00 €");
    }

    #[test]
    fn what_a_new_budget_starts_with_is_one_of_the_offered_periods() {
        let mut f = fixture();
        let choice = start_choices("month", &TestLocale).unwrap()[2].clone();
        let new = NewBudget {
            name: "",
            categories: &[f.food.clone()],
            accounts: &[],
            every: "month",
            amount: "10",
            carry_over: false,
            first_day: &choice.day,
            last_day: "",
        };
        add_budget(&mut f.book, &new, &TestLocale).expect("added");
        let row = &view(&f.book, &TestLocale).unwrap().budgets[0];
        assert_eq!(row.first_day, "2026-09-01");
        // The first period is the one before the current one, so a change
        // can reach it.
        assert_eq!(row.changes.last().unwrap().day, "2026-09-01");
    }

    #[test]
    fn a_budget_over_several_categories_from_one_account_has_one_bar() {
        let mut f = fixture();
        let savings = AccountId::generate();
        f.book
            .add_account(&Account {
                id: savings,
                name: "Savings".into(),
                kind: AccountKind::Asset,
                currency: EUR,
                account_type: Some(AccountType::Savings),
                parent: None,
                closed: false,
                number: None,
                notes: None,
                institution: None,
            })
            .expect("account added");
        let new = NewBudget {
            name: "",
            categories: &[f.food.clone(), f.rent.clone()],
            accounts: &[f.checking.to_string()],
            every: "month",
            amount: "1000,00",
            carry_over: false,
            first_day: "",
            last_day: "",
        };
        add_budget(&mut f.book, &new, &TestLocale).expect("added");
        let (groceries, rent) = (f.groceries.clone(), f.rent.clone());
        f.spend(&groceries, day(2026, 10, 5), 12_000);
        f.spend(&rent, day(2026, 10, 6), 80_000);
        // Money paid from another account is not counted.
        let rent_id = f
            .book
            .categories()
            .expect("categories")
            .into_iter()
            .map(|entry| entry.category.id)
            .find(|id| id.to_string() == rent)
            .expect("Rent");
        let from_savings = Transaction::new(
            TransactionId::generate(),
            day(2026, 10, 7),
            None,
            vec![
                Split::new(savings, Money::from_minor_units(-5_000, EUR)),
                Split::new(rent_id, Money::from_minor_units(5_000, EUR)),
            ],
        )
        .expect("balanced");
        f.book.add_transaction(&from_savings).expect("added");
        let bar = bars(&f.book, &TestLocale, "this-month", &[])
            .expect("bars")
            .remove(0);
        assert_eq!(bar.name, "Food, Rent");
        assert_eq!(bar.categories, [f.food.clone(), f.rent.clone()]);
        assert_eq!(bar.accounts, "Checking");
        assert_eq!(bar.spent, "920,00 €");
        let row = view(&f.book, &TestLocale).unwrap().budgets.remove(0);
        assert_eq!(row.name, "Food, Rent");
        assert_eq!(row.accounts, "Checking");
        // The page offers the open accounts by name.
        let shown = view(&f.book, &TestLocale).unwrap();
        let names: Vec<&str> = shown.accounts.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["Checking", "Savings"]);
        // No category, or an account that isn't the user's, is refused.
        let none = NewBudget {
            name: "",
            categories: &[],
            ..new
        };
        let error = add_budget(&mut f.book, &none, &TestLocale).expect_err("no category");
        assert_eq!(
            f.code(&error),
            (
                "store.budget-choice",
                vec![("reason", "no-category".to_owned())]
            )
        );
        let foreign = NewBudget {
            name: "",
            categories: &[f.salary.clone()],
            accounts: &[f.food.clone()],
            ..new
        };
        let error = add_budget(&mut f.book, &foreign, &TestLocale).expect_err("not an account");
        assert_eq!(f.code(&error).0, "input.no-such-entry");
    }

    #[test]
    fn the_tile_shows_only_the_budgets_it_names() {
        let mut f = monthly_food();
        let rent = f.rent.clone();
        f.add(&rent, "month", "800,00", false, "2026-10-01");
        let listed = choices(&f.book).expect("choices");
        let names: Vec<&str> = listed.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Food", "Rent"]);
        let rent_budget = listed[1].id.clone();
        let (shown, _) = bars_filtered(
            &f.book,
            &TestLocale,
            "this-month",
            &[],
            &[rent_budget.clone(), "gone".to_owned()],
            &Scope::default(),
        )
        .expect("bars");
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].budget, rent_budget);
        // Nothing named: every budget.
        let (all, _) = bars_filtered(
            &f.book,
            &TestLocale,
            "this-month",
            &[],
            &[],
            &Scope::default(),
        )
        .expect("bars");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn the_page_lists_only_spending_categories_to_choose_from() {
        let f = fixture();
        let shown = view(&f.book, &TestLocale).expect("view");
        let names: Vec<&str> = shown.categories.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Food", "Groceries", "Rent"]);
    }

    /// A budget can have a name the user gives it (M6-B203, D-664): the
    /// page, the bars and the tile's choice show it, the categories stay
    /// in their own field, and a change of only the name leaves every
    /// planned amount as it was.
    #[test]
    fn a_named_budget_is_shown_by_its_name_and_a_new_name_keeps_its_amounts() {
        let mut f = fixture();
        let food = f.food.clone();
        let new = NewBudget {
            name: "  Weekly shop ",
            categories: std::slice::from_ref(&food),
            accounts: &[],
            every: "month",
            amount: "400,00",
            carry_over: false,
            first_day: "2026-10-01",
            last_day: "",
        };
        let id = add_budget(&mut f.book, &new, &TestLocale)
            .expect("added")
            .to_string();
        let bar = &bars(&f.book, &TestLocale, "this-month", &[]).expect("bars")[0];
        assert_eq!(bar.name, "Weekly shop");
        assert_eq!(bar.own_name, "Weekly shop");
        assert_eq!(bar.category_names, "Food");
        let row = &view(&f.book, &TestLocale).expect("view").budgets[0];
        assert_eq!(
            (
                row.name.as_str(),
                row.own_name.as_str(),
                row.category_names.as_str()
            ),
            ("Weekly shop", "Weekly shop", "Food")
        );
        assert_eq!(choices(&f.book).expect("choices")[0].name, "Weekly shop");

        // November's amount is planned ahead; a new name alone keeps it.
        change_budget(
            &mut f.book,
            &id,
            &amount("2026-11-01", "450,00"),
            &TestLocale,
        )
        .expect("planned ahead");
        let before = f.book.budgets().expect("budgets")[0].amounts.clone();
        assert_eq!(before.len(), 2);
        let rename = BudgetChange {
            name: "Food at home",
            changes_amount: false,
            from_day: "2026-10-01",
            amount: "400,00",
            carry_over: false,
            counts: None,
        };
        let changes = f.book.history().expect("history").len();
        change_budget(&mut f.book, &id, &rename, &TestLocale).expect("renamed");
        let budget = &f.book.budgets().expect("budgets")[0];
        assert_eq!(budget.name.as_deref(), Some("Food at home"));
        assert_eq!(budget.amounts, before);
        assert_eq!(f.book.history().expect("history").len(), changes + 1);

        // A name and an amount together are one change, which one Undo
        // takes back.
        let both = BudgetChange {
            name: "",
            ..amount("2026-10-01", "380,00")
        };
        change_budget(&mut f.book, &id, &both, &TestLocale).expect("changed");
        let row = &view(&f.book, &TestLocale).expect("view").budgets[0];
        assert_eq!((row.name.as_str(), row.own_name.as_str()), ("Food", ""));
        assert_eq!(f.book.history().expect("history").len(), changes + 2);
        f.book.undo().expect("undone");
        let budget = &f.book.budgets().expect("budgets")[0];
        assert_eq!(budget.name.as_deref(), Some("Food at home"));
        assert_eq!(budget.amounts, before);

        // A name the book can't keep says why, and nothing changes.
        let error = change_budget(
            &mut f.book,
            &id,
            &BudgetChange {
                name: "Two\nlines",
                ..amount("2026-10-01", "1,00")
            },
            &TestLocale,
        )
        .expect_err("refused");
        assert_eq!(f.code(&error).0, "store.budget-name");
        assert_eq!(f.book.budgets().expect("budgets")[0].amounts, before);
    }

    #[test]
    fn what_a_budget_counts_changes_from_this_period_on_and_the_page_says_what_it_counted() {
        // D-689: September keeps counting food; from October on it counts
        // food and rent, paid from checking.
        let mut f = fixture();
        let (food, rent) = (f.food.clone(), f.rent.clone());
        f.add(&food, "month", "400,00", false, "2026-09-01");
        let id = f.book.budgets().expect("budgets")[0].id.to_string();
        let row = &view(&f.book, &TestLocale).expect("view").budgets[0];
        assert_eq!(row.counts_from, "2026-10-01");
        assert!(row.history.is_empty());
        assert!(row.account_ids.is_empty());
        let checking = f.checking.to_string();
        let (categories, accounts) = ([food.clone(), rent.clone()], [checking.clone()]);
        let mut change = BudgetChange {
            name: "",
            changes_amount: false,
            from_day: "2026-09-01",
            amount: "400,00",
            carry_over: false,
            counts: Some((&categories, &accounts)),
        };
        // Not from a period that has passed.
        let error = change_budget(&mut f.book, &id, &change, &TestLocale).expect_err("refused");
        assert_eq!(
            f.code(&error),
            (
                "store.budget-period",
                vec![("reason", "before-current-period".to_owned())]
            )
        );
        let changes = f.book.history().expect("history").len();
        change.from_day = "2026-10-01";
        change_budget(&mut f.book, &id, &change, &TestLocale).expect("changed");
        assert_eq!(f.book.history().expect("history").len(), changes + 1);
        let row = &view(&f.book, &TestLocale).expect("view").budgets[0];
        assert_eq!(row.categories, [food.clone(), rent.clone()]);
        assert_eq!(row.category_names, "Food, Rent");
        assert_eq!(row.account_ids, [checking]);
        assert_eq!(row.accounts, "Checking");
        assert_eq!(
            row.history,
            [CountsLine {
                earlier: true,
                day: "2026-09-30".into(),
                category_names: "Food".into(),
                accounts: String::new(),
            }]
        );
        // The same set again is no change.
        change_budget(&mut f.book, &id, &change, &TestLocale).expect("nothing to do");
        assert_eq!(f.book.history().expect("history").len(), changes + 1);
        // One Undo takes it back.
        f.book.undo().expect("undone");
        let row = &view(&f.book, &TestLocale).expect("view").budgets[0];
        assert_eq!(row.categories, [food]);
        assert!(row.history.is_empty());
    }
}
