// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's import pipeline: bank statements into one account of a
//! book (D-030, D-092).
//!
//! An importer (`kassensturz-import`) reads a file into [`Statement`]s
//! without ever seeing a book. This crate puts them into the account the
//! user chose, as one user action that undo takes back as a whole (D-086),
//! in the order of D-030:
//!
//! 1. Entries the bank hasn't booked yet are left out.
//! 2. The duplicate check (Q-039, D-083): an entry whose bank ID the account
//!    already holds, or that an earlier entry of the statements has, is
//!    skipped. One that looks like a transaction the account already has is
//!    left out until the user chooses to import it ([`Choices`]): the same
//!    amount, at most three business days apart, unless both have bank IDs
//!    and they differ (D-092). Transactions typed in by hand count too.
//! 3. Every other entry becomes a transaction: the account's part, with
//!    everything the bank delivered (D-067), and one part without a
//!    category (D-083). Its payee is the other party's name, which the user
//!    can change.
//! 4. The Transfers to suggest that involve a new transaction (D-087). They
//!    are only suggestions: the user confirms each one.
//! 5. The Rules, on the new transactions only (D-088).
//! 6. For each statement with a closing balance, KassenSturz's balance of
//!    the account on that day against the bank's (D-029).
//!
//! [`preview_import`] runs exactly these steps and takes them back, so the
//! user sees what an import would do, Rules and Transfers included, before
//! [`import_statements`] does it. Every front end calls these two (the
//! command-line tool now, the app from M3, bank connections from M4), so
//! none of them has to repeat the order.
//!
//! The other way round, [`export_transactions`] gathers a book's transactions
//! for an exporter (`kassensturz-export`), so no front end has to repeat how
//! a row's category or Transfer is found (D-099). [`export_book`] gathers a
//! whole book for the complete export, and [`fill_book`] fills a new book
//! with one read back (D-029, D-046).
//!
//! This crate depends on `kassensturz-core`, `kassensturz-import`,
//! `kassensturz-export` and `kassensturz-store`, which may not depend on
//! each other (`docs/ARCHITECTURE.md`).

#![forbid(unsafe_code)]

mod book;
mod entries;
mod error;
mod export;
mod report;

use std::collections::HashSet;

use kassensturz_core::{AccountId, ImportedDetails, Money, Split, Transaction, TransactionId};
use kassensturz_import::{Entry, ImportError, Statement};
use kassensturz_store::{Action, Book, RuleRun, StoreError};

use entries::{Decision, bank_date, decide};

pub use book::{export_book, fill_book};
pub use error::PipelineError;
pub use export::export_transactions;
pub use report::{
    BalanceCheck, Duplicate, EntryOutcome, EntryRef, EntryResult, ImportReport, Summary,
};

/// What the user decided after a preview.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choices {
    /// The entries that look like a transaction the account already has
    /// ([`EntryOutcome::Similar`]) and that the user wants imported anyway.
    /// Naming any other entry here changes nothing: a duplicate is still
    /// skipped, and so is an entry the bank hasn't booked.
    pub import_similar: HashSet<EntryRef>,
}

/// What [`import_statements`] would do with the same arguments, without
/// changing the book: the import runs, Rules and Transfers included, and is
/// taken back (`Book::dry_run`). Nothing is recorded in the history.
///
/// The IDs of the new transactions in the report don't exist in the book
/// afterwards; the real import gives them new ones.
pub fn preview_import(
    book: &mut Book,
    account: AccountId,
    statements: &[Statement],
    choices: &Choices,
) -> Result<ImportReport, PipelineError> {
    book.dry_run(|book| run(book, account, statements, choices))
}

/// Imports `statements` into `account`, as one user action that undo takes
/// back as a whole (D-086), and reports what it did. The steps are those of
/// the crate's description, in that order.
///
/// `account` must be one of the user's accounts, not a category, and open,
/// and every statement must be in its currency, with the decimal places the
/// book stores for it (D-027); otherwise nothing is imported. A statement in
/// a withdrawn currency such as the Mark is refused until M2.9 converts it
/// (D-077). If anything fails, the book stays as it was.
pub fn import_statements(
    book: &mut Book,
    account: AccountId,
    statements: &[Statement],
    choices: &Choices,
) -> Result<ImportReport, PipelineError> {
    book.as_one_action(Action::ImportStatement, |book| {
        run(book, account, statements, choices)
    })
}

/// The steps of the pipeline, inside a user action or a dry run.
fn run(
    book: &mut Book,
    account: AccountId,
    statements: &[Statement],
    choices: &Choices,
) -> Result<ImportReport, PipelineError> {
    let account = book.account(account)?;
    if !account.kind.is_account() {
        return Err(StoreError::NotAnAccount(account.id).into());
    }
    if account.closed {
        return Err(StoreError::AccountClosed(account.id).into());
    }
    let digits = book.currency_minor_digits(account.currency)?;
    for statement in statements {
        let currency = statement.currency;
        if currency.code() != account.currency {
            return Err(ImportError::AccountCurrencyDiffers {
                account: account.id,
                account_currency: account.currency,
                statement_currency: currency.code(),
            }
            .into());
        }
        if currency.minor_digits() != digits {
            return Err(ImportError::DecimalPlacesDiffer {
                currency: account.currency,
                book: digits,
                file: currency.minor_digits(),
            }
            .into());
        }
    }

    let held = book.account_parts(account.id)?;
    let mut entries = Vec::new();
    let mut added = Vec::new();
    for (at, decision) in decide(statements, &held, choices) {
        let outcome = match decision {
            Decision::Import { similar } => {
                let entry = &statements[at.statement].entries[at.entry];
                let transaction = transaction_for(account.id, entry)?;
                book.add_transaction(&transaction)?;
                added.push(transaction.id());
                EntryOutcome::Imported {
                    transaction: transaction.id(),
                    similar,
                }
            }
            Decision::LeaveOut(outcome) => outcome,
        };
        entries.push(EntryResult { entry: at, outcome });
    }

    let (transfers, rules) = if added.is_empty() {
        (Vec::new(), RuleRun::default())
    } else {
        let new: HashSet<TransactionId> = added.iter().copied().collect();
        let transfers = book
            .transfer_suggestions()?
            .into_iter()
            .filter(|found| {
                new.contains(&found.outgoing.transaction)
                    || new.contains(&found.incoming.transaction)
            })
            .collect();
        (transfers, book.run_rules_on(&added)?)
    };

    let parts = book.account_parts(account.id)?;
    let mut balances = Vec::new();
    for (index, statement) in statements.iter().enumerate() {
        let Some(closing) = statement.closing_balance else {
            continue;
        };
        let booked = parts
            .iter()
            .filter(|part| bank_date(part) <= closing.date)
            .map(|part| part.amount);
        let balance =
            Money::checked_sum(account.currency, booked).map_err(|_| StoreError::AmountOverflow)?;
        balances.push(BalanceCheck {
            statement: index,
            date: closing.date,
            bank: closing.amount,
            book: balance,
        });
    }

    Ok(ImportReport {
        entries,
        transfers,
        rules,
        balances,
    })
}

/// The transaction an entry becomes: the account's part, with everything
/// the bank delivered, and one part without a category (D-083).
fn transaction_for(account: AccountId, entry: &Entry) -> Result<Transaction, PipelineError> {
    let delivered = ImportedDetails {
        bank_id: entry.bank_id.clone(),
        booking_date: Some(entry.booking_date),
        value_date: entry.value_date,
        other_party_name: entry.other_party_name.clone(),
        other_party_iban: entry.other_party_iban.clone(),
        remittance_information: entry.remittance_information.clone(),
    };
    let own = Split {
        imported: Some(delivered),
        ..Split::new(account, entry.amount)
    };
    let open = entry
        .amount
        .checked_neg()
        .map_err(|_| StoreError::AmountOverflow)?;
    let transaction = Transaction::new(
        TransactionId::generate(),
        entry.booking_date,
        entry.other_party_name.clone(),
        vec![own, Split::uncategorized(open)],
    )
    // Two parts in one currency that cancel out, the first with an account.
    .expect("an entry and its opposite form a transaction");
    Ok(transaction)
}
