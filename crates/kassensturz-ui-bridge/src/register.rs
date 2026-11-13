// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An account's register, ready for display: every amount and date is
//! already text in the user's regional format, the rows are sorted here and
//! the balance after each entry is added up here, from the exact amounts
//! (D-065, D-118). The writes behind the register's pages are in
//! `transaction_entry.rs`.

use std::collections::HashMap;

use kassensturz_core::{AccountId, AccountKind, CurrencyCode, Date, Money, TransactionId};
use kassensturz_store::{Book, StoreError, TransactionOutline};

use crate::format::{AmountFormat, Locale};
use crate::register_columns::{Column, Columns};
use crate::tags::{self, TagText};

/// Writes amounts of one currency in the user's regional format.
pub struct AmountWriter<'a, L> {
    locale: &'a L,
    format: AmountFormat,
    minor_digits: u8,
}

impl<'a, L: Locale> AmountWriter<'a, L> {
    /// For amounts in `currency`, with the decimal places `book` stores for
    /// it (D-027).
    pub fn new(book: &Book, currency: CurrencyCode, locale: &'a L) -> Result<Self, StoreError> {
        Ok(Self {
            locale,
            format: locale.amount_format(currency),
            minor_digits: book.currency_minor_digits(currency)?,
        })
    }

    /// `amount` as text, e.g. "-53,20 €", with the decimal places the user
    /// chose to see, and exactly.
    pub fn text(&self, amount: Money) -> Shown {
        let group_digits = |value| self.locale.group_digits(value);
        let shown_digits = self.locale.shown_decimal_places(self.minor_digits);
        Shown {
            text: self
                .format
                .format_rounded(amount, self.minor_digits, shown_digits, group_digits),
            exact: self.format.format(amount, self.minor_digits, group_digits),
        }
    }

    /// A balance as text, and whether it is shown as "You owe": a debt reads
    /// "You owe 300,00 €" rather than "-300,00 €" (D-067). Only accounts of
    /// money the user owes, such as credit cards, are shown that way.
    pub fn balance(&self, balance: Money, kind: AccountKind) -> (Shown, bool) {
        let owes = kind == AccountKind::Liability && balance.is_negative();
        match balance.checked_neg() {
            Ok(debt) if owes => (self.text(debt), true),
            _ => (self.text(balance), false),
        }
    }
}

/// An amount as the UI shows it, and exactly. They differ only when the
/// user chose to see fewer decimal places than the currency has; the exact
/// amount then shows on hover (D-002).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// As shown, e.g. "54 €".
    pub text: String,
    /// Exactly, e.g. "53,90 €".
    pub exact: String,
}

/// For each transaction that waits for the other side of a monthly move the
/// user remembered, the move as "from → to" (D-749, 3.5).
fn waiting_texts(book: &Book) -> Result<HashMap<TransactionId, String>, StoreError> {
    let waiting = book.lonely_waiting_moves()?;
    if waiting.is_empty() {
        return Ok(HashMap::new());
    }
    let accounts = book.all_accounts()?;
    let name_of = |id: AccountId| {
        accounts
            .iter()
            .find(|account| account.id == id)
            .map(|account| account.name.clone())
            .unwrap_or_default()
    };
    Ok(waiting
        .into_iter()
        .map(|(id, moved)| {
            (
                id,
                format!(
                    "{} → {}",
                    name_of(moved.from_account),
                    name_of(moved.to_account)
                ),
            )
        })
        .collect())
}

/// What the transaction list shows for one account.
#[derive(Debug, PartialEq, Eq)]
pub struct RegisterView {
    /// The account's name.
    pub account_name: String,
    /// Its balance; for a debt, what the user owes.
    pub balance: Shown,
    /// Whether the balance is shown as "You owe" (D-067).
    pub owes: bool,
    /// Whether the rows say what the account held after each entry. They do
    /// so only in date order and only while the column is chosen: in any
    /// other order the figure beside an entry would read as if it were the
    /// one before or after its neighbour (US1.18, D-656).
    pub balance_shown: bool,
    /// Its transactions, in the order asked for ([`Sort`]).
    pub rows: Vec<RegisterRow>,
}

/// One transaction in the list.
#[derive(Debug, PartialEq, Eq)]
pub struct RegisterRow {
    /// The transaction's ID as text, to name it to QML.
    pub id: String,
    /// The booking date.
    pub date: String,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// The category, if the transaction has exactly one; for a Transfer the
    /// other account's name.
    pub category: String,
    /// The amount, as seen from the account.
    pub amount: Shown,
    /// Whether money came in, shown as "From" rather than "To" (D-034).
    pub incoming: bool,
    /// Whether this is the account's starting balance (`docs/GLOSSARY.md`).
    pub starting_balance: bool,
    /// What kind of entry it is, as a key the UI translates (`EntryKind`).
    pub kind: &'static str,
    /// What the account held after this entry, counting its entries in
    /// date order; for a debt, what the user owed.
    pub balance: Shown,
    /// Whether that balance is shown as "You owe" (D-067).
    pub owes: bool,
    /// What the user noted about it; may be empty.
    pub note: String,
    /// The other party's IBAN, or whatever account number the bank gave,
    /// exactly as delivered. Filled only while its column is chosen
    /// (D-657); empty for a transaction typed in by hand.
    pub other_party_iban: String,
    /// What the payment came with, on one line. Filled only while its
    /// column is chosen.
    pub purpose: String,
    /// The day the money counts from or until, as the user's locale writes
    /// dates. Filled only while its column is chosen.
    pub value_date: String,
    /// What a bank or file delivered with it (the other party's IBAN and the
    /// purpose), for the quick filter only: filled while the user searches,
    /// whether or not those columns are shown (M6-B244). Never shown.
    pub delivered: String,
    /// Every tag it carries, on the whole or on a part, in the user's
    /// order (D-713): a search by any of them finds the line (Q-493).
    pub tags: Vec<TagText>,
    /// For an entry that is not sorted yet and fits a monthly move the user
    /// remembered, whose other side hasn't come: the move as "from → to"
    /// (US6.3, D-749, 3.5); empty for every other entry.
    pub waits_for: String,
}

impl RegisterRow {
    /// The names of its tags, as one line, e.g. "Holiday, Tax".
    pub fn tag_line(&self) -> String {
        self.tags
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Whether the row carries the tag with the ID `tag` (as text), on the
    /// whole or on a part. An empty `tag` fits every row.
    pub fn has_tag(&self, tag: &str) -> bool {
        tag.is_empty() || self.tags.iter().any(|own| own.id == tag)
    }

    /// Whether the row fits what the user typed in the quick filter: every
    /// word of `query` appears, ignoring case, in its date, who it came from
    /// or went to, category, amount (as shown or exactly) or note. An empty
    /// query fits every row.
    pub fn fits(&self, query: &str) -> bool {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return true;
        }
        let haystack = [
            &self.date,
            &self.other_party,
            &self.category,
            &self.amount.text,
            &self.amount.exact,
            &self.note,
            &self.other_party_iban,
            &self.purpose,
            &self.value_date,
            &self.delivered,
            &self.tag_line(),
        ]
        .map(|text| text.to_lowercase())
        .join("\n");
        words.iter().all(|word| haystack.contains(word.as_str()))
    }
}

/// What a transaction is, seen from one account. The register's page can
/// change only the simple kind here; the others get their own pages (D-118).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// Money paid or received, booked to one category or to none yet.
    Simple,
    /// The account's starting balance, which "Check against my bank"
    /// corrects (D-116).
    StartingBalance,
    /// Money moved to or from another of the user's accounts (D-021).
    Transfer,
    /// Broken down into several Parts (D-017), or booked to the account
    /// more than once.
    Parts,
}

impl EntryKind {
    /// The key the UI knows the kind by.
    pub fn key(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::StartingBalance => "starting-balance",
            Self::Transfer => "transfer",
            Self::Parts => "parts",
        }
    }
}

/// One transaction as seen from an account, before it is written as text.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    /// The transaction. Its ID begins with the time it was added (D-067).
    pub transaction: TransactionId,
    /// The booking date.
    pub date: Date,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// The category, if the transaction has exactly one; for a Transfer the
    /// other account's name.
    pub category: String,
    /// What it did to the account.
    pub amount: Money,
    /// What kind of entry it is.
    pub kind: EntryKind,
    /// What the account held after it, counting the entries in date order.
    pub balance: Money,
    /// What the user noted about it: the transaction's note, or else the
    /// note of its part in this account (D-132); may be empty.
    pub note: String,
    /// Every tag it carries, on the whole or on a part (D-713).
    pub tags: Vec<TagText>,
}

impl Entry {
    /// Whether this is the account's starting balance.
    pub fn starting_balance(&self) -> bool {
        self.kind == EntryKind::StartingBalance
    }

    /// The entry as text for `writer`'s locale, in an account of `kind`.
    pub fn row(&self, writer: &AmountWriter<'_, impl Locale>, kind: AccountKind) -> RegisterRow {
        let (balance, owes) = writer.balance(self.balance, kind);
        RegisterRow {
            id: self.transaction.to_string(),
            date: writer.locale.short_date(self.date),
            other_party: self.other_party.clone(),
            category: self.category.clone(),
            amount: writer.text(self.amount),
            incoming: self.amount.minor_units() > 0,
            starting_balance: self.starting_balance(),
            kind: self.kind.key(),
            balance,
            owes,
            note: self.note.clone(),
            other_party_iban: String::new(),
            purpose: String::new(),
            value_date: String::new(),
            delivered: String::new(),
            tags: self.tags.clone(),
            waits_for: String::new(),
        }
    }
}

/// What a transaction is, seen from `account`, and what its other part is
/// booked to: the category's or the other account's name, if it has exactly
/// one other part.
pub fn kind_of(
    book: &Book,
    account: AccountId,
    transaction: &kassensturz_core::Transaction,
) -> Result<(EntryKind, String), StoreError> {
    let parts = transaction
        .splits()
        .iter()
        .map(|split| (split.account, split.foreign.is_some()));
    Ok(match one_other_part(account, parts) {
        None => (EntryKind::Parts, String::new()),
        Some(None) => (EntryKind::Simple, String::new()),
        Some(Some(id)) => kind_from(&book.account(id)?),
    })
}

/// Where the one other part of a transaction is booked to, seen from
/// `account`, from its `parts` (each one's account and whether it is in
/// another currency): `None` if it has several parts, a foreign one, or
/// several in `account`; `Some(None)` if the other part is not sorted yet.
fn one_other_part(
    account: AccountId,
    parts: impl IntoIterator<Item = (Option<AccountId>, bool)>,
) -> Option<Option<AccountId>> {
    let mut own = 0;
    let mut others = Vec::new();
    for (booked_to, foreign) in parts {
        if booked_to == Some(account) {
            own += 1;
        } else {
            others.push((booked_to, foreign));
        }
    }
    match others.as_slice() {
        [(other, false)] if own == 1 => Some(*other),
        _ => None,
    }
}

/// The kind of a line whose one other part is booked to `other`, with the
/// name it shows: the category's or the other account's, none for a
/// starting balance.
fn kind_from(other: &kassensturz_core::Account) -> (EntryKind, String) {
    match other.kind {
        AccountKind::Income | AccountKind::Expense => (EntryKind::Simple, other.name.clone()),
        AccountKind::Equity => (EntryKind::StartingBalance, String::new()),
        AccountKind::Asset | AccountKind::Liability => (EntryKind::Transfer, other.name.clone()),
    }
}

/// `account`'s transactions, oldest first, each with what its other part is
/// booked to and what the account held after it, added up from the exact
/// amounts (D-065). The last entry's balance is the account's balance.
pub fn entries(book: &Book, account: AccountId) -> Result<Vec<Entry>, StoreError> {
    let details = book.account(account)?;
    let mut balance = Money::zero(details.currency);
    let mut entries = Vec::new();
    // Read once, not once per line, and only the outline of each
    // transaction: making whole transactions took most of the time a
    // register with thousands of lines needed to open (M6-B8, D-387).
    let outlines: HashMap<TransactionId, TransactionOutline> =
        book.transaction_outlines(account)?;
    let tag_names = tags::names(book)?;
    let accounts: HashMap<AccountId, kassensturz_core::Account> = book
        .all_accounts()?
        .into_iter()
        .map(|account| (account.id, account))
        .collect();
    for entry in book.register(account)? {
        let outline = outlines
            .get(&entry.transaction)
            .ok_or(StoreError::UnknownTransaction(entry.transaction))?;
        let parts = outline
            .parts
            .iter()
            .map(|part| (part.account, part.foreign));
        let (kind, category) = match one_other_part(account, parts) {
            None => (EntryKind::Parts, String::new()),
            Some(None) => (EntryKind::Simple, String::new()),
            Some(Some(id)) => kind_from(accounts.get(&id).ok_or(StoreError::UnknownAccount(id))?),
        };
        balance = balance
            .checked_add(entry.amount)
            .map_err(|_| StoreError::AmountOverflow)?;
        let note = outline
            .note
            .as_deref()
            .or_else(|| {
                outline
                    .parts
                    .iter()
                    .find(|part| part.account == Some(account))
                    .and_then(|part| part.note.as_deref())
            })
            .unwrap_or_default()
            .to_owned();
        entries.push(Entry {
            transaction: entry.transaction,
            date: entry.booking_date,
            other_party: entry.payee.unwrap_or_default(),
            category,
            amount: entry.amount,
            kind,
            balance,
            note,
            tags: tags::texts(&tag_names, &outline.tags),
        });
    }
    Ok(entries)
}

/// The newest `limit` entries of `account` that are not its starting
/// balance, each a different transaction, newest first (the order of
/// [`entries`] reversed). Unlike [`entries`] it reads only those
/// transactions, not every one of the account, so a book with years of
/// history opens the overview as fast as a small one; the balance after
/// each entry isn't added up, so it stays zero. `accounts` is every account
/// of the book, read once by the caller.
pub fn latest_entries(
    book: &Book,
    account: AccountId,
    accounts: &HashMap<AccountId, kassensturz_core::Account>,
    limit: usize,
) -> Result<Vec<Entry>, StoreError> {
    let details = accounts
        .get(&account)
        .ok_or(StoreError::UnknownAccount(account))?;
    let mut found: Vec<Entry> = Vec::new();
    let tag_names = tags::names(book)?;
    // The register is in date order, so the newest are at its end. An
    // account can show one transaction on two days (D-108): only the later
    // counts, as in the overview's list of the newest.
    for entry in book.register(account)?.into_iter().rev() {
        if found.len() == limit {
            break;
        }
        if found
            .iter()
            .any(|listed| listed.transaction == entry.transaction)
        {
            continue;
        }
        let transaction = book.transaction(entry.transaction)?;
        let parts = transaction
            .splits()
            .iter()
            .map(|split| (split.account, split.foreign.is_some()));
        let (kind, category) = match one_other_part(account, parts) {
            None => (EntryKind::Parts, String::new()),
            Some(None) => (EntryKind::Simple, String::new()),
            Some(Some(id)) => kind_from(accounts.get(&id).ok_or(StoreError::UnknownAccount(id))?),
        };
        if kind == EntryKind::StartingBalance {
            continue;
        }
        let note = transaction
            .note()
            .or_else(|| {
                transaction
                    .splits()
                    .iter()
                    .find(|split| split.account == Some(account))
                    .and_then(|split| split.note.as_deref())
            })
            .unwrap_or_default()
            .to_owned();
        found.push(Entry {
            transaction: entry.transaction,
            date: entry.booking_date,
            other_party: entry.payee.unwrap_or_default(),
            category,
            amount: entry.amount,
            kind,
            balance: Money::zero(details.currency),
            note,
            tags: tags::texts(&tag_names, &transaction.all_tags()),
        });
    }
    Ok(found)
}

/// What the register is sorted by. The balance after each entry doesn't
/// change with the order: it always counts the entries in date order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    /// By date; on the same day, the entry added last comes last (D-067).
    #[default]
    Date,
    /// By amount, money in before money out when ascending.
    Amount,
    /// By who the money came from or went to, ignoring case; entries without
    /// come first when ascending.
    OtherParty,
    /// By category, ignoring case; entries without come first when
    /// ascending.
    Category,
    /// By the other party's IBAN, ignoring case and spaces; entries without
    /// one (typed in by hand) come first when ascending (D-658).
    OtherPartyIban,
    /// By the purpose, ignoring case; entries without come first when
    /// ascending.
    Purpose,
    /// By the value date, the earliest first when ascending; entries without
    /// one come first.
    ValueDate,
}

impl SortKey {
    /// The key the UI knows the sort by.
    pub fn key(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Amount => "amount",
            Self::OtherParty => "other-party",
            Self::Category => "category",
            Self::OtherPartyIban => "other-party-iban",
            Self::Purpose => "purpose",
            Self::ValueDate => "value-date",
        }
    }

    /// The sort for a key from [`SortKey::key`].
    pub fn from_key(key: &str) -> Option<Self> {
        [
            Self::Date,
            Self::Amount,
            Self::OtherParty,
            Self::Category,
            Self::OtherPartyIban,
            Self::Purpose,
            Self::ValueDate,
        ]
        .into_iter()
        .find(|sort| sort.key() == key)
    }

    /// Whether the order depends on what a bank or file delivered.
    pub fn needs_import(self) -> bool {
        matches!(self, Self::OtherPartyIban | Self::Purpose | Self::ValueDate)
    }
}

/// How the register is sorted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    /// What by.
    pub key: SortKey,
    /// Whether largest or latest first.
    pub descending: bool,
}

impl Default for Sort {
    /// Newest first, like in a banking app.
    fn default() -> Self {
        Self {
            key: SortKey::Date,
            descending: true,
        }
    }
}

impl Sort {
    /// Sorts `entries` as asked. A tie keeps the date order, which is how
    /// they come from [`entries`]; so does sorting by date.
    #[cfg(test)]
    pub fn apply(self, entries: &mut [Entry]) {
        self.apply_with(entries, &HashMap::new());
    }

    /// Like [`Sort::apply`], with what was delivered for the entries, which
    /// the orders by IBAN, purpose and value date need. An entry with no
    /// delivery sorts as an empty one.
    pub fn apply_with(
        self,
        entries: &mut [Entry],
        imported: &HashMap<(TransactionId, Date), ImportedText>,
    ) {
        let key = self.key;
        let none = ImportedText::default();
        let delivered = |entry: &Entry| {
            imported
                .get(&(entry.transaction, entry.date))
                .unwrap_or(&none)
        };
        entries.sort_by(|a, b| {
            let ordering = match key {
                SortKey::Date => (a.date, a.transaction).cmp(&(b.date, b.transaction)),
                SortKey::Amount => a.amount.minor_units().cmp(&b.amount.minor_units()),
                SortKey::OtherParty => a
                    .other_party
                    .to_lowercase()
                    .cmp(&b.other_party.to_lowercase()),
                SortKey::Category => a.category.to_lowercase().cmp(&b.category.to_lowercase()),
                SortKey::OtherPartyIban => compact(&delivered(a).other_party_iban)
                    .cmp(&compact(&delivered(b).other_party_iban)),
                SortKey::Purpose => delivered(a)
                    .purpose
                    .to_lowercase()
                    .cmp(&delivered(b).purpose.to_lowercase()),
                SortKey::ValueDate => delivered(a).value_day.cmp(&delivered(b).value_day),
            };
            if self.descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
}

/// `text` in lower case without its spaces, to compare IBANs written with
/// and without them.
fn compact(text: &str) -> String {
    text.split_whitespace().collect::<String>().to_lowercase()
}

/// What a bank or file delivered for an entry, written as text for the
/// register's detail columns (US1.19).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedText {
    /// The other party's IBAN, or whatever account number the bank gave.
    pub other_party_iban: String,
    /// What the payment came with, on one line.
    pub purpose: String,
    /// The day the money counts from or until, as the locale writes dates.
    pub value_date: String,
    /// The same day as a date, to sort by it exactly.
    pub value_day: Option<Date>,
}

/// What a bank or file delivered for each entry of `account`, by the
/// transaction and the day the register shows it on. An entry that has
/// several parts in the account shows what the first part with a delivery
/// came with. Entries typed in by hand have none.
///
/// The text is shown as it was delivered, and is untrusted (AGENTS.md §8):
/// the screens show it as plain text. Only white space is tidied, since the
/// register keeps a line to one row.
pub fn imported_texts(
    book: &Book,
    account: AccountId,
    locale: &impl Locale,
) -> Result<HashMap<(TransactionId, Date), ImportedText>, StoreError> {
    let mut found: HashMap<(TransactionId, Date), ImportedText> = HashMap::new();
    for part in book.account_parts(account)? {
        let Some(imported) = part.imported else {
            continue;
        };
        found
            .entry((part.transaction, part.booking_date))
            .or_insert_with(|| ImportedText {
                other_party_iban: one_line(imported.other_party_iban.as_deref()),
                purpose: one_line(imported.remittance_information.as_deref()),
                value_date: imported
                    .value_date
                    .map(|date| locale.short_date(date))
                    .unwrap_or_default(),
                value_day: imported.value_date,
            });
    }
    Ok(found)
}

/// `text` with every run of white space, e.g. the line breaks of a bank's
/// remittance information, written as one space.
fn one_line(text: Option<&str>) -> String {
    text.unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads `account`'s register from `book`, sorts it and formats it for
/// `locale`, with the default columns.
#[cfg(test)]
pub fn register_view(
    book: &Book,
    account: AccountId,
    sort: Sort,
    locale: &impl Locale,
) -> Result<RegisterView, StoreError> {
    register_view_with(book, account, sort, Columns::default(), locale)
}

/// Reads `account`'s register as `register_view_with` does, for the
/// `columns` the user chose. When `searching` with the quick filter, what a
/// bank or file delivered is read even if no column shows it, so the filter
/// finds a purpose or an IBAN in a hidden column too (M6-B244).
pub fn register_view_searching(
    book: &Book,
    account: AccountId,
    sort: Sort,
    columns: Columns,
    searching: bool,
    locale: &impl Locale,
) -> Result<RegisterView, StoreError> {
    view_of(book, account, sort, columns, searching, locale)
}

/// Like [`register_view`], for the `columns` the user chose for the
/// account. What a bank or file delivered is read only if one of the
/// columns shows it, and the balance after each entry only comes with date
/// order (US1.18, D-656): in another order it would be a figure beside the
/// wrong neighbours, and a wrong balance is the worst bug (AGENTS.md §2).
/// The app uses [`register_view_searching`].
#[cfg(test)]
pub fn register_view_with(
    book: &Book,
    account: AccountId,
    sort: Sort,
    columns: Columns,
    locale: &impl Locale,
) -> Result<RegisterView, StoreError> {
    view_of(book, account, sort, columns, false, locale)
}

fn view_of(
    book: &Book,
    account: AccountId,
    sort: Sort,
    columns: Columns,
    searching: bool,
    locale: &impl Locale,
) -> Result<RegisterView, StoreError> {
    let details = book.account(account)?;
    let writer = AmountWriter::new(book, details.currency, locale)?;
    let mut entries = entries(book, account)?;
    // The last entry in date order holds the balance, so it isn't added up
    // a second time.
    let now = entries
        .last()
        .map_or(Money::zero(details.currency), |entry| entry.balance);
    let (balance, owes) = writer.balance(now, details.kind);
    // What was delivered is read only if a column shows it, the order needs
    // it or the user searches.
    let imported = if searching || columns.needs_import() || sort.key.needs_import() {
        imported_texts(book, account, locale)?
    } else {
        HashMap::new()
    };
    sort.apply_with(&mut entries, &imported);
    let waiting = waiting_texts(book)?;
    let balance_shown = sort.key == SortKey::Date && columns.shows(Column::Balance);
    let rows = entries
        .iter()
        .map(|entry| {
            let mut row = entry.row(&writer, details.kind);
            if !balance_shown {
                row.balance = Shown::default();
                row.owes = false;
            }
            // Only an entry nobody sorted waits (a Rule skips it, a hand-made
            // category stays).
            if entry.category.is_empty()
                && let Some(text) = waiting.get(&entry.transaction)
            {
                row.waits_for.clone_from(text);
            }
            if let Some(text) = imported.get(&(entry.transaction, entry.date)) {
                if columns.shows(Column::OtherPartyIban) {
                    row.other_party_iban.clone_from(&text.other_party_iban);
                }
                if columns.shows(Column::Purpose) {
                    row.purpose.clone_from(&text.purpose);
                }
                if columns.shows(Column::ValueDate) {
                    row.value_date.clone_from(&text.value_date);
                }
                if searching {
                    row.delivered = format!("{}\n{}", text.other_party_iban, text.purpose);
                }
            }
            row
        })
        .collect();
    Ok(RegisterView {
        account_name: details.name,
        balance,
        owes,
        balance_shown,
        rows,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::format::Affixes;

    /// A fixed, German-like format, independent of the machine's settings.
    pub(crate) struct TestLocale;

    impl Locale for TestLocale {
        fn amount_format(&self, _currency: CurrencyCode) -> AmountFormat {
            AmountFormat {
                decimal_point: ",".into(),
                zero_digit: '0',
                positive: Affixes::spaced(String::new(), " €".into()),
                negative: Affixes::spaced("-".into(), " €".into()),
            }
        }

        fn decimal_point(&self) -> String {
            ",".into()
        }

        fn group_digits(&self, value: u64) -> String {
            value.to_string()
        }

        fn group_separator(&self) -> String {
            ".".into()
        }

        fn short_date(&self, date: Date) -> String {
            date.to_string()
        }

        fn month_and_year(&self, date: Date) -> String {
            // English, so the tests read like the window.
            const MONTHS: [&str; 12] = [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ];
            let month = usize::try_from(date.month() - 1).expect("months start at 1");
            format!("{} {}", MONTHS[month], date.year())
        }

        fn read_date(&self, text: &str) -> Option<Date> {
            text.parse().ok()
        }

        fn today(&self) -> Date {
            Date::constant(2026, 10, 19)
        }
    }
}

#[cfg(test)]
mod sort_tests {
    use super::*;

    fn entry(minor: i64, day: i8, other: &str, category: &str) -> Entry {
        let eur = CurrencyCode::new("EUR").expect("EUR");
        Entry {
            transaction: TransactionId::generate(),
            date: Date::constant(2026, 10, day),
            other_party: other.into(),
            category: category.into(),
            amount: Money::from_minor_units(minor, eur),
            kind: EntryKind::Simple,
            balance: Money::from_minor_units(minor, eur),
            note: String::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn newest_first_by_default_and_ties_keep_the_order_added() {
        let mut entries = vec![
            entry(1, 5, "", ""),
            entry(2, 5, "", ""),
            entry(3, 4, "", ""),
        ];
        let (first, second, third) = (
            entries[0].transaction,
            entries[1].transaction,
            entries[2].transaction,
        );
        Sort::default().apply(&mut entries);
        let order: Vec<_> = entries.iter().map(|entry| entry.transaction).collect();
        assert_eq!(order, [second, first, third]);
        Sort {
            key: SortKey::Date,
            descending: false,
        }
        .apply(&mut entries);
        let order: Vec<_> = entries.iter().map(|entry| entry.transaction).collect();
        assert_eq!(order, [third, first, second]);
    }

    #[test]
    fn keys_round_trip() {
        for key in [
            SortKey::Date,
            SortKey::Amount,
            SortKey::OtherParty,
            SortKey::Category,
            SortKey::OtherPartyIban,
            SortKey::Purpose,
            SortKey::ValueDate,
        ] {
            assert_eq!(SortKey::from_key(key.key()), Some(key));
        }
    }

    #[test]
    fn the_quick_filter_needs_every_word_somewhere_in_the_row_ignoring_case() {
        let row = RegisterRow {
            id: String::new(),
            date: "2026-03-05".into(),
            other_party: "Corner Bakery".into(),
            category: "Food".into(),
            amount: Shown {
                text: "-4 €".into(),
                exact: "-3,80 €".into(),
            },
            incoming: false,
            starting_balance: false,
            kind: "simple",
            balance: Shown {
                text: "100 €".into(),
                exact: "100,00 €".into(),
            },
            owes: false,
            note: "Rolls for Sunday".into(),
            other_party_iban: String::new(),
            purpose: String::new(),
            value_date: String::new(),
            delivered: String::new(),
            tags: vec![TagText {
                id: "tag-1".into(),
                name: "Holiday".into(),
            }],
            waits_for: String::new(),
        };
        assert!(row.fits(""));
        assert!(row.fits("  "));
        assert!(row.fits("bakery"));
        assert!(row.fits("BAKERY food"));
        assert!(row.fits("sunday 3,80"));
        assert!(row.fits("2026-03"));
        assert!(!row.fits("bakery rent"));
        // A tag counts as part of its text, and the tag filter finds it by
        // ID (Q-493).
        assert!(row.fits("holiday"));
        assert!(row.has_tag("tag-1"));
        assert!(row.has_tag(""));
        assert!(!row.has_tag("tag-2"));
        assert_eq!(row.tag_line(), "Holiday");
        // What the account held afterwards is not part of the row's text.
        assert!(!row.fits("100"));
    }
}

#[cfg(test)]
mod view_tests {
    use kassensturz_core::{Account, ImportedDetails, Split, Transaction};

    use super::tests::TestLocale;
    use super::*;

    fn eur() -> CurrencyCode {
        CurrencyCode::new("EUR").expect("EUR")
    }

    fn money(minor: i64) -> Money {
        Money::from_minor_units(minor, eur())
    }

    fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: eur(),
            account_type: None,
            closed: false,
            parent: None,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        id
    }

    /// What a bank delivers; the IBAN is the example for Germany in SWIFT's
    /// IBAN Registry (D-055).
    fn delivered(purpose: &str) -> ImportedDetails {
        ImportedDetails {
            bank_id: Some("2026100500017".into()),
            booking_date: Some(Date::constant(2026, 10, 5)),
            value_date: Some(Date::constant(2026, 10, 7)),
            other_party_name: Some("Corner Grocery".into()),
            other_party_iban: Some("DE89370400440532013000".into()),
            remittance_information: Some(purpose.into()),
        }
    }

    /// A checking account with an entry a bank delivered, one typed in by
    /// hand and one more by hand on a later day, so the three orders differ.
    fn book_with_entries(purpose: &str) -> (Book, AccountId) {
        let mut book = Book::create_in_memory().expect("book");
        book.use_currency(eur()).expect("currency");
        let checking = account(&mut book, "Checking", AccountKind::Asset);
        let food = account(&mut book, "Food", AccountKind::Expense);
        let mut parts = vec![
            Split::new(checking, money(-5320)),
            Split::new(food, money(5320)),
        ];
        parts[0].imported = Some(delivered(purpose));
        let imported = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 5),
            Some("Corner Grocery".into()),
            parts,
        )
        .expect("transaction");
        book.add_transaction(&imported).expect("added");
        for (day, minor) in [(6, 10_000), (8, -900)] {
            let parts = vec![
                Split::new(checking, money(minor)),
                Split::new(food, money(-minor)),
            ];
            let by_hand = Transaction::new(
                TransactionId::generate(),
                Date::constant(2026, 10, day),
                Some("Cash".into()),
                parts,
            )
            .expect("transaction");
            book.add_transaction(&by_hand).expect("added");
        }
        (book, checking)
    }

    fn view(book: &Book, account: AccountId, sort: Sort, columns: Columns) -> RegisterView {
        register_view_with(book, account, sort, columns, &TestLocale).expect("register")
    }

    fn by(key: SortKey, descending: bool) -> Sort {
        Sort { key, descending }
    }

    #[test]
    fn the_default_columns_show_no_delivered_details() {
        let (book, checking) = book_with_entries("Card payment");
        let default = view(&book, checking, Sort::default(), Columns::default());
        assert!(default.balance_shown);
        for row in &default.rows {
            assert_eq!(row.other_party_iban, "");
            assert_eq!(row.purpose, "");
            assert_eq!(row.value_date, "");
        }
        // The same as the register has always been.
        let plain = register_view(&book, checking, Sort::default(), &TestLocale).expect("view");
        assert_eq!(plain, default);
    }

    #[test]
    fn a_chosen_detail_shows_what_the_bank_delivered_as_text_and_only_for_those_entries() {
        let (book, checking) = book_with_entries("Card\npayment   no. 7");
        let columns = Columns::default()
            .with(Column::OtherPartyIban, true)
            .with(Column::Purpose, true)
            .with(Column::ValueDate, true);
        let shown = view(&book, checking, by(SortKey::Date, false), columns);
        let [grocery, cash_in, cash_out] = shown.rows.as_slice() else {
            panic!("three rows");
        };
        assert_eq!(grocery.other_party_iban, "DE89370400440532013000");
        assert_eq!(grocery.purpose, "Card payment no. 7");
        assert_eq!(grocery.value_date, "2026-10-07");
        for by_hand in [cash_in, cash_out] {
            assert_eq!(by_hand.other_party_iban, "");
            assert_eq!(by_hand.purpose, "");
            assert_eq!(by_hand.value_date, "");
        }
        // Only the column that is chosen is filled.
        let only_iban = view(
            &book,
            checking,
            by(SortKey::Date, false),
            Columns::default().with(Column::OtherPartyIban, true),
        );
        assert_eq!(only_iban.rows[0].other_party_iban, "DE89370400440532013000");
        assert_eq!(only_iban.rows[0].purpose, "");
        // The quick filter finds what a chosen column shows.
        assert!(shown.rows[0].fits("DE89 card"));
        assert!(!shown.rows[0].fits("DE89 rent"));
    }

    /// The quick filter finds what was delivered also while its columns are
    /// hidden: a mandate reference in the purpose was found only with the
    /// purpose column shown (M6-B244, US6 P3 VM round 1, A7).
    #[test]
    fn the_quick_filter_finds_a_hidden_purpose_and_iban() {
        let (book, checking) = book_with_entries("Card payment MREF+MANDATE-TEST-001");
        let hidden = Columns::default();
        assert!(!hidden.shows(Column::Purpose));
        let quiet = view(&book, checking, by(SortKey::Date, false), hidden);
        assert!(quiet.rows.iter().all(|row| row.delivered.is_empty()));
        let searched = register_view_searching(
            &book,
            checking,
            by(SortKey::Date, false),
            hidden,
            true,
            &TestLocale,
        )
        .expect("register");
        let fitting: Vec<_> = searched
            .rows
            .iter()
            .filter(|row| row.fits("mandate-test-001"))
            .collect();
        assert_eq!(fitting.len(), 1);
        // Still not shown in the hidden columns.
        assert_eq!(fitting[0].purpose, "");
        assert_eq!(fitting[0].other_party_iban, "");
        assert_eq!(
            searched
                .rows
                .iter()
                .filter(|row| row.fits("DE89370400440532013000"))
                .count(),
            1
        );
        assert!(!searched.rows.iter().any(|row| row.fits("MANDATE-TEST-002")));
    }

    #[test]
    fn the_balance_after_each_entry_comes_only_with_date_order() {
        let (book, checking) = book_with_entries("Card payment");
        let balances = |sort| {
            let shown = view(&book, checking, sort, Columns::default());
            let balances: Vec<_> = shown
                .rows
                .iter()
                .map(|row| row.balance.text.clone())
                .collect();
            (shown.balance_shown, balances)
        };
        assert_eq!(
            balances(by(SortKey::Date, true)),
            (
                true,
                ["37,80 €", "46,80 €", "-53,20 €"]
                    .map(String::from)
                    .to_vec()
            )
        );
        assert_eq!(
            balances(by(SortKey::Date, false)),
            (
                true,
                ["-53,20 €", "46,80 €", "37,80 €"]
                    .map(String::from)
                    .to_vec()
            )
        );
        for key in [SortKey::Amount, SortKey::OtherParty, SortKey::Category] {
            for descending in [false, true] {
                let (shown, balances) = balances(by(key, descending));
                assert!(!shown, "{key:?}");
                assert!(balances.iter().all(String::is_empty), "{key:?}");
            }
        }
        // The account's own balance is still there, whatever the order.
        let amount = view(
            &book,
            checking,
            by(SortKey::Amount, false),
            Columns::default(),
        );
        assert_eq!(amount.balance.text, "37,80 €");
    }

    #[test]
    fn a_hidden_balance_column_leaves_no_balance_in_the_rows() {
        let (book, checking) = book_with_entries("Card payment");
        let hidden = Columns::default().with(Column::Balance, false);
        let shown = view(&book, checking, Sort::default(), hidden);
        assert!(!shown.balance_shown);
        assert!(shown.rows.iter().all(|row| row.balance == Shown::default()));
    }

    #[test]
    fn the_detail_columns_sort_by_what_the_bank_delivered_even_when_hidden() {
        let (book, checking) = book_with_entries("Card payment");
        let amounts = |key, descending| -> Vec<String> {
            // The default columns show none of the details: the order still
            // needs them.
            view(&book, checking, by(key, descending), Columns::default())
                .rows
                .iter()
                .map(|row| row.amount.exact.clone())
                .collect()
        };
        for key in [
            SortKey::OtherPartyIban,
            SortKey::Purpose,
            SortKey::ValueDate,
        ] {
            // Entries typed in by hand have nothing delivered: first when
            // ascending, in date order; the delivered one last.
            assert_eq!(
                amounts(key, false),
                ["100,00 €", "-9,00 €", "-53,20 €"],
                "{key:?}"
            );
            assert_eq!(
                amounts(key, true),
                ["-53,20 €", "100,00 €", "-9,00 €"],
                "{key:?}"
            );
        }
    }

    #[test]
    fn the_value_date_sorts_as_a_date_and_the_iban_ignores_case_and_spaces() {
        let mut book = Book::create_in_memory().expect("book");
        book.use_currency(eur()).expect("currency");
        let checking = account(&mut book, "Checking", AccountKind::Asset);
        let food = account(&mut book, "Food", AccountKind::Expense);
        // Booked on days 5, 6 and 7 (amounts -0,05 to -0,07 €), but with value
        // dates in another order, and IBANs written in different cases and
        // with and without spaces.
        for (day, iban, value_date) in [
            (
                5,
                "de89 3704 0044 0532 0130 00",
                Date::constant(2026, 11, 1),
            ),
            (
                6,
                "DE02 1203 0000 0000 2020 51",
                Date::constant(2026, 10, 9),
            ),
            (7, "de02120300000000202051x", Date::constant(2026, 10, 30)),
        ] {
            let mut details = delivered("x");
            details.other_party_iban = Some(iban.into());
            details.value_date = Some(value_date);
            let mut parts = vec![
                Split::new(checking, money(-i64::from(day))),
                Split::new(food, money(i64::from(day))),
            ];
            parts[0].imported = Some(details);
            let transaction = Transaction::new(
                TransactionId::generate(),
                Date::constant(2026, 10, day),
                None,
                parts,
            )
            .expect("transaction");
            book.add_transaction(&transaction).expect("added");
        }
        let amounts = |key| -> Vec<String> {
            view(&book, checking, by(key, false), Columns::default())
                .rows
                .iter()
                .map(|row| row.amount.exact.clone())
                .collect()
        };
        assert_eq!(
            amounts(SortKey::ValueDate),
            ["-0,06 €", "-0,07 €", "-0,05 €"]
        );
        // "DE02 1203 ..." < "DE02 1203 ...x" < "DE89 ...", whatever the
        // case and the spaces.
        assert_eq!(
            amounts(SortKey::OtherPartyIban),
            ["-0,06 €", "-0,07 €", "-0,05 €"]
        );
    }

    #[test]
    fn amounts_sort_by_their_exact_value_not_their_text() {
        let (book, checking) = book_with_entries("Card payment");
        let shown = view(
            &book,
            checking,
            by(SortKey::Amount, false),
            Columns::default(),
        );
        let amounts: Vec<_> = shown
            .rows
            .iter()
            .map(|row| row.amount.exact.clone())
            .collect();
        // As text "-9,00 €" would come after "-53,20 €" and "100,00 €" before
        // "-9,00 €".
        assert_eq!(amounts, ["-53,20 €", "-9,00 €", "100,00 €"]);
    }
}
