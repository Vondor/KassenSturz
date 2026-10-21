// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The complete export: a whole book as one JSON file, documented in
//! `docs/EXPORT_FORMAT.md`, and reading it back (D-029, D-046, Q-049).
//!
//! It is the stable way to move a book's data: to a new book, to another
//! program, or from a development build to 0.1 (D-045). Everything the book
//! holds is in it, with its IDs, exactly as stored, except the change
//! history, which belongs to the book file and not to its data (D-066).

mod json;
mod read;

use std::collections::{HashMap, HashSet};
use std::io::Write;

use kassensturz_core::{
    Account, AccountKind, AccountType, AmountTest, Condition, Currency, CurrencyCode, DatedRate,
    MatchMode, Money, RateSource, RoundingMode, RuleId, RuleNode, SplitId, TextField, TextOperator,
    Transaction,
};

use crate::data::ExportData;
use crate::decimal_text;
use crate::error::ExportError;
use crate::exporter::{ExportKind, Exporter, ExporterInfo};

use json::Json;

pub use read::{MAX_FILE_BYTES, read_book, read_book_file};

/// The value of the file's `format` key: it marks a complete export of a
/// KassenSturz book.
pub const FORMAT: &str = "kassensturz-book";

/// The version of the format this KassenSturz writes, the file's `version`.
/// Every change to the format gets a new one; KassenSturz reads every
/// version up to its own and refuses newer ones (`docs/EXPORT_FORMAT.md`).
/// Version 2 added a part's original amount, from before it was converted
/// out of a withdrawn currency (D-104), version 3 a part's own date and
/// payee, which its account shows instead of the transaction's (D-108).
pub const VERSION: u64 = 3;

/// How deep Rules can sit inside each other in the file: a top-level Rule is
/// at depth 1. Deeper trees are refused when writing and reading, so a file
/// KassenSturz writes can always be read back, and a file can't nest
/// without end.
pub const MAX_RULE_DEPTH: usize = 32;

/// A whole book, for the complete export: everything it holds except its
/// change history (D-029, D-066).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookData {
    /// How the book's calculations round (D-027).
    pub rounding_mode: RoundingMode,
    /// For how many days the book keeps its change history, `None` for
    /// forever (D-066).
    pub history_keep_days: Option<u32>,
    /// The currencies the book uses, with the decimal places it stores for
    /// each (D-027). Every amount below is in one of them.
    pub currencies: Vec<Currency>,
    /// The accounts and categories, and the internal account for starting
    /// balances.
    pub accounts: Vec<Account>,
    /// The price table (D-028).
    pub rates: Vec<DatedRate>,
    /// The Rules, as a tree in the user's order (D-014).
    pub rules: Vec<RuleNode>,
    /// The transactions with their parts.
    pub transactions: Vec<Transaction>,
    /// The parts a Rule booked that the Rules may change again (D-088).
    pub rule_marks: Vec<BookedByRule>,
}

/// A part a Rule booked, which the Rules may book again when the user
/// changes that Rule (D-088).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BookedByRule {
    /// The part.
    pub split: SplitId,
    /// The Rule that booked it, or `None` if it was removed since.
    pub rule: Option<RuleId>,
}

/// The complete export of a book (D-029): one JSON file in the format of
/// `docs/EXPORT_FORMAT.md`. ID `book`, extension `json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookExport;

impl Exporter for BookExport {
    fn info(&self) -> ExporterInfo {
        ExporterInfo {
            id: "book",
            kind: ExportKind::Book,
            extension: "json",
        }
    }

    fn write(&self, data: &ExportData, out: &mut dyn Write) -> Result<(), ExportError> {
        data.check()?;
        let ExportData::Book(book) = data else {
            return Err(ExportError::WrongKind);
        };
        let writer = BookWriter::new(book);
        let file = Json::Object(vec![
            ("format", Json::text(FORMAT)),
            ("version", Json::Number(VERSION)),
            (
                "settings",
                Json::Object(vec![
                    (
                        "rounding_mode",
                        Json::text(rounding_name(book.rounding_mode)),
                    ),
                    (
                        "history_keep_days",
                        book.history_keep_days
                            .map_or(Json::Null, |days| Json::Number(days.into())),
                    ),
                ]),
            ),
            (
                "currencies",
                Json::List(
                    book.currencies
                        .iter()
                        .map(|currency| {
                            Json::Object(vec![
                                ("code", Json::text(currency.code().as_str())),
                                ("minor_digits", Json::Number(currency.minor_digits().into())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "accounts",
                Json::List(book.accounts.iter().map(account).collect()),
            ),
            ("rates", Json::List(book.rates.iter().map(rate).collect())),
            (
                "rules",
                Json::List(
                    book.rules
                        .iter()
                        .map(|node| writer.rule(node))
                        .collect::<Result<_, _>>()?,
                ),
            ),
            (
                "transactions",
                Json::List(
                    book.transactions
                        .iter()
                        .map(|transaction| writer.transaction(transaction))
                        .collect::<Result<_, _>>()?,
                ),
            ),
        ]);
        file.write(out)?;
        out.write_all(b"\n")?;
        Ok(())
    }
}

impl BookData {
    /// Fails if the data can't be written so that it reads back the same:
    /// an amount in a currency the book doesn't list, a mark for a part no
    /// transaction has, or Rules nested deeper than [`MAX_RULE_DEPTH`].
    pub(crate) fn check(&self) -> Result<(), ExportError> {
        let listed: HashSet<CurrencyCode> = self
            .currencies
            .iter()
            .map(|currency| currency.code())
            .collect();
        let is_listed = |amount: Money| {
            if listed.contains(&amount.currency()) {
                Ok(())
            } else {
                Err(ExportError::UnlistedCurrency {
                    currency: amount.currency(),
                    path: None,
                })
            }
        };
        let mut parts = HashSet::new();
        for transaction in &self.transactions {
            for split in transaction.splits() {
                is_listed(split.amount)?;
                if let Some(foreign) = split.foreign {
                    is_listed(foreign.amount)?;
                }
                if let Some(original) = split.original {
                    is_listed(original.amount)?;
                }
                parts.insert(split.id);
            }
        }
        if let Some(mark) = self
            .rule_marks
            .iter()
            .find(|mark| !parts.contains(&mark.split))
        {
            return Err(ExportError::MarkWithoutPart(mark.split));
        }
        let mut level: Vec<&RuleNode> = self.rules.iter().collect();
        let mut depth = 0;
        while !level.is_empty() {
            depth += 1;
            if depth > MAX_RULE_DEPTH {
                return Err(ExportError::RulesTooDeep {
                    max: MAX_RULE_DEPTH,
                });
            }
            for node in &level {
                for condition in &node.rule.conditions {
                    if let Condition::Amount(test) = condition {
                        let (min, max) = amount_range(*test);
                        for amount in min.into_iter().chain(max) {
                            is_listed(amount)?;
                        }
                    }
                }
            }
            level = level.iter().flat_map(|node| &node.sub_rules).collect();
        }
        Ok(())
    }
}

/// Writes the parts of the file that need the book's decimal places or its
/// marks.
struct BookWriter {
    digits: HashMap<CurrencyCode, u8>,
    marks: HashMap<SplitId, Option<RuleId>>,
}

impl BookWriter {
    fn new(book: &BookData) -> Self {
        Self {
            digits: book
                .currencies
                .iter()
                .map(|currency| (currency.code(), currency.minor_digits()))
                .collect(),
            marks: book
                .rule_marks
                .iter()
                .map(|mark| (mark.split, mark.rule))
                .collect(),
        }
    }

    /// `amount` as text with its currency's decimal places, e.g. `-12.50`.
    fn amount(&self, amount: Money) -> Result<Json, ExportError> {
        let currency = amount.currency();
        let digits = *self
            .digits
            .get(&currency)
            .ok_or(ExportError::UnlistedCurrency {
                currency,
                path: None,
            })?;
        decimal_text(amount.minor_units(), digits, '.')
            .map(Json::Text)
            .ok_or(ExportError::BadDecimalPlaces { currency, digits })
    }

    fn transaction(&self, transaction: &Transaction) -> Result<Json, ExportError> {
        let parts = transaction
            .splits()
            .iter()
            .map(|split| {
                let foreign = match split.foreign {
                    Some(foreign) => Json::Object(vec![
                        ("amount", self.amount(foreign.amount)?),
                        ("currency", Json::text(foreign.amount.currency().as_str())),
                        ("rate", foreign.rate.as_ref().map_or(Json::Null, rate)),
                    ]),
                    None => Json::Null,
                };
                let original = match split.original {
                    Some(original) => Json::Object(vec![
                        ("amount", self.amount(original.amount)?),
                        ("currency", Json::text(original.amount.currency().as_str())),
                        ("rate", rate(&original.rate)),
                    ]),
                    None => Json::Null,
                };
                let delivered = match &split.imported {
                    Some(imported) => Json::Object(vec![
                        ("bank_id", Json::optional_text(imported.bank_id.as_deref())),
                        ("booking_date", Json::optional_date(imported.booking_date)),
                        ("value_date", Json::optional_date(imported.value_date)),
                        (
                            "other_party_name",
                            Json::optional_text(imported.other_party_name.as_deref()),
                        ),
                        (
                            "other_party_iban",
                            Json::optional_text(imported.other_party_iban.as_deref()),
                        ),
                        (
                            "remittance_information",
                            Json::optional_text(imported.remittance_information.as_deref()),
                        ),
                    ]),
                    None => Json::Null,
                };
                let booked_by_rule = match self.marks.get(&split.id) {
                    Some(rule) => Json::Object(vec![("rule", Json::optional_id(*rule))]),
                    None => Json::Null,
                };
                Ok(Json::Object(vec![
                    ("id", Json::id(split.id)),
                    ("account", Json::optional_id(split.account)),
                    ("amount", self.amount(split.amount)?),
                    ("foreign", foreign),
                    ("original", original),
                    ("date", Json::optional_date(split.own_date)),
                    ("payee", Json::optional_text(split.own_payee.as_deref())),
                    ("delivered", delivered),
                    ("booked_by_rule", booked_by_rule),
                ]))
            })
            .collect::<Result<_, ExportError>>()?;
        Ok(Json::Object(vec![
            ("id", Json::id(transaction.id())),
            ("date", Json::date(transaction.booking_date())),
            ("payee", Json::optional_text(transaction.payee())),
            ("currency", Json::text(transaction.currency().as_str())),
            ("parts", Json::List(parts)),
        ]))
    }

    fn rule(&self, node: &RuleNode) -> Result<Json, ExportError> {
        let rule = &node.rule;
        let (category, transfer) = match rule.actions.book_to {
            Some(kassensturz_core::BookTo::Category(id)) => (Some(id), None),
            Some(kassensturz_core::BookTo::Transfer(id)) => (None, Some(id)),
            None => (None, None),
        };
        Ok(Json::Object(vec![
            ("id", Json::id(rule.id)),
            ("name", Json::text(&rule.name)),
            ("match", Json::text(match_name(rule.match_mode))),
            (
                "conditions",
                Json::List(
                    rule.conditions
                        .iter()
                        .map(|condition| self.condition(condition))
                        .collect::<Result<_, _>>()?,
                ),
            ),
            ("category", Json::optional_id(category)),
            ("transfer_account", Json::optional_id(transfer)),
            ("payee", Json::optional_text(rule.actions.payee.as_deref())),
            (
                "sub_rules",
                Json::List(
                    node.sub_rules
                        .iter()
                        .map(|sub| self.rule(sub))
                        .collect::<Result<_, _>>()?,
                ),
            ),
        ]))
    }

    fn condition(&self, condition: &Condition) -> Result<Json, ExportError> {
        let fields = match condition {
            Condition::Text {
                field,
                operator,
                value,
                case_sensitive,
            } => vec![
                ("field", Json::text(text_field_name(*field))),
                ("operator", Json::text(text_operator_name(*operator))),
                ("text", Json::text(value)),
                ("case_sensitive", Json::Bool(*case_sensitive)),
            ],
            Condition::Amount(test) => {
                let mut fields = vec![("field", Json::text("amount"))];
                match test {
                    AmountTest::Equals(amount) => fields.extend([
                        ("operator", Json::text("equals")),
                        ("amount", self.amount(*amount)?),
                        ("currency", Json::text(amount.currency().as_str())),
                    ]),
                    AmountTest::Between { min, max } => fields.extend([
                        ("operator", Json::text("between")),
                        ("min", self.amount(*min)?),
                        ("max", self.amount(*max)?),
                        ("currency", Json::text(min.currency().as_str())),
                    ]),
                    AmountTest::MoneyIn => fields.push(("operator", Json::text("money_in"))),
                    AmountTest::MoneyOut => fields.push(("operator", Json::text("money_out"))),
                }
                fields
            }
            Condition::BookingDate { first, last } => vec![
                ("field", Json::text("booking_date")),
                ("operator", Json::text("between")),
                ("first", Json::date(*first)),
                ("last", Json::date(*last)),
            ],
            Condition::Currency(currency) => vec![
                ("field", Json::text("currency")),
                ("operator", Json::text("is")),
                ("currency", Json::text(currency.as_str())),
            ],
            Condition::Account(account) => vec![
                ("field", Json::text("account")),
                ("operator", Json::text("is")),
                ("account", Json::id(*account)),
            ],
        };
        Ok(Json::Object(fields))
    }
}

fn account(account: &Account) -> Json {
    Json::Object(vec![
        ("id", Json::id(account.id)),
        ("name", Json::text(&account.name)),
        ("kind", Json::text(kind_name(account.kind))),
        ("currency", Json::text(account.currency.as_str())),
        (
            "type",
            account.account_type.map_or(Json::Null, |account_type| {
                Json::text(type_name(account_type))
            }),
        ),
        ("closed", Json::Bool(account.closed)),
        ("parent", Json::optional_id(account.parent)),
    ])
}

fn rate(rate: &DatedRate) -> Json {
    Json::Object(vec![
        ("id", Json::id(rate.id)),
        ("date", Json::date(rate.date)),
        ("from", Json::text(rate.from.as_str())),
        ("to", Json::text(rate.to.as_str())),
        ("rate", Json::Text(rate.rate.to_string())),
        ("source", Json::text(source_name(rate.source))),
    ])
}

/// The amounts an amount condition names, lowest first.
fn amount_range(test: AmountTest) -> (Option<Money>, Option<Money>) {
    match test {
        AmountTest::Equals(amount) => (Some(amount), None),
        AmountTest::Between { min, max } => (Some(min), Some(max)),
        AmountTest::MoneyIn | AmountTest::MoneyOut => (None, None),
    }
}

// The words of the format. They are keys of the file, not text for users:
// the same in every language, and never changed within a version
// (`docs/EXPORT_FORMAT.md`). Most are the words the book itself stores
// (`docs/BOOK_FORMAT.md`).

const ROUNDING_MODES: [(RoundingMode, &str); 7] = [
    (RoundingMode::HalfAwayFromZero, "half_away_from_zero"),
    (RoundingMode::HalfEven, "half_even"),
    (RoundingMode::HalfTowardZero, "half_toward_zero"),
    (RoundingMode::TowardZero, "toward_zero"),
    (RoundingMode::AwayFromZero, "away_from_zero"),
    (RoundingMode::Floor, "floor"),
    (RoundingMode::Ceiling, "ceiling"),
];

const KINDS: [(AccountKind, &str); 5] = [
    (AccountKind::Asset, "asset"),
    (AccountKind::Liability, "liability"),
    (AccountKind::Income, "income"),
    (AccountKind::Expense, "expense"),
    (AccountKind::Equity, "equity"),
];

const TYPES: [(AccountType, &str); 4] = [
    (AccountType::Checking, "checking"),
    (AccountType::Savings, "savings"),
    (AccountType::Cash, "cash"),
    (AccountType::CreditCard, "credit_card"),
];

/// Version 1 has only the first one.
const SOURCES: [(RateSource, &str); 2] = [
    (RateSource::Manual, "manual"),
    (RateSource::Changeover, "changeover"),
];

const MATCH_MODES: [(MatchMode, &str); 2] = [(MatchMode::All, "all"), (MatchMode::Any, "any")];

const TEXT_FIELDS: [(TextField, &str); 4] = [
    (TextField::OtherPartyName, "other_party_name"),
    (TextField::OtherPartyIban, "other_party_iban"),
    (TextField::RemittanceInformation, "remittance_information"),
    (TextField::Payee, "payee"),
];

const TEXT_OPERATORS: [(TextOperator, &str); 5] = [
    (TextOperator::Contains, "contains"),
    (TextOperator::Equals, "equals"),
    (TextOperator::StartsWith, "starts_with"),
    (TextOperator::EndsWith, "ends_with"),
    (TextOperator::Matches, "matches"),
];

/// The word for `value` in `table`. Every table above lists every value.
fn name_of<T: PartialEq + Copy>(table: &[(T, &'static str)], value: T) -> &'static str {
    table
        .iter()
        .find(|(listed, _)| *listed == value)
        .map(|(_, name)| *name)
        .expect("every value has a word in the format")
}

fn rounding_name(mode: RoundingMode) -> &'static str {
    name_of(&ROUNDING_MODES, mode)
}

fn kind_name(kind: AccountKind) -> &'static str {
    name_of(&KINDS, kind)
}

fn type_name(account_type: AccountType) -> &'static str {
    name_of(&TYPES, account_type)
}

fn source_name(source: RateSource) -> &'static str {
    name_of(&SOURCES, source)
}

fn match_name(mode: MatchMode) -> &'static str {
    name_of(&MATCH_MODES, mode)
}

fn text_field_name(field: TextField) -> &'static str {
    name_of(&TEXT_FIELDS, field)
}

fn text_operator_name(operator: TextOperator) -> &'static str {
    name_of(&TEXT_OPERATORS, operator)
}

#[cfg(test)]
mod tests;
