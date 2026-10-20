// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an export is made from: plain data, independent of the book and of
//! the file format.

use kassensturz_core::{CurrencyCode, Date, Money};

/// What an exporter writes. The caller gathers it from a book.
///
/// The complete export of a book (M2.8) adds a variant. Code that handles
/// one variant has to say what it does with the others, so a new one is
/// never ignored silently.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExportData {
    /// The transactions of one or more accounts, one list per account.
    Transactions(Vec<AccountTransactions>),
}

/// One account's transactions, as its register shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountTransactions {
    /// The account's name, as the user gave it.
    pub account: String,
    /// The account's currency. Every row's amount is in it.
    pub currency: CurrencyCode,
    /// The decimal places the book stores for the currency (D-027), so
    /// amounts are written exactly as stored.
    pub minor_digits: u8,
    /// The transactions, oldest first.
    pub rows: Vec<TransactionRow>,
}

/// One transaction as seen from one account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionRow {
    /// The booking date.
    pub date: Date,
    /// Who the money came from or went to.
    pub payee: Option<String>,
    /// What the other side of the transaction is.
    pub counterpart: Counterpart,
    /// What the bank or file delivered as the purpose, if the transaction
    /// came from one (D-067).
    pub memo: Option<String>,
    /// What the transaction did to the account, in the account's currency:
    /// positive amounts came in, negative ones went out. The amounts of an
    /// account's rows add up to its balance.
    pub amount: Money,
}

/// What the other side of a transaction, seen from an account, is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Counterpart {
    /// The transaction has no category yet (D-083).
    Uncategorized,
    /// One category, named by its path from the top-level category, e.g.
    /// `Food/Groceries`.
    Category(String),
    /// A Transfer to or from one other account of the user's, by its name.
    Transfer(String),
    /// Several other parts, so no single category: how many there are, at
    /// least two. The complete export has them one by one.
    Parts(usize),
}

/// The most decimal places a currency can have here: 10^18 is the largest
/// power of ten a 64-bit amount can be divided into.
const MAX_MINOR_DIGITS: u8 = 18;

impl ExportData {
    /// Fails if the data contradicts itself: a row in another currency than
    /// its account, or more decimal places than an amount can have.
    /// [`export`](crate::export) and [`export_file`](crate::export_file)
    /// call it first, so nothing is written for data that can't be written
    /// exactly (AGENTS.md §2).
    pub fn check(&self) -> Result<(), crate::ExportError> {
        match self {
            Self::Transactions(accounts) => {
                for account in accounts {
                    if account.minor_digits > MAX_MINOR_DIGITS {
                        return Err(crate::ExportError::BadDecimalPlaces {
                            currency: account.currency,
                            digits: account.minor_digits,
                        });
                    }
                    if let Some(row) = account
                        .rows
                        .iter()
                        .find(|row| row.amount.currency() != account.currency)
                    {
                        return Err(crate::ExportError::CurrencyMismatch {
                            expected: account.currency,
                            found: row.amount.currency(),
                        });
                    }
                }
                Ok(())
            }
        }
    }
}
