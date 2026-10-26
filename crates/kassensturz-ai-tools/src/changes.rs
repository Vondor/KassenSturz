// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Change sets: what a model wants to change in a book, held for the user
//! to review, and applied only when the user confirms (D-171, D-199 to
//! D-201, AGENTS.md §3.1 rule 9, §8).
//!
//! A model prepares one with [`PROPOSE_CHANGES`]: new categories, new Rules
//! and a category for transactions, nothing else. [`propose`] checks the
//! call against the book and turns it into a [`ChangeSet`] whose
//! [`ProposedChange`]s are what the user sees, before and after, with exact
//! amounts. Nothing in the book changes then. Only [`ChangeSet::apply`]
//! changes it, and it takes a [`Confirmed`], which only the user's confirm
//! button makes. It runs the store's own operations as one user action,
//! [`Action::ApplyAiChanges`] (D-198), so one undo takes back the whole set
//! and the history keeps it apart from the user's own changes.
//!
//! The set doesn't care how the call arrived: the chat hands it over in
//! memory, and the MCP write tools (M5.10) will hand over the same JSON
//! through a request file. Whatever the way, the app checks and shows it
//! here, and applies it here.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, AmountTest, BookTo, Condition, CurrencyCode, Date,
    ErrorReport, MatchMode, Money, Report, Rule, RuleId, Split, SplitId, TextField, TextOperator,
    Transaction, TransactionId,
};
use kassensturz_store::{Action, Book, StoreError};
use serde_json::{Map, Value, json};

use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};
use crate::text::hides_text;

/// The function a model calls to prepare changes. It is not in
/// [`crate::FUNCTIONS`], the read functions every way in offers: the chat
/// offers it next to them, and the MCP server only once its write tools are
/// turned on (M5.10, D-035).
pub const PROPOSE_CHANGES: Function = Function {
    name: "propose_changes",
    description: "Prepares changes to the user's book for the user to review: new \
        categories, new Rules that sort future transactions into a category, and a category \
        for transactions you found with search_transactions. Nothing changes until the user \
        confirms them in KassenSturz's window, and the user may refuse them. A new category \
        has a key of your choice, which new_rules and categorize use as new_category. Only a \
        transaction with one category, or none yet, can get another one; a transfer between \
        the user's own accounts can't. Prepare everything that belongs together in one call.",
    parameters: r#"{"type":"object","properties":{"new_categories":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"key":{"type":"string","maxLength":40,"description":"Your short name for it, used as new_category below."},"name":{"type":"string","maxLength":100},"parent_id":{"type":"string","description":"Put it inside this category, ID from list_categories. Default: at the top."},"income":{"type":"boolean","description":"An income category, not a spending one. Default false. Inside a category it must be the same kind."},"currency":{"type":"string","description":"ISO 4217 code. Default: the parent's, or the one all the user's accounts use."}},"required":["key","name"],"additionalProperties":false}},"new_rules":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"name":{"type":"string","maxLength":100},"match":{"type":"string","enum":["all","any"],"description":"Whether every condition must hold or one is enough. Default all."},"conditions":{"type":"array","minItems":1,"maxItems":10,"items":{"type":"object","description":"Either field, test and text, or money_in: true, or money_out: true.","properties":{"field":{"type":"string","enum":["payee","other_party_name","bank_text"]},"test":{"type":"string","enum":["contains","equals","starts_with","ends_with"]},"text":{"type":"string","maxLength":100},"case_sensitive":{"type":"boolean"},"money_in":{"type":"boolean"},"money_out":{"type":"boolean"}},"additionalProperties":false}},"category_id":{"type":"string","description":"Where it books what it finds, ID from list_categories."},"new_category":{"type":"string","description":"Or the key of a category in new_categories."}},"required":["name","conditions"],"additionalProperties":false}},"categorize":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"transaction_id":{"type":"string","description":"ID from search_transactions."},"category_id":{"type":"string","description":"ID from list_categories."},"new_category":{"type":"string","description":"Or the key of a category in new_categories."}},"required":["transaction_id"],"additionalProperties":false}}},"additionalProperties":false}"#,
};

/// The most changes one set may hold, so the user can still review it.
pub const MAX_CHANGES: usize = 100;

/// The most characters of a name or a condition's text.
const MAX_NAME_CHARS: usize = 100;

/// The most characters of a new category's key.
const MAX_KEY_CHARS: usize = 40;

/// The most conditions of one Rule.
const MAX_CONDITIONS: usize = 10;

/// The names of every argument [`PROPOSE_CHANGES`] takes, at any depth:
/// the keywords of `ai-tools.bad-argument` for it.
#[cfg(test)]
pub(crate) const ARGUMENTS: &[&str] = &[
    "new_categories",
    "key",
    "name",
    "parent_id",
    "income",
    "currency",
    "new_rules",
    "match",
    "conditions",
    "field",
    "test",
    "text",
    "case_sensitive",
    "money_in",
    "money_out",
    "category_id",
    "new_category",
    "categorize",
    "transaction_id",
];

/// One change of a set, as the user reviews it. Every text in it came from
/// the model or the book: shown as plain text (threat T-08).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposedChange {
    /// A new category.
    NewCategory(NewCategory),
    /// A new Rule, at the end of the list.
    NewRule(NewRule),
    /// A transaction booked to another category, or to one for the first
    /// time.
    Categorize(Categorize),
}

/// A new category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCategory {
    /// The ID it will have.
    pub id: AccountId,
    /// Its name, with the names of the categories above it first.
    pub path: Vec<String>,
    /// The category it goes inside, if any.
    pub parent: Option<AccountId>,
    /// An income category, or a spending one.
    pub income: bool,
    /// Its currency.
    pub currency: CurrencyCode,
}

/// A new Rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRule {
    /// The Rule, which books to a category.
    pub rule: Rule,
    /// That category's path, new or not.
    pub category: Vec<String>,
}

/// A transaction that gets a category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Categorize {
    /// The transaction.
    pub transaction: TransactionId,
    /// The day the user's account shows it.
    pub date: Date,
    /// Its payee, if any.
    pub payee: Option<String>,
    /// The user's account the money moved in or out of.
    pub account: String,
    /// What it did to that account, in the account's currency: positive
    /// for money in (D-065: the front end writes it as text).
    pub amount: Money,
    /// The category's path before, `None` for "no category yet".
    pub before: Option<Vec<String>>,
    /// The category's path after.
    pub after: Vec<String>,
    /// The category it gets.
    pub category: AccountId,
    /// The transaction as the user reviewed it: applying fails if it
    /// changed since.
    reviewed: Transaction,
    /// Its part that gets the category.
    part: SplitId,
}

/// The changes a model prepared, checked against the book. Only
/// [`propose`] makes one, and only [`ChangeSet::apply`] changes the book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSet {
    changes: Vec<ProposedChange>,
}

/// The user's yes to one change set. Only the confirm button of the
/// KassenSturz window makes one (D-171): never the chat, the MCP server or
/// anything a model can reach, which M5.12 checks.
#[derive(Debug)]
pub struct Confirmed(());

impl Confirmed {
    /// The user pressed the confirm button for the set on the screen.
    pub fn by_the_user() -> Self {
        Self(())
    }
}

/// What the call prepared: the set for the user, and the answer for the
/// model, which says that it waits for the user.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    /// The set to review.
    pub changes: ChangeSet,
    /// The model's answer to its call.
    pub output: Output,
}

/// Why a change set wasn't applied. Nothing of it was: the book is as it
/// was before.
#[derive(Debug)]
pub enum ChangeError {
    /// The book changed since the set was prepared: a transaction in it
    /// was changed or removed.
    OutOfDate,
    /// The book refused a change, e.g. because the user added a category
    /// of the same name since.
    NotApplied(StoreError),
}

impl From<StoreError> for ChangeError {
    fn from(error: StoreError) -> Self {
        Self::NotApplied(error)
    }
}

impl core::fmt::Display for ChangeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for ChangeError {}

impl Report for ChangeError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::OutOfDate => ErrorReport::new("ai-changes.out-of-date"),
            Self::NotApplied(error) => {
                ErrorReport::new("ai-changes.not-applied").caused_by(error.report())
            }
        }
    }
}

impl ChangeSet {
    /// The changes, in the order they are applied: new categories, new
    /// Rules, then the transactions.
    pub fn changes(&self) -> &[ProposedChange] {
        &self.changes
    }

    /// Applies the set to `book` as one user action, which one undo takes
    /// back (D-198). Everything is applied or nothing: if the book changed
    /// since the set was prepared, or the book refuses one of the changes,
    /// the book stays as it was.
    pub fn apply(self, book: &mut Book, confirmed: Confirmed) -> Result<(), ChangeError> {
        let Confirmed(()) = confirmed;
        book.as_one_action(Action::ApplyAiChanges, |book| {
            for change in &self.changes {
                match change {
                    ProposedChange::NewCategory(new) => {
                        book.use_currency(new.currency)?;
                        book.add_account(&Account {
                            id: new.id,
                            name: new.path.last().cloned().unwrap_or_default(),
                            kind: if new.income {
                                AccountKind::Income
                            } else {
                                AccountKind::Expense
                            },
                            currency: new.currency,
                            account_type: None,
                            parent: new.parent,
                            closed: false,
                            number: None,
                            notes: None,
                            institution: None,
                        })?;
                    }
                    ProposedChange::NewRule(new) => book.add_rule(&new.rule, None)?,
                    ProposedChange::Categorize(change) => {
                        let stored = match book.transaction(change.transaction) {
                            Err(StoreError::UnknownTransaction(_)) => {
                                return Err(ChangeError::OutOfDate);
                            }
                            other => other?,
                        };
                        if stored != change.reviewed {
                            return Err(ChangeError::OutOfDate);
                        }
                        book.update_transaction(&recategorized(change)?)?;
                    }
                }
            }
            Ok(())
        })
    }
}

/// The reviewed transaction with the part booked to the new category.
fn recategorized(change: &Categorize) -> Result<Transaction, ChangeError> {
    let before = &change.reviewed;
    let splits = before
        .splits()
        .iter()
        .map(|split| {
            if split.id == change.part {
                Split {
                    account: Some(change.category),
                    // A category in the transaction's currency (checked when
                    // proposed) takes no amount in another one.
                    foreign: None,
                    ..split.clone()
                }
            } else {
                split.clone()
            }
        })
        .collect();
    Transaction::new(
        before.id(),
        before.booking_date(),
        before.payee().map(str::to_owned),
        splits,
    )
    .map(|transaction| transaction.with_details_of(before))
    .map_err(|_| ChangeError::OutOfDate)
}

/// Checks a model's call of [`PROPOSE_CHANGES`] against `book` and turns it
/// into a change set for the user to review. The book doesn't change.
pub fn propose(book: &Book, arguments: &Value) -> Result<Proposal, ToolError> {
    let call = Object::read(
        arguments,
        &["new_categories", "new_rules", "categorize"],
        "arguments",
    )?;
    let categories = call.array("new_categories")?;
    let rules = call.array("new_rules")?;
    let transactions = call.array("categorize")?;
    let count = categories.len() + rules.len() + transactions.len();
    if count == 0 {
        return Err(ToolError::BadArgument {
            argument: "arguments",
        });
    }
    if count > MAX_CHANGES {
        return Err(ToolError::TooManyChanges);
    }

    let mut known = Known::read(book)?;
    let mut changes = Vec::with_capacity(count);
    for category in categories {
        changes.push(ProposedChange::NewCategory(known.new_category(category)?));
    }
    let mut rule_names = HashSet::new();
    for rule in rules {
        changes.push(ProposedChange::NewRule(known.new_rule(
            book,
            rule,
            &mut rule_names,
        )?));
    }
    let mut seen = HashSet::new();
    for item in transactions {
        let change = known.categorize(book, item)?;
        if !seen.insert(change.transaction) {
            return Err(ToolError::BadArgument {
                argument: "transaction_id",
            });
        }
        changes.push(ProposedChange::Categorize(change));
    }

    let mut figures = Figures::new();
    let total = u64::try_from(count).unwrap_or(u64::MAX);
    figures.add_count(total);
    let output = Output::new(
        json!({
            "prepared": total,
            "status": "waiting for the user's review; nothing is changed until the user confirms",
        }),
        figures,
    );
    Ok(Proposal {
        changes: ChangeSet { changes },
        output,
    })
}

/// What the checks need to know about the book, and the categories the set
/// adds so far.
struct Known {
    accounts: HashMap<AccountId, Account>,
    paths: HashMap<AccountId, Vec<String>>,
    /// The new categories by their keys.
    keys: HashMap<String, AccountId>,
    /// The currency of the user's open accounts, if they all use one.
    usual_currency: Option<CurrencyCode>,
}

impl Known {
    fn read(book: &Book) -> Result<Self, StoreError> {
        let accounts: HashMap<_, _> = book
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, account))
            .collect();
        let paths = book
            .categories()?
            .into_iter()
            .map(|entry| (entry.category.id, entry.path))
            .collect();
        let mut currencies = accounts
            .values()
            .filter(|account| account.kind.is_account() && !account.closed)
            .map(|account| account.currency);
        let usual_currency = currencies
            .next()
            .filter(|first| currencies.all(|other| other == *first));
        Ok(Self {
            accounts,
            paths,
            keys: HashMap::new(),
            usual_currency,
        })
    }

    /// An open category the set may book to: one of the book's, by
    /// `category_id`, or a new one, by `new_category`.
    fn target(&self, item: &Object<'_>) -> Result<AccountId, ToolError> {
        match (item.id("category_id")?, item.string("new_category")?) {
            (Some(id), None) => {
                let usable = self
                    .accounts
                    .get(&id)
                    .is_some_and(|category| category.kind.is_category() && !category.closed);
                if usable {
                    Ok(id)
                } else {
                    Err(ToolError::UnknownCategory)
                }
            }
            (None, Some(key)) => self.keys.get(key).copied().ok_or(ToolError::BadArgument {
                argument: "new_category",
            }),
            _ => Err(ToolError::BadArgument {
                argument: "category_id",
            }),
        }
    }

    fn path(&self, id: AccountId) -> Vec<String> {
        self.paths.get(&id).cloned().unwrap_or_default()
    }

    fn new_category(&mut self, value: &Value) -> Result<NewCategory, ToolError> {
        let item = Object::read(
            value,
            &["key", "name", "parent_id", "income", "currency"],
            "new_categories",
        )?;
        let key = item.required_string("key")?.trim();
        if key.is_empty() || key.chars().count() > MAX_KEY_CHARS || self.keys.contains_key(key) {
            return Err(ToolError::BadArgument { argument: "key" });
        }
        let name = plain_text(item.required_string("name")?, "name")?;
        let parent = match item.id("parent_id")? {
            Some(id) => Some(
                self.accounts
                    .get(&id)
                    .filter(|parent| parent.kind.is_category() && !parent.closed)
                    .ok_or(ToolError::UnknownCategory)?
                    .clone(),
            ),
            None => None,
        };
        // The kind is never taken from the parent silently: a model that
        // leaves `income` out asked for a spending category (the default),
        // and one inside an income category is refused, so that a new
        // spending category cannot end up under an income group.
        let income = item.boolean("income")?.unwrap_or(false);
        if let Some(parent) = &parent
            && (parent.kind == AccountKind::Income) != income
        {
            return Err(ToolError::BadArgument { argument: "income" });
        }
        let asked_currency = item
            .string("currency")?
            .map(|code| {
                CurrencyCode::new(code)
                    .ok()
                    .filter(|currency| {
                        currency.iso_minor_digits().is_some() && currency.withdrawn().is_none()
                    })
                    .ok_or(ToolError::BadArgument {
                        argument: "currency",
                    })
            })
            .transpose()?;
        let currency = match (&parent, asked_currency) {
            (Some(parent), Some(asked)) if asked != parent.currency => {
                return Err(ToolError::BadArgument {
                    argument: "currency",
                });
            }
            (Some(parent), _) => parent.currency,
            (None, Some(asked)) => asked,
            (None, None) => self.usual_currency.ok_or(ToolError::BadArgument {
                argument: "currency",
            })?,
        };
        let kind = if income {
            AccountKind::Income
        } else {
            AccountKind::Expense
        };
        let parent_id = parent.as_ref().map(|parent| parent.id);
        // Categories of one kind inside the same category have different
        // names, as the store checks when it adds one.
        let wanted = name.to_lowercase();
        if self.accounts.values().any(|other| {
            other.kind == kind
                && other.parent == parent_id
                && other.name.trim().to_lowercase() == wanted
        }) {
            return Err(ToolError::NameTaken);
        }
        let id = AccountId::generate();
        let mut path = parent_id
            .map(|parent| self.path(parent))
            .unwrap_or_default();
        path.push(name.clone());
        self.accounts.insert(
            id,
            Account {
                id,
                name,
                kind,
                currency,
                account_type: None,
                parent: parent_id,
                closed: false,
                number: None,
                notes: None,
                institution: None,
            },
        );
        self.paths.insert(id, path.clone());
        self.keys.insert(key.to_owned(), id);
        Ok(NewCategory {
            id,
            path,
            parent: parent_id,
            income,
            currency,
        })
    }

    fn new_rule(
        &self,
        book: &Book,
        value: &Value,
        names: &mut HashSet<String>,
    ) -> Result<NewRule, ToolError> {
        let item = Object::read(
            value,
            &["name", "match", "conditions", "category_id", "new_category"],
            "new_rules",
        )?;
        let name = plain_text(item.required_string("name")?, "name")?;
        let wanted = name.to_lowercase();
        let taken = book
            .rules()?
            .iter()
            .any(|node| node.rule.name.trim().to_lowercase() == wanted);
        if taken || !names.insert(wanted) {
            return Err(ToolError::NameTaken);
        }
        let match_mode = match item.string("match")? {
            None | Some("all") => MatchMode::All,
            Some("any") => MatchMode::Any,
            Some(_) => return Err(ToolError::BadArgument { argument: "match" }),
        };
        let listed = item.array("conditions")?;
        if listed.is_empty() || listed.len() > MAX_CONDITIONS {
            return Err(ToolError::BadArgument {
                argument: "conditions",
            });
        }
        let conditions = listed
            .iter()
            .map(condition)
            .collect::<Result<Vec<_>, _>>()?;
        let category = self.target(&item)?;
        let rule = Rule {
            id: RuleId::generate(),
            name,
            match_mode,
            conditions,
            actions: Actions {
                book_to: Some(BookTo::Category(category)),
                payee: None,
            },
        };
        rule.check().map_err(|_| ToolError::BadArgument {
            argument: "conditions",
        })?;
        Ok(NewRule {
            rule,
            category: self.path(category),
        })
    }

    fn categorize(&self, book: &Book, value: &Value) -> Result<Categorize, ToolError> {
        let item = Object::read(
            value,
            &["transaction_id", "category_id", "new_category"],
            "categorize",
        )?;
        let id = item
            .uuid("transaction_id")?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "transaction_id",
            })?;
        let transaction = match book.transaction(id) {
            Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
            other => other?,
        };
        let category = self.target(&item)?;
        // One part in one of the user's accounts, and one part booked to a
        // category or to none yet: anything else, such as a transfer, a
        // starting balance or a transaction split over several categories,
        // is the user's to change.
        let account_of = |split: &Split| split.account.and_then(|id| self.accounts.get(&id));
        let [first, second] = transaction.splits() else {
            return Err(ToolError::NotCategorizable);
        };
        let (own, part) = match (account_of(first), account_of(second)) {
            (Some(account), other) if account.kind.is_account() => (first, (second, other)),
            (other, Some(account)) if account.kind.is_account() => (second, (first, other)),
            _ => return Err(ToolError::NotCategorizable),
        };
        let (part, before) = part;
        let before = match before {
            None => None,
            Some(account) if account.kind.is_category() && !account.closed => Some(account),
            Some(_) => return Err(ToolError::NotCategorizable),
        };
        if before.is_some_and(|before| before.id == category) {
            return Err(ToolError::BadArgument {
                argument: "category_id",
            });
        }
        let target = &self.accounts[&category];
        if target.currency != transaction.currency() {
            return Err(ToolError::OtherCurrency);
        }
        let own_account = account_of(own).ok_or(ToolError::NotCategorizable)?;
        Ok(Categorize {
            transaction: id,
            date: transaction.date_of(own),
            payee: transaction.payee_of(own).map(str::to_owned),
            account: own_account.name.clone(),
            amount: own.account_amount(),
            before: before.map(|before| self.path(before.id)),
            after: self.path(category),
            category,
            part: part.id,
            reviewed: transaction,
        })
    }
}

/// One condition of a new Rule: a text test on the payee, the other party's
/// name or the bank's text, or money in or out. No IBAN, since a model
/// never sees one (D-186), and no pattern, the advanced way (D-015).
fn condition(value: &Value) -> Result<Condition, ToolError> {
    let item = Object::read(
        value,
        &[
            "field",
            "test",
            "text",
            "case_sensitive",
            "money_in",
            "money_out",
        ],
        "conditions",
    )?;
    let money_in = item.boolean("money_in")?;
    let money_out = item.boolean("money_out")?;
    let field = item.string("field")?;
    match (field, money_in, money_out) {
        (None, Some(true), None) => Ok(Condition::Amount(AmountTest::MoneyIn)),
        (None, None, Some(true)) => Ok(Condition::Amount(AmountTest::MoneyOut)),
        (Some(field), None, None) => {
            let field = match field {
                "payee" => TextField::Payee,
                "other_party_name" => TextField::OtherPartyName,
                "bank_text" => TextField::RemittanceInformation,
                _ => return Err(ToolError::BadArgument { argument: "field" }),
            };
            let operator = match item.required_string("test")? {
                "contains" => TextOperator::Contains,
                "equals" => TextOperator::Equals,
                "starts_with" => TextOperator::StartsWith,
                "ends_with" => TextOperator::EndsWith,
                _ => return Err(ToolError::BadArgument { argument: "test" }),
            };
            Ok(Condition::Text {
                field,
                operator,
                value: plain_text(item.required_string("text")?, "text")?,
                case_sensitive: item.boolean("case_sensitive")?.unwrap_or(false),
            })
        }
        _ => Err(ToolError::BadArgument {
            argument: "conditions",
        }),
    }
}

/// A name or text a model wrote, as the book will keep it: trimmed, not
/// empty, not too long, and without control characters or characters that
/// hide text (threat T-08).
fn plain_text(text: &str, argument: &'static str) -> Result<String, ToolError> {
    let text = text.trim();
    let bad = text.is_empty()
        || text.chars().count() > MAX_NAME_CHARS
        || text.chars().any(|c| c.is_control() || hides_text(c));
    if bad {
        return Err(ToolError::BadArgument { argument });
    }
    Ok(text.to_owned())
}

/// A JSON object of a call that has only the keys it may have.
struct Object<'a> {
    values: &'a Map<String, Value>,
}

impl<'a> Object<'a> {
    /// `value` as an object with only `keys`; `argument` names it when it
    /// isn't one.
    fn read(value: &'a Value, keys: &[&str], argument: &'static str) -> Result<Self, ToolError> {
        let values = value
            .as_object()
            .filter(|values| values.keys().all(|key| keys.contains(&key.as_str())))
            .ok_or(ToolError::BadArgument { argument })?;
        Ok(Self { values })
    }

    /// The value `name`, if given; null counts as not given.
    fn get(&self, name: &str) -> Option<&'a Value> {
        self.values.get(name).filter(|value| !value.is_null())
    }

    fn string(&self, name: &'static str) -> Result<Option<&'a str>, ToolError> {
        self.get(name)
            .map(|value| {
                value
                    .as_str()
                    .ok_or(ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    fn required_string(&self, name: &'static str) -> Result<&'a str, ToolError> {
        self.string(name)?
            .ok_or(ToolError::BadArgument { argument: name })
    }

    fn boolean(&self, name: &'static str) -> Result<Option<bool>, ToolError> {
        self.get(name)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or(ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    /// A list; not given is an empty one.
    fn array(&self, name: &'static str) -> Result<&'a [Value], ToolError> {
        match self.get(name) {
            None => Ok(&[]),
            Some(Value::Array(items)) => Ok(items),
            Some(_) => Err(ToolError::BadArgument { argument: name }),
        }
    }

    fn uuid(&self, name: &'static str) -> Result<Option<[u8; 16]>, ToolError> {
        self.string(name)?
            .map(|text| {
                uuid::Uuid::try_parse(text)
                    .map(uuid::Uuid::into_bytes)
                    .map_err(|_| ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    fn id(&self, name: &'static str) -> Result<Option<AccountId>, ToolError> {
        Ok(self.uuid(name)?.map(AccountId::from_bytes))
    }
}

#[cfg(test)]
mod tests;
