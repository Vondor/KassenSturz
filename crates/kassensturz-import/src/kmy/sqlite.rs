// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KMyMoney's SQLite file: the same book as a `.kmy` file, kept in tables
//! (M2.10).
//!
//! It is built clean-room like the `.kmy` reader (D-044): the tables and
//! columns were learned from a sample saved by KMyMoney (D-102), never from
//! KMyMoney's code. The reading is the only new part: the records it finds
//! go through the same mapping as a `.kmy` file's, so both give the same
//! book (D-103).
//!
//! The file is untrusted (AGENTS.md §8). It is opened from memory, read
//! only, so nothing of it is written to disk and nothing in it can be
//! changed; SQLite's defensive mode is on and the file's own schema isn't
//! trusted, so it can't run functions. Only fixed queries on tables the
//! file must have as real tables are run, never anything the file holds.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use rusqlite::config::DbConfig;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, MAIN_DB, Row};

use super::records::{
    RawAccount, RawAddress, RawCurrency, RawInstitution, RawPayee, RawPrice, RawSplit, RawTag,
    RawTransaction, Records,
};
use super::{find, mapping};
use crate::book::SkippedData;
use crate::error::ImportError;
use crate::importer::{
    ImportKind, Imported, Importer, ImporterInfo, RECOGNITION_BYTES, Recognition,
};
use crate::limits::Limits;

/// The first bytes of every SQLite database file.
const MAGIC: &[u8] = b"SQLite format 3\0";

/// The only version of KMyMoney's tables the importer reads
/// (`kmmFileInfo.version`): the sample's.
const VERSION: &str = "15";

/// A table every KMyMoney file has, whose name tells a KMyMoney file from
/// another SQLite file.
const MARK: &[u8] = b"kmmFileInfo";

/// The importer for KMyMoney's SQLite files, with the ID `kmy-sqlite`. It
/// reads a whole book, for a new KassenSturz book.
#[derive(Clone, Copy, Debug, Default)]
pub struct KmySqlite;

impl Importer for KmySqlite {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "kmy-sqlite",
            kind: ImportKind::Book,
            extensions: &["sqlite", "db"],
        }
    }

    fn recognize(&self, start: &[u8]) -> Recognition {
        let start = &start[..start.len().min(RECOGNITION_BYTES)];
        if !start.starts_with(MAGIC) {
            Recognition::No
        } else if find(start, MARK) {
            Recognition::Yes
        } else {
            // Any SQLite file starts so, and the tables' names can be
            // further into the file than the first bytes.
            Recognition::Maybe
        }
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        let connection = open(input)?;
        let records = Reader::new(&connection, limits).records()?;
        Ok(Imported::Book(mapping::book(records)?))
    }
}

/// `import.malformed`: a SQLite file has no lines to point at.
fn malformed() -> ImportError {
    ImportError::Malformed { line: None }
}

/// Any failure of SQLite means the file isn't a KMyMoney database or is
/// damaged.
fn sql(_: rusqlite::Error) -> ImportError {
    malformed()
}

/// Opens `input` as a read-only database in memory.
fn open(input: &[u8]) -> Result<Connection, ImportError> {
    if !input.starts_with(MAGIC) {
        return Err(malformed());
    }
    let mut connection = Connection::open_in_memory().map_err(sql)?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(sql)?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(sql)?;
    connection
        .deserialize_read_exact(MAIN_DB, input, input.len(), true)
        .map_err(sql)?;
    Ok(connection)
}

/// Tables the importer reads. A file without one of them isn't a KMyMoney
/// database.
const READ: [&str; 9] = [
    "kmmFileInfo",
    "kmmAccounts",
    "kmmPayees",
    "kmmTransactions",
    "kmmSplits",
    "kmmPrices",
    "kmmSecurities",
    "kmmKeyValuePairs",
    "kmmInstitutions",
];

/// Tables whose rows are only counted, as what the book leaves out. A file
/// may lack one.
const COUNTED: [(&str, SkippedData); 6] = [
    ("kmmCostCenter", SkippedData::Other),
    ("kmmSchedules", SkippedData::Schedules),
    ("kmmBudgetConfig", SkippedData::Budgets),
    ("kmmReportConfig", SkippedData::Reports),
    ("kmmOnlineJobs", SkippedData::OnlineBanking),
    // The IBANs and other identifiers kept with the user's accounts.
    ("kmmAccountsPayeeIdentifier", SkippedData::Other),
];

/// Tables the importer reads if the file has them: the currency list, for
/// currencies KassenSturz's tables don't list (D-106).
const CURRENCIES: &str = "kmmCurrencies";

/// Tables the importer reads if the file has them: the tags and which
/// splits carry them (D-132).
const TAGS: &str = "kmmTags";
const TAG_SPLITS: &str = "kmmTagSplits";

/// Tables KMyMoney has that the book doesn't need, whether or not they hold
/// anything: the payments of schedules (the schedules are counted), its
/// plug-ins, and the identifier definitions the rows of other tables point
/// to.
const IGNORED: [&str; 3] = [
    "kmmSchedulePaymentHistory",
    "kmmPluginInfo",
    "kmmPayeeIdentifier",
];

/// The records of a database, read with fixed queries.
struct Reader<'a> {
    connection: &'a Connection,
    limits: &'a Limits,
    /// How many rows were read so far.
    entries: Cell<usize>,
}

impl<'a> Reader<'a> {
    fn new(connection: &'a Connection, limits: &'a Limits) -> Self {
        Self {
            connection,
            limits,
            entries: Cell::new(0),
        }
    }

    fn records(&self) -> Result<Records, ImportError> {
        self.check_tables_and_version()?;
        let mut records = Records::default();

        let owner = self.payees(&mut records)?;
        if owner {
            records.skipped.add(SkippedData::Owner);
        }
        self.institutions(&mut records)?;
        self.accounts(&mut records)?;
        self.tags(&mut records)?;
        self.transactions(&mut records)?;
        self.prices(&mut records)?;
        self.currencies(&mut records)?;
        self.query("SELECT id FROM kmmSecurities", |row| {
            records.securities.insert(self.required(row, 0)?);
            Ok(())
        })?;

        let tables = self.tables()?;
        for (table, what) in COUNTED {
            if tables.contains(table) {
                let rows = self.count(table)?;
                records.skipped.add_many(what, rows);
            }
        }
        // A table the importer doesn't know holds something it leaves out.
        let known = |table: &str| {
            READ.contains(&table)
                || table == CURRENCIES
                || table == TAGS
                || table == TAG_SPLITS
                || IGNORED.contains(&table)
                || COUNTED.iter().any(|(counted, _)| *counted == table)
                || table == "kmmPayeesPayeeIdentifier"
                || table.starts_with("sqlite_")
        };
        let unknown = tables.iter().filter(|table| !known(table)).count();
        records.skipped.add_many(SkippedData::Other, unknown);
        Ok(records)
    }

    /// Fails unless the file has every table the importer reads, as tables,
    /// and says it has the layout the importer knows.
    fn check_tables_and_version(&self) -> Result<(), ImportError> {
        let tables = self.tables()?;
        if !READ.iter().all(|table| tables.contains(*table)) {
            return Err(malformed());
        }
        let mut versions = Vec::new();
        self.query("SELECT version FROM kmmFileInfo", |row| {
            versions.push(self.optional(row, 0)?);
            Ok(())
        })?;
        if versions != [VERSION] {
            return Err(malformed());
        }
        Ok(())
    }

    /// The names of the file's tables. A view of the same name is no table.
    fn tables(&self) -> Result<HashSet<String>, ImportError> {
        let mut names = HashSet::new();
        let mut statement = self
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .map_err(sql)?;
        let mut rows = statement.query([]).map_err(sql)?;
        while let Some(row) = rows.next().map_err(sql)? {
            names.insert(row.get::<_, String>(0).map_err(sql)?);
        }
        Ok(names)
    }

    /// How many rows `table`, one of the fixed names above, has.
    fn count(&self, table: &str) -> Result<usize, ImportError> {
        let count: i64 = self
            .connection
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })
            .map_err(sql)?;
        usize::try_from(count).map_err(|_| malformed())
    }

    /// Runs `query` and calls `each` for every row, holding the rows to
    /// the limit on entries.
    fn query(
        &self,
        query: &str,
        mut each: impl FnMut(&Row<'_>) -> Result<(), ImportError>,
    ) -> Result<(), ImportError> {
        let mut statement = self.connection.prepare(query).map_err(sql)?;
        let mut rows = statement.query([]).map_err(sql)?;
        while let Some(row) = rows.next().map_err(sql)? {
            self.entries.set(self.entries.get().saturating_add(1));
            self.limits.check_entries(self.entries.get())?;
            each(row)?;
        }
        Ok(())
    }

    /// The text in column `index`, or `None` if it is empty (NULL). A
    /// number is read as the text of the number; a decimal number or a blob
    /// is nothing KMyMoney writes in these columns.
    fn column(&self, row: &Row<'_>, index: usize) -> Result<Option<String>, ImportError> {
        match row.get_ref(index).map_err(sql)? {
            ValueRef::Null => Ok(None),
            ValueRef::Integer(number) => Ok(Some(number.to_string())),
            ValueRef::Text(bytes) => {
                let text = std::str::from_utf8(bytes).map_err(|_| malformed())?;
                self.limits.check_text(text, None)?;
                Ok(Some(text.to_owned()))
            }
            ValueRef::Real(_) | ValueRef::Blob(_) => Err(malformed()),
        }
    }

    /// The text in a column the table declares NOT NULL.
    fn required(&self, row: &Row<'_>, index: usize) -> Result<String, ImportError> {
        self.column(row, index)?.ok_or_else(malformed)
    }

    /// The text in a column that may be empty; NULL is empty.
    fn optional(&self, row: &Row<'_>, index: usize) -> Result<String, ImportError> {
        Ok(self.column(row, index)?.unwrap_or_default())
    }

    /// Reads the payees, with what KMyMoney keeps about them (D-132).
    /// KMyMoney keeps the owner's own details in a row with the ID `USER`,
    /// as the sample shows (D-102); that row is no payee. Returns whether
    /// the owner gave any details.
    fn payees(&self, records: &mut Records) -> Result<bool, ImportError> {
        // Whether a payee has identifiers, such as an IBAN, kept in the
        // table of KMyMoney's plug-in; the book can't hold those yet.
        let with_identifiers = self.payees_with_identifiers()?;
        let mut owner = false;
        let mut identifiers = 0;
        self.query(
            "SELECT id, name, reference, email, addressStreet, addressCity, \
             addressZipcode, addressState, telephone, notes, defaultAccountId, \
             matchData, matchIgnoreCase, matchKeys, idPattern, urlTemplate \
             FROM kmmPayees ORDER BY id",
            |row| {
                let id = self.required(row, 0)?;
                let name = self.optional(row, 1)?;
                if id == "USER" {
                    // Anything besides the name, in the other columns.
                    // Anything besides the name, in the other columns; how
                    // KMyMoney would recognize the owner says nothing.
                    let mut details = with_identifiers.contains(&id);
                    for index in (2..16).filter(|index| ![11, 12].contains(index)) {
                        details |= !self.optional(row, index)?.trim().is_empty();
                    }
                    owner |= details || !name.trim().is_empty();
                    return Ok(());
                }
                if with_identifiers.contains(&id) {
                    identifiers += 1;
                }
                records.payees.push(RawPayee {
                    reference: self.optional(row, 2)?,
                    email: self.optional(row, 3)?,
                    address: RawAddress {
                        street: self.optional(row, 4)?,
                        city: self.optional(row, 5)?,
                        postal_code: self.optional(row, 6)?,
                        state: self.optional(row, 7)?,
                        phone: self.optional(row, 8)?,
                        country: String::new(),
                    },
                    matching: self.optional(row, 11)?,
                    match_ignores_case: self.optional(row, 12)?,
                    match_keys: self.optional(row, 13)?,
                    id,
                    name,
                });
                Ok(())
            },
        )?;
        records.skipped.add_many(SkippedData::Other, identifiers);
        Ok(owner)
    }

    /// Reads the institutions (D-132).
    fn institutions(&self, records: &mut Records) -> Result<(), ImportError> {
        self.query(
            "SELECT id, name, manager, routingCode, addressStreet, addressCity, \
             addressZipcode, telephone FROM kmmInstitutions ORDER BY id",
            |row| {
                records.institutions.push(RawInstitution {
                    id: self.required(row, 0)?,
                    name: self.optional(row, 1)?,
                    contact: self.optional(row, 2)?,
                    bank_code: self.optional(row, 3)?,
                    address: RawAddress {
                        street: self.optional(row, 4)?,
                        city: self.optional(row, 5)?,
                        postal_code: self.optional(row, 6)?,
                        phone: self.optional(row, 7)?,
                        state: String::new(),
                        country: String::new(),
                    },
                });
                Ok(())
            },
        )
    }

    /// Reads the tags, if the file has them (D-132).
    fn tags(&self, records: &mut Records) -> Result<(), ImportError> {
        if !self.tables()?.contains(TAGS) {
            return Ok(());
        }
        self.query("SELECT id, name FROM kmmTags ORDER BY id", |row| {
            records.tags.push(RawTag {
                id: self.required(row, 0)?,
                name: self.optional(row, 1)?,
            });
            Ok(())
        })
    }

    /// The IDs of the payees that have a row in `kmmPayeesPayeeIdentifier`.
    fn payees_with_identifiers(&self) -> Result<HashSet<String>, ImportError> {
        let mut ids = HashSet::new();
        if self.tables()?.contains("kmmPayeesPayeeIdentifier") {
            self.query("SELECT payeeId FROM kmmPayeesPayeeIdentifier", |row| {
                ids.insert(self.required(row, 0)?);
                Ok(())
            })?;
        }
        Ok(ids)
    }

    /// Reads the accounts, with the settings for online banking that
    /// KMyMoney keeps for them as key-value pairs.
    fn accounts(&self, records: &mut Records) -> Result<(), ImportError> {
        let mut online = HashSet::new();
        self.query(
            "SELECT DISTINCT kvpId FROM kmmKeyValuePairs WHERE kvpType = 'ONLINEBANKING'",
            |row| {
                online.insert(self.optional(row, 0)?);
                Ok(())
            },
        )?;
        self.query(
            "SELECT id, parentId, accountType, accountName, accountNumber, \
             description, currencyId, institutionId FROM kmmAccounts ORDER BY id",
            |row| {
                let id = self.required(row, 0)?;
                records.accounts.push(RawAccount {
                    online_banking: online.contains(&id),
                    id,
                    parent: self.optional(row, 1)?,
                    account_type: self.required(row, 2)?,
                    name: self.optional(row, 3)?,
                    number: self.optional(row, 4)?,
                    description: self.optional(row, 5)?,
                    currency: self.optional(row, 6)?,
                    institution: self.optional(row, 7)?,
                    line: None,
                });
                Ok(())
            },
        )?;
        Ok(())
    }

    /// Reads the transactions and puts every split into its transaction.
    fn transactions(&self, records: &mut Records) -> Result<(), ImportError> {
        let mut index: HashMap<String, usize> = HashMap::new();
        self.query(
            "SELECT id, postDate, memo, currencyId, entryDate FROM kmmTransactions ORDER BY id",
            |row| {
                let id = self.required(row, 0)?;
                if index.insert(id, records.transactions.len()).is_some() {
                    return Err(malformed());
                }
                records.transactions.push(RawTransaction {
                    date: self.optional(row, 1)?,
                    memo: self.optional(row, 2)?,
                    currency: self.optional(row, 3)?,
                    entered: self.optional(row, 4)?,
                    splits: Vec::new(),
                    line: None,
                });
                Ok(())
            },
        )?;
        // The tags of each split, by the split's transaction and its number
        // within it, in the order the file gives them (D-132).
        let mut tags: HashMap<(String, String), Vec<String>> = HashMap::new();
        if self.tables()?.contains(TAG_SPLITS) {
            self.query(
                "SELECT transactionId, splitId, tagId FROM kmmTagSplits \
                 ORDER BY transactionId, splitId, rowid",
                |row| {
                    let key = (self.required(row, 0)?, self.required(row, 1)?);
                    tags.entry(key).or_default().push(self.required(row, 2)?);
                    Ok(())
                },
            )?;
        }
        self.query(
            "SELECT transactionId, payeeId, reconcileFlag, value, shares, memo, \
             accountId, checkNumber, bankId, splitId FROM kmmSplits \
             ORDER BY transactionId, splitId",
            |row| {
                let transaction_id = self.required(row, 0)?;
                let transaction = *index.get(&transaction_id).ok_or_else(malformed)?;
                let split_id = self.required(row, 9)?;
                records.transactions[transaction].splits.push(RawSplit {
                    payee: self.optional(row, 1)?,
                    reconciled: self.optional(row, 2)?,
                    value: self.required(row, 3)?,
                    shares: self.required(row, 4)?,
                    memo: self.optional(row, 5)?,
                    account: self.required(row, 6)?,
                    number: self.optional(row, 7)?,
                    bank_id: self.optional(row, 8)?,
                    tags: tags.remove(&(transaction_id, split_id)).unwrap_or_default(),
                    line: None,
                });
                Ok(())
            },
        )?;
        // A tag on a split the file doesn't have is left out.
        let stray: usize = tags.values().map(Vec::len).sum();
        records.skipped.add_many(SkippedData::Tags, stray);
        Ok(())
    }

    /// Reads the price table.
    fn prices(&self, records: &mut Records) -> Result<(), ImportError> {
        self.query(
            "SELECT fromId, toId, priceDate, price FROM kmmPrices \
             ORDER BY fromId, toId, priceDate",
            |row| {
                records.prices.push(RawPrice {
                    from: self.required(row, 0)?,
                    to: self.required(row, 1)?,
                    date: self.required(row, 2)?,
                    price: self.required(row, 3)?,
                    line: None,
                });
                Ok(())
            },
        )?;
        Ok(())
    }

    /// Reads the currency list, if the file has one.
    fn currencies(&self, records: &mut Records) -> Result<(), ImportError> {
        if !self.tables()?.contains(CURRENCIES) {
            return Ok(());
        }
        self.query(
            "SELECT ISOcode, smallestAccountFraction FROM kmmCurrencies ORDER BY ISOcode",
            |row| {
                records.currencies.push(RawCurrency {
                    id: self.required(row, 0)?,
                    fraction: self.optional(row, 1)?,
                    line: None,
                });
                Ok(())
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
