// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Planned payments as the screens show them (US7.1, D-769,
//! `docs/SCHEDULES.md` section 2): the list under Accounts › Planned
//! payments…, the form, "Plan this again…" on an entry, and "Did this
//! happen?" for a payment whose day passed without an entry.
//!
//! The core decides when a payment is due and which entry fits it
//! (`kassensturz_core::schedule`, `Book::match_schedules`); this module only
//! writes what the screens show as text (D-065), reads what the user typed,
//! and runs each choice as one action, so one Undo takes it back. Nothing
//! here books, matches or skips a payment without the user's click, and a
//! plan never sends money (D-053).

use std::collections::HashSet;

use kassensturz_core::{
    AccountId, CurrencyCode, Date, Decided, MAX_SCHEDULE_EVERY, Money, OccurrenceState, Report,
    Schedule, ScheduleId, ScheduleUnit, TransactionId, compact_iban, due_dates,
};
use kassensturz_store::{Action, Book, NewSchedule, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::parse::{AmountInputError, AmountReader};
use crate::register::{AmountWriter, Shown};
use crate::transaction_entry::typed_amount;

/// How many days back "Did this happen?" looks for payments that never
/// came; older ones are the user's to find in the plan's list of dates.
const DID_THIS_HAPPEN_DAYS: i32 = 92;

/// How many days ahead "Coming up" looks.
pub const COMING_UP_DAYS: i32 = 30;

/// How far from a payment's day an entry may be to be offered for "Match an
/// entry…": the user decides, so this only keeps the list short.
const CHOICE_DAYS: i32 = 31;

/// Why a screen's request about planned payments failed.
#[derive(Debug)]
pub enum PlansError {
    /// The plan, entry or account the screen named isn't there (any more).
    NoSuchEntry,
    /// The typed amount isn't an amount in the account's currency.
    Amount(AmountInputError, CurrencyCode),
    /// The typed date isn't a date.
    Date,
    /// "How often" isn't 1 to 52 weeks, months or years.
    Every,
    /// The book refused it.
    Store(StoreError),
}

impl From<StoreError> for PlansError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &PlansError, locale: &impl Locale) -> ErrorText {
    match error {
        PlansError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        PlansError::Amount(error, currency) => ErrorText::amount(*error, book, *currency, locale),
        PlansError::Date => ErrorText {
            code: "input.malformed-date",
            params: vec![("example", locale.short_date(locale.today()))],
        },
        PlansError::Every => ErrorText::plain("input.plan-every"),
        PlansError::Store(error) => report_text(book, &error.report(), locale),
    }
}

/// One plan, for the list.
#[derive(Debug, PartialEq, Eq)]
pub struct PlanText {
    /// Its ID as text.
    pub id: String,
    /// What the user calls it.
    pub name: String,
    /// The account's name and ID.
    pub account: String,
    pub account_id: String,
    /// The expected amount, without its sign; `outgoing` says which way.
    pub amount: Shown,
    /// Whether money leaves the account.
    pub outgoing: bool,
    /// `week`, `month` or `year`, and how many of them lie between two
    /// payments.
    pub unit: &'static str,
    pub every: i32,
    /// The first payment's day, and the last day one may be on (empty for
    /// no end), as the user's locale writes dates.
    pub first_due: String,
    pub ends: String,
    /// The next payment still open, as the user's locale writes dates;
    /// empty if paused or there is none.
    pub next_due: String,
    /// Whether it is paused, by the user or because its account is closed.
    pub paused: bool,
    /// Whether its account is closed.
    pub account_closed: bool,
    /// The category's name; empty for none.
    pub category: String,
    /// The other party it waits for, as the bank names it; may be empty.
    pub party: String,
}

/// One payment of a plan, for "Did this happen?", "Coming up" and the
/// import summary.
#[derive(Debug, PartialEq, Eq)]
pub struct PaymentText {
    /// The plan's ID and name.
    pub plan_id: String,
    pub name: String,
    /// The account's name.
    pub account: String,
    /// The expected amount, without its sign.
    pub amount: Shown,
    /// Whether money leaves the account.
    pub outgoing: bool,
    /// The day it is due, after a weekend moved it, as the locale writes
    /// dates.
    pub due: String,
    /// The day it is counted on, ISO 8601, to name it in calls.
    pub due_on: String,
    /// `matched`, `skipped`, `upcoming`, `due` or `overdue`.
    pub state: &'static str,
}

/// The form for a new or changed plan, as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanForm {
    /// What it is for, at most 60 characters.
    pub name: String,
    /// The account's ID.
    pub account_id: String,
    /// Whether money leaves the account.
    pub outgoing: bool,
    /// The amount as typed, without sign.
    pub amount: String,
    /// The category's ID; empty for none.
    pub category_id: String,
    /// `week`, `month` or `year`.
    pub unit: String,
    /// How many units lie between two payments, as typed.
    pub every: String,
    /// The first payment's day as typed.
    pub first_due: String,
    /// The last day as typed; empty for no end.
    pub ends: String,
    /// How far the amount may vary, as typed; empty for not at all.
    pub tolerance: String,
    /// The other party's name or IBAN as the bank shows it; may be empty.
    pub party: String,
    /// For "Plan this again…": the entry that is the first payment, as
    /// text; empty otherwise.
    pub from_entry: String,
}

/// An entry the user may pick for "Match an entry…".
#[derive(Debug, PartialEq, Eq)]
pub struct EntryChoice {
    /// The transaction's ID as text.
    pub id: String,
    /// The day the account shows it on, as the locale writes dates.
    pub date: String,
    /// Its payee or the other party; may be empty.
    pub payee: String,
    /// Its amount in the account, with its sign.
    pub amount: Shown,
}

/// The ID `text` names, if it is one written as IDs are written.
fn id_bytes(text: &str) -> Option<[u8; 16]> {
    let digits: Vec<u8> = text.bytes().filter(|byte| *byte != b'-').collect();
    if digits.len() != 32 {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(bytes)
}

/// The plan `text` names.
fn plan_id(text: &str) -> Result<ScheduleId, PlansError> {
    let id = ScheduleId::from_bytes(id_bytes(text).ok_or(PlansError::NoSuchEntry)?);
    if id.to_string() == text {
        Ok(id)
    } else {
        Err(PlansError::NoSuchEntry)
    }
}

/// The transaction `text` names.
fn transaction_id(text: &str) -> Result<TransactionId, PlansError> {
    let id = TransactionId::from_bytes(id_bytes(text).ok_or(PlansError::NoSuchEntry)?);
    if id.to_string() == text {
        Ok(id)
    } else {
        Err(PlansError::NoSuchEntry)
    }
}

/// The account `text` names.
fn account_id(text: &str) -> Result<AccountId, PlansError> {
    let id = AccountId::from_bytes(id_bytes(text).ok_or(PlansError::NoSuchEntry)?);
    if id.to_string() == text {
        Ok(id)
    } else {
        Err(PlansError::NoSuchEntry)
    }
}

/// The plan `id` names, refused as no entry when it is gone.
fn plan(book: &Book, id: &str) -> Result<Schedule, PlansError> {
    match book.schedule(plan_id(id)?) {
        Ok(plan) => Ok(plan),
        Err(StoreError::UnknownSchedule(_)) => Err(PlansError::NoSuchEntry),
        Err(error) => Err(error.into()),
    }
}

/// `date` moved by `days`, or `date` itself at the end of the calendar.
fn days_from(date: Date, days: i32) -> Date {
    let mut moved = date;
    for _ in 0..days.unsigned_abs() {
        let next = if days < 0 {
            moved.yesterday()
        } else {
            moved.tomorrow()
        };
        match next {
            Ok(next) => moved = next,
            Err(_) => break,
        }
    }
    moved
}

/// The amount without its sign, as the screens show it next to "out" or
/// "in".
fn size_text(book: &Book, amount: Money, locale: &impl Locale) -> Result<Shown, StoreError> {
    let size = if amount.is_negative() {
        amount
            .checked_neg()
            .map_err(|_| StoreError::AmountOverflow)?
    } else {
        amount
    };
    Ok(AmountWriter::new(book, amount.currency(), locale)?.text(size))
}

/// The other party a plan waits for, as the bank names it.
fn party_text(plan: &Schedule) -> String {
    plan.party_iban
        .clone()
        .or_else(|| plan.party_name.clone())
        .unwrap_or_default()
}

/// Every plan, paused ones too, by name.
pub fn list(book: &Book, locale: &impl Locale) -> Result<Vec<PlanText>, PlansError> {
    let today = locale.today();
    let accounts = book.all_accounts()?;
    let name_of = |id: AccountId| {
        accounts
            .iter()
            .find(|account| account.id == id)
            .map(|account| account.name.clone())
            .unwrap_or_default()
    };
    let closed = |id: AccountId| {
        accounts
            .iter()
            .any(|account| account.id == id && account.closed)
    };
    let mut plans = book.schedules()?;
    plans.sort_by_cached_key(|plan| (plan.name.to_lowercase(), plan.id));
    let mut shown = Vec::with_capacity(plans.len());
    for plan in plans {
        let next_due = book
            .next_due(plan.id, today)?
            .map(|due| locale.short_date(due.date))
            .unwrap_or_default();
        shown.push(PlanText {
            id: plan.id.to_string(),
            name: plan.name.clone(),
            account: name_of(plan.account),
            account_id: plan.account.to_string(),
            amount: size_text(book, plan.amount, locale)?,
            outgoing: plan.is_outgoing(),
            unit: plan.unit.word(),
            every: i32::from(plan.every),
            first_due: locale.short_date(plan.anchor),
            ends: plan
                .ends_on
                .map(|day| locale.short_date(day))
                .unwrap_or_default(),
            next_due,
            paused: plan.paused || closed(plan.account),
            account_closed: closed(plan.account),
            category: plan.category.map(name_of).unwrap_or_default(),
            party: party_text(&plan),
        });
    }
    Ok(shown)
}

/// The payments of the expected plans from `from` to `to`, in `states`,
/// sorted by date.
fn payments(
    book: &Book,
    from: Date,
    to: Date,
    states: &[OccurrenceState],
    locale: &impl Locale,
) -> Result<Vec<PaymentText>, PlansError> {
    let accounts = book.all_accounts()?;
    let plans = book.schedules()?;
    let mut shown = Vec::new();
    for found in book.planned_occurrences(from, to, locale.today())? {
        if !states.contains(&found.state) {
            continue;
        }
        let Some(plan) = plans.iter().find(|plan| plan.id == found.schedule) else {
            continue;
        };
        shown.push(PaymentText {
            plan_id: plan.id.to_string(),
            name: plan.name.clone(),
            account: accounts
                .iter()
                .find(|account| account.id == plan.account)
                .map(|account| account.name.clone())
                .unwrap_or_default(),
            amount: size_text(book, plan.amount, locale)?,
            outgoing: plan.is_outgoing(),
            due: locale.short_date(found.due.date),
            due_on: found.due.counted.to_string(),
            state: found.state.word(),
        });
    }
    Ok(shown)
}

/// The payments whose day passed without an entry ("Did this happen?",
/// 2.5), from the last three months, oldest first.
pub fn did_this_happen(book: &Book, locale: &impl Locale) -> Result<Vec<PaymentText>, PlansError> {
    let today = locale.today();
    payments(
        book,
        days_from(today, -DID_THIS_HAPPEN_DAYS),
        today,
        &[OccurrenceState::Overdue],
        locale,
    )
}

/// The payments still to come in the next `days` days ("Coming up", 2.6),
/// the ones due now first.
pub fn coming_up(
    book: &Book,
    days: i32,
    locale: &impl Locale,
) -> Result<Vec<PaymentText>, PlansError> {
    let today = locale.today();
    payments(
        book,
        days_from(today, -5),
        days_from(today, days),
        &[OccurrenceState::Due, OccurrenceState::Upcoming],
        locale,
    )
}

/// The form for a new plan on `account_id`: money going out, every month
/// from today, no end.
pub fn new_form(account_id: &str, locale: &impl Locale) -> PlanForm {
    PlanForm {
        account_id: account_id.to_owned(),
        outgoing: true,
        unit: ScheduleUnit::Month.word().to_owned(),
        every: "1".to_owned(),
        first_due: locale.short_date(locale.today()),
        ..PlanForm::default()
    }
}

/// The amount without its sign as the form shows it, exactly.
fn amount_input(book: &Book, amount: Money, locale: &impl Locale) -> Result<String, StoreError> {
    let size = if amount.is_negative() {
        amount
            .checked_neg()
            .map_err(|_| StoreError::AmountOverflow)?
    } else {
        amount
    };
    typed_amount(book, size, locale)
}

/// `plan` as the form shows it.
fn form_of(book: &Book, plan: &NewSchedule, locale: &impl Locale) -> Result<PlanForm, StoreError> {
    Ok(PlanForm {
        name: plan.name.clone(),
        account_id: plan.account.to_string(),
        outgoing: plan.amount.is_negative(),
        amount: amount_input(book, plan.amount, locale)?,
        category_id: plan.category.map(|id| id.to_string()).unwrap_or_default(),
        unit: plan.unit.word().to_owned(),
        every: plan.every.to_string(),
        first_due: locale.short_date(plan.anchor),
        ends: plan
            .ends_on
            .map(|day| locale.short_date(day))
            .unwrap_or_default(),
        tolerance: if plan.tolerance.is_zero() {
            String::new()
        } else {
            amount_input(book, plan.tolerance, locale)?
        },
        party: plan
            .party_iban
            .clone()
            .or_else(|| plan.party_name.clone())
            .unwrap_or_default(),
        from_entry: String::new(),
    })
}

/// The form of the plan `id`, to change it.
pub fn edit_form(book: &Book, id: &str, locale: &impl Locale) -> Result<PlanForm, PlansError> {
    let plan = plan(book, id)?;
    let new = NewSchedule {
        name: plan.name,
        account: plan.account,
        amount: plan.amount,
        tolerance: plan.tolerance,
        category: plan.category,
        party_iban: plan.party_iban,
        party_name: plan.party_name,
        unit: plan.unit,
        every: plan.every,
        anchor: plan.anchor,
        track_from: plan.track_from,
        ends_on: plan.ends_on,
    };
    Ok(form_of(book, &new, locale)?)
}

/// The form "Plan this again…" opens for the entry `transaction` in
/// `account` (2.1): filled from the entry, every month on its day. `None`
/// when no plan could ever match that entry, e.g. a Transfer between the
/// user's own accounts.
pub fn form_from_entry(
    book: &Book,
    transaction: &str,
    account: &str,
    fallback_name: &str,
    locale: &impl Locale,
) -> Result<Option<PlanForm>, PlansError> {
    let id = transaction_id(transaction)?;
    let account = account_id(account)?;
    let new = match book.plan_from_entry(id, account, fallback_name) {
        Ok(new) => new,
        Err(StoreError::UnknownTransaction(_) | StoreError::UnknownAccount(_)) => {
            return Err(PlansError::NoSuchEntry);
        }
        Err(error) => return Err(error.into()),
    };
    Ok(match new {
        Some(new) => Some(PlanForm {
            from_entry: id.to_string(),
            ..form_of(book, &new, locale)?
        }),
        None => None,
    })
}

/// Whether `text` looks like an IBAN: two letters, two digits, and 11 to
/// 30 letters or digits, spaces ignored.
fn looks_like_iban(text: &str) -> bool {
    let compact = compact_iban(text);
    let bytes = compact.as_bytes();
    (15..=34).contains(&bytes.len())
        && bytes[..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..4].iter().all(u8::is_ascii_digit)
        && bytes[4..].iter().all(u8::is_ascii_alphanumeric)
}

/// Reads `text` as an amount in `currency`.
fn read_amount(
    book: &Book,
    text: &str,
    currency: CurrencyCode,
    locale: &impl Locale,
) -> Result<Money, PlansError> {
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
        .map_err(|error| PlansError::Amount(error, currency))
}

/// A size typed without sign, or with one, which counts for nothing: the
/// form says which way the money goes.
fn size_of(amount: Money) -> Result<Money, PlansError> {
    if amount.is_negative() {
        amount
            .checked_neg()
            .map_err(|_| PlansError::Store(StoreError::AmountOverflow))
    } else {
        Ok(amount)
    }
}

/// What the form says, as a plan for the book; its name is checked there.
fn read_form(
    book: &Book,
    form: &PlanForm,
    locale: &impl Locale,
) -> Result<NewSchedule, PlansError> {
    let account = match book.account(account_id(&form.account_id)?) {
        Ok(account) => account,
        Err(StoreError::UnknownAccount(_)) => return Err(PlansError::NoSuchEntry),
        Err(error) => return Err(error.into()),
    };
    let currency = account.currency;
    let size = size_of(read_amount(book, &form.amount, currency, locale)?)?;
    let amount = if form.outgoing {
        size.checked_neg()
            .map_err(|_| PlansError::Store(StoreError::AmountOverflow))?
    } else {
        size
    };
    let tolerance = if form.tolerance.trim().is_empty() {
        Money::zero(currency)
    } else {
        size_of(read_amount(book, &form.tolerance, currency, locale)?)?
    };
    let category = if form.category_id.trim().is_empty() {
        None
    } else {
        Some(account_id(&form.category_id)?)
    };
    let unit = ScheduleUnit::from_word(&form.unit).ok_or(PlansError::Every)?;
    let every = form
        .every
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|every| (1..=MAX_SCHEDULE_EVERY).contains(every))
        .ok_or(PlansError::Every)?;
    let anchor = locale
        .read_date(form.first_due.trim())
        .ok_or(PlansError::Date)?;
    let ends_on = if form.ends.trim().is_empty() {
        None
    } else {
        Some(locale.read_date(form.ends.trim()).ok_or(PlansError::Date)?)
    };
    let party = form.party.trim();
    let (party_iban, party_name) = if party.is_empty() {
        (None, None)
    } else if looks_like_iban(party) {
        (Some(compact_iban(party)), None)
    } else {
        (None, Some(party.to_owned()))
    };
    Ok(NewSchedule {
        name: form.name.clone(),
        account: account.id,
        amount,
        tolerance,
        category,
        party_iban,
        party_name,
        unit,
        every,
        anchor,
        track_from: anchor,
        ends_on,
    })
}

/// The next three payments the form would plan, from today, as the locale
/// writes dates, so nobody has to work out "the 1st that is a Sunday"
/// (2.3). Empty while the form can't be read yet.
pub fn next_dates(book: &Book, form: &PlanForm, locale: &impl Locale) -> Vec<String> {
    let Ok(new) = read_form(book, form, locale) else {
        return Vec::new();
    };
    let plan = Schedule {
        id: ScheduleId::from_bytes([0; 16]),
        name: new.name,
        account: new.account,
        amount: new.amount,
        tolerance: new.tolerance,
        category: new.category,
        party_iban: new.party_iban,
        party_name: new.party_name,
        unit: new.unit,
        every: new.every,
        anchor: new.anchor,
        track_from: new.track_from,
        ends_on: new.ends_on,
        paused: false,
    };
    if plan.check().is_err() {
        return Vec::new();
    }
    let today = locale.today();
    let from = if plan.anchor > today {
        plan.anchor
    } else {
        today
    };
    let to = days_from(from, 3 * 366);
    due_dates(&plan, from, to)
        .into_iter()
        .take(3)
        .map(|due| locale.short_date(due.date))
        .collect()
}

/// Saves the form: a new plan when `id` is empty, else the plan `id`
/// changed, keeping its paused state and the payments already matched or
/// skipped. A new plan from "Plan this again…" takes its entry as its first
/// payment in the same action, so one Undo takes back both. Returns the
/// plan's ID.
pub fn save(
    book: &mut Book,
    id: &str,
    form: &PlanForm,
    locale: &impl Locale,
) -> Result<String, PlansError> {
    let new = read_form(book, form, locale)?;
    if id.is_empty() {
        let entry = if form.from_entry.is_empty() {
            None
        } else {
            Some(transaction_id(&form.from_entry)?)
        };
        let saved = book.as_one_action(Action::AddSchedule, |book| {
            let saved = book.add_schedule(&new)?;
            if let Some(entry) = entry {
                book.match_schedule_occurrence(saved, new.anchor, entry)?;
            }
            Ok::<_, StoreError>(saved)
        })?;
        return Ok(saved.to_string());
    }
    let old = plan(book, id)?;
    // A changed first day moves the day tracking starts from, unless the
    // plan started tracking later on purpose.
    let track_from = if old.track_from == old.anchor || old.track_from < new.anchor {
        new.anchor
    } else {
        old.track_from
    };
    book.edit_schedule(&Schedule {
        id: old.id,
        name: new.name,
        account: new.account,
        amount: new.amount,
        tolerance: new.tolerance,
        category: new.category,
        party_iban: new.party_iban,
        party_name: new.party_name,
        unit: new.unit,
        every: new.every,
        anchor: new.anchor,
        track_from,
        ends_on: new.ends_on,
        paused: old.paused,
    })?;
    Ok(old.id.to_string())
}

/// Pauses the plan `id`, or lets it go on.
pub fn set_paused(book: &mut Book, id: &str, paused: bool) -> Result<(), PlansError> {
    let plan = plan(book, id)?;
    book.set_schedule_paused(plan.id, paused)?;
    Ok(())
}

/// Removes the plan `id`; its entries stay as they are.
pub fn remove(book: &mut Book, id: &str) -> Result<(), PlansError> {
    let plan = plan(book, id)?;
    book.remove_schedule(plan.id)?;
    Ok(())
}

/// The day `due_on` names, ISO 8601 as [`PaymentText::due_on`] has it.
fn due_day(due_on: &str) -> Result<Date, PlansError> {
    due_on.parse().map_err(|_| PlansError::NoSuchEntry)
}

/// "Skip this time" for the payment of `id` counted on `due_on`.
pub fn skip(book: &mut Book, id: &str, due_on: &str) -> Result<(), PlansError> {
    let plan = plan(book, id)?;
    book.skip_schedule_occurrence(plan.id, due_day(due_on)?)?;
    Ok(())
}

/// The form "Book it" opens: the plan's amount without sign, and today.
pub fn book_form(
    book: &Book,
    id: &str,
    locale: &impl Locale,
) -> Result<(String, String), PlansError> {
    let plan = plan(book, id)?;
    Ok((
        amount_input(book, plan.amount, locale)?,
        locale.short_date(locale.today()),
    ))
}

/// "Book it": books the payment of `id` counted on `due_on` as a new entry
/// in the plan's account, on the typed date with the typed amount (the
/// money going the plan's way). One action. Returns the entry's ID.
pub fn book_it(
    book: &mut Book,
    id: &str,
    due_on: &str,
    date: &str,
    amount: &str,
    locale: &impl Locale,
) -> Result<String, PlansError> {
    let plan = plan(book, id)?;
    let due_on = due_day(due_on)?;
    let date = locale.read_date(date.trim()).ok_or(PlansError::Date)?;
    let size = size_of(read_amount(book, amount, plan.amount.currency(), locale)?)?;
    let amount = if plan.is_outgoing() {
        size.checked_neg()
            .map_err(|_| PlansError::Store(StoreError::AmountOverflow))?
    } else {
        size
    };
    let booked = book.book_schedule_occurrence(plan.id, due_on, date, amount)?;
    Ok(booked.to_string())
}

/// The entries the user may pick for "Match an entry…" for the payment of
/// `id` counted on `due_on`: in the plan's account, money going the plan's
/// way, at most a month from the day, not matched to a payment yet; the
/// closest first.
pub fn entry_choices(
    book: &Book,
    id: &str,
    due_on: &str,
    locale: &impl Locale,
) -> Result<Vec<EntryChoice>, PlansError> {
    let plan = plan(book, id)?;
    let due_on = due_day(due_on)?;
    let taken: HashSet<TransactionId> = book
        .decided_occurrences()?
        .into_values()
        .filter_map(|decided| match decided {
            Decided::Matched(transaction) => Some(transaction),
            Decided::Skipped => None,
        })
        .collect();
    let (from, to) = (
        days_from(due_on, -CHOICE_DAYS),
        days_from(due_on, CHOICE_DAYS),
    );
    let writer = AmountWriter::new(book, plan.amount.currency(), locale)?;
    let mut found: Vec<(i64, Date, EntryChoice)> = Vec::new();
    for part in book.account_parts(plan.account)? {
        if taken.contains(&part.transaction)
            || part.amount.is_zero()
            || part.amount.is_negative() != plan.is_outgoing()
            || part.booking_date < from
            || part.booking_date > to
        {
            continue;
        }
        let transaction = book.transaction(part.transaction)?;
        let payee = transaction
            .payee()
            .map(str::to_owned)
            .or_else(|| {
                part.imported
                    .as_ref()
                    .and_then(|imported| imported.other_party_name.clone())
            })
            .unwrap_or_default();
        let distance = (part.booking_date - due_on).get_days().unsigned_abs();
        found.push((
            i64::from(distance),
            part.booking_date,
            EntryChoice {
                id: part.transaction.to_string(),
                date: locale.short_date(part.booking_date),
                payee,
                amount: writer.text(part.amount),
            },
        ));
    }
    found.sort_by(|a, b| (a.0, a.1, &a.2.id).cmp(&(b.0, b.1, &b.2.id)));
    found.dedup_by(|a, b| a.2.id == b.2.id);
    Ok(found.into_iter().map(|(_, _, choice)| choice).collect())
}

/// "Match an entry…": the entry `transaction` the user picked is the
/// payment of `id` counted on `due_on`. One action.
pub fn match_entry(
    book: &mut Book,
    id: &str,
    due_on: &str,
    transaction: &str,
) -> Result<(), PlansError> {
    let plan = plan(book, id)?;
    let transaction = transaction_id(transaction)?;
    book.match_schedule_occurrence(plan.id, due_day(due_on)?, transaction)?;
    Ok(())
}

#[cfg(test)]
mod tests;
