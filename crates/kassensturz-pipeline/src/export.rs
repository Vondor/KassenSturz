// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gathering a book's transactions for an exporter (D-099).

use std::collections::HashMap;

use kassensturz_core::{AccountId, AccountKind, CurrencyCode, Date, Money, Split};
use kassensturz_export::{
    AccountTransactions, Counterpart, ExportData, OriginalRow, TransactionRow,
};
use kassensturz_store::{Book, StoreError};

/// The transactions of `accounts` as an exporter takes them, in the order
/// given: each account's register, oldest first, every row as the account
/// sees it, in the account's currency (`Book::register`). Accounts and
/// categories both can be named. Nothing in the book changes.
///
/// A row's counterpart is the category or the other account when the
/// transaction has exactly one other part, [`Counterpart::Parts`] when it has
/// several, and [`Counterpart::Uncategorized`] when its other part has no
/// category yet. The memo is the purpose a bank delivered for the part
/// booked to the account (D-067); the note is the user's own (B-257).
pub fn export_transactions(book: &Book, accounts: &[AccountId]) -> Result<ExportData, StoreError> {
    export_transactions_in(book, accounts, DateRange::default())
}

/// The days an export is limited to, both ends included. An end left open
/// reaches as far as the book does, so the default holds every day (#901).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateRange {
    /// The first day, or `None` for everything from the start.
    pub from: Option<Date>,
    /// The last day, or `None` for everything up to the end.
    pub until: Option<Date>,
}

impl DateRange {
    /// Whether `day` is in the range. A range whose end lies before its
    /// start holds no day.
    pub fn contains(&self, day: Date) -> bool {
        self.from.is_none_or(|from| day >= from) && self.until.is_none_or(|until| day <= until)
    }

    /// Whether the last day lies before the first.
    pub fn is_reversed(&self) -> bool {
        matches!((self.from, self.until), (Some(from), Some(until)) if until < from)
    }
}

/// Like [`export_transactions`], but only the transactions whose booking
/// date in each account is in `range`. Rows are kept as they are: the
/// amounts add up to an account's balance only when nothing before the
/// range's start is left out. A Transfer is one row in each of its two
/// accounts, each by that account's own date, so a Transfer whose sides
/// fall on different days can be in one file and not in the other.
pub fn export_transactions_in(
    book: &Book,
    accounts: &[AccountId],
    range: DateRange,
) -> Result<ExportData, StoreError> {
    let paths: HashMap<AccountId, String> = book
        .categories()?
        .into_iter()
        .map(|entry| (entry.category.id, entry.path.join("/")))
        .collect();
    let mut exported = Vec::with_capacity(accounts.len());
    for &id in accounts {
        let account = book.account(id)?;
        let mut rows = Vec::new();
        for entry in book.register(id)? {
            if !range.contains(entry.booking_date) {
                continue;
            }
            let transaction = book.transaction(entry.transaction)?;
            let (own, others): (Vec<_>, Vec<_>) = transaction
                .splits()
                .iter()
                .partition(|split| split.account == Some(id));
            let memo = own.iter().find_map(|split| {
                split
                    .imported
                    .as_ref()
                    .and_then(|imported| imported.remittance_information.clone())
            });
            let original = original_of(book, &account.currency, &own)?;
            let counterpart = match others.as_slice() {
                [other] => match other.account {
                    Some(other) => counterpart_of(book, &paths, other)?,
                    None => Counterpart::Uncategorized,
                },
                [] => Counterpart::Uncategorized,
                several => Counterpart::Parts(several.len()),
            };
            rows.push(TransactionRow {
                date: entry.booking_date,
                payee: entry.payee,
                counterpart,
                memo,
                note: transaction.note().map(str::to_owned),
                amount: entry.amount,
                original,
            });
        }
        exported.push(AccountTransactions {
            account: account.name,
            currency: account.currency,
            minor_digits: book.currency_minor_digits(account.currency)?,
            rows,
        });
    }
    Ok(ExportData::Transactions(exported))
}

/// What the part or parts booked to the account were before they were
/// converted from a withdrawn currency (D-104): only if every one of them was,
/// from the same currency into the account's, so the original adds up to the
/// row's amount.
fn original_of(
    book: &Book,
    account_currency: &CurrencyCode,
    own: &[&Split],
) -> Result<Option<OriginalRow>, StoreError> {
    let mut total: Option<Money> = None;
    for split in own {
        let Some(original) = split
            .original
            .filter(|original| original.rate.from == *account_currency)
        else {
            return Ok(None);
        };
        total = match total {
            None => Some(original.amount),
            Some(sum) => match sum.checked_add(original.amount) {
                Ok(sum) => Some(sum),
                // Another currency, or too large: leave the original out
                // rather than write a wrong one.
                Err(_) => return Ok(None),
            },
        };
    }
    total
        .map(|amount| {
            Ok(OriginalRow {
                amount,
                minor_digits: book.currency_minor_digits(amount.currency())?,
            })
        })
        .transpose()
}

fn counterpart_of(
    book: &Book,
    paths: &HashMap<AccountId, String>,
    id: AccountId,
) -> Result<Counterpart, StoreError> {
    let other = book.account(id)?;
    Ok(match other.kind {
        AccountKind::Income | AccountKind::Expense => {
            Counterpart::Category(paths.get(&id).cloned().unwrap_or(other.name))
        }
        AccountKind::Asset | AccountKind::Liability => Counterpart::Transfer(other.name),
        // The internal balancing account is never shown (D-067).
        AccountKind::Equity => Counterpart::Uncategorized,
    })
}
