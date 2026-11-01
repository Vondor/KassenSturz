// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! *Update all accounts* (M6d.7, D-446, D-449): the dashboard widget's one
//! button that fetches every linked open account from its bank and books
//! what came at once.
//!
//! The bank worker fetches (`bank_connection::update_all`); this file runs
//! on the GUI thread, which holds the book. Each fetched statement goes
//! through the same import as *Get new transactions*
//! (`statement_import::import`: the Rules, the duplicate check, the bank's
//! closing balance) and is booked at once as one *Imported a statement*
//! entry of the change history, so Undo takes an account's update back.
//! Entries the duplicate check recognises, or that only look like a
//! transaction the account has, are left out, as the page leaves them out
//! until the user decides. Only two kinds of booked transaction get a
//! notice (`kassensturz_report::notices`): one no Rule sorted, and one
//! whose amount is unusual for its category.
//!
//! Only the user's click starts it (AGENTS.md §3.1 rule 4, T-55): nothing
//! here runs on a timer or when a book opens. The summary lives in memory
//! until the user presses *Seen* or the app ends; nothing about it is
//! stored. The debug log gets event names only, never an account name, an
//! IBAN, an amount or the bank's text (D-359).

use std::collections::HashMap;

use kassensturz_core::{AccountId, Report, TransactionId};
use kassensturz_report::notices::{self, Reason};
use kassensturz_store::{Book, StoreError};

use crate::bank_connection::{self, FetchRequest, UpdateFetch, Updated};
use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::register::{AmountWriter, Shown};
use crate::statement_import::{self, Loaded};

/// What the widget needs to know before it is pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// How many open accounts are linked to a bank account. The widget
    /// takes no room while there is none (D-446).
    pub linked: usize,
    /// The oldest day a linked account was last fetched, in the user's
    /// format; empty when one was never fetched (`never`) or none is linked.
    pub last_updated: String,
    /// Whether a linked account was never fetched.
    pub never: bool,
}

/// What the widget knows about the book now.
pub fn state(book: &Book, locale: &impl Locale) -> Result<State, StoreError> {
    let open: Vec<AccountId> = book
        .accounts(false)?
        .into_iter()
        .map(|account| account.id)
        .collect();
    let links: Vec<_> = book
        .bank_links()?
        .into_iter()
        .filter(|link| open.contains(&link.account))
        .collect();
    let never = links.iter().any(|link| link.last_fetched_on.is_none());
    let oldest = links.iter().filter_map(|link| link.last_fetched_on).min();
    Ok(State {
        linked: links.len(),
        last_updated: match oldest {
            Some(day) if !never => locale.short_date(day),
            _ => String::new(),
        },
        never,
    })
}

/// Every linked open account, in the accounts list's order: what the
/// button fetches. The worker puts them into bank-by-bank order.
pub fn requests(book: &Book, locale: &impl Locale) -> Result<Vec<FetchRequest>, ErrorText> {
    let rows = bank_connection::fetch_rows(book, locale)
        .map_err(|error| report_text(book, &error.report(), locale))?;
    let ids: Vec<String> = rows
        .into_iter()
        .filter(|row| row.linked)
        .map(|row| row.id)
        .collect();
    bank_connection::fetch_requests(book, &ids, locale)
}

/// One account's line of the summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountLine {
    /// The account's ID.
    pub account: String,
    /// Its name, plain text.
    pub name: String,
    /// `updated`, `failed` or `skipped`.
    pub state: &'static str,
    /// How many transactions were booked into it.
    pub booked: usize,
    /// Why it failed.
    pub error: Option<ErrorText>,
    /// Why it was skipped (`bank_connection::Updated::Skipped`); empty
    /// otherwise.
    pub skipped: &'static str,
    /// The bank's closing balance where KassenSturz has another figure for
    /// that day, and the difference; `None` when they agree or the bank
    /// gave none (D-111).
    pub balance_off: Option<BalanceOff>,
}

/// A closing balance that does not agree with the bank's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceOff {
    /// The day the bank gives.
    pub date: String,
    /// What the bank says.
    pub bank: Shown,
    /// How far apart the two are, always positive.
    pub difference: Shown,
}

/// One transaction the user is asked to look at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticeLine {
    /// The transaction's ID, to open it in the register.
    pub transaction: String,
    /// The account it was booked into, its ID and name.
    pub account: String,
    pub account_name: String,
    /// The booking date.
    pub date: String,
    /// Who the money came from or went to, plain text; may be empty.
    pub payee: String,
    /// The amount, as seen from the account.
    pub amount: Shown,
    /// `unsorted` or `unusual`.
    pub reason: &'static str,
    /// For `unusual`: the category's name, its usual amount and the
    /// transaction's amount in it (never negative).
    pub category: String,
    pub usual: Shown,
    pub size: Shown,
}

/// What *Update all accounts* did, for the widget.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// How many accounts were fetched and booked.
    pub updated: usize,
    /// How many transactions were booked.
    pub booked: usize,
    /// One line per account, in the order they were fetched.
    pub accounts: Vec<AccountLine>,
    /// The notices, in the order the transactions were booked.
    pub notices: Vec<NoticeLine>,
}

/// Books every statement of `fetched` into its account, one history entry
/// each, and works out the notices. An account whose import fails is a
/// failed line; the others are booked all the same.
pub fn book_all(book: &mut Book, fetched: Vec<UpdateFetch>, locale: &impl Locale) -> Summary {
    let mut summary = Summary::default();
    // Which account each new transaction went into.
    let mut booked_into: Vec<(TransactionId, AccountId, String)> = Vec::new();
    for fetch in fetched {
        let mut line = AccountLine {
            account: fetch.account.to_string(),
            name: fetch.name.clone(),
            state: "skipped",
            booked: 0,
            error: None,
            skipped: "",
            balance_off: None,
        };
        match fetch.result {
            Updated::Skipped(reason) => line.skipped = reason,
            Updated::Failed(error) => {
                line.state = "failed";
                line.error = Some(error);
            }
            Updated::Fetched(statement) => {
                let loaded = Loaded::from_bank(fetch.account, statement, fetch.day);
                match statement_import::import(book, &loaded, locale) {
                    Ok(view) => {
                        line.state = "updated";
                        line.booked = view.new.len();
                        line.balance_off = view
                            .balances
                            .iter()
                            .rfind(|balance| balance.which == "closing" && !balance.matches)
                            .map(|balance| BalanceOff {
                                date: balance.date.clone(),
                                bank: balance.bank.clone(),
                                difference: balance.difference.clone(),
                            });
                        summary.updated += 1;
                        summary.booked += view.new.len();
                        booked_into.extend(
                            view.new
                                .iter()
                                .map(|&id| (id, fetch.account, fetch.name.clone())),
                        );
                    }
                    Err(error) => {
                        line.state = "failed";
                        line.error = Some(statement_import::error_text(book, &error, locale));
                    }
                }
            }
        }
        summary.accounts.push(line);
    }
    let new: Vec<TransactionId> = booked_into.iter().map(|(id, _, _)| *id).collect();
    // The notices are a help, not a figure: a book that can't be read for
    // them still has every transaction booked, and the user finds the
    // unsorted ones in the register as always.
    let found = notices::notices(book, &new).unwrap_or_default();
    let reasons: HashMap<TransactionId, Reason> = found
        .into_iter()
        .map(|notice| (notice.transaction, notice.reason))
        .collect();
    for (id, account, account_name) in booked_into {
        let Some(reason) = reasons.get(&id) else {
            continue;
        };
        if let Ok(line) = notice_line(book, id, account, &account_name, *reason, locale) {
            summary.notices.push(line);
        }
    }
    summary
}

fn notice_line(
    book: &Book,
    id: TransactionId,
    account: AccountId,
    account_name: &str,
    reason: Reason,
    locale: &impl Locale,
) -> Result<NoticeLine, StoreError> {
    let transaction = book.transaction(id)?;
    let writer = AmountWriter::new(book, transaction.currency(), locale)?;
    let amount = transaction
        .splits()
        .iter()
        .find(|split| split.account == Some(account))
        .map(|split| writer.text(split.amount))
        .unwrap_or_default();
    let mut line = NoticeLine {
        transaction: id.to_string(),
        account: account.to_string(),
        account_name: account_name.to_owned(),
        date: locale.short_date(transaction.booking_date()),
        payee: transaction.payee().unwrap_or_default().to_owned(),
        amount,
        reason: "unsorted",
        category: String::new(),
        usual: Shown::default(),
        size: Shown::default(),
    };
    if let Reason::Unusual {
        category,
        amount,
        usual,
    } = reason
    {
        line.reason = "unusual";
        line.category = book.account(category)?.name;
        line.usual = writer.text(usual);
        line.size = writer.text(amount);
    }
    Ok(line)
}

#[cfg(test)]
#[path = "update_all_tests.rs"]
mod tests;
