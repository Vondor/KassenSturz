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
use kassensturz_store::{Book, StoreError};

use crate::errors::{ErrorText, amount_example, report_text};
use crate::format::{Affixes, AmountFormat, Locale};
use crate::parse::{AmountInputError, AmountReader};
use crate::register::{EntryKind, kind_of};

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
}

impl From<StoreError> for EntryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

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
    /// Whether the account's part came from a bank or a file, so its amount
    /// stays as delivered (D-067).
    pub imported: bool,
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

/// Adds the transaction the user entered.
pub fn add(
    book: &mut Book,
    entered: &NewEntry<'_>,
    locale: &impl Locale,
) -> Result<(), EntryError> {
    let account = find_account(book, entered.account)?;
    let category = find_category(book, &account, entered.category)?;
    let (into_account, into_other) = read_amounts(book, &account, entered, locale)?;
    let date = locale.read_date(entered.date).ok_or(EntryError::Date)?;
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
    .map_err(EntryError::Transaction)?;
    book.add_transaction(&transaction)?;
    Ok(())
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
    // The account's own date and payee, as the register shows them (D-108).
    let date = transaction.date_of(own_split);
    let other_party = transaction.payee_of(own_split).unwrap_or_default();
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
        imported: own.iter().any(|split| split.imported.is_some()),
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
        .map_err(EntryError::Transaction)?;
    book.update_transaction(&transaction)?;
    Ok(())
}

/// Removes the transaction with `id` (as text) from the book, whatever its
/// kind. Undo brings it back (D-086).
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

    fn code(book: &Book, account: &str, result: Result<(), EntryError>) -> &'static str {
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
                "|1000,00 €|1000,00 €",
                "|-12,00 €|844,90 €",
                "Corner Grocery|-53,20 €|946,80 €",
                "example shoes|-89,90 €|856,90 €"
            ]
        );
        // Ignoring case; a tie keeps the date order.
        assert_eq!(
            sorted(SortKey::OtherParty, false),
            [
                "|1000,00 €|1000,00 €",
                "|-12,00 €|844,90 €",
                "Corner Grocery|-53,20 €|946,80 €",
                "example shoes|-89,90 €|856,90 €"
            ]
        );
        assert_eq!(
            sorted(SortKey::Category, true),
            [
                "Corner Grocery|-53,20 €|946,80 €",
                "example shoes|-89,90 €|856,90 €",
                "|1000,00 €|1000,00 €",
                "|-12,00 €|844,90 €"
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
                imported: false,
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
}
