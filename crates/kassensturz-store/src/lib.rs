// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! All database code for KassenSturz's household books.
//!
//! A book is one SQLite file (D-033) with the extension `.ksz` (D-037). No
//! other crate touches a book's database; they call the functions here
//! (D-072). The layout is documented in `docs/BOOK_FORMAT.md` (D-046).

#![forbid(unsafe_code)]

mod backup;
mod error;
mod layout;
mod lock;

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, Currency, CurrencyCode, Date, DatedRate,
    DatedRateId, ExchangeRate, Money, RateSource, RoundingMode, Split, SplitId, Transaction,
    TransactionId,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

pub use error::{Corruption, DatabaseError, StoreError};
pub use lock::Lock;

use lock::HeldLock;

/// What opening a book needs to know about the computer it runs on. Finding
/// these differs between Plasma, GNOME, Windows and macOS, so the front end
/// fills them in, through `kassensturz-platform` (`docs/ARCHITECTURE.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Computer {
    /// The computer's name, e.g. "Laptop". While a book is open here, other
    /// computers are told it's open on this one (D-038).
    pub name: String,
    /// The folder for automatic backups, in KassenSturz's own data folder
    /// (D-033). It is created when the first backup is made.
    pub backup_folder: PathBuf,
}

/// An open household book.
pub struct Book {
    conn: Connection,
    /// The book's lock file, removed when the book is closed. Declared after
    /// `conn`, so the database is closed first. Books in memory have none.
    _lock: Option<HeldLock>,
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
    /// Creates a new, empty book at `path` and opens it on `computer`.
    ///
    /// Fails with [`StoreError::AlreadyExists`] if something is at `path`
    /// already, and never touches an existing file.
    pub fn create(path: &Path, computer: &Computer) -> Result<Self, StoreError> {
        let path = std::path::absolute(path)?;
        // Claim the path first: `create_new` fails if anything exists there,
        // so SQLite can never open an existing file by mistake.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| match error.kind() {
                ErrorKind::AlreadyExists => StoreError::AlreadyExists,
                kind => StoreError::Io(kind),
            })?;
        let book = Self::create_at(&path, computer);
        if book.is_err() {
            // Don't leave a half-made book behind. If removing it fails too,
            // the error that stopped the creation is the one worth reporting.
            let _ = std::fs::remove_file(&path);
        }
        book
    }

    fn create_at(path: &Path, computer: &Computer) -> Result<Self, StoreError> {
        let lock = HeldLock::acquire(path, &computer.name)?;
        let mut conn = connect(path)?;
        layout::migrate(&mut conn, 0)?;
        Self::finish(conn, Some(lock))
    }

    /// Opens the existing book at `path` on `computer`.
    ///
    /// Fails with [`StoreError::Locked`] if the book is open somewhere else
    /// (D-038). A book with an older layout is updated to the current one,
    /// after a backup into `computer`'s backup folder (D-033).
    pub fn open(path: &Path, computer: &Computer) -> Result<Self, StoreError> {
        Self::open_with(path, computer, |book| {
            HeldLock::acquire(book, &computer.name)
        })
    }

    /// Opens the book at `path` like [`Book::open`], although its lock file
    /// says it is open somewhere else, after the user was warned about that
    /// lock, `seen` (D-038). The book is then locked for `computer`.
    ///
    /// Fails with [`StoreError::Locked`] and the current lock if it changed
    /// since, e.g. because the book was opened on yet another computer, so
    /// the user can be warned again.
    pub fn open_anyway(path: &Path, computer: &Computer, seen: &Lock) -> Result<Self, StoreError> {
        Self::open_with(path, computer, |book| {
            HeldLock::take_over(book, &computer.name, seen)
        })
    }

    fn open_with(
        path: &Path,
        computer: &Computer,
        lock: impl FnOnce(&Path) -> Result<HeldLock, StoreError>,
    ) -> Result<Self, StoreError> {
        let path = std::path::absolute(path)?;
        let metadata = std::fs::metadata(&path).map_err(|error| match error.kind() {
            ErrorKind::NotFound => StoreError::NotFound,
            kind => StoreError::Io(kind),
        })?;
        // Before SQLite opens the file, so another program's SQLite file is
        // never changed, not even its journal mode.
        if !metadata.is_file() || !is_book_file(&path)? {
            return Err(StoreError::NotABook);
        }
        let lock = lock(&path)?;
        let mut conn = connect(&path)?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            layout::VERSION => {}
            older if (layout::FIRST_VERSION..layout::VERSION).contains(&older) => {
                backup::back_up(&conn, &path, &computer.backup_folder)
                    .map_err(|error| StoreError::BackupFailed(Box::new(error)))?;
                layout::migrate(&mut conn, older)?;
            }
            found => return Err(StoreError::UnsupportedLayout { found }),
        }
        Self::finish(conn, Some(lock))
    }

    /// Creates a new, empty book that lives in memory only: nothing is saved.
    pub fn create_in_memory() -> Result<Self, StoreError> {
        let mut conn = Connection::open_in_memory()?;
        layout::migrate(&mut conn, 0)?;
        Self::finish(conn, None)
    }

    fn finish(conn: Connection, lock: Option<HeldLock>) -> Result<Self, StoreError> {
        // SQLite checks foreign keys only if asked to, on every connection.
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok(Self { conn, _lock: lock })
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

    /// `currency` with the decimal places this book stores for it, as the
    /// conversions with an exchange rate need it
    /// ([`Money::checked_mul_rate`]).
    pub fn currency(&self, currency: CurrencyCode) -> Result<Currency, StoreError> {
        let minor_digits = self.currency_minor_digits(currency)?;
        Ok(Currency::new(currency, minor_digits).expect("checked when read"))
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
                    .and_then(|minor_digits| Currency::new(currency, minor_digits))
                    .map(Currency::minor_digits)
                    .ok_or(StoreError::Corrupt(Corruption::BadMinorDigits(digits)))
            })
            .transpose()
    }

    /// Adds an account or category. Its currency must be in use in this book.
    ///
    /// The name can't be empty, and an account's type must fit its kind (a
    /// credit card is money the user owes). No two accounts share a name,
    /// ignoring capitals, closed ones included, so the user can always tell
    /// them apart. Categories are checked when they become a tree (M1.5).
    pub fn add_account(&mut self, account: &Account) -> Result<(), StoreError> {
        self.currency_minor_digits(account.currency)?;
        self.check_account_fields(
            account.id,
            &account.name,
            account.kind,
            account.account_type,
        )?;
        self.conn.execute(
            "INSERT INTO accounts (id, name, kind, currency, account_type, closed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                account.id.to_bytes(),
                account.name,
                kind_to_text(account.kind),
                account.currency.as_str(),
                account.account_type.map(type_to_text),
                account.closed,
            ],
        )?;
        Ok(())
    }

    /// The checks `add_account` and `rename_account` share.
    fn check_account_fields(
        &self,
        id: AccountId,
        name: &str,
        kind: AccountKind,
        account_type: Option<AccountType>,
    ) -> Result<(), StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::EmptyAccountName);
        }
        if let Some(account_type) = account_type
            && account_type.kind() != kind
        {
            return Err(StoreError::AccountTypeMismatch { account_type, kind });
        }
        if kind.is_account() {
            let wanted = name.trim().to_lowercase();
            if let Some(existing) = self.all_accounts()?.into_iter().find(|other| {
                other.id != id
                    && other.kind.is_account()
                    && other.name.trim().to_lowercase() == wanted
            }) {
                return Err(StoreError::DuplicateAccountName {
                    existing: existing.id,
                });
            }
        }
        Ok(())
    }

    /// The account or category with this ID.
    pub fn account(&self, id: AccountId) -> Result<Account, StoreError> {
        self.conn
            .query_row(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?1"),
                [id.to_bytes()],
                AccountRow::read,
            )
            .optional()?
            .ok_or(StoreError::UnknownAccount(id))?
            .into_account()
    }

    /// The user's accounts (not categories), sorted by name. Closed accounts
    /// are left out unless `include_closed` is set.
    pub fn accounts(&self, include_closed: bool) -> Result<Vec<Account>, StoreError> {
        let mut accounts: Vec<Account> = self
            .all_accounts()?
            .into_iter()
            .filter(|account| account.kind.is_account() && (include_closed || !account.closed))
            .collect();
        accounts.sort_by_cached_key(|account| (account.name.to_lowercase(), account.id));
        Ok(accounts)
    }

    fn all_accounts(&self) -> Result<Vec<Account>, StoreError> {
        let mut statement = self
            .conn
            .prepare(&format!("SELECT {ACCOUNT_COLUMNS} FROM accounts"))?;
        let rows = statement
            .query_map([], AccountRow::read)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(AccountRow::into_account).collect()
    }

    /// Gives an account or category a new name, under the same rules as
    /// [`Book::add_account`].
    pub fn rename_account(&mut self, id: AccountId, name: &str) -> Result<(), StoreError> {
        let account = self.account(id)?;
        self.check_account_fields(id, name, account.kind, account.account_type)?;
        self.conn.execute(
            "UPDATE accounts SET name = ?2 WHERE id = ?1",
            params![id.to_bytes(), name],
        )?;
        Ok(())
    }

    /// Closes an account: it keeps its history, but takes no new
    /// transactions and drops out of [`Book::accounts`]. Its balance must be
    /// zero (otherwise [`StoreError::AccountNotEmpty`]), so no money is
    /// hidden. Closing a closed account does nothing.
    pub fn close_account(&mut self, id: AccountId) -> Result<(), StoreError> {
        let account = self.account(id)?;
        if !account.kind.is_account() {
            return Err(StoreError::NotAnAccount(id));
        }
        let balance = self.balance(id)?;
        if balance.minor_units() != 0 {
            return Err(StoreError::AccountNotEmpty {
                account: id,
                balance,
            });
        }
        self.set_closed(id, true)
    }

    /// Opens a closed account again. Opening an open account does nothing.
    pub fn reopen_account(&mut self, id: AccountId) -> Result<(), StoreError> {
        self.account(id)?;
        self.set_closed(id, false)
    }

    fn set_closed(&mut self, id: AccountId, closed: bool) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE accounts SET closed = ?2 WHERE id = ?1",
            params![id.to_bytes(), closed],
        )?;
        Ok(())
    }

    /// Adds a transaction. Every part must be booked to an existing account in
    /// the transaction's currency.
    pub fn add_transaction(&mut self, transaction: &Transaction) -> Result<(), StoreError> {
        let booking_date = date_to_text(transaction.booking_date())?;
        let currency = transaction.currency();
        self.currency_minor_digits(currency)?;
        for split in transaction.splits() {
            let account = self.account(split.account)?;
            if account.closed {
                return Err(StoreError::AccountClosed(account.id));
            }
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

    /// Adds an exchange rate to the book's price table (D-028).
    ///
    /// Both currencies must be in use in this book and differ. A book holds
    /// one rate per pair of currencies, direction and date; adding a second
    /// fails with [`StoreError::DuplicateRate`], so the user changes or
    /// removes the first one instead of having two that disagree.
    pub fn add_rate(&mut self, rate: &DatedRate) -> Result<(), StoreError> {
        if rate.from == rate.to {
            return Err(StoreError::RateWithinOneCurrency(rate.from));
        }
        self.currency_minor_digits(rate.from)?;
        self.currency_minor_digits(rate.to)?;
        let date = date_to_text(rate.date)?;
        let existing: Option<[u8; 16]> = self
            .conn
            .query_row(
                "SELECT id FROM prices
                 WHERE from_currency = ?1 AND to_currency = ?2 AND date = ?3",
                params![rate.from.as_str(), rate.to.as_str(), date],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            return Err(StoreError::DuplicateRate {
                existing: DatedRateId::from_bytes(existing),
            });
        }
        self.conn.execute(
            "INSERT INTO prices
                 (id, date, from_currency, to_currency, rate_digits, rate_decimal_places, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rate.id.to_bytes(),
                date,
                rate.from.as_str(),
                rate.to.as_str(),
                rate.rate.digits(),
                rate.rate.decimal_places(),
                source_to_text(rate.source),
            ],
        )?;
        Ok(())
    }

    /// Removes an exchange rate from the price table. Amounts converted with
    /// it keep their values: each conversion stores the amounts it produced
    /// (D-043).
    pub fn remove_rate(&mut self, id: DatedRateId) -> Result<(), StoreError> {
        let removed = self
            .conn
            .execute("DELETE FROM prices WHERE id = ?1", [id.to_bytes()])?;
        if removed == 1 {
            Ok(())
        } else {
            Err(StoreError::UnknownRate(id))
        }
    }

    /// The exchange rate with this ID.
    pub fn rate(&self, id: DatedRateId) -> Result<DatedRate, StoreError> {
        self.conn
            .query_row(
                &format!("SELECT {RATE_COLUMNS} FROM prices WHERE id = ?1"),
                [id.to_bytes()],
                RateRow::read,
            )
            .optional()?
            .ok_or(StoreError::UnknownRate(id))?
            .into_rate()
    }

    /// Every rate from `from` into `to`, oldest first. Rates the other way,
    /// from `to` into `from`, aren't included.
    pub fn rates(
        &self,
        from: CurrencyCode,
        to: CurrencyCode,
    ) -> Result<Vec<DatedRate>, StoreError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {RATE_COLUMNS} FROM prices
             WHERE from_currency = ?1 AND to_currency = ?2
             ORDER BY date"
        ))?;
        let rows = statement
            .query_map([from.as_str(), to.as_str()], RateRow::read)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(RateRow::into_rate).collect()
    }

    /// The rate from `from` into `to` that applies on `date`: the newest one
    /// on or before it. `None` if there's none yet.
    ///
    /// Only rates in this direction count. A conversion the other way divides
    /// by the rate ([`Money::checked_div_rate`]), so the caller decides which
    /// direction to look up.
    pub fn rate_on(
        &self,
        from: CurrencyCode,
        to: CurrencyCode,
        date: Date,
    ) -> Result<Option<DatedRate>, StoreError> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {RATE_COLUMNS} FROM prices
                     WHERE from_currency = ?1 AND to_currency = ?2 AND date <= ?3
                     ORDER BY date DESC LIMIT 1"
                ),
                params![from.as_str(), to.as_str(), date_to_text(date)?],
                RateRow::read,
            )
            .optional()?;
        row.map(RateRow::into_rate).transpose()
    }
}

/// Opens the SQLite database of the book at `path`, which must exist.
fn connect(path: &Path) -> Result<Connection, StoreError> {
    // `path` is absolute, so it never begins with `file:` and SQLite never
    // takes it for a URI, as the bundled SQLite otherwise would whatever the
    // flags say. Without SQLITE_OPEN_CREATE, SQLite never creates a file.
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    // The classic rollback journal, not WAL, so no lasting side files sit
    // next to the book, e.g. in a sync folder (D-033).
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(StoreError::JournalModeRefused { mode });
    }
    Ok(conn)
}

/// Whether the file at `path` begins like a book, read without SQLite: with
/// SQLite's header and KassenSturz's application ID (D-037).
fn is_book_file(path: &Path) -> Result<bool, StoreError> {
    // The header's first 16 bytes name the format, and bytes 68 to 71 hold
    // the application ID, most significant byte first
    // (sqlite.org/fileformat.html).
    let mut header = [0; 72];
    match File::open(path)?.read_exact(&mut header) {
        Ok(()) => Ok(header[..16] == *b"SQLite format 3\0"
            && header[68..] == layout::APPLICATION_ID.to_be_bytes()),
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// The columns [`AccountRow::read`] expects, in its order.
const ACCOUNT_COLUMNS: &str = "id, name, kind, currency, account_type, closed";

/// A row of the accounts table as SQLite returns it, before KassenSturz's
/// checks.
struct AccountRow {
    id: AccountId,
    name: String,
    kind: String,
    currency: String,
    account_type: Option<String>,
    closed: i64,
}

impl AccountRow {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: AccountId::from_bytes(row.get(0)?),
            name: row.get(1)?,
            kind: row.get(2)?,
            currency: row.get(3)?,
            account_type: row.get(4)?,
            closed: row.get(5)?,
        })
    }

    fn into_account(self) -> Result<Account, StoreError> {
        let kind = kind_from_text(&self.kind)?;
        let account_type = self
            .account_type
            .as_deref()
            .map(type_from_text)
            .transpose()?;
        if account_type.is_some_and(|account_type| account_type.kind() != kind) {
            return Err(StoreError::Corrupt(Corruption::AccountTypeMismatch(
                self.id,
            )));
        }
        Ok(Account {
            id: self.id,
            name: self.name,
            kind,
            currency: currency_from_text(&self.currency)?,
            account_type,
            closed: self.closed != 0,
        })
    }
}

/// The columns [`RateRow::read`] expects, in its order.
const RATE_COLUMNS: &str =
    "id, date, from_currency, to_currency, rate_digits, rate_decimal_places, source";

/// A row of the price table as SQLite returns it, before KassenSturz's
/// checks.
struct RateRow {
    id: DatedRateId,
    date: String,
    from: String,
    to: String,
    digits: i64,
    decimal_places: i64,
    source: String,
}

impl RateRow {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: DatedRateId::from_bytes(row.get(0)?),
            date: row.get(1)?,
            from: row.get(2)?,
            to: row.get(3)?,
            digits: row.get(4)?,
            decimal_places: row.get(5)?,
            source: row.get(6)?,
        })
    }

    fn into_rate(self) -> Result<DatedRate, StoreError> {
        let rate = u8::try_from(self.decimal_places)
            .ok()
            .and_then(|places| ExchangeRate::from_digits(self.digits, places).ok())
            .ok_or(StoreError::Corrupt(Corruption::BadRate(self.id)))?;
        Ok(DatedRate {
            id: self.id,
            date: date_from_text(&self.date)?,
            from: currency_from_text(&self.from)?,
            to: currency_from_text(&self.to)?,
            rate,
            source: source_from_text(&self.source)?,
        })
    }
}

fn source_to_text(source: RateSource) -> &'static str {
    match source {
        RateSource::Manual => "manual",
    }
}

fn source_from_text(text: &str) -> Result<RateSource, StoreError> {
    match text {
        "manual" => Ok(RateSource::Manual),
        other => Err(StoreError::Corrupt(Corruption::BadRateSource(other.into()))),
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

fn type_to_text(account_type: AccountType) -> &'static str {
    match account_type {
        AccountType::Checking => "checking",
        AccountType::Savings => "savings",
        AccountType::Cash => "cash",
        AccountType::CreditCard => "credit_card",
    }
}

fn type_from_text(text: &str) -> Result<AccountType, StoreError> {
    AccountType::ALL
        .into_iter()
        .find(|&account_type| type_to_text(account_type) == text)
        .ok_or_else(|| StoreError::Corrupt(Corruption::BadAccountType(text.into())))
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
mod test_folder;
#[cfg(test)]
mod tests;
