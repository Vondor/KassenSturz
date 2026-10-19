// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! All database code for KassenSturz's household books.
//!
//! A book is one SQLite file (D-033) with the extension `.ksz` (D-037). No
//! other crate touches a book's database; they call the functions here
//! (D-072). The layout is documented in `docs/BOOK_FORMAT.md` (D-046).

#![forbid(unsafe_code)]

mod error;
mod layout;

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::Path;

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, RoundingMode, Split, SplitId,
    Transaction, TransactionId,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

pub use error::{Corruption, DatabaseError, StoreError};

/// An open household book.
pub struct Book {
    conn: Connection,
}

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
    /// What it did to this account: positive amounts came in, negative ones
    /// went out.
    pub amount: Money,
}

impl Book {
    /// Creates a new, empty book at `path`.
    ///
    /// Fails with [`StoreError::AlreadyExists`] if something is at `path`
    /// already, and never touches an existing file.
    pub fn create(path: &Path) -> Result<Self, StoreError> {
        // Claim the path first: `create_new` fails if anything exists there,
        // so SQLite can never open an existing file by mistake.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| match error.kind() {
                ErrorKind::AlreadyExists => StoreError::AlreadyExists,
                kind => StoreError::Io(kind),
            })?;
        let book = Self::connect(path).and_then(|mut book| {
            book.create_layout()?;
            Ok(book)
        });
        if book.is_err() {
            // Don't leave a half-made book behind. If removing it fails too,
            // the error that stopped the creation is the one worth reporting.
            let _ = std::fs::remove_file(path);
        }
        book
    }

    /// Opens the existing book at `path`.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        std::fs::metadata(path).map_err(|error| match error.kind() {
            ErrorKind::NotFound => StoreError::NotFound,
            kind => StoreError::Io(kind),
        })?;
        let book = Self::connect(path)?;
        book.check_layout()?;
        Ok(book)
    }

    /// Creates a new, empty book that lives in memory only: nothing is saved.
    pub fn create_in_memory() -> Result<Self, StoreError> {
        let mut book = Self::configure(Connection::open_in_memory()?)?;
        book.create_layout()?;
        Ok(book)
    }

    fn connect(path: &Path) -> Result<Self, StoreError> {
        // Without SQLITE_OPEN_URI a path is always a file name, never a URI,
        // and without SQLITE_OPEN_CREATE SQLite never creates a file.
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let book = Self::configure(Connection::open_with_flags(path, flags)?)?;
        // The classic rollback journal, not WAL, so no lasting side files sit
        // next to the book, e.g. in a sync folder (D-033).
        let mode: String =
            book.conn
                .pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("delete") {
            return Err(StoreError::JournalModeRefused { mode });
        }
        Ok(book)
    }

    fn configure(conn: Connection) -> Result<Self, StoreError> {
        // SQLite checks foreign keys only if asked to, on every connection.
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok(Self { conn })
    }

    fn create_layout(&mut self) -> Result<(), StoreError> {
        let transaction = self.conn.transaction()?;
        transaction.execute_batch(layout::CREATE)?;
        transaction.execute(
            "INSERT INTO settings (id, rounding_mode) VALUES (1, ?1)",
            [rounding_mode_to_text(RoundingMode::default())],
        )?;
        transaction.pragma_update(None, "user_version", layout::VERSION)?;
        transaction.commit()?;
        Ok(())
    }

    fn check_layout(&self) -> Result<(), StoreError> {
        let version: i32 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            layout::VERSION => Ok(()),
            // Other programs' SQLite files usually leave user_version at 0.
            // KassenSturz's own application ID comes with M1 (D-037).
            0 => Err(StoreError::NotABook),
            found => Err(StoreError::UnsupportedLayout { found }),
        }
    }

    /// The rounding mode of this book's calculations (D-027). New books round
    /// half away from zero, [`RoundingMode`]'s default.
    pub fn rounding_mode(&self) -> Result<RoundingMode, StoreError> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT rounding_mode FROM settings WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        rounding_mode_from_text(&text.ok_or(StoreError::Corrupt(Corruption::MissingSettings))?)
    }

    /// Sets the rounding mode of this book's calculations.
    ///
    /// It applies only to calculations from now on: amounts already stored
    /// are never recalculated (D-027).
    pub fn set_rounding_mode(&mut self, rounding: RoundingMode) -> Result<(), StoreError> {
        let changed = self.conn.execute(
            "UPDATE settings SET rounding_mode = ?1 WHERE id = 1",
            [rounding_mode_to_text(rounding)],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(StoreError::Corrupt(Corruption::MissingSettings))
        }
    }

    /// Starts using `currency` in this book, unless it's in use already, and
    /// returns its number of decimal places (2 for the euro).
    ///
    /// The first time, the decimal places come from KassenSturz's ISO 4217
    /// table (D-062) and are stored in the book. From then on the stored ones
    /// count, even if a newer table says otherwise, so amounts are never
    /// rescaled (D-027). A currency the table doesn't list can't be used.
    pub fn use_currency(&mut self, currency: CurrencyCode) -> Result<u8, StoreError> {
        if let Some(stored) = self.stored_minor_digits(currency)? {
            return Ok(stored);
        }
        let minor_digits = currency
            .iso_minor_digits()
            .ok_or(StoreError::UnlistedCurrency(currency))?;
        self.conn.execute(
            "INSERT INTO currencies (code, minor_digits) VALUES (?1, ?2)",
            params![currency.as_str(), minor_digits],
        )?;
        Ok(minor_digits)
    }

    /// How many decimal places `currency` has in this book.
    pub fn currency_minor_digits(&self, currency: CurrencyCode) -> Result<u8, StoreError> {
        self.stored_minor_digits(currency)?
            .ok_or(StoreError::UnknownCurrency(currency))
    }

    fn stored_minor_digits(&self, currency: CurrencyCode) -> Result<Option<u8>, StoreError> {
        let digits: Option<i64> = self
            .conn
            .query_row(
                "SELECT minor_digits FROM currencies WHERE code = ?1",
                [currency.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        digits
            .map(|digits| {
                u8::try_from(digits)
                    .ok()
                    .filter(|&digits| digits <= layout::MAX_MINOR_DIGITS)
                    .ok_or(StoreError::Corrupt(Corruption::BadMinorDigits(digits)))
            })
            .transpose()
    }

    /// Adds an account or category. Its currency must be in use in this book.
    pub fn add_account(&mut self, account: &Account) -> Result<(), StoreError> {
        self.currency_minor_digits(account.currency)?;
        self.conn.execute(
            "INSERT INTO accounts (id, name, kind, currency) VALUES (?1, ?2, ?3, ?4)",
            params![
                account.id.to_bytes(),
                account.name,
                kind_to_text(account.kind),
                account.currency.as_str()
            ],
        )?;
        Ok(())
    }

    /// The account or category with this ID.
    pub fn account(&self, id: AccountId) -> Result<Account, StoreError> {
        let row: Option<(String, String, String)> = self
            .conn
            .query_row(
                "SELECT name, kind, currency FROM accounts WHERE id = ?1",
                [id.to_bytes()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (name, kind, currency) = row.ok_or(StoreError::UnknownAccount(id))?;
        Ok(Account {
            id,
            name,
            kind: kind_from_text(&kind)?,
            currency: currency_from_text(&currency)?,
        })
    }

    /// Adds a transaction. Every part must be booked to an existing account in
    /// the transaction's currency.
    pub fn add_transaction(&mut self, transaction: &Transaction) -> Result<(), StoreError> {
        let booking_date = date_to_text(transaction.booking_date())?;
        let currency = transaction.currency();
        self.currency_minor_digits(currency)?;
        for split in transaction.splits() {
            let account = self.account(split.account)?;
            if account.currency != currency {
                return Err(StoreError::SplitCurrencyMismatch {
                    account: account.id,
                    account_currency: account.currency,
                    transaction_currency: currency,
                });
            }
        }

        let id = transaction.id().to_bytes();
        let sql = self.conn.transaction()?;
        sql.execute(
            "INSERT INTO transactions (id, booking_date, payee, currency) VALUES (?1, ?2, ?3, ?4)",
            params![id, booking_date, transaction.payee(), currency.as_str()],
        )?;
        {
            let mut insert = sql.prepare(
                "INSERT INTO splits (id, transaction_id, position, account_id, amount)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (position, split) in (0_i64..).zip(transaction.splits()) {
                insert.execute(params![
                    split.id.to_bytes(),
                    id,
                    position,
                    split.account.to_bytes(),
                    split.amount.minor_units()
                ])?;
            }
        }
        sql.commit()?;
        Ok(())
    }

    /// The transaction with this ID. It's checked again on the way out, so a
    /// transaction that doesn't balance is reported, never returned.
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
        let booking_date = date_from_text(&booking_date)?;
        let currency = currency_from_text(&currency)?;

        let mut statement = self.conn.prepare(
            "SELECT id, account_id, amount FROM splits WHERE transaction_id = ?1 ORDER BY position",
        )?;
        let splits = statement
            .query_map([id.to_bytes()], |row| {
                Ok(Split {
                    id: SplitId::from_bytes(row.get(0)?),
                    account: AccountId::from_bytes(row.get(1)?),
                    amount: Money::from_minor_units(row.get(2)?, currency),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Transaction::new(id, booking_date, payee, splits).map_err(|error| {
            StoreError::Corrupt(Corruption::InvalidTransaction {
                transaction: id,
                error,
            })
        })
    }

    /// The transactions that touch `account`, oldest first, each as seen from
    /// that account.
    pub fn register(&self, account: AccountId) -> Result<Vec<RegisterEntry>, StoreError> {
        let account = self.account(account)?;
        let mut statement = self.conn.prepare(
            "SELECT t.id, t.booking_date, t.payee, t.currency, s.amount
             FROM splits AS s JOIN transactions AS t ON t.id = s.transaction_id
             WHERE s.account_id = ?1
             ORDER BY t.booking_date, t.id, s.position",
        )?;
        let mut rows = statement.query([account.id.to_bytes()])?;
        let mut entries: Vec<RegisterEntry> = Vec::new();
        while let Some(row) = rows.next()? {
            let transaction = TransactionId::from_bytes(row.get(0)?);
            if currency_from_text(&row.get::<_, String>(3)?)? != account.currency {
                return Err(StoreError::Corrupt(Corruption::CurrencyMismatch {
                    transaction,
                }));
            }
            let amount = Money::from_minor_units(row.get(4)?, account.currency);
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

    /// The balance of `account`: everything booked to it, added up exactly.
    pub fn balance(&self, account: AccountId) -> Result<Money, StoreError> {
        let account = self.account(account)?;
        let mut statement = self
            .conn
            .prepare("SELECT amount FROM splits WHERE account_id = ?1")?;
        let amounts = statement
            .query_map([account.id.to_bytes()], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let amounts = amounts
            .into_iter()
            .map(|minor_units| Money::from_minor_units(minor_units, account.currency));
        Money::checked_sum(account.currency, amounts).map_err(|_| StoreError::AmountOverflow)
    }
}

fn kind_to_text(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Asset => "asset",
        AccountKind::Liability => "liability",
        AccountKind::Income => "income",
        AccountKind::Expense => "expense",
        AccountKind::Equity => "equity",
    }
}

fn kind_from_text(text: &str) -> Result<AccountKind, StoreError> {
    match text {
        "asset" => Ok(AccountKind::Asset),
        "liability" => Ok(AccountKind::Liability),
        "income" => Ok(AccountKind::Income),
        "expense" => Ok(AccountKind::Expense),
        "equity" => Ok(AccountKind::Equity),
        other => Err(StoreError::Corrupt(Corruption::BadAccountKind(
            other.into(),
        ))),
    }
}

fn rounding_mode_to_text(rounding: RoundingMode) -> &'static str {
    match rounding {
        RoundingMode::HalfAwayFromZero => "half_away_from_zero",
        RoundingMode::HalfEven => "half_even",
        RoundingMode::HalfTowardZero => "half_toward_zero",
        RoundingMode::TowardZero => "toward_zero",
        RoundingMode::AwayFromZero => "away_from_zero",
        RoundingMode::Floor => "floor",
        RoundingMode::Ceiling => "ceiling",
    }
}

fn rounding_mode_from_text(text: &str) -> Result<RoundingMode, StoreError> {
    RoundingMode::ALL
        .into_iter()
        .find(|&rounding| rounding_mode_to_text(rounding) == text)
        .ok_or_else(|| StoreError::Corrupt(Corruption::BadRoundingMode(text.into())))
}

fn currency_from_text(text: &str) -> Result<CurrencyCode, StoreError> {
    CurrencyCode::new(text)
        .map_err(|_| StoreError::Corrupt(Corruption::BadCurrencyCode(text.into())))
}

/// ISO 8601, e.g. `2026-10-18`: sorts correctly as text and other programs
/// can read it (D-061).
fn date_to_text(date: Date) -> Result<String, StoreError> {
    if date.year() < 1 {
        return Err(StoreError::DateOutOfRange(date));
    }
    Ok(date.to_string())
}

fn date_from_text(text: &str) -> Result<Date, StoreError> {
    text.parse()
        .map_err(|_| StoreError::Corrupt(Corruption::BadDate(text.into())))
}

#[cfg(test)]
mod tests;
