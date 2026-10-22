// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Rules screens (M3.11, D-124): the list of Rules, the editor with its
//! conditions, "create a Rule from this transaction", and the suggestion to
//! make a Rule after the user sorted the same payee into the same category by
//! hand several times (D-071).
//!
//! The editor works on a [`RuleForm`], all of it text and flags (D-065): the
//! screen fills one in, this module turns it into a core [`Rule`] for the
//! book to check (D-088) and back. Amounts are typed without a sign next to
//! a direction, as Q-045 asks; the book keeps them signed. A Rule made
//! elsewhere, e.g. with the command-line tool, opens in the same editor
//! without losing anything: a range that spans zero keeps its signs.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, AccountKind, AmountTest, BookTo, Condition, CurrencyCode, Date, MatchMode,
    Money, Report, Rule, RuleFacts, RuleId, RuleNode, RuleSet, SplitId, TextField, TextOperator,
    Transaction, TransactionId,
};
use kassensturz_store::{Book, RuleMatch, StoreError};

use crate::accounts::usual_currency;
use crate::errors::{ErrorText, report_text};
use crate::format::Locale;
use crate::overview::LatestRow;
use crate::parse::{AmountInputError, AmountReader};
use crate::register::{AmountWriter, entries};
use crate::settings::Settings;
use crate::transaction_entry::typed_amount;

/// How often the user has to sort a payee into the same category by hand
/// before KassenSturz offers a Rule (D-071). Fixed here, not a setting.
pub const SUGGEST_AFTER: usize = 3;

/// Why something the user entered wasn't accepted. The UI shows a message for
/// each code (D-034).
#[derive(Debug)]
pub enum RuleEditError {
    /// An amount in a condition isn't one. `condition` counts from 0.
    Amount {
        /// The condition.
        condition: usize,
        /// What is wrong with it.
        error: AmountInputError,
        /// The currency it was read in.
        currency: CurrencyCode,
    },
    /// An amount in a condition is below zero; the direction says which way
    /// the money moves.
    NotPositive(usize),
    /// A day in a condition isn't a date.
    Date(usize),
    /// The currency in a condition is none KassenSturz lists.
    UnknownCurrency(usize),
    /// An account, category or Rule isn't in the book, e.g. it was just
    /// removed.
    NoSuchEntry,
    /// The book refused it, e.g. a name that is taken next to it.
    Store(StoreError),
}

impl From<StoreError> for RuleEditError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One condition of a Rule as the editor holds it. A field that doesn't
/// belong to the condition's `kind` is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConditionForm {
    /// `text`, `amount`, `date`, `currency` or `account`.
    pub kind: String,
    /// For `text`: `other-party-name`, `other-party-iban`, `purpose` or
    /// `payee`.
    pub field: String,
    /// For `text`: `contains`, `is`, `starts-with`, `ends-with` or
    /// `pattern`, the last one for experts (D-015).
    pub operator: String,
    /// For `text`: what to look for.
    pub value: String,
    /// For `text`: whether capitals count.
    pub case_sensitive: bool,
    /// For `amount`: `out` or `in`, which way the money moves, or `signed`
    /// for a range across zero made elsewhere, whose amounts keep their
    /// signs.
    pub direction: String,
    /// For `amount`: `any`, `exactly` or `between`.
    pub amount_test: String,
    /// For `amount`: the amount, or the first end of the range.
    pub amount: String,
    /// For `amount`: the other end of the range.
    pub amount_to: String,
    /// For `amount` and `currency`: the ISO 4217 code.
    pub currency: String,
    /// For `date`: the first day.
    pub first_date: String,
    /// For `date`: the last day.
    pub last_date: String,
    /// For `account`: the account's ID as text.
    pub account: String,
}

/// A Rule as the editor holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleForm {
    /// The Rule's ID as text; empty for a new Rule.
    pub id: String,
    /// The ID of the Rule a new Rule goes inside as text; empty for the
    /// top level. Ignored for a Rule that exists.
    pub parent: String,
    /// Its name.
    pub name: String,
    /// Whether all conditions have to hold, otherwise one is enough.
    pub match_all: bool,
    /// Its conditions.
    pub conditions: Vec<ConditionForm>,
    /// `category`, `transfer`, or empty for neither.
    pub book_to_kind: String,
    /// The ID of the category or account as text.
    pub book_to: String,
    /// The payee to set; empty to leave it.
    pub payee: String,
}

/// One Rule in the list.
#[derive(Debug, PartialEq, Eq)]
pub struct RuleRow {
    /// The Rule's ID as text.
    pub id: String,
    /// Its name.
    pub name: String,
    /// How deep it sits: 0 for a top-level Rule.
    pub depth: usize,
    /// Its place among the Rules next to it, from 0.
    pub position: usize,
    /// How many Rules are next to it, itself included.
    pub siblings: usize,
    /// `category`, `transfer`, or empty if it books nowhere.
    pub book_to_kind: &'static str,
    /// The category's path, e.g. "Food › Groceries", or the account's name.
    pub book_to: String,
    /// The payee it sets, empty for none.
    pub payee: String,
    /// How many conditions it has.
    pub conditions: usize,
}

/// A category or account the Rule can book to.
#[derive(Debug, PartialEq, Eq)]
pub struct Choice {
    /// Its ID as text.
    pub id: String,
    /// Its path, or the account's name.
    pub name: String,
    /// How deep a category sits, for indenting the list.
    pub depth: usize,
    /// Whether it is an income category; false for spending categories and
    /// accounts.
    pub income: bool,
}

/// What the editor offers to choose from.
#[derive(Debug, PartialEq, Eq)]
pub struct Choices {
    /// Every category, in tree order.
    pub categories: Vec<Choice>,
    /// The user's open accounts.
    pub accounts: Vec<Choice>,
    /// The currency amounts are in unless a condition says otherwise.
    pub currency: String,
}

/// A payee the user sorted into the same category by hand several times.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// The payee, as it was last typed.
    pub payee: String,
    /// The category's ID as text.
    pub category: String,
    /// The category's path.
    pub category_path: String,
    /// How many times.
    pub count: usize,
}

/// What changing a Rule did or would do to what it booked earlier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rebooked {
    /// How many transactions are booked differently.
    pub changed: usize,
    /// How many couldn't be, e.g. for lack of an exchange rate.
    pub skipped: usize,
}

/// A Rule and its place in the tree.
struct Placed<'a> {
    node: &'a Rule,
    parent: Option<RuleId>,
    depth: usize,
    position: usize,
    siblings: usize,
}

fn place<'a>(
    nodes: &'a [RuleNode],
    parent: Option<RuleId>,
    depth: usize,
    into: &mut Vec<Placed<'a>>,
) {
    for (position, node) in nodes.iter().enumerate() {
        into.push(Placed {
            node: &node.rule,
            parent,
            depth,
            position,
            siblings: nodes.len(),
        });
        place(&node.sub_rules, Some(node.rule.id), depth + 1, into);
    }
}

/// Every Rule of the book in list order: each Rule before its sub-Rules.
fn placed(tree: &[RuleNode]) -> Vec<Placed<'_>> {
    let mut all = Vec::new();
    place(tree, None, 0, &mut all);
    all
}

/// The category paths by ID.
fn category_paths(book: &Book) -> Result<HashMap<AccountId, String>, StoreError> {
    Ok(book
        .categories()?
        .into_iter()
        .map(|entry| (entry.category.id, entry.path.join(" › ")))
        .collect())
}

/// The Rules of `book` in list order.
pub fn view(book: &Book) -> Result<Vec<RuleRow>, StoreError> {
    let tree = book.rules()?;
    let paths = category_paths(book)?;
    let accounts: HashMap<AccountId, Account> = book
        .all_accounts()?
        .into_iter()
        .map(|account| (account.id, account))
        .collect();
    Ok(placed(&tree)
        .into_iter()
        .map(|placed| {
            let rule = placed.node;
            let (book_to_kind, book_to) = match rule.actions.book_to {
                Some(BookTo::Category(id)) => ("category", paths.get(&id).cloned()),
                Some(BookTo::Transfer(id)) => (
                    "transfer",
                    accounts.get(&id).map(|account| account.name.clone()),
                ),
                None => ("", None),
            };
            RuleRow {
                id: rule.id.to_string(),
                name: rule.name.clone(),
                depth: placed.depth,
                position: placed.position,
                siblings: placed.siblings,
                book_to_kind,
                book_to: book_to.unwrap_or_default(),
                payee: rule.actions.payee.clone().unwrap_or_default(),
                conditions: rule.conditions.len(),
            }
        })
        .collect())
}

/// What the editor offers: every category, the open accounts, and the
/// currency amounts count in by default.
pub fn choices(book: &Book, fallback: CurrencyCode) -> Result<Choices, StoreError> {
    let categories = book
        .categories()?
        .into_iter()
        .map(|entry| Choice {
            id: entry.category.id.to_string(),
            name: entry.path.join(" › "),
            depth: entry.path.len() - 1,
            income: entry.category.kind == AccountKind::Income,
        })
        .collect();
    let accounts = book
        .all_accounts()?
        .into_iter()
        .filter(|account| account.kind.is_account() && !account.closed)
        .map(|account| Choice {
            id: account.id.to_string(),
            name: account.name,
            depth: 0,
            income: false,
        })
        .collect();
    Ok(Choices {
        categories,
        accounts,
        currency: usual_currency(book, fallback)?.to_string(),
    })
}

/// An empty form for a new Rule inside `parent` (an ID, empty for the top
/// level) with one text condition to fill in.
pub fn new_form(parent: &str) -> RuleForm {
    RuleForm {
        parent: parent.to_owned(),
        match_all: true,
        conditions: vec![text_condition("other-party-name", "contains", "")],
        ..RuleForm::default()
    }
}

fn text_condition(field: &str, operator: &str, value: &str) -> ConditionForm {
    ConditionForm {
        kind: "text".into(),
        field: field.into(),
        operator: operator.into(),
        value: value.into(),
        ..ConditionForm::default()
    }
}

const fn field_key(field: TextField) -> &'static str {
    match field {
        TextField::OtherPartyName => "other-party-name",
        TextField::OtherPartyIban => "other-party-iban",
        TextField::RemittanceInformation => "purpose",
        TextField::Payee => "payee",
    }
}

fn field_of(key: &str) -> Option<TextField> {
    TextField::ALL.into_iter().find(|&f| field_key(f) == key)
}

const fn operator_key(operator: TextOperator) -> &'static str {
    match operator {
        TextOperator::Contains => "contains",
        TextOperator::Equals => "is",
        TextOperator::StartsWith => "starts-with",
        TextOperator::EndsWith => "ends-with",
        TextOperator::Matches => "pattern",
    }
}

fn operator_of(key: &str) -> Option<TextOperator> {
    TextOperator::ALL
        .into_iter()
        .find(|&o| operator_key(o) == key)
}

/// A stored Rule as the editor holds it.
pub fn form_of(book: &Book, id: &str, locale: &impl Locale) -> Result<RuleForm, RuleEditError> {
    let tree = book.rules()?;
    let all = placed(&tree);
    let found = all
        .iter()
        .find(|placed| placed.node.id.to_string() == id)
        .ok_or(RuleEditError::NoSuchEntry)?;
    let rule = found.node;
    let mut conditions = Vec::new();
    for condition in &rule.conditions {
        conditions.push(condition_form(book, condition, locale)?);
    }
    let (book_to_kind, book_to) = match rule.actions.book_to {
        Some(BookTo::Category(id)) => ("category", id.to_string()),
        Some(BookTo::Transfer(id)) => ("transfer", id.to_string()),
        None => ("", String::new()),
    };
    Ok(RuleForm {
        id: rule.id.to_string(),
        parent: found.parent.map(|id| id.to_string()).unwrap_or_default(),
        name: rule.name.clone(),
        match_all: rule.match_mode == MatchMode::All,
        conditions,
        book_to_kind: book_to_kind.into(),
        book_to,
        payee: rule.actions.payee.clone().unwrap_or_default(),
    })
}

fn condition_form(
    book: &Book,
    condition: &Condition,
    locale: &impl Locale,
) -> Result<ConditionForm, StoreError> {
    let empty = ConditionForm::default;
    Ok(match condition {
        Condition::Text {
            field,
            operator,
            value,
            case_sensitive,
        } => ConditionForm {
            case_sensitive: *case_sensitive,
            ..text_condition(field_key(*field), operator_key(*operator), value)
        },
        Condition::Amount(test) => {
            let unsigned = |money: Money| match money.checked_neg() {
                Ok(positive) if money.is_negative() => typed_amount(book, positive, locale),
                _ => typed_amount(book, money, locale),
            };
            // Money going out is typed without its minus sign (Q-045); only
            // a range across zero keeps the signs, which tell the ends apart.
            let amount = |money: Money| {
                unsigned(money).map(|text| {
                    if money.is_negative() {
                        format!("-{text}")
                    } else {
                        text
                    }
                })
            };
            let form = ConditionForm {
                kind: "amount".into(),
                ..empty()
            };
            match *test {
                AmountTest::MoneyIn => ConditionForm {
                    direction: "in".into(),
                    amount_test: "any".into(),
                    ..form
                },
                AmountTest::MoneyOut => ConditionForm {
                    direction: "out".into(),
                    amount_test: "any".into(),
                    ..form
                },
                AmountTest::Equals(money) => ConditionForm {
                    direction: if money.is_negative() { "out" } else { "in" }.into(),
                    amount_test: "exactly".into(),
                    amount: unsigned(money)?,
                    currency: money.currency().to_string(),
                    ..form
                },
                AmountTest::Between { min, max } if !min.is_negative() => ConditionForm {
                    direction: "in".into(),
                    amount_test: "between".into(),
                    amount: unsigned(min)?,
                    amount_to: unsigned(max)?,
                    currency: min.currency().to_string(),
                    ..form
                },
                AmountTest::Between { min, max } if max.minor_units() <= 0 => ConditionForm {
                    direction: "out".into(),
                    amount_test: "between".into(),
                    // Typed lowest first: −50 to −10 is 10 to 50.
                    amount: unsigned(max)?,
                    amount_to: unsigned(min)?,
                    currency: min.currency().to_string(),
                    ..form
                },
                AmountTest::Between { min, max } => ConditionForm {
                    direction: "signed".into(),
                    amount_test: "between".into(),
                    amount: amount(min)?,
                    amount_to: amount(max)?,
                    currency: min.currency().to_string(),
                    ..form
                },
            }
        }
        Condition::BookingDate { first, last } => ConditionForm {
            kind: "date".into(),
            first_date: locale.short_date(*first),
            last_date: locale.short_date(*last),
            ..empty()
        },
        Condition::Currency(currency) => ConditionForm {
            kind: "currency".into(),
            currency: currency.to_string(),
            ..empty()
        },
        Condition::Account(account) => ConditionForm {
            kind: "account".into(),
            account: account.to_string(),
            ..empty()
        },
    })
}

/// The Rule the form describes, and the Rule it goes inside, for the book to
/// check (D-088). A new Rule gets a new ID.
fn rule_of(
    book: &Book,
    form: &RuleForm,
    locale: &impl Locale,
) -> Result<(Rule, Option<RuleId>), RuleEditError> {
    let tree = book.rules()?;
    let all = placed(&tree);
    let rule_id = |text: &str| {
        all.iter()
            .find(|placed| placed.node.id.to_string() == text)
            .map(|placed| placed.node.id)
            .ok_or(RuleEditError::NoSuchEntry)
    };
    let (id, parent) = if form.id.is_empty() {
        let parent = if form.parent.is_empty() {
            None
        } else {
            Some(rule_id(&form.parent)?)
        };
        (RuleId::generate(), parent)
    } else {
        let id = rule_id(&form.id)?;
        let parent = all
            .iter()
            .find(|placed| placed.node.id == id)
            .and_then(|placed| placed.parent);
        (id, parent)
    };
    let accounts = book.all_accounts()?;
    let find = |text: &str, wanted: fn(&Account) -> bool| {
        accounts
            .iter()
            .find(|account| wanted(account) && account.id.to_string() == text)
            .map(|account| account.id)
            .ok_or(RuleEditError::NoSuchEntry)
    };
    let mut conditions = Vec::new();
    for (at, condition) in form.conditions.iter().enumerate() {
        conditions.push(read_condition(book, at, condition, &accounts, locale)?);
    }
    let book_to = match form.book_to_kind.as_str() {
        "category" => Some(BookTo::Category(find(&form.book_to, |a| {
            a.kind.is_category()
        })?)),
        "transfer" => Some(BookTo::Transfer(find(&form.book_to, |a| {
            a.kind.is_account()
        })?)),
        _ => None,
    };
    let payee = form.payee.trim();
    Ok((
        Rule {
            id,
            name: form.name.trim().to_owned(),
            match_mode: if form.match_all {
                MatchMode::All
            } else {
                MatchMode::Any
            },
            conditions,
            actions: kassensturz_core::Actions {
                book_to,
                payee: (!payee.is_empty()).then(|| payee.to_owned()),
            },
        },
        parent,
    ))
}

fn read_condition(
    book: &Book,
    at: usize,
    form: &ConditionForm,
    accounts: &[Account],
    locale: &impl Locale,
) -> Result<Condition, RuleEditError> {
    match form.kind.as_str() {
        "text" => Ok(Condition::Text {
            field: field_of(&form.field).ok_or(RuleEditError::NoSuchEntry)?,
            operator: operator_of(&form.operator).ok_or(RuleEditError::NoSuchEntry)?,
            value: form.value.clone(),
            case_sensitive: form.case_sensitive,
        }),
        "amount" => read_amount_test(book, at, form, locale).map(Condition::Amount),
        "date" => {
            let day = |text: &str| locale.read_date(text).ok_or(RuleEditError::Date(at));
            Ok(Condition::BookingDate {
                first: day(&form.first_date)?,
                last: day(&form.last_date)?,
            })
        }
        "currency" => Ok(Condition::Currency(read_currency(at, &form.currency)?)),
        "account" => accounts
            .iter()
            .find(|account| account.kind.is_account() && account.id.to_string() == form.account)
            .map(|account| Condition::Account(account.id))
            .ok_or(RuleEditError::NoSuchEntry),
        _ => Err(RuleEditError::NoSuchEntry),
    }
}

fn read_currency(at: usize, text: &str) -> Result<CurrencyCode, RuleEditError> {
    CurrencyCode::new(text.trim())
        .ok()
        .filter(|currency| currency.iso_minor_digits().is_some())
        .ok_or(RuleEditError::UnknownCurrency(at))
}

/// Reads the amounts of an `amount` condition: typed without a sign next to
/// the direction (Q-045), then signed as the book keeps them.
fn read_amount_test(
    book: &Book,
    at: usize,
    form: &ConditionForm,
    locale: &impl Locale,
) -> Result<AmountTest, RuleEditError> {
    let out = form.direction == "out";
    if form.amount_test == "any" {
        return Ok(if out {
            AmountTest::MoneyOut
        } else {
            AmountTest::MoneyIn
        });
    }
    let currency = read_currency(at, &form.currency)?;
    let minor_digits = book.currency_minor_digits(currency)?;
    let format = locale.amount_format(currency);
    let group_separator = locale.group_separator();
    let reader = AmountReader {
        format: &format,
        group_separator: &group_separator,
        currency,
        minor_digits,
    };
    let signed = form.direction == "signed";
    let read = |text: &str| -> Result<Money, RuleEditError> {
        let amount = reader.read(text).map_err(|error| RuleEditError::Amount {
            condition: at,
            error,
            currency,
        })?;
        if signed {
            return Ok(amount);
        }
        if amount.is_negative() {
            return Err(RuleEditError::NotPositive(at));
        }
        if out {
            amount.checked_neg().map_err(|_| RuleEditError::Amount {
                condition: at,
                error: AmountInputError::TooLarge,
                currency,
            })
        } else {
            Ok(amount)
        }
    };
    let first = read(&form.amount)?;
    if form.amount_test == "exactly" {
        return Ok(AmountTest::Equals(first));
    }
    let second = read(&form.amount_to)?;
    // Typed in either order: the range runs from the lower to the higher.
    let (min, max) = if first.minor_units() <= second.minor_units() {
        (first, second)
    } else {
        (second, first)
    };
    Ok(AmountTest::Between { min, max })
}

/// Adds the Rule the form describes, at the end of its list.
pub fn add_rule(
    book: &mut Book,
    form: &RuleForm,
    locale: &impl Locale,
) -> Result<RuleId, RuleEditError> {
    let (rule, parent) = rule_of(book, form, locale)?;
    book.add_rule(&rule, parent)?;
    Ok(rule.id)
}

/// Changes a Rule to what the form describes. With `rebook`, the
/// transactions it and its sub-Rules booked earlier are booked again in the
/// same action (Q-044); otherwise they stay as they are.
pub fn change_rule(
    book: &mut Book,
    form: &RuleForm,
    rebook: bool,
    locale: &impl Locale,
) -> Result<Rebooked, RuleEditError> {
    let (rule, _) = rule_of(book, form, locale)?;
    if rebook {
        let run = book.update_rule_and_rebook(&rule)?;
        Ok(Rebooked {
            changed: run.changed.len(),
            skipped: run.skipped.len(),
        })
    } else {
        book.update_rule(&rule)?;
        Ok(Rebooked::default())
    }
}

/// What changing the Rule to what the form describes would do to what it
/// booked earlier, without changing anything.
pub fn preview_rebook(
    book: &Book,
    form: &RuleForm,
    locale: &impl Locale,
) -> Result<Rebooked, RuleEditError> {
    let (rule, _) = rule_of(book, form, locale)?;
    let run = book.preview_rebook(&rule)?;
    Ok(Rebooked {
        changed: run.changed.len(),
        skipped: run.skipped.len(),
    })
}

/// The transactions the Rule with this ID matches as the book is now,
/// newest first, as the register of the account each is in shows them: every
/// transaction it decides, and those its sub-Rules decide, whether or not a
/// Rule booked them (D-131). A Rule that comes after another one that fits a
/// transaction doesn't match it, because the first Rule that fits decides.
pub fn matches(
    book: &Book,
    id: &str,
    locale: &impl Locale,
) -> Result<Vec<LatestRow>, RuleEditError> {
    let id = find_rule(book, id)?;
    let found = book
        .rule_matches()?
        .into_iter()
        .filter(|found| found.rules.contains(&id))
        .collect();
    Ok(match_rows(book, found, locale)?)
}

/// The transactions the Rule the form describes fits on its own, as
/// [`matches`] lists them, without the Rule being in the book: what the
/// editor shows while the user types. The Rules before it and its sub-Rules
/// aren't looked at (D-132).
pub fn preview_matches(
    book: &Book,
    form: &RuleForm,
    locale: &impl Locale,
) -> Result<Vec<LatestRow>, RuleEditError> {
    let (rule, _) = rule_of(book, form, locale)?;
    let found = book.rule_preview(&rule)?;
    Ok(match_rows(book, found, locale)?)
}

/// `found` as the register of the account each is in shows them, newest
/// first.
fn match_rows(
    book: &Book,
    found: Vec<RuleMatch>,
    locale: &impl Locale,
) -> Result<Vec<LatestRow>, StoreError> {
    let mut wanted: HashMap<AccountId, HashSet<TransactionId>> = HashMap::new();
    for found in found {
        wanted
            .entry(found.account)
            .or_default()
            .insert(found.transaction);
    }
    let accounts: HashMap<AccountId, Account> = book
        .all_accounts()?
        .into_iter()
        .map(|account| (account.id, account))
        .collect();
    let mut rows = Vec::new();
    for (account_id, transactions) in wanted {
        let Some(account) = accounts.get(&account_id) else {
            continue;
        };
        let writer = AmountWriter::new(book, account.currency, locale)?;
        for entry in entries(book, account_id)? {
            if transactions.contains(&entry.transaction) {
                let row = entry.row(&writer, account.kind);
                rows.push((
                    entry.date,
                    entry.transaction,
                    LatestRow {
                        account_id: account.id.to_string(),
                        account: account.name.clone(),
                        id: row.id,
                        date: row.date,
                        other_party: row.other_party,
                        category: row.category,
                        amount: row.amount,
                        incoming: row.incoming,
                        kind: row.kind,
                    },
                ));
            }
        }
    }
    rows.sort_by_key(|(date, transaction, _)| core::cmp::Reverse((*date, *transaction)));
    Ok(rows.into_iter().map(|(_, _, row)| row).collect())
}

/// Removes a Rule with its sub-Rules. What it booked keeps its booking.
pub fn remove_rule(book: &mut Book, id: &str) -> Result<(), RuleEditError> {
    let id = find_rule(book, id)?;
    Ok(book.remove_rule(id)?)
}

/// Moves a Rule one place up or down among the Rules next to it. The first
/// Rule that matches decides (D-031), so the order matters. A Rule at the
/// end of its list stays.
pub fn move_rule(book: &mut Book, id: &str, up: bool) -> Result<(), RuleEditError> {
    let tree = book.rules()?;
    let all = placed(&tree);
    let found = all
        .iter()
        .find(|placed| placed.node.id.to_string() == id)
        .ok_or(RuleEditError::NoSuchEntry)?;
    let position = if up {
        found.position.saturating_sub(1)
    } else {
        found.position + 1
    };
    if position == found.position || position >= found.siblings {
        return Ok(());
    }
    let (rule, parent) = (found.node.id, found.parent);
    Ok(book.move_rule(rule, parent, position)?)
}

fn find_rule(book: &Book, id: &str) -> Result<RuleId, RuleEditError> {
    let tree = book.rules()?;
    placed(&tree)
        .iter()
        .find(|placed| placed.node.id.to_string() == id)
        .map(|placed| placed.node.id)
        .ok_or(RuleEditError::NoSuchEntry)
}

/// A Rule to start from for a transaction the user opened in `account`
/// (D-088): its name, what the bank delivered or the payee as the condition,
/// and the category it was sorted into. The user adjusts it in the editor.
pub fn form_from_transaction(
    book: &Book,
    account: &str,
    transaction: &str,
) -> Result<RuleForm, RuleEditError> {
    let transaction = find_transaction(book, account, transaction)?;
    let own = transaction
        .splits()
        .iter()
        .find(|split| split.account.is_some_and(|id| id.to_string() == account))
        .ok_or(RuleEditError::NoSuchEntry)?;
    let others: Vec<_> = transaction
        .splits()
        .iter()
        .filter(|split| split.id != own.id)
        .collect();
    let payee = transaction.payee_of(own).map(str::trim).unwrap_or_default();
    let details = own.imported.as_ref();
    let from_bank = |get: fn(&kassensturz_core::ImportedDetails) -> &Option<String>| {
        details
            .and_then(|details| get(details).as_deref())
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    let condition = if let Some(name) = from_bank(|d| &d.other_party_name) {
        text_condition("other-party-name", "contains", name)
    } else if !payee.is_empty() {
        text_condition("payee", "is", payee)
    } else if let Some(purpose) = from_bank(|d| &d.remittance_information) {
        text_condition("purpose", "contains", purpose)
    } else {
        text_condition("other-party-name", "contains", "")
    };
    let category = match others.as_slice() {
        [other] => other
            .account
            .map(|id| book.account(id))
            .transpose()?
            .filter(|other| other.kind.is_category()),
        _ => None,
    };
    let name = if payee.is_empty() {
        condition.value.chars().take(40).collect()
    } else {
        payee.to_owned()
    };
    Ok(RuleForm {
        name,
        match_all: true,
        conditions: vec![condition],
        book_to_kind: if category.is_some() { "category" } else { "" }.into(),
        book_to: category.map(|c| c.id.to_string()).unwrap_or_default(),
        ..RuleForm::default()
    })
}

fn find_transaction(
    book: &Book,
    account: &str,
    transaction: &str,
) -> Result<Transaction, RuleEditError> {
    book.transactions()?
        .into_iter()
        .find(|found| {
            found.id().to_string() == transaction
                && found
                    .splits()
                    .iter()
                    .any(|split| split.account.is_some_and(|id| id.to_string() == account))
        })
        .ok_or(RuleEditError::NoSuchEntry)
}

/// A Rule to start from for a suggestion: the payee, whatever the capitals,
/// booked to the category.
pub fn form_from_suggestion(
    book: &Book,
    payee: &str,
    category: &str,
) -> Result<RuleForm, RuleEditError> {
    let category = book
        .categories()?
        .into_iter()
        .find(|entry| entry.category.id.to_string() == category)
        .ok_or(RuleEditError::NoSuchEntry)?;
    Ok(RuleForm {
        name: payee.trim().to_owned(),
        match_all: true,
        conditions: vec![text_condition("payee", "is", payee.trim())],
        book_to_kind: "category".into(),
        book_to: category.category.id.to_string(),
        ..RuleForm::default()
    })
}

/// How a dismissed suggestion is remembered: its category and its payee
/// without capitals, so a payee typed another way is the same one.
pub fn dismissal_key(payee: &str, category: &str) -> String {
    let payee: String = payee
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!("{category}\t{payee}")
}

/// The settings group that remembers which suggestions the user turned down.
const SUGGESTIONS: &str = "RuleSuggestions";
/// How many turned-down suggestions are remembered; the oldest are forgotten
/// first.
const REMEMBERED: usize = 200;

/// The suggestions the user turned down, as [`dismissal_key`]s. They are
/// kept in the app's settings, not in the book: nothing about the book's
/// layout changes for a "no thanks" (D-124).
pub fn dismissed(settings: &Settings) -> Vec<String> {
    settings
        .text(SUGGESTIONS, "Dismissed")
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Remembers that the user doesn't want a Rule for `payee` and `category`.
/// Returns whether it was saved.
pub fn dismiss(settings: &Settings, payee: &str, category: &str) -> bool {
    let mut all = dismissed(settings);
    let key = dismissal_key(payee, category);
    if !all.contains(&key) {
        all.push(key);
    }
    let start = all.len().saturating_sub(REMEMBERED);
    settings.set_text(SUGGESTIONS, "Dismissed", &all[start..].join("\n"))
}

/// A transaction the user sorted by hand: money moving between one of their
/// accounts and one category, that no Rule booked.
struct Sorted<'a> {
    id: TransactionId,
    date: Date,
    payee: &'a str,
    category: AccountId,
    facts: RuleFacts<'a>,
}

/// How often a payee was sorted into each category by hand.
struct PayeeCount {
    /// The payee without capitals.
    name: String,
    /// Each category and how many times.
    categories: Vec<(AccountId, usize)>,
    /// The place of its newest transaction in the list of sorted ones.
    newest: usize,
}

/// The payees the user sorted into the same category by hand at least
/// [`SUGGEST_AFTER`] times, more often than into any other (D-071), unless
/// the Rules already decide for them or the user said no. Plain code, so it
/// works with AI off. Most often first.
pub fn suggestions(book: &Book, dismissed: &[String]) -> Result<Vec<Suggestion>, StoreError> {
    let accounts: HashMap<AccountId, Account> = book
        .all_accounts()?
        .into_iter()
        .map(|account| (account.id, account))
        .collect();
    let paths = category_paths(book)?;
    let booked_by_rules: HashSet<SplitId> = book
        .rule_marks()?
        .into_iter()
        .map(|mark| mark.split)
        .collect();
    let transactions = book.transactions()?;
    let rules = RuleSet::new(&book.rules()?).map_err(StoreError::Rule)?;

    let mut sorted: Vec<Sorted<'_>> = Vec::new();
    for transaction in &transactions {
        let [first, second] = transaction.splits() else {
            continue;
        };
        if booked_by_rules.contains(&first.id) || booked_by_rules.contains(&second.id) {
            continue;
        }
        let is_account = |id: Option<AccountId>| {
            id.and_then(|id| accounts.get(&id))
                .is_some_and(|account| account.kind.is_account())
        };
        let (own, category) = match (first.account, second.account) {
            (Some(category), _) if is_account(second.account) && paths.contains_key(&category) => {
                (second, category)
            }
            (_, Some(category)) if is_account(first.account) && paths.contains_key(&category) => {
                (first, category)
            }
            _ => continue,
        };
        let Some(account) = own.account else { continue };
        let Some(payee) = transaction
            .payee_of(own)
            .map(str::trim)
            .filter(|payee| !payee.is_empty())
        else {
            continue;
        };
        if own.foreign.is_some() {
            continue;
        }
        sorted.push(Sorted {
            id: transaction.id(),
            date: transaction.date_of(own),
            payee,
            category,
            facts: RuleFacts {
                account,
                amount: own.amount,
                booking_date: transaction.date_of(own),
                payee: Some(payee),
                imported: own.imported.as_ref(),
            },
        });
    }
    // The newest first, so a payee is shown the way it was typed last.
    sorted.sort_by_key(|found| core::cmp::Reverse((found.date, found.id)));

    // Per payee (ignoring capitals), how often each category.
    let mut payees: Vec<PayeeCount> = Vec::new();
    for (at, found) in sorted.iter().enumerate() {
        let name = found.payee.to_lowercase();
        let entry = match payees.iter().position(|seen| seen.name == name) {
            Some(index) => &mut payees[index],
            None => {
                payees.push(PayeeCount {
                    name,
                    categories: Vec::new(),
                    newest: at,
                });
                payees.last_mut().expect("just pushed")
            }
        };
        match entry
            .categories
            .iter_mut()
            .find(|(category, _)| *category == found.category)
        {
            Some((_, count)) => *count += 1,
            None => entry.categories.push((found.category, 1)),
        }
    }

    let mut found = Vec::new();
    for payee in payees {
        let mut ranked = payee.categories;
        ranked.sort_by_key(|&(_, count)| core::cmp::Reverse(count));
        let (category, count) = ranked[0];
        let tied = ranked.get(1).is_some_and(|&(_, other)| other == count);
        if count < SUGGEST_AFTER || tied {
            continue;
        }
        let latest = &sorted[payee.newest];
        let payee = latest.payee.to_owned();
        let category_id = category.to_string();
        if dismissed.contains(&dismissal_key(&payee, &category_id)) {
            continue;
        }
        // A Rule that matches decides, so another one would never be used.
        if rules.evaluate(&latest.facts).is_some() {
            continue;
        }
        found.push(Suggestion {
            payee,
            category: category_id,
            category_path: paths.get(&category).cloned().unwrap_or_default(),
            count,
        });
    }
    found.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.payee.to_lowercase().cmp(&b.payee.to_lowercase()))
    });
    Ok(found)
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &RuleEditError, locale: &impl Locale) -> ErrorText {
    let numbered = |mut text: ErrorText, condition: usize| {
        text.params.push(("condition", (condition + 1).to_string()));
        text
    };
    match error {
        RuleEditError::Amount {
            condition,
            error,
            currency,
        } => numbered(
            ErrorText::amount(*error, book, *currency, locale),
            *condition,
        ),
        RuleEditError::NotPositive(condition) => {
            numbered(ErrorText::plain("input.amount-not-positive"), *condition)
        }
        RuleEditError::Date(condition) => numbered(
            ErrorText {
                code: "input.malformed-date",
                params: vec![("example", locale.short_date(locale.today()))],
            },
            *condition,
        ),
        RuleEditError::UnknownCurrency(condition) => {
            numbered(ErrorText::plain("input.unknown-currency"), *condition)
        }
        RuleEditError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        RuleEditError::Store(error) => report_text(book, &error.report(), locale),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kassensturz_core::AccountType;
    use kassensturz_import::{Entry, Statement, StatementAccount};

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::statement_import::{Loaded, import};
    use crate::test_folder::TestFolder;
    use crate::transaction_entry::{self, NewEntry};

    fn checking(book: &mut Book) -> String {
        add_account(
            book,
            &NewAccount {
                name: "Checking account",
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "1000",
                date: "2026-09-01",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string()
    }

    fn category(book: &Book, path: &str) -> String {
        book.categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.path.join(" › ") == path)
            .expect("category")
            .category
            .id
            .to_string()
    }

    /// A payment typed by hand; `category` is a category's ID, empty for none.
    fn typed(
        book: &mut Book,
        account: &str,
        other_party: &str,
        cents: &str,
        category: &str,
        date: &str,
    ) -> String {
        transaction_entry::add(
            book,
            &NewEntry {
                account,
                money_in: false,
                amount: cents,
                other_party,
                category,
                date,
            },
            &TestLocale,
        )
        .expect("transaction added")
        .to_string()
    }

    fn text_form(name: &str, field: &str, operator: &str, value: &str, to: &str) -> RuleForm {
        RuleForm {
            name: name.into(),
            match_all: true,
            conditions: vec![text_condition(field, operator, value)],
            book_to_kind: "category".into(),
            book_to: to.into(),
            ..RuleForm::default()
        }
    }

    fn add(book: &mut Book, form: &RuleForm) -> String {
        add_rule(book, form, &TestLocale)
            .expect("Rule added")
            .to_string()
    }

    fn code(book: &Book, error: &RuleEditError) -> ErrorText {
        error_text(book, error, &TestLocale)
    }

    fn refused(book: &mut Book, form: &RuleForm) -> ErrorText {
        let error = add_rule(book, form, &TestLocale).expect_err("refused");
        code(book, &error)
    }

    fn amount_form(direction: &str, test: &str, from: &str, to: &str) -> RuleForm {
        RuleForm {
            name: "Amounts".into(),
            match_all: true,
            conditions: vec![ConditionForm {
                kind: "amount".into(),
                direction: direction.into(),
                amount_test: test.into(),
                amount: from.into(),
                amount_to: to.into(),
                currency: "EUR".into(),
                ..ConditionForm::default()
            }],
            ..RuleForm::default()
        }
    }

    fn statement(entries: &[(i8, i64, &str)]) -> Vec<Statement> {
        let eur = kassensturz_core::Currency::new(CurrencyCode::new("EUR").expect("code"), 2)
            .expect("places");
        vec![Statement {
            account: StatementAccount::default(),
            currency: eur,
            opening_balance: None,
            closing_balance: None,
            entries: entries
                .iter()
                .map(|&(day, cents, party)| {
                    let mut entry = Entry::booked(
                        Date::new(2026, 10, day).expect("a date"),
                        Money::from_minor_units(cents, eur.code()),
                    );
                    entry.other_party_name = Some(party.into());
                    entry
                })
                .collect(),
        }]
    }

    fn category_of(book: &Book, transaction: &str) -> Option<String> {
        let found = book
            .transactions()
            .expect("transactions")
            .into_iter()
            .find(|found| found.id().to_string() == transaction)
            .expect("transaction");
        found
            .splits()
            .iter()
            .filter_map(|split| split.account)
            .find_map(|id| {
                let account = book.account(id).expect("account");
                account.kind.is_category().then_some(account.name)
            })
    }

    #[test]
    fn lists_the_rules_in_order_with_what_they_do() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let account = checking(&mut book);
        let shop = add(
            &mut book,
            &text_form("Shop", "other-party-name", "contains", "shop", &groceries),
        );
        let mut to_cash = text_form("Cash", "purpose", "starts-with", "ATM", "");
        to_cash.book_to_kind = "transfer".into();
        to_cash.book_to = account.clone();
        add(&mut book, &to_cash);
        let mut inside = text_form("Bakery", "payee", "is", "Bakery", "");
        inside.parent = shop.clone();
        inside.book_to_kind = String::new();
        inside.payee = " Fresh bread ".into();
        add(&mut book, &inside);

        let rows = view(&book).expect("view");
        let shown: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    row.name.as_str(),
                    row.depth,
                    (row.position, row.siblings),
                    row.book_to_kind,
                    row.book_to.as_str(),
                    row.payee.as_str(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("Shop", 0, (0, 2), "category", "Food › Groceries", ""),
                ("Bakery", 1, (0, 1), "", "", "Fresh bread"),
                ("Cash", 0, (1, 2), "transfer", "Checking account", ""),
            ]
        );
        assert_eq!(rows[0].id, shop);
        assert!(rows.iter().all(|row| row.conditions == 1));
    }

    #[test]
    fn a_form_round_trips_every_kind_of_condition() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let account = checking(&mut book);
        let condition = |kind: &str| ConditionForm {
            kind: kind.into(),
            ..ConditionForm::default()
        };
        let form = RuleForm {
            name: "Everything".into(),
            match_all: false,
            conditions: vec![
                ConditionForm {
                    case_sensitive: true,
                    ..text_condition("other-party-iban", "ends-with", "0123")
                },
                text_condition("purpose", "pattern", "^Rent \\d+$"),
                ConditionForm {
                    direction: "out".into(),
                    amount_test: "exactly".into(),
                    amount: "9,99".into(),
                    currency: "EUR".into(),
                    ..condition("amount")
                },
                ConditionForm {
                    direction: "in".into(),
                    amount_test: "between".into(),
                    amount: "10,00".into(),
                    amount_to: "50,50".into(),
                    currency: "EUR".into(),
                    ..condition("amount")
                },
                ConditionForm {
                    direction: "out".into(),
                    amount_test: "any".into(),
                    ..condition("amount")
                },
                ConditionForm {
                    first_date: "2026-01-01".into(),
                    last_date: "2026-12-31".into(),
                    ..condition("date")
                },
                ConditionForm {
                    currency: "EUR".into(),
                    ..condition("currency")
                },
                ConditionForm {
                    account: account.clone(),
                    ..condition("account")
                },
            ],
            book_to_kind: "category".into(),
            book_to: groceries,
            payee: "Everything shop".into(),
            ..RuleForm::default()
        };
        let id = add(&mut book, &form);
        let read = form_of(&book, &id, &TestLocale).expect("form");
        assert_eq!(read, RuleForm { id, ..form });
    }

    #[test]
    fn amounts_are_typed_without_a_sign_and_kept_signed() {
        let mut book = started();
        checking(&mut book);
        let eur = CurrencyCode::new("EUR").expect("code");
        let money = |cents| Money::from_minor_units(cents, eur);
        let tests = |book: &Book| -> Vec<AmountTest> {
            let mut found = Vec::new();
            for node in book.rules().expect("rules") {
                for condition in &node.rule.conditions {
                    if let Condition::Amount(test) = condition {
                        found.push(*test);
                    }
                }
            }
            found
        };
        let next = |book: &mut Book, form: RuleForm| {
            let mut form = form;
            form.name = format!("Rule {}", book.rules().expect("rules").len());
            add(book, &form)
        };
        next(&mut book, amount_form("out", "exactly", "9,99", ""));
        // Typed in either order: the range runs from the lower to the higher.
        next(&mut book, amount_form("out", "between", "50", "10"));
        next(&mut book, amount_form("in", "between", "1.200,00", "2"));
        next(&mut book, amount_form("in", "any", "", ""));
        assert_eq!(
            tests(&book),
            [
                AmountTest::Equals(money(-999)),
                AmountTest::Between {
                    min: money(-5000),
                    max: money(-1000)
                },
                AmountTest::Between {
                    min: money(200),
                    max: money(120_000)
                },
                AmountTest::MoneyIn,
            ]
        );
        // The editor shows money going out without its minus sign.
        let second = view(&book).expect("view")[1].id.clone();
        let form = form_of(&book, &second, &TestLocale).expect("form");
        assert_eq!(
            (
                form.conditions[0].direction.as_str(),
                form.conditions[0].amount.as_str(),
                form.conditions[0].amount_to.as_str()
            ),
            ("out", "10,00", "50,00")
        );

        // A range across zero, e.g. from the command-line tool, opens with
        // its signs and is saved the same way.
        let across = Rule {
            id: RuleId::generate(),
            name: "Across".into(),
            match_mode: MatchMode::All,
            conditions: vec![Condition::Amount(AmountTest::Between {
                min: money(-500),
                max: money(500),
            })],
            actions: kassensturz_core::Actions::default(),
        };
        book.add_rule(&across, None).expect("added");
        let mut form = form_of(&book, &across.id.to_string(), &TestLocale).expect("form");
        assert_eq!(form.conditions[0].direction, "signed");
        assert_eq!(
            (
                &form.conditions[0].amount[..],
                &form.conditions[0].amount_to[..]
            ),
            ("-5,00", "5,00")
        );
        form.name = "Across, renamed".into();
        change_rule(&mut book, &form, false, &TestLocale).expect("changed");
        assert_eq!(
            book.rule(across.id).expect("rule").conditions,
            across.conditions
        );
    }

    #[test]
    fn says_what_is_wrong_and_which_condition() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let account = checking(&mut book);
        let base = text_form("Shop", "payee", "is", "Shop", &groceries);
        let with = |extra: ConditionForm| RuleForm {
            conditions: vec![base.conditions[0].clone(), extra],
            ..base.clone()
        };
        let amount = |direction: &str, text: &str| ConditionForm {
            kind: "amount".into(),
            direction: direction.into(),
            amount_test: "exactly".into(),
            amount: text.into(),
            currency: "EUR".into(),
            ..ConditionForm::default()
        };
        let params = |error: ErrorText| (error.code, error.params);
        let condition_2 = ("condition", "2".to_owned());

        assert_eq!(
            params(refused(&mut book, &with(amount("out", "")))),
            ("input.empty-amount", vec![condition_2.clone()])
        );
        let malformed = refused(&mut book, &with(amount("out", "12.5.0")));
        assert_eq!(malformed.code, "input.malformed-amount");
        assert!(malformed.params.contains(&condition_2));
        assert_eq!(
            refused(&mut book, &with(amount("out", "1,234"))).code,
            "input.too-many-decimal-places"
        );
        // The direction says which way the money moves: no minus sign.
        assert_eq!(
            params(refused(&mut book, &with(amount("out", "-5")))),
            ("input.amount-not-positive", vec![condition_2.clone()])
        );
        let date = ConditionForm {
            kind: "date".into(),
            first_date: "last Tuesday".into(),
            last_date: "2026-12-31".into(),
            ..ConditionForm::default()
        };
        let bad_date = refused(&mut book, &with(date.clone()));
        assert_eq!(bad_date.code, "input.malformed-date");
        assert!(
            bad_date
                .params
                .contains(&("example", "2026-10-19".to_owned()))
        );
        let currency = ConditionForm {
            kind: "currency".into(),
            currency: "XXX".into(),
            ..ConditionForm::default()
        };
        assert_eq!(
            refused(&mut book, &with(currency)).code,
            "input.unknown-currency"
        );
        let gone = ConditionForm {
            kind: "account".into(),
            account: "gone".into(),
            ..ConditionForm::default()
        };
        assert_eq!(refused(&mut book, &with(gone)).code, "input.no-such-entry");
        // A category is no account to look at, and an account no category.
        let not_account = ConditionForm {
            kind: "account".into(),
            account: groceries.clone(),
            ..ConditionForm::default()
        };
        assert_eq!(
            refused(&mut book, &with(not_account)).code,
            "input.no-such-entry"
        );
        let to_account = RuleForm {
            book_to: account.clone(),
            ..base.clone()
        };
        assert_eq!(refused(&mut book, &to_account).code, "input.no-such-entry");

        // What the book itself checks (D-088), as codes the screen writes.
        let nothing = RuleForm {
            conditions: Vec::new(),
            ..base.clone()
        };
        assert_eq!(refused(&mut book, &nothing).code, "rule.no-conditions");
        let empty = text_form("Shop", "payee", "is", "  ", &groceries);
        let error = refused(&mut book, &empty);
        assert_eq!(error.code, "rule.empty-text");
        assert!(error.params.contains(&("condition", "1".to_owned())));
        let pattern = text_form("Shop", "payee", "pattern", "(unclosed", &groceries);
        assert_eq!(refused(&mut book, &pattern).code, "rule.bad-pattern");
        let nameless = RuleForm {
            name: "  ".into(),
            ..base.clone()
        };
        assert_eq!(refused(&mut book, &nameless).code, "store.empty-rule-name");
        let empty_payee = RuleForm {
            payee: "  ".into(),
            book_to_kind: String::new(),
            ..base.clone()
        };
        // Spaces only means "leave the payee": nothing to set, no error.
        add(&mut book, &empty_payee);
        assert_eq!(
            refused(
                &mut book,
                &RuleForm {
                    name: "SHOP".into(),
                    ..base
                }
            )
            .code,
            "store.duplicate-rule-name"
        );
        assert!(matches!(
            form_of(&book, "gone", &TestLocale),
            Err(RuleEditError::NoSuchEntry)
        ));
    }

    #[test]
    fn moves_and_removes_rules_and_the_order_decides_the_booking() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let clothes = category(&book, "Clothes");
        let account = checking(&mut book);
        let first = add(
            &mut book,
            &text_form("Shops", "payee", "contains", "shop", &groceries),
        );
        let second = add(
            &mut book,
            &text_form("Book shops", "payee", "contains", "book", &clothes),
        );
        let names = |book: &Book| -> Vec<String> {
            view(book)
                .expect("view")
                .into_iter()
                .map(|r| r.name)
                .collect()
        };
        assert_eq!(names(&book), ["Shops", "Book shops"]);

        // The first Rule that fits decides.
        let bought = typed(
            &mut book,
            &account,
            "Example Bookshop",
            "5",
            "",
            "2026-10-02",
        );
        book.run_rules().expect("run");
        assert_eq!(category_of(&book, &bought).as_deref(), Some("Groceries"));

        move_rule(&mut book, &second, true).expect("moved up");
        assert_eq!(names(&book), ["Book shops", "Shops"]);
        let later = typed(
            &mut book,
            &account,
            "Example Bookshop",
            "7",
            "",
            "2026-10-03",
        );
        book.run_rules().expect("run");
        assert_eq!(category_of(&book, &later).as_deref(), Some("Clothes"));
        // At the ends of the list, moving does nothing.
        move_rule(&mut book, &second, true).expect("stays");
        move_rule(&mut book, &first, false).expect("stays");
        assert_eq!(names(&book), ["Book shops", "Shops"]);
        move_rule(&mut book, &second, false).expect("moved down");
        assert_eq!(names(&book), ["Shops", "Book shops"]);

        // What a removed Rule booked keeps its booking.
        remove_rule(&mut book, &first).expect("removed");
        assert_eq!(names(&book), ["Book shops"]);
        assert_eq!(category_of(&book, &bought).as_deref(), Some("Groceries"));
        assert!(matches!(
            remove_rule(&mut book, &first),
            Err(RuleEditError::NoSuchEntry)
        ));
        assert!(matches!(
            move_rule(&mut book, "gone", true),
            Err(RuleEditError::NoSuchEntry)
        ));
    }

    #[test]
    fn an_imported_statement_follows_a_rule_made_in_the_editor() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let account = checking(&mut book);
        add(
            &mut book,
            &text_form("Shop", "other-party-name", "contains", "market", &groceries),
        );
        let loaded = Loaded::new(
            account_id(&book, &account),
            PathBuf::from("october.sta"),
            statement(&[(5, -3525, "Example Market 12"), (6, -1000, "Unknown")]),
        );
        let done = import(&mut book, &loaded, &TestLocale).expect("imported");
        assert_eq!(done.rules, 1);
        assert_eq!(
            category_of(&book, &done.new[0].to_string()).as_deref(),
            Some("Groceries")
        );
        assert_eq!(category_of(&book, &done.new[1].to_string()), None);
    }

    #[test]
    fn lists_what_a_rule_matches_newest_first_booked_by_it_or_not() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let clothes = category(&book, "Clothes");
        let account = checking(&mut book);
        let shops = add(
            &mut book,
            &text_form("Shops", "payee", "contains", "shop", &groceries),
        );
        let novels = add(
            &mut book,
            &text_form("Novels", "payee", "contains", "novel", &clothes),
        );
        // Sorted by hand, so no Rule booked it; and one that is not sorted.
        let by_hand = typed(
            &mut book,
            &account,
            "Example Shop",
            "5",
            &groceries,
            "2026-10-02",
        );
        let waiting = typed(&mut book, &account, "Example Shop", "7", "", "2026-10-03");
        typed(&mut book, &account, "Example Bakery", "3", "", "2026-10-04");
        let novel = typed(&mut book, &account, "Novel Corner", "9", "", "2026-10-05");

        let found = matches(&book, &shops, &TestLocale).expect("matches");
        let ids: Vec<&str> = found.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, [waiting.as_str(), by_hand.as_str()], "newest first");
        assert_eq!(found[0].account, "Checking account");
        assert_eq!(found[0].account_id, account);
        assert_eq!(found[0].other_party, "Example Shop");
        assert!(!found[0].incoming);
        assert_eq!(found[1].category, "Groceries");

        let found = matches(&book, &novels, &TestLocale).expect("matches");
        let ids: Vec<&str> = found.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, [novel.as_str()]);

        assert!(matches!(
            matches(&book, "gone", &TestLocale),
            Err(RuleEditError::NoSuchEntry)
        ));
    }

    #[test]
    fn previews_what_a_rule_being_typed_fits_without_saving_it() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let account = checking(&mut book);
        add(
            &mut book,
            &text_form("Shops", "payee", "contains", "shop", &groceries),
        );
        let by_hand = typed(&mut book, &account, "Example Shop", "5", "", "2026-10-02");
        let other = typed(&mut book, &account, "Example Bakery", "3", "", "2026-10-03");
        let rules_before = view(&book).expect("view").len();

        // A form that is not saved, and that the saved Rule before it would
        // take first: the preview looks at its conditions alone.
        let draft = text_form("Bread", "payee", "contains", "bakery", &groceries);
        let found = preview_matches(&book, &draft, &TestLocale).expect("preview");
        let ids: Vec<&str> = found.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, [other.as_str()]);
        assert_eq!(found[0].account, "Checking account");

        let wide = text_form("Example", "payee", "contains", "example", &groceries);
        let found = preview_matches(&book, &wide, &TestLocale).expect("preview");
        let ids: Vec<&str> = found.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, [other.as_str(), by_hand.as_str()], "newest first");
        assert_eq!(view(&book).expect("view").len(), rules_before, "not saved");

        // The editor shows nothing while the form isn't a Rule yet.
        let empty = RuleForm::default();
        assert!(preview_matches(&book, &empty, &TestLocale).is_err());
    }

    fn account_id(book: &Book, id: &str) -> AccountId {
        book.all_accounts()
            .expect("accounts")
            .into_iter()
            .find(|account| account.id.to_string() == id)
            .expect("account")
            .id
    }

    #[test]
    fn changing_a_rule_books_again_what_it_booked_but_not_what_the_user_changed() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let clothes = category(&book, "Clothes");
        let account = checking(&mut book);
        let id = add(
            &mut book,
            &text_form("Shop", "other-party-name", "contains", "market", &groceries),
        );
        let loaded = Loaded::new(
            account_id(&book, &account),
            PathBuf::from("october.sta"),
            statement(&[
                (5, -3525, "Example Market 12"),
                (6, -1250, "Example Market 3"),
            ]),
        );
        let done = import(&mut book, &loaded, &TestLocale).expect("imported");
        let (kept, mine) = (done.new[0].to_string(), done.new[1].to_string());

        // The user sorts one of them by hand.
        let rent = category(&book, "Rent");
        let edit = transaction_entry::details(&book, &account, &mine, &TestLocale).expect("form");
        transaction_entry::edit(
            &mut book,
            &mine,
            &NewEntry {
                account: &account,
                money_in: edit.money_in,
                amount: &edit.amount,
                other_party: &edit.other_party,
                category: &rent,
                date: &edit.date,
            },
            &TestLocale,
        )
        .expect("edited");

        let mut form = form_of(&book, &id, &TestLocale).expect("form");
        form.book_to = clothes;

        // Without asking for it, only the Rule changes.
        let preview = preview_rebook(&book, &form, &TestLocale).expect("preview");
        assert_eq!(
            preview,
            Rebooked {
                changed: 1,
                skipped: 0
            }
        );
        assert_eq!(category_of(&book, &kept).as_deref(), Some("Groceries"));
        let none = change_rule(&mut book, &form, false, &TestLocale).expect("changed");
        assert_eq!(none, Rebooked::default());
        assert_eq!(category_of(&book, &kept).as_deref(), Some("Groceries"));

        form.book_to = groceries;
        let before = book.transactions().expect("transactions").len();
        let back = change_rule(&mut book, &form, true, &TestLocale).expect("changed");
        assert_eq!(back, Rebooked::default());
        assert_eq!(book.transactions().expect("transactions").len(), before);
        // The Rule now books to Groceries again (it still did), and the
        // transaction the user sorted keeps their category.
        assert_eq!(category_of(&book, &mine).as_deref(), Some("Rent"));

        form.book_to = category(&book, "Clothes");
        let moved = change_rule(&mut book, &form, true, &TestLocale).expect("changed");
        assert_eq!(
            moved,
            Rebooked {
                changed: 1,
                skipped: 0
            }
        );
        assert_eq!(category_of(&book, &kept).as_deref(), Some("Clothes"));
        assert_eq!(category_of(&book, &mine).as_deref(), Some("Rent"));
        // One user action: undo takes the change and the rebooking back.
        book.undo().expect("undone");
        assert_eq!(category_of(&book, &kept).as_deref(), Some("Groceries"));
    }

    #[test]
    fn a_rule_from_a_transaction_starts_with_what_the_bank_delivered() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let account = checking(&mut book);
        let loaded = Loaded::new(
            account_id(&book, &account),
            PathBuf::from("october.sta"),
            statement(&[(5, -3525, "Example Market 12")]),
        );
        let done = import(&mut book, &loaded, &TestLocale).expect("imported");
        let imported = done.new[0].to_string();
        let form = form_from_transaction(&book, &account, &imported).expect("form");
        assert_eq!(
            form.conditions,
            [text_condition(
                "other-party-name",
                "contains",
                "Example Market 12"
            )]
        );
        assert_eq!(
            (form.name.as_str(), form.book_to_kind.as_str()),
            ("Example Market 12", "")
        );
        // The user adjusts it and the Rule applies to what comes next.
        let mut form = form;
        form.conditions[0].value = "Example Market".into();
        form.book_to_kind = "category".into();
        form.book_to = groceries;
        form.name = "Market".into();
        add(&mut book, &form);
        let next = Loaded::new(
            account_id(&book, &account),
            PathBuf::from("november.sta"),
            statement(&[(15, -999, "EXAMPLE MARKET 7")]),
        );
        let again = import(&mut book, &next, &TestLocale).expect("imported");
        assert_eq!(
            category_of(&book, &again.new[0].to_string()).as_deref(),
            Some("Groceries")
        );

        // A transaction typed by hand has a payee and a category.
        let rent = category(&book, "Rent");
        let by_hand = typed(&mut book, &account, "Landlord", "700", &rent, "2026-10-01");
        let form = form_from_transaction(&book, &account, &by_hand).expect("form");
        assert_eq!(form.conditions, [text_condition("payee", "is", "Landlord")]);
        assert_eq!(
            (
                form.name.as_str(),
                form.book_to_kind.as_str(),
                form.book_to.as_str()
            ),
            ("Landlord", "category", rent.as_str())
        );
        assert!(matches!(
            form_from_transaction(&book, &account, "gone"),
            Err(RuleEditError::NoSuchEntry)
        ));
        assert!(matches!(
            form_from_transaction(&book, "gone", &by_hand),
            Err(RuleEditError::NoSuchEntry)
        ));
    }

    #[test]
    fn suggests_a_rule_after_the_same_payee_was_sorted_by_hand_several_times() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let eating_out = category(&book, "Food › Eating out");
        let account = checking(&mut book);
        let found = |book: &Book| suggestions(book, &[]).expect("suggestions");

        typed(
            &mut book,
            &account,
            "Corner Shop",
            "5",
            &groceries,
            "2026-10-01",
        );
        typed(
            &mut book,
            &account,
            "corner shop",
            "6",
            &groceries,
            "2026-10-02",
        );
        assert!(found(&book).is_empty(), "twice is not several times");
        // Not sorted yet doesn't count.
        typed(&mut book, &account, "Corner Shop", "7", "", "2026-10-03");
        assert!(found(&book).is_empty());
        typed(
            &mut book,
            &account,
            " Corner SHOP ",
            "8",
            &groceries,
            "2026-10-04",
        );
        let offered = found(&book);
        assert_eq!(offered.len(), 1);
        // The way it was typed last, and the category's path.
        assert_eq!(
            offered[0],
            Suggestion {
                payee: "Corner SHOP".into(),
                category: groceries.clone(),
                category_path: "Food › Groceries".into(),
                count: 3,
            }
        );

        // Two categories equally often: no suggestion until one is ahead.
        typed(
            &mut book,
            &account,
            "Corner Shop",
            "9",
            &eating_out,
            "2026-10-05",
        );
        assert_eq!(found(&book).len(), 1, "3 against 1");
        typed(
            &mut book,
            &account,
            "Corner Shop",
            "9",
            &eating_out,
            "2026-10-06",
        );
        typed(
            &mut book,
            &account,
            "Corner Shop",
            "9",
            &eating_out,
            "2026-10-07",
        );
        assert!(found(&book).is_empty(), "3 against 3");

        // Saying no is remembered, whatever the capitals.
        typed(
            &mut book,
            &account,
            "Corner Shop",
            "9",
            &groceries,
            "2026-10-08",
        );
        let offered = found(&book);
        assert_eq!(offered.len(), 1);
        let no = dismissal_key("CORNER shop", &groceries);
        assert!(
            suggestions(&book, std::slice::from_ref(&no))
                .expect("suggestions")
                .is_empty()
        );
        // Another category is another suggestion.
        assert_eq!(
            suggestions(&book, &[dismissal_key("Corner Shop", &eating_out)])
                .expect("suggestions")
                .len(),
            1
        );
    }

    #[test]
    fn a_suggestion_becomes_a_rule_and_then_stops_being_suggested() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let account = checking(&mut book);
        for (day, shop) in [(1, "Corner Shop"), (2, "Corner Shop"), (3, "corner shop")] {
            typed(
                &mut book,
                &account,
                shop,
                "5",
                &groceries,
                &format!("2026-10-0{day}"),
            );
        }
        // A Rule's own bookings are not "by hand": three of them make no
        // suggestion.
        add(
            &mut book,
            &text_form(
                "Market",
                "other-party-name",
                "contains",
                "market",
                &groceries,
            ),
        );
        let loaded = Loaded::new(
            account_id(&book, &account),
            PathBuf::from("october.sta"),
            statement(&[
                (5, -100, "Market A"),
                (6, -200, "Market A"),
                (7, -300, "Market A"),
            ]),
        );
        import(&mut book, &loaded, &TestLocale).expect("imported");
        let offered = suggestions(&book, &[]).expect("suggestions");
        assert_eq!(
            offered.iter().map(|s| s.payee.as_str()).collect::<Vec<_>>(),
            ["corner shop"]
        );

        let form =
            form_from_suggestion(&book, &offered[0].payee, &offered[0].category).expect("form");
        assert_eq!(form.name, "corner shop");
        add(&mut book, &form);
        assert!(suggestions(&book, &[]).expect("suggestions").is_empty());

        // The new Rule sorts what the user types next, whatever the capitals.
        let next = typed(&mut book, &account, "CORNER SHOP", "4", "", "2026-10-09");
        book.run_rules().expect("run");
        assert_eq!(category_of(&book, &next).as_deref(), Some("Groceries"));
        assert!(matches!(
            form_from_suggestion(&book, "x", "gone"),
            Err(RuleEditError::NoSuchEntry)
        ));
    }

    #[test]
    fn a_rule_that_already_decides_hides_the_suggestion() {
        let mut book = started();
        let groceries = category(&book, "Food › Groceries");
        let clothes = category(&book, "Clothes");
        let account = checking(&mut book);
        // The user keeps overriding a Rule by hand; the Rule still wins for
        // new transactions, so another Rule would never be used.
        add(
            &mut book,
            &text_form("Shop", "payee", "contains", "shop", &clothes),
        );
        for day in 1..=3 {
            typed(
                &mut book,
                &account,
                "Corner Shop",
                "5",
                &groceries,
                &format!("2026-10-0{day}"),
            );
        }
        assert!(suggestions(&book, &[]).expect("suggestions").is_empty());
    }

    #[test]
    fn turned_down_suggestions_are_kept_in_the_settings_file() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(dismissed(&settings).is_empty());
        assert!(dismiss(&settings, "Corner\nShop", "category-1"));
        assert!(dismiss(&settings, "corner shop", "category-1"));
        assert!(dismiss(&settings, "Bakery", "category-2"));
        let again = dismissed(&Settings::at(&file));
        assert_eq!(
            again,
            [
                dismissal_key("corner shop", "category-1"),
                dismissal_key("Bakery", "category-2")
            ]
        );
        // The payee's capitals and line breaks don't make a new entry.
        assert_eq!(
            dismissal_key("Corner\nShop", "c"),
            dismissal_key("corner shop", "c")
        );
        // Only the latest are remembered.
        for number in 0..(REMEMBERED + 5) {
            assert!(dismiss(&settings, &format!("Shop {number}"), "c"));
        }
        let all = dismissed(&settings);
        assert_eq!(all.len(), REMEMBERED);
        assert_eq!(
            all.last(),
            Some(&dismissal_key(&format!("Shop {}", REMEMBERED + 4), "c"))
        );
    }

    #[test]
    fn offers_the_categories_the_open_accounts_and_the_usual_currency() {
        let mut book = started();
        let account = checking(&mut book);
        let eur = CurrencyCode::new("EUR").expect("code");
        let offered = choices(&book, eur).expect("choices");
        assert_eq!(offered.currency, "EUR");
        assert_eq!(offered.accounts.len(), 1);
        assert_eq!(offered.accounts[0].id, account);
        let groceries = offered
            .categories
            .iter()
            .find(|choice| choice.name == "Food › Groceries")
            .expect("Groceries");
        assert_eq!((groceries.depth, groceries.income), (1, false));
        assert!(offered.categories.iter().any(|choice| choice.income));
        let blank = new_form("");
        assert_eq!(blank.conditions.len(), 1);
        assert!(blank.match_all);
    }
}
