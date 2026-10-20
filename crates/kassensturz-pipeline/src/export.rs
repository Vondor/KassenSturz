// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gathering a book's transactions for an exporter (D-099).

use std::collections::HashMap;

use kassensturz_core::{AccountId, AccountKind};
use kassensturz_export::{AccountTransactions, Counterpart, ExportData, TransactionRow};
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
/// booked to the account (D-067).
pub fn export_transactions(book: &Book, accounts: &[AccountId]) -> Result<ExportData, StoreError> {
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
                amount: entry.amount,
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
