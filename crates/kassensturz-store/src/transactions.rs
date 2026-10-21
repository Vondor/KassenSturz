// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transactions and their parts in a book: adding, changing, removing and
//! reading them, an account's register and balance, and converting amounts
//! for parts in another currency (D-017, D-043, D-067, D-083).

use std::collections::HashMap;

use kassensturz_core::{
    Account, AccountId, CurrencyCode, Date, DatedRate, DatedRateId, ForeignAmount, ImportedDetails,
    Money, OriginalAmount, Split, SplitId, Transaction, TransactionId,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Corruption, StoreError};
use crate::{
    Action, Book, RateRow, currency_from_text, date_from_text, date_to_text, source_to_text,
};

/// One line of an account's register: a transaction as seen from that
/// account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterEntry {
    /// The transaction.
    pub transaction: TransactionId,
    /// Its booking date.
    pub booking_date: Date,
    /// Who the money came from or went to.
    pub payee: Option<String>,
    /// What it did to this account, in the account's currency: positive
    /// amounts came in, negative ones went out.
    pub amount: Money,
}

/// One part booked to an account, with what a bank or file delivered for
/// it, e.g. for comparing a statement's entries with what the account
/// already holds (D-030, D-092).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountPart {
    /// The transaction it belongs to.
    pub transaction: TransactionId,
    /// The part.
    pub split: SplitId,
    /// The transaction's booking date, the one the user sees.
    pub booking_date: Date,
    /// What it does to the account, in the account's currency: positive
    /// amounts came in, negative ones went out.
    pub amount: Money,
    /// What a bank or file delivered for it, if it came from one (D-067).
    pub imported: Option<ImportedDetails>,
}

impl Book {
    /// Adds a transaction.
    ///
    /// Its currency must be in use in this book. Every part with an account
    /// must be booked to an existing account in the part's currency: the
    /// transaction's, or for a part in another currency, that of its foreign
    /// amount (D-043). A part's original amount, from before it was
    /// converted out of a withdrawn currency, must be in a currency in use in
    /// this book too (D-104). A closed account takes no new transactions
    /// (D-081). Neither the transaction's ID nor its parts' may be in use
    /// already.
    pub fn add_transaction(&mut self, transaction: &Transaction) -> Result<(), StoreError> {
        let accounts = self.check_transaction(transaction)?;
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM transactions WHERE id = ?1",
                [transaction.id().to_bytes()],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(StoreError::TransactionIdInUse(transaction.id()));
        }
        for split in transaction.splits() {
            if let Some(account) = split.account.and_then(|id| accounts.get(&id))
                && account.closed
            {
                return Err(StoreError::AccountClosed(account.id));
            }
        }
        self.as_one_action(Action::AddTransaction, |book| {
            book.conn.execute(
                "INSERT INTO transactions (id, booking_date, payee, currency)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    transaction.id().to_bytes(),
                    date_to_text(transaction.booking_date())?,
                    transaction.payee(),
                    transaction.currency().as_str()
                ],
            )?;
            insert_splits(&book.conn, transaction)
        })
    }

    /// Replaces the transaction with `transaction`'s ID by `transaction`:
    /// its date, payee and parts, which can be added, removed, resized or
    /// given other categories, after the fact (D-017). The changed
    /// transaction is checked as in [`Book::add_transaction`].
    ///
    /// Three things can't change. A part delivered by a bank or file keeps
    /// its account, its amount in that account's currency and what was
    /// delivered with it (D-067), so the transaction keeps adding up to what
    /// the bank said; otherwise the change fails with
    /// [`StoreError::ImportedSplitChanged`]. A part converted from a
    /// withdrawn currency keeps its original amount, its rate and the amount
    /// it was converted into (D-104), so the original amount is never lost
    /// (AGENTS.md §5); only its account may change, and no other part can
    /// get one ([`StoreError::OriginalChanged`]). And the total booked to a
    /// closed account stays the same ([`StoreError::AccountClosed`]), so no
    /// money appears in or vanishes from it.
    pub fn update_transaction(&mut self, transaction: &Transaction) -> Result<(), StoreError> {
        let stored = self.transaction(transaction.id())?;
        let accounts = self.check_transaction(transaction)?;
        check_imported_unchanged(&stored, transaction)?;
        check_originals_unchanged(&stored, transaction)?;
        let before = account_totals(&stored)?;
        let after = account_totals(transaction)?;
        let touched = stored
            .splits()
            .iter()
            .chain(transaction.splits())
            .filter_map(|split| split.account);
        for id in touched {
            let closed = match accounts.get(&id) {
                Some(account) => account.closed,
                None => self.account(id)?.closed,
            };
            let total = |totals: &HashMap<AccountId, Money>| {
                totals.get(&id).map_or(0, |amount| amount.minor_units())
            };
            if closed && total(&before) != total(&after) {
                return Err(StoreError::AccountClosed(id));
            }
        }
        let id = transaction.id().to_bytes();
        self.as_one_action(Action::EditTransaction, |book| {
            book.conn.execute(
                "UPDATE transactions SET booking_date = ?2, payee = ?3, currency = ?4
                 WHERE id = ?1",
                params![
                    id,
                    date_to_text(transaction.booking_date())?,
                    transaction.payee(),
                    transaction.currency().as_str()
                ],
            )?;
            // Their rates and imported details go with them, and come back
            // with the new parts. The history records only the parts that
            // really changed.
            book.conn
                .execute("DELETE FROM splits WHERE transaction_id = ?1", [id])?;
            insert_splits(&book.conn, transaction)
        })
    }

    /// Removes a transaction with all its parts, also one delivered by a
    /// bank or file. The total of a closed account can't change, so a
    /// transaction that books money to one stays ([`StoreError::AccountClosed`]).
    ///
    /// It reads only what this needs, so a damaged transaction can be
    /// removed too.
    pub fn remove_transaction(&mut self, id: TransactionId) -> Result<(), StoreError> {
        // What the transaction books to each closed account, in the
        // account's currency, added up in 128 bits.
        let mut statement = self.conn.prepare(
            "SELECT s.account_id, coalesce(s.foreign_amount, s.amount)
             FROM splits AS s JOIN accounts AS a ON a.id = s.account_id
             WHERE s.transaction_id = ?1 AND a.closed = 1
             ORDER BY s.position",
        )?;
        let rows = statement
            .query_map([id.to_bytes()], |row| {
                Ok((AccountId::from_bytes(row.get(0)?), row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut totals: Vec<(AccountId, i128)> = Vec::new();
        for (account, amount) in rows {
            match totals.iter_mut().find(|(id, _)| *id == account) {
                Some((_, total)) => *total += i128::from(amount),
                None => totals.push((account, i128::from(amount))),
            }
        }
        if let Some(&(account, _)) = totals.iter().find(|(_, total)| *total != 0) {
            return Err(StoreError::AccountClosed(account));
        }
        // Its parts, their rates and imported details go with it, and come
        // back with it when the removal is undone.
        self.as_one_action(Action::RemoveTransaction, |book| {
            let removed = book
                .conn
                .execute("DELETE FROM transactions WHERE id = ?1", [id.to_bytes()])?;
            if removed == 1 {
                Ok(())
            } else {
                Err(StoreError::UnknownTransaction(id))
            }
        })
    }

    /// The transaction with this ID. It's checked again on the way out, so a
    /// transaction that doesn't balance, or a part that isn't in its
    /// account's currency, is reported, never returned.
    pub fn transaction(&self, id: TransactionId) -> Result<Transaction, StoreError> {
        let row: Option<(String, Option<String>, String)> = self
            .conn
            .query_row(
                "SELECT booking_date, payee, currency FROM transactions WHERE id = ?1",
                [id.to_bytes()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (booking_date, payee, currency) = row.ok_or(StoreError::UnknownTransaction(id))?;

        let mut statement = self.conn.prepare(&format!(
            "SELECT {SPLIT_COLUMNS}, {IMPORTED_COLUMNS} {SPLIT_TABLES}
             WHERE s.transaction_id = ?1 ORDER BY s.position"
        ))?;
        let rows = statement
            .query_map([id.to_bytes()], SplitRow::read)?
            .collect::<Result<Vec<_>, _>>()?;
        assemble(id, &booking_date, payee, &currency, rows, |account| {
            let currency: Option<String> = self
                .conn
                .query_row(
                    "SELECT currency FROM accounts WHERE id = ?1",
                    [account.to_bytes()],
                    |row| row.get(0),
                )
                .optional()?;
            currency.as_deref().map(currency_from_text).transpose()
        })
    }

    /// Every transaction in the book, sorted by booking date and then by ID,
    /// each checked as [`Book::transaction`] checks it. Reads the book once,
    /// e.g. for the complete export (D-029).
    pub fn transactions(&self) -> Result<Vec<Transaction>, StoreError> {
        let accounts: HashMap<AccountId, CurrencyCode> = self
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, account.currency))
            .collect();
        let mut statement = self.conn.prepare(&format!(
            "SELECT {SPLIT_COLUMNS}, {IMPORTED_COLUMNS}, s.transaction_id {SPLIT_TABLES}
             ORDER BY s.transaction_id, s.position"
        ))?;
        let mut parts: HashMap<[u8; 16], Vec<SplitRow>> = HashMap::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let transaction: [u8; 16] = row.get(SPLIT_ROW_COLUMNS)?;
            parts
                .entry(transaction)
                .or_default()
                .push(SplitRow::read(row)?);
        }
        drop(rows);
        let mut statement = self.conn.prepare(
            "SELECT id, booking_date, payee, currency FROM transactions ORDER BY booking_date, id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, [u8; 16]>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, booking_date, payee, currency)| {
                let own = parts.remove(&id).unwrap_or_default();
                assemble(
                    TransactionId::from_bytes(id),
                    &booking_date,
                    payee,
                    &currency,
                    own,
                    |account| Ok(accounts.get(&account).copied()),
                )
            })
            .collect()
    }

    /// The transactions that touch `account`, oldest first, each as seen from
    /// that account, in its currency.
    pub fn register(&self, account: AccountId) -> Result<Vec<RegisterEntry>, StoreError> {
        let account = self.account(account)?;
        let mut statement = self.conn.prepare(
            "SELECT t.id, t.booking_date, t.payee, t.currency,
                    s.id, s.amount, s.foreign_amount, s.foreign_currency
             FROM splits AS s JOIN transactions AS t ON t.id = s.transaction_id
             WHERE s.account_id = ?1
             ORDER BY t.booking_date, t.id, s.position",
        )?;
        let mut rows = statement.query([account.id.to_bytes()])?;
        let mut entries: Vec<RegisterEntry> = Vec::new();
        while let Some(row) = rows.next()? {
            let transaction = TransactionId::from_bytes(row.get(0)?);
            let amount = account_amount(&account, transaction, row, 3)?;
            match entries.last_mut() {
                // One account can appear in several parts of a transaction.
                Some(last) if last.transaction == transaction => {
                    last.amount = last
                        .amount
                        .checked_add(amount)
                        .map_err(|_| StoreError::AmountOverflow)?;
                }
                _ => entries.push(RegisterEntry {
                    transaction,
                    booking_date: date_from_text(&row.get::<_, String>(1)?)?,
                    payee: row.get(2)?,
                    amount,
                }),
            }
        }
        Ok(entries)
    }

    /// The parts booked to `account`, oldest first, each with its amount in
    /// the account's currency and what a bank or file delivered for it.
    /// Unlike [`Book::register`], a transaction with several parts in the
    /// account gives one entry per part.
    pub fn account_parts(&self, account: AccountId) -> Result<Vec<AccountPart>, StoreError> {
        let account = self.account(account)?;
        let mut statement = self.conn.prepare(&format!(
            "SELECT t.id, t.booking_date, t.currency,
                    s.id, s.amount, s.foreign_amount, s.foreign_currency, {IMPORTED_COLUMNS}
             FROM splits AS s JOIN transactions AS t ON t.id = s.transaction_id
             LEFT JOIN split_imports AS i ON i.split_id = s.id
             WHERE s.account_id = ?1
             ORDER BY t.booking_date, t.id, s.position"
        ))?;
        let mut rows = statement.query([account.id.to_bytes()])?;
        let mut parts = Vec::new();
        while let Some(row) = rows.next()? {
            let transaction = TransactionId::from_bytes(row.get(0)?);
            parts.push(AccountPart {
                transaction,
                split: SplitId::from_bytes(row.get(3)?),
                booking_date: date_from_text(&row.get::<_, String>(1)?)?,
                amount: account_amount(&account, transaction, row, 2)?,
                imported: ImportedRow::read(row, 7)?
                    .map(ImportedRow::into_details)
                    .transpose()?,
            });
        }
        Ok(parts)
    }

    /// The balance of `account`, in its currency: everything booked to it,
    /// added up exactly. For a part in another currency than its
    /// transaction's, that's its foreign amount (D-043).
    pub fn balance(&self, account: AccountId) -> Result<Money, StoreError> {
        let account = self.account(account)?;
        let mut statement = self.conn.prepare(
            "SELECT t.id, t.currency, s.id, s.amount, s.foreign_amount, s.foreign_currency
             FROM splits AS s JOIN transactions AS t ON t.id = s.transaction_id
             WHERE s.account_id = ?1",
        )?;
        let mut rows = statement.query([account.id.to_bytes()])?;
        let mut amounts = Vec::new();
        while let Some(row) = rows.next()? {
            let transaction = TransactionId::from_bytes(row.get(0)?);
            amounts.push(account_amount(&account, transaction, row, 1)?);
        }
        Money::checked_sum(account.currency, amounts).map_err(|_| StoreError::AmountOverflow)
    }

    /// Converts `amount` into the currency `to` with the exchange rate that
    /// applies on `date`, and returns the result with the rate used, which a
    /// part in another currency records ([`ForeignAmount::rate`], D-043).
    ///
    /// The rate is the newest one on or before `date` in either direction: a
    /// rate from `amount`'s currency into `to` is multiplied by, one the
    /// other way divided by (D-079). If both directions have one on that
    /// newest day, the one from `amount`'s currency into `to` counts. The
    /// result is rounded once to whole minor units, in the book's rounding
    /// mode (D-027). Fails with [`StoreError::NoRate`] if there's no rate
    /// between the two currencies on or before `date`, so the user can enter
    /// one.
    pub fn convert(
        &self,
        amount: Money,
        to: CurrencyCode,
        date: Date,
    ) -> Result<(Money, DatedRate), StoreError> {
        let from = amount.currency();
        if from == to {
            return Err(StoreError::RateWithinOneCurrency(from));
        }
        let forward = self.rate_on(from, to, date)?;
        let backward = self.rate_on(to, from, date)?;
        let (rate, multiply) = match (forward, backward) {
            (Some(forward), Some(backward)) if backward.date > forward.date => (backward, false),
            (Some(forward), _) => (forward, true),
            (None, Some(backward)) => (backward, false),
            (None, None) => return Err(StoreError::NoRate { from, to, date }),
        };
        let rounding = self.rounding_mode()?;
        let (from, to) = (self.currency(from)?, self.currency(to)?);
        let converted = if multiply {
            amount.checked_mul_rate(from, rate.rate, to, rounding)
        } else {
            amount.checked_div_rate(from, rate.rate, to, rounding)
        };
        let converted = converted.map_err(|_| StoreError::AmountOverflow)?;
        Ok((converted, rate))
    }

    /// The checks `add_transaction` and `update_transaction` share. Returns
    /// the accounts the transaction's parts are booked to.
    fn check_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<HashMap<AccountId, Account>, StoreError> {
        date_to_text(transaction.booking_date())?;
        self.currency_minor_digits(transaction.currency())?;
        let mut accounts = HashMap::new();
        for split in transaction.splits() {
            let dates = split.imported.iter().flat_map(|imported| {
                [imported.booking_date, imported.value_date]
                    .into_iter()
                    .flatten()
            });
            let rate_date = split
                .foreign
                .and_then(|foreign| foreign.rate)
                .or(split.original.map(|original| original.rate))
                .map(|rate| rate.date);
            for date in dates.chain(rate_date) {
                date_to_text(date)?;
            }
            if let Some(original) = split.original {
                self.currency_minor_digits(original.amount.currency())?;
            }
            let used_by: Option<[u8; 16]> = self
                .conn
                .query_row(
                    "SELECT transaction_id FROM splits WHERE id = ?1",
                    [split.id.to_bytes()],
                    |row| row.get(0),
                )
                .optional()?;
            if used_by.is_some_and(|other| other != transaction.id().to_bytes()) {
                return Err(StoreError::SplitIdInUse(split.id));
            }
            let Some(id) = split.account else {
                continue;
            };
            let account = self.account(id)?;
            let split_currency = split.account_amount().currency();
            if account.currency != split_currency {
                return Err(StoreError::SplitCurrencyMismatch {
                    account: id,
                    account_currency: account.currency,
                    split_currency,
                });
            }
            accounts.insert(id, account);
        }
        Ok(accounts)
    }
}

/// A transaction from its row and its parts' rows, checked: it balances,
/// and every part is in its account's currency, which `account_currency`
/// looks up (`None` for an account that isn't in the book).
fn assemble(
    id: TransactionId,
    booking_date: &str,
    payee: Option<String>,
    currency: &str,
    rows: Vec<SplitRow>,
    account_currency: impl Fn(AccountId) -> Result<Option<CurrencyCode>, StoreError>,
) -> Result<Transaction, StoreError> {
    let booking_date = date_from_text(booking_date)?;
    let currency = currency_from_text(currency)?;
    let splits = rows
        .into_iter()
        .map(|row| row.into_split(currency))
        .collect::<Result<Vec<_>, _>>()?;
    let transaction = Transaction::new(id, booking_date, payee, splits).map_err(|error| {
        StoreError::Corrupt(Corruption::InvalidTransaction {
            transaction: id,
            error,
        })
    })?;
    for split in transaction.splits() {
        let Some(account) = split.account else {
            continue;
        };
        let account_currency =
            account_currency(account)?.ok_or(StoreError::Corrupt(Corruption::BrokenReferences))?;
        if account_currency != split.account_amount().currency() {
            return Err(StoreError::Corrupt(Corruption::CurrencyMismatch {
                transaction: id,
            }));
        }
    }
    Ok(transaction)
}

/// Fails if `changed` changes a part of `stored` that a bank or file
/// delivered, or claims that a part was delivered that wasn't (D-067).
fn check_imported_unchanged(stored: &Transaction, changed: &Transaction) -> Result<(), StoreError> {
    for old in stored.splits().iter().filter(|old| old.imported.is_some()) {
        let kept = changed.splits().iter().any(|new| {
            new.id == old.id
                && new.account == old.account
                && new.account_amount() == old.account_amount()
                && new.imported == old.imported
        });
        if !kept {
            return Err(StoreError::ImportedSplitChanged(old.id));
        }
    }
    for new in changed.splits().iter().filter(|new| new.imported.is_some()) {
        let delivered = stored
            .splits()
            .iter()
            .any(|old| old.id == new.id && old.imported.is_some());
        if !delivered {
            return Err(StoreError::ImportedSplitChanged(new.id));
        }
    }
    Ok(())
}

/// Fails if `changed` changes the original amount of a part of `stored`, its
/// rate or the amount it was converted into, or gives a part an original
/// amount it didn't have (D-104).
fn check_originals_unchanged(
    stored: &Transaction,
    changed: &Transaction,
) -> Result<(), StoreError> {
    let converted = |split: &Split| {
        split.original.map(|original| {
            if original.rate.from == split.amount.currency() {
                split.amount
            } else {
                split.account_amount()
            }
        })
    };
    for old in stored.splits().iter().filter(|old| old.original.is_some()) {
        let kept = changed.splits().iter().any(|new| {
            new.id == old.id && new.original == old.original && converted(new) == converted(old)
        });
        if !kept {
            return Err(StoreError::OriginalChanged(old.id));
        }
    }
    for new in changed.splits().iter().filter(|new| new.original.is_some()) {
        let had = stored
            .splits()
            .iter()
            .any(|old| old.id == new.id && old.original == new.original);
        if !had {
            return Err(StoreError::OriginalChanged(new.id));
        }
    }
    Ok(())
}

/// What `transaction` books to each account, in the account's currency.
fn account_totals(transaction: &Transaction) -> Result<HashMap<AccountId, Money>, StoreError> {
    let mut totals: HashMap<AccountId, Money> = HashMap::new();
    for split in transaction.splits() {
        let Some(account) = split.account else {
            continue;
        };
        let amount = split.account_amount();
        let total = match totals.get(&account) {
            Some(total) => total
                .checked_add(amount)
                .map_err(|_| StoreError::AmountOverflow)?,
            None => amount,
        };
        totals.insert(account, total);
    }
    Ok(totals)
}

/// Writes the parts of `transaction`, with their rates and imported details.
pub(crate) fn insert_splits(
    conn: &Connection,
    transaction: &Transaction,
) -> Result<(), StoreError> {
    let mut insert = conn.prepare(
        "INSERT INTO splits
             (id, transaction_id, position, account_id, amount, foreign_amount, foreign_currency,
              original_amount, original_currency)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    let mut insert_imported = conn.prepare(
        "INSERT INTO split_imports
             (split_id, bank_id, booking_date, value_date,
              other_party_name, other_party_iban, remittance_information)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let optional_date = |date: Option<Date>| date.map(date_to_text).transpose();
    for (position, split) in (0_i64..).zip(transaction.splits()) {
        let id = split.id.to_bytes();
        insert.execute(params![
            id,
            transaction.id().to_bytes(),
            position,
            split.account.map(AccountId::to_bytes),
            split.amount.minor_units(),
            split.foreign.map(|foreign| foreign.amount.minor_units()),
            split
                .foreign
                .map(|foreign| foreign.amount.currency().as_str().to_owned()),
            split.original.map(|original| original.amount.minor_units()),
            split
                .original
                .map(|original| original.amount.currency().as_str().to_owned()),
        ])?;
        if let Some(rate) = split_rate(split) {
            insert_split_rate(conn, split.id, &rate)?;
        }
        if let Some(imported) = &split.imported {
            insert_imported.execute(params![
                id,
                imported.bank_id,
                optional_date(imported.booking_date)?,
                optional_date(imported.value_date)?,
                imported.other_party_name,
                imported.other_party_iban,
                imported.remittance_information,
            ])?;
        }
    }
    Ok(())
}

/// The one rate a part keeps: the fixed one its original amount was
/// converted with, or else the one its foreign amount was (D-104).
pub(crate) fn split_rate(split: &Split) -> Option<DatedRate> {
    split
        .original
        .map(|original| original.rate)
        .or(split.foreign.and_then(|foreign| foreign.rate))
}

/// Writes the copy of the rate the part `split` was converted with.
pub(crate) fn insert_split_rate(
    conn: &Connection,
    split: SplitId,
    rate: &DatedRate,
) -> Result<(), StoreError> {
    conn.prepare(
        "INSERT INTO split_rates
             (split_id, rate_id, date, from_currency, to_currency,
              rate_digits, rate_decimal_places, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?
    .execute(params![
        split.to_bytes(),
        rate.id.to_bytes(),
        date_to_text(rate.date)?,
        rate.from.as_str(),
        rate.to.as_str(),
        rate.rate.digits(),
        rate.rate.decimal_places(),
        source_to_text(rate.source),
    ])?;
    Ok(())
}

/// The amount a row of `splits` books to `account`, in its currency, read
/// from five columns starting at `first`: the transaction's currency, the
/// part's ID, its amount, its foreign amount and its foreign currency.
fn account_amount(
    account: &Account,
    transaction: TransactionId,
    row: &rusqlite::Row<'_>,
    first: usize,
) -> Result<Money, StoreError> {
    let split = SplitId::from_bytes(row.get(first + 1)?);
    let foreign = foreign_columns(split, row.get(first + 3)?, row.get(first + 4)?)?;
    let amount = match foreign {
        Some(foreign) => foreign,
        None => Money::from_minor_units(
            row.get(first + 2)?,
            currency_from_text(&row.get::<_, String>(first)?)?,
        ),
    };
    if amount.currency() != account.currency {
        return Err(StoreError::Corrupt(Corruption::CurrencyMismatch {
            transaction,
        }));
    }
    Ok(amount)
}

/// A part's foreign or original amount from its two columns, which are both
/// set or both empty.
fn foreign_columns(
    split: SplitId,
    amount: Option<i64>,
    currency: Option<String>,
) -> Result<Option<Money>, StoreError> {
    match (amount, currency) {
        (Some(amount), Some(currency)) => Ok(Some(Money::from_minor_units(
            amount,
            currency_from_text(&currency)?,
        ))),
        (None, None) => Ok(None),
        _ => Err(StoreError::Corrupt(Corruption::BadSplit(split))),
    }
}

/// The columns [`SplitRow::read`] expects, in its order, followed by
/// [`IMPORTED_COLUMNS`]: [`SPLIT_ROW_COLUMNS`] together.
const SPLIT_COLUMNS: &str = "
    s.id, s.account_id, s.amount, s.foreign_amount, s.foreign_currency,
    s.original_amount, s.original_currency, r.rate_id, r.date, r.from_currency, r.to_currency, r.rate_digits,
    r.rate_decimal_places, r.source";

/// The columns of `split_imports` (as `i`) [`ImportedRow::read`] expects, in
/// its order.
const IMPORTED_COLUMNS: &str = "
    i.split_id, i.bank_id, i.booking_date, i.value_date,
    i.other_party_name, i.other_party_iban, i.remittance_information";

/// How many columns [`SplitRow::read`] reads.
const SPLIT_ROW_COLUMNS: usize = 21;

/// The tables behind [`SPLIT_COLUMNS`].
const SPLIT_TABLES: &str = "
    FROM splits AS s
    LEFT JOIN split_rates AS r ON r.split_id = s.id
    LEFT JOIN split_imports AS i ON i.split_id = s.id";

/// A part as SQLite returns it, before KassenSturz's checks.
struct SplitRow {
    id: SplitId,
    account: Option<[u8; 16]>,
    amount: i64,
    foreign_amount: Option<i64>,
    foreign_currency: Option<String>,
    original_amount: Option<i64>,
    original_currency: Option<String>,
    rate: Option<RateRow>,
    imported: Option<ImportedRow>,
}

/// What a bank or file delivered for a part, as SQLite returns it.
struct ImportedRow {
    bank_id: Option<String>,
    booking_date: Option<String>,
    value_date: Option<String>,
    other_party_name: Option<String>,
    other_party_iban: Option<String>,
    remittance_information: Option<String>,
}

impl SplitRow {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let rate = match row.get::<_, Option<[u8; 16]>>(7)? {
            Some(id) => Some(RateRow {
                id: DatedRateId::from_bytes(id),
                date: row.get(8)?,
                from: row.get(9)?,
                to: row.get(10)?,
                digits: row.get(11)?,
                decimal_places: row.get(12)?,
                source: row.get(13)?,
            }),
            None => None,
        };
        let imported = ImportedRow::read(row, 14)?;
        Ok(Self {
            id: SplitId::from_bytes(row.get(0)?),
            account: row.get(1)?,
            amount: row.get(2)?,
            foreign_amount: row.get(3)?,
            foreign_currency: row.get(4)?,
            original_amount: row.get(5)?,
            original_currency: row.get(6)?,
            rate,
            imported,
        })
    }

    /// The part, in a transaction in `currency`.
    fn into_split(self, currency: CurrencyCode) -> Result<Split, StoreError> {
        let bad = || StoreError::Corrupt(Corruption::BadSplit(self.id));
        let foreign_amount = foreign_columns(self.id, self.foreign_amount, self.foreign_currency)?;
        let original_amount =
            foreign_columns(self.id, self.original_amount, self.original_currency)?;
        let mut rate = self.rate.map(RateRow::into_rate).transpose()?;
        // The part's one rate is its original amount's, if it has one: the
        // fixed rate it was converted with, which it can't be without.
        let original = match original_amount {
            Some(amount) => Some(OriginalAmount {
                amount,
                rate: rate.take().ok_or_else(bad)?,
            }),
            None => None,
        };
        let foreign = match (foreign_amount, rate) {
            (Some(amount), rate) => Some(ForeignAmount { amount, rate }),
            (None, None) => None,
            // A rate belongs to a part in another currency.
            (None, Some(_)) => return Err(bad()),
        };
        let imported = self.imported.map(ImportedRow::into_details).transpose()?;
        Ok(Split {
            id: self.id,
            account: self.account.map(AccountId::from_bytes),
            amount: Money::from_minor_units(self.amount, currency),
            foreign,
            imported,
            original,
        })
    }
}

impl ImportedRow {
    /// What was delivered for a part, from the columns of
    /// [`IMPORTED_COLUMNS`] starting at `first`, or `None` if nothing was.
    fn read(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Option<Self>> {
        if row.get::<_, Option<[u8; 16]>>(first)?.is_none() {
            return Ok(None);
        }
        Ok(Some(Self {
            bank_id: row.get(first + 1)?,
            booking_date: row.get(first + 2)?,
            value_date: row.get(first + 3)?,
            other_party_name: row.get(first + 4)?,
            other_party_iban: row.get(first + 5)?,
            remittance_information: row.get(first + 6)?,
        }))
    }

    fn into_details(self) -> Result<ImportedDetails, StoreError> {
        let optional_date = |text: Option<String>| text.as_deref().map(date_from_text).transpose();
        Ok(ImportedDetails {
            bank_id: self.bank_id,
            booking_date: optional_date(self.booking_date)?,
            value_date: optional_date(self.value_date)?,
            other_party_name: self.other_party_name,
            other_party_iban: self.other_party_iban,
            remittance_information: self.remittance_information,
        })
    }
}

#[cfg(test)]
mod tests;
