// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Planned payments (D-769, `docs/SCHEDULES.md`): a payment the user
//! expects on a day, repeating, such as rent or an insurance. The code word
//! is *schedule*; the UI says *planned payment*.
//!
//! A schedule is only a plan beside the book's transactions: it moves no
//! money and sends nothing (D-053). Its occurrences are computed
//! ([`due_dates`]), never stored; the book stores only the ones that were
//! decided, matched to an entry or skipped. This module decides when a
//! payment is due, in which state an occurrence is ([`OccurrenceState`]),
//! which entries fit which occurrence ([`match_schedules`]) and what is
//! left this month ([`forecast`], D-770). It changes nothing.
//!
//! What never decides a match: the order of the entries or schedules, their
//! IDs or the time they were imported. Where two entries fit an occurrence
//! equally well, or one entry fits two occurrences, nothing is matched and
//! the occurrence is reported as [`UnclearOccurrence`], for the user.

use std::collections::HashMap;

use jiff::ToSpan;
use jiff::civil::Weekday;

use crate::Date;
use crate::account::AccountId;
use crate::budget::salary_month_of;
use crate::currency::CurrencyCode;
use crate::id::define_id;
use crate::money::{Money, MoneyError};
use crate::recurring_move::{OtherParty, compact_iban, compact_name};
use crate::transaction::TransactionId;
use crate::transfer::{MAX_BUSINESS_DAYS, business_days_apart, days_between};

define_id!(
    /// Identifies one planned payment within a book.
    ScheduleId
);

/// The most characters a planned payment's name may have.
pub const MAX_SCHEDULE_NAME_CHARS: usize = 60;

/// The most characters of the other party's name or IBAN a planned payment
/// keeps. A longer text is cut on a character boundary, never refused, so
/// "Plan this again…" works on every importer's entries; both sides are cut
/// the same way when they are compared.
pub const MAX_SCHEDULE_PARTY_CHARS: usize = 140;

/// The most planned payments a book holds.
pub const MAX_SCHEDULES: usize = 200;

/// The highest "every" a planned payment may have: every 52 weeks, months
/// or years.
pub const MAX_SCHEDULE_EVERY: u8 = 52;

/// The most due dates one call of [`due_dates`] yields, a limit against
/// runaway loops.
pub const MAX_DUE_DATES: usize = 1000;

/// How many years after its `from` date [`due_dates`] looks at most, a
/// limit against runaway loops.
pub const MAX_DUE_YEARS: i16 = 5;

/// How many years back matching and the "Did this happen?" list look for
/// open occurrences: older ones stay open, and the user can still match
/// them by hand. Shorter than [`MAX_DUE_YEARS`], so the look back and the
/// days up to the month's end fit into one call of [`due_dates`].
pub const LOOK_BACK_YEARS: i16 = 4;

/// The most months [`forecast`] projects after the current one.
pub const MAX_FORECAST_MONTHS: usize = 12;

/// How a planned payment repeats: every `every` weeks, months or years.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScheduleUnit {
    /// Every `every` weeks.
    Week,
    /// Every `every` months: a quarter is every 3 months.
    Month,
    /// Every `every` years.
    Year,
}

impl ScheduleUnit {
    /// Every unit, in the order the UI lists them.
    pub const ALL: [Self; 3] = [Self::Week, Self::Month, Self::Year];

    /// The word the book and the complete export store for it.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    /// The unit with this word.
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|unit| unit.word() == word)
    }
}

/// A payment the user expects on a day, repeating (D-769).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    /// Its ID.
    pub id: ScheduleId,
    /// What it is for, 1 to [`MAX_SCHEDULE_NAME_CHARS`] characters, one
    /// line, without spaces around it.
    pub name: String,
    /// The user's account it is paid from or into.
    pub account: AccountId,
    /// The amount, never zero: negative for money that goes out. Its
    /// currency is the account's.
    pub amount: Money,
    /// How much a matching entry may differ from `amount`, with the same
    /// sign: 0 or more, less than the amount itself. In `amount`'s
    /// currency.
    pub tolerance: Money,
    /// The category to give a matching entry that has none yet.
    pub category: Option<AccountId>,
    /// The other party's IBAN as the bank shows it, if the user wants it
    /// to be the same.
    pub party_iban: Option<String>,
    /// The other party's name as the bank shows it, if the user wants it to
    /// be the same.
    pub party_name: Option<String>,
    /// Weeks, months or years.
    pub unit: ScheduleUnit,
    /// How many of them between two payments, 1 to [`MAX_SCHEDULE_EVERY`].
    pub every: u8,
    /// The first due date, the one the pattern is counted from.
    pub anchor: Date,
    /// The first date an occurrence counts: occurrences before it never
    /// exist, so a plan made today is not "overdue" for last year. Never
    /// before `anchor`.
    pub track_from: Date,
    /// The last date an occurrence may have, if the plan ends.
    pub ends_on: Option<Date>,
    /// Whether the user paused it: a paused plan is not expected, matched
    /// or counted.
    pub paused: bool,
}

/// Why a planned payment can't be kept as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    /// The name is empty, longer than [`MAX_SCHEDULE_NAME_CHARS`], has
    /// spaces around it or more than one line.
    Name,
    /// The amount is zero.
    AmountZero,
    /// "Every" is not 1 to [`MAX_SCHEDULE_EVERY`].
    EveryRange,
    /// It ends before its first due date, or starts counting before it.
    EndBeforeStart,
    /// The tolerance is negative, in another currency, or not less than the
    /// amount.
    ToleranceRange,
    /// The other party's name or IBAN is longer than
    /// [`MAX_SCHEDULE_PARTY_CHARS`], or the IBAN has no character.
    Party,
    /// The amount to book an occurrence with is zero, in another currency
    /// than the plan, or goes the other way.
    BookedAmount,
}

impl ScheduleError {
    /// The word `docs/ERROR_CODES.md` gives for it.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::AmountZero => "amount-zero",
            Self::EveryRange => "every-range",
            Self::EndBeforeStart => "end-before-start",
            Self::ToleranceRange => "tolerance-range",
            Self::Party => "party",
            Self::BookedAmount => "booked-amount",
        }
    }
}

/// `text` cut to at most [`MAX_SCHEDULE_PARTY_CHARS`] characters, on a
/// character boundary; text of nothing but spaces counts as none.
pub fn cut_party(text: Option<&str>) -> Option<String> {
    text.filter(|text| !text.trim().is_empty())
        .map(|text| text.chars().take(MAX_SCHEDULE_PARTY_CHARS).collect())
}

impl Schedule {
    /// Checks what a planned payment must be by itself (3.1). Whether its
    /// account is an open account of the user's in the amount's currency,
    /// and its category a category, is for the book to check.
    pub fn check(&self) -> Result<(), ScheduleError> {
        let name_chars = self.name.chars().count();
        if name_chars == 0
            || name_chars > MAX_SCHEDULE_NAME_CHARS
            || self.name.trim() != self.name
            || self.name.contains(['\n', '\r'])
        {
            return Err(ScheduleError::Name);
        }
        if self.amount.is_zero() {
            return Err(ScheduleError::AmountZero);
        }
        if !(1..=MAX_SCHEDULE_EVERY).contains(&self.every) {
            return Err(ScheduleError::EveryRange);
        }
        if self.track_from < self.anchor || self.ends_on.is_some_and(|end| end < self.anchor) {
            return Err(ScheduleError::EndBeforeStart);
        }
        let tolerance = self.tolerance.minor_units();
        if self.tolerance.currency() != self.amount.currency()
            || tolerance < 0
            || i128::from(tolerance) >= i128::from(self.amount.minor_units()).abs()
        {
            return Err(ScheduleError::ToleranceRange);
        }
        for party in [&self.party_iban, &self.party_name].into_iter().flatten() {
            if party.trim().is_empty() || party.chars().count() > MAX_SCHEDULE_PARTY_CHARS {
                return Err(ScheduleError::Party);
            }
        }
        Ok(())
    }

    /// Whether money goes out: the amount is negative.
    pub const fn is_outgoing(&self) -> bool {
        self.amount.is_negative()
    }

    /// Whether `party`, the other party an entry names, is compatible with
    /// the one this plan remembers (3.4 point 3): the same IBAN if it
    /// remembers one, spaces and capitals ignored; otherwise the same name
    /// if it remembers one, spaces and capitals ignored, both cut to
    /// [`MAX_SCHEDULE_PARTY_CHARS`]; otherwise any party.
    pub fn party_fits(&self, party: &OtherParty) -> bool {
        let cut = |text: &str| -> String { text.chars().take(MAX_SCHEDULE_PARTY_CHARS).collect() };
        if let Some(iban) = &self.party_iban {
            return party
                .iban
                .as_deref()
                .is_some_and(|given| compact_iban(&cut(given)) == compact_iban(&cut(iban)));
        }
        if let Some(name) = &self.party_name {
            return party
                .name
                .as_deref()
                .is_some_and(|given| compact_name(&cut(given)) == compact_name(&cut(name)));
        }
        true
    }

    /// The `k`-th counted date, from the anchor (never from the previous
    /// date, so 31 Jan, 28 Feb, 31 Mar): `None` outside the calendar.
    fn counted(&self, k: i64) -> Option<Date> {
        let every = i64::from(self.every);
        match self.unit {
            ScheduleUnit::Week => {
                let days = k.checked_mul(every)?.checked_mul(7)?;
                self.anchor.checked_add(days.days()).ok()
            }
            ScheduleUnit::Month => add_months(self.anchor, k.checked_mul(every)?),
            ScheduleUnit::Year => add_months(self.anchor, k.checked_mul(every)?.checked_mul(12)?),
        }
    }

    /// A `k` whose counted date is not after `date`, close below it, so
    /// [`due_dates`] needn't count every date from the anchor.
    fn k_before(&self, date: Date) -> i64 {
        if date <= self.anchor {
            return 0;
        }
        let every = i64::from(self.every);
        let months = |from: Date, to: Date| {
            (i64::from(to.year()) - i64::from(from.year())) * 12 + i64::from(to.month())
                - i64::from(from.month())
        };
        let steps = match self.unit {
            ScheduleUnit::Week => whole_days(self.anchor, date) / (7 * every),
            ScheduleUnit::Month => months(self.anchor, date) / every,
            ScheduleUnit::Year => months(self.anchor, date) / (12 * every),
        };
        (steps - 1).max(0)
    }
}

/// The calendar days from `from` to `to`, 0 if `to` is earlier.
fn whole_days(from: Date, to: Date) -> i64 {
    from.until(to)
        .map_or(0, |span| i64::from(span.get_days()))
        .max(0)
}

/// `date` moved by `months` months, its day cut back to the new month's
/// last day.
fn add_months(date: Date, months: i64) -> Option<Date> {
    let index = i64::from(date.year()) * 12 + i64::from(date.month()) - 1 + months;
    let year = i16::try_from(index.div_euclid(12)).ok()?;
    let month = i8::try_from(index.rem_euclid(12) + 1).ok()?;
    let first = Date::new(year, month, 1).ok()?;
    Date::new(year, month, date.day().min(first.days_in_month())).ok()
}

/// `date`, moved forward to Monday when it falls on a Saturday or Sunday,
/// because banks book on banking days. Public holidays are not considered
/// (D-030, D-749).
pub fn banking_day(date: Date) -> Option<Date> {
    match date.weekday() {
        Weekday::Saturday => date.checked_add(2.days()).ok(),
        Weekday::Sunday => date.checked_add(1.day()).ok(),
        _ => Some(date),
    }
}

/// `date` plus `count` banking days (Mondays to Fridays), counted as
/// [`business_days_apart`] counts them.
pub fn add_banking_days(date: Date, count: u32) -> Option<Date> {
    let mut day = date;
    let mut left = count;
    while left > 0 {
        day = day.tomorrow().ok()?;
        if !matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday) {
            left -= 1;
        }
    }
    Some(day)
}

/// `date` minus `count` banking days, the other way round from
/// [`add_banking_days`]: the earliest day that is at most `count` banking
/// days before `date`.
pub fn sub_banking_days(date: Date, count: u32) -> Option<Date> {
    let mut day = date;
    loop {
        let earlier = day.yesterday().ok()?;
        if business_days_apart(earlier, date, count).is_none() {
            return Some(day);
        }
        day = earlier;
    }
}

/// One occurrence of a planned payment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Due {
    /// The date counted from the anchor, before it was moved off a weekend.
    /// The book keeps a decided occurrence by it, so changing the weekend
    /// rule later never orphans one.
    pub counted: Date,
    /// The date it is due: `counted`, moved forward to Monday from a
    /// weekend. An occurrence belongs to this date's month.
    pub date: Date,
}

/// The occurrences of `schedule` whose (moved) date is from `from` to `to`,
/// both included, oldest first (3.2).
///
/// It stops at `ends_on` (counted dates), never yields one counted before
/// `track_from`, never looks more than [`MAX_DUE_YEARS`] after `from` and
/// yields at most [`MAX_DUE_DATES`]. A paused plan has occurrences too:
/// whether they count is the caller's decision.
pub fn due_dates(schedule: &Schedule, from: Date, to: Date) -> Vec<Due> {
    let mut found = Vec::new();
    if from > to || schedule.every == 0 {
        return found;
    }
    let Some(limit) = from.checked_add(i64::from(MAX_DUE_YEARS).years()).ok() else {
        return found;
    };
    // A moved date is at most two days after its counted one.
    let earliest = schedule
        .track_from
        .max(from.checked_sub(7.days()).unwrap_or(from));
    let mut k = schedule.k_before(earliest);
    while found.len() < MAX_DUE_DATES {
        let Some(counted) = schedule.counted(k) else {
            break;
        };
        k += 1;
        if counted > limit || schedule.ends_on.is_some_and(|end| counted > end) {
            break;
        }
        let Some(date) = banking_day(counted) else {
            break;
        };
        if date > to || date > limit {
            break;
        }
        if counted < schedule.track_from || date < from {
            continue;
        }
        found.push(Due { counted, date });
    }
    found
}

/// What the user decided about an occurrence, the only thing the book
/// stores about one (3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decided {
    /// An entry was matched to it: this transaction.
    Matched(TransactionId),
    /// The user skipped it this time.
    Skipped,
}

/// The state of an occurrence on a given day (3.3), as a fixed English word
/// for the UI, which translates it (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OccurrenceState {
    /// An entry was matched to it.
    Matched,
    /// The user skipped it.
    Skipped,
    /// Open, its date is after today.
    Upcoming,
    /// Open, its date is today or at most [`MAX_BUSINESS_DAYS`] banking
    /// days ago: the bank may still deliver it.
    Due,
    /// Open, more than [`MAX_BUSINESS_DAYS`] banking days ago: "Did this
    /// happen?".
    Overdue,
}

impl OccurrenceState {
    /// The word the UI gets.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Skipped => "skipped",
            Self::Upcoming => "upcoming",
            Self::Due => "due",
            Self::Overdue => "overdue",
        }
    }

    /// The state of the occurrence due on `date`, decided as `decided`, on
    /// `today`.
    pub fn of(date: Date, decided: Option<Decided>, today: Date) -> Self {
        match decided {
            Some(Decided::Matched(_)) => Self::Matched,
            Some(Decided::Skipped) => Self::Skipped,
            None if date > today => Self::Upcoming,
            None if business_days_apart(date, today, MAX_BUSINESS_DAYS).is_some() => Self::Due,
            None => Self::Overdue,
        }
    }
}

/// One entry that may be matched to a planned payment: a transaction's part
/// in one of the user's accounts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleEntry {
    /// The transaction.
    pub transaction: TransactionId,
    /// The account of its part.
    pub account: AccountId,
    /// What the part does to the account, in the account's currency.
    pub amount: Money,
    /// The transaction's booking date.
    pub booking_date: Date,
    /// The other party its bank named on the part.
    pub party: OtherParty,
}

/// How well an entry fits an occurrence: the smaller, the better.
type Closeness = (i128, u32, u32);

/// Whether `entry` fits `due` of `schedule` (3.4 points 1 to 3), and how
/// close: the difference of the amounts, then the banking and calendar days
/// between the dates. That the entry is no side of a Transfer and not
/// matched already is for the caller to check.
pub fn entry_fits(schedule: &Schedule, due: Due, entry: &ScheduleEntry) -> Option<Closeness> {
    if entry.account != schedule.account
        || entry.amount.currency() != schedule.amount.currency()
        || entry.amount.is_zero()
        || entry.amount.is_negative() != schedule.amount.is_negative()
    {
        return None;
    }
    let difference =
        (i128::from(entry.amount.minor_units()) - i128::from(schedule.amount.minor_units())).abs();
    if difference > i128::from(schedule.tolerance.minor_units()) {
        return None;
    }
    business_days_apart(entry.booking_date, due.date, MAX_BUSINESS_DAYS)?;
    if !schedule.party_fits(&entry.party) {
        return None;
    }
    let (business, calendar) = days_between(entry.booking_date, due.date);
    Some((difference, business, calendar))
}

/// An open occurrence of a planned payment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OpenOccurrence {
    /// The planned payment.
    pub schedule: ScheduleId,
    /// When it is due.
    pub due: Due,
}

/// An entry that fits an open occurrence, and nothing else fits as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduleMatch {
    /// The occurrence.
    pub occurrence: OpenOccurrence,
    /// The entry's transaction.
    pub transaction: TransactionId,
}

/// An occurrence that entries fit but that can't be matched by itself:
/// two entries fit it equally well, or its entry fits another occurrence
/// too. The user decides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnclearOccurrence {
    /// The occurrence.
    pub occurrence: OpenOccurrence,
    /// The entries in question, sorted by date, then ID.
    pub transactions: Vec<TransactionId>,
}

/// What [`match_schedules`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScheduleMatches {
    /// The matches, sorted by date, then plan.
    pub matches: Vec<ScheduleMatch>,
    /// The occurrences that need the user, sorted the same way.
    pub unclear: Vec<UnclearOccurrence>,
}

/// The entries among `entries` that fit the `open` occurrences of
/// `schedules` (3.4).
///
/// For each occurrence, the entries that fit it are ranked by
/// [`entry_fits`]'s closeness; the closest is taken if no other entry is as
/// close. An entry that fits two or more open occurrences is taken by
/// none of them (a weekly plan and a monthly one of the same amount), and
/// those occurrences are unclear too. So the result never depends on the
/// order of `schedules`, `open` or `entries`. The caller passes only
/// entries that may be matched: no side of a Transfer or of a suggestion,
/// none matched already.
pub fn match_schedules(
    schedules: &[Schedule],
    open: &[OpenOccurrence],
    entries: &[ScheduleEntry],
) -> ScheduleMatches {
    let plans: HashMap<ScheduleId, &Schedule> =
        schedules.iter().map(|plan| (plan.id, plan)).collect();
    let dates: HashMap<TransactionId, Date> = entries
        .iter()
        .map(|entry| (entry.transaction, entry.booking_date))
        .collect();
    let mut open: Vec<OpenOccurrence> = open.to_vec();
    open.sort();
    open.dedup();
    // Which occurrences each entry fits, and what fits each occurrence.
    let mut fits: Vec<(OpenOccurrence, Vec<(Closeness, TransactionId)>)> = Vec::new();
    let mut fitted: HashMap<TransactionId, usize> = HashMap::new();
    for occurrence in &open {
        let Some(plan) = plans.get(&occurrence.schedule) else {
            continue;
        };
        let mut found: Vec<(Closeness, TransactionId)> = entries
            .iter()
            .filter_map(|entry| {
                entry_fits(plan, occurrence.due, entry).map(|close| (close, entry.transaction))
            })
            .collect();
        if found.is_empty() {
            continue;
        }
        found.sort();
        found.dedup_by_key(|(_, transaction)| *transaction);
        for (_, transaction) in &found {
            *fitted.entry(*transaction).or_default() += 1;
        }
        fits.push((*occurrence, found));
    }
    let by_date = |id: &TransactionId| (dates.get(id).copied(), *id);
    let mut result = ScheduleMatches::default();
    for (occurrence, found) in fits {
        let best = found[0].0;
        let mut tied: Vec<TransactionId> = found
            .iter()
            .filter(|(close, _)| *close == best)
            .map(|(_, transaction)| *transaction)
            .collect();
        if tied.len() == 1 && fitted[&tied[0]] == 1 {
            result.matches.push(ScheduleMatch {
                occurrence,
                transaction: tied[0],
            });
        } else {
            tied.sort_by_key(by_date);
            result.unclear.push(UnclearOccurrence {
                occurrence,
                transactions: tied,
            });
        }
    }
    result
        .matches
        .sort_by_key(|found| (found.occurrence.due.date, found.occurrence.schedule));
    result
        .unclear
        .sort_by_key(|found| (found.occurrence.due.date, found.occurrence.schedule));
    result
}

/// One month of the forecast's projection (5.2, `months`): a calendar
/// month, or a salary month when the forecast's months start on another
/// day (#921, D-645).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonthPlan {
    /// The month's first day.
    pub month: Date,
    /// The month's last day.
    pub last: Date,
    /// The balance it starts with: the previous month's end, and for the
    /// first month after the current one, the current month's
    /// [`Forecast::expected_end`].
    pub start: Money,
    /// The planned income due in the month.
    pub planned_in: Money,
    /// The planned payments due in the month, as a positive amount.
    pub planned_out: Money,
    /// `start + planned_in − planned_out`.
    pub end: Money,
}

/// What is left this month, once the planned payments still to come are
/// taken off (D-770, 5.2). Every figure is exact, from code (D-052).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forecast {
    /// The booked balance of the chosen accounts at the end of today.
    pub balance_now: Money,
    /// The open outgoing occurrences due from today to the month's end, and
    /// those due in the last [`MAX_BUSINESS_DAYS`] banking days, as a
    /// positive amount.
    pub out_due: Money,
    /// The same for incoming ones.
    pub in_due: Money,
    /// `balance_now − out_due`: planned income that has not arrived is not
    /// added, so the figure may be too low, never too high.
    pub left: Money,
    /// `left + in_due`, the balance at the month's end with the income
    /// expected.
    pub expected_end: Money,
    /// The open occurrences more than [`MAX_BUSINESS_DAYS`] banking days
    /// ago, at most [`LOOK_BACK_YEARS`] back: counted in no figure, listed
    /// as "Did this happen?". Sorted by date, then plan.
    pub overdue: Vec<OpenOccurrence>,
    /// The months after the current one, each end the next start.
    pub months: Vec<MonthPlan>,
}

/// Why the forecast can't be computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForecastError {
    /// A sum doesn't fit into an amount.
    Money(MoneyError),
    /// A date is outside the calendar KassenSturz knows.
    Calendar,
}

impl From<MoneyError> for ForecastError {
    fn from(error: MoneyError) -> Self {
        Self::Money(error)
    }
}

/// The forecast for `today`'s month and the `months` months after it (at
/// most [`MAX_FORECAST_MONTHS`]), for accounts whose booked balance at the
/// end of today is `balance_now` (D-770).
///
/// Only `schedules` in `balance_now`'s currency count, and only those that
/// are not paused; the caller leaves out the plans of accounts that are
/// closed or not chosen. `decided` holds what the book stores about
/// occurrences, by plan and counted date: a matched or skipped occurrence
/// counts for nothing, as its entry is in the balance already or the user
/// skipped it.
pub fn forecast(
    balance_now: Money,
    schedules: &[Schedule],
    decided: &HashMap<(ScheduleId, Date), Decided>,
    today: Date,
    months: usize,
) -> Result<Forecast, ForecastError> {
    forecast_from_day(balance_now, schedules, decided, today, months, 1)
}

/// [`forecast`] for months that start on day `starts_on` (1 to 31) of the
/// month, as a salary month does (#921, D-645): "this month" runs from the
/// last such day to the day before the next one, and in a month shorter
/// than `starts_on` its last day starts it. Day 1 is the calendar month.
/// The same days as a budget or a report of that salary month, from the
/// same function ([`salary_month_of`]).
pub fn forecast_from_day(
    balance_now: Money,
    schedules: &[Schedule],
    decided: &HashMap<(ScheduleId, Date), Decided>,
    today: Date,
    months: usize,
    starts_on: i8,
) -> Result<Forecast, ForecastError> {
    let period = |day: Date| salary_month_of(day, starts_on).ok_or(ForecastError::Calendar);
    let currency: CurrencyCode = balance_now.currency();
    let zero = Money::zero(currency);
    let plans: Vec<&Schedule> = schedules
        .iter()
        .filter(|plan| !plan.paused && plan.amount.currency() == currency)
        .collect();
    let (_, month_end) = period(today)?;
    let look_back = today
        .checked_sub(i64::from(LOOK_BACK_YEARS).years())
        .map_err(|_| ForecastError::Calendar)?;
    let (mut out_due, mut in_due) = (zero, zero);
    let mut overdue = Vec::new();
    for plan in &plans {
        for due in due_dates(plan, look_back, month_end) {
            let state = OccurrenceState::of(
                due.date,
                decided.get(&(plan.id, due.counted)).copied(),
                today,
            );
            match state {
                OccurrenceState::Matched | OccurrenceState::Skipped => {}
                OccurrenceState::Overdue => overdue.push(OpenOccurrence {
                    schedule: plan.id,
                    due,
                }),
                OccurrenceState::Upcoming | OccurrenceState::Due => {
                    if plan.is_outgoing() {
                        out_due = out_due.checked_sub(plan.amount)?;
                    } else {
                        in_due = in_due.checked_add(plan.amount)?;
                    }
                }
            }
        }
    }
    overdue.sort_by_key(|found| (found.due.date, found.schedule));
    let left = balance_now.checked_sub(out_due)?;
    let expected_end = left.checked_add(in_due)?;
    let mut projected = Vec::new();
    let mut start = expected_end;
    let mut next = month_end.tomorrow().map_err(|_| ForecastError::Calendar)?;
    for _ in 0..months.min(MAX_FORECAST_MONTHS) {
        let (first, last) = period(next)?;
        let (mut planned_in, mut planned_out) = (zero, zero);
        for plan in &plans {
            for due in due_dates(plan, first, last) {
                if decided.contains_key(&(plan.id, due.counted)) {
                    continue;
                }
                if plan.is_outgoing() {
                    planned_out = planned_out.checked_sub(plan.amount)?;
                } else {
                    planned_in = planned_in.checked_add(plan.amount)?;
                }
            }
        }
        let end = start.checked_add(planned_in)?.checked_sub(planned_out)?;
        projected.push(MonthPlan {
            month: first,
            last,
            start,
            planned_in,
            planned_out,
            end,
        });
        start = end;
        next = last.tomorrow().map_err(|_| ForecastError::Calendar)?;
    }
    Ok(Forecast {
        balance_now,
        out_due,
        in_due,
        left,
        expected_end,
        overdue,
        months: projected,
    })
}

#[cfg(test)]
mod tests;
