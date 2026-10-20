// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Another program's book, such as KMyMoney's, into a new KassenSturz book
//! (M2.9): turned into the data of a complete export and filled in through
//! [`fill_book`], so it passes every check a book from a file passes.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, AccountKind, Currency, CurrencyCode, DatedRate, DatedRateId, ForeignAmount,
    Money, RateSource, Split, Transaction, TransactionId, TransferMatch,
};
use kassensturz_export::BookData;
use kassensturz_import::{BookAccount, ImportError, ImportedBook, Skipped, currency};
use kassensturz_store::{Book, StoreError};

use crate::book::fill_book;
use crate::error::PipelineError;

/// What [`fill_book_from`] put into the new book, and what it couldn't.
#[derive(Debug)]
pub struct BookImportReport {
    /// How many of the user's accounts the book has now.
    pub accounts: usize,
    /// How many categories.
    pub categories: usize,
    /// How many transactions.
    pub transactions: usize,
    /// How many exchange rates.
    pub rates: usize,
    /// How many transactions got a part without a category for what their
    /// parts didn't add up to, e.g. an entry the other program imported
    /// from a bank and nobody gave a category yet (D-083).
    pub completed: usize,
    /// Accounts and categories whose name another one had already, ignoring
    /// capitals, and that got a number after it, e.g. "Savings (2)": their
    /// new IDs and their names in the file.
    pub renamed: Vec<(AccountId, String)>,
    /// Categories that sat inside one in another currency, which a book
    /// can't add up (D-082), and are top-level categories now.
    pub moved_to_top: Vec<AccountId>,
    /// Accounts the other program had closed although money was still in
    /// them, which stay open, so no money is hidden (D-081).
    pub left_open: Vec<AccountId>,
    /// The Transfers to suggest between the new book's accounts (D-087):
    /// two transactions without a category yet that look like the two
    /// sides of one. Nothing is joined: the user confirms each one.
    pub transfers: Vec<TransferMatch>,
    /// What the file held that the book can't, as the importer counted it
    /// (D-029).
    pub skipped: Vec<Skipped>,
}

/// Fills the new, empty `book` with `imported`, a book another program
/// kept, and reports what it did.
///
/// Everything gets a new ID. Accounts and categories keep their names,
/// with a number after one that is taken already (`renamed`); a
/// transaction whose parts don't add up gets a part without a category for
/// the difference (`completed`). The decimal places of each currency are
/// the ones the importer read the amounts in, so nothing is rescaled
/// (D-027). The book's settings stay the ones it was created with, and,
/// as for a complete export, nothing is recorded in its change history
/// ([`fill_book`]): undoing the import would only leave an empty book.
///
/// If anything is refused, the book stays empty, so the front end can
/// remove it again.
pub fn fill_book_from(
    book: &mut Book,
    imported: &ImportedBook,
) -> Result<BookImportReport, PipelineError> {
    let mut mapper = Mapper::default();
    let mut accounts = mapper.accounts(&imported.accounts)?;
    let mut completed = 0;
    let mut transactions = Vec::with_capacity(imported.transactions.len());
    for transaction in &imported.transactions {
        mapper.uses(transaction.currency)?;
        let mut splits = Vec::with_capacity(transaction.parts.len() + 1);
        for part in &transaction.parts {
            let account = match &part.account {
                Some(key) => Some(mapper.id(key)?),
                None => None,
            };
            splits.push(Split {
                account,
                foreign: part
                    .account_amount
                    .map(|amount| ForeignAmount { amount, rate: None }),
                imported: part.details.clone(),
                ..Split::uncategorized(part.amount)
            });
        }
        let currency = transaction.currency.code();
        let total = Money::checked_sum(currency, splits.iter().map(|split| split.amount))
            .map_err(|_| StoreError::AmountOverflow)?;
        if !total.is_zero() || splits.len() < 2 {
            let open = total
                .checked_neg()
                .map_err(|_| StoreError::AmountOverflow)?;
            splits.push(Split::uncategorized(open));
            completed += 1;
        }
        let transaction = Transaction::new(
            TransactionId::generate(),
            transaction.date,
            transaction.payee.clone(),
            splits,
        )
        // The importer's checks (`kassensturz_import::import`) make sure of
        // what `Transaction::new` asks: one currency, an account for every
        // amount in another one, the same sign for both of a part's
        // amounts. A book handed over without them is refused here.
        .map_err(|_| ImportError::Malformed { line: None })?;
        transactions.push(transaction);
    }
    let mut rates = Vec::with_capacity(imported.rates.len());
    for rate in &imported.rates {
        for code in [rate.from, rate.to] {
            mapper.uses_code(code)?;
        }
        rates.push(DatedRate {
            id: DatedRateId::generate(),
            date: rate.date,
            from: rate.from,
            to: rate.to,
            rate: rate.rate,
            // Entered in the other program, the only way a rate gets into
            // a book in v1 (D-028).
            source: RateSource::Manual,
        });
    }
    let left_open = Mapper::keep_open_with_money(&mut accounts, &transactions);
    let data = BookData {
        rounding_mode: book.rounding_mode()?,
        history_keep_days: match book.history_retention()? {
            kassensturz_store::HistoryRetention::Days(days) => Some(days),
            kassensturz_store::HistoryRetention::Forever => None,
        },
        currencies: mapper.currencies,
        accounts,
        rates,
        rules: Vec::new(),
        transactions,
        rule_marks: Vec::new(),
    };
    fill_book(book, &data)?;
    let count = |kind: fn(AccountKind) -> bool| {
        data.accounts
            .iter()
            .filter(|account| kind(account.kind))
            .count()
    };
    Ok(BookImportReport {
        accounts: count(AccountKind::is_account),
        categories: count(AccountKind::is_category),
        transactions: data.transactions.len(),
        rates: data.rates.len(),
        completed,
        renamed: mapper.renamed,
        moved_to_top: mapper.moved_to_top,
        left_open,
        transfers: book.transfer_suggestions()?,
        skipped: imported.skipped.clone(),
    })
}

/// Turns the file's keys and names into a book's IDs and unique names.
#[derive(Default)]
struct Mapper {
    ids: HashMap<String, AccountId>,
    currencies: Vec<Currency>,
    renamed: Vec<(AccountId, String)>,
    moved_to_top: Vec<AccountId>,
}

impl Mapper {
    /// The ID the account with `key` gets.
    fn id(&self, key: &str) -> Result<AccountId, ImportError> {
        self.ids
            .get(key)
            .copied()
            .ok_or(ImportError::Malformed { line: None })
    }

    /// Notes that the book uses `currency` with its decimal places. Fails
    /// if the same code came with others before.
    fn uses(&mut self, currency: Currency) -> Result<(), ImportError> {
        match self
            .currencies
            .iter()
            .find(|known| known.code() == currency.code())
        {
            Some(known) if known.minor_digits() != currency.minor_digits() => {
                Err(ImportError::DecimalPlacesDiffer {
                    currency: currency.code(),
                    book: known.minor_digits(),
                    file: currency.minor_digits(),
                })
            }
            Some(_) => Ok(()),
            None => {
                self.currencies.push(currency);
                Ok(())
            }
        }
    }

    /// Notes that the book uses the currency `code`, which no amount gave
    /// decimal places yet, e.g. one only rates are in: KassenSturz's ISO
    /// 4217 table gives them (D-027).
    fn uses_code(&mut self, code: CurrencyCode) -> Result<(), ImportError> {
        if self.currencies.iter().any(|known| known.code() == code) {
            return Ok(());
        }
        self.uses(currency(code)?)
    }

    /// The book's accounts and categories, with new IDs, names that are
    /// unique where the book needs them to be (D-081, D-082), and every
    /// category inside one of its own currency.
    fn accounts(&mut self, imported: &[BookAccount]) -> Result<Vec<Account>, PipelineError> {
        for account in imported {
            if self.ids.contains_key(&account.key) {
                return Err(ImportError::Malformed { line: None }.into());
            }
            self.ids.insert(account.key.clone(), AccountId::generate());
            self.uses(account.currency)?;
        }
        let by_key: HashMap<&str, &BookAccount> = imported
            .iter()
            .map(|account| (account.key.as_str(), account))
            .collect();
        let mut accounts = Vec::with_capacity(imported.len());
        // Names in use, per list of names that must differ: the user's
        // accounts share one, categories one per kind and parent.
        let mut taken: HashSet<(Option<AccountKind>, Option<AccountId>, String)> = HashSet::new();
        for account in imported {
            let id = self.id(&account.key)?;
            let parent = match &account.parent {
                Some(key) => {
                    let parent = by_key
                        .get(key.as_str())
                        .ok_or(ImportError::Malformed { line: None })?;
                    if parent.currency.code() == account.currency.code() {
                        Some(self.id(key)?)
                    } else {
                        self.moved_to_top.push(id);
                        None
                    }
                }
                None => None,
            };
            let list = ((!account.kind.is_account()).then_some(account.kind), parent);
            let wanted = account.name.trim();
            let mut name = wanted.to_owned();
            let mut number = 1;
            while !taken.insert((list.0, list.1, name.to_lowercase())) {
                number += 1;
                name = format!("{wanted} ({number})");
            }
            if number > 1 {
                self.renamed.push((id, account.name.clone()));
            }
            accounts.push(Account {
                id,
                name,
                kind: account.kind,
                currency: account.currency.code(),
                account_type: account.account_type,
                parent,
                // Only the user's accounts can be closed (D-081).
                closed: account.closed && account.kind.is_account(),
            });
        }
        Ok(accounts)
    }

    /// Opens again the accounts the file says are closed whose balance
    /// isn't zero, since a book closes only those (D-081), and returns
    /// them: no money is hidden.
    fn keep_open_with_money(
        accounts: &mut [Account],
        transactions: &[Transaction],
    ) -> Vec<AccountId> {
        let mut balances: HashMap<AccountId, i128> = HashMap::new();
        for split in transactions.iter().flat_map(Transaction::splits) {
            if let Some(account) = split.account {
                *balances.entry(account).or_default() +=
                    i128::from(split.account_amount().minor_units());
            }
        }
        let mut left_open = Vec::new();
        for account in accounts.iter_mut().filter(|account| account.closed) {
            if balances
                .get(&account.id)
                .is_some_and(|&balance| balance != 0)
            {
                account.closed = false;
                left_open.push(account.id);
            }
        }
        left_open
    }
}
