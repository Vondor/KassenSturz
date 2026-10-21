// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A `.kmy` file's records in KassenSturz's terms: which of KMyMoney's
//! accounts are accounts and which are categories, amounts from fractions,
//! and what a book can't hold counted (D-029, D-101).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{AccountKind, AccountType, Currency, CurrencyCode, Date, ImportedDetails};

use super::fraction::Fraction;
use super::records::{RawAccount, RawSplit, RawTransaction, Records, Skips};
use crate::amount;
use crate::book::{
    BookAccount, BookPart, BookRate, BookTransaction, ImportedBook, Skipped, SkippedData,
};
use crate::error::ImportError;

/// KMyMoney's top-level accounts, one for each kind, which every other
/// account and category sits inside. They aren't imported: a book has its
/// kinds without them.
const TOP_LEVEL: [(&str, AccountKind); 5] = [
    ("AStd::Asset", AccountKind::Asset),
    ("AStd::Liability", AccountKind::Liability),
    ("AStd::Income", AccountKind::Income),
    ("AStd::Expense", AccountKind::Expense),
    ("AStd::Equity", AccountKind::Equity),
];

/// KMyMoney's numbers for the types a book has too, as KMyMoney names
/// them in the SQLite version of the sample (`accountTypeString`, D-102):
/// `1` Girokonto, `2` Sparkonto, `3` Bargeld. Other types come in without
/// one; their kind still comes from the top-level account they sit in.
const TYPES: [(&str, AccountType); 3] = [
    ("1", AccountType::Checking),
    ("2", AccountType::Savings),
    ("3", AccountType::Cash),
];

/// The book that `records` describe.
pub(super) fn book(records: Records) -> Result<ImportedBook, ImportError> {
    let mut skipped = records.skipped;
    let accounts = Accounts::new(&records.accounts, &records.securities)?;
    for account in &records.accounts {
        if accounts.investments.contains(account.id.as_str()) {
            skipped.add(SkippedData::Investments);
        }
    }
    let mut book_accounts = Vec::with_capacity(records.accounts.len());
    for account in &records.accounts {
        if let Some(book_account) = accounts.book_account(account, &mut skipped)? {
            book_accounts.push(book_account);
        }
    }

    let payees: HashMap<&str, &str> = records
        .payees
        .iter()
        .map(|payee| (payee.id.as_str(), payee.name.as_str()))
        .collect();
    let mut used_payees: HashSet<&str> = HashSet::new();
    let mut transactions = Vec::with_capacity(records.transactions.len());
    for raw in &records.transactions {
        let transaction = transaction(raw, &accounts, &payees, &mut skipped)?;
        used_payees.extend(
            raw.splits
                .iter()
                .map(|split| split.payee.as_str())
                .filter(|payee| !payee.is_empty()),
        );
        transactions.push(transaction);
    }
    for payee in &records.payees {
        if payee.details || !used_payees.contains(payee.id.as_str()) {
            skipped.add(SkippedData::Payees);
        }
    }

    let mut rates = Vec::new();
    let mut seen = HashSet::new();
    for price in &records.prices {
        let date = date(&price.date).map_err(|error| error.at(price.line))?;
        let fraction = fraction(&price.price).map_err(|error| error.at(price.line))?;
        let rate = match (known_currency(&price.from), known_currency(&price.to)) {
            (Some(from), Some(to)) if from != to => fraction
                .rate()
                .map(|rate| BookRate {
                    date,
                    from,
                    to,
                    rate,
                })
                // A book holds one rate a day for each direction.
                .filter(|_| seen.insert((from, to, date))),
            _ => None,
        };
        match rate {
            Some(rate) => rates.push(rate),
            None => skipped.add(SkippedData::Rates),
        }
    }

    Ok(ImportedBook {
        accounts: book_accounts,
        transactions,
        rates,
        skipped: skipped
            .into_counts()
            .into_iter()
            .map(|(what, count)| Skipped { what, count })
            .collect(),
    })
}

/// What the importer knows about each account of the file.
struct Accounts<'a> {
    /// Every account by its ID, top-level ones included.
    by_id: HashMap<&'a str, &'a RawAccount>,
    /// The kind of every account but the top-level ones, from the
    /// top-level account it sits in.
    kinds: HashMap<&'a str, AccountKind>,
    /// The accounts that count in a security: investments, which the book
    /// leaves out.
    investments: HashSet<&'a str>,
}

impl<'a> Accounts<'a> {
    fn new(accounts: &'a [RawAccount], securities: &HashSet<String>) -> Result<Self, ImportError> {
        let malformed = |account: &RawAccount| ImportError::Malformed { line: account.line };
        let mut by_id = HashMap::with_capacity(accounts.len());
        for account in accounts {
            if by_id.insert(account.id.as_str(), account).is_some() {
                return Err(malformed(account));
            }
        }
        let mut kinds = HashMap::with_capacity(accounts.len());
        let mut investments = HashSet::new();
        for account in accounts {
            if account.parent.is_empty() {
                // A top-level account must be one of the five.
                top_level(&account.id).ok_or_else(|| malformed(account))?;
                continue;
            }
            // Up the tree to the top-level account; a tree that loops or
            // points nowhere is broken.
            let mut parent = account;
            let mut steps = 0;
            let kind = loop {
                if parent.parent.is_empty() {
                    break top_level(&parent.id).ok_or_else(|| malformed(account))?;
                }
                parent = by_id
                    .get(parent.parent.as_str())
                    .ok_or_else(|| malformed(account))?;
                steps += 1;
                if steps > accounts.len() {
                    return Err(malformed(account));
                }
            };
            kinds.insert(account.id.as_str(), kind);
            if securities.contains(&account.currency) {
                investments.insert(account.id.as_str());
            }
        }
        Ok(Self {
            by_id,
            kinds,
            investments,
        })
    }

    /// The book's account for `account`, or `None` if the book leaves it
    /// out: a top-level account or an investment.
    fn book_account(
        &self,
        account: &RawAccount,
        skipped: &mut Skips,
    ) -> Result<Option<BookAccount>, ImportError> {
        let Some(&kind) = self.kinds.get(account.id.as_str()) else {
            return Ok(None);
        };
        if self.investments.contains(account.id.as_str()) {
            return Ok(None);
        }
        let currency = amount::currency(code(&account.currency, account.line)?)
            .map_err(|error| error.at(account.line))?;
        let account_type = TYPES
            .iter()
            .find(|(number, _)| *number == account.account_type)
            .map(|&(_, account_type)| account_type)
            .filter(|account_type| account_type.kind() == kind);
        let parent_is_top = self
            .by_id
            .get(account.parent.as_str())
            .is_some_and(|parent| parent.parent.is_empty());
        // Categories keep their tree; a book doesn't nest accounts.
        let parent = (kind.is_category() && !parent_is_top).then(|| account.parent.clone());
        if !kind.is_category() && !parent_is_top {
            skipped.add(SkippedData::AccountGroups);
        }
        if !account.number.trim().is_empty() {
            skipped.add(SkippedData::AccountNumbers);
        }
        if !account.description.trim().is_empty() {
            skipped.add(SkippedData::Notes);
        }
        if account.online_banking {
            skipped.add(SkippedData::OnlineBanking);
        }
        Ok(Some(BookAccount {
            key: account.id.clone(),
            name: account.name.clone(),
            kind,
            account_type,
            currency,
            parent,
            // The samples close no account, so the importer doesn't know
            // how a file says it: every account comes in open, which hides
            // no money (D-081).
            closed: false,
        }))
    }
}

/// The kind of the top-level account `id`, if it is one of the five.
fn top_level(id: &str) -> Option<AccountKind> {
    TOP_LEVEL
        .iter()
        .find(|(top, _)| *top == id)
        .map(|&(_, kind)| kind)
}

/// One transaction of the book.
fn transaction(
    raw: &RawTransaction,
    accounts: &Accounts<'_>,
    payees: &HashMap<&str, &str>,
    skipped: &mut Skips,
) -> Result<BookTransaction, ImportError> {
    let at_transaction = |error: ImportError| error.at(raw.line);
    let currency = amount::currency(code(&raw.currency, raw.line)?).map_err(at_transaction)?;
    // A book has one payee for a transaction; KMyMoney keeps one for each
    // split, and the first one given stands for the transaction.
    let payee = match raw.splits.iter().find(|split| !split.payee.is_empty()) {
        Some(split) => Some(
            (*payees
                .get(split.payee.as_str())
                .ok_or(ImportError::Malformed { line: split.line })?)
            .to_owned(),
        )
        .filter(|name| !name.trim().is_empty()),
        None => None,
    };
    let mut parts = Vec::with_capacity(raw.splits.len());
    let mut memo_kept = false;
    for split in &raw.splits {
        let part =
            part(split, currency, accounts, skipped).map_err(|error| error.at(split.line))?;
        memo_kept |= part.details.is_some() && split.memo == raw.memo;
        parts.push(part);
    }
    if !raw.memo.trim().is_empty() && !memo_kept {
        skipped.add(SkippedData::Notes);
    }
    Ok(BookTransaction {
        date: date(&raw.date).map_err(at_transaction)?,
        payee,
        currency,
        parts,
    })
}

/// One part of a transaction in `currency`.
fn part(
    split: &RawSplit,
    currency: Currency,
    accounts: &Accounts<'_>,
    skipped: &mut Skips,
) -> Result<BookPart, ImportError> {
    let malformed = || ImportError::Malformed { line: split.line };
    let value = fraction(&split.value)?;
    let shares = fraction(&split.shares)?;
    let amount = value.money(currency, &split.value)?;
    if split.reconciled != "0" && !split.reconciled.is_empty() {
        skipped.add(SkippedData::Reconciliation);
    }
    if !split.number.trim().is_empty() {
        skipped.add(SkippedData::Notes);
    }
    let account = accounts
        .by_id
        .get(split.account.as_str())
        .ok_or_else(malformed)?;
    let Some(&kind) = accounts.kinds.get(account.id.as_str()) else {
        // Booked to a top-level account itself.
        return Err(malformed());
    };
    if accounts.investments.contains(account.id.as_str()) {
        // Its money stays in the transaction, without a category.
        if !split.memo.trim().is_empty() {
            skipped.add(SkippedData::Notes);
        }
        return Ok(BookPart {
            account: None,
            amount,
            account_amount: None,
            details: None,
        });
    }
    let account_currency = code(&account.currency, account.line)?;
    let account_amount = if account_currency == currency.code() {
        // Both amounts are in the same currency: they must agree.
        if !value.equals(shares) {
            return Err(malformed());
        }
        None
    } else {
        let account_currency = amount::currency(account_currency)?;
        Some(shares.money(account_currency, &split.shares)?)
    };
    // What a bank delivered, or the note kept with the part, stays with a
    // part booked to one of the user's accounts (D-083).
    let details = if kind.is_account() {
        let text = |text: &str| (!text.trim().is_empty()).then(|| text.to_owned());
        let details = ImportedDetails {
            bank_id: text(&split.bank_id),
            remittance_information: text(&split.memo),
            ..ImportedDetails::default()
        };
        (details != ImportedDetails::default()).then_some(details)
    } else {
        if !split.memo.trim().is_empty() {
            skipped.add(SkippedData::Notes);
        }
        None
    };
    Ok(BookPart {
        account: Some(account.id.clone()),
        amount,
        account_amount,
        details,
    })
}

/// The fraction `text`, which must be one.
fn fraction(text: &str) -> Result<Fraction, ImportError> {
    Fraction::parse(text).ok_or_else(|| ImportError::BadAmount {
        found: text.to_owned(),
        line: None,
    })
}

/// The currency code `text`, which must be one.
fn code(text: &str, line: Option<u64>) -> Result<CurrencyCode, ImportError> {
    CurrencyCode::new(text).map_err(|_| ImportError::Malformed { line })
}

/// The currency code `text`, if KassenSturz's ISO 4217 table lists it.
fn known_currency(text: &str) -> Option<CurrencyCode> {
    CurrencyCode::new(text)
        .ok()
        .filter(|code| code.iso_minor_digits().is_some())
}

/// A date as the file writes it, `2026-10-01`.
fn date(text: &str) -> Result<Date, ImportError> {
    let bad = || ImportError::BadDate {
        found: text.to_owned(),
        line: None,
    };
    let well_formed = text.len() == 10
        && text.bytes().enumerate().all(|(at, byte)| match at {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    if !well_formed {
        return Err(bad());
    }
    text.parse().map_err(|_| bad())
}
