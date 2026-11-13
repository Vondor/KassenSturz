// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Planned payments (D-769, `docs/SCHEDULES.md`): making, changing, pausing
//! and removing them, matching the entries a bank delivers to their
//! occurrences, the user's own decisions on an occurrence (match by hand,
//! skip, book), and the forecast that reads them (D-770).
//!
//! A plan never moves money by itself: a match only links an entry to an
//! occurrence and, where its part has no category yet, gives it the plan's
//! one; it never changes an amount, a date or a part. Only
//! [`Book::book_schedule_occurrence`], on the user's click, creates an
//! entry. Every change is one user action, which Undo takes back.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    AccountId, AccountKind, AccountType, CurrencyCode, Date, Decided, Due, Forecast, ForecastError,
    LOOK_BACK_YEARS, MAX_BUSINESS_DAYS, MAX_SCHEDULES, Money, OccurrenceState, OpenOccurrence,
    OtherParty, Schedule, ScheduleEntry, ScheduleError, ScheduleId, ScheduleUnit, Split,
    Transaction, TransactionId, add_banking_days, cut_party, due_dates, forecast, match_schedules,
};
use rusqlite::{OptionalExtension, params};

use crate::cached::Cached;
use crate::error::{Corruption, StoreError};
use crate::{Action, Book, currency_from_text, date_from_text, date_to_text};

/// The columns [`read_schedule`] expects, in its order.
const COLUMNS: &str = "id, name, account_id, amount, currency, tolerance, category_id,
                       party_iban, party_name, unit, every, anchor, track_from, ends_on, paused";

/// A row of `schedules` as SQLite returns it.
struct ScheduleRow {
    id: [u8; 16],
    name: String,
    account: [u8; 16],
    amount: i64,
    currency: String,
    tolerance: i64,
    category: Option<[u8; 16]>,
    party_iban: Option<String>,
    party_name: Option<String>,
    unit: String,
    every: u8,
    anchor: String,
    track_from: String,
    ends_on: Option<String>,
    paused: bool,
}

fn read_schedule(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduleRow> {
    Ok(ScheduleRow {
        id: row.get(0)?,
        name: row.get(1)?,
        account: row.get(2)?,
        amount: row.get(3)?,
        currency: row.get(4)?,
        tolerance: row.get(5)?,
        category: row.get(6)?,
        party_iban: row.get(7)?,
        party_name: row.get(8)?,
        unit: row.get(9)?,
        every: row.get(10)?,
        anchor: row.get(11)?,
        track_from: row.get(12)?,
        ends_on: row.get(13)?,
        paused: row.get(14)?,
    })
}

fn schedule_from(row: ScheduleRow) -> Result<Schedule, StoreError> {
    let currency = currency_from_text(&row.currency)?;
    Ok(Schedule {
        id: ScheduleId::from_bytes(row.id),
        name: row.name,
        account: AccountId::from_bytes(row.account),
        amount: Money::from_minor_units(row.amount, currency),
        tolerance: Money::from_minor_units(row.tolerance, currency),
        category: row.category.map(AccountId::from_bytes),
        party_iban: row.party_iban,
        party_name: row.party_name,
        unit: ScheduleUnit::from_word(&row.unit).ok_or(StoreError::Corrupt(
            Corruption::BadSchedule(ScheduleId::from_bytes(row.id)),
        ))?,
        every: row.every,
        anchor: date_from_text(&row.anchor)?,
        track_from: date_from_text(&row.track_from)?,
        ends_on: row.ends_on.as_deref().map(date_from_text).transpose()?,
        paused: row.paused,
    })
}

/// A planned payment to make ([`Book::add_schedule`]): everything but its
/// ID. A new plan is not paused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSchedule {
    /// What it is for: 1 to 60 characters, one line; spaces around it are
    /// taken off.
    pub name: String,
    /// The user's account it is paid from or into.
    pub account: AccountId,
    /// The amount, negative for money out, in the account's currency.
    pub amount: Money,
    /// How much a matching entry may differ, 0 or more.
    pub tolerance: Money,
    /// The category to give a matching entry that has none.
    pub category: Option<AccountId>,
    /// The other party's IBAN, as the bank shows it; cut to 140
    /// characters.
    pub party_iban: Option<String>,
    /// The other party's name, as the bank shows it; cut to 140
    /// characters.
    pub party_name: Option<String>,
    /// Weeks, months or years.
    pub unit: ScheduleUnit,
    /// How many of them between two payments, 1 to 52.
    pub every: u8,
    /// The first due date.
    pub anchor: Date,
    /// The first date an occurrence counts, `anchor` or later.
    pub track_from: Date,
    /// The last date, if it ends.
    pub ends_on: Option<Date>,
}

impl NewSchedule {
    /// The plan `transaction` would make if it repeated every month on its
    /// day, from its booking date ("Plan this again…", 2.1): its part in
    /// `account`, with that part's amount, its other part's category if it
    /// has exactly one other part in a category, and the other party its
    /// bank named. The name is the payee, else the other party, else
    /// `fallback_name`; it is cut to 60 characters. `None` if the
    /// transaction has no single part in `account`, or one of zero.
    pub fn from_entry(
        transaction: &Transaction,
        account: AccountId,
        fallback_name: &str,
    ) -> Option<Self> {
        let mut own = transaction
            .splits()
            .iter()
            .filter(|split| split.account == Some(account));
        let part = own.next()?;
        if own.next().is_some() || part.foreign.is_some() || part.amount.is_zero() {
            return None;
        }
        let others: Vec<&Split> = transaction
            .splits()
            .iter()
            .filter(|split| split.id != part.id)
            .collect();
        let category = match others.as_slice() {
            [other] if other.foreign.is_none() => other.account,
            _ => None,
        };
        let imported = part.imported.as_ref();
        let party_name = cut_party(imported.and_then(|i| i.other_party_name.as_deref()));
        let party_iban = cut_party(imported.and_then(|i| i.other_party_iban.as_deref()));
        let name: String = transaction
            .payee_of(part)
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| party_name.clone())
            .unwrap_or_else(|| fallback_name.to_owned())
            .split(['\n', '\r'])
            .next()
            .unwrap_or_default()
            .trim()
            .chars()
            .take(kassensturz_core::MAX_SCHEDULE_NAME_CHARS)
            .collect::<String>()
            .trim()
            .to_owned();
        let anchor = transaction.date_of(part);
        Some(Self {
            name,
            account,
            amount: part.amount,
            tolerance: Money::zero(part.amount.currency()),
            category,
            party_iban,
            party_name,
            unit: ScheduleUnit::Month,
            every: 1,
            anchor,
            track_from: anchor,
            ends_on: None,
        })
    }
}

/// One occurrence of a planned payment, with its state on a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedOccurrence {
    /// The planned payment.
    pub schedule: ScheduleId,
    /// When it is due.
    pub due: Due,
    /// Its state.
    pub state: OccurrenceState,
    /// The entry matched to it, if one is.
    pub transaction: Option<TransactionId>,
}

/// An entry matched to an occurrence after an import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduleMatchDone {
    /// The occurrence.
    pub occurrence: OpenOccurrence,
    /// The entry.
    pub transaction: TransactionId,
    /// Whether the entry got the plan's category, because its part had
    /// none.
    pub category_set: bool,
}

/// An occurrence that entries fit but that wasn't matched, because they
/// fit equally well or fit another occurrence too: the user decides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleNeedsYou {
    /// The occurrence.
    pub occurrence: OpenOccurrence,
    /// The entries in question, sorted by date.
    pub transactions: Vec<TransactionId>,
}

/// What [`Book::match_schedules`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScheduleMatchReport {
    /// The matches, sorted by date.
    pub matched: Vec<ScheduleMatchDone>,
    /// The occurrences left for the user.
    pub needs_you: Vec<ScheduleNeedsYou>,
}

/// The forecast for some accounts in one currency (D-770).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountsForecast {
    /// The figures.
    pub forecast: Forecast,
    /// The accounts it counts, sorted by ID.
    pub accounts: Vec<AccountId>,
    /// The chosen accounts in another currency, left out (D-043: no
    /// conversion without a rate), sorted by ID.
    pub left_out: Vec<AccountId>,
    /// Whether any plan counts: with none, the figure means nothing
    /// (`unknown`, D-734).
    pub has_plans: bool,
}

impl Book {
    /// Every planned payment, in the order they were made.
    pub fn schedules(&self) -> Result<Vec<Schedule>, StoreError> {
        let mut statement = self
            .conn
            .prepare_cached(&format!("SELECT {COLUMNS} FROM schedules ORDER BY id"))?;
        let rows = statement
            .query_map([], read_schedule)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(schedule_from).collect()
    }

    /// The planned payment `id` ([`StoreError::UnknownSchedule`]).
    pub fn schedule(&self, id: ScheduleId) -> Result<Schedule, StoreError> {
        let row = self
            .conn
            .query_row_cached(
                &format!("SELECT {COLUMNS} FROM schedules WHERE id = ?1"),
                [id.to_bytes()],
                read_schedule,
            )
            .optional()?
            .ok_or(StoreError::UnknownSchedule(id))?;
        schedule_from(row)
    }

    /// What the book stores about occurrences: per plan and counted date,
    /// whether it was matched (to which entry) or skipped.
    pub fn decided_occurrences(&self) -> Result<HashMap<(ScheduleId, Date), Decided>, StoreError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT schedule_id, due_on, status, transaction_id FROM schedule_occurrences",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, [u8; 16]>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<[u8; 16]>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut decided = HashMap::with_capacity(rows.len());
        for (schedule, due_on, status, transaction) in rows {
            let state = match (status.as_str(), transaction) {
                ("matched", Some(transaction)) => {
                    Decided::Matched(TransactionId::from_bytes(transaction))
                }
                ("skipped", None) => Decided::Skipped,
                _ => {
                    return Err(StoreError::Corrupt(Corruption::BadSchedule(
                        ScheduleId::from_bytes(schedule),
                    )));
                }
            };
            decided.insert(
                (ScheduleId::from_bytes(schedule), date_from_text(&due_on)?),
                state,
            );
        }
        Ok(decided)
    }

    /// Checks `schedule` against the book: an open account of the user's
    /// in the amount's currency, a category in that currency.
    fn check_schedule(&self, schedule: &Schedule) -> Result<(), StoreError> {
        schedule.check().map_err(StoreError::Schedule)?;
        let account = self.account(schedule.account)?;
        if !account.kind.is_account() {
            return Err(StoreError::NotAnAccount(account.id));
        }
        if account.closed {
            return Err(StoreError::ScheduleAccountClosed(account.id));
        }
        if account.currency != schedule.amount.currency() {
            return Err(StoreError::ScheduleCurrencyDiffers {
                account: account.id,
                currency: schedule.amount.currency(),
            });
        }
        if let Some(category) = schedule.category {
            let found = match self.account(category) {
                Ok(found) => found,
                Err(StoreError::UnknownAccount(_)) => {
                    return Err(StoreError::ScheduleCategory(category));
                }
                Err(error) => return Err(error),
            };
            if !found.kind.is_category() || found.currency != schedule.amount.currency() {
                return Err(StoreError::ScheduleCategory(category));
            }
        }
        Ok(())
    }

    fn insert_schedule(&self, schedule: &Schedule) -> Result<(), StoreError> {
        self.conn.execute_cached(
            "INSERT INTO schedules (id, name, account_id, amount, currency, tolerance,
                 category_id, party_iban, party_name, unit, every, anchor, track_from,
                 ends_on, paused)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                schedule.id.to_bytes(),
                schedule.name,
                schedule.account.to_bytes(),
                schedule.amount.minor_units(),
                schedule.amount.currency().as_str(),
                schedule.tolerance.minor_units(),
                schedule.category.map(AccountId::to_bytes),
                schedule.party_iban,
                schedule.party_name,
                schedule.unit.word(),
                schedule.every,
                date_to_text(schedule.anchor)?,
                date_to_text(schedule.track_from)?,
                schedule.ends_on.map(date_to_text).transpose()?,
                schedule.paused,
            ],
        )?;
        Ok(())
    }

    /// The plan "Plan this again…" offers for the entry `transaction` in
    /// `account` (2.1), or `None` where no plan could ever match it: an
    /// entry with more than one part among the user's accounts (a
    /// Transfer), none in `account`, or one in another currency or of
    /// zero. The category is the other part's only where that is a
    /// category in the entry's currency.
    pub fn plan_from_entry(
        &self,
        transaction: TransactionId,
        account: AccountId,
        fallback_name: &str,
    ) -> Result<Option<NewSchedule>, StoreError> {
        let stored = self.transaction(transaction)?;
        let accounts = self.account_ids()?;
        if Self::as_entry(&stored, account, &accounts).is_none() {
            return Ok(None);
        }
        let Some(mut new) = NewSchedule::from_entry(&stored, account, fallback_name) else {
            return Ok(None);
        };
        if new.name.is_empty() {
            fallback_name.clone_into(&mut new.name);
        }
        if let Some(category) = new.category {
            let found = self.account(category)?;
            if !found.kind.is_category() || found.currency != new.amount.currency() {
                new.category = None;
            }
        }
        Ok(Some(new))
    }

    /// Makes the planned payment `new` (D-769), the user action
    /// [`Action::AddSchedule`], and returns its ID.
    ///
    /// The name loses the spaces around it; the other party's name and IBAN
    /// are cut to 140 characters, never refused. Refused: a plan that isn't
    /// valid by itself ([`StoreError::Schedule`]), on something other than
    /// one of the user's accounts ([`StoreError::NotAnAccount`]), on a
    /// closed one ([`StoreError::ScheduleAccountClosed`]), in another
    /// currency than its account ([`StoreError::ScheduleCurrencyDiffers`]),
    /// with a category that isn't one in that currency
    /// ([`StoreError::ScheduleCategory`]), or a plan more than the book
    /// holds ([`StoreError::TooManySchedules`]).
    pub fn add_schedule(&mut self, new: &NewSchedule) -> Result<ScheduleId, StoreError> {
        let schedule = Schedule {
            id: ScheduleId::generate(),
            name: new.name.trim().to_owned(),
            account: new.account,
            amount: new.amount,
            tolerance: new.tolerance,
            category: new.category,
            party_iban: cut_party(new.party_iban.as_deref()),
            party_name: cut_party(new.party_name.as_deref()),
            unit: new.unit,
            every: new.every,
            anchor: new.anchor,
            track_from: new.track_from,
            ends_on: new.ends_on,
            paused: false,
        };
        self.check_schedule(&schedule)?;
        let count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM schedules", [], |row| row.get(0))?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_SCHEDULES {
            return Err(StoreError::TooManySchedules { max: MAX_SCHEDULES });
        }
        self.as_one_action(Action::AddSchedule, |book| book.insert_schedule(&schedule))?;
        Ok(schedule.id)
    }

    /// Puts `schedules` and their decided occurrences into a new book being
    /// filled from a complete export (D-029), with their IDs, checked as
    /// [`Book::add_schedule`] checks a new one. Plans of an account that is
    /// closed later in the fill come in while it is still open. A matched
    /// occurrence must name an entry in the book
    /// ([`StoreError::UnknownTransaction`]) that no other one names
    /// ([`StoreError::ScheduleEntryTaken`]).
    pub fn restore_schedules(
        &mut self,
        schedules: &[Schedule],
        decided: &[(ScheduleId, Date, Decided)],
    ) -> Result<(), StoreError> {
        if schedules.len() > MAX_SCHEDULES {
            return Err(StoreError::TooManySchedules { max: MAX_SCHEDULES });
        }
        for schedule in schedules {
            self.check_schedule(schedule)?;
        }
        let mut taken = HashSet::new();
        for &(_, _, state) in decided {
            if let Decided::Matched(transaction) = state {
                self.transaction(transaction)?;
                if !taken.insert(transaction) {
                    return Err(StoreError::ScheduleEntryTaken(transaction));
                }
            }
        }
        self.as_one_action(Action::AddSchedule, |book| {
            for schedule in schedules {
                book.insert_schedule(schedule)?;
            }
            for &(schedule, due_on, state) in decided {
                book.schedule(schedule)?;
                book.insert_occurrence(schedule, due_on, state)?;
            }
            Ok::<(), StoreError>(())
        })
    }

    /// Changes the planned payment with `schedule`'s ID to `schedule` (the
    /// user action [`Action::EditSchedule`]), checked as
    /// [`Book::add_schedule`] checks a new one, but keeping how many plans
    /// there are. Occurrences matched or skipped before stay as they are:
    /// their entries are not changed.
    pub fn edit_schedule(&mut self, schedule: &Schedule) -> Result<(), StoreError> {
        self.schedule(schedule.id)?;
        let schedule = Schedule {
            name: schedule.name.trim().to_owned(),
            party_iban: cut_party(schedule.party_iban.as_deref()),
            party_name: cut_party(schedule.party_name.as_deref()),
            ..schedule.clone()
        };
        self.check_schedule(&schedule)?;
        self.as_one_action(Action::EditSchedule, |book| {
            book.conn.execute_cached(
                "UPDATE schedules SET name = ?2, account_id = ?3, amount = ?4, currency = ?5,
                     tolerance = ?6, category_id = ?7, party_iban = ?8, party_name = ?9,
                     unit = ?10, every = ?11, anchor = ?12, track_from = ?13, ends_on = ?14,
                     paused = ?15
                 WHERE id = ?1",
                params![
                    schedule.id.to_bytes(),
                    schedule.name,
                    schedule.account.to_bytes(),
                    schedule.amount.minor_units(),
                    schedule.amount.currency().as_str(),
                    schedule.tolerance.minor_units(),
                    schedule.category.map(AccountId::to_bytes),
                    schedule.party_iban,
                    schedule.party_name,
                    schedule.unit.word(),
                    schedule.every,
                    date_to_text(schedule.anchor)?,
                    date_to_text(schedule.track_from)?,
                    schedule.ends_on.map(date_to_text).transpose()?,
                    schedule.paused,
                ],
            )?;
            Ok::<(), StoreError>(())
        })
    }

    /// Pauses the planned payment `id`, or lets it go on again (the user
    /// action [`Action::PauseSchedule`]): a paused plan is not expected,
    /// matched or counted. Nothing is recorded when it is already so.
    pub fn set_schedule_paused(&mut self, id: ScheduleId, paused: bool) -> Result<(), StoreError> {
        self.schedule(id)?;
        self.as_one_action(Action::PauseSchedule, |book| {
            book.conn.execute_cached(
                "UPDATE schedules SET paused = ?2 WHERE id = ?1",
                params![id.to_bytes(), paused],
            )?;
            Ok::<(), StoreError>(())
        })
    }

    /// Removes the planned payment `id` (the user action
    /// [`Action::RemoveSchedule`]): entries matched to it stay as they are,
    /// KassenSturz just stops expecting it. Undo brings it back with what
    /// was decided about its occurrences.
    pub fn remove_schedule(&mut self, id: ScheduleId) -> Result<(), StoreError> {
        self.schedule(id)?;
        self.as_one_action(Action::RemoveSchedule, |book| {
            book.conn
                .execute_cached("DELETE FROM schedules WHERE id = ?1", [id.to_bytes()])?;
            Ok::<(), StoreError>(())
        })
    }

    /// Whether `schedule` is expected at all: not paused, and its account
    /// open (closing an account pauses its plans; reopening it lets them go
    /// on, 2.7).
    fn is_active(&self, schedule: &Schedule) -> Result<bool, StoreError> {
        Ok(!schedule.paused && !self.account(schedule.account)?.closed)
    }

    /// The plans that are expected: not paused, on an open account.
    fn active_schedules(&self) -> Result<Vec<Schedule>, StoreError> {
        let mut active = Vec::new();
        for schedule in self.schedules()? {
            if self.is_active(&schedule)? {
                active.push(schedule);
            }
        }
        Ok(active)
    }

    /// The occurrences of every expected plan whose (moved) date is from
    /// `from` to `to`, with their state on `today`, sorted by date, then
    /// plan. Paused plans and plans of closed accounts have none.
    pub fn planned_occurrences(
        &self,
        from: Date,
        to: Date,
        today: Date,
    ) -> Result<Vec<PlannedOccurrence>, StoreError> {
        let decided = self.decided_occurrences()?;
        let mut found = Vec::new();
        for schedule in self.active_schedules()? {
            for due in due_dates(&schedule, from, to) {
                let decision = decided.get(&(schedule.id, due.counted)).copied();
                found.push(PlannedOccurrence {
                    schedule: schedule.id,
                    due,
                    state: OccurrenceState::of(due.date, decision, today),
                    transaction: match decision {
                        Some(Decided::Matched(transaction)) => Some(transaction),
                        _ => None,
                    },
                });
            }
        }
        found.sort_by_key(|occurrence| (occurrence.due.date, occurrence.schedule));
        Ok(found)
    }

    /// The next open occurrence of `id` due today or later, if the plan is
    /// expected and has one within five years ("next: Mon 2 Nov").
    pub fn next_due(&self, id: ScheduleId, today: Date) -> Result<Option<Due>, StoreError> {
        let schedule = self.schedule(id)?;
        if !self.is_active(&schedule)? {
            return Ok(None);
        }
        let decided = self.decided_occurrences()?;
        let until = years_from(today, LOOK_BACK_YEARS).ok_or_else(|| out_of_calendar(today))?;
        Ok(due_dates(&schedule, today, until)
            .into_iter()
            .find(|due| !decided.contains_key(&(schedule.id, due.counted))))
    }

    /// The occurrence of `schedule` counted on `due_on`, if it has one
    /// ([`StoreError::UnknownOccurrence`]).
    fn occurrence(&self, schedule: &Schedule, due_on: Date) -> Result<Due, StoreError> {
        let unknown = StoreError::UnknownOccurrence {
            schedule: schedule.id,
            due_on,
        };
        // A moved date is at most two days after its counted one.
        let Some(until) = due_on
            .tomorrow()
            .and_then(|day| day.tomorrow())
            .and_then(|day| day.tomorrow())
            .ok()
        else {
            return Err(unknown);
        };
        due_dates(schedule, due_on, until)
            .into_iter()
            .find(|due| due.counted == due_on)
            .ok_or(unknown)
    }

    fn insert_occurrence(
        &self,
        schedule: ScheduleId,
        due_on: Date,
        state: Decided,
    ) -> Result<(), StoreError> {
        let (status, transaction) = match state {
            Decided::Matched(transaction) => ("matched", Some(transaction.to_bytes())),
            Decided::Skipped => ("skipped", None),
        };
        self.conn.execute_cached(
            "INSERT INTO schedule_occurrences (id, schedule_id, due_on, status, transaction_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                ScheduleId::generate().to_bytes(),
                schedule.to_bytes(),
                date_to_text(due_on)?,
                status,
                transaction,
            ],
        )?;
        Ok(())
    }

    /// The transactions matched to an occurrence already.
    fn matched_transactions(&self) -> Result<HashSet<TransactionId>, StoreError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT transaction_id FROM schedule_occurrences WHERE transaction_id IS NOT NULL",
        )?;
        let rows = statement
            .query_map([], |row| row.get::<_, [u8; 16]>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(TransactionId::from_bytes).collect())
    }

    /// `transaction` as an entry that may be matched to a plan on
    /// `account`: exactly one part in one of the user's accounts, that one
    /// in `account`, in the transaction's currency, not zero.
    fn as_entry(
        transaction: &Transaction,
        account: AccountId,
        accounts: &HashSet<AccountId>,
    ) -> Option<ScheduleEntry> {
        let mut own = transaction
            .splits()
            .iter()
            .filter(|split| split.account.is_some_and(|id| accounts.contains(&id)));
        let part = own.next()?;
        if own.next().is_some()
            || part.account != Some(account)
            || part.foreign.is_some()
            || part.amount.is_zero()
        {
            return None;
        }
        let party = part
            .imported
            .as_ref()
            .map_or_else(OtherParty::default, |imported| {
                OtherParty::new(
                    imported.other_party_iban.as_deref(),
                    imported.other_party_name.as_deref(),
                )
            });
        Some(ScheduleEntry {
            transaction: transaction.id(),
            account,
            amount: part.amount,
            booking_date: transaction.booking_date(),
            party,
        })
    }

    /// The transactions that may not be matched to a plan: those matched
    /// already, the sides of a Transfer the book suggests, and the sides
    /// of a remembered monthly move whose other half may still come
    /// (3.4 point 4).
    fn not_matchable(&self) -> Result<HashSet<TransactionId>, StoreError> {
        let mut taken = self.matched_transactions()?;
        for found in self.transfer_suggestions()? {
            taken.insert(found.outgoing.transaction);
            taken.insert(found.incoming.transaction);
        }
        taken.extend(self.waiting_for_moves()?);
        Ok(taken)
    }

    /// Gives the part of `transaction` without a category the category of
    /// `schedule`, if it has exactly two parts, the account's and one
    /// without a category, and the plan has a category. A Rule's or the
    /// user's category always stays (D-031). Returns whether it did.
    fn fill_category(
        &mut self,
        schedule: &Schedule,
        transaction: TransactionId,
    ) -> Result<bool, StoreError> {
        let Some(category) = schedule.category else {
            return Ok(false);
        };
        let stored = self.transaction(transaction)?;
        let [first, second] = stored.splits() else {
            return Ok(false);
        };
        let open = match (first.account, second.account) {
            (Some(account), None) if account == schedule.account => second.id,
            (None, Some(account)) if account == schedule.account => first.id,
            _ => return Ok(false),
        };
        if stored.splits().iter().any(|split| split.foreign.is_some())
            || self.account(category)?.currency != stored.currency()
        {
            return Ok(false);
        }
        let splits: Vec<Split> = stored
            .splits()
            .iter()
            .map(|split| {
                let mut split = split.clone();
                if split.id == open {
                    split.account = Some(category);
                }
                split
            })
            .collect();
        let changed = Transaction::new(
            stored.id(),
            stored.booking_date(),
            stored.payee().map(str::to_owned),
            splits,
        )
        .map_err(|error| invalid(stored.id(), error))?
        .with_details_of(&stored);
        self.update_transaction(&changed)?;
        Ok(true)
    }

    /// Matches the entries that fit open occurrences of the expected plans
    /// (D-769, 3.4 and 3.5), as one user action of its own,
    /// [`Action::MatchSchedules`]: one Undo takes back the matches, and the
    /// categories they gave, and leaves the entries. Nothing is recorded
    /// when nothing fits.
    ///
    /// It looks at every open occurrence from [`LOOK_BACK_YEARS`] before
    /// `today` up to [`MAX_BUSINESS_DAYS`] banking days after it, so a late
    /// file for an old month matches too, and running it twice matches
    /// nothing new. With `involving`, only matches of those entries are
    /// made, e.g. the ones an import added, so an entry whose match the
    /// user took back is not matched again behind their back; occurrences
    /// that need the user are reported only when one of their entries is
    /// among them. Which entries fit is [`match_schedules`]'s decision, on
    /// all entries of the plans' accounts, so a new entry never wins over
    /// an old one that fits as well.
    pub fn match_schedules(
        &mut self,
        today: Date,
        involving: Option<&HashSet<TransactionId>>,
    ) -> Result<ScheduleMatchReport, StoreError> {
        let plans = self.active_schedules()?;
        if plans.is_empty() {
            return Ok(ScheduleMatchReport::default());
        }
        let decided = self.decided_occurrences()?;
        let from = years_from(today, -LOOK_BACK_YEARS).ok_or_else(|| out_of_calendar(today))?;
        let to =
            add_banking_days(today, MAX_BUSINESS_DAYS).ok_or_else(|| out_of_calendar(today))?;
        let mut open = Vec::new();
        for plan in &plans {
            for due in due_dates(plan, from, to) {
                if !decided.contains_key(&(plan.id, due.counted)) {
                    open.push(OpenOccurrence {
                        schedule: plan.id,
                        due,
                    });
                }
            }
        }
        if open.is_empty() {
            return Ok(ScheduleMatchReport::default());
        }
        let taken = self.not_matchable()?;
        let accounts = self.account_ids()?;
        let plan_accounts: HashSet<AccountId> = plans.iter().map(|plan| plan.account).collect();
        let mut entries = Vec::new();
        for &account in &plan_accounts {
            for transaction in self.transactions_of(account)? {
                if taken.contains(&transaction.id()) {
                    continue;
                }
                if let Some(entry) = Self::as_entry(&transaction, account, &accounts) {
                    entries.push(entry);
                }
            }
        }
        let found = match_schedules(&plans, &open, &entries);
        let wanted = |ids: &[TransactionId]| {
            involving.is_none_or(|involving| ids.iter().any(|id| involving.contains(id)))
        };
        let mut report = ScheduleMatchReport::default();
        for unclear in found.unclear {
            if wanted(&unclear.transactions) {
                report.needs_you.push(ScheduleNeedsYou {
                    occurrence: unclear.occurrence,
                    transactions: unclear.transactions,
                });
            }
        }
        let matches: Vec<_> = found
            .matches
            .into_iter()
            .filter(|found| wanted(&[found.transaction]))
            .collect();
        if matches.is_empty() {
            return Ok(report);
        }
        let by_id: HashMap<ScheduleId, &Schedule> =
            plans.iter().map(|plan| (plan.id, plan)).collect();
        report.matched = self.as_one_action(Action::MatchSchedules, |book| {
            let mut done = Vec::with_capacity(matches.len());
            for found in &matches {
                let plan = by_id[&found.occurrence.schedule];
                book.insert_occurrence(
                    plan.id,
                    found.occurrence.due.counted,
                    Decided::Matched(found.transaction),
                )?;
                let category_set = book.fill_category(plan, found.transaction)?;
                done.push(ScheduleMatchDone {
                    occurrence: found.occurrence,
                    transaction: found.transaction,
                    category_set,
                });
            }
            Ok::<_, StoreError>(done)
        })?;
        Ok(report)
    }

    /// The decided occurrence of `schedule` on `due_on` must not exist yet
    /// ([`StoreError::OccurrenceDecided`]).
    fn check_open(&self, schedule: ScheduleId, due_on: Date) -> Result<(), StoreError> {
        if self
            .decided_occurrences()?
            .contains_key(&(schedule, due_on))
        {
            return Err(StoreError::OccurrenceDecided { schedule, due_on });
        }
        Ok(())
    }

    /// Matches the entry `transaction` the user picked to the occurrence of
    /// `schedule` counted on `due_on` (the user action
    /// [`Action::MatchScheduleOccurrence`]), whatever its amount and date:
    /// the user decides. It must have its one part among the user's
    /// accounts in the plan's account, in the plan's currency, with money
    /// going the same way, and be no side of a Transfer
    /// ([`StoreError::ScheduleEntryNotFitting`]); it can't be matched to a
    /// second occurrence ([`StoreError::ScheduleEntryTaken`]). As an
    /// automatic match, it gives the entry the plan's category only where
    /// its part has none.
    pub fn match_schedule_occurrence(
        &mut self,
        schedule: ScheduleId,
        due_on: Date,
        transaction: TransactionId,
    ) -> Result<(), StoreError> {
        let plan = self.schedule(schedule)?;
        self.occurrence(&plan, due_on)?;
        self.check_open(schedule, due_on)?;
        if self.matched_transactions()?.contains(&transaction) {
            return Err(StoreError::ScheduleEntryTaken(transaction));
        }
        let stored = self.transaction(transaction)?;
        let accounts = self.account_ids()?;
        let fits = Self::as_entry(&stored, plan.account, &accounts).is_some_and(|entry| {
            entry.amount.currency() == plan.amount.currency()
                && entry.amount.is_negative() == plan.amount.is_negative()
        });
        if !fits {
            return Err(StoreError::ScheduleEntryNotFitting(transaction));
        }
        self.as_one_action(Action::MatchScheduleOccurrence, |book| {
            book.insert_occurrence(schedule, due_on, Decided::Matched(transaction))?;
            book.fill_category(&plan, transaction)?;
            Ok::<(), StoreError>(())
        })
    }

    /// Skips the occurrence of `schedule` counted on `due_on` this time (the
    /// user action [`Action::SkipScheduleOccurrence`]): it is no longer
    /// expected or counted.
    pub fn skip_schedule_occurrence(
        &mut self,
        schedule: ScheduleId,
        due_on: Date,
    ) -> Result<(), StoreError> {
        let plan = self.schedule(schedule)?;
        self.occurrence(&plan, due_on)?;
        self.check_open(schedule, due_on)?;
        self.as_one_action(Action::SkipScheduleOccurrence, |book| {
            book.insert_occurrence(schedule, due_on, Decided::Skipped)
        })
    }

    /// Takes back the skip of the occurrence of `schedule` counted on
    /// `due_on` (the user action [`Action::UnskipScheduleOccurrence`]), so
    /// it is open again. Refused when it isn't skipped
    /// ([`StoreError::OccurrenceDecided`]): a match is taken back by Undo or
    /// by removing the entry.
    pub fn unskip_schedule_occurrence(
        &mut self,
        schedule: ScheduleId,
        due_on: Date,
    ) -> Result<(), StoreError> {
        self.schedule(schedule)?;
        if self.decided_occurrences()?.get(&(schedule, due_on)) != Some(&Decided::Skipped) {
            return Err(StoreError::OccurrenceDecided { schedule, due_on });
        }
        self.as_one_action(Action::UnskipScheduleOccurrence, |book| {
            book.conn.execute_cached(
                "DELETE FROM schedule_occurrences
                 WHERE schedule_id = ?1 AND due_on = ?2 AND status = 'skipped'",
                params![schedule.to_bytes(), date_to_text(due_on)?],
            )?;
            Ok::<(), StoreError>(())
        })
    }

    /// Books the occurrence of `schedule` counted on `due_on` as a new
    /// entry, on the user's click ("Book it", 2.5), the one place a plan
    /// creates one: on `date`, of `amount` (the plan's or one the user
    /// changed, in the plan's currency, money going the same way), into the
    /// plan's account, with one other part in the plan's category or
    /// without one, and the other party's name as payee. It is matched to
    /// the occurrence in the same user action,
    /// [`Action::BookScheduleOccurrence`], which one Undo takes back.
    /// Returns the new entry's ID.
    pub fn book_schedule_occurrence(
        &mut self,
        schedule: ScheduleId,
        due_on: Date,
        date: Date,
        amount: Money,
    ) -> Result<TransactionId, StoreError> {
        let plan = self.schedule(schedule)?;
        self.occurrence(&plan, due_on)?;
        self.check_open(schedule, due_on)?;
        if self.account(plan.account)?.closed {
            return Err(StoreError::ScheduleAccountClosed(plan.account));
        }
        if amount.currency() != plan.amount.currency()
            || amount.is_zero()
            || amount.is_negative() != plan.amount.is_negative()
        {
            return Err(StoreError::Schedule(ScheduleError::BookedAmount));
        }
        let other = amount
            .checked_neg()
            .map_err(|_| StoreError::AmountOverflow)?;
        let other_part = match plan.category {
            Some(category) => Split::new(category, other),
            None => Split::uncategorized(other),
        };
        let transaction = Transaction::new(
            TransactionId::generate(),
            date,
            plan.party_name.clone().or_else(|| Some(plan.name.clone())),
            vec![Split::new(plan.account, amount), other_part],
        );
        let transaction = match transaction {
            Ok(transaction) => transaction,
            Err(_) => return Err(StoreError::AmountOverflow),
        };
        let id = transaction.id();
        self.as_one_action(Action::BookScheduleOccurrence, |book| {
            book.add_transaction(&transaction)?;
            book.insert_occurrence(schedule, due_on, Decided::Matched(id))
        })?;
        Ok(id)
    }

    /// The accounts the forecast counts when the user chose none: the open
    /// checking and cash accounts, sorted by ID. A credit card is not
    /// among them, as its bill is paid from the checking account and would
    /// be counted twice (5.2).
    pub fn default_forecast_accounts(&self) -> Result<Vec<AccountId>, StoreError> {
        let mut chosen: Vec<AccountId> = self
            .accounts(false)?
            .into_iter()
            .filter(|account| {
                account.kind == AccountKind::Asset
                    && matches!(
                        account.account_type,
                        Some(AccountType::Checking | AccountType::Cash)
                    )
            })
            .map(|account| account.id)
            .collect();
        chosen.sort();
        Ok(chosen)
    }

    /// The forecast for today's month and `months` months after it (D-770),
    /// over `accounts` in `currency`: their booked balance at the end of
    /// `today`, and the planned payments of those accounts that are
    /// expected. Accounts in another currency are left out and listed, as
    /// are closed ones; accounts that aren't the user's are refused
    /// ([`StoreError::NotAnAccount`]). Every figure is exact.
    pub fn forecast(
        &self,
        accounts: &[AccountId],
        currency: CurrencyCode,
        today: Date,
        months: usize,
    ) -> Result<AccountsForecast, StoreError> {
        let mut counted = Vec::new();
        let mut left_out = Vec::new();
        for &id in accounts {
            let account = self.account(id)?;
            if !account.kind.is_account() {
                return Err(StoreError::NotAnAccount(id));
            }
            if account.closed {
                continue;
            }
            if account.currency == currency {
                counted.push(id);
            } else {
                left_out.push(id);
            }
        }
        counted.sort();
        counted.dedup();
        left_out.sort();
        left_out.dedup();
        let mut balance = Money::zero(currency);
        for &id in &counted {
            balance = balance
                .checked_add(self.balance_on(id, today)?)
                .map_err(|_| StoreError::AmountOverflow)?;
        }
        let plans: Vec<Schedule> = self
            .active_schedules()?
            .into_iter()
            .filter(|plan| counted.contains(&plan.account))
            .collect();
        let decided = self.decided_occurrences()?;
        let result =
            forecast(balance, &plans, &decided, today, months).map_err(|error| match error {
                ForecastError::Money(_) => StoreError::AmountOverflow,
                ForecastError::Calendar => out_of_calendar(today),
            })?;
        Ok(AccountsForecast {
            forecast: result,
            accounts: counted,
            left_out,
            has_plans: plans.iter().any(|plan| plan.amount.currency() == currency),
        })
    }
}

/// `day` moved by `years` years, cut back to the month's last day (29
/// February in a common year), if that is in the calendar.
fn years_from(day: Date, years: i16) -> Option<Date> {
    let year = day.year().checked_add(years)?;
    let first = Date::new(year, day.month(), 1).ok()?;
    Date::new(year, day.month(), day.day().min(first.days_in_month())).ok()
}

/// A date outside the calendar KassenSturz knows, worked out from `day`.
fn out_of_calendar(day: Date) -> StoreError {
    StoreError::Corrupt(Corruption::BadDate(day.to_string()))
}

/// A transaction KassenSturz built again from a stored one that doesn't
/// hold together.
fn invalid(transaction: TransactionId, error: kassensturz_core::TransactionError) -> StoreError {
    StoreError::Corrupt(Corruption::InvalidTransaction { transaction, error })
}

#[cfg(test)]
mod tests;
