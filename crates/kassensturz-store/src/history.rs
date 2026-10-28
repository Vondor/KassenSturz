// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The change history: every change to a book, one entry per user action,
//! with the values before and after of every row the action touched, so it
//! can be undone and redone, also after a restart (D-032, D-066, D-086).
//!
//! KassenSturz records it itself, with temporary triggers on its own
//! connection to the book, which aren't stored in the book. While a user
//! action is under way, they copy every row it touches into a staging
//! table: its values before, the first time it is touched, and its values
//! after. When the action ends, what it changed becomes one entry of the
//! history. A change to the book outside a user action is refused, so no
//! change can escape the history.
//!
//! The history is kept in the book, or only in memory if the user keeps it
//! for 0 days (D-066). Its layout is documented in `docs/BOOK_FORMAT.md`.

use std::path::Path;

use kassensturz_core::{AccountId, Date, Timestamp};
use rusqlite::{Connection, OptionalExtension, ToSql, params};

use crate::cached::Cached;
use crate::error::{Corruption, StoreError};
use crate::{Book, date_from_text};

/// What the user did in one change to the book, e.g. adding an account. The
/// UI uses it to say what undo and redo would do, in the user's language
/// (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Changed how the book's calculations round (D-027).
    SetRoundingMode,
    /// Started using a currency in the book.
    UseCurrency,
    /// Added an account, or the internal account for starting balances.
    AddAccount,
    /// Renamed an account.
    RenameAccount,
    /// Closed an account (D-081).
    CloseAccount,
    /// Opened a closed account again.
    ReopenAccount,
    /// Added a category.
    AddCategory,
    /// Renamed a category.
    RenameCategory,
    /// Moved a category inside another one, or to the top level (D-082).
    MoveCategory,
    /// Added an exchange rate to the price table (D-028).
    AddRate,
    /// Removed an exchange rate from the price table.
    RemoveRate,
    /// Added a transaction.
    AddTransaction,
    /// Changed a transaction: its date, payee or Parts (D-017).
    EditTransaction,
    /// Removed a transaction.
    RemoveTransaction,
    /// Joined the two sides of a Transfer between the user's own accounts
    /// into one transaction (D-021, D-087).
    JoinTransfer,
    /// Added a Rule or a sub-Rule (D-014).
    AddRule,
    /// Changed a Rule: its name, conditions or actions.
    EditRule,
    /// Moved a Rule to another place in the list, or inside another Rule.
    MoveRule,
    /// Removed a Rule, with its sub-Rules.
    RemoveRule,
    /// Ran the Rules on the book's transactions (D-031, D-088).
    RunRules,
    /// Imported a bank statement, or the statements of one file, into one
    /// account (D-030, D-092).
    ImportStatement,
    /// Corrected an account's starting balance so that it holds on a day
    /// what the bank said, and noted that balance and its day on the
    /// account (D-111, D-116, D-117).
    CorrectStartingBalance,
    /// Added a bank or other institution (D-132).
    AddInstitution,
    /// Added a payee: someone the user pays or receives money from, with
    /// what the user keeps about them (D-132).
    AddPayee,
    /// Added a tag (D-132).
    AddTag,
    /// Linked an account to the bank account it is filled from, or changed
    /// that link (Q-086, D-148).
    LinkBankAccount,
    /// Removed an account's bank link.
    UnlinkBankAccount,
    /// Fetched an account's statement from its bank, imported it, and noted
    /// the day on the bank link (D-148).
    FetchStatement,
    /// Applied changes an AI prepared, after the user confirmed them: a
    /// change set (D-171, D-198). Whatever it changed is one change, so the
    /// history says the AI's changes came together, and one undo takes them
    /// back.
    ApplyAiChanges,
    /// Removed a category that nothing used (D-253).
    RemoveCategory,
}

impl Action {
    /// Every action, in the order of the list above.
    pub const ALL: [Self; 30] = [
        Self::SetRoundingMode,
        Self::UseCurrency,
        Self::AddAccount,
        Self::RenameAccount,
        Self::CloseAccount,
        Self::ReopenAccount,
        Self::AddCategory,
        Self::RenameCategory,
        Self::MoveCategory,
        Self::AddRate,
        Self::RemoveRate,
        Self::AddTransaction,
        Self::EditTransaction,
        Self::RemoveTransaction,
        Self::JoinTransfer,
        Self::AddRule,
        Self::EditRule,
        Self::MoveRule,
        Self::RemoveRule,
        Self::RunRules,
        Self::ImportStatement,
        Self::CorrectStartingBalance,
        Self::AddInstitution,
        Self::AddPayee,
        Self::AddTag,
        Self::LinkBankAccount,
        Self::UnlinkBankAccount,
        Self::FetchStatement,
        Self::ApplyAiChanges,
        Self::RemoveCategory,
    ];
}

/// Identifies one change in a book's change history: a UUID, version 7, like
/// everything in a book (D-067).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChangeId(uuid::Uuid);

impl ChangeId {
    fn generate() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(uuid::Uuid::from_bytes(bytes))
    }

    fn to_bytes(self) -> [u8; 16] {
        self.0.into_bytes()
    }
}

impl core::fmt::Display for ChangeId {
    /// The usual hyphenated form, e.g. for log messages.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

/// One entry of the change history: one user action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Its ID.
    pub id: ChangeId,
    /// What the user did.
    pub action: Action,
    /// When, in whole seconds. The UI shows it in the user's time zone.
    pub made_at: Timestamp,
    /// Whether it is undone, so that redo can make it again.
    pub undone: bool,
    /// The name of the file it read, such as the statement file of an
    /// import, without its folder ([`Book::note_file`], D-109).
    pub file_name: Option<String>,
}

/// What one change did to the book's transactions, so a front end can say
/// which change undo or redo takes back, e.g. which of two imports (#51,
/// D-109). A change that touched no transaction, such as adding an account,
/// has an empty summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    /// How many transactions it added.
    pub added: usize,
    /// How many it removed.
    pub removed: usize,
    /// How many it changed: their date, payee or parts.
    pub changed: usize,
    /// The accounts and categories the parts of these transactions are
    /// booked to, before or after the change, sorted by ID.
    pub accounts: Vec<AccountId>,
    /// The earliest and the latest booking date of these transactions,
    /// before or after the change.
    pub dates: Option<(Date, Date)>,
}

/// How long a book keeps its change history (D-066). The user chooses it,
/// and it is stored in the book, so every computer keeps the same history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryRetention {
    /// For this many days. With 0, nothing is stored in the book: the
    /// history is kept in memory until the book is closed.
    Days(u32),
    /// For as long as the book exists.
    Forever,
}

impl Default for HistoryRetention {
    /// 30 days (D-066).
    fn default() -> Self {
        Self::Days(30)
    }
}

/// A table whose changes the history records: the name, the column that
/// identifies a row (its primary key) and every column with its type, in
/// the table's order.
///
/// The history keeps the rows of `name` in a table `changed_<name>` with the
/// same columns (`docs/BOOK_FORMAT.md`). A migration that changes a recorded
/// table changes its `changed_` table and this list the same way, and
/// converts the rows kept there (D-066); a test checks that they match.
pub(crate) struct Recorded {
    pub(crate) name: &'static str,
    pub(crate) key: &'static str,
    pub(crate) columns: &'static [(&'static str, &'static str)],
}

/// Every table that holds the book's data. Only the history's own tables
/// aren't recorded: `changes`, the `changed_` tables and `history_settings`.
pub(crate) const RECORDED: [Recorded; 17] = [
    Recorded {
        name: "settings",
        key: "id",
        columns: &[("id", "INTEGER"), ("rounding_mode", "TEXT")],
    },
    Recorded {
        name: "currencies",
        key: "code",
        columns: &[("code", "TEXT"), ("minor_digits", "INTEGER")],
    },
    Recorded {
        name: "accounts",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("name", "TEXT"),
            ("kind", "TEXT"),
            ("currency", "TEXT"),
            ("account_type", "TEXT"),
            ("closed", "INTEGER"),
            ("parent_id", "BLOB"),
            ("stated_balance", "INTEGER"),
            ("stated_on", "TEXT"),
            ("number", "TEXT"),
            ("notes", "TEXT"),
            ("institution_id", "BLOB"),
        ],
    },
    Recorded {
        name: "transactions",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("booking_date", "TEXT"),
            ("payee", "TEXT"),
            ("currency", "TEXT"),
            ("note", "TEXT"),
            ("entered_on", "TEXT"),
        ],
    },
    Recorded {
        name: "splits",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("transaction_id", "BLOB"),
            ("position", "INTEGER"),
            ("account_id", "BLOB"),
            ("amount", "INTEGER"),
            ("foreign_amount", "INTEGER"),
            ("foreign_currency", "TEXT"),
            ("original_amount", "INTEGER"),
            ("original_currency", "TEXT"),
            ("own_date", "TEXT"),
            ("own_payee", "TEXT"),
            ("note", "TEXT"),
            ("number", "TEXT"),
            ("status", "TEXT"),
        ],
    },
    Recorded {
        name: "split_rates",
        key: "split_id",
        columns: &[
            ("split_id", "BLOB"),
            ("rate_id", "BLOB"),
            ("date", "TEXT"),
            ("from_currency", "TEXT"),
            ("to_currency", "TEXT"),
            ("rate_digits", "INTEGER"),
            ("rate_decimal_places", "INTEGER"),
            ("source", "TEXT"),
        ],
    },
    Recorded {
        name: "split_imports",
        key: "split_id",
        columns: &[
            ("split_id", "BLOB"),
            ("bank_id", "TEXT"),
            ("booking_date", "TEXT"),
            ("value_date", "TEXT"),
            ("other_party_name", "TEXT"),
            ("other_party_iban", "TEXT"),
            ("remittance_information", "TEXT"),
        ],
    },
    Recorded {
        name: "prices",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("date", "TEXT"),
            ("from_currency", "TEXT"),
            ("to_currency", "TEXT"),
            ("rate_digits", "INTEGER"),
            ("rate_decimal_places", "INTEGER"),
            ("source", "TEXT"),
        ],
    },
    Recorded {
        name: "rules",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("parent_id", "BLOB"),
            ("position", "INTEGER"),
            ("name", "TEXT"),
            ("match_mode", "TEXT"),
            ("category_id", "BLOB"),
            ("transfer_account_id", "BLOB"),
            ("payee", "TEXT"),
        ],
    },
    Recorded {
        name: "rule_conditions",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("rule_id", "BLOB"),
            ("position", "INTEGER"),
            ("field", "TEXT"),
            ("operator", "TEXT"),
            ("text_value", "TEXT"),
            ("case_sensitive", "INTEGER"),
            ("amount_min", "INTEGER"),
            ("amount_max", "INTEGER"),
            ("currency", "TEXT"),
            ("date_first", "TEXT"),
            ("date_last", "TEXT"),
            ("account_id", "BLOB"),
        ],
    },
    Recorded {
        name: "split_rules",
        key: "split_id",
        columns: &[("split_id", "BLOB"), ("rule_id", "BLOB")],
    },
    Recorded {
        name: "institutions",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("name", "TEXT"),
            ("bank_code", "TEXT"),
            ("contact", "TEXT"),
            ("address", "TEXT"),
            ("phone", "TEXT"),
        ],
    },
    Recorded {
        name: "payees",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("name", "TEXT"),
            ("address", "TEXT"),
            ("email", "TEXT"),
            ("phone", "TEXT"),
            ("reference", "TEXT"),
            ("recognition", "TEXT"),
            ("key_words", "TEXT"),
            ("recognition_ignores_case", "INTEGER"),
        ],
    },
    Recorded {
        name: "tags",
        key: "id",
        columns: &[("id", "BLOB"), ("name", "TEXT")],
    },
    Recorded {
        name: "split_tags",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("split_id", "BLOB"),
            ("tag_id", "BLOB"),
            ("position", "INTEGER"),
        ],
    },
    Recorded {
        name: "attachments",
        key: "id",
        columns: &[
            ("id", "BLOB"),
            ("transaction_id", "BLOB"),
            ("position", "INTEGER"),
            ("link", "TEXT"),
            ("title", "TEXT"),
        ],
    },
    Recorded {
        name: "bank_links",
        key: "account_id",
        columns: &[
            ("account_id", "BLOB"),
            ("iban", "TEXT"),
            ("bank_code", "TEXT"),
            ("number", "TEXT"),
            ("last_fetched_on", "TEXT"),
        ],
    },
];

impl Recorded {
    /// The column names, separated by commas, each after `prefix`, e.g.
    /// `NEW.`.
    fn names(&self, prefix: &str) -> String {
        self.columns
            .iter()
            .map(|(name, _)| format!("{prefix}{name}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// SQL that is true if the rows `a` and `b` hold the same values, or are
    /// both missing.
    fn same(&self, a: &str, b: &str) -> String {
        self.columns
            .iter()
            .map(|(name, _)| format!("{a}.{name} IS {b}.{name}"))
            .collect::<Vec<_>>()
            .join(" AND ")
    }

    /// The columns with their types, for a table that holds copies of rows.
    fn definitions(&self) -> String {
        self.columns
            .iter()
            .map(|&(name, kind)| {
                let not_null = if name == self.key { " NOT NULL" } else { "" };
                format!("{name} {kind}{not_null},")
            })
            .collect::<Vec<_>>()
            .join("\n    ")
    }

    /// The temporary staging table, the history's table in memory, and the
    /// triggers that record each change to this table (see [`prepare`]).
    fn temporary_sql(&self) -> String {
        let Self { name, key, .. } = self;
        let definitions = self.definitions();
        let columns = self.names("");
        let old = self.names("OLD.");
        let new = self.names("NEW.");
        // Inside a trigger, SQLite doesn't allow a database name in front of
        // the table an INSERT or DELETE writes to; `staged_<name>` exists
        // only in `temp`, so it can't be mistaken for another table.
        let refuse = "SELECT RAISE(ABORT, 'KassenSturz changed a book outside a user action') \
                      WHERE NOT EXISTS (SELECT 1 FROM temp.history_state);";
        let recording = "(SELECT recording FROM temp.history_state)";
        // The values before are those of the first time the action touches
        // the row; a row it added has none.
        let before = format!(
            "INSERT INTO staged_{name} (side, {columns}) SELECT 'before', {old}
                 WHERE {recording}
                 AND NOT EXISTS (SELECT 1 FROM temp.staged_{name} WHERE {key} = OLD.{key});
             DELETE FROM staged_{name} WHERE {key} = OLD.{key} AND side = 'after';"
        );
        let after = format!(
            "INSERT INTO staged_{name} (side, {columns}) SELECT 'after', {new} WHERE {recording};"
        );
        format!(
            "
CREATE TEMP TABLE staged_{name} (
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    {definitions}
    PRIMARY KEY ({key}, side)
) STRICT;

CREATE TEMP TABLE changed_{name} (
    change_id BLOB NOT NULL REFERENCES changes (id) ON DELETE CASCADE,
    side TEXT NOT NULL CHECK (side IN ('before', 'after')),
    {definitions}
    PRIMARY KEY (change_id, {key}, side)
) STRICT;

CREATE TEMP TRIGGER history_insert_{name} AFTER INSERT ON main.{name}
BEGIN
    {refuse}
    {after}
END;

CREATE TEMP TRIGGER history_update_{name} AFTER UPDATE ON main.{name}
BEGIN
    {refuse}
    {before}
    {after}
END;

CREATE TEMP TRIGGER history_delete_{name} AFTER DELETE ON main.{name}
BEGIN
    {refuse}
    {before}
END;
"
        )
    }
}

/// Sets up the recording on a new connection to a book: the staging tables,
/// the history's tables in memory and the triggers. They live in SQLite's
/// temporary database, so they are made again for every connection and are
/// never stored in the book.
///
/// The history's tables in memory, for a book that keeps its history for 0
/// days, have the same names as those in the book, so every statement names
/// the database it means (`main` or `temp`). `history_state` holds the state
/// of the user action under way: no row while there is none, so every change
/// is refused; `recording` 1 while one is recorded, 0 while undo or redo
/// write rows back, which isn't recorded again; `file_name` the file the
/// action reads, if [`Book::note_file`] named one.
pub(crate) fn prepare(conn: &Connection) -> Result<(), StoreError> {
    let actions = Action::ALL
        .map(|action| format!("'{}'", action_to_text(action)))
        .join(", ");
    let mut sql = format!(
        "
CREATE TEMP TABLE history_state (
    recording INTEGER NOT NULL CHECK (recording IN (0, 1)),
    file_name TEXT
) STRICT;

CREATE TEMP TABLE changes (
    id BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    action TEXT NOT NULL CHECK (action IN ({actions})),
    made_at TEXT NOT NULL
        CHECK (made_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    undone INTEGER NOT NULL CHECK (undone IN (0, 1)),
    file_name TEXT
) STRICT;
"
    );
    for table in &RECORDED {
        sql.push_str(&table.temporary_sql());
    }
    conn.execute_batch(&sql)?;
    Ok(())
}

/// Which values of a row the history writes back: undo the ones before a
/// change, redo the ones after it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Before,
    After,
}

impl Side {
    fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Before => Self::After,
            Self::After => Self::Before,
        }
    }
}

impl Book {
    /// Runs `change` as one user action: the history records everything it
    /// changes in the book as one entry, which undo takes back as a whole
    /// (D-066). A front end uses it when one thing the user does takes
    /// several calls, e.g. adding an account in a currency the book doesn't
    /// use yet.
    ///
    /// If `change` fails, everything it changed is taken back and nothing is
    /// recorded. If it changes nothing, nothing is recorded either. Inside a
    /// user action, every change to the book becomes part of it, including
    /// other calls of this function; their `action` is then not used.
    ///
    /// # Panics
    ///
    /// If `change` calls [`Book::undo`], [`Book::redo`],
    /// [`Book::set_history_retention`] or [`Book::clean_up_history`]: they
    /// can't be part of a user action.
    pub fn as_one_action<T, E>(
        &mut self,
        action: Action,
        change: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        if self.recording {
            // Part of the action under way. Its own savepoint makes sure a
            // failure takes back only this part.
            return self.in_savepoint(change);
        }
        self.in_savepoint(|book| {
            book.conn
                .execute_cached("INSERT INTO temp.history_state (recording) VALUES (1)", [])
                .map_err(StoreError::from)?;
            book.recording = true;
            let changed = change(book);
            book.recording = false;
            let value = changed?;
            book.record(action)?;
            Ok(value)
        })
    }

    /// Runs `change` as [`Book::as_one_action`] would, then takes back
    /// everything it changed: the book stays exactly as it was, and nothing
    /// is recorded. Returns what `change` returned, e.g. what an import
    /// would do, so the user can check it first (D-092).
    ///
    /// Inside a user action, only what `change` did is taken back. IDs that
    /// `change` gave new rows don't exist in the book afterwards.
    ///
    /// # Panics
    ///
    /// If `change` calls [`Book::undo`], [`Book::redo`],
    /// [`Book::set_history_retention`] or [`Book::clean_up_history`], as in
    /// [`Book::as_one_action`].
    pub fn dry_run<T, E>(&mut self, change: impl FnOnce(&mut Self) -> Result<T, E>) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        self.conn
            .execute_batch("SAVEPOINT book")
            .map_err(StoreError::from)?;
        let outer = self.recording;
        let started = if outer {
            Ok(())
        } else {
            // Recorded like a real action, so it fails where that would.
            self.conn
                .execute_cached("INSERT INTO temp.history_state (recording) VALUES (1)", [])
                .map(|_| ())
        };
        let result = match started {
            Ok(()) => {
                self.recording = true;
                let result = change(self);
                self.recording = outer;
                result
            }
            Err(error) => Err(StoreError::from(error).into()),
        };
        // Also takes back the staged rows and the state of the action.
        self.roll_back();
        result
    }

    /// Notes the file the user action under way reads, such as the statement
    /// file of an import, so the history can say which file a change came
    /// from (D-109). Only the file's name is kept, never its folder, which
    /// tells more about the user than the history needs; a path without a
    /// name, such as `/`, notes nothing. If the action changes nothing, the
    /// name isn't kept either.
    ///
    /// # Panics
    ///
    /// Outside [`Book::as_one_action`] and [`Book::dry_run`].
    pub fn note_file(&mut self, path: &Path) -> Result<(), StoreError> {
        assert!(
            self.recording,
            "only a user action can note the file it reads"
        );
        if let Some(name) = path.file_name() {
            self.conn.execute_cached(
                "UPDATE temp.history_state SET file_name = ?1",
                [name.to_string_lossy()],
            )?;
        }
        Ok(())
    }

    /// Ends the user action under way: what it changed becomes one entry of
    /// the history, unless it changed nothing.
    fn record(&mut self, action: Action) -> Result<(), StoreError> {
        let mut changed = false;
        for table in &RECORDED {
            let name = table.name;
            let key = table.key;
            // A row the action changed and then changed back isn't a change.
            self.conn.execute_cached(
                &format!(
                    "DELETE FROM temp.staged_{name} WHERE {key} IN (
                         SELECT b.{key} FROM temp.staged_{name} AS b
                         JOIN temp.staged_{name} AS a ON a.{key} = b.{key} AND a.side = 'after'
                         WHERE b.side = 'before' AND {same})",
                    same = table.same("b", "a")
                ),
                [],
            )?;
            changed |= self.conn.query_row_cached(
                &format!("SELECT EXISTS (SELECT 1 FROM temp.staged_{name})"),
                [],
                |row| row.get::<_, bool>(0),
            )?;
        }
        if changed {
            let schema = self.history_schema()?;
            // What was undone can't be redone after a new change. Changes are
            // undone newest first, so the undone ones are the newest ones: if
            // the newest isn't undone, none is. Without this look, every
            // change would scan the whole history, and a book filled one
            // action at a time would slow down with every action (#84).
            let undone: bool = self.conn.query_row_cached(
                &format!(
                    "SELECT EXISTS (SELECT 1 FROM {schema}.changes
                     WHERE sequence = (SELECT max(sequence) FROM {schema}.changes) AND undone = 1)"
                ),
                [],
                |row| row.get(0),
            )?;
            if undone {
                delete_changes(&self.conn, schema, "undone = 1", &[])?;
            }
            let id = ChangeId::generate();
            let made_at = whole_seconds(Timestamp::now());
            self.conn.execute_cached(
                &format!(
                    "INSERT INTO {schema}.changes (id, sequence, action, made_at, undone, file_name)
                     SELECT ?1, coalesce(max(sequence), 0) + 1, ?2, ?3, 0,
                            (SELECT file_name FROM temp.history_state)
                     FROM {schema}.changes"
                ),
                params![id.to_bytes(), action_to_text(action), made_at.to_string()],
            )?;
            for table in &RECORDED {
                let name = table.name;
                let columns = table.names("");
                self.conn.execute_cached(
                    &format!(
                        "INSERT INTO {schema}.changed_{name} (change_id, side, {columns})
                         SELECT ?1, side, {columns} FROM temp.staged_{name}"
                    ),
                    [id.to_bytes()],
                )?;
            }
            prune_after_change(&self.conn, self.history_retention()?, made_at)?;
        }
        for table in &RECORDED {
            self.conn
                .execute_cached(&format!("DELETE FROM temp.staged_{}", table.name), [])?;
        }
        self.conn
            .execute_cached("DELETE FROM temp.history_state", [])?;
        Ok(())
    }

    /// Undoes the newest change that isn't undone yet, also one made before
    /// the book was last opened, and returns it. `None` if there is none.
    ///
    /// Every row the change touched gets its values from before the change
    /// back (D-066). Changes are undone newest first, so the book is always
    /// as it was at some earlier moment. If the book doesn't hold what the
    /// change left in it, e.g. because another program changed it, nothing
    /// is changed and it fails with [`Corruption::HistoryMismatch`].
    ///
    /// # Panics
    ///
    /// Inside [`Book::as_one_action`].
    pub fn undo(&mut self) -> Result<Option<Change>, StoreError> {
        let Some(change) = self.change_to_undo()? else {
            return Ok(None);
        };
        self.restore(change.id, Side::Before)?;
        Ok(Some(Change {
            undone: true,
            ..change
        }))
    }

    /// Makes the change that was undone last again and returns it. `None` if
    /// nothing was undone since the last change.
    ///
    /// Every row the change touched gets its values from after the change
    /// back (D-066). Like [`Book::undo`], it fails with
    /// [`Corruption::HistoryMismatch`] if the book doesn't hold what it held
    /// before the change.
    ///
    /// # Panics
    ///
    /// Inside [`Book::as_one_action`].
    pub fn redo(&mut self) -> Result<Option<Change>, StoreError> {
        let Some(change) = self.change_to_redo()? else {
            return Ok(None);
        };
        self.restore(change.id, Side::After)?;
        Ok(Some(Change {
            undone: false,
            ..change
        }))
    }

    /// The change [`Book::undo`] would undo, so the UI can say what it is.
    pub fn change_to_undo(&self) -> Result<Option<Change>, StoreError> {
        self.changes("WHERE undone = 0 ORDER BY sequence DESC LIMIT 1")
            .map(|changes| changes.into_iter().next())
    }

    /// The change [`Book::redo`] would make again.
    pub fn change_to_redo(&self) -> Result<Option<Change>, StoreError> {
        self.changes("WHERE undone = 1 ORDER BY sequence LIMIT 1")
            .map(|changes| changes.into_iter().next())
    }

    /// Every change in the history, newest first. The undone ones, which
    /// redo can make again, come first.
    pub fn history(&self) -> Result<Vec<Change>, StoreError> {
        self.changes("ORDER BY sequence DESC")
    }

    /// The changes in the history that `rest`, the end of a query, selects,
    /// in its order.
    fn changes(&self, rest: &str) -> Result<Vec<Change>, StoreError> {
        let schema = self.history_schema()?;
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT id, action, made_at, undone, file_name FROM {schema}.changes {rest}"
        ))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    ChangeId::from_bytes(row.get(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, action, made_at, undone, file_name)| {
                Ok(Change {
                    id,
                    action: action_from_text(&action)?,
                    made_at: made_at
                        .parse()
                        .map_err(|_| StoreError::Corrupt(Corruption::BadTimestamp(made_at)))?,
                    undone,
                    file_name,
                })
            })
            .collect()
    }

    /// What `change`, an entry of the history, did to the book's
    /// transactions ([`ChangeSummary`]). It is read from the rows the change
    /// recorded, and from the book for the parts and dates of a transaction
    /// it changed only in part. So it says the same whether the change is
    /// done or undone, as long as the changes before it are done, as they are
    /// for the change undo would take back and the one redo would make
    /// again.
    pub fn change_summary(&self, change: ChangeId) -> Result<ChangeSummary, StoreError> {
        let schema = self.history_schema()?;
        let id = change.to_bytes();
        // The transactions it touched, itself or through one of their
        // parts, and on which sides the history has them.
        let touched = format!(
            "SELECT id FROM {schema}.changed_transactions WHERE change_id = ?1
             UNION SELECT transaction_id FROM {schema}.changed_splits WHERE change_id = ?1"
        );
        let mut summary = ChangeSummary::default();
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT
                 EXISTS (SELECT 1 FROM {schema}.changed_transactions
                         WHERE change_id = ?1 AND id = touched.id AND side = 'before'),
                 EXISTS (SELECT 1 FROM {schema}.changed_transactions
                         WHERE change_id = ?1 AND id = touched.id AND side = 'after')
             FROM ({touched}) AS touched"
        ))?;
        let mut rows = statement.query([id])?;
        while let Some(row) = rows.next()? {
            match (row.get::<_, bool>(0)?, row.get::<_, bool>(1)?) {
                (false, true) => summary.added += 1,
                (true, false) => summary.removed += 1,
                _ => summary.changed += 1,
            }
        }
        drop(rows);
        drop(statement);
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT DISTINCT account_id FROM (
                 SELECT account_id FROM {schema}.changed_splits WHERE change_id = ?1
                 UNION ALL
                 SELECT account_id FROM main.splits WHERE transaction_id IN ({touched}))
             WHERE account_id IS NOT NULL
             ORDER BY account_id"
        ))?;
        summary.accounts = statement
            .query_map([id], |row| Ok(AccountId::from_bytes(row.get(0)?)))?
            .collect::<Result<_, _>>()?;
        let (first, last): (Option<String>, Option<String>) = self.conn.query_row_cached(
            &format!(
                "SELECT min(booking_date), max(booking_date) FROM (
                     SELECT booking_date FROM {schema}.changed_transactions WHERE change_id = ?1
                     UNION ALL
                     SELECT booking_date FROM main.transactions WHERE id IN ({touched}))"
            ),
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if let (Some(first), Some(last)) = (first, last) {
            summary.dates = Some((date_from_text(&first)?, date_from_text(&last)?));
        }
        Ok(summary)
    }

    /// Writes the values of `side` back into every row that `change`
    /// touched.
    fn restore(&mut self, change: ChangeId, side: Side) -> Result<(), StoreError> {
        assert!(
            !self.recording && self.conn.is_autocommit(),
            "undo and redo can't be part of a user action"
        );
        let schema = self.history_schema()?;
        // Without foreign keys, so writing a row back never deletes the rows
        // that refer to it (ON DELETE CASCADE): rows the change didn't touch
        // stay as they are. Every reference is checked before the end, as in
        // a migration. SQLite only switches them outside a transaction.
        self.conn.pragma_update(None, "foreign_keys", false)?;
        let restored = self.in_savepoint(|book| book.write_back(schema, change, side));
        let switched_on = self.conn.pragma_update(None, "foreign_keys", true);
        restored?;
        switched_on?;
        Ok(())
    }

    fn write_back(&mut self, schema: &str, change: ChangeId, side: Side) -> Result<(), StoreError> {
        self.conn
            .execute_cached("INSERT INTO temp.history_state (recording) VALUES (0)", [])?;
        let id = change.to_bytes();
        // The book must hold what the other side says, or the values written
        // back wouldn't fit what is around them.
        for table in &RECORDED {
            let Recorded { name, key, .. } = table;
            let differs: bool = self.conn.query_row_cached(
                &format!(
                    "SELECT EXISTS (
                         SELECT 1 FROM (
                             SELECT DISTINCT {key} FROM {schema}.changed_{name} WHERE change_id = ?1
                         ) AS touched
                         LEFT JOIN main.{name} AS b ON b.{key} = touched.{key}
                         LEFT JOIN {schema}.changed_{name} AS h
                             ON h.change_id = ?1 AND h.side = ?2 AND h.{key} = touched.{key}
                         WHERE NOT ({same}))",
                    same = table.same("b", "h")
                ),
                params![id, side.other().as_str()],
                |row| row.get(0),
            )?;
            if differs {
                return Err(StoreError::Corrupt(Corruption::HistoryMismatch));
            }
        }
        for table in &RECORDED {
            let Recorded { name, key, .. } = table;
            let columns = table.names("");
            self.conn
                .execute_cached(
                    &format!(
                        "DELETE FROM main.{name} WHERE {key} IN (
                             SELECT {key} FROM {schema}.changed_{name} WHERE change_id = ?1)"
                    ),
                    [id],
                )
                .map_err(mismatch_if_refused)?;
            self.conn
                .execute_cached(
                    &format!(
                        "INSERT INTO main.{name} ({columns})
                         SELECT {columns} FROM {schema}.changed_{name}
                         WHERE change_id = ?1 AND side = ?2"
                    ),
                    params![id, side.as_str()],
                )
                .map_err(mismatch_if_refused)?;
        }
        let broken_references: i64 = self.conn.query_row_cached(
            "SELECT count(*) FROM pragma_foreign_key_check",
            [],
            |row| row.get(0),
        )?;
        if broken_references > 0 {
            return Err(StoreError::Corrupt(Corruption::HistoryMismatch));
        }
        self.conn.execute_cached(
            &format!("UPDATE {schema}.changes SET undone = ?2 WHERE id = ?1"),
            params![id, side == Side::Before],
        )?;
        self.conn
            .execute_cached("DELETE FROM temp.history_state", [])?;
        Ok(())
    }

    /// How long this book keeps its change history (D-066). New books keep
    /// it for 30 days.
    pub fn history_retention(&self) -> Result<HistoryRetention, StoreError> {
        let days: Option<Option<i64>> = self
            .conn
            .query_row_cached(
                "SELECT keep_days FROM main.history_settings WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        match days.ok_or(StoreError::Corrupt(Corruption::MissingSettings))? {
            None => Ok(HistoryRetention::Forever),
            Some(days) => u32::try_from(days)
                .map(HistoryRetention::Days)
                .map_err(|_| StoreError::Corrupt(Corruption::BadHistoryRetention(days))),
        }
    }

    /// Sets how long this book keeps its change history (D-066). Older
    /// changes are deleted right away and can't be undone anymore.
    ///
    /// With 0 days nothing is stored in the book: the history moves into
    /// memory, so what was done while the book is open can still be undone,
    /// until it is closed. With more days it moves back into the book.
    /// Setting it is no change to the book's data, so it isn't recorded.
    ///
    /// # Panics
    ///
    /// Inside [`Book::as_one_action`].
    pub fn set_history_retention(&mut self, keep: HistoryRetention) -> Result<(), StoreError> {
        assert!(
            !self.recording,
            "the history's settings can't be part of a user action"
        );
        self.in_savepoint(|book| {
            let from = book.history_schema()?;
            let days = match keep {
                HistoryRetention::Days(days) => Some(i64::from(days)),
                HistoryRetention::Forever => None,
            };
            let changed = book.conn.execute_cached(
                "UPDATE main.history_settings SET keep_days = ?1 WHERE id = 1",
                [days],
            )?;
            if changed != 1 {
                return Err(StoreError::Corrupt(Corruption::MissingSettings));
            }
            let to = book.history_schema()?;
            if from != to {
                move_history(&book.conn, from, to)?;
            }
            prune(&book.conn, keep, Timestamp::now())
        })
    }

    /// Deletes the whole change history, in the book and in memory, for good
    /// (D-066): nothing done so far can be undone or redone anymore. The book
    /// overwrites what it deletes (`docs/BOOK_FORMAT.md`), so the history is
    /// gone from its file too, though not from copies of it, such as
    /// backups.
    ///
    /// # Panics
    ///
    /// Inside [`Book::as_one_action`].
    pub fn clean_up_history(&mut self) -> Result<(), StoreError> {
        assert!(
            !self.recording,
            "cleaning up the history can't be part of a user action"
        );
        self.in_savepoint(|book| {
            for schema in ["main", "temp"] {
                delete_changes(&book.conn, schema, "1", &[])?;
            }
            Ok(())
        })
    }

    /// Deletes the changes older than the book keeps them, when it is
    /// opened.
    pub(crate) fn prune_history(&mut self) -> Result<(), StoreError> {
        let keep = self.history_retention()?;
        self.in_savepoint(|book| prune(&book.conn, keep, Timestamp::now()))
    }

    /// Where the history is kept: in the book (`main`), or in memory
    /// (`temp`) if the book keeps it for 0 days.
    fn history_schema(&self) -> Result<&'static str, StoreError> {
        Ok(match self.history_retention()? {
            HistoryRetention::Days(0) => "temp",
            _ => "main",
        })
    }

    /// Runs `change` in a savepoint: if it fails, everything it changed is
    /// taken back. Outside a transaction, the savepoint starts one, which
    /// ends with it.
    pub(crate) fn in_savepoint<T, E>(
        &mut self,
        change: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        self.conn
            .execute_batch("SAVEPOINT book")
            .map_err(StoreError::from)?;
        match change(self) {
            Ok(value) => match self.conn.execute_batch("RELEASE book") {
                Ok(()) => Ok(value),
                Err(error) => {
                    self.roll_back();
                    Err(StoreError::from(error).into())
                }
            },
            Err(error) => {
                self.roll_back();
                Err(error)
            }
        }
    }

    /// Takes back everything since the last savepoint. If even that fails,
    /// the whole transaction is rolled back, so the book is never left half
    /// changed.
    fn roll_back(&self) {
        if self
            .conn
            .execute_batch("ROLLBACK TO book; RELEASE book")
            .is_err()
            && !self.conn.is_autocommit()
        {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

/// Deletes the changes in `schema` that `which`, an SQL condition on
/// `changes`, selects, with the rows they kept.
fn delete_changes(
    conn: &Connection,
    schema: &str,
    which: &str,
    params: &[&dyn ToSql],
) -> Result<(), StoreError> {
    for table in &RECORDED {
        conn.execute_cached(
            &format!(
                "DELETE FROM {schema}.changed_{name}
                 WHERE change_id IN (SELECT id FROM {schema}.changes WHERE {which})",
                name = table.name
            ),
            params,
        )?;
    }
    conn.execute_cached(
        &format!("DELETE FROM {schema}.changes WHERE {which}"),
        params,
    )?;
    Ok(())
}

/// Moves the whole history from the database `from` into `to`, which holds
/// none, e.g. into memory when the user keeps it for 0 days.
fn move_history(conn: &Connection, from: &str, to: &str) -> Result<(), StoreError> {
    conn.execute_cached(
        &format!(
            "INSERT INTO {to}.changes (id, sequence, action, made_at, undone, file_name)
             SELECT id, sequence, action, made_at, undone, file_name FROM {from}.changes"
        ),
        [],
    )?;
    for table in &RECORDED {
        let name = table.name;
        let columns = table.names("");
        conn.execute_cached(
            &format!(
                "INSERT INTO {to}.changed_{name} (change_id, side, {columns})
                 SELECT change_id, side, {columns} FROM {from}.changed_{name}"
            ),
            [],
        )?;
    }
    delete_changes(conn, from, "1", &[])
}

/// [`prune`] after a change, which only has to look further if the oldest
/// change is too old. Changes are made in the order of time, so the oldest
/// is the first to be too old, and looking at it is cheap, where [`prune`]
/// reads the whole history: done after every change, that made every change
/// slower than the one before (#84). Changes made with a wrong clock are
/// cleaned up when the book is opened, which does the whole look.
fn prune_after_change(
    conn: &Connection,
    keep: HistoryRetention,
    now: Timestamp,
) -> Result<(), StoreError> {
    if let HistoryRetention::Days(days @ 1..) = keep {
        let oldest: Option<String> = conn
            .query_row_cached(
                "SELECT made_at FROM main.changes ORDER BY sequence LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let expired = |made_at: &str| {
            let limit = now.as_second().saturating_sub(86_400 * i64::from(days));
            made_at
                .parse::<Timestamp>()
                .is_ok_and(|made_at| made_at.as_second() < limit)
        };
        if !oldest.as_deref().is_some_and(expired) {
            return Ok(());
        }
    }
    prune(conn, keep, now)
}

/// Deletes the changes the book no longer keeps at `now`: with 0 days every
/// change stored in the book, otherwise those made more than the days
/// before. It deletes from the oldest change up to the newest one that is
/// too old, so the history has no gaps even if a computer's clock was wrong
/// when a change was made, and the history in memory isn't touched.
fn prune(conn: &Connection, keep: HistoryRetention, now: Timestamp) -> Result<(), StoreError> {
    let last: Option<i64> = match keep {
        HistoryRetention::Forever => None,
        HistoryRetention::Days(0) => {
            conn.query_row_cached("SELECT max(sequence) FROM main.changes", [], |row| {
                row.get(0)
            })?
        }
        HistoryRetention::Days(days) => {
            let limit = now
                .as_second()
                .checked_sub(86_400 * i64::from(days))
                .and_then(|second| Timestamp::from_second(second).ok());
            match limit {
                Some(limit) => conn.query_row_cached(
                    "SELECT max(sequence) FROM main.changes WHERE made_at < ?1",
                    [limit.to_string()],
                    |row| row.get(0),
                )?,
                // Further back than any time, so nothing is that old.
                None => None,
            }
        }
    };
    match last {
        Some(last) => delete_changes(conn, "main", "sequence <= ?1", &[&last]),
        None => Ok(()),
    }
}

/// `time` without the fraction of a second, which the history doesn't keep.
fn whole_seconds(time: Timestamp) -> Timestamp {
    Timestamp::from_second(time.as_second()).unwrap_or(time)
}

/// Turns a refusal by one of SQLite's checks while rows are written back into
/// a history that doesn't fit the book.
fn mismatch_if_refused(error: rusqlite::Error) -> StoreError {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::ConstraintViolation) => {
            StoreError::Corrupt(Corruption::HistoryMismatch)
        }
        _ => error.into(),
    }
}

fn action_to_text(action: Action) -> &'static str {
    match action {
        Action::SetRoundingMode => "set_rounding_mode",
        Action::UseCurrency => "use_currency",
        Action::AddAccount => "add_account",
        Action::RenameAccount => "rename_account",
        Action::CloseAccount => "close_account",
        Action::ReopenAccount => "reopen_account",
        Action::AddCategory => "add_category",
        Action::RenameCategory => "rename_category",
        Action::MoveCategory => "move_category",
        Action::AddRate => "add_rate",
        Action::RemoveRate => "remove_rate",
        Action::AddTransaction => "add_transaction",
        Action::EditTransaction => "edit_transaction",
        Action::RemoveTransaction => "remove_transaction",
        Action::JoinTransfer => "join_transfer",
        Action::AddRule => "add_rule",
        Action::EditRule => "edit_rule",
        Action::MoveRule => "move_rule",
        Action::RemoveRule => "remove_rule",
        Action::RunRules => "run_rules",
        Action::ImportStatement => "import_statement",
        Action::CorrectStartingBalance => "correct_starting_balance",
        Action::AddInstitution => "add_institution",
        Action::AddPayee => "add_payee",
        Action::AddTag => "add_tag",
        Action::LinkBankAccount => "link_bank_account",
        Action::UnlinkBankAccount => "unlink_bank_account",
        Action::FetchStatement => "fetch_statement",
        Action::ApplyAiChanges => "apply_ai_changes",
        Action::RemoveCategory => "remove_category",
    }
}

fn action_from_text(text: &str) -> Result<Action, StoreError> {
    Action::ALL
        .into_iter()
        .find(|&action| action_to_text(action) == text)
        .ok_or_else(|| StoreError::Corrupt(Corruption::BadAction(text.into())))
}

#[cfg(test)]
mod tests;
