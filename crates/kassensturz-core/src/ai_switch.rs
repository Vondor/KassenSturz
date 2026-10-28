// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether AI is switched on (D-013, D-168), read without Qt.
//!
//! The one "AI features" toggle lives in the app's settings file next to the
//! other settings that belong to this computer, never in a book (D-168). The
//! app writes it with KConfig; programs without Qt, such as
//! `kassensturz-mcp`, read it here. Anything but an explicit `Enabled=true`
//! in the `[AI]` group, including a missing, unreadable or odd file, means
//! off: AI never starts on a guess (AGENTS.md §8).
//!
//! This module exists only in a build with AI (feature `ai`, D-039).

use std::path::{Path, PathBuf};

/// The settings group of the AI settings.
pub const GROUP: &str = "AI";

/// The key of the toggle in [`GROUP`].
pub const ENABLED_KEY: &str = "Enabled";

/// The value that switches AI on; every other value is off.
pub const ON: &str = "true";

/// The key of the second toggle in [`GROUP`]: whether the user's own AI
/// tools may prepare changes through `kassensturz-mcp` (D-035, D-219). It
/// counts only while AI itself is on.
pub const WRITE_TOOLS_KEY: &str = "McpWriteTools";

/// One row of the permission grid in the AI settings (D-252).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The categories.
    Categories,
    /// The Rules.
    Rules,
    /// The accounts.
    Accounts,
    /// The transactions.
    Transactions,
}

/// One cell of the permission grid: what the AI may do with one row (D-252).
/// Reading is a cell like the others; every change the AI prepares is shown
/// to the user and applied only after the user confirms it, whatever the
/// cells say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// Add categories.
    NewCategories,
    /// Add Rules.
    NewRules,
    /// Put transactions into categories (Transactions, Change; the field
    /// lock decides which fields).
    SortTransactions,
    /// Rename categories and move them.
    EditCategories,
    /// Change Rules and put them in another order.
    EditRules,
    /// Delete Rules.
    DeleteRules,
    /// Add transactions (Transactions, Add): payments in cash (D-258).
    NewTransactions,
    /// Remove transactions (Transactions, Delete): by hand, in cash accounts.
    DeleteTransactions,
    /// Read the categories.
    CategoriesRead,
    /// Read the Rules.
    RulesRead,
    /// Read the accounts and their balances.
    AccountsRead,
    /// Read the transactions.
    TransactionsRead,
    /// Delete categories that nothing uses (D-253).
    DeleteCategories,
    /// Add accounts (D-253).
    NewAccounts,
    /// Rename accounts, close them and open them again (D-253).
    EditAccounts,
}

impl Permission {
    /// Every permission, in the order the settings page shows them.
    pub const ALL: [Self; 15] = [
        Self::CategoriesRead,
        Self::NewCategories,
        Self::EditCategories,
        Self::DeleteCategories,
        Self::RulesRead,
        Self::NewRules,
        Self::EditRules,
        Self::DeleteRules,
        Self::AccountsRead,
        Self::NewAccounts,
        Self::EditAccounts,
        Self::TransactionsRead,
        Self::SortTransactions,
        Self::NewTransactions,
        Self::DeleteTransactions,
    ];

    /// The key of this permission in [`GROUP`].
    pub const fn key(self) -> &'static str {
        match self {
            Self::NewCategories => "AllowNewCategories",
            Self::NewRules => "AllowNewRules",
            Self::SortTransactions => "AllowSortTransactions",
            Self::NewTransactions => "AllowNewTransactions",
            Self::DeleteTransactions => "AllowDeleteTransactions",
            Self::EditCategories => "AllowEditCategories",
            Self::EditRules => "AllowEditRules",
            Self::DeleteRules => "AllowDeleteRules",
            Self::CategoriesRead => "AllowReadCategories",
            Self::RulesRead => "AllowReadRules",
            Self::AccountsRead => "AllowReadAccounts",
            Self::TransactionsRead => "AllowReadTransactions",
            Self::DeleteCategories => "AllowDeleteCategories",
            Self::NewAccounts => "AllowNewAccounts",
            Self::EditAccounts => "AllowEditAccounts",
        }
    }

    /// The row this permission belongs to.
    pub const fn row(self) -> Row {
        match self {
            Self::NewCategories
            | Self::EditCategories
            | Self::DeleteCategories
            | Self::CategoriesRead => Row::Categories,
            Self::NewRules | Self::EditRules | Self::DeleteRules | Self::RulesRead => Row::Rules,
            Self::AccountsRead | Self::NewAccounts | Self::EditAccounts => Row::Accounts,
            Self::SortTransactions
            | Self::NewTransactions
            | Self::DeleteTransactions
            | Self::TransactionsRead => Row::Transactions,
        }
    }

    /// Whether it is a "Read" cell.
    pub const fn is_read(self) -> bool {
        matches!(
            self,
            Self::CategoriesRead | Self::RulesRead | Self::AccountsRead | Self::TransactionsRead
        )
    }

    /// Whether it is a cell that changes or removes what is there (Change
    /// or Delete). These switch Read of their row on (D-252).
    pub const fn needs_read(self) -> bool {
        matches!(
            self,
            Self::EditCategories
                | Self::EditRules
                | Self::DeleteRules
                | Self::SortTransactions
                | Self::DeleteTransactions
                | Self::DeleteCategories
                | Self::EditAccounts
        )
    }

    /// The Read cell of `row`.
    pub const fn read_of(row: Row) -> Self {
        match row {
            Row::Categories => Self::CategoriesRead,
            Row::Rules => Self::RulesRead,
            Row::Accounts => Self::AccountsRead,
            Row::Transactions => Self::TransactionsRead,
        }
    }

    /// Whether it is on when the user never chose: no cell is. The user
    /// switches on what they want the AI to do (D-252).
    pub const fn on_by_default(self) -> bool {
        false
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A field of a transaction the user can lock against the AI (D-252).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// The amount.
    Amount,
    /// The date.
    Date,
    /// The payee.
    Payee,
    /// The account.
    Account,
    /// The category.
    Category,
    /// The note.
    Note,
}

impl Field {
    /// Every field, in the order the settings page shows them.
    pub const ALL: [Self; 6] = [
        Self::Amount,
        Self::Date,
        Self::Payee,
        Self::Account,
        Self::Category,
        Self::Note,
    ];

    /// The key of this field's lock in [`GROUP`].
    pub const fn key(self) -> &'static str {
        match self {
            Self::Amount => "LockAmount",
            Self::Date => "LockDate",
            Self::Payee => "LockPayee",
            Self::Account => "LockAccount",
            Self::Category => "LockCategory",
            Self::Note => "LockNote",
        }
    }

    /// The field's fixed name, as the AI's functions and the error codes
    /// write it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Amount => "amount",
            Self::Date => "date",
            Self::Payee => "payee",
            Self::Account => "account",
            Self::Category => "category",
            Self::Note => "note",
        }
    }

    /// Whether it is locked when the user never chose: the fields a wrong
    /// guess would hurt most (D-252).
    pub const fn locked_by_default(self) -> bool {
        matches!(self, Self::Amount | Self::Date | Self::Payee)
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// What the AI may do (D-239, D-252): the cells of the grid and the locked
/// fields of transactions. It decides what a model is offered to read and
/// to prepare and what a prepared change set may hold; a change set is in
/// every case applied only after the user confirms it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    cells: u16,
    locks: u8,
}

impl Permissions {
    /// Nothing may be read or prepared. Fields keep their default locks.
    pub const NONE: Self = Self {
        cells: 0,
        locks: Self::default_locks(),
    };

    const fn default_locks() -> u8 {
        let mut bits = 0;
        let mut index = 0;
        while index < Field::ALL.len() {
            if Field::ALL[index].locked_by_default() {
                bits |= Field::ALL[index].bit();
            }
            index += 1;
        }
        bits
    }

    /// What a user who never chose gets: nothing (D-252).
    pub fn defaults() -> Self {
        Self::of(
            |permission| permission.on_by_default(),
            |field| field.locked_by_default(),
        )
    }

    /// Every cell, with the default locks, for tests and tools that check a
    /// set on its own terms.
    pub fn all() -> Self {
        Self::of(|_| true, |field| field.locked_by_default())
    }

    fn of(on: impl Fn(Permission) -> bool, locked: impl Fn(Field) -> bool) -> Self {
        let cells = Permission::ALL
            .iter()
            .filter(|permission| on(**permission))
            .fold(0, |bits, permission| bits | permission.bit());
        let locks = Field::ALL
            .iter()
            .filter(|field| locked(**field))
            .fold(0, |bits, field| bits | field.bit());
        Self { cells, locks }
    }

    /// Whether `permission` is given. A change or a deletion also gives the
    /// reading of its row: the AI can't change what it can't see (D-252).
    pub fn allows(self, permission: Permission) -> bool {
        self.cells & permission.bit() != 0
            || (permission.is_read()
                && Permission::ALL.iter().any(|other| {
                    other.needs_read()
                        && other.row() == permission.row()
                        && self.cells & other.bit() != 0
                }))
    }

    /// Whether the AI may read `row`.
    pub fn reads(self, row: Row) -> bool {
        self.allows(Permission::read_of(row))
    }

    /// These permissions with `permission` given or taken away.
    #[must_use]
    pub fn with(self, permission: Permission, on: bool) -> Self {
        let cells = if on {
            self.cells | permission.bit()
        } else {
            self.cells & !permission.bit()
        };
        Self { cells, ..self }
    }

    /// Whether `field` of a transaction is locked against the AI.
    pub fn locked(self, field: Field) -> bool {
        self.locks & field.bit() != 0
    }

    /// These permissions with `field` locked or unlocked.
    #[must_use]
    pub fn with_lock(self, field: Field, locked: bool) -> Self {
        let locks = if locked {
            self.locks | field.bit()
        } else {
            self.locks & !field.bit()
        };
        Self { locks, ..self }
    }

    /// Whether the AI may put transactions into categories: Transactions,
    /// Change, and the Category field not locked (D-252).
    pub fn sorts(self) -> bool {
        self.allows(Permission::SortTransactions) && !self.locked(Field::Category)
    }

    /// Whether nothing may be read or prepared.
    pub fn is_none(self) -> bool {
        self.cells == 0
    }

    /// Whether any kind of change may be prepared.
    pub fn can_change(self) -> bool {
        Permission::ALL
            .iter()
            .any(|permission| !permission.is_read() && self.allows(*permission))
    }
}

/// The settings file's name in the user's settings folder.
const FILE: &str = "kassensturzrc";

/// Whether the settings file's `text` switches AI on.
pub fn enabled_in(text: &str) -> bool {
    value_in(text, GROUP, ENABLED_KEY) == Some(ON)
}

/// Whether the settings file's `text` switches AI and the MCP write tools
/// on. The write tools are off whenever AI is (D-013).
pub fn write_tools_in(text: &str) -> bool {
    enabled_in(text) && value_in(text, GROUP, WRITE_TOOLS_KEY) == Some(ON)
}

/// The permissions of the settings file's `text` (D-239, D-252). None while
/// AI is off. A permission the user never chose has its default (off); one
/// with any value but an explicit `true` or `false` is off. A lock the user
/// never chose has its default; one with any value but an explicit `false`
/// is locked. So a damaged file never gives more than the user chose.
pub fn permissions_in(text: &str) -> Permissions {
    if !enabled_in(text) {
        return Permissions::NONE;
    }
    Permissions::of(
        |permission| match value_in(text, GROUP, permission.key()) {
            None => permission.on_by_default(),
            Some(value) => value == ON,
        },
        |field| match value_in(text, GROUP, field.key()) {
            None => field.locked_by_default(),
            Some(value) => value != "false",
        },
    )
}

/// The value of `key` in `group` of the settings file's `text`, with the
/// spaces around it removed, as KConfig wrote it: the last entry of a key
/// wins, as in KConfig. KConfig's escapes (`\s`, `\\` and so on) are left
/// as they are, for the caller to refuse or read.
pub fn value_in<'t>(text: &'t str, group: &str, key: &str) -> Option<&'t str> {
    let mut in_group = false;
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = name == group;
        } else if in_group
            && let Some((name, value)) = line.split_once('=')
            && name.trim_end() == key
        {
            found = Some(value.trim());
        }
    }
    found
}

/// Whether the settings file at `path` switches AI on. A file that can't be
/// read is off.
pub fn enabled_at(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| enabled_in(&text))
}

/// Where KConfig keeps the app's settings file, where this build knows it.
/// `None` elsewhere, which counts as off until the Windows and macOS
/// packaging steps (M7) say where their settings go.
pub fn settings_file() -> Option<PathBuf> {
    if cfg!(not(target_os = "linux")) {
        return None;
    }
    let folder = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(folder) if !folder.is_empty() => PathBuf::from(folder),
        _ => {
            PathBuf::from(std::env::var_os("HOME").filter(|home| !home.is_empty())?).join(".config")
        }
    };
    Some(folder.join(FILE))
}

/// Whether AI is switched on in this user's settings.
pub fn enabled() -> bool {
    settings_file().is_some_and(|path| enabled_at(&path))
}

/// Whether AI and the MCP write tools are both switched on in this user's
/// settings. A file that can't be read is off.
pub fn write_tools_enabled() -> bool {
    settings_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .is_some_and(|text| write_tools_in(&text))
}

/// The permissions in this user's settings. None where the settings file
/// can't be read, as with the AI switch.
pub fn permissions() -> Permissions {
    settings_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or(Permissions::NONE, |text| permissions_in(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_explicit_true_in_the_ai_group_is_on() {
        assert!(!enabled_in(""));
        assert!(!enabled_in("[AI]\n"));
        assert!(enabled_in("[AI]\nEnabled=true\n"));
        assert!(enabled_in("[Books]\nLastBook=/a\n\n[AI]\nEnabled = true\n"));
        assert!(!enabled_in("[AI]\nEnabled=false\n"));
        assert!(!enabled_in("[AI]\nEnabled=1\n"));
        assert!(!enabled_in("[AI]\nEnabled=True\n"));
        // The key in another group, or another key, doesn't count.
        assert!(!enabled_in("[Other]\nEnabled=true\n"));
        assert!(!enabled_in("[AI]\nEnabledX=true\n"));
        // The last entry wins.
        assert!(!enabled_in("[AI]\nEnabled=true\nEnabled=false\n"));
        assert!(enabled_in("[AI]\nEnabled=false\nEnabled=true\n"));
    }

    #[test]
    fn values_are_read_from_their_group() {
        let text = "[AI]\nModel = a\n[Other]\nModel=b\n[AI]\nModel=c\nEmpty=\n";
        assert_eq!(value_in(text, "AI", "Model"), Some("c"));
        assert_eq!(value_in(text, "Other", "Model"), Some("b"));
        assert_eq!(value_in(text, "AI", "Empty"), Some(""));
        assert_eq!(value_in(text, "AI", "Missing"), None);
        assert_eq!(value_in(text, "None", "Model"), None);
    }

    #[test]
    fn the_write_tools_are_on_only_with_ai_on() {
        assert!(!write_tools_in(""));
        assert!(!write_tools_in("[AI]\nMcpWriteTools=true\n"));
        assert!(!write_tools_in("[AI]\nEnabled=true\n"));
        assert!(!write_tools_in("[AI]\nEnabled=false\nMcpWriteTools=true\n"));
        assert!(!write_tools_in("[AI]\nEnabled=true\nMcpWriteTools=1\n"));
        assert!(write_tools_in("[AI]\nEnabled=true\nMcpWriteTools=true\n"));
        assert!(!write_tools_in(
            "[AI]\nEnabled=true\n[Other]\nMcpWriteTools=true\n"
        ));
    }

    #[test]
    fn permissions_are_none_with_ai_off_and_everything_is_off_by_default() {
        assert!(permissions_in("").is_none());
        assert!(permissions_in("[AI]\nAllowEditRules=true\n").is_none());
        let defaults = permissions_in("[AI]\nEnabled=true\n");
        assert_eq!(defaults, Permissions::defaults());
        for permission in Permission::ALL {
            assert!(!defaults.allows(permission), "{permission:?}");
        }
        assert!(defaults.is_none() && !defaults.can_change());
    }

    #[test]
    fn a_chosen_permission_wins_and_an_odd_value_is_off() {
        let text = "[AI]\nEnabled=true\nAllowNewRules=true\nAllowDeleteRules=true\n\
                    AllowEditRules=yes\nAllowSortTransactions=1\nAllowNewCategories=false\n";
        let got = permissions_in(text);
        assert!(got.allows(Permission::NewRules));
        assert!(got.allows(Permission::DeleteRules));
        assert!(!got.allows(Permission::EditRules));
        assert!(!got.allows(Permission::SortTransactions));
        assert!(!got.allows(Permission::NewCategories));
    }

    #[test]
    fn a_change_or_a_deletion_gives_the_reading_of_its_row() {
        let got = permissions_in("[AI]\nEnabled=true\nAllowDeleteRules=true\n");
        assert!(got.reads(Row::Rules));
        assert!(!got.reads(Row::Categories) && !got.reads(Row::Accounts));
        // Adding alone does not.
        let added = permissions_in("[AI]\nEnabled=true\nAllowNewRules=true\n");
        assert!(!added.reads(Row::Rules));
        assert!(added.can_change());
        let sorted = permissions_in("[AI]\nEnabled=true\nAllowSortTransactions=true\n");
        assert!(sorted.reads(Row::Transactions) && !sorted.reads(Row::Categories));
    }

    #[test]
    fn the_amount_the_date_and_the_payee_are_locked_until_the_user_chooses() {
        let text = "[AI]\nEnabled=true\n";
        let got = permissions_in(text);
        for field in Field::ALL {
            assert_eq!(got.locked(field), field.locked_by_default(), "{field:?}");
        }
        assert!(got.locked(Field::Amount) && !got.locked(Field::Note));
        let chosen =
            permissions_in("[AI]\nEnabled=true\nLockAmount=false\nLockNote=true\nLockDate=maybe\n");
        assert!(!chosen.locked(Field::Amount));
        assert!(chosen.locked(Field::Note));
        assert!(chosen.locked(Field::Date), "an odd value stays locked");
        assert!(chosen.with_lock(Field::Amount, true).locked(Field::Amount));
        let keys: std::collections::HashSet<_> = Field::ALL.iter().map(|f| f.key()).collect();
        assert_eq!(keys.len(), Field::ALL.len());
    }

    #[test]
    fn with_gives_and_takes_one_permission() {
        let all = Permissions::all();
        let less = all.with(Permission::EditRules, false);
        assert!(!less.allows(Permission::EditRules));
        assert!(less.allows(Permission::DeleteRules));
        assert_eq!(less.with(Permission::EditRules, true), all);
        let keys: std::collections::HashSet<_> = Permission::ALL.iter().map(|p| p.key()).collect();
        assert_eq!(keys.len(), Permission::ALL.len());
    }

    #[test]
    fn a_missing_file_is_off() {
        assert!(!enabled_at(Path::new("/nonexistent/kassensturzrc")));
    }
}
