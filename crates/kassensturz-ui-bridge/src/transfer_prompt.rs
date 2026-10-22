// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The prompt for a suggested Transfer (M3.7, D-030, D-087, D-120): two
//! transactions that look like one move of money between the user's own
//! accounts, shown for the user to confirm or to leave apart.
//!
//! The core finds the suggestions (`Book::transfer_suggestions`) and joins a
//! confirmed pair (`Book::join_transfer`); this module only writes what the
//! prompt shows as text (D-065) and refuses a pair the book no longer
//! suggests. The prompt is used after a transaction the user typed, and by
//! the import (M3.10) for the Transfers its statements suggest.

use kassensturz_core::{
    Account, AccountId, Report, Split, Transaction, TransferMatch, TransferSide,
};
use kassensturz_store::{Book, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::register::{AmountWriter, Shown};

/// Why a Transfer couldn't be shown or joined.
#[derive(Debug)]
pub enum TransferPromptError {
    /// The pair isn't suggested (any more), e.g. one of the transactions was
    /// changed or removed since the prompt was shown.
    NoSuchSuggestion,
    /// The book refused it.
    Store(StoreError),
}

impl From<StoreError> for TransferPromptError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One side of a suggested Transfer, as the prompt shows it.
#[derive(Debug, PartialEq, Eq)]
pub struct SideText {
    /// The transaction's ID as text.
    pub id: String,
    /// The name of the account the money left or arrived in.
    pub account: String,
    /// The date its account shows (D-108), in the user's format.
    pub date: String,
    /// The other party its account shows; may be empty.
    pub other_party: String,
}

/// A suggested Transfer, as the prompt shows it: the money that left one
/// account and arrived in another.
#[derive(Debug, PartialEq, Eq)]
pub struct SuggestionText {
    /// The side the money left from.
    pub outgoing: SideText,
    /// The side the money arrived in.
    pub incoming: SideText,
    /// How much moved, without a sign.
    pub amount: Shown,
    /// How many business days lie between the two dates; 0 for the same day.
    pub business_days: u32,
}

/// The Transfers to ask about, in date order (D-087): those that involve the
/// transaction with `involving` (as text), or all of them if it is empty.
pub fn suggestions(
    book: &Book,
    involving: &str,
    locale: &impl Locale,
) -> Result<Vec<SuggestionText>, TransferPromptError> {
    let accounts = book.all_accounts()?;
    book.transfer_suggestions()?
        .iter()
        .filter(|found| {
            involving.is_empty()
                || found.outgoing.transaction.to_string() == involving
                || found.incoming.transaction.to_string() == involving
        })
        .map(|found| suggestion_text(book, &accounts, found, locale))
        .collect()
}

/// Joins the two transactions of a suggested Transfer, the way the user
/// confirmed it. A pair the book doesn't suggest (any more) is refused, so a
/// prompt that went stale can't join the wrong transactions.
pub fn join(book: &mut Book, outgoing: &str, incoming: &str) -> Result<(), TransferPromptError> {
    let found = book
        .transfer_suggestions()?
        .into_iter()
        .find(|found| {
            found.outgoing.transaction.to_string() == outgoing
                && found.incoming.transaction.to_string() == incoming
        })
        .ok_or(TransferPromptError::NoSuchSuggestion)?;
    book.join_transfer(found.outgoing.transaction, found.incoming.transaction)?;
    Ok(())
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &TransferPromptError, locale: &impl Locale) -> ErrorText {
    match error {
        TransferPromptError::NoSuchSuggestion => ErrorText::plain("input.no-such-entry"),
        TransferPromptError::Store(error) => report_text(book, &error.report(), locale),
    }
}

fn suggestion_text(
    book: &Book,
    accounts: &[Account],
    found: &TransferMatch,
    locale: &impl Locale,
) -> Result<SuggestionText, TransferPromptError> {
    // Both sides are the same size, and the incoming one is positive.
    let amount = found.incoming.amount;
    Ok(SuggestionText {
        outgoing: side_text(book, accounts, &found.outgoing, locale)?,
        incoming: side_text(book, accounts, &found.incoming, locale)?,
        amount: AmountWriter::new(book, amount.currency(), locale)?.text(amount),
        business_days: found.business_days,
    })
}

fn side_text(
    book: &Book,
    accounts: &[Account],
    side: &TransferSide,
    locale: &impl Locale,
) -> Result<SideText, TransferPromptError> {
    let transaction = book.transaction(side.transaction)?;
    let booked = booked_part(&transaction, side.account);
    Ok(SideText {
        id: side.transaction.to_string(),
        account: accounts
            .iter()
            .find(|account| account.id == side.account)
            .map(|account| account.name.clone())
            .unwrap_or_default(),
        date: locale.short_date(side.booking_date),
        other_party: booked
            .and_then(|part| transaction.payee_of(part))
            .unwrap_or_default()
            .to_owned(),
    })
}

/// The part of `transaction` that books to `account`.
fn booked_part(transaction: &Transaction, account: AccountId) -> Option<&Split> {
    transaction
        .splits()
        .iter()
        .find(|part| part.account == Some(account))
}

#[cfg(test)]
mod tests {
    use kassensturz_core::AccountType;

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::register::{Sort, register_view};
    use crate::transaction_entry::{self, NewEntry};

    /// A book after the first start with a checking account holding 1000 €
    /// and a savings account holding 500 €, made by M3.3's `add_account`.
    fn book_with_two_accounts() -> (Book, String, String) {
        let mut book = started();
        let mut account = |name: &str, account_type, balance: &str| {
            add_account(
                &mut book,
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
        };
        let checking = account("Checking account", AccountType::Checking, "1000");
        let savings = account("Savings", AccountType::Savings, "500");
        (book, checking, savings)
    }

    /// Types a transaction the way the register's page does, and returns its
    /// ID (M3.5's real output).
    fn typed(
        book: &mut Book,
        account: &str,
        money_in: bool,
        amount: &str,
        other_party: &str,
        category: &str,
        date: &str,
    ) -> String {
        transaction_entry::add(
            book,
            &NewEntry {
                account,
                money_in,
                amount,
                other_party,
                category,
                date,
            },
            &TestLocale,
        )
        .expect("transaction added")
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

    fn balance_of(book: &Book, account: &str) -> String {
        let id = transaction_entry::find_account(book, account)
            .expect("account")
            .id;
        register_view(book, id, Sort::default(), &TestLocale)
            .expect("register")
            .balance
            .text
    }

    #[test]
    fn suggests_the_two_sides_of_a_move_the_user_typed() {
        let (mut book, checking, savings) = book_with_two_accounts();
        let out = typed(
            &mut book,
            &checking,
            false,
            "100",
            "My savings",
            "",
            "2026-10-05",
        );
        // Nothing to join it with yet.
        assert_eq!(suggestions(&book, &out, &TestLocale).expect("none"), []);
        let arrived = typed(&mut book, &savings, true, "100", "", "", "2026-10-06");
        let found = suggestions(&book, &arrived, &TestLocale).expect("suggested");
        assert_eq!(
            found,
            [SuggestionText {
                outgoing: SideText {
                    id: out.clone(),
                    account: "Checking account".into(),
                    date: "2026-10-05".into(),
                    other_party: "My savings".into(),
                },
                incoming: SideText {
                    id: arrived.clone(),
                    account: "Savings".into(),
                    date: "2026-10-06".into(),
                    other_party: String::new(),
                },
                amount: Shown {
                    text: "100,00 €".into(),
                    exact: "100,00 €".into(),
                },
                business_days: 1,
            }]
        );
        // Asking for either side, or for all, finds it; another transaction
        // doesn't.
        assert_eq!(suggestions(&book, &out, &TestLocale).expect("out"), found);
        assert_eq!(suggestions(&book, "", &TestLocale).expect("all"), found);
        let other = typed(
            &mut book,
            &checking,
            false,
            "7,50",
            "Bakery",
            "",
            "2026-10-06",
        );
        assert_eq!(suggestions(&book, &other, &TestLocale).expect("none"), []);
    }

    #[test]
    fn a_side_with_a_category_is_not_asked_about() {
        let (mut book, checking, savings) = book_with_two_accounts();
        let groceries = category_id(&book, "Food › Groceries");
        typed(
            &mut book,
            &checking,
            false,
            "100",
            "",
            &groceries,
            "2026-10-05",
        );
        let arrived = typed(&mut book, &savings, true, "100", "", "", "2026-10-05");
        assert_eq!(suggestions(&book, &arrived, &TestLocale).expect("none"), []);
    }

    #[test]
    fn joining_keeps_both_registers_as_their_banks_booked_them() {
        let (mut book, checking, savings) = book_with_two_accounts();
        let out = typed(
            &mut book,
            &checking,
            false,
            "100",
            "My savings",
            "",
            "2026-10-05",
        );
        let arrived = typed(
            &mut book,
            &savings,
            true,
            "100",
            "From checking",
            "",
            "2026-10-07",
        );
        join(&mut book, &out, &arrived).expect("joined");
        // One Transfer now, seen from both accounts (D-108): each keeps the
        // date and party its bank showed, and neither balance changed.
        let checking_view = register_view(
            &book,
            transaction_entry::find_account(&book, &checking)
                .expect("checking")
                .id,
            Sort::default(),
            &TestLocale,
        )
        .expect("register");
        let savings_view = register_view(
            &book,
            transaction_entry::find_account(&book, &savings)
                .expect("savings")
                .id,
            Sort::default(),
            &TestLocale,
        )
        .expect("register");
        let summary = |view: &crate::register::RegisterView| {
            view.rows
                .iter()
                .filter(|row| !row.starting_balance)
                .map(|row| {
                    (
                        row.date.clone(),
                        row.other_party.clone(),
                        row.category.clone(),
                        row.kind,
                        row.amount.text.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            summary(&checking_view),
            [(
                "2026-10-05".into(),
                "My savings".into(),
                "Savings".into(),
                "transfer",
                "-100,00 €".into()
            )]
        );
        assert_eq!(
            summary(&savings_view),
            [(
                "2026-10-07".into(),
                "From checking".into(),
                "Checking account".into(),
                "transfer",
                "100,00 €".into()
            )]
        );
        assert_eq!(checking_view.balance.text, "900,00 €");
        assert_eq!(savings_view.balance.text, "600,00 €");
        // Nothing is left to ask about, and undo takes the join back whole.
        assert_eq!(suggestions(&book, "", &TestLocale).expect("none"), []);
        book.undo().expect("undone");
        assert_eq!(suggestions(&book, "", &TestLocale).expect("again").len(), 1);
        assert_eq!(balance_of(&book, &checking), "900,00 €");
        assert_eq!(balance_of(&book, &savings), "600,00 €");
    }

    #[test]
    fn a_pair_the_book_no_longer_suggests_is_refused() {
        let (mut book, checking, savings) = book_with_two_accounts();
        let out = typed(&mut book, &checking, false, "100", "", "", "2026-10-05");
        let arrived = typed(&mut book, &savings, true, "100", "", "", "2026-10-05");
        // The user sorts the outgoing side while the prompt is open.
        let groceries = category_id(&book, "Food › Groceries");
        transaction_entry::edit(
            &mut book,
            &out,
            &NewEntry {
                account: &checking,
                money_in: false,
                amount: "100",
                other_party: "",
                category: &groceries,
                date: "2026-10-05",
            },
            &TestLocale,
        )
        .expect("edited");
        let error = join(&mut book, &out, &arrived).expect_err("refused");
        assert!(matches!(error, TransferPromptError::NoSuchSuggestion));
        assert_eq!(
            error_text(&book, &error, &TestLocale),
            ErrorText::plain("input.no-such-entry")
        );
        // The wrong order is refused too: the money left from `out`.
        let (mut book, checking, savings) = book_with_two_accounts();
        let out = typed(&mut book, &checking, false, "100", "", "", "2026-10-05");
        let arrived = typed(&mut book, &savings, true, "100", "", "", "2026-10-05");
        assert!(join(&mut book, &arrived, &out).is_err());
        join(&mut book, &out, &arrived).expect("joined");
    }
}
