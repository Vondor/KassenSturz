// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a complete export back (D-029, D-046).
//!
//! The file is untrusted, like any file (AGENTS.md §8): someone may have
//! written it by hand or with another program. So it is read strictly. Every
//! key must be one the format has, every value of the type it needs, every
//! amount exact in its currency's decimal places, and every ID unique;
//! otherwise nothing is read and the error says where in the file the
//! problem is. What the format can't check on its own, e.g. that a
//! category's parent exists or that names are unique, the book checks when
//! the data goes into it. Text is kept exactly as it is in the file.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use kassensturz_core::{
    Account, AccountId, Actions, AmountTest, BookTo, Condition, Currency, CurrencyCode, Date,
    DatedRate, DatedRateId, ExchangeRate, ForeignAmount, ImportedDetails, Money, Rule, RuleId,
    RuleNode, Split, SplitId, Transaction, TransactionId,
};
use serde_json::{Map, Value};

use super::{
    BookData, BookedByRule, FORMAT, KINDS, MATCH_MODES, MAX_RULE_DEPTH, ROUNDING_MODES, SOURCES,
    TEXT_FIELDS, TEXT_OPERATORS, TYPES, VERSION,
};
use crate::error::{Expected, ExportError};

/// The largest file read back: 512 MiB, far more than a household's book
/// needs, so a file can't make KassenSturz run out of memory.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

type Result<T> = std::result::Result<T, ExportError>;

/// Reads the complete export in the file at `path`, as [`read_book`] does.
/// At most one byte more than [`MAX_FILE_BYTES`] is read.
pub fn read_book_file(path: &Path) -> Result<BookData> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    read_book(&bytes)
}

/// Reads a complete export written by [`BookExport`](super::BookExport), or
/// by another program in the same format (`docs/EXPORT_FORMAT.md`).
///
/// Fails if the file is too large, isn't JSON, isn't a complete export, was
/// written by a newer KassenSturz, or doesn't follow the format; the error
/// names the place in the file. Nothing is checked against a book here: the
/// book does that when the data goes into it.
pub fn read_book(bytes: &[u8]) -> Result<BookData> {
    read_at_most(bytes, MAX_FILE_BYTES)
}

/// [`read_book`] with `max` bytes as the largest file.
pub(super) fn read_at_most(bytes: &[u8], max: u64) -> Result<BookData> {
    if u64::try_from(bytes.len()).map_or(true, |len| len > max) {
        return Err(ExportError::TooLarge { max });
    }
    let file: Value = serde_json::from_slice(bytes).map_err(|error| ExportError::NotJson {
        line: error.line(),
        column: error.column(),
    })?;
    // First whether it is a complete export at all, then its version, so
    // another JSON file or a newer one isn't reported as full of mistakes.
    let top = file.as_object().ok_or(ExportError::NotABook)?;
    if top.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(ExportError::NotABook);
    }
    let version = top.get("version").ok_or(ExportError::MissingValue {
        path: "version".into(),
    })?;
    match version.as_u64() {
        Some(VERSION) => {}
        Some(newer) if newer > VERSION => return Err(ExportError::NewerVersion { version: newer }),
        _ => return Err(bad("version", Expected::Number)),
    }
    let top = Object::new(
        &file,
        "",
        &[
            "format",
            "version",
            "settings",
            "currencies",
            "accounts",
            "rates",
            "rules",
            "transactions",
        ],
    )?;
    let mut reader = Reader::default();

    let settings = top.object("settings", &["rounding_mode", "history_keep_days"])?;
    let rounding_mode = settings.keyword("rounding_mode", &ROUNDING_MODES)?;
    let history_keep_days = match settings.optional("history_keep_days") {
        Some((value, path)) => Some(
            value
                .as_u64()
                .and_then(|days| u32::try_from(days).ok())
                .ok_or_else(|| bad(&path, Expected::Number))?,
        ),
        None => None,
    };

    let mut currencies = Vec::new();
    for (value, path) in top.list("currencies")? {
        let currency = Object::new(value, &path, &["code", "minor_digits"])?;
        let code = currency.currency("code")?;
        let (digits, digits_path) = currency.required("minor_digits")?;
        let minor_digits = digits
            .as_u64()
            .and_then(|digits| u8::try_from(digits).ok())
            .and_then(|digits| Currency::new(code, digits))
            .ok_or_else(|| bad(&digits_path, Expected::Number))?;
        if reader
            .digits
            .insert(code, minor_digits.minor_digits())
            .is_some()
        {
            return Err(ExportError::Duplicate { path });
        }
        currencies.push(minor_digits);
    }

    let mut accounts = Vec::new();
    let mut account_ids = HashSet::new();
    for (value, path) in top.list("accounts")? {
        let account = Object::new(
            value,
            &path,
            &["id", "name", "kind", "currency", "type", "closed", "parent"],
        )?;
        let id = AccountId::from_bytes(account.id("id")?);
        if !account_ids.insert(id) {
            return Err(ExportError::Duplicate {
                path: format!("{path}.id"),
            });
        }
        accounts.push(Account {
            id,
            name: account.text("name")?,
            kind: account.keyword("kind", &KINDS)?,
            currency: account.currency("currency")?,
            account_type: account.optional_keyword("type", &TYPES)?,
            closed: account.boolean("closed")?,
            parent: account.optional_id("parent")?.map(AccountId::from_bytes),
        });
    }

    let mut rates = Vec::new();
    let mut rate_ids = HashSet::new();
    for (value, path) in top.list("rates")? {
        let object = Object::new(
            value,
            &path,
            &["id", "date", "from", "to", "rate", "source"],
        )?;
        let rate = reader.rate(&object)?;
        if !rate_ids.insert(rate.id) {
            return Err(ExportError::Duplicate {
                path: format!("{path}.id"),
            });
        }
        rates.push(rate);
    }

    let mut rules = Vec::new();
    for (value, path) in top.list("rules")? {
        rules.push(reader.rule(value, &path, 1)?);
    }

    let mut transactions = Vec::new();
    let mut rule_marks = Vec::new();
    let mut transaction_ids = HashSet::new();
    for (value, path) in top.list("transactions")? {
        let transaction = reader.transaction(value, &path, &mut rule_marks)?;
        if !transaction_ids.insert(transaction.id()) {
            return Err(ExportError::Duplicate {
                path: format!("{path}.id"),
            });
        }
        transactions.push(transaction);
    }

    Ok(BookData {
        rounding_mode,
        history_keep_days,
        currencies,
        accounts,
        rates,
        rules,
        transactions,
        rule_marks,
    })
}

/// What reading needs to remember across the file.
#[derive(Default)]
struct Reader {
    /// The decimal places of each currency the file lists.
    digits: HashMap<CurrencyCode, u8>,
    /// The IDs of the parts read so far: unique across the whole book.
    parts: HashSet<SplitId>,
    /// The IDs of the Rules read so far.
    rules: HashSet<RuleId>,
}

impl Reader {
    /// An amount in `currency`, e.g. `-12.50`: an optional minus, digits,
    /// and optionally a point and at most as many digits as the currency
    /// has decimal places. Read with integers only, never rounded
    /// (AGENTS.md §3.1 rule 1).
    fn amount(&self, value: &Value, path: &str, currency: CurrencyCode) -> Result<Money> {
        let digits = *self
            .digits
            .get(&currency)
            .ok_or_else(|| ExportError::UnlistedCurrency {
                currency,
                path: Some(path.to_owned()),
            })?;
        let text = value.as_str().ok_or_else(|| bad(path, Expected::Amount))?;
        parse_amount(text, digits)
            .map(|units| Money::from_minor_units(units, currency))
            .ok_or_else(|| bad(path, Expected::Amount))
    }

    fn rate(&self, rate: &Object<'_>) -> Result<DatedRate> {
        let (value, path) = rate.required("rate")?;
        let exchange_rate = value
            .as_str()
            .filter(|text| is_plain_decimal(text))
            .and_then(|text| text.parse::<ExchangeRate>().ok())
            .ok_or_else(|| bad(&path, Expected::Rate))?;
        Ok(DatedRate {
            id: DatedRateId::from_bytes(rate.id("id")?),
            date: rate.date("date")?,
            from: rate.currency("from")?,
            to: rate.currency("to")?,
            rate: exchange_rate,
            source: rate.keyword("source", &SOURCES)?,
        })
    }

    fn transaction(
        &mut self,
        value: &Value,
        path: &str,
        marks: &mut Vec<BookedByRule>,
    ) -> Result<Transaction> {
        let transaction = Object::new(value, path, &["id", "date", "payee", "currency", "parts"])?;
        let id = TransactionId::from_bytes(transaction.id("id")?);
        let booking_date = transaction.date("date")?;
        let payee = transaction.optional_text("payee")?;
        let currency = transaction.currency("currency")?;
        let mut splits = Vec::new();
        for (value, path) in transaction.list("parts")? {
            let part = Object::new(
                value,
                &path,
                &[
                    "id",
                    "account",
                    "amount",
                    "foreign",
                    "delivered",
                    "booked_by_rule",
                ],
            )?;
            let split = SplitId::from_bytes(part.id("id")?);
            if !self.parts.insert(split) {
                return Err(ExportError::Duplicate {
                    path: format!("{path}.id"),
                });
            }
            let (amount, amount_path) = part.required("amount")?;
            let amount = self.amount(amount, &amount_path, currency)?;
            let foreign = match part.optional("foreign") {
                Some((value, path)) => {
                    let foreign = Object::new(value, &path, &["amount", "currency", "rate"])?;
                    let foreign_currency = foreign.currency("currency")?;
                    let (amount, amount_path) = foreign.required("amount")?;
                    let rate = match foreign.optional("rate") {
                        Some((value, path)) => Some(self.rate(&Object::new(
                            value,
                            &path,
                            &["id", "date", "from", "to", "rate", "source"],
                        )?)?),
                        None => None,
                    };
                    Some(ForeignAmount {
                        amount: self.amount(amount, &amount_path, foreign_currency)?,
                        rate,
                    })
                }
                None => None,
            };
            let imported = match part.optional("delivered") {
                Some((value, path)) => {
                    let delivered = Object::new(
                        value,
                        &path,
                        &[
                            "bank_id",
                            "booking_date",
                            "value_date",
                            "other_party_name",
                            "other_party_iban",
                            "remittance_information",
                        ],
                    )?;
                    Some(ImportedDetails {
                        bank_id: delivered.optional_text("bank_id")?,
                        booking_date: delivered.optional_date("booking_date")?,
                        value_date: delivered.optional_date("value_date")?,
                        other_party_name: delivered.optional_text("other_party_name")?,
                        other_party_iban: delivered.optional_text("other_party_iban")?,
                        remittance_information: delivered
                            .optional_text("remittance_information")?,
                    })
                }
                None => None,
            };
            if let Some((value, path)) = part.optional("booked_by_rule") {
                let mark = Object::new(value, &path, &["rule"])?;
                marks.push(BookedByRule {
                    split,
                    rule: mark.optional_id("rule")?.map(RuleId::from_bytes),
                });
            }
            splits.push(Split {
                id: split,
                account: part.optional_id("account")?.map(AccountId::from_bytes),
                amount,
                foreign,
                imported,
            });
        }
        Transaction::new(id, booking_date, payee, splits).map_err(|error| {
            ExportError::InvalidTransaction {
                path: path.to_owned(),
                error,
            }
        })
    }

    fn rule(&mut self, value: &Value, path: &str, depth: usize) -> Result<RuleNode> {
        if depth > MAX_RULE_DEPTH {
            return Err(ExportError::RulesTooDeep {
                max: MAX_RULE_DEPTH,
            });
        }
        let rule = Object::new(
            value,
            path,
            &[
                "id",
                "name",
                "match",
                "conditions",
                "category",
                "transfer_account",
                "payee",
                "sub_rules",
            ],
        )?;
        let id = RuleId::from_bytes(rule.id("id")?);
        if !self.rules.insert(id) {
            return Err(ExportError::Duplicate {
                path: format!("{path}.id"),
            });
        }
        let category = rule.optional_id("category")?.map(AccountId::from_bytes);
        let transfer = rule
            .optional_id("transfer_account")?
            .map(AccountId::from_bytes);
        let book_to = match (category, transfer) {
            (Some(category), None) => Some(BookTo::Category(category)),
            (None, Some(account)) => Some(BookTo::Transfer(account)),
            (None, None) => None,
            (Some(_), Some(_)) => {
                return Err(bad(&format!("{path}.transfer_account"), Expected::Null));
            }
        };
        let mut conditions = Vec::new();
        for (value, path) in rule.list("conditions")? {
            conditions.push(self.condition(value, &path)?);
        }
        let mut sub_rules = Vec::new();
        if let Some((_, _)) = rule.optional("sub_rules") {
            for (value, path) in rule.list("sub_rules")? {
                sub_rules.push(self.rule(value, &path, depth + 1)?);
            }
        }
        Ok(RuleNode {
            rule: Rule {
                id,
                name: rule.text("name")?,
                match_mode: rule.keyword("match", &MATCH_MODES)?,
                conditions,
                actions: Actions {
                    book_to,
                    payee: rule.optional_text("payee")?,
                },
            },
            sub_rules,
        })
    }

    fn condition(&self, value: &Value, path: &str) -> Result<Condition> {
        // The field decides which other keys the condition has.
        let field = value
            .as_object()
            .ok_or_else(|| bad(path, Expected::Object))?
            .get("field")
            .ok_or_else(|| ExportError::MissingValue {
                path: format!("{path}.field"),
            })?
            .as_str()
            .unwrap_or_default();
        let operator = |condition: &Object<'_>, allowed: &[&str]| -> Result<String> {
            let (value, path) = condition.required("operator")?;
            value
                .as_str()
                .filter(|operator| allowed.contains(operator))
                .map(str::to_owned)
                .ok_or_else(|| bad(&path, Expected::Keyword))
        };
        if let Some(&(field, _)) = TEXT_FIELDS.iter().find(|(_, name)| *name == field) {
            let condition = Object::new(
                value,
                path,
                &["field", "operator", "text", "case_sensitive"],
            )?;
            return Ok(Condition::Text {
                field,
                operator: condition.keyword("operator", &TEXT_OPERATORS)?,
                value: condition.text("text")?,
                case_sensitive: condition.boolean("case_sensitive")?,
            });
        }
        match field {
            "amount" => {
                let any = Object::new(
                    value,
                    path,
                    &["field", "operator", "amount", "min", "max", "currency"],
                )?;
                let test = match operator(&any, &["equals", "between", "money_in", "money_out"])?
                    .as_str()
                {
                    "equals" => {
                        let condition =
                            Object::new(value, path, &["field", "operator", "amount", "currency"])?;
                        let currency = condition.currency("currency")?;
                        let (amount, amount_path) = condition.required("amount")?;
                        AmountTest::Equals(self.amount(amount, &amount_path, currency)?)
                    }
                    "between" => {
                        let condition = Object::new(
                            value,
                            path,
                            &["field", "operator", "min", "max", "currency"],
                        )?;
                        let currency = condition.currency("currency")?;
                        let (min, min_path) = condition.required("min")?;
                        let (max, max_path) = condition.required("max")?;
                        AmountTest::Between {
                            min: self.amount(min, &min_path, currency)?,
                            max: self.amount(max, &max_path, currency)?,
                        }
                    }
                    money_in_or_out => {
                        Object::new(value, path, &["field", "operator"])?;
                        if money_in_or_out == "money_in" {
                            AmountTest::MoneyIn
                        } else {
                            AmountTest::MoneyOut
                        }
                    }
                };
                Ok(Condition::Amount(test))
            }
            "booking_date" => {
                let condition = Object::new(value, path, &["field", "operator", "first", "last"])?;
                operator(&condition, &["between"])?;
                Ok(Condition::BookingDate {
                    first: condition.date("first")?,
                    last: condition.date("last")?,
                })
            }
            "currency" => {
                let condition = Object::new(value, path, &["field", "operator", "currency"])?;
                operator(&condition, &["is"])?;
                Ok(Condition::Currency(condition.currency("currency")?))
            }
            "account" => {
                let condition = Object::new(value, path, &["field", "operator", "account"])?;
                operator(&condition, &["is"])?;
                Ok(Condition::Account(AccountId::from_bytes(
                    condition.id("account")?,
                )))
            }
            _ => Err(bad(&format!("{path}.field"), Expected::Keyword)),
        }
    }
}

/// A JSON object of the file, with where it is, e.g.
/// `transactions[3].parts[0]`. Only the keys the format gives it may be in
/// it.
struct Object<'a> {
    map: &'a Map<String, Value>,
    path: String,
}

impl<'a> Object<'a> {
    /// `value` as an object, if it is one and has no key outside `keys`.
    fn new(value: &'a Value, path: &str, keys: &[&str]) -> Result<Self> {
        let map = value
            .as_object()
            .ok_or_else(|| bad(path_or_top(path), Expected::Object))?;
        if let Some(key) = map.keys().find(|key| !keys.contains(&key.as_str())) {
            return Err(ExportError::UnknownValue {
                path: join(path, key),
            });
        }
        Ok(Self {
            map,
            path: path.to_owned(),
        })
    }

    /// The value of `key`, and where it is. It must be there; whether it may
    /// be `null` is up to the caller.
    fn required(&self, key: &str) -> Result<(&'a Value, String)> {
        let path = join(&self.path, key);
        match self.map.get(key) {
            Some(value) => Ok((value, path)),
            None => Err(ExportError::MissingValue { path }),
        }
    }

    /// The value of `key`, unless it is left out or `null`.
    fn optional(&self, key: &str) -> Option<(&'a Value, String)> {
        self.map
            .get(key)
            .filter(|value| !value.is_null())
            .map(|value| (value, join(&self.path, key)))
    }

    fn object(&self, key: &str, keys: &[&str]) -> Result<Object<'a>> {
        let (value, path) = self.required(key)?;
        Object::new(value, &path, keys)
    }

    /// The entries of the list at `key`, each with where it is.
    fn list(&self, key: &str) -> Result<Vec<(&'a Value, String)>> {
        let (value, path) = self.required(key)?;
        let items = value.as_array().ok_or_else(|| bad(&path, Expected::List))?;
        Ok(items
            .iter()
            .enumerate()
            .map(|(index, item)| (item, format!("{path}[{index}]")))
            .collect())
    }

    fn text(&self, key: &str) -> Result<String> {
        let (value, path) = self.required(key)?;
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| bad(&path, Expected::Text))
    }

    fn optional_text(&self, key: &str) -> Result<Option<String>> {
        self.optional(key)
            .map(|(value, path)| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| bad(&path, Expected::Text))
            })
            .transpose()
    }

    fn boolean(&self, key: &str) -> Result<bool> {
        let (value, path) = self.required(key)?;
        value.as_bool().ok_or_else(|| bad(&path, Expected::YesOrNo))
    }

    fn id(&self, key: &str) -> Result<[u8; 16]> {
        let (value, path) = self.required(key)?;
        parse_id(value, &path)
    }

    fn optional_id(&self, key: &str) -> Result<Option<[u8; 16]>> {
        self.optional(key)
            .map(|(value, path)| parse_id(value, &path))
            .transpose()
    }

    fn date(&self, key: &str) -> Result<Date> {
        let (value, path) = self.required(key)?;
        parse_date(value, &path)
    }

    fn optional_date(&self, key: &str) -> Result<Option<Date>> {
        self.optional(key)
            .map(|(value, path)| parse_date(value, &path))
            .transpose()
    }

    fn currency(&self, key: &str) -> Result<CurrencyCode> {
        let (value, path) = self.required(key)?;
        value
            .as_str()
            .and_then(|text| CurrencyCode::new(text).ok())
            .ok_or_else(|| bad(&path, Expected::Currency))
    }

    /// One of the words in `table`, e.g. `asset` for an account's kind.
    fn keyword<T: Copy>(&self, key: &str, table: &[(T, &str)]) -> Result<T> {
        let (value, path) = self.required(key)?;
        parse_keyword(value, &path, table)
    }

    fn optional_keyword<T: Copy>(&self, key: &str, table: &[(T, &str)]) -> Result<Option<T>> {
        self.optional(key)
            .map(|(value, path)| parse_keyword(value, &path, table))
            .transpose()
    }
}

fn parse_keyword<T: Copy>(value: &Value, path: &str, table: &[(T, &str)]) -> Result<T> {
    value
        .as_str()
        .and_then(|word| table.iter().find(|(_, name)| *name == word))
        .map(|(value, _)| *value)
        .ok_or_else(|| bad(path, Expected::Keyword))
}

/// A UUID in its usual form with hyphens, e.g.
/// `0192f0a4-5b6c-7d8e-9f01-23456789abcd`, in small or capital letters.
fn parse_id(value: &Value, path: &str) -> Result<[u8; 16]> {
    value
        .as_str()
        .filter(|text| text.len() == 36)
        .and_then(|text| uuid::Uuid::try_parse(text).ok())
        .map(uuid::Uuid::into_bytes)
        .ok_or_else(|| bad(path, Expected::Id))
}

/// An ISO 8601 calendar date, exactly `YYYY-MM-DD` (D-061).
fn parse_date(value: &Value, path: &str) -> Result<Date> {
    value
        .as_str()
        .filter(|text| {
            let bytes = text.as_bytes();
            bytes.len() == 10
                && bytes.iter().enumerate().all(|(index, byte)| match index {
                    4 | 7 => *byte == b'-',
                    _ => byte.is_ascii_digit(),
                })
        })
        .and_then(|text| text.parse::<Date>().ok())
        .ok_or_else(|| bad(path, Expected::Date))
}

/// Whether `text` is digits, optionally followed by a point and more
/// digits, as amounts and rates are written: no sign, exponent or spaces.
fn is_plain_decimal(text: &str) -> bool {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, "0"));
    !whole.is_empty()
        && !fraction.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
}

/// The minor units of `text` in a currency with `digits` decimal places:
/// `-12.5` with 2 gives -1250. `None` if it isn't an amount as the format
/// writes them, has more decimal places, or doesn't fit into 64 bits.
pub(crate) fn parse_amount(text: &str, digits: u8) -> Option<i64> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if !is_plain_decimal(unsigned) {
        return None;
    }
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if fraction.len() > usize::from(digits) {
        return None;
    }
    // Digits of the minor units, as a magnitude that holds i64::MIN too.
    let mut magnitude: u64 = 0;
    let padding = usize::from(digits) - fraction.len();
    for byte in whole
        .bytes()
        .chain(fraction.bytes())
        .chain(std::iter::repeat_n(b'0', padding))
    {
        magnitude = magnitude
            .checked_mul(10)?
            .checked_add(u64::from(byte - b'0'))?;
    }
    if negative {
        0_i64.checked_sub_unsigned(magnitude)
    } else {
        i64::try_from(magnitude).ok()
    }
}

fn bad(path: &str, expected: Expected) -> ExportError {
    ExportError::BadValue {
        path: path.to_owned(),
        expected,
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// The whole file is named by an empty path; an error names it `.`.
fn path_or_top(path: &str) -> &str {
    if path.is_empty() { "." } else { path }
}
