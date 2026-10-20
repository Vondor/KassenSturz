// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an import did, or a preview would do: the import summary (D-030)
//! and the balance check (D-029).

use kassensturz_core::{Date, Money, TransactionId, TransferMatch};
use kassensturz_store::RuleRun;

/// One entry of the statements given to the pipeline: the statement's
/// position in the list, then the entry's in that statement, both counted
/// from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryRef {
    /// The statement.
    pub statement: usize,
    /// The entry in it.
    pub entry: usize,
}

/// What an import did, or a preview would do.
#[derive(Debug)]
pub struct ImportReport {
    /// What happened to each entry, in the order of the statements and
    /// their entries.
    pub entries: Vec<EntryResult>,
    /// The Transfers to suggest that have a new transaction as one side
    /// (D-087), sorted by the outgoing side's date. Nothing is joined: the
    /// user confirms each one (`Book::join_transfer`).
    pub transfers: Vec<TransferMatch>,
    /// What the Rules did to the new transactions (D-088). A transaction in
    /// a suggested Transfer waits for the user's decision on it.
    pub rules: RuleRun,
    /// The balance check for each statement with a closing balance, in the
    /// statements' order (D-029).
    pub balances: Vec<BalanceCheck>,
}

impl ImportReport {
    /// How many entries were imported, left out or skipped.
    pub fn summary(&self) -> Summary {
        let mut summary = Summary::default();
        for result in &self.entries {
            let count = match result.outcome {
                EntryOutcome::Imported { .. } => &mut summary.imported,
                EntryOutcome::Similar(_) => &mut summary.to_decide,
                EntryOutcome::Duplicate(_) => &mut summary.duplicates,
                EntryOutcome::Pending => &mut summary.pending,
            };
            *count += 1;
        }
        summary
    }

    /// The new transactions, in the order of their entries.
    pub fn imported(&self) -> Vec<TransactionId> {
        self.entries
            .iter()
            .filter_map(|result| match result.outcome {
                EntryOutcome::Imported { transaction, .. } => Some(transaction),
                _ => None,
            })
            .collect()
    }
}

/// The import summary: how many entries went where (D-030).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Imported as new transactions.
    pub imported: usize,
    /// Left out because they look like a transaction the account already
    /// has: the user decides.
    pub to_decide: usize,
    /// Skipped because the account already holds their bank ID, or an
    /// earlier entry has it.
    pub duplicates: usize,
    /// Left out because the bank hasn't booked them yet.
    pub pending: usize,
}

/// What happened to one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryResult {
    /// The entry.
    pub entry: EntryRef,
    /// What happened to it.
    pub outcome: EntryOutcome,
}

/// What happened to one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryOutcome {
    /// Imported as a new transaction. `similar` names the transactions it
    /// looks like, if the user chose to import it anyway; otherwise it is
    /// empty.
    Imported {
        /// The new transaction.
        transaction: TransactionId,
        /// What it looks like.
        similar: Vec<TransactionId>,
    },
    /// Left out, because it looks like these transactions the account
    /// already has, oldest first: the same amount, at most three business
    /// days apart, and not two different bank IDs (D-092). The user decides
    /// ([`crate::Choices::import_similar`]).
    Similar(Vec<TransactionId>),
    /// Skipped, since it is in the book already (D-030).
    Duplicate(Duplicate),
    /// Left out, since the bank hasn't booked it yet (D-030).
    Pending,
}

/// Where an entry that was skipped is already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Duplicate {
    /// The account holds its bank ID already, in this transaction.
    InBook(TransactionId),
    /// An earlier entry of the statements has the same bank ID.
    InStatements(EntryRef),
}

/// KassenSturz's balance of the account against the one the bank reported
/// at the end of a statement (D-029).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalanceCheck {
    /// The statement, counted from 0.
    pub statement: usize,
    /// The day the bank's balance is for.
    pub date: Date,
    /// The bank's balance at the end of that day.
    pub bank: Money,
    /// KassenSturz's balance at the end of that day: everything booked to
    /// the account on or before it, by the day the bank booked it where it
    /// says so, otherwise by the transaction's booking date.
    pub book: Money,
}

impl BalanceCheck {
    /// Whether both say the same.
    pub fn matches(&self) -> bool {
        self.bank == self.book
    }
}
