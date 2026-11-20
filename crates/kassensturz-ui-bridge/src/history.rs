// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Undo, redo and the change history (M3.8, D-032, D-066, D-086, D-121).
//!
//! The store records every user action and can take the newest one back
//! (`Book::undo`) or make it again (`Book::redo`); this module writes what
//! the screens say about them as text (D-065): which change undo would take
//! back, which redo would make again, the list of changes, how long the book
//! keeps them, and how many a shorter setting would delete. What an action
//! is called is the UI's to say (D-034), so an action goes to QML as a key
//! (`add-transaction`) that `ChangeNames.qml` puts into words.

use kassensturz_core::{AccountId, AccountKind, Money, Report, Timestamp, TransactionId};
use kassensturz_store::{
    Action, Book, Change, ChangeId, ChangeSummary, HistoryRetention, StoreError,
};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::register::AmountWriter;

/// How many changes the list shows, newest first. The store keeps the
/// others as long as the book's setting says.
pub const SHOWN: usize = 100;

/// The settings key for a book that keeps its history for as long as it
/// exists; any other key is a number of days.
const FOREVER: &str = "forever";

/// Why undo, redo or a history setting didn't work.
#[derive(Debug)]
pub enum HistoryError {
    /// The setting isn't one the UI offers: not `forever` and not a number
    /// of days.
    NoSuchChoice,
    /// The book refused it, e.g. because it no longer holds what the
    /// history says (`store.corrupt.history-mismatch`).
    Store(StoreError),
}

impl From<StoreError> for HistoryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One change of the history, as the screens show it.
#[derive(Debug, PartialEq, Eq)]
pub struct ChangeText {
    /// The change's ID in the history, which the receipt inbox's journal
    /// names it by (US7, #333).
    pub id: String,
    /// What the user did, as a key `ChangeNames.qml` knows ([`action_key`]).
    pub action: &'static str,
    /// When, in the user's time zone and format.
    pub made_at: String,
    /// When, as seconds since 1970 (UTC), so the history page can group
    /// changes by day and by sitting (B-395, D-870).
    pub made_at_seconds: i64,
    /// Whether it is undone, so that redo can make it again.
    pub undone: bool,
    /// The name of the file it read, such as an imported statement; empty
    /// for a change that read none (D-109).
    pub file_name: String,
    /// Which transactions it touched. Only the changes undo and redo are
    /// about have one, since the store can tell that only for them (#51).
    pub summary: Option<SummaryText>,
    /// Whether it touched a saved dashboard, so Undo attributes it to the
    /// Overview and not to the Statistic Builder, whose saved views share
    /// its action (B-419). Read only for the changes undo and redo are
    /// about, like `summary`.
    pub dashboard: bool,
}

/// Which transactions a change touched, so the user can tell two imports or
/// two edits apart (#51).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SummaryText {
    /// How many transactions it added.
    pub added: usize,
    /// How many it removed.
    pub removed: usize,
    /// How many it changed.
    pub changed: usize,
    /// The names of the accounts they are in, sorted by name. Categories and
    /// the book's internal account are left out.
    pub accounts: Vec<String>,
    /// The earliest of their dates, in the user's format; empty if none.
    pub first_date: String,
    /// The latest of their dates, in the user's format; empty if none.
    pub last_date: String,
    /// The IDs of the transactions it touched, so the screens can light
    /// their rows up after the change, its undo or its redo (B-384, D-857).
    pub transactions: Vec<TransactionId>,
    /// The one transaction it touched, in words, so "Undone: …" can name
    /// it; `None` when it touched several, or the transaction is in the
    /// book neither before nor after the step.
    pub booking: Option<BookingText>,
}

/// One transaction as the undo and redo notices name it (B-384, D-857):
/// whom it was with, or else its category or the other account, and its
/// amount in the user's account, written by Rust (D-065).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BookingText {
    /// The payee, the category or the other account's name; empty when
    /// it has none of these.
    pub name: String,
    /// Its amount as seen from the user's account, e.g. "-23,80 €".
    pub amount: String,
}

/// What the history screen and the undo and redo actions show.
#[derive(Debug, PartialEq, Eq)]
pub struct View {
    /// The change undo would take back.
    pub undo: Option<ChangeText>,
    /// The change redo would make again.
    pub redo: Option<ChangeText>,
    /// The newest [`SHOWN`] changes, newest first, the undone ones with
    /// `undone` set.
    pub changes: Vec<ChangeText>,
    /// How many changes the history holds in all.
    pub total: usize,
    /// How long the book keeps them, as a key: `forever` or a number of
    /// days ([`set_retention`]).
    pub retention: String,
}

/// The key `ChangeNames.qml` puts into words for `action`. Adding an action
/// to the store fails to compile here until it has one.
pub fn action_key(action: Action) -> &'static str {
    match action {
        Action::SetRoundingMode => "set-rounding-mode",
        Action::UseCurrency => "use-currency",
        Action::AddAccount => "add-account",
        Action::RenameAccount => "rename-account",
        Action::CloseAccount => "close-account",
        Action::ReopenAccount => "reopen-account",
        Action::AddCategory => "add-category",
        Action::RenameCategory => "rename-category",
        Action::MoveCategory => "move-category",
        Action::AddRate => "add-rate",
        Action::RemoveRate => "remove-rate",
        Action::AddTransaction => "add-transaction",
        Action::EditTransaction => "edit-transaction",
        Action::RemoveTransaction => "remove-transaction",
        Action::JoinTransfer => "join-transfer",
        Action::AddRule => "add-rule",
        Action::EditRule => "edit-rule",
        Action::MoveRule => "move-rule",
        Action::RemoveRule => "remove-rule",
        Action::RemoveCategory => "remove-category",
        Action::AddView => "add-view",
        Action::EditView => "edit-view",
        Action::RenameView => "rename-view",
        Action::MoveView => "move-view",
        Action::RemoveView => "remove-view",
        Action::AddGroup => "add-group",
        Action::EditGroup => "edit-group",
        Action::MoveGroup => "move-group",
        Action::RemoveGroup => "remove-group",
        Action::SetHiddenCategories => "set-hidden-categories",
        Action::AddBudget => "add-budget",
        Action::EditBudget => "edit-budget",
        Action::RemoveBudget => "remove-budget",
        Action::ChangeAccountType => "change-account-type",
        Action::RenameTag => "rename-tag",
        Action::RemoveTag => "remove-tag",
        Action::MergeTags => "merge-tags",
        Action::AddRecurringMove => "add-recurring-move",
        Action::RemoveRecurringMove => "remove-recurring-move",
        Action::JoinRecurringMoves => "join-recurring-moves",
        Action::JoinTransfers => "join-transfers",
        Action::AddSchedule => "add-schedule",
        Action::EditSchedule => "edit-schedule",
        Action::RemoveSchedule => "remove-schedule",
        Action::PauseSchedule => "pause-schedule",
        Action::MatchSchedules => "match-schedules",
        Action::MatchScheduleOccurrence => "match-schedule-occurrence",
        Action::SkipScheduleOccurrence => "skip-schedule-occurrence",
        Action::UnskipScheduleOccurrence => "unskip-schedule-occurrence",
        Action::BookScheduleOccurrence => "book-schedule-occurrence",
        Action::RunRules => "run-rules",
        Action::ImportStatement => "import-statement",
        Action::CorrectStartingBalance => "correct-starting-balance",
        Action::AddInstitution => "add-institution",
        Action::AddPayee => "add-payee",
        Action::AddTag => "add-tag",
        Action::LinkBankAccount => "link-bank-account",
        Action::UnlinkBankAccount => "unlink-bank-account",
        Action::FetchStatement => "fetch-statement",
        Action::ApplyAiChanges => "apply-ai-changes",
    }
}

/// Everything the screens show about the history. `when` writes a time in
/// the user's time zone and format.
pub fn view(
    book: &Book,
    when: &impl Fn(Timestamp) -> String,
    locale: &impl Locale,
) -> Result<View, HistoryError> {
    let describe = |change: Option<Change>| {
        change
            .map(|change| change_text(book, &change, true, when, locale))
            .transpose()
    };
    let all = book.history()?;
    Ok(View {
        undo: describe(book.change_to_undo()?)?,
        redo: describe(book.change_to_redo()?)?,
        total: all.len(),
        changes: all
            .iter()
            .take(SHOWN)
            .map(|change| change_text(book, change, false, when, locale))
            .collect::<Result<_, _>>()?,
        retention: retention_key(book.history_retention()?),
    })
}

/// Takes the newest change back and returns it, as it was described before
/// (`None` if there is nothing to undo).
pub fn undo(
    book: &mut Book,
    when: &impl Fn(Timestamp) -> String,
    locale: &impl Locale,
) -> Result<Option<ChangeText>, HistoryError> {
    let Some(next) = book.change_to_undo()? else {
        return Ok(None);
    };
    // Described first: the summary is read from the book as it is now.
    let mut text = change_text(book, &next, true, when, locale)?;
    book.undo()?;
    name_booking_after(book, &mut text, locale)?;
    Ok(Some(ChangeText {
        undone: true,
        ..text
    }))
}

/// Makes the change that was undone last again and returns it.
pub fn redo(
    book: &mut Book,
    when: &impl Fn(Timestamp) -> String,
    locale: &impl Locale,
) -> Result<Option<ChangeText>, HistoryError> {
    let Some(next) = book.change_to_redo()? else {
        return Ok(None);
    };
    let mut text = change_text(book, &next, true, when, locale)?;
    book.redo()?;
    name_booking_after(book, &mut text, locale)?;
    Ok(Some(ChangeText {
        undone: false,
        ..text
    }))
}

/// Sets how long the book keeps its history: `forever` or a number of days
/// as digits (D-066). Older changes are deleted right away.
pub fn set_retention(book: &mut Book, key: &str) -> Result<(), HistoryError> {
    let keep = read_retention(key)?;
    book.set_history_retention(keep)?;
    Ok(())
}

/// How many changes `key` as the setting would delete from the book now, so
/// the user can be asked first. With 0 days nothing is deleted yet: the
/// history moves into memory and is lost when the book closes.
pub fn expiring(book: &Book, key: &str, now: Timestamp) -> Result<usize, HistoryError> {
    let keep = read_retention(key)?;
    Ok(book.history_expiring(keep, now)?)
}

/// Deletes the whole history for good (D-066, D-086). Backups keep what
/// they held.
pub fn clean_up(book: &mut Book) -> Result<(), HistoryError> {
    book.clean_up_history()?;
    Ok(())
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &HistoryError, locale: &impl Locale) -> ErrorText {
    match error {
        HistoryError::NoSuchChoice => ErrorText::plain("input.no-such-entry"),
        HistoryError::Store(error) => report_text(book, &error.report(), locale),
    }
}

fn read_retention(key: &str) -> Result<HistoryRetention, HistoryError> {
    if key == FOREVER {
        return Ok(HistoryRetention::Forever);
    }
    // Digits only: `parse` would also take a sign.
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(HistoryError::NoSuchChoice);
    }
    key.parse()
        .map(HistoryRetention::Days)
        .map_err(|_| HistoryError::NoSuchChoice)
}

fn retention_key(keep: HistoryRetention) -> String {
    match keep {
        HistoryRetention::Forever => FOREVER.to_owned(),
        HistoryRetention::Days(days) => days.to_string(),
    }
}

fn change_text(
    book: &Book,
    change: &Change,
    with_summary: bool,
    when: &impl Fn(Timestamp) -> String,
    locale: &impl Locale,
) -> Result<ChangeText, HistoryError> {
    let summary = if with_summary {
        let summary = book.change_summary(change.id)?;
        summary_text(book, change.id, &summary, locale)?
    } else {
        None
    };
    let dashboard = with_summary && book.change_touched_dashboard(change.id)?;
    Ok(ChangeText {
        id: change.id.to_string(),
        action: action_key(change.action),
        made_at: when(change.made_at),
        made_at_seconds: change.made_at.as_second(),
        undone: change.undone,
        file_name: change.file_name.clone().unwrap_or_default(),
        summary,
        dashboard,
    })
}

/// A transaction that wasn't in the book before the step (an added one
/// undone is gone, a removed one undone is back) is named from the book
/// after it.
fn name_booking_after(
    book: &Book,
    text: &mut ChangeText,
    locale: &impl Locale,
) -> Result<(), HistoryError> {
    if let Some(summary) = text.summary.as_mut()
        && summary.booking.is_none()
        && let [only] = summary.transactions.as_slice()
    {
        summary.booking = booking_text(book, *only, locale)?;
    }
    Ok(())
}

/// `transaction` in words ([`BookingText`]), or `None` if the book doesn't
/// hold it (now).
fn booking_text(
    book: &Book,
    transaction: TransactionId,
    locale: &impl Locale,
) -> Result<Option<BookingText>, HistoryError> {
    let transaction = match book.transaction(transaction) {
        Ok(transaction) => transaction,
        Err(StoreError::UnknownTransaction(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let parts: Vec<_> = transaction
        .splits()
        .iter()
        .filter_map(|split| split.account.map(|account| (account, split.amount)))
        .collect();
    booking_words(book, transaction.payee(), &parts, locale)
}

/// A booking named by its payee (or its category's name) and the amount in
/// its own account, from its `parts`.
fn booking_words(
    book: &Book,
    payee: Option<&str>,
    parts: &[(AccountId, Money)],
    locale: &impl Locale,
) -> Result<Option<BookingText>, HistoryError> {
    let mut own = None;
    let mut other = None;
    for &(account, amount) in parts {
        let account = book.account(account)?;
        match account.kind {
            AccountKind::Asset | AccountKind::Liability if own.is_none() => {
                own = Some(amount);
            }
            AccountKind::Equity => {}
            _ if other.is_none() => other = Some(account.name),
            _ => {}
        }
    }
    let Some(amount) = own else {
        return Ok(None);
    };
    let name = payee
        .map(kassensturz_store::shown_text)
        .filter(|payee| !payee.trim().is_empty())
        .or(other)
        .unwrap_or_default();
    let writer = AmountWriter::new(book, amount.currency(), locale)?;
    Ok(Some(BookingText {
        name,
        amount: writer.text(amount).text,
    }))
}

/// `None` for a change that touched no transaction.
fn summary_text(
    book: &Book,
    change: ChangeId,
    summary: &ChangeSummary,
    locale: &impl Locale,
) -> Result<Option<SummaryText>, HistoryError> {
    if summary.added + summary.removed + summary.changed == 0 {
        return Ok(None);
    }
    let accounts = book.accounts(true)?;
    let mut names: Vec<String> = accounts
        .iter()
        .filter(|account| summary.accounts.contains(&account.id))
        .map(|account| account.name.clone())
        .collect();
    names.sort_by_cached_key(|name| name.to_lowercase());
    let (first_date, last_date) = summary.dates.map_or_else(
        || (String::new(), String::new()),
        |(first, last)| (locale.short_date(first), locale.short_date(last)),
    );
    let booking = match summary.transactions.as_slice() {
        [only] => match booking_text(book, *only, locale)? {
            Some(booking) => Some(booking),
            // A removed booking is gone from the book: name it from the
            // history's copy of it, so the label before Undo names it too
            // (B-399).
            None if summary.removed == 1 => match book.booking_before(change, *only)? {
                Some(before) => {
                    booking_words(book, before.payee.as_deref(), &before.parts, locale)?
                }
                None => None,
            },
            None => None,
        },
        _ => None,
    };
    Ok(Some(SummaryText {
        added: summary.added,
        removed: summary.removed,
        changed: summary.changed,
        accounts: names,
        first_date,
        last_date,
        transactions: summary.transactions.clone(),
        booking,
    }))
}

#[cfg(test)]
mod tests {
    use kassensturz_core::AccountType;
    use kassensturz_store::Corruption;

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::transaction_entry::{self, NewEntry};
    use crate::transfer_prompt;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).expect("a valid time")
    }

    fn when(time: Timestamp) -> String {
        time.as_second().to_string()
    }

    /// A started book with one account holding 1000 € on 2026-10-01, made
    /// by M3.3's `add_account`; returns its ID too.
    fn book_with_account(name: &str) -> (Book, String) {
        let mut book = started();
        let id = add_account(
            &mut book,
            &NewAccount {
                name,
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "1000",
                date: "2026-10-01",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string();
        (book, id)
    }

    fn add_entry(
        book: &mut Book,
        account: &str,
        money_in: bool,
        amount: &str,
        date: &str,
    ) -> String {
        transaction_entry::add(
            book,
            &NewEntry {
                account,
                money_in,
                amount,
                other_party: "Corner Grocery",
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

    fn view_of(book: &Book) -> View {
        view(book, &when, &TestLocale).expect("view")
    }

    #[test]
    fn every_action_has_its_own_key_and_a_name_in_the_qml() {
        let names = include_str!("../qml/ChangeNames.qml");
        let mut keys: Vec<&str> = Action::ALL
            .iter()
            .map(|action| action_key(*action))
            .collect();
        for key in &keys {
            assert!(
                names.contains(&format!("\"{key}\"")),
                "ChangeNames.qml has no name for {key}"
            );
        }
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Action::ALL.len());
    }

    #[test]
    fn a_book_without_changes_has_nothing_to_undo() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let shown = view_of(&book);
        assert_eq!((shown.undo, shown.redo, shown.total), (None, None, 0));
        assert_eq!(undo(&mut book, &when, &TestLocale).expect("undo"), None);
        assert_eq!(redo(&mut book, &when, &TestLocale).expect("redo"), None);
    }

    #[test]
    fn undo_names_the_change_and_what_it_touched_and_redo_makes_it_again() {
        let (mut book, account) = book_with_account("Checking account");
        let added = add_entry(&mut book, &account, false, "53,20", "2026-10-05");

        let shown = view_of(&book);
        let next = shown.undo.expect("something to undo");
        assert_eq!(next.action, "add-transaction");
        let summary = next.summary.expect("summary");
        // The transaction, named for "Undone: …" (B-384, D-857).
        let ids: Vec<String> = summary
            .transactions
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(ids, vec![added.clone()]);
        assert_eq!(
            summary,
            SummaryText {
                added: 1,
                accounts: vec!["Checking account".into()],
                first_date: "2026-10-05".into(),
                last_date: "2026-10-05".into(),
                transactions: summary.transactions.clone(),
                booking: Some(BookingText {
                    name: "Corner Grocery".into(),
                    amount: "-53,20 €".into(),
                }),
                ..SummaryText::default()
            }
        );
        assert!(shown.redo.is_none());

        let taken_back = undo(&mut book, &when, &TestLocale)
            .expect("undo")
            .expect("a change");
        assert_eq!(taken_back.action, "add-transaction");
        assert!(taken_back.undone);
        // Named from the book before the step: it is gone after.
        assert_eq!(
            taken_back
                .summary
                .as_ref()
                .and_then(|summary| summary.booking.as_ref())
                .map(|booking| booking.name.as_str()),
            Some("Corner Grocery")
        );
        let shown = view_of(&book);
        assert_eq!(shown.undo.expect("older change").action, "add-account");
        let redo_next = shown.redo.expect("something to redo");
        assert_eq!(redo_next.action, "add-transaction");
        assert_eq!(redo_next.summary.expect("summary").added, 1);

        let made_again = redo(&mut book, &when, &TestLocale)
            .expect("redo")
            .expect("a change");
        assert!(!made_again.undone);
        // Named from the book after the step: it wasn't there before.
        let booking = made_again
            .summary
            .and_then(|summary| summary.booking)
            .expect("the booking named");
        assert_eq!(booking.name, "Corner Grocery");
        assert_eq!(booking.amount, "-53,20 €");
        assert_eq!(
            view_of(&book).undo.expect("undo again").action,
            "add-transaction"
        );
    }

    /// A change of a dashboard and a change of a saved view share their
    /// actions; only the recorded rows tell them apart (B-419). Made with
    /// the real saves, read back through the history that Undo uses.
    #[test]
    fn a_change_says_whether_it_touched_a_dashboard() {
        let (mut book, _) = book_with_account("Checking account");
        let view =
            r#"{"format":"kassensturz-dashboard","version":1,"kind":"view","name":"Saved view"}"#;
        book.add_view(view).expect("view saved");
        let seen = view_of(&book).undo.expect("undo");
        assert_eq!(seen.action, "add-view");
        assert!(!seen.dashboard, "a saved view is not a dashboard");

        crate::dashboards::add(&mut book, "empty", "Holidays", "", "Overview")
            .expect("dashboard added");
        let seen = view_of(&book).undo.expect("undo");
        assert_eq!(seen.action, "add-view");
        assert!(seen.dashboard, "a dashboard is one");

        // Undone, it is described first and says the same.
        let taken_back = undo(&mut book, &when, &TestLocale)
            .expect("undo")
            .expect("a change");
        assert!(taken_back.dashboard);
        // A change of a transaction is neither.
        let (mut book, account) = book_with_account("Checking account");
        add_entry(&mut book, &account, false, "5", "2026-10-02");
        assert!(!view_of(&book).undo.expect("undo").dashboard);
    }

    #[test]
    fn two_edits_of_different_days_can_be_told_apart() {
        let (mut book, account) = book_with_account("Checking account");
        let first = add_entry(&mut book, &account, false, "10", "2026-10-02");
        let second = add_entry(&mut book, &account, false, "20", "2026-10-09");
        transaction_entry::remove(&mut book, &account, &second).expect("removed");

        let removal = view_of(&book).undo.expect("undo").summary.expect("summary");
        assert_eq!(
            (removal.removed, removal.first_date.as_str()),
            (1, "2026-10-09")
        );
        // The label before Undo names the removed booking, though it is gone
        // from the book (B-399).
        let removed_booking = removal.booking.as_ref().expect("the removal is named");
        assert_eq!(
            (
                removed_booking.name.as_str(),
                removed_booking.amount.as_str()
            ),
            ("Corner Grocery", "-20,00 €")
        );
        let first_text = undo(&mut book, &when, &TestLocale)
            .expect("undo")
            .expect("a change");
        assert_eq!(
            first_text.summary.and_then(|summary| summary.booking),
            Some(BookingText {
                name: "Corner Grocery".into(),
                amount: "-20,00 €".into()
            })
        );
        let addition = view_of(&book).undo.expect("undo").summary.expect("summary");
        assert_eq!(
            (addition.added, addition.first_date.as_str()),
            (1, "2026-10-09")
        );
        // The first one is still there to be told apart.
        transaction_entry::remove(&mut book, &account, &first).expect("removed");
        let first_removal = view_of(&book).undo.expect("undo").summary.expect("summary");
        assert_eq!(first_removal.first_date, "2026-10-02");
    }

    #[test]
    fn a_joined_transfer_is_named_and_undo_takes_it_apart() {
        let (mut book, checking) = book_with_account("Checking account");
        let savings = add_account(
            &mut book,
            &NewAccount {
                name: "Savings",
                account_type: AccountType::Savings,
                currency: "EUR",
                balance: "500",
                date: "2026-10-01",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string();
        let out = add_entry(&mut book, &checking, false, "100", "2026-10-05");
        let into = add_entry(&mut book, &savings, true, "100", "2026-10-06");
        transfer_prompt::join(&mut book, &out, &into).expect("joined");

        let next = view_of(&book).undo.expect("undo");
        assert_eq!(next.action, "join-transfer");
        let summary = next.summary.expect("summary");
        assert_eq!(summary.accounts, ["Checking account", "Savings"]);

        undo(&mut book, &when, &TestLocale).expect("undo");
        // The two sides are suggested again, since nothing was joined.
        assert_eq!(
            transfer_prompt::suggestions(&book, "", &TestLocale)
                .expect("suggestions")
                .len(),
            1
        );
    }

    #[test]
    fn undoing_everything_leads_back_to_the_first_start() {
        let (mut book, account) = book_with_account("Checking account");
        add_entry(&mut book, &account, true, "5", "2026-10-05");
        while undo(&mut book, &when, &TestLocale).expect("undo").is_some() {}
        let regional = kassensturz_core::CurrencyCode::new("EUR").expect("valid code");
        let state = crate::first_start::state(&book, regional).expect("state");
        assert!(!state.has_account);
        // The first start works again on the emptied book.
        crate::first_start::start(&mut book, "EUR", &crate::first_start::tests::defaults())
            .expect("started again");
        assert!(view_of(&book).undo.is_some());
        // And everything can be made again, in the order it was done.
        let mut made = 0;
        while redo(&mut book, &when, &TestLocale).expect("redo").is_some() {
            made += 1;
        }
        // A new change ended what could be redone (D-066).
        assert_eq!(made, 0);
    }

    #[test]
    fn the_list_shows_the_newest_changes_first_with_the_undone_ones_marked() {
        let (mut book, account) = book_with_account("Checking account");
        for day in 1..=9 {
            add_entry(&mut book, &account, false, "1", &format!("2026-10-0{day}"));
        }
        undo(&mut book, &when, &TestLocale).expect("undo");
        let shown = view_of(&book);
        assert_eq!(shown.total, shown.changes.len());
        assert!(shown.changes[0].undone);
        assert!(!shown.changes[1].undone);
        assert_eq!(shown.changes[0].action, "add-transaction");
        assert_eq!(shown.changes.last().expect("oldest").action, "add-category");
        // Only the changes undo and redo are about carry a summary.
        assert!(shown.changes.iter().all(|change| change.summary.is_none()));
    }

    #[test]
    fn the_list_is_cut_after_the_newest_hundred() {
        let (mut book, account) = book_with_account("Checking account");
        for day in 0..(SHOWN + 5) {
            add_entry(
                &mut book,
                &account,
                false,
                "1",
                &format!("2026-09-{:02}", 1 + day % 28),
            );
        }
        let shown = view_of(&book);
        assert_eq!(shown.changes.len(), SHOWN);
        assert!(shown.total > SHOWN);
    }

    #[test]
    fn the_retention_is_30_days_and_can_be_changed_and_read_back() {
        let (mut book, _) = book_with_account("Checking account");
        assert_eq!(view_of(&book).retention, "30");
        for key in ["forever", "7", "365", "0", "30"] {
            set_retention(&mut book, key).expect("set");
            assert_eq!(view_of(&book).retention, key);
        }
    }

    #[test]
    fn a_retention_the_screen_doesnt_offer_is_refused() {
        let mut book = Book::create_in_memory().expect("book in memory");
        for key in ["", "-1", "+5", "1e3", " 7", "weeks", "99999999999"] {
            let error = set_retention(&mut book, key).expect_err("refused");
            assert_eq!(
                error_text(&book, &error, &TestLocale).code,
                "input.no-such-entry",
                "{key:?}"
            );
        }
        assert_eq!(view_of(&book).retention, "30");
    }

    #[test]
    fn keeping_no_history_still_allows_undo_until_the_book_closes() {
        let (mut book, account) = book_with_account("Checking account");
        set_retention(&mut book, "0").expect("set");
        add_entry(&mut book, &account, false, "5", "2026-10-05");
        assert_eq!(
            undo(&mut book, &when, &TestLocale)
                .expect("undo")
                .expect("change")
                .action,
            "add-transaction"
        );
    }

    /// The count the user is asked with is the store's own rule (B-274):
    /// a clock a month ahead alone makes no change old enough to go, since
    /// the book wasn't changed on other days after them.
    #[test]
    fn expiring_counts_what_the_book_would_really_delete() {
        let (mut book, _) = book_with_account("Checking account");
        let total = view_of(&book).total;
        assert!(total > 0);
        let now = Timestamp::now();
        assert_eq!(expiring(&book, "30", now).expect("count"), 0);
        let later = at(now.as_second() + 31 * 86_400);
        assert_eq!(expiring(&book, "30", later).expect("count"), 0);
        assert_eq!(
            book.history_expiring(HistoryRetention::Days(30), later)
                .expect("count"),
            0
        );
        assert!(expiring(&book, "weeks", now).is_err());
        // Nothing is too old yet, so a shorter setting deletes nothing now.
        set_retention(&mut book, "7").expect("set");
        assert_eq!(view_of(&book).total, total);
    }

    #[test]
    fn cleaning_up_deletes_the_history_for_good() {
        let (mut book, account) = book_with_account("Checking account");
        add_entry(&mut book, &account, false, "5", "2026-10-05");
        undo(&mut book, &when, &TestLocale).expect("undo");
        assert!(view_of(&book).redo.is_some());
        clean_up(&mut book).expect("cleaned up");
        let shown = view_of(&book);
        assert_eq!((shown.undo, shown.redo, shown.total), (None, None, 0));
        // The book keeps what it held.
        assert_eq!(book.accounts(true).expect("accounts").len(), 1);
        // Changes after the clean-up are recorded again.
        add_entry(&mut book, &account, false, "5", "2026-10-06");
        assert_eq!(view_of(&book).total, 1);
    }

    #[test]
    fn a_book_that_doesnt_fit_its_history_is_reported_with_its_own_code() {
        // How the store reaches this state is tested in `kassensturz-store`;
        // what matters here is that the screen gets a code it can explain.
        let book = Book::create_in_memory().expect("book in memory");
        let error = HistoryError::Store(StoreError::Corrupt(Corruption::HistoryMismatch));
        assert_eq!(
            error_text(&book, &error, &TestLocale).code,
            "store.corrupt.history-mismatch"
        );
    }
}
