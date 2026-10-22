// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An account's register, ready for display: every amount and date is
//! already text in the user's regional format (D-065).

use kassensturz_core::{AccountId, AccountKind, CurrencyCode, Date, Money, TransactionId};
use kassensturz_store::{Book, StoreError};

use crate::format::{AmountFormat, Locale};

/// Writes amounts of one currency in the user's regional format.
pub struct AmountWriter<'a, L> {
    locale: &'a L,
    format: AmountFormat,
    minor_digits: u8,
}

impl<'a, L: Locale> AmountWriter<'a, L> {
    /// For amounts in `currency`, with the decimal places `book` stores for
    /// it (D-027).
    pub fn new(book: &Book, currency: CurrencyCode, locale: &'a L) -> Result<Self, StoreError> {
        Ok(Self {
            locale,
            format: locale.amount_format(currency),
            minor_digits: book.currency_minor_digits(currency)?,
        })
    }

    /// `amount` as text, e.g. "-53,20 €", with the decimal places the user
    /// chose to see, and exactly.
    pub fn text(&self, amount: Money) -> Shown {
        let group_digits = |value| self.locale.group_digits(value);
        let shown_digits = self.locale.shown_decimal_places(self.minor_digits);
        Shown {
            text: self
                .format
                .format_rounded(amount, self.minor_digits, shown_digits, group_digits),
            exact: self.format.format(amount, self.minor_digits, group_digits),
        }
    }

    /// A balance as text, and whether it is shown as "You owe": a debt reads
    /// "You owe 300,00 €" rather than "-300,00 €" (D-067). Only accounts of
    /// money the user owes, such as credit cards, are shown that way.
    pub fn balance(&self, balance: Money, kind: AccountKind) -> (Shown, bool) {
        let owes = kind == AccountKind::Liability && balance.is_negative();
        match balance.checked_neg() {
            Ok(debt) if owes => (self.text(debt), true),
            _ => (self.text(balance), false),
        }
    }
}

/// An amount as the UI shows it, and exactly. They differ only when the
/// user chose to see fewer decimal places than the currency has; the exact
/// amount then shows on hover (D-002).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// As shown, e.g. "54 €".
    pub text: String,
    /// Exactly, e.g. "53,90 €".
    pub exact: String,
}

/// What the transaction list shows for one account.
#[derive(Debug, PartialEq, Eq)]
pub struct RegisterView {
    /// The account's name.
    pub account_name: String,
    /// Its balance; for a debt, what the user owes.
    pub balance: Shown,
    /// Whether the balance is shown as "You owe" (D-067).
    pub owes: bool,
    /// Its transactions, newest first, like in a banking app.
    pub rows: Vec<RegisterRow>,
}

/// One transaction in the list.
#[derive(Debug, PartialEq, Eq)]
pub struct RegisterRow {
    /// The booking date.
    pub date: String,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// The category, if the transaction has exactly one.
    pub category: String,
    /// The amount, as seen from the account.
    pub amount: Shown,
    /// Whether money came in, shown as "From" rather than "To" (D-034).
    pub incoming: bool,
    /// Whether this is the account's starting balance (`docs/GLOSSARY.md`).
    pub starting_balance: bool,
}

/// One transaction as seen from an account, before it is written as text.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    /// The transaction. Its ID begins with the time it was added (D-067).
    pub transaction: TransactionId,
    /// The booking date.
    pub date: Date,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// The category, if the transaction has exactly one.
    pub category: String,
    /// What it did to the account.
    pub amount: Money,
    /// Whether this is the account's starting balance.
    pub starting_balance: bool,
}

impl Entry {
    /// The entry as text for `writer`'s locale.
    pub fn row(&self, writer: &AmountWriter<'_, impl Locale>) -> RegisterRow {
        RegisterRow {
            date: writer.locale.short_date(self.date),
            other_party: self.other_party.clone(),
            category: self.category.clone(),
            amount: writer.text(self.amount),
            incoming: self.amount.minor_units() > 0,
            starting_balance: self.starting_balance,
        }
    }
}

/// `account`'s transactions, oldest first, with what the other parts of each
/// are booked to.
pub fn entries(book: &Book, account: AccountId) -> Result<Vec<Entry>, StoreError> {
    let mut entries = Vec::new();
    for entry in book.register(account)? {
        let transaction = book.transaction(entry.transaction)?;
        let mut others = transaction
            .splits()
            .iter()
            .filter(|split| split.account != Some(account));
        let (mut category, mut starting_balance) = (String::new(), false);
        if let (Some(other), None) = (others.next(), others.next())
            && let Some(id) = other.account
        {
            let other = book.account(id)?;
            match other.kind {
                AccountKind::Income | AccountKind::Expense => category = other.name,
                AccountKind::Equity => starting_balance = true,
                AccountKind::Asset | AccountKind::Liability => {}
            }
        }
        entries.push(Entry {
            transaction: entry.transaction,
            date: entry.booking_date,
            other_party: entry.payee.unwrap_or_default(),
            category,
            amount: entry.amount,
            starting_balance,
        });
    }
    Ok(entries)
}

/// Reads `account`'s register from `book` and formats it for `locale`.
pub fn register_view(
    book: &Book,
    account: AccountId,
    locale: &impl Locale,
) -> Result<RegisterView, StoreError> {
    let details = book.account(account)?;
    let writer = AmountWriter::new(book, details.currency, locale)?;
    let (balance, owes) = writer.balance(book.balance(account)?, details.kind);
    let rows = entries(book, account)?
        .iter()
        .rev()
        .map(|entry| entry.row(&writer))
        .collect();
    Ok(RegisterView {
        account_name: details.name,
        balance,
        owes,
        rows,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::format::Affixes;

    /// A fixed, German-like format, independent of the machine's settings.
    pub(crate) struct TestLocale;

    impl Locale for TestLocale {
        fn amount_format(&self, _currency: CurrencyCode) -> AmountFormat {
            AmountFormat {
                decimal_point: ",".into(),
                zero_digit: '0',
                positive: Affixes::spaced(String::new(), " €".into()),
                negative: Affixes::spaced("-".into(), " €".into()),
            }
        }

        fn decimal_point(&self) -> String {
            ",".into()
        }

        fn group_digits(&self, value: u64) -> String {
            value.to_string()
        }

        fn group_separator(&self) -> String {
            ".".into()
        }

        fn short_date(&self, date: Date) -> String {
            date.to_string()
        }

        fn read_date(&self, text: &str) -> Option<Date> {
            text.parse().ok()
        }

        fn today(&self) -> Date {
            Date::constant(2026, 10, 19)
        }
    }
}
