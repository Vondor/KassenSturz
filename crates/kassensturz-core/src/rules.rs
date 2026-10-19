// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rules: the user's own instructions for categorizing transactions, like an
//! email client's filters (D-014, D-031, D-088).
//!
//! A Rule has conditions, e.g. "the purpose contains REWE", and actions,
//! e.g. "book it to Groceries". Rules form a tree: a sub-Rule is checked
//! only when its parent matched, and refines the parent's actions. Of the
//! top-level Rules, the first one in the user's order that matches decides;
//! later ones aren't checked.
//!
//! This is plain, deterministic code, not AI (`docs/ARCHITECTURE.md`):
//! the same Rules always give the same result. Patterns use the `regex`
//! crate, whose matching takes linear time, so no pattern a user pastes in
//! can hang KassenSturz (D-015).

use regex::{Regex, RegexBuilder};

use crate::Date;
use crate::account::AccountId;
use crate::currency::CurrencyCode;
use crate::id::define_id;
use crate::money::Money;
use crate::transaction::ImportedDetails;

define_id!(
    /// Identifies a Rule within a book.
    RuleId
);

/// Whether a Rule needs all of its conditions or any one of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MatchMode {
    /// Every condition has to hold.
    #[default]
    All,
    /// One condition is enough.
    Any,
}

/// A field of a transaction that holds text, as a condition can test it.
///
/// Together with the amount, the booking date, the currency and the account
/// ([`Condition`]), these are the fields Rules can look at. The list grows
/// as KassenSturz learns more about a transaction (D-014): a new field is a
/// new entry here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextField {
    /// The other party's name as the bank delivered it: who paid, or who
    /// was paid. Shown as "Other party" (D-034).
    OtherPartyName,
    /// The other party's IBAN or account number as the bank delivered it.
    /// Spaces don't count, on either side.
    OtherPartyIban,
    /// The text the payment came with (German: Verwendungszweck). Shown as
    /// "Purpose" (`docs/GLOSSARY.md`).
    RemittanceInformation,
    /// The payee, as the user or an earlier Rule set it.
    Payee,
}

impl TextField {
    /// Every text field, in the order the UI lists them.
    pub const ALL: [Self; 4] = [
        Self::OtherPartyName,
        Self::OtherPartyIban,
        Self::RemittanceInformation,
        Self::Payee,
    ];
}

/// How a text condition compares a field with its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextOperator {
    /// The field contains the value somewhere.
    Contains,
    /// The field is exactly the value.
    Equals,
    /// The field begins with the value.
    StartsWith,
    /// The field ends with the value.
    EndsWith,
    /// The value is a pattern, a regular expression, that matches somewhere
    /// in the field, unless it says otherwise with `^` and `$`. Offered as
    /// an advanced option, never needed for the others (D-015).
    Matches,
}

impl TextOperator {
    /// Every operator, in the order the UI lists them.
    pub const ALL: [Self; 5] = [
        Self::Contains,
        Self::Equals,
        Self::StartsWith,
        Self::EndsWith,
        Self::Matches,
    ];
}

/// How a condition tests the amount: what the transaction does to the
/// account, positive for money coming in, negative for money going out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmountTest {
    /// Exactly this amount, e.g. −9.99 EUR for a subscription.
    Equals(Money),
    /// From `min` up to and including `max`, both in one currency. For
    /// money going out, both are negative: −50.00 to −10.00.
    Between {
        /// The lowest amount that matches.
        min: Money,
        /// The highest amount that matches.
        max: Money,
    },
    /// Any money coming in.
    MoneyIn,
    /// Any money going out.
    MoneyOut,
}

/// One condition of a Rule. A field that a transaction doesn't have, e.g.
/// the purpose of one entered by hand, makes a condition on it fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    /// A text field compared with a value, ignoring capitals unless
    /// `case_sensitive` is set (D-014).
    Text {
        /// The field.
        field: TextField,
        /// How it is compared.
        operator: TextOperator,
        /// The text, or for [`TextOperator::Matches`] the pattern. Never
        /// empty.
        value: String,
        /// Whether capitals count.
        case_sensitive: bool,
    },
    /// The amount.
    Amount(AmountTest),
    /// The booking date, the one the user sees, from `first` up to and
    /// including `last`.
    BookingDate {
        /// The first day that matches.
        first: Date,
        /// The last day that matches.
        last: Date,
    },
    /// The transaction's currency.
    Currency(CurrencyCode),
    /// Which of the user's accounts the money moves in or out of.
    Account(AccountId),
}

/// Where a Rule books the transaction's other side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BookTo {
    /// To a category, e.g. Groceries.
    Category(AccountId),
    /// To another of the user's own accounts, which makes it a Transfer,
    /// e.g. a cash withdrawal into the Cash account (D-031).
    Transfer(AccountId),
}

/// What a Rule does to a transaction it matches (D-031). Either part can be
/// missing: a parent Rule may leave the category to its sub-Rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Actions {
    /// Where the other side is booked.
    pub book_to: Option<BookTo>,
    /// The payee to show, e.g. "Supermarket" for "REWE MARKT 1234 SAGT
    /// DANKE". Never empty.
    pub payee: Option<String>,
}

impl Actions {
    /// These actions refined by a sub-Rule's: each action the sub-Rule has
    /// replaces this Rule's, the others stay (D-014).
    #[must_use]
    pub fn refined_by(self, sub: Self) -> Self {
        Self {
            book_to: sub.book_to.or(self.book_to),
            payee: sub.payee.or(self.payee),
        }
    }

    /// Whether there is nothing to do.
    pub fn is_empty(&self) -> bool {
        self.book_to.is_none() && self.payee.is_none()
    }
}

/// One Rule, without its sub-Rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Its ID.
    pub id: RuleId,
    /// The name the user gave it, e.g. "Groceries".
    pub name: String,
    /// Whether all conditions have to hold, or one is enough.
    pub match_mode: MatchMode,
    /// Its conditions, at least one.
    pub conditions: Vec<Condition>,
    /// What it does to a transaction it matches.
    pub actions: Actions,
}

impl Rule {
    /// Checks the Rule on its own: it has a condition, no text or payee is
    /// empty, every pattern is one KassenSturz can use, and no range ends
    /// before it starts. Names are checked by the book, which knows the
    /// Rule's siblings.
    pub fn check(&self) -> Result<(), RuleError> {
        compile(self).map(|_| ())
    }
}

/// A Rule with its sub-Rules, in the user's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleNode {
    /// The Rule.
    pub rule: Rule,
    /// Its sub-Rules, checked only when it matched.
    pub sub_rules: Vec<RuleNode>,
}

/// A transaction as Rules see it: what moves in or out of one of the user's
/// accounts, and what the bank delivered for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleFacts<'a> {
    /// The user's account.
    pub account: AccountId,
    /// What the transaction does to it, in the transaction's currency.
    pub amount: Money,
    /// The booking date, the one the user sees.
    pub booking_date: Date,
    /// The payee, if any.
    pub payee: Option<&'a str>,
    /// What the bank or file delivered for the account's part, if anything.
    pub imported: Option<&'a ImportedDetails>,
}

impl RuleFacts<'_> {
    fn text(&self, field: TextField) -> Option<&str> {
        let imported = |get: fn(&ImportedDetails) -> &Option<String>| {
            self.imported.and_then(|imported| get(imported).as_deref())
        };
        match field {
            TextField::OtherPartyName => imported(|imported| &imported.other_party_name),
            TextField::OtherPartyIban => imported(|imported| &imported.other_party_iban),
            TextField::RemittanceInformation => {
                imported(|imported| &imported.remittance_information)
            }
            TextField::Payee => self.payee,
        }
    }
}

/// What the Rules do to one transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleOutcome {
    /// The Rules that matched, from the top-level Rule down to the sub-Rule
    /// that decided last. Never empty.
    pub rules: Vec<RuleId>,
    /// The actions, each from the deepest of them that has it.
    pub actions: Actions,
}

impl RuleOutcome {
    /// The Rule that decided: the deepest one that matched.
    pub fn rule(&self) -> RuleId {
        *self.rules.last().expect("at least one Rule matched")
    }
}

/// A tree of Rules ready to be run: checked, with their patterns compiled
/// once, so they can be applied to many transactions.
#[derive(Clone, Debug)]
pub struct RuleSet {
    nodes: Vec<Compiled>,
}

impl RuleSet {
    /// Checks every Rule in `rules` ([`Rule::check`]) and prepares them.
    pub fn new(rules: &[RuleNode]) -> Result<Self, RuleError> {
        Ok(Self {
            nodes: compile_nodes(rules)?,
        })
    }

    /// What the Rules do to the transaction `facts` describes, if any Rule
    /// matches (D-014, D-031).
    ///
    /// The first top-level Rule that matches decides; later ones aren't
    /// checked. Then its sub-Rules are checked in the same way: the first
    /// one that matches refines the actions, and its own sub-Rules refine
    /// them further. A Rule that matches but has no actions, and no
    /// sub-Rule that has any, still decides, so the transaction stays as it
    /// is.
    pub fn evaluate(&self, facts: &RuleFacts<'_>) -> Option<RuleOutcome> {
        let mut rules = Vec::new();
        let mut actions = Actions::default();
        let mut level = &self.nodes;
        while let Some(node) = level.iter().find(|node| node.matches(facts)) {
            rules.push(node.id);
            actions = actions.refined_by(node.actions.clone());
            level = &node.sub_rules;
        }
        (!rules.is_empty()).then_some(RuleOutcome { rules, actions })
    }
}

/// A Rule ready to be checked against transactions.
#[derive(Clone, Debug)]
struct Compiled {
    id: RuleId,
    match_mode: MatchMode,
    conditions: Vec<CompiledCondition>,
    actions: Actions,
    sub_rules: Vec<Compiled>,
}

impl Compiled {
    fn matches(&self, facts: &RuleFacts<'_>) -> bool {
        let mut results = self
            .conditions
            .iter()
            .map(|condition| condition.holds(facts));
        match self.match_mode {
            MatchMode::All => results.all(|holds| holds),
            MatchMode::Any => results.any(|holds| holds),
        }
    }
}

#[derive(Clone, Debug)]
enum CompiledCondition {
    Text { field: TextField, test: TextTest },
    Amount(AmountTest),
    BookingDate { first: Date, last: Date },
    Currency(CurrencyCode),
    Account(AccountId),
}

/// A text comparison, with the value prepared once.
#[derive(Clone, Debug)]
enum TextTest {
    Plain {
        operator: TextOperator,
        /// Lowercase unless capitals count.
        value: String,
        case_sensitive: bool,
    },
    Pattern(Regex),
}

impl CompiledCondition {
    fn holds(&self, facts: &RuleFacts<'_>) -> bool {
        match self {
            Self::Text { field, test } => facts.text(*field).is_some_and(|text| {
                let text = prepare(*field, text);
                match test {
                    TextTest::Pattern(pattern) => pattern.is_match(&text),
                    TextTest::Plain {
                        operator,
                        value,
                        case_sensitive,
                    } => {
                        let text = if *case_sensitive {
                            text
                        } else {
                            text.to_lowercase()
                        };
                        match operator {
                            TextOperator::Contains => text.contains(value.as_str()),
                            TextOperator::Equals => text == *value,
                            TextOperator::StartsWith => text.starts_with(value.as_str()),
                            TextOperator::EndsWith => text.ends_with(value.as_str()),
                            TextOperator::Matches => unreachable!("compiled as a pattern"),
                        }
                    }
                }
            }),
            Self::Amount(test) => {
                let amount = facts.amount;
                let units = amount.minor_units();
                match *test {
                    AmountTest::Equals(value) => value == amount,
                    AmountTest::Between { min, max } => {
                        min.currency() == amount.currency()
                            && (min.minor_units()..=max.minor_units()).contains(&units)
                    }
                    AmountTest::MoneyIn => units > 0,
                    AmountTest::MoneyOut => units < 0,
                }
            }
            Self::BookingDate { first, last } => (*first..=*last).contains(&facts.booking_date),
            Self::Currency(currency) => facts.amount.currency() == *currency,
            Self::Account(account) => facts.account == *account,
        }
    }
}

/// A field's text as conditions compare it: IBANs without spaces, since
/// banks and people write them in groups of four or not at all.
fn prepare(field: TextField, text: &str) -> String {
    match field {
        TextField::OtherPartyIban => text.chars().filter(|c| !c.is_whitespace()).collect(),
        _ => text.to_owned(),
    }
}

fn compile_nodes(nodes: &[RuleNode]) -> Result<Vec<Compiled>, RuleError> {
    nodes
        .iter()
        .map(|node| {
            let mut compiled = compile(&node.rule)?;
            compiled.sub_rules = compile_nodes(&node.sub_rules)?;
            Ok(compiled)
        })
        .collect()
}

/// Checks `rule` and prepares it, without its sub-Rules.
fn compile(rule: &Rule) -> Result<Compiled, RuleError> {
    let id = rule.id;
    if rule.conditions.is_empty() {
        return Err(RuleError::NoConditions(id));
    }
    if rule
        .actions
        .payee
        .as_deref()
        .is_some_and(|payee| payee.trim().is_empty())
    {
        return Err(RuleError::EmptyPayee(id));
    }
    let conditions = rule
        .conditions
        .iter()
        .enumerate()
        .map(|(index, condition)| compile_condition(id, index, condition))
        .collect::<Result<_, _>>()?;
    Ok(Compiled {
        id,
        match_mode: rule.match_mode,
        conditions,
        actions: rule.actions.clone(),
        sub_rules: Vec::new(),
    })
}

fn compile_condition(
    rule: RuleId,
    condition: usize,
    given: &Condition,
) -> Result<CompiledCondition, RuleError> {
    Ok(match given {
        Condition::Text {
            field,
            operator,
            value,
            case_sensitive,
        } => {
            let value = prepare(*field, value);
            if value.trim().is_empty() {
                return Err(RuleError::EmptyText { rule, condition });
            }
            let test = match operator {
                TextOperator::Matches => TextTest::Pattern(
                    RegexBuilder::new(&value)
                        .case_insensitive(!case_sensitive)
                        .build()
                        .map_err(|_| RuleError::BadPattern { rule, condition })?,
                ),
                operator => TextTest::Plain {
                    operator: *operator,
                    value: if *case_sensitive {
                        value
                    } else {
                        value.to_lowercase()
                    },
                    case_sensitive: *case_sensitive,
                },
            };
            CompiledCondition::Text {
                field: *field,
                test,
            }
        }
        Condition::Amount(test) => {
            if let AmountTest::Between { min, max } = test {
                if min.currency() != max.currency() {
                    return Err(RuleError::AmountRangeCurrencies { rule, condition });
                }
                if min.minor_units() > max.minor_units() {
                    return Err(RuleError::AmountRangeReversed { rule, condition });
                }
            }
            CompiledCondition::Amount(*test)
        }
        Condition::BookingDate { first, last } => {
            if first > last {
                return Err(RuleError::DateRangeReversed { rule, condition });
            }
            CompiledCondition::BookingDate {
                first: *first,
                last: *last,
            }
        }
        Condition::Currency(currency) => CompiledCondition::Currency(*currency),
        Condition::Account(account) => CompiledCondition::Account(*account),
    })
}

/// Why a Rule can't be used. `condition` is the position of the condition
/// in the Rule's list, from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleError {
    /// A Rule needs at least one condition; without one, it would match
    /// every transaction.
    NoConditions(RuleId),
    /// A text condition's value is empty or only spaces, which every text
    /// would contain.
    EmptyText {
        /// The Rule.
        rule: RuleId,
        /// Which of its conditions.
        condition: usize,
    },
    /// A pattern isn't a regular expression the `regex` crate accepts, or
    /// it would take too much memory.
    BadPattern {
        /// The Rule.
        rule: RuleId,
        /// Which of its conditions.
        condition: usize,
    },
    /// An amount range ends below where it starts.
    AmountRangeReversed {
        /// The Rule.
        rule: RuleId,
        /// Which of its conditions.
        condition: usize,
    },
    /// The two ends of an amount range are in different currencies.
    AmountRangeCurrencies {
        /// The Rule.
        rule: RuleId,
        /// Which of its conditions.
        condition: usize,
    },
    /// A date range ends before it starts.
    DateRangeReversed {
        /// The Rule.
        rule: RuleId,
        /// Which of its conditions.
        condition: usize,
    },
    /// The payee a Rule sets is empty or only spaces.
    EmptyPayee(RuleId),
}

#[cfg(test)]
mod tests;
