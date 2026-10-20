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
mod fill;
mod history;
mod layout;
mod lock;
mod rules;
mod transactions;
mod transfers;

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, Currency, CurrencyCode, Date, DatedRate,
    DatedRateId, ExchangeRate, RateSource, RoundingMode,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

pub use error::{Corruption, DatabaseError, StoreError};
pub use history::{Action, Change, ChangeId, HistoryRetention};
pub use lock::Lock;
pub use rules::{RuleChange, RuleMark, RuleRun, RuleSkip};
pub use transactions::{AccountPart, RegisterEntry};

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
///
/// Every change to it is recorded in its change history, one entry per user
/// action, and can be undone and redone (D-066, [`Book::undo`]).
pub struct Book {
    conn: Connection,
    /// Whether a user action is under way ([`Book::as_one_action`]).
    recording: bool,
    /// Whether a new book is being filled ([`Book::fill_new`]).
    filling: bool,
    /// The book's lock file, removed when the book is closed. Declared after
    /// `conn`, so the database is closed first. Books in memory have none.
    _lock: Option<HeldLock>,
}

/// One category in the category tree, as [`Book::categories`] lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryEntry {
    /// The category.
    pub category: Account,
    /// The names from the top-level category down to this one, e.g.
    /// `["Food", "Groceries"]`. Its length minus one is how deep it sits.
    pub path: Vec<String>,
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
        // SQLite overwrites what is deleted, so a change history that is
        // cleaned up or too old is gone from the file too (D-066).
        conn.pragma_update(None, "secure_delete", true)?;
        // The history's temporary tables are kept in memory, never in a file.
        // Set before they are made: changing it drops temporary tables.
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        // Without it, a row that a REPLACE deletes fires no trigger, so the
        // history would miss it. The history's triggers fire no others.
        conn.pragma_update(None, "recursive_triggers", true)?;
        history::prepare(&conn)?;
        let mut book = Self {
            conn,
            recording: false,
            filling: false,
            _lock: lock,
        };
        book.prune_history()?;
        Ok(book)
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
        self.as_one_action(Action::SetRoundingMode, |book| {
            let changed = book.conn.execute(
                "UPDATE settings SET rounding_mode = ?1 WHERE id = 1",
                [rounding_mode_to_text(rounding)],
            )?;
            if changed == 1 {
                Ok(())
            } else {
                Err(StoreError::Corrupt(Corruption::MissingSettings))
            }
        })
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
        self.as_one_action(Action::UseCurrency, |book| {
            book.conn.execute(
                "INSERT INTO currencies (code, minor_digits) VALUES (?1, ?2)",
                params![currency.as_str(), minor_digits],
            )?;
            Ok(minor_digits)
        })
    }

    /// How many decimal places `currency` has in this book.
    pub fn currency_minor_digits(&self, currency: CurrencyCode) -> Result<u8, StoreError> {
        self.stored_minor_digits(currency)?
            .ok_or(StoreError::UnknownCurrency(currency))
    }

    /// `currency` with the decimal places this book stores for it, as the
    /// conversions with an exchange rate need it
    /// ([`Money::checked_mul_rate`](kassensturz_core::Money::checked_mul_rate)).
    pub fn currency(&self, currency: CurrencyCode) -> Result<Currency, StoreError> {
        let minor_digits = self.currency_minor_digits(currency)?;
        Ok(Currency::new(currency, minor_digits).expect("checked when read"))
    }

    /// Every currency in use in this book, with the decimal places it stores
    /// for each, sorted by code.
    pub fn currencies(&self) -> Result<Vec<Currency>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT code, minor_digits FROM currencies ORDER BY code")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(code, digits)| {
                let code = currency_from_text(&code)?;
                u8::try_from(digits)
                    .ok()
                    .and_then(|minor_digits| Currency::new(code, minor_digits))
                    .ok_or(StoreError::Corrupt(Corruption::BadMinorDigits(digits)))
            })
            .collect()
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
        self.check_account(account)?;
        let action = if account.kind.is_category() {
            Action::AddCategory
        } else {
            Action::AddAccount
        };
        self.as_one_action(action, |book| {
            book.conn.execute(
                "INSERT INTO accounts (id, name, kind, currency, account_type, closed, parent_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    account.id.to_bytes(),
                    account.name,
                    kind_to_text(account.kind),
                    account.currency.as_str(),
                    account.account_type.map(type_to_text),
                    account.closed,
                    account.parent.map(AccountId::to_bytes),
                ],
            )?;
            Ok(())
        })
    }

    /// The checks `add_account`, `rename_account` and `move_category` share,
    /// on the account as it is going to be.
    fn check_account(&self, account: &Account) -> Result<(), StoreError> {
        if account.name.trim().is_empty() {
            return Err(StoreError::EmptyAccountName);
        }
        if let Some(account_type) = account.account_type
            && account_type.kind() != account.kind
        {
            return Err(StoreError::AccountTypeMismatch {
                account_type,
                kind: account.kind,
            });
        }
        let others: Vec<Account> = self
            .all_accounts()?
            .into_iter()
            .filter(|other| other.id != account.id)
            .collect();
        if let Some(parent) = account.parent {
            if !account.kind.is_category() {
                return Err(StoreError::NotACategory(account.id));
            }
            if parent == account.id {
                return Err(StoreError::CategoryInsideItself(account.id));
            }
            let parent = others
                .iter()
                .find(|other| other.id == parent)
                .ok_or(StoreError::UnknownAccount(parent))?;
            if !parent.kind.is_category() {
                return Err(StoreError::ParentNotACategory(parent.id));
            }
            if parent.kind != account.kind {
                return Err(StoreError::ParentKindMismatch {
                    parent: parent.id,
                    parent_kind: parent.kind,
                    kind: account.kind,
                });
            }
            if parent.currency != account.currency {
                return Err(StoreError::ParentCurrencyMismatch {
                    parent: parent.id,
                    parent_currency: parent.currency,
                    currency: account.currency,
                });
            }
            // Walking up from the parent must not meet the category itself.
            // A file with a loop that doesn't include it would never end
            // otherwise, so the walk takes at most as many steps as there are
            // categories.
            let mut above = parent;
            let mut steps = 0;
            while let Some(next) = above.parent {
                if next == account.id {
                    return Err(StoreError::CategoryInsideItself(account.id));
                }
                steps += 1;
                match others.iter().find(|other| other.id == next) {
                    Some(next) if steps <= others.len() => above = next,
                    Some(_) => {
                        return Err(StoreError::Corrupt(Corruption::BadCategoryTree(parent.id)));
                    }
                    None => break,
                }
            }
        }
        // Accounts share one list; categories share one with their siblings,
        // the categories of the same kind inside the same category.
        let wanted = account.name.trim().to_lowercase();
        let clash = others.iter().find(|other| {
            let same_list = if account.kind.is_account() {
                other.kind.is_account()
            } else {
                other.kind == account.kind && other.parent == account.parent
            };
            same_list && other.name.trim().to_lowercase() == wanted
        });
        if let Some(existing) = clash {
            return Err(StoreError::DuplicateAccountName {
                existing: existing.id,
            });
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

    /// Every account and category, the internal account for starting
    /// balances included, sorted by ID. [`Book::accounts`] and
    /// [`Book::categories`] give the lists the user sees.
    pub fn all_accounts(&self) -> Result<Vec<Account>, StoreError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {ACCOUNT_COLUMNS} FROM accounts ORDER BY id"
        ))?;
        let rows = statement
            .query_map([], AccountRow::read)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(AccountRow::into_account).collect()
    }

    /// Gives an account or category a new name, under the same rules as
    /// [`Book::add_account`].
    pub fn rename_account(&mut self, id: AccountId, name: &str) -> Result<(), StoreError> {
        let account = Account {
            name: name.into(),
            ..self.account(id)?
        };
        self.check_account(&account)?;
        let action = if account.kind.is_category() {
            Action::RenameCategory
        } else {
            Action::RenameAccount
        };
        self.as_one_action(action, |book| {
            book.conn.execute(
                "UPDATE accounts SET name = ?2 WHERE id = ?1",
                params![id.to_bytes(), name],
            )?;
            Ok(())
        })
    }

    /// Moves a category inside `parent`, or to the top level if `parent` is
    /// `None`. Its subcategories and its transactions go with it. The new
    /// place must follow the same rules as in [`Book::add_account`]: the same
    /// kind and currency, no clash of names there, and no moving a category
    /// inside itself or its own subcategories.
    pub fn move_category(
        &mut self,
        id: AccountId,
        parent: Option<AccountId>,
    ) -> Result<(), StoreError> {
        let account = self.account(id)?;
        if !account.kind.is_category() {
            return Err(StoreError::NotACategory(id));
        }
        let moved = Account { parent, ..account };
        self.check_account(&moved)?;
        self.as_one_action(Action::MoveCategory, |book| {
            book.conn.execute(
                "UPDATE accounts SET parent_id = ?2 WHERE id = ?1",
                params![id.to_bytes(), parent.map(AccountId::to_bytes)],
            )?;
            Ok(())
        })
    }

    /// Every income and spending category as a tree, flattened: income
    /// before spending, each category followed by its subcategories, siblings
    /// sorted by name.
    pub fn categories(&self) -> Result<Vec<CategoryEntry>, StoreError> {
        let categories: Vec<Account> = self
            .all_accounts()?
            .into_iter()
            .filter(|account| account.kind.is_category())
            .collect();
        let mut children: HashMap<Option<AccountId>, Vec<&Account>> = HashMap::new();
        for category in &categories {
            // Whatever is in the file, a category sits inside a category of
            // its own kind and currency.
            if let Some(parent) = category.parent {
                let fits = categories.iter().any(|other| {
                    other.id == parent
                        && other.kind == category.kind
                        && other.currency == category.currency
                });
                if !fits {
                    return Err(StoreError::Corrupt(Corruption::BadCategoryTree(
                        category.id,
                    )));
                }
            }
            children.entry(category.parent).or_default().push(category);
        }
        for siblings in children.values_mut() {
            siblings.sort_by_cached_key(|category| {
                (
                    category.kind == AccountKind::Expense,
                    category.name.to_lowercase(),
                    category.id,
                )
            });
        }
        let mut entries = Vec::with_capacity(categories.len());
        // Depth first, with a stack of what is still to visit.
        let mut to_visit: Vec<(&Account, Vec<String>)> = children
            .get(&None)
            .into_iter()
            .flatten()
            .rev()
            .map(|&category| (category, Vec::new()))
            .collect();
        while let Some((category, mut path)) = to_visit.pop() {
            path.push(category.name.clone());
            for &child in children.get(&Some(category.id)).into_iter().flatten().rev() {
                to_visit.push((child, path.clone()));
            }
            entries.push(CategoryEntry {
                category: category.clone(),
                path,
            });
        }
        // Categories that no top-level category leads to sit in a loop.
        if entries.len() != categories.len() {
            let reached: HashSet<AccountId> =
                entries.iter().map(|entry| entry.category.id).collect();
            let lost = categories
                .iter()
                .map(|category| category.id)
                .filter(|id| !reached.contains(id))
                .min()
                .expect("a category was not reached");
            return Err(StoreError::Corrupt(Corruption::BadCategoryTree(lost)));
        }
        Ok(entries)
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
        let action = if closed {
            Action::CloseAccount
        } else {
            Action::ReopenAccount
        };
        self.as_one_action(action, |book| {
            book.conn.execute(
                "UPDATE accounts SET closed = ?2 WHERE id = ?1",
                params![id.to_bytes(), closed],
            )?;
            Ok(())
        })
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
        self.as_one_action(Action::AddRate, |book| {
            book.conn.execute(
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
        })
    }

    /// Removes an exchange rate from the price table. Amounts converted with
    /// it keep their values: each conversion stores the amounts it produced
    /// (D-043).
    pub fn remove_rate(&mut self, id: DatedRateId) -> Result<(), StoreError> {
        self.as_one_action(Action::RemoveRate, |book| {
            let removed = book
                .conn
                .execute("DELETE FROM prices WHERE id = ?1", [id.to_bytes()])?;
            if removed == 1 {
                Ok(())
            } else {
                Err(StoreError::UnknownRate(id))
            }
        })
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

    /// Every rate in the price table, sorted by the currency priced, the
    /// currency it is priced in and the date.
    pub fn all_rates(&self) -> Result<Vec<DatedRate>, StoreError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {RATE_COLUMNS} FROM prices ORDER BY from_currency, to_currency, date"
        ))?;
        let rows = statement
            .query_map([], RateRow::read)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(RateRow::into_rate).collect()
    }

    /// The rate from `from` into `to` that applies on `date`: the newest one
    /// on or before it. `None` if there's none yet.
    ///
    /// Only rates in this direction count. [`Book::convert`] looks in both
    /// directions and divides by a rate the other way
    /// ([`Money::checked_div_rate`](kassensturz_core::Money::checked_div_rate)).
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
const ACCOUNT_COLUMNS: &str = "id, name, kind, currency, account_type, closed, parent_id";

/// A row of the accounts table as SQLite returns it, before KassenSturz's
/// checks.
struct AccountRow {
    id: AccountId,
    name: String,
    kind: String,
    currency: String,
    account_type: Option<String>,
    closed: i64,
    parent: Option<[u8; 16]>,
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
            parent: row.get(6)?,
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
            parent: self.parent.map(AccountId::from_bytes),
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
