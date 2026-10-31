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

use kassensturz_core::{AccountId, Budget, BudgetEvery, BudgetId, CurrencyCode, Date, Report};
use kassensturz_report::{
    BudgetGrouping, BudgetNote, BudgetPeriod, BudgetUtilization, Filter, ReportError, Source,
    TextFormat, budget_utilization,
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
            ChartsError::Chart(_) => Self::UnknownName("period"),
        }
    }
}

/// A budget's period word, as QML says it (the keys of D-344).
const fn every_key(every: BudgetEvery) -> &'static str {
    match every {
        BudgetEvery::Month => "month",
        BudgetEvery::Quarter => "quarter",
        BudgetEvery::Year => "year",
        BudgetEvery::Range => "range",
    }
}

fn every_of(key: &str) -> Result<BudgetEvery, BudgetsError> {
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
    /// `over`, `nothing-to-spend` or `carried-in`.
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
    /// The category's ID as text.
    pub category: String,
    /// The category's names from the top-level category down, joined.
    pub name: String,
    /// `month`, `quarter`, `year` or `range`.
    pub every: &'static str,
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

fn bar<L: Locale>(
    shown: &BudgetUtilization,
    period: &BudgetPeriod,
    locale: &L,
    text: &AppText<'_, L>,
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
                BudgetNote::NothingToSpend => (String::new(), false),
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
        category: shown.category.to_string(),
        name: shown.path.join(" › "),
        every: every_key(shown.every),
        currency: shown.currency.to_string(),
        first_day: locale.short_date(period.first_day),
        last_day: locale.short_date(period.last_day),
        counted_to: locale.short_date(period.counted_to),
        running: period.counted_to < period.last_day,
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
/// earlier. `only` are the IDs of categories: only the budgets of those and
/// the categories inside them (empty: every budget). A budget that has not
/// started or has ended by then has no bar.
pub fn bars_filtered(
    book: &Book,
    locale: &impl Locale,
    period_key: &str,
    only: &[String],
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
    let shown = budget_utilization(&source, period, today, BudgetGrouping::Budget, &filter)?;
    let text = AppText::new(book, locale);
    let bars = shown
        .iter()
        .flat_map(|budget| {
            budget
                .periods
                .iter()
                .map(|period| bar(budget, period, locale, &text))
        })
        .collect();
    Ok((bars, view))
}

/// As [`bars_filtered`], with nothing narrowed by a filter bar.
#[cfg(test)]
pub fn bars(
    book: &Book,
    locale: &impl Locale,
    period_key: &str,
    only: &[String],
) -> Result<Vec<Bar>, BudgetsError> {
    bars_filtered(book, locale, period_key, only, &Scope::default()).map(|(bars, _)| bars)
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
    /// The category's ID as text.
    pub category: String,
    /// The category's names from the top-level category down, joined.
    pub name: String,
    /// `month`, `quarter`, `year` or `range`.
    pub every: &'static str,
    /// The category's currency.
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
    /// This period's bar; none for a budget that is not running.
    pub bar: Option<Bar>,
}

/// Everything the page shows.
#[derive(Debug)]
pub struct BudgetsView {
    /// The budgets, in the order of the category tree.
    pub budgets: Vec<BudgetRow>,
    /// The spending categories a budget can be set on, in tree order.
    pub categories: Vec<CategoryRow>,
}

/// The budgets of `book` with the bars of the period that contains today.
pub fn view(book: &Book, locale: &impl Locale) -> Result<BudgetsView, BudgetsError> {
    let today = locale.today();
    let categories: Vec<CategoryRow> = category_view(book)?.rows;
    let bars = bars(book, locale, "this-month", &[])?;
    let text = AppText::new(book, locale);
    let mut rows = Vec::new();
    for budget in book.budgets()? {
        let Some(category) = categories
            .iter()
            .find(|row| row.id == budget.category.to_string())
        else {
            continue;
        };
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
        let planned = budget
            .amount_for(start)
            .or_else(|| budget.amounts.first())
            .ok_or(StoreError::Corrupt(
                kassensturz_store::Corruption::BrokenReferences,
            ))?;
        let currency = book.account(budget.category)?.currency;
        let ids = budget.id.to_string();
        rows.push(BudgetRow {
            id: ids.clone(),
            category: category.id.clone(),
            name: category.path.clone(),
            every: every_key(budget.every),
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
    Ok(BudgetsView {
        budgets: rows,
        categories,
    })
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
    /// The spending category's ID as text.
    pub category: &'a str,
    /// `month`, `quarter`, `year` or `range`.
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
    let (category, currency) = category_of(book, new.category)?;
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
    Ok(book.add_budget(category, every, first_day, last_day, amount, carry_over)?)
}

/// Plans `amount_text` with `carry_over` for the budget from the period that
/// starts on `from_day` (ISO 8601 text, one of the budget's
/// `changes`) on (D-345, D-361).
pub fn change_budget(
    book: &mut Book,
    id: &str,
    from_day: &str,
    amount_text: &str,
    carry_over: bool,
    locale: &impl Locale,
) -> Result<(), BudgetsError> {
    let budget = budget_of(book, id)?;
    let currency = book.account(budget.category)?.currency;
    let amount = read_amount(book, amount_text, currency, locale)?;
    let from = iso_day(from_day)?;
    let carry_over = carry_over && budget.every.repeats();
    Ok(book.edit_budget_amount(budget.id, from, amount, carry_over)?)
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
                category,
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
        // The month before is complete and had nothing spent.
        let september = bars(&f.book, &TestLocale, "2026-09", &[]).expect("bars");
        assert!(september.is_empty());
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
        fn new<'a>(category: &'a str, amount: &'a str) -> NewBudget<'a> {
            NewBudget {
                category,
                every: "month",
                amount,
                carry_over: false,
                first_day: "",
                last_day: "",
            }
        }
        let income = add_budget(&mut f.book, &new(&f.salary.clone(), "100,00"), &TestLocale)
            .expect_err("income has no budget");
        assert_eq!(f.code(&income).0, "store.budget-not-spending");
        let zero =
            add_budget(&mut f.book, &new(&f.food.clone(), "0"), &TestLocale).expect_err("zero");
        assert_eq!(
            f.code(&zero),
            ("store.budget-amount", vec![("reason", "zero".to_owned())])
        );
        let text = add_budget(&mut f.book, &new(&f.food.clone(), "plenty"), &TestLocale)
            .expect_err("not an amount");
        assert_eq!(f.code(&text).0, "input.malformed-amount");
        let unknown = add_budget(&mut f.book, &new("nothing", "5"), &TestLocale)
            .expect_err("no such category");
        assert_eq!(f.code(&unknown).0, "input.no-such-entry");
        let food = f.food.clone();
        let from_another_start = NewBudget {
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
        let again = add_budget(&mut f.book, &new(&f.food.clone(), "5"), &TestLocale)
            .expect_err("one at a time");
        assert_eq!(f.code(&again).0, "store.budget-overlap");
    }

    #[test]
    fn a_range_takes_typed_days_and_never_carries_over() {
        let mut f = fixture();
        let new = NewBudget {
            category: &f.food.clone(),
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
        assert!(row.bar.is_none());
        assert!(!row.can_stop);
        assert_eq!(row.changes.len(), 1);
        let no_end = NewBudget {
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
        change_budget(&mut f.book, &id, "2026-10-01", "500,00", false, &TestLocale)
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
        let error = change_budget(&mut f.book, &id, "2026-10-05", "1", false, &TestLocale)
            .expect_err("not a period start");
        assert_eq!(f.code(&error).0, "store.budget-period");
        let error = change_budget(
            &mut f.book,
            "nothing",
            "2026-10-01",
            "1",
            false,
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
    fn what_a_new_budget_starts_with_is_one_of_the_offered_periods() {
        let mut f = fixture();
        let choice = start_choices("month", &TestLocale).unwrap()[2].clone();
        let new = NewBudget {
            category: &f.food.clone(),
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
    fn the_page_lists_only_spending_categories_to_choose_from() {
        let f = fixture();
        let shown = view(&f.book, &TestLocale).expect("view");
        let names: Vec<&str> = shown.categories.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Food", "Groceries", "Rent"]);
    }
}
