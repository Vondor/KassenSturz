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

/// The key in [`GROUP`] of the folder the user chose for their bills
/// (US5.13, D-739): the user's own AI tools may link a file inside it to a
/// transaction. Kept per computer, as the system's folder chooser returned
/// it.
pub const BILL_FOLDER_KEY: &str = "BillFolder";

/// The key in [`GROUP`] of the receipt inbox (US7, #333, D-752): the
/// folder the user drops new bills into. Kept per computer, as the
/// system's folder chooser returned it.
pub const INBOX_FOLDER_KEY: &str = "InboxFolder";

/// The key in [`GROUP`] of the archive the receipt inbox files bills into
/// (US7, #333, D-752), per computer like the inbox.
pub const ARCHIVE_FOLDER_KEY: &str = "ArchiveFolder";

/// The key in [`GROUP`] of the user's own "Notes for the AI" (US7, #808):
/// free text the user wrote, kept per computer and never in a book.
pub const NOTES_KEY: &str = "Notes";

/// How many characters the notes may have. The notes go into every chat
/// request, so a limit keeps them from using up a small model's context
/// (D-052); about a page of text.
pub const MAX_NOTES_CHARS: usize = 2000;

/// The key in [`GROUP`] (`[AI]`) of the user's custom chat quick-prompt
/// buttons (US11.3, #1452, D-1013): kept per computer in `kassensturzrc`,
/// never in a book (D-892), and ignored while AI is off (D-013).
pub const CUSTOM_CHAT_BUTTONS_KEY: &str = "CustomChatButtons";

/// Maximum number of user-defined custom chat buttons (D-1013).
pub const MAX_CUSTOM_CHAT_BUTTONS: usize = 20;

/// Maximum length (in Unicode scalar values) of a custom button's label.
pub const MAX_CHAT_BUTTON_LABEL_CHARS: usize = 60;

/// Maximum length (in Unicode scalar values) of a custom button's prompt.
pub const MAX_CHAT_BUTTON_PROMPT_CHARS: usize = 2000;

/// Maximum length (in bytes) of a custom button's ASCII identifier.
pub const MAX_CHAT_BUTTON_ID_BYTES: usize = 64;

/// One of the four built-in quick-prompt buttons in the AI chat panel
/// (US11.3, #1452, D-1013). Each is shown by default when its key is absent
/// from `[AI]` in `kassensturzrc`, and hidden when its key is `false` or
/// while AI is off (D-013).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinChatButton {
    /// "Suggest categories" (`categories`).
    Categories,
    /// "Explain this month" (`month`).
    Month,
    /// "Prepare Rules" (`rules`).
    Rules,
    /// "Describe a chart" (`describe_chart`).
    DescribeChart,
}

impl BuiltinChatButton {
    /// Every built-in chat button, in the order the UI shows them.
    pub const ALL: [Self; 4] = [
        Self::Categories,
        Self::Month,
        Self::Rules,
        Self::DescribeChart,
    ];

    /// The stable identifier used between QML and Rust.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Categories => "categories",
            Self::Month => "month",
            Self::Rules => "rules",
            Self::DescribeChart => "describe_chart",
        }
    }

    /// Looks up a built-in chat button by its stable identifier.
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|button| button.word() == word)
    }

    /// The key of this built-in chat button in [`GROUP`].
    pub const fn key(self) -> &'static str {
        match self {
            Self::Categories => "ChatButtonCategories",
            Self::Month => "ChatButtonMonth",
            Self::Rules => "ChatButtonRules",
            Self::DescribeChart => "ChatButtonDescribeChart",
        }
    }

    /// Whether the button is on when the user never changed it: every
    /// built-in button starts on so older settings files keep the exact
    /// previous behavior (D-1013).
    pub const fn on_by_default(self) -> bool {
        true
    }
}

/// One user-defined custom quick-prompt button for the AI chat panel
/// (US11.3, #1452, D-1013).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomChatButton {
    /// Stable ASCII identifier (`a-z`, `A-Z`, `0-9`, `-`, `_`).
    pub id: String,
    /// Single-line button label (1..=[`MAX_CHAT_BUTTON_LABEL_CHARS`] chars).
    pub label: String,
    /// Prompt text sent or inserted into the chat input
    /// (1..=[`MAX_CHAT_BUTTON_PROMPT_CHARS`] chars).
    pub prompt: String,
    /// Whether the button is shown in the chat panel.
    pub enabled: bool,
    /// When `false` (default), clicking sends the prompt immediately; when
    /// `true`, clicking inserts the prompt into the chat input box without
    /// sending so the user can edit it first (D-1013).
    pub insert_only: bool,
}

/// Whether `id` is a safe ASCII identifier for a custom chat button.
pub fn is_valid_chat_button_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_CHAT_BUTTON_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Checks a custom button label typed by the user: single-line, non-empty
/// after trimming, no NUL or control characters, at most
/// [`MAX_CHAT_BUTTON_LABEL_CHARS`] characters.
pub fn validate_chat_button_label(raw: &str) -> Option<String> {
    if raw.chars().any(|c| c == '\0' || c.is_control()) {
        return None;
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_CHAT_BUTTON_LABEL_CHARS {
        return None;
    }
    Some(trimmed.to_owned())
}

/// Reads a custom button label from `kassensturzrc`: rejects NULs, drops
/// control characters (turning line breaks and tabs into spaces), trims,
/// and caps at [`MAX_CHAT_BUTTON_LABEL_CHARS`]. Returns `None` if empty.
pub fn sanitize_chat_button_label(raw: &str) -> Option<String> {
    if raw.contains('\0') {
        return None;
    }
    let mut cleaned = String::with_capacity(raw.len());
    for c in raw.chars() {
        if matches!(c, '\n' | '\r' | '\t') {
            cleaned.push(' ');
        } else if !c.is_control() {
            cleaned.push(c);
        }
    }
    let trimmed: String = cleaned
        .trim()
        .chars()
        .take(MAX_CHAT_BUTTON_LABEL_CHARS)
        .collect();
    let trimmed = trimmed.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Checks a custom button prompt typed by the user: non-empty after
/// trimming, no NUL or control characters other than line breaks and tabs,
/// normalized to `\n`, at most [`MAX_CHAT_BUTTON_PROMPT_CHARS`] characters.
pub fn validate_chat_button_prompt(raw: &str) -> Option<String> {
    if raw
        .chars()
        .any(|c| c == '\0' || (c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
    {
        return None;
    }
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_CHAT_BUTTON_PROMPT_CHARS {
        return None;
    }
    Some(trimmed.to_owned())
}

/// Reads a custom button prompt from `kassensturzrc`: rejects NULs,
/// normalizes line breaks to `\n`, drops control characters other than
/// `\n` and `\t`, trims, and caps at [`MAX_CHAT_BUTTON_PROMPT_CHARS`].
/// Returns `None` if empty.
pub fn sanitize_chat_button_prompt(raw: &str) -> Option<String> {
    if raw.contains('\0') {
        return None;
    }
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let cleaned: String = normalized
        .chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect();
    let capped: String = cleaned
        .trim()
        .chars()
        .take(MAX_CHAT_BUTTON_PROMPT_CHARS)
        .collect();
    let trimmed = capped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

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
    /// The saved charts (D-282): the AI may read, propose, change and
    /// delete them (D-681).
    Charts,
    /// The budgets (D-358): read, add and change; never delete.
    Budgets,
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
    /// Propose a new chart for the user to save (Charts, Add; D-282).
    NewViews,
    /// Read the budgets (Budgets, Read; D-358).
    BudgetsRead,
    /// Prepare new budgets (Budgets, Add; D-358).
    NewBudgets,
    /// Change the amount or carry over of budgets from the current period
    /// on (Budgets, Change; D-358).
    EditBudgets,
    /// Read the saved charts (Charts, Read; D-681).
    ChartsRead,
    /// Change a saved chart (Charts, Change; D-681).
    EditViews,
    /// Delete a saved chart (Charts, Delete; D-681).
    DeleteViews,
    /// Write a custom chart's description (Chart descriptions; D-709,
    /// D-729). It adds nothing alone: a new custom chart also needs
    /// [`Self::NewViews`], a changed description of a saved one
    /// [`Self::EditViews`].
    CustomCharts,
    /// Join two sides of a move between the user's accounts as one
    /// Transfer (Moves between your accounts, Change; D-693). Its own cell,
    /// not part of Transactions › Change: a user who allowed sorting never
    /// allowed merging two transactions into one. It reads the
    /// transactions, so it switches Transactions › Read on.
    JoinTransfers,
    /// Put tags on whole transactions and take them off (Tags on
    /// transactions, Change; US5.2, D-744). Its own cell, off by default: a
    /// tag is the user's own word, and a user who allowed sorting never
    /// allowed labelling. Only tags the book already has; it reads the
    /// transactions, so it switches Transactions › Read on.
    TagTransactions,
    /// Break a transaction into parts, or join its parts back into one
    /// (Parts of transactions, Change; US5.1, D-737). Its own cell, off by
    /// default: a user who allowed sorting never allowed the AI to divide a
    /// payment. The account's own part never changes, and the parts must
    /// add up to it exactly. It reads the transactions, so it switches
    /// Transactions › Read on.
    SplitTransactions,
    /// Link a bill from the bill folder to a transaction (Transactions,
    /// Attach bills; US5.13, D-739). Offered to the user's own AI tools
    /// only, over MCP, and only while a bill folder is chosen. Its own
    /// cell, off by default: a link may name only a file inside the folder
    /// the user chose. It reads the transactions, so it switches
    /// Transactions › Read on.
    AttachBills,
    /// Move a bill from the receipt inbox into the archive and link it to
    /// a transaction (Transactions, File bills from the inbox; US7, #333,
    /// D-752). Offered to the user's own AI tools only, over MCP, and only
    /// while both folders are chosen. Its own cell, off by default, apart
    /// from Attach bills, so a user who allowed links never gets moves by
    /// accident (D-739). It reads the transactions, so it switches
    /// Transactions › Read on.
    FileBills,
    /// Prepare new planned payments (Planned payments, Add; US7, B-357,
    /// D-814). Its own cell, off by default: a plan is what the user
    /// expects in the future, so a user who allowed sorting never allowed
    /// the AI to plan. The amount is one the user named or one an entry
    /// was booked with, never one the model worked out. It reads the
    /// transactions, so it switches Transactions › Read on.
    NewPlans,
    /// Change the amount, how often, the end, the name, the category or
    /// the pause of planned payments (Planned payments, Change; US7,
    /// B-357, D-814). It reads the transactions, so it switches
    /// Transactions › Read on.
    EditPlans,
    /// Make new tags for the transactions a change set tags (Tags on
    /// transactions, Add; D-830). Its own cell, off by default and apart
    /// from [`Self::TagTransactions`]: a user who allowed the AI to use
    /// their tags never allowed it to make new words for them. A new tag is
    /// made only together with a transaction it goes on, so it needs
    /// [`Self::TagTransactions`] too to be offered; it reads the
    /// transactions, so it switches Transactions › Read on.
    NewTags,
}

impl Permission {
    /// Every permission, in the order the settings page shows them.
    pub const ALL: [Self; 31] = [
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
        Self::ChartsRead,
        Self::NewViews,
        Self::EditViews,
        Self::DeleteViews,
        Self::CustomCharts,
        Self::BudgetsRead,
        Self::NewBudgets,
        Self::EditBudgets,
        Self::JoinTransfers,
        Self::TagTransactions,
        Self::NewTags,
        Self::SplitTransactions,
        Self::AttachBills,
        Self::FileBills,
        Self::NewPlans,
        Self::EditPlans,
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
            Self::NewViews => "AllowNewViews",
            Self::BudgetsRead => "AllowReadBudgets",
            Self::NewBudgets => "AllowNewBudgets",
            Self::EditBudgets => "AllowEditBudgets",
            Self::ChartsRead => "AllowReadViews",
            Self::EditViews => "AllowEditViews",
            Self::DeleteViews => "AllowDeleteViews",
            Self::CustomCharts => "AllowCustomCharts",
            Self::JoinTransfers => "AllowJoinTransfers",
            Self::TagTransactions => "AllowTagTransactions",
            Self::SplitTransactions => "AllowSplitTransactions",
            Self::AttachBills => "AllowAttachBills",
            Self::FileBills => "AllowFileBills",
            Self::NewPlans => "AllowNewPlans",
            Self::EditPlans => "AllowEditPlans",
            Self::NewTags => "AllowNewTags",
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
            | Self::TransactionsRead
            | Self::JoinTransfers
            | Self::TagTransactions
            | Self::SplitTransactions
            | Self::AttachBills
            | Self::FileBills
            | Self::NewPlans
            | Self::EditPlans
            | Self::NewTags => Row::Transactions,
            Self::ChartsRead
            | Self::NewViews
            | Self::EditViews
            | Self::DeleteViews
            | Self::CustomCharts => Row::Charts,
            Self::BudgetsRead | Self::NewBudgets | Self::EditBudgets => Row::Budgets,
        }
    }

    /// Whether it is a "Read" cell.
    pub const fn is_read(self) -> bool {
        matches!(
            self,
            Self::CategoriesRead
                | Self::RulesRead
                | Self::AccountsRead
                | Self::TransactionsRead
                | Self::BudgetsRead
                | Self::ChartsRead
        )
    }

    /// Whether it is a cell that changes or removes what is there (Change
    /// or Delete). These switch Read of their row on (D-252). So does
    /// Budgets, Add: a budget is prepared from what the category spent, and
    /// a new one must not overlap one the AI can't see (D-358).
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
                | Self::NewBudgets
                | Self::EditBudgets
                | Self::EditViews
                | Self::DeleteViews
                | Self::JoinTransfers
                | Self::TagTransactions
                | Self::SplitTransactions
                | Self::AttachBills
                | Self::FileBills
                | Self::NewPlans
                | Self::EditPlans
                | Self::NewTags
        )
    }

    /// The Read cell of `row`, if it has one.
    pub const fn read_of(row: Row) -> Option<Self> {
        match row {
            Row::Categories => Some(Self::CategoriesRead),
            Row::Rules => Some(Self::RulesRead),
            Row::Accounts => Some(Self::AccountsRead),
            Row::Transactions => Some(Self::TransactionsRead),
            Row::Charts => Some(Self::ChartsRead),
            Row::Budgets => Some(Self::BudgetsRead),
        }
    }

    /// Whether it is on when the user never chose: no cell is. The user
    /// switches on what they want the AI to do (D-252).
    pub const fn on_by_default(self) -> bool {
        false
    }

    const fn bit(self) -> u32 {
        1 << self as u32
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
    cells: u32,
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
        Permission::read_of(row).is_some_and(|read| self.allows(read))
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

    /// What `self` and `other` allow together: a cell only where both give
    /// it, a field locked where either locks it (B-436).
    #[must_use]
    pub fn narrowest(self, other: Self) -> Self {
        Self {
            cells: self.cells & other.cells,
            locks: self.locks | other.locks,
        }
    }

    /// Whether nothing may be read or prepared.
    pub fn is_none(self) -> bool {
        self.cells == 0
    }

    /// Whether any kind of change may be prepared. Chart descriptions
    /// alone prepare nothing: they only widen Charts › Add and Change
    /// (D-729).
    pub fn can_change(self) -> bool {
        Permission::ALL.iter().any(|permission| {
            !permission.is_read()
                && *permission != Permission::CustomCharts
                && self.allows(*permission)
        })
    }

    /// Whether the AI may write a custom chart's description: the cell
    /// Chart descriptions with Charts › Add or Charts › Change (D-729).
    pub fn writes_chart_descriptions(self) -> bool {
        self.allows(Permission::CustomCharts)
            && (self.allows(Permission::NewViews) || self.allows(Permission::EditViews))
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

/// Whether the built-in chat button `button` is enabled in the settings
/// file's `text` (US11.3, #1452, D-1013). Always `false` while AI is off
/// (D-013); otherwise `true` unless explicitly set to `false`, so older
/// settings files keep every built-in button on.
pub fn builtin_chat_button_in(text: &str, button: BuiltinChatButton) -> bool {
    enabled_in(text) && value_in(text, GROUP, button.key()) != Some("false")
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

/// The bill folder of the settings file's `text` (D-739): `None` while AI
/// is off, when none is chosen, or when the entry is not a plain absolute
/// path. KConfig's escapes of a written path (`\\`, `\s`, `\t`, `\n`,
/// `\r`) are read back; any other escape leaves the folder unset, so a
/// damaged file never names another folder.
pub fn bill_folder_in(text: &str) -> Option<PathBuf> {
    folder_in(text, BILL_FOLDER_KEY)
}

/// The receipt inbox of the settings file's `text` (US7, #333), read as
/// [`bill_folder_in`] reads the bill folder.
pub fn inbox_folder_in(text: &str) -> Option<PathBuf> {
    folder_in(text, INBOX_FOLDER_KEY)
}

/// The archive of the receipt inbox in the settings file's `text` (US7,
/// #333), read as [`bill_folder_in`] reads the bill folder.
pub fn archive_folder_in(text: &str) -> Option<PathBuf> {
    folder_in(text, ARCHIVE_FOLDER_KEY)
}

/// The folder under `key` of [`GROUP`], as [`bill_folder_in`] describes.
fn folder_in(text: &str, key: &str) -> Option<PathBuf> {
    if !enabled_in(text) {
        return None;
    }
    let raw = value_in(text, GROUP, key)?;
    let mut path = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            path.push(c);
            continue;
        }
        path.push(match chars.next()? {
            '\\' => '\\',
            's' => ' ',
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            _ => return None,
        });
    }
    let path = PathBuf::from(path);
    path.is_absolute().then_some(path)
}

/// The user's "Notes for the AI" in the settings file's `text` (US7, #808):
/// empty while AI is off or when there are none. KConfig's escapes of a
/// written text (`\\`, `\s`, `\t`, `\n`, `\r`) are read back; an entry with
/// any other escape counts as no notes, so a damaged file never sends
/// something the user did not write. Control characters other than line
/// breaks are dropped, and the text is cut at [`MAX_NOTES_CHARS`].
pub fn notes_in(text: &str) -> String {
    if !enabled_in(text) {
        return String::new();
    }
    let Some(raw) = value_in(text, GROUP, NOTES_KEY) else {
        return String::new();
    };
    let mut notes = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        let c = if c == '\\' {
            match chars.next() {
                Some('\\') => '\\',
                Some('s') => ' ',
                Some('t') => '\t',
                Some('n') => '\n',
                Some('r') => '\r',
                _ => return String::new(),
            }
        } else {
            c
        };
        if c == '\n' || c == '\t' || !c.is_control() {
            notes.push(c);
        }
    }
    notes
        .chars()
        .take(MAX_NOTES_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The newest settings text (the Store app's copy where there is one, B-448),
/// but only when AI is on in every file that counts (B-436). Folders and
/// notes are read from it, so a folder changed in the Store app is the one
/// the MCP server and the chat use, not the older one of the real
/// `%LOCALAPPDATA%` file.
fn newest_enabled_text() -> Option<String> {
    newest_enabled_text_of(&settings_files())
}

/// [`newest_enabled_text`] over `files`, the newest last.
fn newest_enabled_text_of(files: &[PathBuf]) -> Option<String> {
    let texts = texts_of(files)?;
    all_enabled(&texts).then(|| texts.into_iter().next_back())?
}

/// The user's notes in this user's settings, empty when there are none or
/// AI is off.
pub fn notes() -> String {
    newest_enabled_text().map_or_else(String::new, |text| notes_in(&text))
}

/// The bill folder in this user's settings, if one is chosen and AI is on.
pub fn bill_folder() -> Option<PathBuf> {
    newest_enabled_text().and_then(|text| bill_folder_in(&text))
}

/// The receipt inbox and its archive in this user's settings (US7, #333):
/// `None` unless AI is on and both are chosen.
pub fn inbox_folders() -> Option<(PathBuf, PathBuf)> {
    let text = newest_enabled_text()?;
    Some((inbox_folder_in(&text)?, archive_folder_in(&text)?))
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

/// Where KConfig keeps the app's settings file, where this build knows it:
/// on Linux in `$XDG_CONFIG_HOME` or `~/.config`, and on Windows in
/// `%LOCALAPPDATA%`, Qt's `GenericConfigLocation` there (B-322). `None`
/// elsewhere, which counts as off until the macOS packaging says where its
/// settings go.
pub fn settings_file() -> Option<PathBuf> {
    settings_file_for(std::env::consts::OS, &|name| {
        std::env::var_os(name).filter(|value| !value.is_empty())
    })
}

/// [`settings_file`] on the system `os` (as `std::env::consts::OS` names
/// it), with the environment read through `var`.
///
/// On Windows `XDG_CONFIG_HOME` comes first: Qt there doesn't read it, but
/// the app's test profile sets it to the folder of the settings file it
/// keeps for itself, so the switch reads that file in the test profile
/// too (Q-399). Without it, `%LOCALAPPDATA%` is where KConfig writes.
fn settings_file_for(
    os: &str,
    var: &impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let folder = match os {
        "linux" => var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".config")))?,
        "windows" => var("XDG_CONFIG_HOME")
            .or_else(|| var("LOCALAPPDATA"))
            .map(PathBuf::from)?,
        _ => return None,
    };
    Some(folder.join(FILE))
}

/// The Microsoft Store package's name (`Identity Name` in
/// `packaging/windows/msix/AppxManifest.xml`). Its family name is this, an
/// underscore and the publisher's hash.
const STORE_PACKAGE_NAME: &str = "47425SerpentSoftware.KassenSturz-TheHouseholdBudge";

/// Every settings file whose word counts for the AI switch (B-436, #1347):
/// [`settings_file`], and on Windows also the Store app's own copy of it.
///
/// Windows keeps what the Store (MSIX) app writes to `%LOCALAPPDATA%` in
/// the package's `Packages\<family>\LocalCache\Local` (D-945). A program
/// started outside the package, such as `kassensturz-mcp.exe` of the
/// installer build or the package's own one started by its path, reads the
/// real `%LOCALAPPDATA%\kassensturzrc` instead. That file can be an older
/// state, for example AI still on from the installer build, while the
/// user switched AI off in the Store app. So the Store app's copy is read
/// too, and AI counts as on only when every one of these files says so.
/// The Store app's copy comes last: it is the newer view where it exists.
pub fn settings_files() -> Vec<PathBuf> {
    settings_files_for(
        std::env::consts::OS,
        |name| std::env::var_os(name).filter(|value| !value.is_empty()),
        |folder| {
            std::fs::read_dir(folder)
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .filter_map(|entry| entry.file_name().into_string().ok())
                        .collect()
                })
                .unwrap_or_default()
        },
    )
}

/// [`settings_files`] on the system `os`, with the environment read through
/// `var` and the names in a folder listed by `names`.
fn settings_files_for(
    os: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
    names: impl Fn(&Path) -> Vec<String>,
) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = settings_file_for(os, &var).into_iter().collect();
    // The test profile keeps its own file (Q-399): no package copy counts there.
    if os == "windows"
        && var("XDG_CONFIG_HOME").is_none()
        && let Some(local) = var("LOCALAPPDATA").map(PathBuf::from)
    {
        let packages = local.join("Packages");
        let prefix = format!("{STORE_PACKAGE_NAME}_");
        let mut families: Vec<String> = names(&packages)
            .into_iter()
            .filter(|name| name.starts_with(&prefix))
            .collect();
        families.sort();
        files.extend(families.into_iter().map(|family| {
            packages
                .join(family)
                .join("LocalCache")
                .join("Local")
                .join(FILE)
        }));
    }
    files
}

/// The text of every settings file in [`settings_files`] that is there.
/// `None` when none is, or when one is there but can't be read: AI is off
/// then.
fn settings_texts() -> Option<Vec<String>> {
    texts_of(&settings_files())
}

/// The text of each of `files` that is there; `None` when none is or one
/// that is there can't be read.
fn texts_of(files: &[PathBuf]) -> Option<Vec<String>> {
    let mut texts = Vec::new();
    for file in files {
        match std::fs::read_to_string(file) {
            Ok(text) => texts.push(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    (!texts.is_empty()).then_some(texts)
}

/// Whether every one of `texts` switches AI on, and there is one.
fn all_enabled(texts: &[String]) -> bool {
    !texts.is_empty() && texts.iter().all(|text| enabled_in(text))
}

/// Whether every one of `texts` switches AI and the write tools on.
fn all_write_tools(texts: &[String]) -> bool {
    !texts.is_empty() && texts.iter().all(|text| write_tools_in(text))
}

/// What all of `texts` allow together: a cell only where every one gives
/// it, a field locked where any one locks it. None without a text.
fn permissions_of(texts: &[String]) -> Permissions {
    let mut each = texts.iter().map(|text| permissions_in(text));
    let Some(first) = each.next() else {
        return Permissions::NONE;
    };
    each.fold(first, Permissions::narrowest)
}

/// The text of the settings file that reflects the user's latest changes:
/// the last one of [`settings_files`] that can be read, which is the Store
/// app's copy where there is one. For values that aren't a switch, such as
/// the book opened last.
pub fn newest_settings_text() -> Option<String> {
    settings_files()
        .iter()
        .rev()
        .find_map(|file| std::fs::read_to_string(file).ok())
}

/// Whether AI is switched on in this user's settings: in every settings
/// file that counts (B-436).
pub fn enabled() -> bool {
    settings_texts().is_some_and(|texts| all_enabled(&texts))
}

/// Whether this program's own settings file switches AI on while another
/// copy of KassenSturz on this computer (the Store app's, B-436) switches
/// it off, so that AI counts as off here (B-449). The chat says so
/// instead of silently reading nothing.
pub fn off_in_other_copy() -> bool {
    settings_file().is_some_and(|own| off_in_other_copy_of(&own, &settings_files()))
}

/// [`off_in_other_copy`] with `own` and all the `files` that count.
fn off_in_other_copy_of(own: &Path, files: &[PathBuf]) -> bool {
    enabled_at(own) && !texts_of(files).is_some_and(|texts| all_enabled(&texts))
}

/// Whether AI and the MCP write tools are both switched on in this user's
/// settings, in every file that counts. A file that can't be read is off.
pub fn write_tools_enabled() -> bool {
    settings_texts().is_some_and(|texts| all_write_tools(&texts))
}

/// The permissions in this user's settings, the narrowest of every file
/// that counts. None where a settings file can't be read, as with the AI
/// switch.
pub fn permissions() -> Permissions {
    settings_texts().map_or(Permissions::NONE, |texts| permissions_of(&texts))
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
    fn changing_or_deleting_a_saved_chart_gives_the_reading_of_the_charts() {
        // D-681: the AI finds a chart's ID with list_views.
        for key in ["AllowEditViews", "AllowDeleteViews"] {
            let got = permissions_in(&format!("[AI]\nEnabled=true\n{key}=true\n"));
            assert!(got.reads(Row::Charts), "{key}");
            assert!(!got.reads(Row::Categories), "{key}");
        }
        let added = permissions_in("[AI]\nEnabled=true\nAllowNewViews=true\n");
        assert!(!added.reads(Row::Charts));
        let read = permissions_in("[AI]\nEnabled=true\nAllowReadViews=true\n");
        assert!(read.reads(Row::Charts) && !read.can_change());
    }

    #[test]
    fn chart_descriptions_widen_adding_and_changing_charts_and_prepare_nothing_alone() {
        // D-729: the cell reads nothing and prepares nothing on its own.
        let alone = permissions_in("[AI]\nEnabled=true\nAllowCustomCharts=true\n");
        assert!(alone.allows(Permission::CustomCharts));
        assert!(!alone.reads(Row::Charts) && !alone.can_change());
        assert!(!alone.writes_chart_descriptions());
        for key in ["AllowNewViews", "AllowEditViews"] {
            let with = permissions_in(&format!(
                "[AI]\nEnabled=true\nAllowCustomCharts=true\n{key}=true\n"
            ));
            assert!(with.writes_chart_descriptions(), "{key}");
            let without = permissions_in(&format!("[AI]\nEnabled=true\n{key}=true\n"));
            assert!(!without.writes_chart_descriptions(), "{key}");
        }
        assert!(!Permission::CustomCharts.on_by_default());
        assert!(!Permission::CustomCharts.needs_read());
        assert_eq!(Permission::CustomCharts.row(), Row::Charts);
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
    fn the_inbox_and_its_archive_are_read_like_the_bill_folder_and_only_while_ai_is_on() {
        // An absolute path starts with a drive on Windows.
        let root = if cfg!(windows) { "C:" } else { "" };
        let on = "[AI]\nEnabled=true\n";
        let both = format!(
            "{on}InboxFolder={root}/home/sam/In\\sbox\nArchiveFolder={root}/home/sam/Filed\n"
        );
        assert_eq!(
            inbox_folder_in(&both),
            Some(PathBuf::from(format!("{root}/home/sam/In box")))
        );
        assert_eq!(
            archive_folder_in(&both),
            Some(PathBuf::from(format!("{root}/home/sam/Filed")))
        );
        assert_eq!(inbox_folder_in(on), None);
        assert_eq!(
            archive_folder_in("[AI]\nEnabled=true\nArchiveFolder=relative\n"),
            None
        );
        let off = both.replace("Enabled=true", "Enabled=false");
        assert_eq!(inbox_folder_in(&off), None);
        assert_eq!(archive_folder_in(&off), None);
        assert!(!Permission::FileBills.on_by_default());
        assert!(Permission::FileBills.needs_read());
        assert_eq!(Permission::FileBills.row(), Row::Transactions);
        assert_ne!(Permission::FileBills.key(), Permission::AttachBills.key());
    }

    #[test]
    fn the_settings_file_is_where_kconfig_keeps_it_on_each_system() {
        // B-322: on Windows the switch used to read nothing, so AI stayed
        // off there whatever the toggle wrote, and the consent couldn't be
        // saved.
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| std::ffi::OsString::from(value))
            }
        };
        assert_eq!(
            settings_file_for("linux", &env(&[("HOME", "/home/sam")])),
            Some(Path::new("/home/sam/.config").join(FILE))
        );
        assert_eq!(
            settings_file_for(
                "linux",
                &env(&[("HOME", "/home/sam"), ("XDG_CONFIG_HOME", "/elsewhere")])
            ),
            Some(Path::new("/elsewhere").join(FILE))
        );
        assert_eq!(
            settings_file_for(
                "windows",
                &env(&[("LOCALAPPDATA", r"C:\Users\Sam\AppData\Local")])
            ),
            Some(Path::new(r"C:\Users\Sam\AppData\Local").join(FILE))
        );
        // The test profile's own settings folder wins on Windows too.
        assert_eq!(
            settings_file_for(
                "windows",
                &env(&[
                    ("LOCALAPPDATA", r"C:\Users\Sam\AppData\Local"),
                    ("XDG_CONFIG_HOME", r"C:\Test\config")
                ])
            ),
            Some(Path::new(r"C:\Test\config").join(FILE))
        );
        assert_eq!(settings_file_for("windows", &env(&[])), None);
        assert_eq!(
            settings_file_for("macos", &env(&[("HOME", "/Users/sam")])),
            None
        );
    }

    /// B-448 (#1363): the folders come from the newest file, the Store
    /// app's copy, and only while AI is on in every file.
    #[test]
    fn folders_come_from_the_newest_file_and_only_while_every_file_is_on() {
        let dir = std::env::temp_dir().join(format!("ks-b448-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = if cfg!(windows) { "C:" } else { "" };
        let (real, copy) = (dir.join("real"), dir.join("copy"));
        std::fs::write(
            &real,
            format!("[AI]\nEnabled=true\nBillFolder={root}/old/Bills\n"),
        )
        .unwrap();
        std::fs::write(
            &copy,
            format!("[AI]\nEnabled=true\nBillFolder={root}/new/Bills\n"),
        )
        .unwrap();
        let files = [real.clone(), copy.clone()];
        let text = newest_enabled_text_of(&files).unwrap();
        assert_eq!(
            bill_folder_in(&text),
            Some(PathBuf::from(format!("{root}/new/Bills")))
        );
        std::fs::write(
            &copy,
            format!("[AI]\nEnabled=false\nBillFolder={root}/new/Bills\n"),
        )
        .unwrap();
        assert_eq!(newest_enabled_text_of(&files), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// B-449 (#1364): on in this file, off in another one that counts.
    #[test]
    fn on_here_and_off_in_the_other_copy_is_told_apart_from_off_here() {
        let dir = std::env::temp_dir().join(format!("ks-b449-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (own, copy) = (dir.join("own"), dir.join("copy"));
        std::fs::write(&own, "[AI]\nEnabled=true\n").unwrap();
        std::fs::write(&copy, "[AI]\nEnabled=false\n").unwrap();
        let files = [own.clone(), copy.clone()];
        assert!(off_in_other_copy_of(&own, &files));
        std::fs::write(&copy, "[AI]\nEnabled=true\n").unwrap();
        assert!(!off_in_other_copy_of(&own, &files));
        std::fs::write(&own, "[AI]\nEnabled=false\n").unwrap();
        assert!(!off_in_other_copy_of(&own, &files));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// B-436 (#1347): the Store app writes its settings into the package's
    /// copy, while a program outside the package reads the real file.
    #[test]
    fn on_windows_the_store_apps_copy_counts_too() {
        let env = |name: &str| (name == "LOCALAPPDATA").then(|| r"C:\L".into());
        let family = format!("{STORE_PACKAGE_NAME}_6gxp6ch7ybyne");
        let names = |folder: &Path| {
            assert_eq!(folder, Path::new(r"C:\L").join("Packages"));
            vec![
                "Other.App_abc".to_owned(),
                family.clone(),
                format!("{STORE_PACKAGE_NAME}Longer_x"),
            ]
        };
        let copy = Path::new(r"C:\L")
            .join("Packages")
            .join(&family)
            .join("LocalCache")
            .join("Local")
            .join(FILE);
        assert_eq!(
            settings_files_for("windows", env, names),
            vec![Path::new(r"C:\L").join(FILE), copy]
        );
        // The test profile keeps to its own file.
        let test_profile = |name: &str| match name {
            "LOCALAPPDATA" => Some(r"C:\L".into()),
            "XDG_CONFIG_HOME" => Some(r"C:\T".into()),
            _ => None,
        };
        assert_eq!(
            settings_files_for("windows", test_profile, |_| vec![family.clone()]),
            vec![Path::new(r"C:\T").join(FILE)]
        );
        // Linux never looks for a package.
        let linux = |name: &str| (name == "HOME").then(|| "/home/sam".into());
        assert_eq!(
            settings_files_for("linux", linux, |_| panic!("no listing on Linux")),
            vec![Path::new("/home/sam/.config").join(FILE)]
        );
    }

    #[test]
    fn off_in_any_settings_file_is_off() {
        let on = "[AI]\nEnabled=true\nMcpWriteTools=true\n".to_owned();
        let off = "[AI]\nEnabled=false\n".to_owned();
        assert!(all_enabled(std::slice::from_ref(&on)));
        assert!(all_enabled(&[on.clone(), on.clone()]));
        // AI on from the installer build, switched off in the Store app.
        assert!(!all_enabled(&[on.clone(), off.clone()]));
        assert!(!all_enabled(&[off.clone(), on.clone()]));
        assert!(!all_enabled(&[]));
        assert!(all_write_tools(std::slice::from_ref(&on)));
        assert!(!all_write_tools(&[on.clone(), off.clone()]));
        assert!(!all_write_tools(&[]));
    }

    #[test]
    fn the_permissions_of_two_files_are_what_both_allow() {
        let wide = Permissions::all().with_lock(Field::ALL[0], false);
        let narrow = Permissions::NONE
            .with(Permission::ALL[0], true)
            .with(Permission::ALL[1], true);
        let both = wide.narrowest(narrow);
        assert_eq!(both, narrow.narrowest(wide));
        for permission in Permission::ALL {
            assert_eq!(
                both.allows(permission),
                wide.allows(permission) && narrow.allows(permission)
            );
        }
        // A lock in either file holds.
        assert_eq!(both.locked(Field::ALL[0]), narrow.locked(Field::ALL[0]));
        assert_eq!(permissions_of(&[]), Permissions::NONE);
    }

    #[test]
    fn a_file_that_is_there_but_cant_be_read_is_off() {
        let folder = std::env::temp_dir().join(format!("ks-b436-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let on = folder.join("on");
        std::fs::write(&on, "[AI]\nEnabled=true\n").unwrap();
        let missing = folder.join("missing");
        assert_eq!(
            texts_of(&[on.clone(), missing.clone()]).map(|texts| all_enabled(&texts)),
            Some(true)
        );
        assert_eq!(texts_of(std::slice::from_ref(&missing)), None);
        // A folder where the file should be can't be read as text.
        assert_eq!(texts_of(&[on, folder.clone()]), None);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn the_store_package_name_is_the_manifests() {
        let manifest = include_str!("../../../packaging/windows/msix/AppxManifest.xml");
        assert!(manifest.contains(&format!("Name=\"{STORE_PACKAGE_NAME}\"")));
    }

    #[test]
    fn a_missing_file_is_off() {
        assert!(!enabled_at(Path::new("/nonexistent/kassensturzrc")));
    }
    #[test]
    fn the_bill_folder_is_read_back_as_kconfig_wrote_it_and_only_while_ai_is_on() {
        // An absolute path starts with a drive on Windows.
        let root = if cfg!(windows) { "C:" } else { "" };
        let on = "[AI]\nEnabled=true\n";
        let read = |value: &str| bill_folder_in(&format!("{on}BillFolder={value}\n"));
        assert_eq!(bill_folder_in(on), None);
        assert_eq!(
            read(&format!("{root}/home/sam/Bills")),
            Some(PathBuf::from(format!("{root}/home/sam/Bills")))
        );
        assert_eq!(
            read(&format!("{root}/home/sam/My Bills\\s")),
            Some(PathBuf::from(format!("{root}/home/sam/My Bills ")))
        );
        assert_eq!(
            read(&format!("{root}/a\\\\b")),
            Some(PathBuf::from(format!("{root}/a\\b")))
        );
        // Off, relative, empty or with an escape KassenSturz doesn't know.
        assert_eq!(
            bill_folder_in(&format!(
                "[AI]\nEnabled=false\nBillFolder={root}/home/sam/Bills\n"
            )),
            None
        );
        assert_eq!(read("Bills"), None);
        assert_eq!(read(""), None);
        assert_eq!(read(&format!("{root}/a\\x2fb")), None);
        assert_eq!(read(&format!("{root}/a\\")), None);
    }

    #[test]
    fn the_notes_are_read_back_as_kconfig_wrote_them_and_only_while_ai_is_on() {
        let on = "[AI]\nEnabled=true\n";
        let read = |value: &str| notes_in(&format!("{on}Notes={value}\n"));
        assert_eq!(notes_in(on), "");
        assert_eq!(read("Salary comes on the 28th"), "Salary comes on the 28th");
        // KConfig writes a line break as \n and a backslash as two.
        assert_eq!(
            read("Salary on the 28th\\nCorner Market is groceries"),
            "Salary on the 28th\nCorner Market is groceries"
        );
        assert_eq!(read("a\\\\b"), "a\\b");
        assert_eq!(read("Ümlaut and 日本"), "Ümlaut and 日本");
        // Off, empty, an escape KassenSturz doesn't know, or a lone backslash.
        assert_eq!(notes_in("[AI]\nEnabled=false\nNotes=hello\n"), "");
        assert_eq!(read(""), "");
        assert_eq!(read("a\\x41"), "");
        assert_eq!(read("a\\"), "");
        // Control characters go, line breaks stay, the length is limited.
        assert_eq!(read("a\u{7}b\\nc"), "ab\nc");
        assert_eq!(
            read(&"x".repeat(MAX_NOTES_CHARS + 50)).chars().count(),
            MAX_NOTES_CHARS
        );
    }

    #[test]
    fn builtin_chat_buttons_default_on_and_hide_when_ai_is_off() {
        let on = "[AI]\nEnabled=true\n";
        for button in BuiltinChatButton::ALL {
            assert!(button.on_by_default());
            assert_eq!(BuiltinChatButton::from_word(button.word()), Some(button));
            assert!(
                builtin_chat_button_in(on, button),
                "{button:?} defaults on when absent"
            );
            assert!(
                !builtin_chat_button_in("[AI]\nEnabled=false\n", button),
                "{button:?} is off when AI is off"
            );
            assert!(
                !builtin_chat_button_in("", button),
                "{button:?} is off when file is empty"
            );
        }
        assert_eq!(BuiltinChatButton::from_word("unknown"), None);
        let keys: std::collections::HashSet<_> =
            BuiltinChatButton::ALL.iter().map(|b| b.key()).collect();
        assert_eq!(keys.len(), BuiltinChatButton::ALL.len());

        let text = "[AI]\nEnabled=true\nChatButtonCategories=false\nChatButtonMonth=maybe\nChatButtonDescribeChart=false\n";
        assert!(!builtin_chat_button_in(text, BuiltinChatButton::Categories));
        assert!(builtin_chat_button_in(text, BuiltinChatButton::Month));
        assert!(builtin_chat_button_in(text, BuiltinChatButton::Rules));
        assert!(!builtin_chat_button_in(
            text,
            BuiltinChatButton::DescribeChart
        ));
    }

    #[test]
    fn custom_chat_button_validation_and_sanitization_enforce_limits_and_safety() {
        assert!(is_valid_chat_button_id("btn-1_aZ"));
        assert!(!is_valid_chat_button_id(""));
        assert!(!is_valid_chat_button_id("a b"));
        assert!(!is_valid_chat_button_id("a/b"));
        assert!(!is_valid_chat_button_id(
            &"a".repeat(MAX_CHAT_BUTTON_ID_BYTES + 1)
        ));

        // Labels: single-line, trimmed, no control chars or NULs, max 60 chars.
        assert_eq!(
            validate_chat_button_label("  Weekly summary  ").as_deref(),
            Some("Weekly summary")
        );
        assert_eq!(validate_chat_button_label("   "), None);
        assert_eq!(validate_chat_button_label("two\nlines"), None);
        assert_eq!(validate_chat_button_label("has\0nul"), None);
        assert_eq!(validate_chat_button_label("bell\u{7}"), None);
        assert_eq!(
            validate_chat_button_label(&"a".repeat(MAX_CHAT_BUTTON_LABEL_CHARS)).map(|s| s.len()),
            Some(MAX_CHAT_BUTTON_LABEL_CHARS)
        );
        assert_eq!(
            validate_chat_button_label(&"a".repeat(MAX_CHAT_BUTTON_LABEL_CHARS + 1)),
            None
        );

        // Label sanitization on read: rejects NUL, cleans control chars, caps length.
        assert_eq!(sanitize_chat_button_label("has\0nul"), None);
        assert_eq!(
            sanitize_chat_button_label("  Line 1\nLine 2\u{7}  ").as_deref(),
            Some("Line 1 Line 2")
        );
        assert_eq!(
            sanitize_chat_button_label(&"ü".repeat(MAX_CHAT_BUTTON_LABEL_CHARS + 10))
                .map(|s| s.chars().count()),
            Some(MAX_CHAT_BUTTON_LABEL_CHARS)
        );

        // Prompts: multi-line allowed, CRLF normalized, no NUL or other control chars, max 2000 chars.
        assert_eq!(
            validate_chat_button_prompt("  Line 1\r\nLine 2\t!  ").as_deref(),
            Some("Line 1\nLine 2\t!")
        );
        assert_eq!(validate_chat_button_prompt("   \n  "), None);
        assert_eq!(validate_chat_button_prompt("bad\0nul"), None);
        assert_eq!(validate_chat_button_prompt("bad\u{7}bell"), None);
        assert_eq!(
            validate_chat_button_prompt(&"x".repeat(MAX_CHAT_BUTTON_PROMPT_CHARS + 1)),
            None
        );

        // Prompt sanitization on read: rejects NUL, strips other control chars, caps length.
        assert_eq!(sanitize_chat_button_prompt("bad\0nul"), None);
        assert_eq!(
            sanitize_chat_button_prompt("a\u{7}b\r\nc").as_deref(),
            Some("ab\nc")
        );
        assert_eq!(
            sanitize_chat_button_prompt(&"ä".repeat(MAX_CHAT_BUTTON_PROMPT_CHARS + 25))
                .map(|s| s.chars().count()),
            Some(MAX_CHAT_BUTTON_PROMPT_CHARS)
        );
    }
}
