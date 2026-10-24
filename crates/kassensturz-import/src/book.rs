// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A whole book that another program kept, such as KMyMoney's (M2.9), as
//! its file holds it: what goes into a new KassenSturz book, and what was
//! left out (D-029).

use kassensturz_core::{
    AccountKind, AccountType, Currency, CurrencyCode, Date, ExchangeRate, ImportedDetails,
    MatchMode, Money, PartStatus, PayeeRecognition, TextField, TextOperator,
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
///
/// Its currencies can include withdrawn ones, such as the German mark,
/// with the decimal places they had ([`crate::currency`]): the
/// pipeline converts their amounts into the currencies that replaced them,
/// and each converted part keeps the amount it had (D-077, D-104). They can
/// also include currencies neither of KassenSturz's tables lists, such as
/// the old Romanian leu ([`ImportedBook::unlisted`]): the user says what
/// replaced each one before the pipeline converts it the same way (D-106).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedBook {
    /// The banks and other institutions the accounts are at, in the order
    /// of the file (D-132).
    pub institutions: Vec<BookInstitution>,
    /// The accounts and categories, and the internal accounts for starting
    /// balances, in the order of the file.
    pub accounts: Vec<BookAccount>,
    /// The payees, with what the file keeps about them, in the order of the
    /// file (D-132). Every payee comes in, whether a transaction names it
    /// or not.
    pub payees: Vec<BookPayee>,
    /// The Rules the file's own payee recognition amounts to, in the order
    /// of the payees (D-160).
    pub rules: Vec<BookRule>,
    /// The tags, in the order of the file (D-132).
    pub tags: Vec<BookTag>,
    /// The transactions, in the order of the file.
    pub transactions: Vec<BookTransaction>,
    /// The exchange rates of the price table (D-028).
    pub rates: Vec<BookRate>,
    /// The currencies of accounts and transactions that neither
    /// KassenSturz's ISO 4217 table nor its table of withdrawn currencies
    /// lists, sorted by code (D-106).
    pub unlisted: Vec<UnlistedCurrency>,
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
    /// For a category: the key of the category it sits inside. For an
    /// account: the key of the account it was grouped under, if any
    /// (D-081, D-132). `None` for a top-level one.
    pub parent: Option<String>,
    /// Whether the user closed it in the other program.
    pub closed: bool,
    /// For an account: its account number or IBAN, as the file gives it
    /// (D-132).
    pub number: Option<String>,
    /// What the user wrote about it (D-132).
    pub notes: Option<String>,
    /// For an account: the key of the institution it is at (D-132).
    pub institution: Option<String>,
}

/// A bank or other institution of an imported book (D-132).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookInstitution {
    /// The file's own ID for it, unique within the file.
    pub key: String,
    /// Its name, as the file gives it.
    pub name: String,
    /// Its bank code, as the file gives it.
    pub bank_code: Option<String>,
    /// Who the user talks to there.
    pub contact: Option<String>,
    /// Its address, as lines of text.
    pub address: Option<String>,
    /// Its phone number.
    pub phone: Option<String>,
}

/// A payee of an imported book: who money goes to or comes from, with what
/// the file keeps about them (D-132).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookPayee {
    /// The file's own ID for it, unique within the file.
    pub key: String,
    /// The name, as transactions name it.
    pub name: String,
    /// Their address, as lines of text.
    pub address: Option<String>,
    /// Their e-mail address.
    pub email: Option<String>,
    /// Their phone number.
    pub phone: Option<String>,
    /// The user's number with them.
    pub reference: Option<String>,
    /// How the other program recognized them in an imported entry.
    pub recognition: PayeeRecognition,
    /// Whether that ignored capitals.
    pub recognition_ignores_case: bool,
}

/// A Rule an imported book asks for: the other program recognized a payee
/// in the text of an imported entry and, usually, filled in a default
/// category (D-160). Like everything in [`ImportedBook`] it names accounts
/// by the file's keys; the pipeline makes the Rule, checks its patterns and
/// reports the ones KassenSturz can't use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookRule {
    /// The name of the Rule: the payee's.
    pub name: String,
    /// Whether one of the conditions is enough or all are needed.
    pub match_mode: MatchMode,
    /// The conditions: what text field is compared, how, and with what.
    /// Never empty.
    pub conditions: Vec<BookCondition>,
    /// Whether capitals count in the conditions.
    pub case_sensitive: bool,
    /// The key of the account or category to book to, if the file has one
    /// for the payee. The pipeline books to a category, or makes a Transfer
    /// if it is one of the user's accounts.
    pub book_to: Option<String>,
    /// The payee to set. Never empty.
    pub payee: String,
}

/// One text condition of a [`BookRule`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookCondition {
    /// The field it looks at.
    pub field: TextField,
    /// How it compares.
    pub operator: TextOperator,
    /// The text, or for [`TextOperator::Matches`] the pattern as the file
    /// has it.
    pub value: String,
}

/// A tag of an imported book (D-132).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookTag {
    /// The file's own ID for it, unique within the file.
    pub key: String,
    /// Its name, as the file gives it.
    pub name: String,
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
    /// What the user wrote about the transaction as a whole (D-132).
    pub note: Option<String>,
    /// The day it was entered, if the file says (D-132).
    pub entered_on: Option<Date>,
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
    /// What the user wrote about the part (D-132). Another program's one
    /// text per part is the delivered text in `details` on a part booked to
    /// an account, and this note on any other part.
    pub note: Option<String>,
    /// A cheque number or other reference (D-132).
    pub number: Option<String>,
    /// Whether the user checked it against a bank statement (D-132).
    pub status: Option<PartStatus>,
    /// The keys of the tags on it, each once, in the file's order (D-132).
    pub tags: Vec<String>,
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

/// A currency of an imported book that neither of KassenSturz's tables
/// lists, such as the old Romanian leu (ROL), which the new one replaced in
/// 2005 (D-106). No account can be in it: before the book is filled, the
/// user says which current currency replaced it, at what rate and from
/// which day ([`Replacement`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnlistedCurrency {
    /// The currency, with the decimal places the file counts it in.
    pub currency: Currency,
    /// What the file's own rates say replaced it, if they say so exactly.
    /// Only ever a suggestion for the user to check: another program's
    /// rates can be off, or the wrong way round (Q-065).
    pub suggestion: Option<Replacement>,
}

/// Which current currency replaced a withdrawn one, at what rate and from
/// which day (D-106).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Replacement {
    /// The current currency that replaced it: RON for the old leu.
    pub successor: CurrencyCode,
    /// How many units of the withdrawn currency one unit of the successor
    /// was worth: 10000 for the old leu.
    pub rate: ExchangeRate,
    /// The day the rate took effect.
    pub date: Date,
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
/// can't hold them yet (D-029, D-132). The UI names each one in the user's
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
    /// Tags on parts that the reader couldn't match to the file's list of
    /// tags, counted as tags; the list itself comes in (D-132).
    Tags,
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
    /// Payees that couldn't become a Rule, because the file has only one of
    /// a way to recognize them in imported entries and a default category,
    /// or a way this importer doesn't know, counted as payees (D-160).
    PayeeRules,
    /// Anything else the file holds, such as cost centers, the bank
    /// identifiers a plug-in kept, or a part of the file KassenSturz doesn't
    /// know, counted as entries.
    Other,
}

impl SkippedData {
    /// Every kind, in the order a report lists them.
    pub const ALL: [Self; 10] = [
        Self::Investments,
        Self::Schedules,
        Self::Budgets,
        Self::Reports,
        Self::Tags,
        Self::Rates,
        Self::OnlineBanking,
        Self::Owner,
        Self::PayeeRules,
        Self::Other,
    ];

    /// The kind's keyword, the one word the UI translates (D-034, D-084).
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Investments => "investments",
            Self::Schedules => "schedules",
            Self::Budgets => "budgets",
            Self::Reports => "reports",
            Self::Tags => "tags",
            Self::Rates => "rates",
            Self::OnlineBanking => "online-banking",
            Self::Owner => "owner",
            Self::PayeeRules => "payee-rules",
            Self::Other => "other",
        }
    }
}
