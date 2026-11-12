// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adding, changing and removing a transaction from the register (M3.5,
//! D-118): money paid or received, how much, from or to whom (D-034), what
//! for and when.
//!
//! What the user types comes in as text and is read here in their regional
//! format (`parse.rs`, D-065); what the page shows goes out as text. The
//! page changes only a simple transaction: one of the user's accounts and
//! one category, or none yet, in the account's currency. A starting balance
//! is corrected through "Check against my bank" (D-116), a Transfer and a
//! transaction with several Parts get their own pages (M3.6, M3.7); here
//! they can only be removed.

use kassensturz_core::{
    Account, AccountId, AccountKind, Money, Report, Split, Transaction, TransactionError,
    TransactionId,
};
use kassensturz_store::{Action, Book, HistoryRetention, StoreError};

use crate::errors::{ErrorText, amount_example, report_text};
use crate::format::{Affixes, AmountFormat, Locale};
use crate::parse::{AmountInputError, AmountReader};
use crate::register::{EntryKind, kind_of};
use crate::tags::{self, TagInput, TagText};

/// Why something the user entered wasn't accepted. The UI shows a message
/// for each code (D-034).
#[derive(Debug)]
pub enum EntryError {
    /// What the user typed as an amount isn't one.
    Amount(AmountInputError),
    /// An amount has to be more than zero: whether money came in or went
    /// out is chosen separately.
    NotPositive,
    /// What the user typed as a date isn't one.
    Date,
    /// There is no such account, category or transaction, e.g. it was just
    /// removed elsewhere.
    NoSuchEntry,
    /// The transaction isn't a simple one, so this page can't change it.
    NotEditableHere,
    /// The book refused it, e.g. the account is closed.
    Store(StoreError),
    /// The transaction doesn't add up; only an amount too large can get here.
    Transaction(TransactionError),
    /// A note longer than `MAX_NOTE_CHARS` (or than the note it replaces),
    /// or one with a control character other than a line break or a tab.
    /// `max` is the longest it may be.
    Note { max: usize },
}

impl From<StoreError> for EntryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// The longest note the page takes (the maintainer's choice, D-703); the AI
/// keeps its own limit of 200. A note that is already longer, e.g. a memo
/// from a KMyMoney book, may stay as long when the user changes it.
pub const MAX_NOTE_CHARS: usize = 512;

/// A transaction as the user entered it on the page.
pub struct NewEntry<'a> {
    /// The ID of the account it belongs to, as text.
    pub account: &'a str,
    /// Whether money came in, rather than went out.
    pub money_in: bool,
    /// The amount, as typed, without a sign.
    pub amount: &'a str,
    /// Who the money came from or went to, as typed; may be empty.
    pub other_party: &'a str,
    /// The ID of its category as text; empty for none yet.
    pub category: &'a str,
    /// The date, as typed.
    pub date: &'a str,
    /// What the user wrote about it, as typed. `Some` sets the note (empty
    /// removes it); `None` leaves a transaction's note as it is, and an
    /// added one has none.
    pub note: Option<&'a str>,
    /// The tags on the whole transaction, as chosen (D-712, Q-493). `Some`
    /// sets them (empty removes them); `None` leaves a transaction's tags as
    /// they are, and an added one has none.
    pub tags: Option<&'a [TagInput<'a>]>,
}

/// A transaction as the page shows it to be changed, every amount and date
/// as text (D-065).
#[derive(Debug, PartialEq, Eq)]
pub struct EntryForm {
    /// The transaction's ID as text.
    pub id: String,
    /// What kind of entry it is, as a key the UI translates (`EntryKind`).
    /// Only `simple` can be changed on the page.
    pub kind: &'static str,
    /// The ID of the account it is seen from, as text.
    pub account: String,
    /// Whether money came in.
    pub money_in: bool,
    /// The amount without sign, written the way the user types it, e.g.
    /// "1.234,56" in German, with every decimal place.
    pub amount: String,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// The ID of its category as text; empty for none, or for a kind that
    /// has no single category.
    pub category: String,
    /// For a Transfer, the other account's name.
    pub other_account: String,
    /// The booking date, in the user's format.
    pub date: String,
    /// What the user wrote about the transaction as a whole; may be empty.
    pub note: String,
    /// The tags on the whole transaction, in the user's order (D-712).
    pub tags: Vec<TagText>,
    /// The tags its parts carry of their own that the whole hasn't, each
    /// once: shown read-only, as they are changed on the Parts page.
    pub part_tags: Vec<TagText>,
    /// Whether the account's part came from a bank or a file, so its amount
    /// stays as delivered (D-067).
    pub imported: bool,
    /// The other party's name as the bank or file delivered it, when the
    /// page shows another one (the user or a Rule changed it); empty
    /// otherwise. The original stays next to what the user sees (D-375).
    pub delivered_name: String,
}

/// What removing a transaction takes with it, for the plain warning before
/// a removal that takes what a bank or file delivered (D-375).
#[derive(Debug, PartialEq, Eq)]
pub struct RemovalNotice {
    /// Whether a bank or file delivered one of its parts: removing it
    /// removes what was delivered too, and only Undo brings that back.
    pub holds_original: bool,
    /// How long Undo can still bring it back, as a key the UI translates:
    /// `days` (see `undo_days`), `forever`, or `until-closed` when the
    /// history is kept only while the book is open (0 days, D-066).
    pub undo: &'static str,
    /// For `days`, how many.
    pub undo_days: u32,
}

/// A category the page offers.
#[derive(Debug, PartialEq, Eq)]
pub struct CategoryChoice {
    /// Its ID as text.
    pub id: String,
    /// Its path, e.g. "Food › Groceries".
    pub path: String,
    /// How deep it sits, for indenting the list.
    pub depth: usize,
}

/// An account the page offers when it isn't opened from a register.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountChoice {
    /// Its ID as text.
    pub id: String,
    /// Its name.
    pub name: String,
}

/// The open accounts the user can add a transaction to, in the order they
/// were added (D-067).
pub fn account_choices(book: &Book) -> Result<Vec<AccountChoice>, StoreError> {
    let mut accounts: Vec<Account> = book
        .all_accounts()?
        .into_iter()
        .filter(|account| account.kind.is_account() && !account.closed)
        .collect();
    accounts.sort_by_key(|account| account.id);
    Ok(accounts
        .into_iter()
        .map(|account| AccountChoice {
            id: account.id.to_string(),
            name: account.name,
        })
        .collect())
}

/// The categories a transaction in `account` can be booked to, in tree
/// order: those for money in or for money out, in the account's currency,
/// and `current` (an ID as text) even if it is of the other kind, so a
/// transaction keeps the category it has.
pub fn category_choices(
    book: &Book,
    account: &str,
    money_in: bool,
    current: &str,
) -> Result<Vec<CategoryChoice>, EntryError> {
    let account = find_account(book, account)?;
    let kind = if money_in {
        AccountKind::Income
    } else {
        AccountKind::Expense
    };
    Ok(book
        .categories()?
        .into_iter()
        .filter(|entry| {
            let category = &entry.category;
            category.currency == account.currency
                && !category.closed
                && (category.kind == kind || category.id.to_string() == current)
        })
        .map(|entry| CategoryChoice {
            id: entry.category.id.to_string(),
            path: entry.path.join(" › "),
            depth: entry.path.len() - 1,
        })
        .collect())
}

/// Adds the transaction the user entered, and returns its ID, e.g. to ask
/// whether it is one side of a Transfer (`transfer_prompt.rs`).
pub fn add(
    book: &mut Book,
    entered: &NewEntry<'_>,
    locale: &impl Locale,
) -> Result<TransactionId, EntryError> {
    let account = find_account(book, entered.account)?;
    let category = find_category(book, &account, entered.category)?;
    let (into_account, into_other) = read_amounts(book, &account, entered, locale)?;
    let date = locale.read_date(entered.date).ok_or(EntryError::Date)?;
    let note = read_note(entered.note, None)?.flatten();
    let other = match category {
        Some(category) => Split::new(category.id, into_other),
        None => Split::uncategorized(into_other),
    };
    let transaction = Transaction::new(
        TransactionId::generate(),
        date,
        payee(entered.other_party),
        vec![Split::new(account.id, into_account), other],
    )
    .map_err(EntryError::Transaction)?
    .with_note(note);
    // A tag typed new is made in the same user action, so one Undo takes
    // back both (D-086).
    book.as_one_action(Action::AddTransaction, |book| {
        let tags = match entered.tags {
            Some(wanted) => tags::resolve(book, wanted)?,
            None => Vec::new(),
        };
        book.add_transaction(&transaction.clone().with_tags(tags))
    })?;
    Ok(transaction.id())
}

/// The transaction with `id` (as text) as the page shows it, seen from
/// `account`.
pub fn details(
    book: &Book,
    account: &str,
    id: &str,
    locale: &impl Locale,
) -> Result<EntryForm, EntryError> {
    let account = find_account(book, account)?;
    let transaction = find_transaction(book, account.id, id)?;
    let (kind, other_name) = kind_of(book, account.id, &transaction)?;
    let own = transaction
        .splits()
        .iter()
        .filter(|split| split.account == Some(account.id))
        .collect::<Vec<_>>();
    let amount = Money::checked_sum(
        account.currency,
        own.iter().map(|split| split.account_amount()),
    )
    .map_err(|_| StoreError::AmountOverflow)?;
    let money_in = amount.minor_units() > 0;
    let unsigned = match amount.checked_neg() {
        Ok(positive) if amount.is_negative() => positive,
        _ => amount,
    };
    let category = match kind {
        EntryKind::Simple => transaction
            .splits()
            .iter()
            .find(|split| split.account != Some(account.id))
            .and_then(|split| split.account)
            .map(|id| id.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };
    let own_split = own.first().ok_or(EntryError::NoSuchEntry)?;
    let names = tags::names(book)?;
    // The account's own date and payee, as the register shows them (D-108).
    let date = transaction.date_of(own_split);
    let other_party = transaction.payee_of(own_split).unwrap_or_default();
    let delivered_name = own_split
        .imported
        .as_ref()
        .and_then(|delivered| delivered.other_party_name.as_deref())
        .filter(|name| *name != other_party)
        .unwrap_or_default()
        .to_owned();
    Ok(EntryForm {
        id: transaction.id().to_string(),
        kind: kind.key(),
        account: account.id.to_string(),
        money_in,
        amount: typed_amount(book, unsigned, locale)?,
        other_party: other_party.to_owned(),
        category,
        other_account: match kind {
            EntryKind::Transfer => other_name,
            _ => String::new(),
        },
        date: locale.short_date(date),
        note: transaction.note().unwrap_or_default().to_owned(),
        tags: tags::texts(&names, transaction.tags()),
        part_tags: tags::texts(
            &names,
            &transaction
                .all_tags()
                .into_iter()
                .filter(|tag| !transaction.tags().contains(tag))
                .collect::<Vec<_>>(),
        ),
        imported: own.iter().any(|split| split.imported.is_some()),
        delivered_name,
    })
}

/// What removing the transaction with `id` (as text), seen from `account`,
/// takes with it, and how long Undo can bring it back (D-066, D-375).
pub fn removal_notice(book: &Book, account: &str, id: &str) -> Result<RemovalNotice, EntryError> {
    let account = find_account(book, account)?;
    let transaction = find_transaction(book, account.id, id)?;
    let (undo, undo_days) = match book.history_retention()? {
        HistoryRetention::Forever => ("forever", 0),
        HistoryRetention::Days(0) => ("until-closed", 0),
        HistoryRetention::Days(days) => ("days", days),
    };
    Ok(RemovalNotice {
        holds_original: transaction
            .splits()
            .iter()
            .any(|split| split.imported.is_some()),
        undo,
        undo_days,
    })
}

/// Replaces what the user changed on a simple transaction: its direction,
/// amount, other party, category and date. The parts keep their IDs, so the
/// history records only what changed, and the account's part keeps what a
/// bank or file delivered with it; an imported amount can't change
/// (`store.imported-split-changed`).
pub fn edit(
    book: &mut Book,
    id: &str,
    entered: &NewEntry<'_>,
    locale: &impl Locale,
) -> Result<(), EntryError> {
    let account = find_account(book, entered.account)?;
    let stored = find_transaction(book, account.id, id)?;
    let (kind, _) = kind_of(book, account.id, &stored)?;
    if kind != EntryKind::Simple {
        return Err(EntryError::NotEditableHere);
    }
    let category = find_category(book, &account, entered.category)?;
    let (into_account, into_other) = read_amounts(book, &account, entered, locale)?;
    let date = locale.read_date(entered.date).ok_or(EntryError::Date)?;
    let note = read_note(entered.note, stored.note())?;
    let splits = stored
        .splits()
        .iter()
        .map(|split| {
            if split.account == Some(account.id) {
                Split {
                    amount: into_account,
                    ..split.clone()
                }
            } else {
                Split {
                    account: category.as_ref().map(|category| category.id),
                    amount: into_other,
                    ..split.clone()
                }
            }
        })
        .collect();
    let transaction = Transaction::new(stored.id(), date, payee(entered.other_party), splits)
        .map_err(EntryError::Transaction)?
        .with_details_of(&stored);
    let transaction = match note {
        Some(note) => transaction.with_note(note),
        None => transaction,
    };
    book.as_one_action(Action::EditTransaction, |book| {
        let transaction = match entered.tags {
            Some(wanted) => {
                let tags = tags::resolve(book, wanted)?;
                transaction.with_tags(tags)
            }
            None => transaction,
        };
        book.update_transaction(&transaction)
    })?;
    Ok(())
}

/// Removes the transaction with `id` (as text) from the book, whatever its
/// kind, with what a bank or file delivered for it: the page asked first
/// when it holds that (D-375). Undo brings it back (D-086).
pub fn remove(book: &mut Book, account: &str, id: &str) -> Result<(), EntryError> {
    let account = find_account(book, account)?;
    let transaction = find_transaction(book, account.id, id)?;
    book.remove_transaction(transaction.id())?;
    Ok(())
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(
    book: &Book,
    account: &str,
    error: &EntryError,
    locale: &impl Locale,
) -> ErrorText {
    let plain = ErrorText::plain;
    match error {
        EntryError::Amount(error) => match find_account(book, account) {
            Ok(account) => ErrorText::amount(*error, book, account.currency, locale),
            Err(_) => plain("input.no-such-entry"),
        },
        EntryError::NotPositive => plain("input.amount-not-positive"),
        EntryError::Date => ErrorText {
            code: "input.malformed-date",
            params: vec![("example", locale.short_date(locale.today()))],
        },
        EntryError::NoSuchEntry => plain("input.no-such-entry"),
        EntryError::NotEditableHere => plain("input.not-editable-here"),
        EntryError::Store(error) => report_text(book, &error.report(), locale),
        EntryError::Transaction(error) => report_text(book, &error.report(), locale),
        EntryError::Note { max } => ErrorText {
            code: "input.note-invalid",
            params: vec![("max", max.to_string())],
        },
    }
}

/// An amount in `account`'s currency written the way the user should type
/// it, e.g. "1.234,56" in German: shown when what they typed couldn't be
/// read.
pub fn example(book: &Book, account: &str, locale: &impl Locale) -> Result<String, EntryError> {
    let account = find_account(book, account)?;
    Ok(amount_example(book, account.currency, locale)?)
}

/// The user's open account the UI means by `id`.
pub(crate) fn find_account(book: &Book, id: &str) -> Result<Account, EntryError> {
    book.all_accounts()?
        .into_iter()
        .find(|account| account.kind.is_account() && account.id.to_string() == id)
        .ok_or(EntryError::NoSuchEntry)
}

/// The category the UI means by `id`, in `account`'s currency; `None` for
/// an empty ID, i.e. no category yet.
fn find_category(book: &Book, account: &Account, id: &str) -> Result<Option<Account>, EntryError> {
    if id.is_empty() {
        return Ok(None);
    }
    book.all_accounts()?
        .into_iter()
        .find(|category| {
            category.kind.is_category()
                && !category.closed
                && category.currency == account.currency
                && category.id.to_string() == id
        })
        .map(Some)
        .ok_or(EntryError::NoSuchEntry)
}

/// The transaction the UI means by `id`, which must touch `account`.
pub(crate) fn find_transaction(
    book: &Book,
    account: AccountId,
    id: &str,
) -> Result<Transaction, EntryError> {
    let entry = book
        .register(account)?
        .into_iter()
        .find(|entry| entry.transaction.to_string() == id)
        .ok_or(EntryError::NoSuchEntry)?;
    Ok(book.transaction(entry.transaction)?)
}

/// The typed amount as what it does to the account and to the other part.
fn read_amounts(
    book: &Book,
    account: &Account,
    entered: &NewEntry<'_>,
    locale: &impl Locale,
) -> Result<(Money, Money), EntryError> {
    let format = locale.amount_format(account.currency);
    let group_separator = locale.group_separator();
    let reader = AmountReader {
        format: &format,
        group_separator: &group_separator,
        currency: account.currency,
        minor_digits: book.currency_minor_digits(account.currency)?,
    };
    let amount = reader.read(entered.amount).map_err(EntryError::Amount)?;
    if amount.is_zero() || amount.is_negative() {
        return Err(EntryError::NotPositive);
    }
    let too_large = |_| EntryError::Amount(AmountInputError::TooLarge);
    let into_account = if entered.money_in {
        amount
    } else {
        amount.checked_neg().map_err(too_large)?
    };
    let into_other = into_account.checked_neg().map_err(too_large)?;
    Ok((into_account, into_other))
}

/// `amount` written the way the user types it: with the regional
/// separators, every decimal place and no currency symbol.
pub(crate) fn typed_amount(
    book: &Book,
    amount: Money,
    locale: &impl Locale,
) -> Result<String, StoreError> {
    let format = AmountFormat {
        positive: Affixes::default(),
        negative: Affixes::default(),
        ..locale.amount_format(amount.currency())
    };
    let minor_digits = book.currency_minor_digits(amount.currency())?;
    Ok(format.format(amount, minor_digits, |value| locale.group_digits(value)))
}

/// A note as typed: `None` to leave the note as it is, else the note to
/// keep (`None` inside for none), trimmed at both ends, with every line
/// break written as `\n` (D-703). It may have line breaks and tabs, as
/// memos from a bank or KMyMoney do; another control character is refused,
/// and so is a note longer than [`MAX_NOTE_CHARS`] or than `stored`, the
/// note it replaces, whichever is longer, so a long imported note can be
/// changed without being cut.
fn read_note(
    typed: Option<&str>,
    stored: Option<&str>,
) -> Result<Option<Option<String>>, EntryError> {
    let Some(typed) = typed else {
        return Ok(None);
    };
    let note = normalized_note(typed);
    let longest = note_limit(stored.unwrap_or_default());
    if note.chars().count() > longest
        || note
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(EntryError::Note { max: longest });
    }
    Ok(Some((!note.is_empty()).then_some(note)))
}

/// A note as it is kept: trimmed at both ends, every line break `\n`.
fn normalized_note(typed: &str) -> String {
    typed.trim().replace("\r\n", "\n").replace('\r', "\n")
}

/// How long the note `typed` is, counted the way a save counts it, so the
/// page's counter and its stop at the limit agree with the save (B-226).
pub fn note_length(typed: &str) -> usize {
    normalized_note(typed).chars().count()
}

/// The longest note that may replace `stored`: [`MAX_NOTE_CHARS`], or the
/// stored note's own length if it is longer, e.g. a memo imported from a
/// KMyMoney book (D-703). An added transaction has none stored (`""`).
pub fn note_limit(stored: &str) -> usize {
    note_length(stored).max(MAX_NOTE_CHARS)
}

/// Who the money came from or went to, without spaces around it; none if
/// nothing was typed.
pub(crate) fn payee(typed: &str) -> Option<String> {
    let trimmed = typed.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountType, CurrencyCode, Date, ImportedDetails};

    use super::*;
    use crate::accounts::{NewAccount, add_account, correct_starting_balance};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::register::{EntryKind, Sort, SortKey, register_view};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// A book after the first start, with the accounts screen's "Checking
    /// account" holding 1000 € on 1 Oct 2026 (M3.3's real output).
    fn book_with_checking() -> (Book, String) {
        let mut book = started();
        let checking = account(&mut book, "Checking account", AccountType::Checking, "1000");
        (book, checking)
    }

    fn account(book: &mut Book, name: &str, account_type: AccountType, balance: &str) -> String {
        add_account(
            book,
            &NewAccount {
                name,
                account_type,
                currency: "EUR",
                balance,
                date: "2026-10-01",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string()
    }

    fn category_id(book: &Book, path: &str) -> String {
        book.categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.path.join(" › ") == path)
            .map(|entry| entry.category.id.to_string())
            .unwrap_or_else(|| panic!("category {path}"))
    }

    fn paid<'a>(account: &'a str, amount: &'a str, to: &'a str, category: &'a str) -> NewEntry<'a> {
        NewEntry {
            account,
            money_in: false,
            amount,
            other_party: to,
            category,
            date: "2026-10-05",
            note: None,
            tags: None,
        }
    }

    fn rows(
        book: &Book,
        account: &str,
        sort: Sort,
    ) -> Vec<(String, String, String, String, &'static str, String)> {
        let id = find_account(book, account).expect("account").id;
        register_view(book, id, sort, &TestLocale)
            .expect("register")
            .rows
            .into_iter()
            .map(|row| {
                (
                    row.date,
                    row.other_party,
                    row.category,
                    row.amount.text,
                    row.kind,
                    row.balance.text,
                )
            })
            .collect()
    }

    fn row(
        date: &str,
        other: &str,
        category: &str,
        amount: &str,
        kind: &'static str,
        balance: &str,
    ) -> (String, String, String, String, &'static str, String) {
        (
            date.into(),
            other.into(),
            category.into(),
            amount.into(),
            kind,
            balance.into(),
        )
    }

    fn code<T: std::fmt::Debug>(
        book: &Book,
        account: &str,
        result: Result<T, EntryError>,
    ) -> &'static str {
        let error = result.expect_err("refused");
        error_text(book, account, &error, &TestLocale).code
    }

    /// The ID of the account's newest (`last`) or oldest transaction.
    fn transaction_at(book: &Book, account: &str, last: bool) -> String {
        let id = find_account(book, account).expect("account").id;
        let mut ids = book
            .register(id)
            .expect("register")
            .into_iter()
            .map(|entry| entry.transaction.to_string());
        if last { ids.next_back() } else { ids.next() }.expect("a transaction")
    }

    #[test]
    fn adds_a_payment_and_shows_what_was_left_after_it() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        add(
            &mut book,
            &paid(&checking, "53,20", "Corner Grocery", &groceries),
            &TestLocale,
        )
        .expect("added");
        let salary = category_id(&book, "Salary");
        add(
            &mut book,
            &NewEntry {
                money_in: true,
                date: "2026-10-03",
                ..paid(&checking, "2.400", "Example Employer Ltd.", &salary)
            },
            &TestLocale,
        )
        .expect("added");

        // Newest first; the balance after each entry counts in date order.
        assert_eq!(
            rows(&book, &checking, Sort::default()),
            [
                row(
                    "2026-10-05",
                    "Corner Grocery",
                    "Groceries",
                    "-53,20 €",
                    "simple",
                    "3346,80 €"
                ),
                row(
                    "2026-10-03",
                    "Example Employer Ltd.",
                    "Salary",
                    "2400,00 €",
                    "simple",
                    "3400,00 €"
                ),
                row(
                    "2026-10-01",
                    "",
                    "",
                    "1000,00 €",
                    "starting-balance",
                    "1000,00 €"
                ),
            ]
        );
        let id = find_account(&book, &checking).expect("account").id;
        let view = register_view(&book, id, Sort::default(), &TestLocale).expect("register");
        assert_eq!(view.balance.text, "3346,80 €");
        assert_eq!(view.account_name, "Checking account");
        assert!(!view.owes);
    }

    #[test]
    fn a_debt_shows_what_was_owed_after_each_entry() {
        let mut book = started();
        let card = account(&mut book, "Credit card", AccountType::CreditCard, "300");
        let clothes = category_id(&book, "Clothes");
        add(
            &mut book,
            &paid(&card, "89,90", "Example Shoes", &clothes),
            &TestLocale,
        )
        .expect("added");
        let id = find_account(&book, &card).expect("account").id;
        let view = register_view(&book, id, Sort::default(), &TestLocale).expect("register");
        assert!(view.owes);
        assert_eq!(view.balance.text, "389,90 €");
        let owed: Vec<_> = view
            .rows
            .iter()
            .map(|row| (row.balance.text.as_str(), row.owes))
            .collect();
        assert_eq!(owed, [("389,90 €", true), ("300,00 €", true)]);
    }

    #[test]
    fn sorts_the_rows_in_rust_without_moving_the_balances() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let clothes = category_id(&book, "Clothes");
        add(
            &mut book,
            &paid(&checking, "53,20", "Corner Grocery", &groceries),
            &TestLocale,
        )
        .expect("added");
        add(
            &mut book,
            &NewEntry {
                date: "2026-10-07",
                ..paid(&checking, "89,90", "example shoes", &clothes)
            },
            &TestLocale,
        )
        .expect("added");
        add(
            &mut book,
            &NewEntry {
                date: "2026-10-09",
                ..paid(&checking, "12", "", "")
            },
            &TestLocale,
        )
        .expect("added");

        let sorted = |key, descending| {
            rows(&book, &checking, Sort { key, descending })
                .into_iter()
                .map(|(_, other, _, amount, _, balance)| format!("{other}|{amount}|{balance}"))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sorted(SortKey::Date, false),
            [
                "|1000,00 €|1000,00 €",
                "Corner Grocery|-53,20 €|946,80 €",
                "example shoes|-89,90 €|856,90 €",
                "|-12,00 €|844,90 €"
            ]
        );
        assert_eq!(
            sorted(SortKey::Amount, true),
            [
                "|1000,00 €|",
                "|-12,00 €|",
                "Corner Grocery|-53,20 €|",
                "example shoes|-89,90 €|"
            ]
        );
        // The balance after each entry comes only with date order (US1.18),
        // so these have none. Ignoring case; a tie keeps the date order.
        assert_eq!(
            sorted(SortKey::OtherParty, false),
            [
                "|1000,00 €|",
                "|-12,00 €|",
                "Corner Grocery|-53,20 €|",
                "example shoes|-89,90 €|"
            ]
        );
        assert_eq!(
            sorted(SortKey::Category, true),
            [
                "Corner Grocery|-53,20 €|",
                "example shoes|-89,90 €|",
                "|1000,00 €|",
                "|-12,00 €|"
            ]
        );
        assert_eq!(SortKey::from_key("other-party"), Some(SortKey::OtherParty));
        assert_eq!(SortKey::from_key("size"), None);
    }

    #[test]
    fn refuses_what_it_cannot_read_with_a_message_each() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let refused = |book: &mut Book, entered: NewEntry<'_>| {
            let result = add(book, &entered, &TestLocale);
            code(book, &checking, result)
        };
        assert_eq!(
            refused(&mut book, paid(&checking, "", "", &groceries)),
            "input.empty-amount"
        );
        assert_eq!(
            refused(&mut book, paid(&checking, "0", "", &groceries)),
            "input.amount-not-positive"
        );
        assert_eq!(
            refused(&mut book, paid(&checking, "-5", "", &groceries)),
            "input.amount-not-positive"
        );
        assert_eq!(
            refused(&mut book, paid(&checking, "12.50", "", &groceries)),
            "input.malformed-amount"
        );
        assert_eq!(
            refused(&mut book, paid(&checking, "1,234", "", &groceries)),
            "input.too-many-decimal-places"
        );
        assert_eq!(
            refused(
                &mut book,
                NewEntry {
                    date: "soon",
                    ..paid(&checking, "5", "", &groceries)
                }
            ),
            "input.malformed-date"
        );
        assert_eq!(
            refused(&mut book, paid("nobody", "5", "", &groceries)),
            "input.no-such-entry"
        );
        // A category the account's currency doesn't have, or that isn't one.
        assert_eq!(
            refused(&mut book, paid(&checking, "5", "", &checking)),
            "input.no-such-entry"
        );
        let error = add(
            &mut book,
            &paid(&checking, "12.50", "", &groceries),
            &TestLocale,
        )
        .expect_err("refused");
        assert_eq!(
            error_text(&book, &checking, &error, &TestLocale).params,
            [("example", "1234,56".to_owned())]
        );
        assert_eq!(
            example(&book, &checking, &TestLocale).expect("example"),
            "1234,56"
        );
        // Nothing was added.
        assert_eq!(rows(&book, &checking, Sort::default()).len(), 1);
    }

    #[test]
    fn changes_a_transaction_and_keeps_its_parts() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let clothes = category_id(&book, "Clothes");
        add(
            &mut book,
            &paid(&checking, "53,20", "Corner Grocery", &groceries),
            &TestLocale,
        )
        .expect("added");
        let id = transaction_at(&book, &checking, true);
        let before = book
            .transaction(
                find_transaction(
                    &book,
                    find_account(&book, &checking).expect("account").id,
                    &id,
                )
                .expect("found")
                .id(),
            )
            .expect("stored");

        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(
            form,
            EntryForm {
                id: id.clone(),
                kind: "simple",
                account: checking.clone(),
                money_in: false,
                amount: "53,20".into(),
                other_party: "Corner Grocery".into(),
                category: groceries.clone(),
                other_account: String::new(),
                date: "2026-10-05".into(),
                note: String::new(),
                imported: false,
                delivered_name: String::new(),
                tags: Vec::new(),
                part_tags: Vec::new(),
            }
        );
        // Saving the form unchanged reads the amount it wrote.
        edit(
            &mut book,
            &id,
            &paid(&checking, &form.amount, &form.other_party, &form.category),
            &TestLocale,
        )
        .expect("saved");

        edit(
            &mut book,
            &id,
            &NewEntry {
                money_in: true,
                date: "2026-10-06",
                ..paid(&checking, "89,90", " Example Shoes ", &clothes)
            },
            &TestLocale,
        )
        .expect("changed");
        assert_eq!(
            rows(&book, &checking, Sort::default()),
            [
                row(
                    "2026-10-06",
                    "Example Shoes",
                    "Clothes",
                    "89,90 €",
                    "simple",
                    "1089,90 €"
                ),
                row(
                    "2026-10-01",
                    "",
                    "",
                    "1000,00 €",
                    "starting-balance",
                    "1000,00 €"
                ),
            ]
        );
        let after = book.transaction(before.id()).expect("stored");
        let ids = |transaction: &Transaction| {
            transaction
                .splits()
                .iter()
                .map(|split| split.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&before), ids(&after));

        // Without a category, the other part has no account.
        edit(&mut book, &id, &paid(&checking, "5", "", ""), &TestLocale).expect("changed");
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(
            (form.category.as_str(), form.money_in, form.amount.as_str()),
            ("", false, "5,00")
        );
        assert_eq!(rows(&book, &checking, Sort::default())[0].2, "");

        remove(&mut book, &checking, &id).expect("removed");
        assert_eq!(rows(&book, &checking, Sort::default()).len(), 1);
        let gone = details(&book, &checking, &id, &TestLocale);
        assert!(matches!(gone, Err(EntryError::NoSuchEntry)));
    }

    #[test]
    fn offers_the_categories_for_the_direction_and_keeps_the_current_one() {
        let (book, checking) = book_with_checking();
        let paths = |money_in, current: &str| {
            category_choices(&book, &checking, money_in, current)
                .expect("choices")
                .into_iter()
                .map(|choice| format!("{}{}", "  ".repeat(choice.depth), choice.path))
                .collect::<Vec<_>>()
        };
        // In tree order, as the categories screen lists them (M3.3).
        assert_eq!(
            paths(false, ""),
            [
                "Clothes",
                "Food",
                "  Food › Eating out",
                "  Food › Groceries",
                "Rent"
            ]
        );
        assert_eq!(paths(true, ""), ["Other income", "Salary"]);
        let clothes = category_id(&book, "Clothes");
        assert_eq!(paths(true, &clothes), ["Other income", "Salary", "Clothes"]);
        assert!(matches!(
            category_choices(&book, "nobody", false, ""),
            Err(EntryError::NoSuchEntry)
        ));
    }

    #[test]
    fn lists_the_open_accounts_in_the_order_they_were_added() {
        let (mut book, checking) = book_with_checking();
        let cash = account(&mut book, "Cash", AccountType::Cash, "");
        let old = account(&mut book, "Old card", AccountType::CreditCard, "");
        let old_id = find_account(&book, &old).expect("account").id;
        book.close_account(old_id).expect("closed");
        let choices: Vec<_> = account_choices(&book)
            .expect("choices")
            .into_iter()
            .map(|choice| (choice.id, choice.name))
            .collect();
        assert_eq!(
            choices,
            [
                (checking, "Checking account".to_owned()),
                (cash, "Cash".to_owned())
            ]
        );
    }

    #[test]
    fn shows_a_starting_balance_a_transfer_and_parts_for_what_they_are() {
        let (mut book, checking) = book_with_checking();
        let savings = account(&mut book, "Savings", AccountType::Savings, "");
        let checking_id = find_account(&book, &checking).expect("account").id;
        let savings_id = find_account(&book, &savings).expect("account").id;
        let groceries = find_category(
            &book,
            &find_account(&book, &checking).expect("account"),
            &category_id(&book, "Food › Groceries"),
        )
        .expect("category")
        .expect("found")
        .id;
        let eur = |minor| Money::from_minor_units(minor, EUR);
        let day = Date::constant(2026, 10, 8);
        let transfer = Transaction::new(
            TransactionId::generate(),
            day,
            None,
            vec![
                Split::new(checking_id, eur(-20000)),
                Split::new(savings_id, eur(20000)),
            ],
        )
        .expect("transfer");
        book.add_transaction(&transfer).expect("added");
        let parts = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 9),
            Some("Supermarket".into()),
            vec![
                Split::new(checking_id, eur(-3000)),
                Split::new(groceries, eur(2000)),
                Split::uncategorized(eur(1000)),
            ],
        )
        .expect("parts");
        book.add_transaction(&parts).expect("added");

        assert_eq!(
            rows(&book, &checking, Sort::default()),
            [
                row(
                    "2026-10-09",
                    "Supermarket",
                    "",
                    "-30,00 €",
                    "parts",
                    "770,00 €"
                ),
                row(
                    "2026-10-08",
                    "",
                    "Savings",
                    "-200,00 €",
                    "transfer",
                    "800,00 €"
                ),
                row(
                    "2026-10-01",
                    "",
                    "",
                    "1000,00 €",
                    "starting-balance",
                    "1000,00 €"
                ),
            ]
        );
        assert_eq!(
            rows(&book, &savings, Sort::default()),
            [row(
                "2026-10-08",
                "",
                "Checking account",
                "200,00 €",
                "transfer",
                "200,00 €"
            )]
        );

        let transfer_id = transfer.id().to_string();
        let form = details(&book, &checking, &transfer_id, &TestLocale).expect("details");
        assert_eq!(
            (
                form.kind,
                form.other_account.as_str(),
                form.amount.as_str(),
                form.money_in
            ),
            ("transfer", "Savings", "200,00", false)
        );
        let refused = edit(
            &mut book,
            &transfer_id,
            &paid(&checking, "5", "", ""),
            &TestLocale,
        );
        assert_eq!(code(&book, &checking, refused), "input.not-editable-here");
        let starting = transaction_at(&book, &checking, false);
        let refused = edit(
            &mut book,
            &starting,
            &paid(&checking, "5", "", ""),
            &TestLocale,
        );
        assert_eq!(code(&book, &checking, refused), "input.not-editable-here");
        assert_eq!(
            details(&book, &checking, &starting, &TestLocale)
                .expect("details")
                .kind,
            EntryKind::StartingBalance.key()
        );

        // "Check against my bank" corrects the starting balance (M3.4a's real
        // output): the register shows the corrected one.
        correct_starting_balance(&mut book, &checking, "1.050", "2026-10-01", &TestLocale)
            .expect("corrected");
        assert_eq!(rows(&book, &checking, Sort::default())[2].3, "1050,00 €");
        assert_eq!(rows(&book, &checking, Sort::default())[0].5, "820,00 €");

        // A Transfer can be removed from either side.
        remove(&mut book, &savings, &transfer_id).expect("removed");
        assert_eq!(rows(&book, &savings, Sort::default()), []);
        assert_eq!(rows(&book, &checking, Sort::default()).len(), 2);
    }

    /// The register reads only an outline of each transaction (M6-B8,
    /// D-387): every line still has the kind, category and note the whole
    /// transaction gives it.
    #[test]
    fn the_register_tells_each_line_as_the_whole_transaction_does() {
        let (mut book, checking) = book_with_checking();
        let savings = account(&mut book, "Savings", AccountType::Savings, "");
        let checking_id = find_account(&book, &checking).expect("account").id;
        let savings_id = find_account(&book, &savings).expect("account").id;
        let groceries = category_id(&book, "Food › Groceries");
        add(
            &mut book,
            &paid(&checking, "12", "Corner Grocery", &groceries),
            &TestLocale,
        )
        .expect("added");
        add(&mut book, &paid(&checking, "3", "Kiosk", ""), &TestLocale).expect("added");
        let eur = |minor| Money::from_minor_units(minor, EUR);
        let day = Date::constant(2026, 10, 8);
        let noted_part = Split {
            note: Some("Rent share".into()),
            ..Split::new(checking_id, eur(-20000))
        };
        let transfer = Transaction::new(
            TransactionId::generate(),
            day,
            None,
            vec![noted_part, Split::new(savings_id, eur(20000))],
        )
        .expect("transfer");
        book.add_transaction(&transfer).expect("added");
        let parts = Transaction::new(
            TransactionId::generate(),
            day,
            Some("Supermarket".into()),
            vec![
                Split::new(checking_id, eur(-3000)),
                Split::uncategorized(eur(3000)),
                Split::new(checking_id, eur(1000)),
                Split::uncategorized(eur(-1000)),
            ],
        )
        .expect("parts")
        .with_note(Some("Bottle deposit back".into()));
        book.add_transaction(&parts).expect("added");

        for id in [checking_id, savings_id] {
            let entries = crate::register::entries(&book, id).expect("register");
            assert!(!entries.is_empty());
            for entry in entries {
                let whole = book.transaction(entry.transaction).expect("read");
                let note = whole
                    .note()
                    .or_else(|| {
                        whole
                            .splits()
                            .iter()
                            .find(|split| split.account == Some(id))
                            .and_then(|split| split.note.as_deref())
                    })
                    .unwrap_or_default();
                assert_eq!(
                    (entry.kind, entry.category, entry.note.as_str()),
                    {
                        let (kind, category) = kind_of(&book, id, &whole).expect("kind");
                        (kind, category, note)
                    },
                    "{:?}",
                    entry.transaction
                );
            }
        }
        let kinds: Vec<EntryKind> = crate::register::entries(&book, checking_id)
            .expect("register")
            .into_iter()
            .map(|entry| entry.kind)
            .collect();
        for kind in [
            EntryKind::Simple,
            EntryKind::StartingBalance,
            EntryKind::Transfer,
            EntryKind::Parts,
        ] {
            assert!(kinds.contains(&kind), "{kind:?}");
        }
    }

    #[test]
    fn keeps_what_the_bank_delivered() {
        let (mut book, checking) = book_with_checking();
        let checking_id = find_account(&book, &checking).expect("account").id;
        let eur = |minor| Money::from_minor_units(minor, EUR);
        let imported = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 8),
            Some("CORNER GROCERY".into()),
            vec![
                Split {
                    imported: Some(ImportedDetails {
                        bank_id: Some("1".into()),
                        booking_date: None,
                        value_date: None,
                        other_party_name: Some("CORNER GROCERY".into()),
                        other_party_iban: None,
                        remittance_information: None,
                    }),
                    ..Split::new(checking_id, eur(-5320))
                },
                Split::uncategorized(eur(5320)),
            ],
        )
        .expect("imported");
        book.add_transaction(&imported).expect("added");
        let id = imported.id().to_string();
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert!(form.imported);
        assert_eq!(form.kind, "simple");
        // Shown as delivered, so there is nothing to show next to it.
        assert_eq!(form.delivered_name, "");

        // Sorting it into a category keeps the amount; changing the amount
        // is refused with the store's code.
        let groceries = category_id(&book, "Food › Groceries");
        edit(
            &mut book,
            &id,
            &paid(&checking, "53,20", "Corner Grocery", &groceries),
            &TestLocale,
        )
        .expect("sorted");
        assert_eq!(rows(&book, &checking, Sort::default())[0].2, "Groceries");
        // The new name is shown, the delivered one stays next to it (D-375).
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(form.other_party, "Corner Grocery");
        assert_eq!(form.delivered_name, "CORNER GROCERY");
        let refused = edit(
            &mut book,
            &id,
            &paid(&checking, "53", "Corner Grocery", &groceries),
            &TestLocale,
        );
        assert_eq!(
            code(&book, &checking, refused),
            "store.imported-split-changed"
        );
    }

    #[test]
    fn removing_says_what_goes_with_it_and_how_long_undo_brings_it_back() {
        let (mut book, checking) = book_with_checking();
        let checking_id = find_account(&book, &checking).expect("account").id;
        let eur = |minor| Money::from_minor_units(minor, EUR);
        // An entry as the import pipeline makes it from a statement, with
        // what the bank delivered on the account's part.
        let imported = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 8),
            Some("CORNER GROCERY".into()),
            vec![
                Split {
                    imported: Some(ImportedDetails {
                        bank_id: Some("1".into()),
                        booking_date: None,
                        value_date: None,
                        other_party_name: Some("CORNER GROCERY".into()),
                        other_party_iban: None,
                        remittance_information: None,
                    }),
                    ..Split::new(checking_id, eur(-5320))
                },
                Split::uncategorized(eur(5320)),
            ],
        )
        .expect("imported");
        book.add_transaction(&imported).expect("added");
        let imported_id = imported.id().to_string();
        let groceries = category_id(&book, "Food › Groceries");
        let typed = add(
            &mut book,
            &paid(&checking, "12,00", "Bakery", &groceries),
            &TestLocale,
        )
        .expect("added")
        .to_string();

        let notice = removal_notice(&book, &checking, &imported_id).expect("notice");
        assert_eq!(
            notice,
            RemovalNotice {
                holds_original: true,
                undo: "days",
                undo_days: 30,
            }
        );
        let typed_notice = removal_notice(&book, &checking, &typed).expect("notice");
        assert!(!typed_notice.holds_original);
        for (keep, undo, days) in [
            (HistoryRetention::Days(0), "until-closed", 0),
            (HistoryRetention::Days(7), "days", 7),
            (HistoryRetention::Forever, "forever", 0),
        ] {
            book.set_history_retention(keep).expect("set");
            let notice = removal_notice(&book, &checking, &imported_id).expect("notice");
            assert_eq!((notice.undo, notice.undo_days), (undo, days));
        }
        // Removing takes the original too; Undo brings it back whole.
        remove(&mut book, &checking, &imported_id).expect("removed");
        assert!(book.transaction(imported.id()).is_err());
        book.undo().expect("undone").expect("a change");
        assert_eq!(book.transaction(imported.id()).expect("back"), imported);
    }

    #[test]
    fn refuses_an_account_that_is_closed() {
        let (mut book, checking) = book_with_checking();
        let cash = account(&mut book, "Cash", AccountType::Cash, "");
        let cash_id = find_account(&book, &cash).expect("account").id;
        book.close_account(cash_id).expect("closed");
        // A closed account is no choice on the page; the book refuses it
        // anyway, and says which account is closed.
        let refused = add(&mut book, &paid(&cash, "5", "", ""), &TestLocale);
        assert_eq!(code(&book, &checking, refused), "store.account-closed");
    }

    #[test]
    fn a_note_is_added_shown_changed_kept_and_removed() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let account_id = find_account(&book, &checking).expect("account").id;
        add(
            &mut book,
            &NewEntry {
                note: Some("  Birthday cake  "),
                ..paid(&checking, "12", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("added");
        let id = transaction_at(&book, &checking, true);
        // The form shows the note, trimmed, and so does the register row,
        // which reads it from the same transaction.
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(form.note, "Birthday cake");
        let view = register_view(&book, account_id, Sort::default(), &TestLocale).expect("view");
        assert_eq!(view.rows[0].note, "Birthday cake");

        // A save without a note field leaves the note as it is.
        edit(
            &mut book,
            &id,
            &paid(&checking, "13", "Corner Grocery", &groceries),
            &TestLocale,
        )
        .expect("edited");
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(
            (form.amount.as_str(), form.note.as_str()),
            ("13,00", "Birthday cake")
        );

        // A new note replaces it; an empty one takes it away.
        edit(
            &mut book,
            &id,
            &NewEntry {
                note: Some("For Sam"),
                ..paid(&checking, "13", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("edited");
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(form.note, "For Sam");
        edit(
            &mut book,
            &id,
            &NewEntry {
                note: Some("   "),
                ..paid(&checking, "13", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("edited");
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(form.note, "");
        let stored = find_transaction(&book, account_id, &id).expect("stored");
        assert_eq!(stored.note(), None);
    }

    #[test]
    fn tags_typed_new_are_made_with_the_save_and_a_part_tag_is_found_too() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let account_id = find_account(&book, &checking).expect("account").id;
        let wanted = [TagInput {
            id: "",
            name: "  Garden party  ",
        }];
        add(
            &mut book,
            &NewEntry {
                tags: Some(&wanted),
                ..paid(&checking, "30", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("added");
        let id = transaction_at(&book, &checking, true);
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(
            form.tags
                .iter()
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>(),
            ["Garden party"]
        );
        let party = form.tags[0].id.clone();

        // One Undo takes back the entry and the tag made with it.
        book.undo().expect("undone");
        assert!(book.tags().expect("tags").is_empty());
        book.redo().expect("redone");

        // A name the book has already, in other capitals, takes that tag;
        // saving without a tags field leaves the tags as they are.
        let again = [
            TagInput {
                id: &party,
                name: "",
            },
            TagInput {
                id: "",
                name: "garden PARTY",
            },
        ];
        edit(
            &mut book,
            &id,
            &NewEntry {
                tags: Some(&again),
                ..paid(&checking, "31", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("edited");
        edit(
            &mut book,
            &id,
            &paid(&checking, "32", "Corner Grocery", &groceries),
            &TestLocale,
        )
        .expect("edited");
        assert_eq!(book.tags().expect("tags").len(), 1);
        let stored = find_transaction(&book, account_id, &id).expect("stored");
        assert_eq!(stored.tags().len(), 1);

        // A part's own tag shows on the form apart from the entry's, and
        // the register row carries both, so search and the tag filter find
        // the entry by either (Q-493).
        let tax = kassensturz_core::Tag {
            id: kassensturz_core::TagId::generate(),
            name: "Tax".into(),
        };
        book.add_tag(&tax).expect("tag");
        let splits = stored
            .splits()
            .iter()
            .map(|split| {
                if split.account == Some(account_id) {
                    split.clone()
                } else {
                    Split {
                        tags: vec![tax.id],
                        ..split.clone()
                    }
                }
            })
            .collect();
        let tagged = Transaction::new(
            stored.id(),
            stored.booking_date(),
            stored.payee().map(str::to_owned),
            splits,
        )
        .expect("balances")
        .with_details_of(&stored);
        book.update_transaction(&tagged).expect("updated");
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(form.part_tags.len(), 1);
        assert_eq!(form.part_tags[0].name, "Tax");
        let view = register_view(&book, account_id, Sort::default(), &TestLocale).expect("view");
        let row = &view.rows[0];
        assert_eq!(row.tag_line(), "Garden party, Tax");
        assert!(row.has_tag(&party) && row.has_tag(&tax.id.to_string()));
        assert!(row.fits("tax") && row.fits("garden"));

        // A tag ID the book doesn't have is refused and nothing changes.
        let unknown = kassensturz_core::TagId::generate().to_string();
        let wrong = [TagInput {
            id: &unknown,
            name: "",
        }];
        assert!(
            edit(
                &mut book,
                &id,
                &NewEntry {
                    tags: Some(&wrong),
                    ..paid(&checking, "99", "Corner Grocery", &groceries)
                },
                &TestLocale,
            )
            .is_err()
        );
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!((form.amount.as_str(), form.tags.len()), ("32,00", 1));
    }

    #[test]
    fn a_note_too_long_or_with_a_control_character_is_refused_and_nothing_changes() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let long = "x".repeat(MAX_NOTE_CHARS + 1);
        for bad in [long.as_str(), "bell\u{7}", "escape\u{1b}[0m"] {
            let refused = add(
                &mut book,
                &NewEntry {
                    note: Some(bad),
                    ..paid(&checking, "12", "Corner Grocery", &groceries)
                },
                &TestLocale,
            );
            let error = refused.expect_err("refused");
            let text = error_text(&book, &checking, &error, &TestLocale);
            assert_eq!(text.code, "input.note-invalid");
            assert_eq!(text.params, [("max", "512".to_owned())]);
        }
        // Nothing was added, and the longest note still fits.
        let id = find_account(&book, &checking).expect("account").id;
        assert_eq!(book.register(id).expect("register").len(), 1);
        let exact = "x".repeat(MAX_NOTE_CHARS);
        add(
            &mut book,
            &NewEntry {
                note: Some(&exact),
                ..paid(&checking, "12", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("added");
        let stored = transaction_at(&book, &checking, true);
        let refused = edit(
            &mut book,
            &stored,
            &NewEntry {
                note: Some(&long),
                ..paid(&checking, "99", "Corner Grocery", &groceries)
            },
            &TestLocale,
        );
        assert_eq!(code(&book, &checking, refused), "input.note-invalid");
        let form = details(&book, &checking, &stored, &TestLocale).expect("details");
        assert_eq!(
            (form.amount.as_str(), form.note.len()),
            ("12,00", MAX_NOTE_CHARS)
        );
    }

    #[test]
    fn the_page_counts_a_note_the_way_a_save_does() {
        // Spaces around it don't count, a line break counts once however it
        // was typed, and a character outside the basic plane counts once.
        assert_eq!(note_length("  two\r\nlines\t "), 9);
        assert_eq!(note_length("caf\u{e9} \u{1f600}"), 6);
        assert_eq!(note_limit(""), MAX_NOTE_CHARS);
        assert_eq!(note_limit("short"), MAX_NOTE_CHARS);
        let imported = "y".repeat(MAX_NOTE_CHARS + 88);
        assert_eq!(note_limit(&imported), MAX_NOTE_CHARS + 88);
        // What the counter calls full is exactly what a save still takes.
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let full = format!(" {}\r\n", "z".repeat(MAX_NOTE_CHARS - 2) + "\r\nz");
        assert_eq!(note_length(&full), note_limit(""));
        add(
            &mut book,
            &NewEntry {
                note: Some(&full),
                ..paid(&checking, "12", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("added");
        let one_more = format!("{full}z");
        assert!(note_length(&one_more) > note_limit(""));
        let refused = add(
            &mut book,
            &NewEntry {
                note: Some(&one_more),
                ..paid(&checking, "12", "Corner Grocery", &groceries)
            },
            &TestLocale,
        );
        assert_eq!(code(&book, &checking, refused), "input.note-invalid");
    }

    #[test]
    fn a_note_keeps_its_line_breaks_and_tabs() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        add(
            &mut book,
            &NewEntry {
                note: Some("  Shopping list\r\n- bread\r- milk\tx2\n  "),
                ..paid(&checking, "12", "Corner Grocery", &groceries)
            },
            &TestLocale,
        )
        .expect("added");
        // Trimmed at both ends, every line break written as `\n` (D-703).
        let id = transaction_at(&book, &checking, true);
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(form.note, "Shopping list\n- bread\n- milk\tx2");
    }

    /// A KMyMoney file with a two-line memo and one of 303 characters, as
    /// such memos often are (synthetic, from the US1 to US3 review), and
    /// `LONGER_MEMO` stands for one longer than the page takes.
    const MEMO_NOTES_KMY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE KMYMONEY-FILE>
<KMYMONEY-FILE>
  <FILEINFO>
    <VERSION id="1"/>
  </FILEINFO>
  <ACCOUNTS>
    <ACCOUNT id="AStd::Asset" parentaccount="" type="9" name="Anlagen" currency="EUR"/>
    <ACCOUNT id="AStd::Expense" parentaccount="" type="13" name="Ausgabe" currency="EUR"/>
    <ACCOUNT id="A000001" parentaccount="AStd::Asset" type="1" name="Checking" currency="EUR"/>
    <ACCOUNT id="A000002" parentaccount="AStd::Expense" type="13" name="Groceries" currency="EUR"/>
  </ACCOUNTS>
  <TRANSACTIONS>
    <TRANSACTION id="T000000000000000001" postdate="2020-01-03" memo="Line one of the memo&#10;line two of the memo" commodity="EUR">
      <SPLITS>
        <SPLIT id="S0001" payee="" reconcileflag="0" value="-2475/100" shares="-2475/100" memo="" account="A000001" bankid=""/>
        <SPLIT id="S0002" payee="" reconcileflag="0" value="2475/100" shares="2475/100" memo="" account="A000002" bankid=""/>
      </SPLITS>
    </TRANSACTION>
    <TRANSACTION id="T000000000000000002" postdate="2020-01-04" memo="Invoice 4711 for the garden shed, paid in two parts as agreed on the phone. Invoice 4711 for the garden shed, paid in two parts as agreed on the phone. Invoice 4711 for the garden shed, paid in two parts as agreed on the phone. Invoice 4711 for the garden shed, paid in two parts as agreed on the phone." commodity="EUR">
      <SPLITS>
        <SPLIT id="S0001" payee="" reconcileflag="0" value="-1000/100" shares="-1000/100" memo="" account="A000001" bankid=""/>
        <SPLIT id="S0002" payee="" reconcileflag="0" value="1000/100" shares="1000/100" memo="" account="A000002" bankid=""/>
      </SPLITS>
    </TRANSACTION>
    <TRANSACTION id="T000000000000000003" postdate="2020-01-05" memo="LONGER_MEMO" commodity="EUR">
      <SPLITS>
        <SPLIT id="S0001" payee="" reconcileflag="0" value="-500/100" shares="-500/100" memo="" account="A000001" bankid=""/>
        <SPLIT id="S0002" payee="" reconcileflag="0" value="500/100" shares="500/100" memo="" account="A000002" bankid=""/>
      </SPLITS>
    </TRANSACTION>
  </TRANSACTIONS>
</KMYMONEY-FILE>
"#;

    /// The book the importer and the pipeline make from [`MEMO_NOTES_KMY`]
    /// with a memo of 600 characters (AGENTS.md §9.4), and its Checking
    /// account's ID as text.
    fn book_with_imported_memos() -> (Book, String) {
        use kassensturz_import::{Imported, Kmy, Limits, import};
        let file = MEMO_NOTES_KMY.replace("LONGER_MEMO", &"Garden shed. ".repeat(47)[..600]);
        let Imported::Book(imported) =
            import(&Kmy, file.as_bytes(), &Limits::default()).expect("the file reads")
        else {
            panic!("a book");
        };
        let mut book = Book::create_in_memory().expect("a book in memory");
        kassensturz_pipeline::fill_book_from(
            &mut book,
            &imported,
            &std::collections::HashMap::new(),
        )
        .expect("filled");
        let checking = book
            .accounts(false)
            .expect("accounts")
            .into_iter()
            .find(|account| account.name == "Checking")
            .expect("the Checking account")
            .id
            .to_string();
        (book, checking)
    }

    /// The Checking account's transaction on `date` (ISO), its ID as text.
    fn imported_on(book: &Book, checking: &str, date: &str) -> String {
        let id = find_account(book, checking).expect("account").id;
        book.register(id)
            .expect("register")
            .into_iter()
            .find(|entry| entry.booking_date.to_string() == date)
            .map(|entry| entry.transaction.to_string())
            .unwrap_or_else(|| panic!("a transaction on {date}"))
    }

    // Fable review F-1 (M6-B219, #686): saving an imported transaction
    // with a long or multi-line memo must never cut or block it (D-703).
    #[test]
    fn an_imported_long_or_multi_line_note_stays_whole_when_the_entry_is_saved() {
        let (mut book, checking) = book_with_imported_memos();
        let two_lines = imported_on(&book, &checking, "2020-01-03");
        let long = imported_on(&book, &checking, "2020-01-04");
        let longer = imported_on(&book, &checking, "2020-01-05");
        let groceries = details(&book, &checking, &two_lines, &TestLocale)
            .expect("details")
            .category;
        let memo = "Line one of the memo\nline two of the memo";
        let long_memo = details(&book, &checking, &long, &TestLocale)
            .expect("details")
            .note;
        assert_eq!(long_memo.chars().count(), 303);
        let longer_memo = details(&book, &checking, &longer, &TestLocale)
            .expect("details")
            .note;
        assert_eq!(longer_memo.chars().count(), 600);
        let notes = [
            (&two_lines, memo),
            (&long, long_memo.as_str()),
            (&longer, longer_memo.as_str()),
        ];

        // The page shows both notes whole. A save that leaves the note
        // field alone sends no note (TransactionPage.qml) and keeps both.
        for (id, note) in notes {
            let form = details(&book, &checking, id, &TestLocale).expect("details");
            assert_eq!(form.note, note);
            edit(
                &mut book,
                id,
                &NewEntry {
                    date: &form.date,
                    ..paid(&checking, &form.amount, "Corner Grocery", "")
                },
                &TestLocale,
            )
            .expect("saved without the note");
            let saved = details(&book, &checking, id, &TestLocale).expect("details");
            assert_eq!((saved.note.as_str(), saved.category.as_str()), (note, ""));
        }

        // Sent unchanged, as a field that shows the whole note sends it,
        // both are taken as they are.
        for (id, note) in notes {
            let form = details(&book, &checking, id, &TestLocale).expect("details");
            edit(
                &mut book,
                id,
                &NewEntry {
                    date: &form.date,
                    note: Some(note),
                    ..paid(&checking, &form.amount, "", &groceries)
                },
                &TestLocale,
            )
            .expect("saved with the note");
            let saved = details(&book, &checking, id, &TestLocale).expect("details");
            assert_eq!(saved.note, note);
        }

        // A note longer than the page takes may be changed at its own
        // length, not beyond it, and the message names that length.
        let changed = longer_memo.replacen("Garden", "Golden", 1);
        let form = details(&book, &checking, &longer, &TestLocale).expect("details");
        let entry = |note| NewEntry {
            date: &form.date,
            note: Some(note),
            ..paid(&checking, &form.amount, "", &groceries)
        };
        edit(&mut book, &longer, &entry(&changed), &TestLocale).expect("changed");
        let too_long = format!("{}!", longer_memo);
        let error = edit(&mut book, &longer, &entry(&too_long), &TestLocale).expect_err("refused");
        let text = error_text(&book, &checking, &error, &TestLocale);
        assert_eq!(
            (text.code, text.params),
            ("input.note-invalid", vec![("max", "600".to_owned())])
        );
        let saved = details(&book, &checking, &longer, &TestLocale).expect("details");
        assert_eq!(saved.note, changed);
    }
}
