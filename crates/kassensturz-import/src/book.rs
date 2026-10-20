// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A whole book that another program kept, such as KMyMoney's (M2.9), as
//! its file holds it: what goes into a new KassenSturz book, and what was
//! left out (D-029).

use kassensturz_core::{
    AccountKind, AccountType, Currency, CurrencyCode, Date, ExchangeRate, ImportedDetails, Money,
};

/// Another program's book, read from its file.
///
/// It holds what the file says, in KassenSturz's terms but without
/// KassenSturz's IDs: accounts and categories are named by the file's own
/// keys, and the import pipeline gives everything new IDs when it fills a
/// new book with it (`kassensturz-pipeline`). Data KassenSturz can't hold
/// yet, such as investments or scheduled transactions, isn't in it; the
/// importer counts it in [`ImportedBook::skipped`], so the user learns what
/// stayed behind (D-029).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedBook {
    /// The accounts and categories, and the internal accounts for starting
    /// balances, in the order of the file.
    pub accounts: Vec<BookAccount>,
    /// The transactions, in the order of the file.
    pub transactions: Vec<BookTransaction>,
    /// The exchange rates of the price table (D-028).
    pub rates: Vec<BookRate>,
    /// What the file held that isn't in the book, each kind once with how
    /// many there were, in the order of [`SkippedData`].
    pub skipped: Vec<Skipped>,
}

/// An account, a category or an internal account of an imported book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookAccount {
    /// The file's own ID for it, unique within the file. Parts and
    /// categories inside it refer to it by this key.
    pub key: String,
    /// Its name, as the file gives it.
    pub name: String,
    /// What it stands for. Users see assets and liabilities as accounts,
    /// income and expense as categories; equity is the internal account for
    /// starting balances (`docs/GLOSSARY.md`).
    pub kind: AccountKind,
    /// The sort of account, where the file says and KassenSturz has one
    /// that fits (D-081). Categories have none.
    pub account_type: Option<AccountType>,
    /// Its currency, with the decimal places its amounts were read in.
    pub currency: Currency,
    /// For a category: the key of the category it sits inside. `None` for
    /// a top-level category and for every other account.
    pub parent: Option<String>,
    /// Whether the user closed it in the other program.
    pub closed: bool,
}

/// A transaction of an imported book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookTransaction {
    /// The date it is shown with.
    pub date: Date,
    /// Who the money came from or went to.
    pub payee: Option<String>,
    /// The currency it counts in: every part's [`BookPart::amount`] is in
    /// it.
    pub currency: Currency,
    /// Its parts, in the order of the file.
    ///
    /// They needn't add up to zero: another program may keep a transaction
    /// whose category is still missing, as KMyMoney does for an imported
    /// entry, and the import pipeline adds a part without a category for
    /// the difference, as for every imported entry (D-083).
    pub parts: Vec<BookPart>,
}

/// One part of an imported transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookPart {
    /// The key of the account or category it is booked to, `None` for a
    /// part without a category, such as one the other program booked to
    /// something KassenSturz skipped, e.g. an investment.
    pub account: Option<String>,
    /// Its amount in the transaction's currency.
    pub amount: Money,
    /// For a part booked to an account in another currency than the
    /// transaction's: its amount in the account's currency (D-043). Both
    /// amounts come from the file, so their ratio is the rate.
    pub account_amount: Option<Money>,
    /// What the file says a bank delivered for the part, or the text the
    /// other program kept with it, exactly as the file has it (D-067). Only
    /// a part booked to an account has it (D-083).
    pub details: Option<ImportedDetails>,
}

/// An exchange rate of an imported book's price table: one unit of `from`
/// costs `rate` units of `to` on `date` (D-028).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BookRate {
    /// The day it applies to.
    pub date: Date,
    /// The currency one unit of which is priced.
    pub from: CurrencyCode,
    /// The currency the price is in.
    pub to: CurrencyCode,
    /// How many units of `to` one unit of `from` costs.
    pub rate: ExchangeRate,
}

/// Something the file held that the imported book doesn't, and how many of
/// it there were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// What it is.
    pub what: SkippedData,
    /// How many: of accounts, transactions, rates, notes, …, as
    /// [`SkippedData`] says for each kind.
    pub count: usize,
}

/// The kinds of data an imported book can leave out, because KassenSturz
/// can't hold them yet (D-029). The UI names each one in the user's
/// language from its keyword (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SkippedData {
    /// Investment and stock accounts, counted as accounts. Parts booked to
    /// them come into the book without a category, so the money that went
    /// in or out of the user's other accounts is still there.
    Investments,
    /// Scheduled and recurring transactions, counted as schedules (D-029).
    Schedules,
    /// Budgets.
    Budgets,
    /// Saved reports.
    Reports,
    /// Banks and other institutions, with their addresses and codes.
    Institutions,
    /// Account numbers and IBANs of the user's accounts, counted as
    /// accounts that had one.
    AccountNumbers,
    /// Accounts that sat inside another account, which a book doesn't nest
    /// (D-081): they come in as accounts of their own, counted as accounts.
    AccountGroups,
    /// Payees no transaction is for, and what was kept with a payee besides
    /// its name (address, e-mail, reference, how to recognize it), counted
    /// as payees.
    Payees,
    /// Tags on transactions, counted as tags.
    Tags,
    /// Notes the book has no place for: on parts that aren't booked to one
    /// of the user's accounts, on transactions, on accounts, and cheque
    /// numbers, counted as notes. The note of a part booked to an account
    /// stays, as the text the payment came with.
    Notes,
    /// Marks that a part was checked against a bank statement (cleared or
    /// reconciled), counted as parts.
    Reconciliation,
    /// Exchange rates and prices the price table can't hold: prices of
    /// securities, rates of a currency KassenSturz doesn't know, such as
    /// one withdrawn since (D-077), and rates it can't keep exactly.
    /// Counted as rates.
    Rates,
    /// Settings and orders for online banking, counted as accounts and
    /// orders.
    OnlineBanking,
    /// The owner's name and address.
    Owner,
    /// Anything else the file holds, such as cost centers or a part of the
    /// file KassenSturz doesn't know, counted as entries.
    Other,
}

impl SkippedData {
    /// Every kind, in the order a report lists them.
    pub const ALL: [Self; 15] = [
        Self::Investments,
        Self::Schedules,
        Self::Budgets,
        Self::Reports,
        Self::Institutions,
        Self::AccountNumbers,
        Self::AccountGroups,
        Self::Payees,
        Self::Tags,
        Self::Notes,
        Self::Reconciliation,
        Self::Rates,
        Self::OnlineBanking,
        Self::Owner,
        Self::Other,
    ];

    /// The kind's keyword, the one word the UI translates (D-034, D-084).
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Investments => "investments",
            Self::Schedules => "schedules",
            Self::Budgets => "budgets",
            Self::Reports => "reports",
            Self::Institutions => "institutions",
            Self::AccountNumbers => "account-numbers",
            Self::AccountGroups => "account-groups",
            Self::Payees => "payees",
            Self::Tags => "tags",
            Self::Notes => "notes",
            Self::Reconciliation => "reconciliation",
            Self::Rates => "rates",
            Self::OnlineBanking => "online-banking",
            Self::Owner => "owner",
            Self::Other => "other",
        }
    }
}
