// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Monthly moves between the user's own accounts, as the screens show them
//! (US6.3, D-749, `docs/RECURRING_MOVES.md`): the offer to remember a move
//! when the user joins a pair, the list under Accounts › Monthly moves…, and
//! "Join all".
//!
//! The core decides what a move is and which pairs fit it
//! (`kassensturz_core::recurring_move`, `Book::recurring_move_for`,
//! `Book::add_recurring_move`, `Book::join_transfers`); this module only
//! writes what the screens show as text (D-065) and runs the user's choices
//! as one action each, so one Undo takes each back. Nothing here remembers a
//! move without the user's tick, and nothing joins a pair the user did not
//! see: "Join all" refuses when the suggestions changed since the list was
//! shown.

use kassensturz_core::{Account, AccountId, Date, RecurringMove, Report, TransactionId};
use kassensturz_store::{Action, Book, NewRecurringMove, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::register::{AmountWriter, Shown};
use crate::transfer_prompt::{TransferPromptError, transaction_id};

/// Why a screen's request about monthly moves failed.
#[derive(Debug)]
pub enum MonthlyMovesError {
    /// The pair or the move isn't there (any more), or the suggestions are
    /// no longer the ones the user saw.
    Stale,
    /// The book refused it.
    Store(StoreError),
}

impl From<StoreError> for MonthlyMovesError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<TransferPromptError> for MonthlyMovesError {
    fn from(error: TransferPromptError) -> Self {
        match error {
            TransferPromptError::NoSuchSuggestion => Self::Stale,
            TransferPromptError::Store(error) => Self::Store(error),
        }
    }
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &MonthlyMovesError, locale: &impl Locale) -> ErrorText {
    match error {
        MonthlyMovesError::Stale => ErrorText::plain("input.no-such-entry"),
        MonthlyMovesError::Store(error) => report_text(book, &error.report(), locale),
    }
}

/// The move the user may ask to be joined by itself, for the box in the
/// Transfer preview.
#[derive(Debug, PartialEq, Eq)]
pub struct OfferText {
    /// The exact amount that leaves each month.
    pub amount: Shown,
    /// The account it leaves.
    pub from: String,
    /// The account it arrives in.
    pub to: String,
    /// The day of the month the paying side was booked on.
    pub day: i32,
    /// The other party the paying side names, as delivered; may be empty.
    pub party: String,
}

/// One remembered move, for the list.
#[derive(Debug, PartialEq, Eq)]
pub struct MoveText {
    /// Its ID as text.
    pub id: String,
    /// The account the money leaves.
    pub from: String,
    /// The account it arrives in.
    pub to: String,
    /// The exact amount.
    pub amount: Shown,
    /// Around which day of the month.
    pub day: i32,
    /// The other party it was remembered with; may be empty.
    pub party: String,
    /// The date of the newest Transfer of this move, as the user's locale
    /// writes dates; empty if there is none (M6-B242).
    pub last_joined: String,
}

/// What "Join all" did.
#[derive(Debug, PartialEq, Eq)]
pub struct JoinedAll {
    /// How many Transfers it made.
    pub joined: usize,
    /// How many pairs it left alone.
    pub left: usize,
    /// The moves that look monthly among the joined pairs, to ask about.
    pub looks_monthly: Vec<LooksMonthly>,
}

/// A move that looks monthly after "Join all", offered once with an
/// unticked box.
#[derive(Debug, PartialEq, Eq)]
pub struct LooksMonthly {
    /// The exact amount.
    pub amount: Shown,
    /// The account the money leaves.
    pub from: String,
    /// The account it arrives in.
    pub to: String,
    /// Around which day of the month.
    pub day: i32,
    /// The other party the paying sides name; may be empty.
    pub party: String,
    /// In how many different months it was joined.
    pub months: usize,
}

/// The name a move is remembered with: the other party's name, or its
/// IBAN, or nothing.
fn party_text(iban: Option<&str>, name: Option<&str>) -> String {
    name.filter(|name| !name.trim().is_empty())
        .or(iban)
        .unwrap_or_default()
        .to_owned()
}

fn nothing(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|text| !text.is_empty())
}

/// Whether `moved` is the move `new` would make: the same accounts, amount
/// and other party.
fn is_that_move(moved: &RecurringMove, new: &NewRecurringMove) -> bool {
    moved.from_account == new.from_account
        && moved.to_account == new.to_account
        && moved.amount == new.amount
        && nothing(moved.from_party_iban.as_deref()) == nothing(new.from_party_iban.as_deref())
        && nothing(moved.from_party_name.as_deref()) == nothing(new.from_party_name.as_deref())
}

fn remembered(book: &Book, new: &NewRecurringMove) -> Result<bool, StoreError> {
    Ok(book
        .recurring_moves()?
        .iter()
        .any(|moved| is_that_move(moved, new)))
}

fn account_name(accounts: &[Account], id: AccountId) -> String {
    accounts
        .iter()
        .find(|account| account.id == id)
        .map(|account| account.name.clone())
        .unwrap_or_default()
}

/// `new`, the move of the pair `a` and `b`, with the day it usually comes:
/// read from every pair the book suggests for the same move, as "Join all"
/// does, not from this pair alone, because right after an import the first
/// pair may be a few days off the rest. The offer says this day and joining
/// remembers it, so what the question promised is what is saved (B-261).
fn with_usual_day(
    book: &Book,
    a: TransactionId,
    b: TransactionId,
    mut new: NewRecurringMove,
    locale: &impl Locale,
) -> Result<NewRecurringMove, MonthlyMovesError> {
    let mut days = vec![new.day_of_month];
    for found in book.transfer_suggestions()? {
        let (out, into) = (found.outgoing.transaction, found.incoming.transaction);
        if (out == a && into == b) || (out == b && into == a) {
            continue;
        }
        if let Ok(other) = book.recurring_move_for(out, into, locale.today())
            && is_same_move(&other, &new)
        {
            days.push(other.day_of_month);
        }
    }
    new.day_of_month = usual_day(&days).unwrap_or(new.day_of_month);
    Ok(new)
}

/// The move to offer for the pair `a` and `b` that the user is about to
/// join, or `None` when none can be remembered for it (not one currency, or
/// not two different accounts of the user) or the book remembers it already.
pub fn offer(
    book: &Book,
    a: &str,
    b: &str,
    locale: &impl Locale,
) -> Result<Option<OfferText>, MonthlyMovesError> {
    let (a, b) = (transaction_id(a)?, transaction_id(b)?);
    let new = match book.recurring_move_for(a, b, locale.today()) {
        Ok(new) => new,
        Err(StoreError::Transfer(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if new.from_account == new.to_account || remembered(book, &new)? {
        return Ok(None);
    }
    let new = with_usual_day(book, a, b, new, locale)?;
    let accounts = book.all_accounts()?;
    Ok(Some(OfferText {
        amount: AmountWriter::new(book, new.amount.currency(), locale)?.text(new.amount),
        from: account_name(&accounts, new.from_account),
        to: account_name(&accounts, new.to_account),
        day: i32::from(new.day_of_month),
        party: party_text(
            new.from_party_iban.as_deref(),
            new.from_party_name.as_deref(),
        ),
    }))
}

/// Joins the suggested pair (`outgoing`, `incoming`) the way `transfer_prompt::join`
/// does and, in the same user action, remembers its move, so one Undo takes
/// both back. The user ticked the box after the preview said what it means.
pub fn join_remembering(
    book: &mut Book,
    outgoing: &str,
    incoming: &str,
    locale: &impl Locale,
) -> Result<(), MonthlyMovesError> {
    let (outgoing, incoming) = (transaction_id(outgoing)?, transaction_id(incoming)?);
    let suggested = book.transfer_suggestions()?.into_iter().any(|found| {
        found.outgoing.transaction == outgoing && found.incoming.transaction == incoming
    });
    if !suggested {
        return Err(MonthlyMovesError::Stale);
    }
    join_and_remember(book, outgoing, incoming, locale)
}

/// Joins `chosen` with `picked`, one of its partners the user picked
/// (`transfer_prompt::join_picked`), and remembers the move in the same
/// action.
pub fn join_picked_remembering(
    book: &mut Book,
    chosen: &str,
    picked: &str,
    locale: &impl Locale,
) -> Result<(), MonthlyMovesError> {
    let (chosen, picked) = (transaction_id(chosen)?, transaction_id(picked)?);
    let partner = book
        .transfer_partners(chosen)?
        .into_iter()
        .any(|partner| partner.side.transaction == picked);
    if !partner {
        return Err(MonthlyMovesError::Stale);
    }
    join_and_remember(book, chosen, picked, locale)
}

fn join_and_remember(
    book: &mut Book,
    a: TransactionId,
    b: TransactionId,
    locale: &impl Locale,
) -> Result<(), MonthlyMovesError> {
    // Worked out first: the join removes the receiving transaction.
    let new = book.recurring_move_for(a, b, locale.today())?;
    let known = remembered(book, &new)?;
    let new = with_usual_day(book, a, b, new, locale)?;
    book.as_one_action(Action::JoinTransfer, |book| {
        book.join_transfer(a, b)?;
        if !known {
            book.add_recurring_move(&new)?;
        }
        Ok::<(), StoreError>(())
    })?;
    Ok(())
}

/// The remembered moves, in the order they were remembered.
pub fn list(book: &Book, locale: &impl Locale) -> Result<Vec<MoveText>, MonthlyMovesError> {
    let accounts = book.all_accounts()?;
    book.recurring_moves()?
        .iter()
        .map(|moved| {
            Ok(MoveText {
                id: moved.id.to_string(),
                from: account_name(&accounts, moved.from_account),
                to: account_name(&accounts, moved.to_account),
                amount: AmountWriter::new(book, moved.amount.currency(), locale)?
                    .text(moved.amount),
                day: i32::from(moved.day_of_month),
                party: party_text(
                    moved.from_party_iban.as_deref(),
                    moved.from_party_name.as_deref(),
                ),
                last_joined: last_joined(book, moved)?
                    .map(|date| locale.short_date(date))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// The date of the newest Transfer that is this move: exactly two parts, the
/// amount leaving `from_account` and arriving in `to_account`. The book
/// doesn't note which Transfer a move joined, so this is read from the
/// Transfers themselves; one the user joined by hand counts too, since it is
/// the same move (M6-B242).
fn last_joined(book: &Book, moved: &RecurringMove) -> Result<Option<Date>, StoreError> {
    let leaving = moved.amount.checked_neg().ok();
    Ok(book
        .transactions_of(moved.from_account)?
        .iter()
        .filter_map(|transaction| {
            let [first, second] = transaction.splits() else {
                return None;
            };
            let (paying, arriving) = if first.account == Some(moved.from_account) {
                (first, second)
            } else {
                (second, first)
            };
            (paying.account == Some(moved.from_account)
                && arriving.account == Some(moved.to_account)
                && Some(paying.amount) == leaving
                && arriving.amount == moved.amount)
                .then(|| transaction.date_of(paying))
        })
        .max())
}

/// Stops joining the move `id` by itself. What it joined stays joined.
pub fn remove(book: &mut Book, id: &str) -> Result<(), MonthlyMovesError> {
    let moved = book
        .recurring_moves()?
        .into_iter()
        .find(|moved| moved.id.to_string() == id)
        .ok_or(MonthlyMovesError::Stale)?;
    book.remove_recurring_move(moved.id)?;
    Ok(())
}

/// A move seen in the pairs "Join all" joined, and the months it was seen
/// in.
struct Seen {
    new: NewRecurringMove,
    months: Vec<(i16, i8)>,
    /// The day of the month of each joined pair's paying side.
    days: Vec<u8>,
}

/// The day of the month most of `days` fall on; of days seen equally often,
/// the one seen first. A move is remembered around that day, not around the
/// first pair's, which may be an exception (M6-B242).
fn usual_day(days: &[u8]) -> Option<u8> {
    let mut best: Option<(u8, usize)> = None;
    for &day in days {
        let count = days.iter().filter(|&&other| other == day).count();
        if best.is_none_or(|(_, most)| count > most) {
            best = Some((day, count));
        }
    }
    best.map(|(day, _)| day)
}

/// Joins every pair the book suggests right now, as one action, after the
/// user saw `expected` of them (D-749, 3.6); the book refuses a pair whose
/// join would lose something. Returns what it did and the moves that look
/// monthly (same two accounts, same amount, same other party, in at least
/// two different months) and are not remembered yet; they come back to the
/// screen for the user to tick, and are kept for [`remember_looks_monthly`].
pub fn join_all(
    book: &mut Book,
    expected: usize,
    locale: &impl Locale,
) -> Result<(JoinedAll, Vec<NewRecurringMove>), MonthlyMovesError> {
    let suggested = book.transfer_suggestions()?;
    if suggested.len() != expected {
        return Err(MonthlyMovesError::Stale);
    }
    let today = locale.today();
    // Worked out before the join, which removes each receiving transaction.
    let mut paying: Vec<(TransactionId, Option<usize>)> = Vec::new();
    let mut moves: Vec<Seen> = Vec::new();
    let mut months_of: Vec<(TransactionId, (i16, i8), u8)> = Vec::new();
    for found in &suggested {
        let (outgoing, incoming) = (found.outgoing, found.incoming);
        let group = match book.recurring_move_for(outgoing.transaction, incoming.transaction, today)
        {
            Ok(new) => Some(
                match moves.iter().position(|seen| is_same_move(&seen.new, &new)) {
                    Some(at) => at,
                    None => {
                        moves.push(Seen {
                            new,
                            months: Vec::new(),
                            days: Vec::new(),
                        });
                        moves.len() - 1
                    }
                },
            ),
            Err(StoreError::Transfer(_)) => None,
            Err(error) => return Err(error.into()),
        };
        paying.push((outgoing.transaction, group));
        months_of.push((
            outgoing.transaction,
            (outgoing.booking_date.year(), outgoing.booking_date.month()),
            u8::try_from(outgoing.booking_date.day()).unwrap_or(1),
        ));
    }
    let pairs: Vec<(TransactionId, TransactionId)> = suggested
        .iter()
        .map(|found| (found.outgoing.transaction, found.incoming.transaction))
        .collect();
    let done = book.join_transfers(&pairs)?;
    for (transaction, group) in &paying {
        let Some(group) = group else { continue };
        if !done.joined.contains(transaction) {
            continue;
        }
        if let Some((_, month, day)) = months_of.iter().find(|(id, ..)| id == transaction) {
            let seen = &mut moves[*group];
            seen.days.push(*day);
            if !seen.months.contains(month) {
                seen.months.push(*month);
            }
        }
    }
    let accounts = book.all_accounts()?;
    let mut candidates = Vec::new();
    let mut looks = Vec::new();
    for seen in moves {
        if seen.months.len() < 2 || remembered(book, &seen.new)? {
            continue;
        }
        let mut new = seen.new;
        new.day_of_month = usual_day(&seen.days).unwrap_or(new.day_of_month);
        looks.push(LooksMonthly {
            amount: AmountWriter::new(book, new.amount.currency(), locale)?.text(new.amount),
            from: account_name(&accounts, new.from_account),
            to: account_name(&accounts, new.to_account),
            day: i32::from(new.day_of_month),
            party: party_text(
                new.from_party_iban.as_deref(),
                new.from_party_name.as_deref(),
            ),
            months: seen.months.len(),
        });
        candidates.push(new);
    }
    Ok((
        JoinedAll {
            joined: done.joined.len(),
            left: done.refused.len(),
            looks_monthly: looks,
        },
        candidates,
    ))
}

/// The same move: the same accounts, amount and other party. The day of the
/// month may differ between pairs; the most usual one counts
/// ([`usual_day`]).
fn is_same_move(a: &NewRecurringMove, b: &NewRecurringMove) -> bool {
    a.from_account == b.from_account
        && a.to_account == b.to_account
        && a.amount == b.amount
        && nothing(a.from_party_iban.as_deref()) == nothing(b.from_party_iban.as_deref())
        && nothing(a.from_party_name.as_deref()) == nothing(b.from_party_name.as_deref())
}

/// Remembers the moves among `candidates` that the user ticked, as one
/// action, so one Undo takes all back. `ticked` counts from 0; an index
/// that isn't a candidate is refused.
pub fn remember_looks_monthly(
    book: &mut Book,
    candidates: &[NewRecurringMove],
    ticked: &[usize],
) -> Result<(), MonthlyMovesError> {
    let chosen: Vec<&NewRecurringMove> = ticked
        .iter()
        .map(|&index| candidates.get(index).ok_or(MonthlyMovesError::Stale))
        .collect::<Result<_, _>>()?;
    if chosen.is_empty() {
        return Ok(());
    }
    book.as_one_action(Action::AddRecurringMove, |book| {
        for new in chosen {
            if !remembered(book, new)? {
                book.add_recurring_move(new)?;
            }
        }
        Ok::<(), StoreError>(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountType, Money};

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::transaction_entry::{self, NewEntry};
    use crate::transfer_prompt;

    fn account(book: &mut Book, name: &str, account_type: AccountType) -> AccountId {
        add_account(
            book,
            &NewAccount {
                name,
                account_type,
                currency: "EUR",
                balance: "1000",
                date: "2026-09-01",
            },
            &TestLocale,
        )
        .expect("account added")
    }

    fn typed(
        book: &mut Book,
        account: AccountId,
        money_in: bool,
        other_party: &str,
        date: &str,
    ) -> String {
        transaction_entry::add(
            book,
            &NewEntry {
                account: &account.to_string(),
                money_in,
                amount: "100",
                other_party,
                category: "",
                date,
                note: None,
                tags: None,
            },
            &TestLocale,
        )
        .expect("transaction added")
        .to_string()
    }

    /// Checking and savings, and a standing order of 100 € that both
    /// accounts show on the 1st of each of `months` (months 10 and up are
    /// 2026-<month>, so 10 = October).
    fn standing_orders(months: &[u8]) -> (Book, AccountId, AccountId, Vec<(String, String)>) {
        let mut book = started();
        let checking = account(&mut book, "Checking account", AccountType::Checking);
        let savings = account(&mut book, "Savings", AccountType::Savings);
        let mut pairs = Vec::new();
        for month in months {
            let date = format!("2026-{month:02}-01");
            let out = typed(&mut book, checking, false, "Standing order", &date);
            let arrived = typed(&mut book, savings, true, "From checking", &date);
            pairs.push((out, arrived));
        }
        (book, checking, savings, pairs)
    }

    #[test]
    fn the_offer_names_the_move_and_joining_with_the_box_ticked_remembers_it() {
        let (mut book, _, _, pairs) = standing_orders(&[10]);
        let (out, arrived) = &pairs[0];
        let offer = offer(&book, out, arrived, &TestLocale)
            .expect("offer")
            .expect("one is offered");
        assert_eq!(
            (
                offer.amount.exact.as_str(),
                offer.from.as_str(),
                offer.to.as_str(),
                offer.day,
            ),
            ("100,00 €", "Checking account", "Savings", 1)
        );
        join_remembering(&mut book, out, arrived, &TestLocale).expect("joined");
        let moves = list(&book, &TestLocale).expect("list");
        assert_eq!(moves.len(), 1);
        assert_eq!(
            (moves[0].from.as_str(), moves[0].to.as_str(), moves[0].day),
            ("Checking account", "Savings", 1)
        );
        assert_eq!(moves[0].amount.exact, "100,00 €");
        // One change: Undo takes the join and the move back together.
        book.undo().expect("undone");
        assert!(list(&book, &TestLocale).expect("list").is_empty());
        assert_eq!(
            transfer_prompt::suggestions(&book, "", &TestLocale)
                .expect("suggested")
                .len(),
            1
        );
    }

    #[test]
    fn joining_without_the_box_remembers_nothing_and_a_known_move_is_not_offered_again() {
        let (mut book, checking, savings, pairs) = standing_orders(&[10, 11]);
        transfer_prompt::join(&mut book, &pairs[0].0, &pairs[0].1).expect("joined");
        assert!(list(&book, &TestLocale).expect("list").is_empty());
        join_remembering(&mut book, &pairs[1].0, &pairs[1].1, &TestLocale).expect("joined");
        assert_eq!(list(&book, &TestLocale).expect("list").len(), 1);
        // The same move, with another pair of entries, is not offered again.
        let out = typed(&mut book, checking, false, "Standing order", "2026-12-01");
        let arrived = typed(&mut book, savings, true, "From checking", "2026-12-01");
        assert_eq!(
            offer(&book, &out, &arrived, &TestLocale).expect("offer"),
            None
        );
        // Two entries that are not a pair offer nothing and say nothing.
        assert!(matches!(
            offer(&book, "nonsense", &arrived, &TestLocale),
            Err(MonthlyMovesError::Stale)
        ));
    }

    #[test]
    fn the_offer_takes_the_usual_day_from_all_pairs_not_from_the_first_alone() {
        let (mut book, checking, savings, pairs) = standing_orders(&[10, 11]);
        // An older pair three days off the usual 1st.
        let out = typed(&mut book, checking, false, "Standing order", "2026-08-03");
        let arrived = typed(&mut book, savings, true, "From checking", "2026-08-03");
        let offer_for = |out: &str, arrived: &str| {
            offer(&book, out, arrived, &TestLocale)
                .expect("offer")
                .expect("one is offered")
                .day
        };
        assert_eq!(offer_for(&out, &arrived), 1);
        assert_eq!(offer_for(&pairs[0].0, &pairs[0].1), 1);
    }

    #[test]
    fn joining_remembers_the_day_the_offer_named() {
        let (mut book, checking, savings, pairs) = standing_orders(&[10, 11]);
        let out = typed(&mut book, checking, false, "Standing order", "2026-08-03");
        let arrived = typed(&mut book, savings, true, "From checking", "2026-08-03");
        let named = offer(&book, &out, &arrived, &TestLocale)
            .expect("offer")
            .expect("one is offered")
            .day;
        join_remembering(&mut book, &out, &arrived, &TestLocale).expect("joined");
        let moves = list(&book, &TestLocale).expect("list");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].day, named);
        assert_eq!(named, 1);
        let _ = pairs;
    }

    #[test]
    fn a_picked_pair_remembers_the_move_too() {
        let (mut book, _, _, pairs) = standing_orders(&[10]);
        join_picked_remembering(&mut book, &pairs[0].1, &pairs[0].0, &TestLocale).expect("joined");
        assert_eq!(list(&book, &TestLocale).expect("list").len(), 1);
        assert!(matches!(
            join_picked_remembering(&mut book, &pairs[0].1, &pairs[0].0, &TestLocale),
            Err(MonthlyMovesError::Stale | MonthlyMovesError::Store(_))
        ));
    }

    #[test]
    fn removing_a_move_keeps_the_joins_and_undo_brings_it_back() {
        let (mut book, _, _, pairs) = standing_orders(&[10]);
        join_remembering(&mut book, &pairs[0].0, &pairs[0].1, &TestLocale).expect("joined");
        let id = list(&book, &TestLocale).expect("list")[0].id.clone();
        remove(&mut book, &id).expect("removed");
        assert!(list(&book, &TestLocale).expect("list").is_empty());
        // What was joined stays joined.
        assert!(
            transfer_prompt::suggestions(&book, "", &TestLocale)
                .expect("suggested")
                .is_empty()
        );
        assert!(matches!(
            remove(&mut book, &id),
            Err(MonthlyMovesError::Stale)
        ));
        book.undo().expect("undone");
        assert_eq!(list(&book, &TestLocale).expect("list").len(), 1);
    }

    /// Three pairs, the first on the 3rd and two on the 1st: the move is
    /// remembered around the 1st, the usual day, not the first pair's
    /// (M6-B242, US6 P3 VM round 1, A4a).
    #[test]
    fn join_all_offers_the_usual_day_and_the_list_says_when_it_was_last_joined() {
        let mut book = started();
        let checking = account(&mut book, "Checking account", AccountType::Checking);
        let savings = account(&mut book, "Savings", AccountType::Savings);
        for date in ["2026-08-03", "2026-09-01", "2026-10-01"] {
            typed(&mut book, checking, false, "Standing order", date);
            typed(&mut book, savings, true, "From checking", date);
        }
        let (done, candidates) = join_all(&mut book, 3, &TestLocale).expect("joined");
        assert_eq!(done.joined, 3);
        assert_eq!(done.looks_monthly.len(), 1);
        assert_eq!(done.looks_monthly[0].day, 1);
        assert_eq!(candidates[0].day_of_month, 1);
        remember_looks_monthly(&mut book, &candidates, &[0]).expect("remembered");
        let moves = list(&book, &TestLocale).expect("list");
        assert_eq!(moves[0].day, 1);
        // The newest of the three joined Transfers.
        assert_eq!(
            moves[0].last_joined,
            TestLocale.short_date(Date::constant(2026, 10, 1))
        );
    }

    #[test]
    fn a_move_never_joined_has_no_last_date() {
        let mut book = started();
        let checking = account(&mut book, "Checking account", AccountType::Checking);
        let savings = account(&mut book, "Savings", AccountType::Savings);
        // Entries of the same amount that aren't a Transfer don't count.
        typed(&mut book, checking, false, "Standing order", "2026-10-01");
        let amount = Money::from_minor_units(10_000, crate::alarms::tests::EUR);
        book.add_recurring_move(&NewRecurringMove {
            from_account: checking,
            to_account: savings,
            amount,
            day_of_month: 1,
            from_party_iban: None,
            from_party_name: None,
            created_on: Date::constant(2026, 10, 2),
        })
        .expect("remembered");
        let moves = list(&book, &TestLocale).expect("list");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].last_joined, "");
    }

    #[test]
    fn the_usual_day_is_the_most_frequent_and_the_first_seen_on_a_tie() {
        assert_eq!(usual_day(&[3, 1, 1]), Some(1));
        assert_eq!(usual_day(&[3, 1]), Some(3));
        assert_eq!(usual_day(&[15]), Some(15));
        assert_eq!(usual_day(&[]), None);
    }

    #[test]
    fn join_all_joins_what_the_user_saw_and_offers_what_looks_monthly_once() {
        let (mut book, _, _, _) = standing_orders(&[9, 10, 11]);
        // What the user saw was a different number: nothing is joined.
        assert!(matches!(
            join_all(&mut book, 2, &TestLocale),
            Err(MonthlyMovesError::Stale)
        ));
        assert_eq!(
            transfer_prompt::suggestions(&book, "", &TestLocale)
                .expect("suggested")
                .len(),
            3
        );
        let (done, candidates) = join_all(&mut book, 3, &TestLocale).expect("joined");
        assert_eq!((done.joined, done.left), (3, 0));
        assert_eq!(done.looks_monthly.len(), 1);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            (
                done.looks_monthly[0].from.as_str(),
                done.looks_monthly[0].to.as_str(),
                done.looks_monthly[0].months,
            ),
            ("Checking account", "Savings", 3)
        );
        assert!(list(&book, &TestLocale).expect("list").is_empty());
        // One Undo takes all three joins back.
        book.undo().expect("undone");
        assert_eq!(
            transfer_prompt::suggestions(&book, "", &TestLocale)
                .expect("suggested")
                .len(),
            3
        );
    }

    #[test]
    fn one_month_does_not_look_monthly_and_a_ticked_move_is_remembered_in_one_action() {
        let (mut book, _, _, _) = standing_orders(&[10]);
        let (done, candidates) = join_all(&mut book, 1, &TestLocale).expect("joined");
        assert_eq!(done.joined, 1);
        assert!(done.looks_monthly.is_empty() && candidates.is_empty());

        let (mut book, _, _, _) = standing_orders(&[9, 10]);
        let (done, candidates) = join_all(&mut book, 2, &TestLocale).expect("joined");
        assert_eq!(done.looks_monthly.len(), 1);
        // An index that is none of the candidates is refused, nothing kept.
        assert!(matches!(
            remember_looks_monthly(&mut book, &candidates, &[0, 5]),
            Err(MonthlyMovesError::Stale)
        ));
        assert!(list(&book, &TestLocale).expect("list").is_empty());
        remember_looks_monthly(&mut book, &candidates, &[]).expect("none ticked");
        assert!(list(&book, &TestLocale).expect("list").is_empty());
        remember_looks_monthly(&mut book, &candidates, &[0]).expect("ticked");
        assert_eq!(list(&book, &TestLocale).expect("list").len(), 1);
        book.undo().expect("undone");
        assert!(list(&book, &TestLocale).expect("list").is_empty());
        // Ticked twice, it is remembered once.
        remember_looks_monthly(&mut book, &candidates, &[0, 0]).expect("ticked");
        assert_eq!(list(&book, &TestLocale).expect("list").len(), 1);
    }
}
