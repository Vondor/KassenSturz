// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Breaking a transaction down into Parts (M3.6, D-017, D-119): what one
//! purchase was for, piece by piece, with one Part's amount worked out from
//! the rest, so nobody has to do the subtraction.
//!
//! What the user types comes in as text and is read here in their regional
//! format (`parse.rs`, D-065); what the page shows goes out as text. The
//! page changes the Parts booked to categories, the total, the other party
//! and the date; a Part booked to another of the user's accounts (a Transfer
//! with a fee, say) is shown as it is and kept (M3.7). A Part booked to a
//! category in another currency also gets its amount in that currency, from
//! what the user types or from the book's exchange rates (D-043, D-028).
//! Saving goes through `Transaction::new`, so the book never holds a
//! transaction whose Parts don't add up.

use kassensturz_core::{
    Account, AccountKind, CurrencyCode, Date, DatedRate, ForeignAmount, Money, Report, Split,
    Transaction, TransactionError,
};
use kassensturz_store::{Book, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::parse::{AmountInputError, AmountReader};
use crate::register::AmountWriter;
use crate::transaction_entry::{EntryError, find_account, find_transaction, payee, typed_amount};

/// Why something the user entered wasn't accepted. The UI shows a message
/// for each code (D-034); a message about one Part names its number.
#[derive(Debug)]
pub enum PartsError {
    /// What the user typed as an amount isn't one: the total (`part` is
    /// `None`) or a Part's amount in `currency`, counted from 1.
    Amount {
        part: Option<usize>,
        currency: CurrencyCode,
        error: AmountInputError,
    },
    /// An amount worked out from the others doesn't fit into an amount.
    TooLarge,
    /// The total has to be more than zero: whether money came in or went
    /// out is chosen separately.
    NotPositive,
    /// What the user typed as a date isn't one.
    Date,
    /// There is no such account, category or transaction, e.g. it was just
    /// removed elsewhere.
    NoSuchEntry,
    /// The account's own part is in another currency than the transaction,
    /// the account is booked more than once, or it is the starting balance
    /// (D-116), so this page can't change it.
    NotEditableHere,
    /// Every transaction needs at least one Part besides the account's.
    NoParts,
    /// More than one Part was left without an amount; only one can be
    /// worked out from the rest.
    TooManyOpenParts,
    /// Every Part has an amount, and they don't add up to the total.
    DoesNotAddUp {
        /// What the Parts add up to, as the user counts them.
        parts: Money,
        /// The total, as the user typed it.
        total: Money,
    },
    /// A Part (counted from 1) is booked to a category in `currency`, no
    /// amount in it was typed, and the book has no rate to work it out.
    ForeignAmountNeeded { part: usize, currency: CurrencyCode },
    /// A Part's amount in the category's currency points the other way than
    /// its amount in the transaction's.
    ForeignSignMismatch { part: usize },
    /// The book refused it, e.g. the account is closed.
    Store(StoreError),
    /// The transaction doesn't add up; only an amount too large or a
    /// mismatch core finds can get here.
    Transaction(TransactionError),
}

impl From<StoreError> for PartsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<EntryError> for PartsError {
    /// The shared lookups return a missing entry or the book's refusal;
    /// the rest is mapped for completeness.
    fn from(error: EntryError) -> Self {
        match error {
            EntryError::Store(error) => Self::Store(error),
            EntryError::Transaction(error) => Self::Transaction(error),
            EntryError::Amount(AmountInputError::TooLarge) => Self::TooLarge,
            EntryError::Amount(_) | EntryError::NoSuchEntry => Self::NoSuchEntry,
            EntryError::NotPositive => Self::NotPositive,
            EntryError::Date => Self::Date,
            EntryError::NotEditableHere => Self::NotEditableHere,
        }
    }
}

/// The transaction's Parts as the user entered them on the page.
pub struct PartsInput<'a> {
    /// Whether money came in, rather than went out.
    pub money_in: bool,
    /// The total, as typed, without a sign.
    pub total: &'a str,
    /// Who the money came from or went to, as typed; may be empty.
    pub other_party: &'a str,
    /// The date, as typed.
    pub date: &'a str,
    /// The Parts booked to categories, in the order shown.
    pub parts: &'a [PartInput<'a>],
}

/// One Part as entered.
pub struct PartInput<'a> {
    /// The ID (as text) of the part it was before, so it keeps its ID;
    /// empty for a new Part.
    pub id: &'a str,
    /// The ID of its category as text; empty for none yet.
    pub category: &'a str,
    /// Its amount in the transaction's currency, as typed: positive for a
    /// Part of the total, with a minus sign for one that points the other
    /// way, e.g. a deposit paid back on the same bill. Empty to have it
    /// worked out from the rest.
    pub amount: &'a str,
    /// For a category in another currency: the amount in that currency, as
    /// typed. Empty to work it out from the book's exchange rates.
    pub foreign_amount: &'a str,
}

/// A transaction as the page shows it to be broken down, every amount and
/// date as text (D-065).
#[derive(Debug, PartialEq, Eq)]
pub struct PartsForm {
    /// The transaction's ID as text.
    pub id: String,
    /// The ID of the account it is seen from, as text.
    pub account: String,
    /// The account's currency, which the transaction counts in.
    pub currency: String,
    /// Whether money came in.
    pub money_in: bool,
    /// The total without sign, written the way the user types it.
    pub total: String,
    /// Whether the total stays as a bank or file delivered it (D-067).
    pub total_fixed: bool,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// The booking date, in the user's format.
    pub date: String,
    /// The Parts booked to categories, in the order they were entered. The
    /// last one is shown without an amount: the page works it out.
    pub parts: Vec<PartForm>,
    /// The Parts booked to another of the user's accounts, which stay as
    /// they are.
    pub fixed: Vec<FixedPart>,
}

/// One Part the page can change.
#[derive(Debug, PartialEq, Eq)]
pub struct PartForm {
    /// The part's ID as text.
    pub id: String,
    /// The ID of its category as text; empty for none yet.
    pub category: String,
    /// Its amount in the transaction's currency, the way the user types it;
    /// empty for the one the page works out.
    pub amount: String,
    /// The category's currency if it isn't the transaction's, else empty.
    pub currency: String,
    /// Its amount in that currency, the way the user types it; empty when
    /// the category is in the transaction's currency.
    pub foreign_amount: String,
}

/// One Part the page shows but doesn't change.
#[derive(Debug, PartialEq, Eq)]
pub struct FixedPart {
    /// The other account's name.
    pub name: String,
    /// The amount moved, without sign, as shown.
    pub amount: String,
    /// Whether the money came from that account into this one.
    pub incoming: bool,
}

/// What the page shows while the user types: the Part worked out from the
/// rest, the amounts in other currencies and whether it all adds up.
#[derive(Debug, PartialEq, Eq)]
pub struct PartsPreview {
    /// One per Part entered, in the same order.
    pub parts: Vec<PartPreview>,
    /// What the Parts add up to, including the ones that stay as they are,
    /// as shown.
    pub sum: String,
    /// What is still to be sorted into a Part, as shown; empty if nothing.
    pub missing: String,
    /// What the Parts have beyond the total, as shown; empty if nothing.
    pub over: String,
}

/// What the page shows for one Part.
#[derive(Debug, PartialEq, Eq, Default)]
pub struct PartPreview {
    /// Its amount in the transaction's currency, the way the user types it:
    /// as typed, or worked out for the open Part.
    pub amount: String,
    /// Whether the amount was worked out from the rest.
    pub open: bool,
    /// The category's currency if it isn't the transaction's, else empty.
    pub currency: String,
    /// Its amount in that currency, the way the user types it: as typed, or
    /// worked out from the book's rates; empty if neither is possible.
    pub foreign_amount: String,
    /// Whether that amount was worked out from the book's rates.
    pub suggested: bool,
    /// The rate behind the two amounts, e.g. "1,0856" with `rate_from` EUR
    /// and `rate_to` USD; empty if there is none to show.
    pub rate: String,
    /// The currency one unit of which the rate prices.
    pub rate_from: String,
    /// The currency the rate is in.
    pub rate_to: String,
    /// The day of the rate when it is one of the book's; empty for the rate
    /// the two amounts imply.
    pub rate_date: String,
}

/// A category a Part can be booked to.
#[derive(Debug, PartialEq, Eq)]
pub struct PartCategoryChoice {
    /// Its ID as text.
    pub id: String,
    /// Its path, e.g. "Food › Groceries".
    pub path: String,
    /// How deep it sits, for indenting the list.
    pub depth: usize,
    /// Its currency if it isn't the transaction's, else empty.
    pub currency: String,
}

/// The categories a Part of the transaction `id` in `account` can be booked
/// to, in tree order: those for money in or for money out, in any currency,
/// and the ones the transaction's Parts have already, even of the other
/// kind, so a Part keeps the category it has.
pub fn category_choices(
    book: &Book,
    account: &str,
    id: &str,
    money_in: bool,
) -> Result<Vec<PartCategoryChoice>, PartsError> {
    let account = find_account(book, account)?;
    let stored = find_transaction(book, account.id, id)?;
    let kind = if money_in {
        AccountKind::Income
    } else {
        AccountKind::Expense
    };
    let current: Vec<_> = stored
        .splits()
        .iter()
        .filter_map(|split| split.account)
        .collect();
    Ok(book
        .categories()?
        .into_iter()
        .filter(|entry| {
            let category = &entry.category;
            !category.closed && (category.kind == kind || current.contains(&category.id))
        })
        .map(|entry| PartCategoryChoice {
            id: entry.category.id.to_string(),
            path: entry.path.join(" › "),
            depth: entry.path.len() - 1,
            currency: other_currency(entry.category.currency, account.currency),
        })
        .collect())
}

/// The transaction with `id` (as text) as the page shows it, seen from
/// `account`.
pub fn details(
    book: &Book,
    account: &str,
    id: &str,
    locale: &impl Locale,
) -> Result<PartsForm, PartsError> {
    let account = find_account(book, account)?;
    let stored = find_transaction(book, account.id, id)?;
    let (own, others) = split_off_own(book, &account, &stored)?;
    let into_account = own.amount;
    let money_in = into_account.minor_units() > 0;
    let direction = Direction::of(money_in);
    let writer = AmountWriter::new(book, account.currency, locale)?;
    let mut parts = Vec::new();
    let mut fixed = Vec::new();
    for split in others {
        let other = split.account.map(|id| book.account(id)).transpose()?;
        match other {
            Some(other) if !other.kind.is_category() => fixed.push(FixedPart {
                name: other.name,
                amount: writer.text(unsigned(split.amount)?).exact,
                incoming: split.amount.is_negative(),
            }),
            other => {
                let foreign_amount = match &split.foreign {
                    Some(foreign) => typed_amount(book, direction.typed(foreign.amount)?, locale)?,
                    None => String::new(),
                };
                parts.push(PartForm {
                    id: split.id.to_string(),
                    category: other
                        .as_ref()
                        .map(|other| other.id.to_string())
                        .unwrap_or_default(),
                    amount: typed_amount(book, direction.typed(split.amount)?, locale)?,
                    currency: other
                        .as_ref()
                        .map(|other| other_currency(other.currency, account.currency))
                        .unwrap_or_default(),
                    foreign_amount,
                });
            }
        }
    }
    // The last Part is worked out from the rest, so adding a Part and typing
    // its amount takes it from there, with no subtraction to do (D-017).
    if let Some(last) = parts.last_mut() {
        last.amount.clear();
    }
    Ok(PartsForm {
        id: stored.id().to_string(),
        account: account.id.to_string(),
        currency: account.currency.to_string(),
        money_in,
        total: typed_amount(book, unsigned(into_account)?, locale)?,
        total_fixed: own.imported.is_some(),
        other_party: stored.payee_of(own).unwrap_or_default().to_owned(),
        date: locale.short_date(stored.date_of(own)),
        parts,
        fixed,
    })
}

/// What the page shows for `entered`, without changing anything.
pub fn preview(
    book: &Book,
    account: &str,
    id: &str,
    entered: &PartsInput<'_>,
    locale: &impl Locale,
) -> Result<PartsPreview, PartsError> {
    let draft = Draft::read(book, account, id, entered, locale)?;
    let resolved = draft.resolve()?;
    let writer = AmountWriter::new(book, draft.account.currency, locale)?;
    let mut parts = Vec::with_capacity(draft.parts.len());
    for (index, amount) in resolved.parts.iter().enumerate() {
        let mut shown = PartPreview {
            amount: typed_amount(book, draft.direction.typed(*amount)?, locale)?,
            open: draft.open == Some(index),
            ..PartPreview::default()
        };
        if let Some(currency) = draft.foreign_currency(index) {
            shown.currency = currency.to_string();
            if let Ok(found) = draft.foreign_of(book, index, *amount, locale) {
                shown.foreign_amount =
                    typed_amount(book, draft.direction.typed(found.amount)?, locale)?;
                shown.suggested = found.suggested;
                let rate = match &found.rate {
                    Some(rate) => Some((
                        rate_text(&rate.rate.to_string(), locale),
                        rate.from,
                        rate.to,
                        locale.short_date(rate.date),
                    )),
                    None => implied_rate(book, *amount, found.amount, locale)?
                        .map(|text| (text, amount.currency(), currency, String::new())),
                };
                if let Some((text, from, to, date)) = rate {
                    shown.rate = text;
                    shown.rate_from = from.to_string();
                    shown.rate_to = to.to_string();
                    shown.rate_date = date;
                }
            }
        }
        parts.push(shown);
    }
    let (missing, over) = match resolved.difference.minor_units() {
        0 => (String::new(), String::new()),
        units if units > 0 => (writer.text(resolved.difference).exact, String::new()),
        _ => (
            String::new(),
            writer.text(unsigned(resolved.difference)?).exact,
        ),
    };
    Ok(PartsPreview {
        parts,
        sum: writer.text(resolved.sum).exact,
        missing,
        over,
    })
}

/// Replaces the transaction `id` with what the user entered: the total, the
/// other party, the date and the Parts. Parts keep their IDs where they had
/// one, so the history records only what changed; a Part booked to another
/// of the user's accounts stays as it is; the account's part keeps what a
/// bank or file delivered with it (`store.imported-split-changed`).
pub fn save(
    book: &mut Book,
    account: &str,
    id: &str,
    entered: &PartsInput<'_>,
    locale: &impl Locale,
) -> Result<(), PartsError> {
    let draft = Draft::read(book, account, id, entered, locale)?;
    let resolved = draft.resolve()?;
    if !resolved.difference.is_zero() {
        return Err(PartsError::DoesNotAddUp {
            parts: resolved.sum,
            total: draft.total,
        });
    }
    let mut splits = Vec::with_capacity(2 + draft.fixed.len() + draft.parts.len());
    splits.push(Split {
        amount: draft.into_account,
        ..draft.own.clone()
    });
    splits.extend(draft.fixed.iter().cloned());
    for (index, (part, amount)) in draft.parts.iter().zip(&resolved.parts).enumerate() {
        let foreign = match draft.foreign_currency(index) {
            Some(_) => {
                let found = draft.foreign_of(book, index, *amount, locale)?;
                Some(ForeignAmount {
                    amount: found.amount,
                    rate: found.rate,
                })
            }
            None => None,
        };
        splits.push(Split {
            account: part.category.as_ref().map(|category| category.id),
            amount: *amount,
            foreign,
            ..part.split.clone()
        });
    }
    let transaction = Transaction::new(
        draft.stored.id(),
        draft.date,
        payee(entered.other_party),
        splits,
    )
    .map_err(PartsError::Transaction)?
    .with_details_of(&draft.stored);
    book.update_transaction(&transaction)?;
    Ok(())
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &PartsError, locale: &impl Locale) -> ErrorText {
    let plain = ErrorText::plain;
    let with_part = |mut text: ErrorText, part: usize| {
        text.params.push(("part", part.to_string()));
        text
    };
    let amount_text = |amount: Money| {
        AmountWriter::new(book, amount.currency(), locale)
            .map(|writer| writer.text(amount).exact)
            .unwrap_or_default()
    };
    match error {
        PartsError::Amount {
            part,
            currency,
            error,
        } => {
            let text = ErrorText::amount(*error, book, *currency, locale);
            match part {
                Some(part) => with_part(text, *part),
                None => text,
            }
        }
        PartsError::TooLarge => plain("input.amount-too-large"),
        PartsError::NotPositive => plain("input.amount-not-positive"),
        PartsError::Date => ErrorText {
            code: "input.malformed-date",
            params: vec![("example", locale.short_date(locale.today()))],
        },
        PartsError::NoSuchEntry => plain("input.no-such-entry"),
        PartsError::NotEditableHere => plain("input.not-editable-here"),
        PartsError::NoParts => plain("input.no-parts"),
        PartsError::TooManyOpenParts => plain("input.too-many-open-parts"),
        PartsError::DoesNotAddUp { parts, total } => {
            let difference = total.checked_sub(*parts).unwrap_or(*total);
            let (name, difference) = match difference.checked_neg() {
                Ok(over) if difference.is_negative() => ("over", over),
                _ => ("missing", difference),
            };
            ErrorText {
                code: "input.parts-dont-add-up",
                params: vec![
                    ("parts", amount_text(*parts)),
                    ("total", amount_text(*total)),
                    (name, amount_text(difference)),
                ],
            }
        }
        PartsError::ForeignAmountNeeded { part, currency } => with_part(
            ErrorText {
                code: "input.foreign-amount-needed",
                params: vec![("currency", currency.to_string())],
            },
            *part,
        ),
        PartsError::ForeignSignMismatch { part } => {
            with_part(plain("input.foreign-sign-mismatch"), *part)
        }
        PartsError::Store(error) => report_text(book, &error.report(), locale),
        PartsError::Transaction(error) => report_text(book, &error.report(), locale),
    }
}

/// Which way the money went, so typed amounts (positive for a Part of the
/// total) and the parts' amounts in the book (positive into their account)
/// can be turned into each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// Money went out of the account: a Part of the total is positive in
    /// its category.
    Out,
    /// Money came into the account: a Part of the total is negative in its
    /// category.
    In,
}

impl Direction {
    fn of(money_in: bool) -> Self {
        if money_in { Self::In } else { Self::Out }
    }

    /// A typed amount as the part's amount in the book, or back.
    fn typed(self, amount: Money) -> Result<Money, PartsError> {
        match self {
            Self::Out => Ok(amount),
            Self::In => amount.checked_neg().map_err(|_| PartsError::TooLarge),
        }
    }
}

/// What the user entered, read into the book's types, before the open Part
/// and the amounts in other currencies are worked out.
struct Draft<'a> {
    account: Account,
    stored: Transaction,
    /// The account's own part, as stored.
    own: Split,
    /// The total as the user typed it, without sign.
    total: Money,
    /// The total as it goes into the account.
    into_account: Money,
    direction: Direction,
    date: Date,
    /// Parts booked to another of the user's accounts, kept as they are.
    fixed: Vec<Split>,
    parts: Vec<DraftPart<'a>>,
    /// The Part whose amount is worked out from the rest.
    open: Option<usize>,
}

struct DraftPart<'a> {
    /// The part as stored, or a new one; its amount is zero while open.
    split: Split,
    category: Option<Account>,
    foreign_amount: &'a str,
}

/// The Parts' amounts once the open one is worked out.
struct Resolved {
    /// One per Part entered, in the transaction's currency and sign.
    parts: Vec<Money>,
    /// What the Parts add up to, as the user counts them, with the ones
    /// that stay as they are.
    sum: Money,
    /// The total minus that sum: positive when something is still to be
    /// sorted, negative when the Parts have too much.
    difference: Money,
}

/// A Part's amount in its category's currency, and the rate behind it.
struct Foreign {
    amount: Money,
    /// The book's rate the amount was worked out with, or that matches the
    /// amount typed; `None` when the two amounts stand on their own.
    rate: Option<DatedRate>,
    /// Whether the amount was worked out from the book's rates.
    suggested: bool,
}

impl<'a> Draft<'a> {
    fn read(
        book: &Book,
        account: &str,
        id: &str,
        entered: &PartsInput<'a>,
        locale: &impl Locale,
    ) -> Result<Self, PartsError> {
        let account = find_account(book, account)?;
        let stored = find_transaction(book, account.id, id)?;
        let (own, others) = split_off_own(book, &account, &stored)?;
        let own = own.clone();
        let direction = Direction::of(entered.money_in);
        let total = read_amount(book, account.currency, entered.total, None, locale)?;
        if total.is_zero() || total.is_negative() {
            return Err(PartsError::NotPositive);
        }
        let into_account = match direction {
            Direction::In => total,
            Direction::Out => total.checked_neg().map_err(|_| PartsError::TooLarge)?,
        };
        let date = locale.read_date(entered.date).ok_or(PartsError::Date)?;
        let mut fixed = Vec::new();
        let mut stored_parts = Vec::new();
        for split in others {
            let other = split.account.map(|id| book.account(id)).transpose()?;
            match other {
                Some(other) if !other.kind.is_category() => fixed.push(split.clone()),
                _ => stored_parts.push(split.clone()),
            }
        }
        if entered.parts.is_empty() {
            return Err(PartsError::NoParts);
        }
        let mut parts = Vec::with_capacity(entered.parts.len());
        let mut open = None;
        for (index, part) in entered.parts.iter().enumerate() {
            let category = find_category(book, part.category)?;
            let amount = if part.amount.trim().is_empty() {
                if open.replace(index).is_some() {
                    return Err(PartsError::TooManyOpenParts);
                }
                Money::zero(account.currency)
            } else {
                direction.typed(read_amount(
                    book,
                    account.currency,
                    part.amount,
                    Some(index + 1),
                    locale,
                )?)?
            };
            // A part the transaction had keeps its ID and whatever else it
            // carries, such as an amount from an old currency (D-104).
            let kept = stored_parts
                .iter()
                .find(|split| !part.id.is_empty() && split.id.to_string() == part.id)
                .cloned();
            let split = match kept {
                Some(split) => Split {
                    amount,
                    foreign: None,
                    ..split
                },
                None => Split::uncategorized(amount),
            };
            parts.push(DraftPart {
                split,
                category,
                foreign_amount: part.foreign_amount,
            });
        }
        Ok(Self {
            account,
            stored,
            own,
            total,
            into_account,
            direction,
            date,
            fixed,
            parts,
            open,
        })
    }

    fn resolve(&self) -> Result<Resolved, PartsError> {
        let mut parts: Vec<Money> = self.parts.iter().map(|part| part.split.amount).collect();
        if let Some(open) = self.open {
            let mut splits = vec![Split {
                amount: self.into_account,
                ..self.own.clone()
            }];
            splits.extend(self.fixed.iter().cloned());
            splits.extend(self.parts.iter().map(|part| part.split.clone()));
            let open_id = self.parts[open].split.id;
            parts[open] =
                Transaction::remainder(&splits, open_id).map_err(|error| match error {
                    TransactionError::Overflow => PartsError::TooLarge,
                    other => PartsError::Transaction(other),
                })?;
        }
        let booked = Money::checked_sum(
            self.account.currency,
            self.fixed
                .iter()
                .map(|split| split.amount)
                .chain(parts.iter().copied()),
        )
        .map_err(|_| PartsError::TooLarge)?;
        let sum = self.direction.typed(booked)?;
        let difference = self
            .total
            .checked_sub(sum)
            .map_err(|_| PartsError::TooLarge)?;
        Ok(Resolved {
            parts,
            sum,
            difference,
        })
    }

    /// The currency of the Part at `index`'s category, if it isn't the
    /// transaction's.
    fn foreign_currency(&self, index: usize) -> Option<CurrencyCode> {
        self.parts[index]
            .category
            .as_ref()
            .map(|category| category.currency)
            .filter(|currency| *currency != self.account.currency)
    }

    /// The amount of the Part at `index`, whose amount in the transaction's
    /// currency is `amount`, in its category's currency: what the user
    /// typed, with the book's rate if it gives exactly that, otherwise
    /// worked out from the book's rate on the transaction's date (D-043).
    fn foreign_of(
        &self,
        book: &Book,
        index: usize,
        amount: Money,
        locale: &impl Locale,
    ) -> Result<Foreign, PartsError> {
        let currency = self
            .foreign_currency(index)
            .ok_or(PartsError::NoSuchEntry)?;
        let typed = self.parts[index].foreign_amount;
        let converted = match book.convert(amount, currency, self.date) {
            Ok(found) => Some(found),
            Err(StoreError::NoRate { .. }) => None,
            Err(error) => return Err(error.into()),
        };
        if typed.trim().is_empty() {
            return match converted {
                Some((amount, rate)) => Ok(Foreign {
                    amount,
                    rate: Some(rate),
                    suggested: true,
                }),
                None => Err(PartsError::ForeignAmountNeeded {
                    part: index + 1,
                    currency,
                }),
            };
        }
        let foreign =
            self.direction
                .typed(read_amount(book, currency, typed, Some(index + 1), locale)?)?;
        let opposite = (foreign.is_negative() && amount.minor_units() > 0)
            || (foreign.minor_units() > 0 && amount.is_negative());
        if opposite {
            return Err(PartsError::ForeignSignMismatch { part: index + 1 });
        }
        Ok(Foreign {
            amount: foreign,
            rate: converted
                .filter(|(suggested, _)| *suggested == foreign)
                .map(|(_, rate)| rate),
            suggested: false,
        })
    }
}

/// The rate two amounts imply, e.g. "1,0811" for 92,50 € and 100,00 USD,
/// with four decimal places, as text for the user; `None` if `amount` is
/// zero. Only shown, never stored: a Part keeps its two amounts (D-043).
fn implied_rate(
    book: &Book,
    amount: Money,
    foreign: Money,
    locale: &impl Locale,
) -> Result<Option<String>, PartsError> {
    if amount.is_zero() {
        return Ok(None);
    }
    const PLACES: u32 = 4;
    let amount_digits = u32::from(book.currency_minor_digits(amount.currency())?);
    let foreign_digits = u32::from(book.currency_minor_digits(foreign.currency())?);
    // rate = (foreign ÷ 10^fd) ÷ (amount ÷ 10^ad), scaled by 10^PLACES and
    // rounded half away from zero, all in 128-bit integers.
    let numerator =
        u128::from(foreign.minor_units().unsigned_abs()) * 10_u128.pow(amount_digits + PLACES);
    let denominator = u128::from(amount.minor_units().unsigned_abs()) * 10_u128.pow(foreign_digits);
    let scaled = (numerator + denominator / 2) / denominator;
    let plain = format!("{scaled:0>width$}", width = PLACES as usize + 1);
    let (whole, fraction) = plain.split_at(plain.len() - PLACES as usize);
    Ok(Some(rate_text(&format!("{whole}.{fraction}"), locale)))
}

/// A rate in its plain form ("1.0856") as the user reads numbers.
fn rate_text(plain: &str, locale: &impl Locale) -> String {
    let grouped = |whole: &str| {
        whole
            .parse::<u64>()
            .map_or_else(|_| whole.to_owned(), |value| locale.group_digits(value))
    };
    match plain.split_once('.') {
        Some((whole, fraction)) => {
            format!("{}{}{fraction}", grouped(whole), locale.decimal_point())
        }
        None => grouped(plain),
    }
}

/// The account's one part of `transaction`, and the others. The page can't
/// change a transaction that books to the account twice or in another
/// currency, nor the starting balance, which "Check against my bank"
/// corrects (D-116).
fn split_off_own<'t>(
    book: &Book,
    account: &Account,
    transaction: &'t Transaction,
) -> Result<(&'t Split, Vec<&'t Split>), PartsError> {
    let (own, others): (Vec<_>, Vec<_>) = transaction
        .splits()
        .iter()
        .partition(|split| split.account == Some(account.id));
    for other in &others {
        if let Some(id) = other.account
            && book.account(id)?.kind == AccountKind::Equity
        {
            return Err(PartsError::NotEditableHere);
        }
    }
    match own.as_slice() {
        [own] if own.foreign.is_none() && own.amount.currency() == account.currency => {
            Ok((own, others))
        }
        [] => Err(PartsError::NoSuchEntry),
        _ => Err(PartsError::NotEditableHere),
    }
}

/// The open category the UI means by `id`, in any currency; `None` for an
/// empty ID, i.e. no category yet.
fn find_category(book: &Book, id: &str) -> Result<Option<Account>, PartsError> {
    if id.is_empty() {
        return Ok(None);
    }
    book.all_accounts()?
        .into_iter()
        .find(|category| {
            category.kind.is_category() && !category.closed && category.id.to_string() == id
        })
        .map(Some)
        .ok_or(PartsError::NoSuchEntry)
}

/// Reads a typed amount in `currency`; `part` names the Part in an error.
fn read_amount(
    book: &Book,
    currency: CurrencyCode,
    text: &str,
    part: Option<usize>,
    locale: &impl Locale,
) -> Result<Money, PartsError> {
    let format = locale.amount_format(currency);
    let group_separator = locale.group_separator();
    let reader = AmountReader {
        format: &format,
        group_separator: &group_separator,
        currency,
        minor_digits: book.currency_minor_digits(currency)?,
    };
    reader.read(text).map_err(|error| PartsError::Amount {
        part,
        currency,
        error,
    })
}

/// `amount` without its sign.
fn unsigned(amount: Money) -> Result<Money, PartsError> {
    if amount.is_negative() {
        amount.checked_neg().map_err(|_| PartsError::TooLarge)
    } else {
        Ok(amount)
    }
}

/// `currency` as text if it isn't `own`, else empty.
fn other_currency(currency: CurrencyCode, own: CurrencyCode) -> String {
    if currency == own {
        String::new()
    } else {
        currency.to_string()
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountType, DatedRateId, ImportedDetails, RateSource, TransactionId};

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::categories::{NewCategory, add_category};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::register::{Sort, register_view};
    use crate::transaction_entry::{NewEntry, add};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };
    const USD: CurrencyCode = match CurrencyCode::new("USD") {
        Ok(code) => code,
        Err(_) => panic!("USD is a valid code"),
    };

    /// A book after the first start, with the accounts screen's "Checking
    /// account" holding 1000 € on 1 Oct 2026 (M3.3's real output), and the
    /// account's ID as text.
    fn book_with_checking() -> (Book, String) {
        let mut book = started();
        let checking = add_account(
            &mut book,
            &NewAccount {
                name: "Checking account",
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "1000",
                date: "2026-10-01",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string();
        (book, checking)
    }

    fn category_id(book: &Book, path: &str) -> String {
        book.categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.path.join(" › ") == path)
            .map(|entry| entry.category.id.to_string())
            .unwrap_or_else(|| panic!("category {path}"))
    }

    /// A payment from the register's page (M3.5's real output), and its ID.
    fn pay(book: &mut Book, account: &str, amount: &str, to: &str, category: &str) -> String {
        add(
            book,
            &NewEntry {
                account,
                money_in: false,
                amount,
                other_party: to,
                category,
                date: "2026-10-05",
            },
            &TestLocale,
        )
        .expect("added");
        newest(book, account)
    }

    /// The ID of the account's newest transaction.
    fn newest(book: &Book, account: &str) -> String {
        let id = find_account(book, account).expect("account").id;
        book.register(id)
            .expect("register")
            .last()
            .map(|entry| entry.transaction.to_string())
            .expect("a transaction")
    }

    fn part<'a>(id: &'a str, category: &'a str, amount: &'a str) -> PartInput<'a> {
        PartInput {
            id,
            category,
            amount,
            foreign_amount: "",
        }
    }

    fn paid<'a>(total: &'a str, parts: &'a [PartInput<'a>]) -> PartsInput<'a> {
        PartsInput {
            money_in: false,
            total,
            other_party: "Corner Grocery",
            date: "2026-10-05",
            parts,
        }
    }

    fn code(book: &Book, result: Result<(), PartsError>) -> ErrorText {
        error_text(book, &result.expect_err("refused"), &TestLocale)
    }

    /// The error of a change that is refused, as the UI shows it. A macro,
    /// so the change is made before the book is read for the message.
    macro_rules! refused {
        ($book:ident, $result:expr $(,)?) => {{
            let result = $result;
            code(&$book, result)
        }};
    }

    fn rows(book: &Book, account: &str) -> Vec<(String, &'static str, String, String)> {
        let id = find_account(book, account).expect("account").id;
        register_view(book, id, Sort::default(), &TestLocale)
            .expect("register")
            .rows
            .into_iter()
            .map(|row| (row.date, row.kind, row.amount.text, row.balance.text))
            .collect()
    }

    /// The transaction's parts booked to categories or to none, as (category
    /// path or "", minor units, foreign amount), in the order stored.
    fn booked(book: &Book, id: &str) -> Vec<(String, i64, Option<Money>)> {
        let transaction = book
            .all_accounts()
            .expect("accounts")
            .iter()
            .filter(|account| account.kind.is_account())
            .flat_map(|account| book.register(account.id).expect("register"))
            .find(|entry| entry.transaction.to_string() == id)
            .map(|entry| book.transaction(entry.transaction).expect("transaction"))
            .expect("found");
        transaction
            .splits()
            .iter()
            .filter(|split| {
                split
                    .account
                    .is_none_or(|id| book.account(id).expect("account").kind.is_category())
            })
            .map(|split| {
                let path = split
                    .account
                    .map(|id| book.account(id).expect("account").name)
                    .unwrap_or_default();
                (
                    path,
                    split.amount.minor_units(),
                    split.foreign.as_ref().map(|foreign| foreign.amount),
                )
            })
            .collect()
    }

    #[test]
    fn breaks_a_purchase_down_and_works_out_the_last_part() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let clothes = category_id(&book, "Clothes");
        let eating_out = category_id(&book, "Food › Eating out");
        let id = pay(&mut book, &checking, "53,20", "Corner Grocery", &groceries);

        // The simple transaction's one Part is shown open, so adding Parts
        // takes their amounts from it (D-017).
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(
            (
                form.total.as_str(),
                form.money_in,
                form.currency.as_str(),
                form.total_fixed,
                form.other_party.as_str(),
                form.date.as_str()
            ),
            ("53,20", false, "EUR", false, "Corner Grocery", "2026-10-05")
        );
        assert_eq!(form.fixed, []);
        assert_eq!(form.parts.len(), 1);
        assert_eq!(
            (
                form.parts[0].category.as_str(),
                form.parts[0].amount.as_str(),
                form.parts[0].currency.as_str()
            ),
            (groceries.as_str(), "", "")
        );
        let first = form.parts[0].id.clone();

        let parts = [
            part(&first, &groceries, ""),
            part("", &clothes, "8"),
            part("", &eating_out, "5,20"),
        ];
        let shown =
            preview(&book, &checking, &id, &paid("53,20", &parts), &TestLocale).expect("preview");
        assert_eq!(
            shown
                .parts
                .iter()
                .map(|part| (part.amount.as_str(), part.open))
                .collect::<Vec<_>>(),
            [("40,00", true), ("8,00", false), ("5,20", false)]
        );
        assert_eq!(
            (
                shown.sum.as_str(),
                shown.missing.as_str(),
                shown.over.as_str()
            ),
            ("53,20 €", "", "")
        );

        save(
            &mut book,
            &checking,
            &id,
            &paid("53,20", &parts),
            &TestLocale,
        )
        .expect("saved");
        assert_eq!(
            rows(&book, &checking),
            [
                (
                    "2026-10-05".into(),
                    "parts",
                    "-53,20 €".into(),
                    "946,80 €".into()
                ),
                (
                    "2026-10-01".into(),
                    "starting-balance",
                    "1000,00 €".into(),
                    "1000,00 €".into()
                ),
            ]
        );
        assert_eq!(
            booked(&book, &id),
            [
                ("Groceries".into(), 4000, None),
                ("Clothes".into(), 800, None),
                ("Eating out".into(), 520, None),
            ]
        );
        // The Part the transaction had keeps its ID.
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(form.parts[0].id, first);
        assert_eq!(
            form.parts
                .iter()
                .map(|part| part.amount.as_str())
                .collect::<Vec<_>>(),
            ["40,00", "8,00", ""]
        );

        // A refund on the same bill points the other way: the open Part
        // grows by it.
        let parts = [part(&first, &groceries, ""), part("", &clothes, "-3")];
        save(
            &mut book,
            &checking,
            &id,
            &paid("53,20", &parts),
            &TestLocale,
        )
        .expect("saved");
        assert_eq!(
            booked(&book, &id),
            [
                ("Groceries".into(), 5620, None),
                ("Clothes".into(), -300, None),
            ]
        );
        assert_eq!(rows(&book, &checking)[0].3, "946,80 €");
    }

    #[test]
    fn refuses_parts_that_do_not_add_up_with_a_message_each() {
        let (mut book, checking) = book_with_checking();
        let groceries = category_id(&book, "Food › Groceries");
        let clothes = category_id(&book, "Clothes");
        let id = pay(&mut book, &checking, "53,20", "Corner Grocery", &groceries);

        // Every amount typed, and too little: the preview says what is
        // missing, saving refuses with the same figures.
        let parts = [part("", &groceries, "40"), part("", &clothes, "13")];
        let shown =
            preview(&book, &checking, &id, &paid("53,20", &parts), &TestLocale).expect("preview");
        assert_eq!(
            (
                shown.sum.as_str(),
                shown.missing.as_str(),
                shown.over.as_str()
            ),
            ("53,00 €", "0,20 €", "")
        );
        let refused = refused!(
            book,
            save(
                &mut book,
                &checking,
                &id,
                &paid("53,20", &parts),
                &TestLocale,
            ),
        );
        assert_eq!(refused.code, "input.parts-dont-add-up");
        assert_eq!(
            refused.params,
            [
                ("parts", "53,00 €".into()),
                ("total", "53,20 €".into()),
                ("missing", "0,20 €".into())
            ]
        );

        // Too much.
        let parts = [part("", &groceries, "40"), part("", &clothes, "14")];
        let shown =
            preview(&book, &checking, &id, &paid("53,20", &parts), &TestLocale).expect("preview");
        assert_eq!(
            (shown.missing.as_str(), shown.over.as_str()),
            ("", "0,80 €")
        );
        let refused = refused!(
            book,
            save(
                &mut book,
                &checking,
                &id,
                &paid("53,20", &parts),
                &TestLocale,
            ),
        );
        assert_eq!(refused.params[2], ("over", "0,80 €".into()));

        let cases: [(&str, &[PartInput<'_>], &str, Option<&str>); 6] = [
            ("53,20", &[], "input.no-parts", None),
            (
                "53,20",
                &[part("", &groceries, ""), part("", &clothes, "")],
                "input.too-many-open-parts",
                None,
            ),
            (
                "53,20",
                &[part("", &groceries, ""), part("", &clothes, "8.5")],
                "input.malformed-amount",
                Some("2"),
            ),
            (
                "0",
                &[part("", &groceries, "")],
                "input.amount-not-positive",
                None,
            ),
            (
                "abc",
                &[part("", &groceries, "")],
                "input.malformed-amount",
                None,
            ),
            (
                "53,20",
                &[part("", "not-a-category", "")],
                "input.no-such-entry",
                None,
            ),
        ];
        for (total, parts, expected, part_number) in cases {
            let refused = refused!(
                book,
                save(&mut book, &checking, &id, &paid(total, parts), &TestLocale),
            );
            assert_eq!(refused.code, expected, "{total} {expected}");
            let found = refused
                .params
                .iter()
                .find(|(name, _)| *name == "part")
                .map(|(_, value)| value.as_str());
            assert_eq!(found, part_number, "{expected}");
        }
        let refused = refused!(
            book,
            save(
                &mut book,
                &checking,
                &id,
                &PartsInput {
                    date: "yesterday",
                    ..paid("53,20", &[part("", &groceries, "")])
                },
                &TestLocale,
            ),
        );
        assert_eq!(refused.code, "input.malformed-date");
        // Nothing of this changed the book.
        assert_eq!(booked(&book, &id), [("Groceries".into(), 5320, None)]);
    }

    #[test]
    fn a_part_in_another_currency_keeps_both_amounts_and_the_rate() {
        let (mut book, checking) = book_with_checking();
        let travel = add_category(
            &mut book,
            &NewCategory {
                name: "Travel",
                income: false,
                parent: "",
                currency: "USD",
            },
            EUR,
        )
        .expect("category")
        .to_string();
        let groceries = category_id(&book, "Food › Groceries");
        let id = pay(&mut book, &checking, "92,50", "Example Hotel", "");

        // The categories offered name a currency only where it differs.
        let choices = category_choices(&book, &checking, &id, false).expect("choices");
        let travel_choice = choices
            .iter()
            .find(|choice| choice.id == travel)
            .expect("travel");
        assert_eq!(
            (travel_choice.path.as_str(), travel_choice.currency.as_str()),
            ("Travel", "USD")
        );
        assert_eq!(
            choices
                .iter()
                .find(|choice| choice.id == groceries)
                .map(|choice| choice.currency.as_str()),
            Some("")
        );
        assert!(
            choices
                .iter()
                .all(|choice| !choice.path.starts_with("Salary"))
        );

        // Without a rate in the book, the amount in dollars has to be typed.
        let open = [part("", &travel, "")];
        let shown =
            preview(&book, &checking, &id, &paid("92,50", &open), &TestLocale).expect("preview");
        assert_eq!(
            (
                shown.parts[0].currency.as_str(),
                shown.parts[0].foreign_amount.as_str(),
                shown.parts[0].suggested,
                shown.parts[0].rate.as_str()
            ),
            ("USD", "", false, "")
        );
        let refused = refused!(
            book,
            save(
                &mut book,
                &checking,
                &id,
                &paid("92,50", &open),
                &TestLocale,
            ),
        );
        assert_eq!(refused.code, "input.foreign-amount-needed");
        assert_eq!(
            refused.params,
            [("currency", "USD".into()), ("part", "1".into())]
        );

        // Both amounts typed: the Part keeps them, and the rate they imply
        // is only shown (D-043).
        let typed = [PartInput {
            foreign_amount: "100",
            ..part("", &travel, "")
        }];
        let shown =
            preview(&book, &checking, &id, &paid("92,50", &typed), &TestLocale).expect("preview");
        assert_eq!(
            (
                shown.parts[0].foreign_amount.as_str(),
                shown.parts[0].rate.as_str(),
                shown.parts[0].rate_from.as_str(),
                shown.parts[0].rate_to.as_str(),
                shown.parts[0].rate_date.as_str()
            ),
            ("100,00", "1,0811", "EUR", "USD", "")
        );
        save(
            &mut book,
            &checking,
            &id,
            &paid("92,50", &typed),
            &TestLocale,
        )
        .expect("saved");
        let stored = book
            .transaction(
                book.register(find_account(&book, &checking).expect("account").id)
                    .expect("register")
                    .last()
                    .expect("entry")
                    .transaction,
            )
            .expect("transaction");
        let foreign = stored.splits()[1].foreign.expect("foreign amount");
        assert_eq!(foreign.amount, Money::from_minor_units(10000, USD));
        assert_eq!(foreign.rate, None);
        assert_eq!(rows(&book, &checking)[0].2, "-92,50 €");
        let form = details(&book, &checking, &id, &TestLocale).expect("details");
        assert_eq!(
            (
                form.parts[0].currency.as_str(),
                form.parts[0].foreign_amount.as_str()
            ),
            ("USD", "100,00")
        );

        // With a rate in the book, the amount is worked out from it and the
        // Part records the rate; so does an amount typed that matches it.
        let rate = DatedRate {
            id: DatedRateId::generate(),
            date: Date::constant(2026, 10, 1),
            from: EUR,
            to: USD,
            rate: "1.0811".parse().expect("rate"),
            source: RateSource::Manual,
        };
        book.add_rate(&rate).expect("rate added");
        let shown =
            preview(&book, &checking, &id, &paid("92,50", &open), &TestLocale).expect("preview");
        assert_eq!(
            (
                shown.parts[0].foreign_amount.as_str(),
                shown.parts[0].suggested,
                shown.parts[0].rate.as_str(),
                shown.parts[0].rate_date.as_str()
            ),
            ("100,00", true, "1,0811", "2026-10-01")
        );
        let recorded = |book: &mut Book, parts: &[PartInput<'_>]| {
            save(book, &checking, &id, &paid("92,50", parts), &TestLocale).expect("saved");
            booked(book, &id)[0].2.expect("foreign");
            let transaction = book
                .transaction(
                    book.register(find_account(book, &checking).expect("account").id)
                        .expect("register")
                        .last()
                        .expect("entry")
                        .transaction,
                )
                .expect("transaction");
            transaction.splits()[1]
                .foreign
                .expect("foreign")
                .rate
                .map(|found| found.id)
        };
        assert_eq!(recorded(&mut book, &open), Some(rate.id));
        assert_eq!(recorded(&mut book, &typed), Some(rate.id));
        let other = [PartInput {
            foreign_amount: "101",
            ..part("", &travel, "")
        }];
        assert_eq!(recorded(&mut book, &other), None);

        // An amount in dollars pointing the other way is refused.
        let opposite = [PartInput {
            foreign_amount: "-5",
            ..part("", &travel, "")
        }];
        let refused = refused!(
            book,
            save(
                &mut book,
                &checking,
                &id,
                &paid("92,50", &opposite),
                &TestLocale,
            ),
        );
        assert_eq!(refused.code, "input.foreign-sign-mismatch");

        // A Part in dollars next to one in euros, worked out from the rest.
        let mixed = [
            PartInput {
                foreign_amount: "80",
                ..part("", &travel, "74")
            },
            part("", &groceries, ""),
        ];
        save(
            &mut book,
            &checking,
            &id,
            &paid("92,50", &mixed),
            &TestLocale,
        )
        .expect("saved");
        assert_eq!(
            booked(&book, &id),
            [
                (
                    "Travel".into(),
                    7400,
                    Some(Money::from_minor_units(8000, USD))
                ),
                ("Groceries".into(), 1850, None),
            ]
        );
    }

    #[test]
    fn keeps_the_banks_total_and_money_moved_to_another_account() {
        let (mut book, checking) = book_with_checking();
        let savings = add_account(
            &mut book,
            &NewAccount {
                name: "Savings",
                account_type: AccountType::Savings,
                currency: "EUR",
                balance: "",
                date: "2026-10-01",
            },
            &TestLocale,
        )
        .expect("account")
        .to_string();
        let checking_id = find_account(&book, &checking).expect("account").id;
        let savings_id = find_account(&book, &savings).expect("account").id;
        let rent = category_id(&book, "Rent");
        let groceries = category_id(&book, "Food › Groceries");
        let eur = |minor| Money::from_minor_units(minor, EUR);

        // An entry from the bank (as the import books it): its total stays.
        let imported = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 6),
            Some("CORNER GROCERY".into()),
            vec![
                Split {
                    imported: Some(ImportedDetails {
                        bank_id: Some("1".into()),
                        booking_date: None,
                        value_date: None,
                        other_party_name: Some("CORNER GROCERY".into()),
                        other_party_iban: None,
                        remittance_information: None,
                    }),
                    ..Split::new(checking_id, eur(-5320))
                },
                Split::uncategorized(eur(5320)),
            ],
        )
        .expect("transaction");
        book.add_transaction(&imported).expect("added");
        let imported = imported.id().to_string();
        let form = details(&book, &checking, &imported, &TestLocale).expect("details");
        assert!(form.total_fixed);
        let refused = refused!(
            book,
            save(
                &mut book,
                &checking,
                &imported,
                &paid("60", &[part("", &groceries, "")]),
                &TestLocale,
            ),
        );
        assert_eq!(refused.code, "store.imported-split-changed");
        save(
            &mut book,
            &checking,
            &imported,
            &paid("53,20", &[part("", &groceries, "40"), part("", "", "")]),
            &TestLocale,
        )
        .expect("saved");
        assert_eq!(
            booked(&book, &imported),
            [("Groceries".into(), 4000, None), ("".into(), 1320, None)]
        );

        // A Transfer with a fee: what Savings got stays as it is and counts
        // in the sum; the fee is the Part to sort.
        let transfer = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 7),
            None,
            vec![
                Split::new(checking_id, eur(-10000)),
                Split::new(savings_id, eur(8000)),
                Split::uncategorized(eur(2000)),
            ],
        )
        .expect("transaction");
        book.add_transaction(&transfer).expect("added");
        let savings_part = transfer.splits()[1].id;
        let transfer = transfer.id().to_string();
        let form = details(&book, &checking, &transfer, &TestLocale).expect("details");
        assert_eq!(
            form.fixed,
            [FixedPart {
                name: "Savings".into(),
                amount: "80,00 €".into(),
                incoming: false,
            }]
        );
        assert_eq!(form.parts.len(), 1);
        let open = [part("", &rent, "")];
        let shown = preview(
            &book,
            &checking,
            &transfer,
            &PartsInput {
                other_party: "",
                date: "2026-10-07",
                ..paid("100", &open)
            },
            &TestLocale,
        )
        .expect("preview");
        assert_eq!(
            (shown.parts[0].amount.as_str(), shown.sum.as_str()),
            ("20,00", "100,00 €")
        );
        save(
            &mut book,
            &checking,
            &transfer,
            &PartsInput {
                other_party: "",
                date: "2026-10-07",
                ..paid("100", &open)
            },
            &TestLocale,
        )
        .expect("saved");
        let stored = book
            .transaction(
                book.register(savings_id)
                    .expect("register")
                    .last()
                    .expect("entry")
                    .transaction,
            )
            .expect("transaction");
        let kept = stored
            .splits()
            .iter()
            .find(|split| split.account == Some(savings_id))
            .expect("savings part");
        assert_eq!((kept.id, kept.amount), (savings_part, eur(8000)));
        assert_eq!(booked(&book, &transfer), [("Rent".into(), 2000, None)]);
        assert_eq!(rows(&book, &savings)[0].2, "80,00 €");

        // Seen from Savings, the checking account's part is the fixed one.
        let form = details(&book, &savings, &transfer, &TestLocale).expect("details");
        assert_eq!(
            (
                form.total.as_str(),
                form.money_in,
                form.fixed[0].name.as_str(),
                form.fixed[0].incoming
            ),
            ("80,00", true, "Checking account", true)
        );
    }

    #[test]
    fn breaks_an_income_down_and_can_turn_it_around() {
        let (mut book, checking) = book_with_checking();
        let salary = category_id(&book, "Salary");
        let other_income = category_id(&book, "Other income");
        add(
            &mut book,
            &NewEntry {
                account: &checking,
                money_in: true,
                amount: "2.400",
                other_party: "Example Employer Ltd.",
                category: &salary,
                date: "2026-10-03",
            },
            &TestLocale,
        )
        .expect("added");
        let id = newest(&book, &checking);
        let received = |parts| PartsInput {
            money_in: true,
            total: "2.400",
            other_party: "Example Employer Ltd.",
            date: "2026-10-03",
            parts,
        };
        let parts = [part("", &salary, "2.000"), part("", &other_income, "")];
        let shown =
            preview(&book, &checking, &id, &received(&parts), &TestLocale).expect("preview");
        assert_eq!(shown.parts[1].amount, "400,00");
        save(&mut book, &checking, &id, &received(&parts), &TestLocale).expect("saved");
        assert_eq!(
            booked(&book, &id),
            [
                ("Salary".into(), -200_000, None),
                ("Other income".into(), -40_000, None)
            ]
        );
        assert_eq!(rows(&book, &checking)[0].2, "2400,00 €");

        // Turned around, every Part points the other way too.
        let turned = [part("", &salary, "")];
        save(
            &mut book,
            &checking,
            &id,
            &PartsInput {
                money_in: false,
                ..received(&turned)
            },
            &TestLocale,
        )
        .expect("saved");
        assert_eq!(booked(&book, &id), [("Salary".into(), 240_000, None)]);
        assert_eq!(rows(&book, &checking)[0].2, "-2400,00 €");
    }

    #[test]
    fn refuses_what_it_cannot_change_here() {
        let (mut book, checking) = book_with_checking();
        let checking_id = find_account(&book, &checking).expect("account").id;
        let groceries = category_id(&book, "Food › Groceries");
        let eur = |minor| Money::from_minor_units(minor, EUR);

        // The account booked twice.
        let twice = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 6),
            None,
            vec![
                Split::new(checking_id, eur(-1000)),
                Split::new(checking_id, eur(-500)),
                Split::uncategorized(eur(1500)),
            ],
        )
        .expect("transaction");
        book.add_transaction(&twice).expect("added");
        let refused = details(&book, &checking, &twice.id().to_string(), &TestLocale);
        assert_eq!(
            error_text(&book, &refused.expect_err("refused"), &TestLocale).code,
            "input.not-editable-here"
        );

        // A transaction in another currency than the account.
        let travel = add_category(
            &mut book,
            &NewCategory {
                name: "Travel",
                income: false,
                parent: "",
                currency: "USD",
            },
            EUR,
        )
        .expect("category");
        let usd = |minor| Money::from_minor_units(minor, USD);
        let abroad = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 6),
            None,
            vec![
                Split {
                    foreign: Some(ForeignAmount {
                        amount: eur(-9250),
                        rate: None,
                    }),
                    ..Split::new(checking_id, usd(-10000))
                },
                Split::new(travel, usd(10000)),
            ],
        )
        .expect("transaction");
        book.add_transaction(&abroad).expect("added");
        let refused = details(&book, &checking, &abroad.id().to_string(), &TestLocale);
        assert_eq!(
            error_text(&book, &refused.expect_err("refused"), &TestLocale).code,
            "input.not-editable-here"
        );

        // The starting balance is corrected through "Check against my bank".
        let starting = book
            .register(checking_id)
            .expect("register")
            .first()
            .map(|entry| entry.transaction.to_string())
            .expect("starting balance");
        let refused = details(&book, &checking, &starting, &TestLocale);
        assert_eq!(
            error_text(&book, &refused.expect_err("refused"), &TestLocale).code,
            "input.not-editable-here"
        );

        // A transaction that isn't the account's, or isn't there.
        let id = pay(&mut book, &checking, "5", "", &groceries);
        let refused = save(
            &mut book,
            "not-an-account",
            &id,
            &paid("5", &[part("", &groceries, "")]),
            &TestLocale,
        );
        assert_eq!(code(&book, refused).code, "input.no-such-entry");
        let refused = save(
            &mut book,
            &checking,
            "not-a-transaction",
            &paid("5", &[part("", &groceries, "")]),
            &TestLocale,
        );
        assert_eq!(code(&book, refused).code, "input.no-such-entry");
    }

    #[test]
    fn writes_rates_the_way_the_user_reads_numbers() {
        assert_eq!(rate_text("1.0856", &TestLocale), "1,0856");
        assert_eq!(rate_text("7", &TestLocale), "7");
        assert_eq!(rate_text("0.5", &TestLocale), "0,5");
        let (mut book, _) = book_with_checking();
        book.use_currency(USD).expect("dollars in use");
        let implied = |amount: i64, foreign: i64| {
            implied_rate(
                &book,
                Money::from_minor_units(amount, EUR),
                Money::from_minor_units(foreign, USD),
                &TestLocale,
            )
            .expect("rate")
        };
        assert_eq!(implied(9250, 10000), Some("1,0811".into()));
        assert_eq!(implied(-9250, -10000), Some("1,0811".into()));
        assert_eq!(implied(10000, 10000), Some("1,0000".into()));
        assert_eq!(implied(0, 10000), None);
    }
}
