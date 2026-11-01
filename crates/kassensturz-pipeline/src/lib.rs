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
mod imported_book;
mod kmy_reports;
mod report;

use std::collections::HashSet;
use std::path::Path;

use kassensturz_core::{
    AccountId, Changeover, Currency, CurrencyCode, DatedRateId, ImportedDetails, Money,
    OriginalAmount, RoundingMode, Split, Transaction, TransactionId, Withdrawn,
};
use kassensturz_import::{Entry, EntryStatus, ImportError, Statement};
use kassensturz_store::{Action, Book, RuleRun, StoreError};

use entries::{Decision, bank_date, decide};

pub use book::{export_book, fill_book};
pub use error::PipelineError;
pub use export::export_transactions;
pub use imported_book::{
    BookImportReport, PatternLeftOut, RuleScope, fill_book_from, fill_book_from_on,
    fill_book_from_with,
};
pub use kmy_reports::{
    FIXED_PERIODS, FixedPeriod, Found, MappedReport, Outcome, ReportBook, ReportLine, map_report,
    map_reports,
};
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
/// book stores for it (D-027); otherwise nothing is imported. A statement
/// may also be in a withdrawn currency that the account's replaced, such as
/// the German mark for an account in euros: its entries are converted at the
/// fixed rate, and each new transaction keeps the amount in the withdrawn
/// currency with its account's part (D-077, D-105). If anything fails, the
/// book stays as it was.
///
/// `file` is the file the statements were read from, if any: the history
/// keeps its name, without the folder, so undo can say which import it
/// takes back (`Book::note_file`, D-109).
pub fn import_statements(
    book: &mut Book,
    account: AccountId,
    statements: &[Statement],
    choices: &Choices,
    file: Option<&Path>,
) -> Result<ImportReport, PipelineError> {
    book.as_one_action(Action::ImportStatement, |book| {
        if let Some(file) = file {
            book.note_file(file)?;
        }
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
    let account_currency = book.currency(account.currency)?;
    let currencies = book.currencies()?;
    let mut withdrawn = Vec::with_capacity(statements.len());
    for statement in statements {
        let currency = statement.currency;
        // A statement from before its currency was replaced by the
        // account's (D-105).
        let replaced = currency
            .code()
            .withdrawn()
            .filter(|replaced| replaced.successor() == account.currency);
        if currency.code() != account.currency && replaced.is_none() {
            let error = match currency.code().withdrawn() {
                Some(withdrawn) => ImportError::ReplacedCurrencyDiffers {
                    account: account.id,
                    account_currency: account.currency,
                    statement_currency: currency.code(),
                    successor: withdrawn.successor(),
                },
                None => ImportError::AccountCurrencyDiffers {
                    account: account.id,
                    account_currency: account.currency,
                    statement_currency: currency.code(),
                },
            };
            return Err(error.into());
        }
        // The decimal places the book stores, or for a withdrawn currency
        // it doesn't list yet, those of KassenSturz's table.
        let digits = currencies
            .iter()
            .find(|known| known.code() == currency.code())
            .map_or_else(
                || replaced.map(|replaced| replaced.currency().minor_digits()),
                |known| Some(known.minor_digits()),
            );
        if digits != Some(currency.minor_digits()) {
            return Err(ImportError::DecimalPlacesDiffer {
                currency: currency.code(),
                book: digits.unwrap_or(account_currency.minor_digits()),
                file: currency.minor_digits(),
            }
            .into());
        }
        withdrawn.push(replaced);
    }
    let rounding = book.rounding_mode()?;
    let converted = convert(
        account.id,
        account_currency,
        statements,
        &withdrawn,
        rounding,
    )?;

    let held = book.account_parts(account.id)?;
    let mut entries = Vec::new();
    let mut added = Vec::new();
    for (at, decision) in decide(statements, &held, choices) {
        let outcome = match decision {
            Decision::Import { similar } => {
                let entry = &statements[at.statement].entries[at.entry];
                let converted = converted[at.statement][at.entry];
                if let Some((_, replaced)) = converted {
                    // The part keeps its amount in the withdrawn currency,
                    // which the book lists then.
                    book.use_currency(replaced.currency().code())?;
                }
                let transaction = transaction_for(account.id, entry, converted)?;
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
        let bank = match withdrawn[index] {
            Some(replaced) => replaced
                .convert(closing.amount, account_currency, rounding)
                .map_err(|_| StoreError::AmountOverflow)?,
            None => closing.amount,
        };
        balances.push(BalanceCheck {
            statement: index,
            date: closing.date,
            bank,
            bank_original: withdrawn[index].map(|_| closing.amount),
            book: balance,
        });
    }

    let mut converted_from: Vec<Withdrawn> = Vec::new();
    for replaced in withdrawn.into_iter().flatten() {
        if !converted_from.contains(&replaced) {
            converted_from.push(replaced);
        }
    }
    Ok(ImportReport {
        entries,
        transfers,
        rules,
        balances,
        converted_from,
    })
}

/// An entry's amount in the account's currency, and the withdrawn currency
/// it was converted from (D-105).
type Converted = (Money, Withdrawn);

/// The amount each entry of `statements` comes to in `account_currency`,
/// for the statements in a `withdrawn` currency, with that currency (D-105).
///
/// As when a book in such a currency is converted (D-104), the account's
/// balance is converted once, as a bank converted it, and each entry comes
/// to the step between the converted balances before and after it: from the
/// statement's opening balance where it gives one, otherwise from where the
/// previous statement in the same currency ended, or from zero. Every entry
/// the bank booked counts, the ones already in the book too, since the
/// bank's balances include them; one it hasn't booked has no amount here.
fn convert(
    account: AccountId,
    account_currency: Currency,
    statements: &[Statement],
    withdrawn: &[Option<Withdrawn>],
    rounding: RoundingMode,
) -> Result<Vec<Vec<Option<Converted>>>, StoreError> {
    let overflow = |_| StoreError::AmountOverflow;
    let mut running: Option<(CurrencyCode, Changeover)> = None;
    let mut converted = Vec::with_capacity(statements.len());
    for (statement, replaced) in statements.iter().zip(withdrawn) {
        let Some(replaced) = *replaced else {
            running = None;
            converted.push(vec![None; statement.entries.len()]);
            continue;
        };
        let code = replaced.currency().code();
        let continues = matches!(&running, Some((earlier, _)) if *earlier == code);
        if statement.opening_balance.is_some() || !continues {
            let mut changeover = Changeover::new(rounding);
            if let Some(opening) = statement.opening_balance {
                let at_rate = replaced
                    .convert(opening.amount, account_currency, rounding)
                    .map_err(overflow)?;
                changeover
                    .book_given(account, opening.amount, at_rate)
                    .map_err(overflow)?;
            }
            running = Some((code, changeover));
        }
        let (_, changeover) = running.as_mut().expect("set above");
        let mut amounts = Vec::with_capacity(statement.entries.len());
        for entry in &statement.entries {
            amounts.push(if entry.status == EntryStatus::Pending {
                None
            } else {
                let step = changeover
                    .book(account, entry.amount, replaced, account_currency)
                    .map_err(overflow)?;
                Some((step, replaced))
            });
        }
        converted.push(amounts);
    }
    Ok(converted)
}

/// The transaction an entry becomes: the account's part, with everything
/// the bank delivered, and one part without a category (D-083). An entry
/// in a withdrawn currency comes to the `converted` amount, and the
/// account's part keeps the entry's amount as its original amount with the
/// fixed rate (D-105); the part without a category has none, so the user
/// can break it down freely.
fn transaction_for(
    account: AccountId,
    entry: &Entry,
    converted: Option<Converted>,
) -> Result<Transaction, PipelineError> {
    let delivered = ImportedDetails {
        bank_id: entry.bank_id.clone(),
        booking_date: Some(entry.booking_date),
        value_date: entry.value_date,
        other_party_name: entry.other_party_name.clone(),
        other_party_iban: entry.other_party_iban.clone(),
        remittance_information: entry.remittance_information.clone(),
    };
    let (amount, original) = match converted {
        Some((amount, replaced)) => (
            amount,
            Some(OriginalAmount {
                amount: entry.amount,
                rate: replaced.dated_rate(DatedRateId::generate()),
            }),
        ),
        None => (entry.amount, None),
    };
    let own = Split {
        imported: Some(delivered),
        original,
        ..Split::new(account, amount)
    };
    let open = amount
        .checked_neg()
        .map_err(|_| StoreError::AmountOverflow)?;
    let transaction = Transaction::new(
        TransactionId::generate(),
        entry.booking_date,
        entry.other_party_name.clone(),
        vec![own, Split::uncategorized(open)],
    )
    // Two parts in one currency that cancel out, the first with an account,
    // and an original amount the way the converted one points.
    .expect("an entry and its opposite form a transaction");
    Ok(transaction)
}
