// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rules on made-up transactions. The IBAN is the example for Germany in
//! SWIFT's IBAN Registry, never one that could belong to someone (D-055).

use super::*;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};
const IBAN: &str = "DE89370400440532013000";

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn october(day: i8) -> Date {
    Date::new(2026, 10, day).expect("valid date")
}

/// A card payment of 53.20 at a supermarket, as a bank delivers it.
fn delivered() -> ImportedDetails {
    ImportedDetails {
        bank_id: Some("2026100500017".into()),
        booking_date: Some(october(5)),
        value_date: Some(october(5)),
        other_party_name: Some("REWE Markt GmbH".into()),
        other_party_iban: Some(IBAN.into()),
        remittance_information: Some("REWE SAGT DANKE 1234".into()),
    }
}

fn facts<'a>(account: AccountId, imported: Option<&'a ImportedDetails>) -> RuleFacts<'a> {
    RuleFacts {
        account,
        amount: eur(-5320),
        booking_date: october(5),
        payee: None,
        imported,
    }
}

fn text(field: TextField, operator: TextOperator, value: &str) -> Condition {
    Condition::Text {
        field,
        operator,
        value: value.into(),
        case_sensitive: false,
    }
}

fn rule(conditions: Vec<Condition>, book_to: Option<BookTo>) -> Rule {
    Rule {
        id: RuleId::generate(),
        name: "Groceries".into(),
        match_mode: MatchMode::All,
        conditions,
        actions: Actions {
            book_to,
            payee: None,
        },
    }
}

fn leaf(rule: Rule) -> RuleNode {
    RuleNode {
        rule,
        sub_rules: Vec::new(),
    }
}

fn run(rules: &[RuleNode], facts: &RuleFacts<'_>) -> Option<RuleOutcome> {
    RuleSet::new(rules).expect("valid Rules").evaluate(facts)
}

/// Whether the one condition holds for the supermarket payment.
fn holds(condition: Condition) -> bool {
    let details = delivered();
    let account = AccountId::generate();
    let found = run(
        &[leaf(rule(vec![condition], None))],
        &facts(account, Some(&details)),
    );
    found.is_some()
}

#[test]
fn text_operators_ignore_capitals_by_default() {
    use TextField::{OtherPartyName as Name, RemittanceInformation as Purpose};
    use TextOperator::*;
    assert!(holds(text(Purpose, Contains, "sagt danke")));
    assert!(holds(text(Name, Equals, "rewe markt gmbh")));
    assert!(holds(text(Name, StartsWith, "Rewe")));
    assert!(holds(text(Purpose, EndsWith, "1234")));
    assert!(!holds(text(Name, Equals, "REWE")));
    assert!(!holds(text(Name, StartsWith, "Markt")));
    assert!(!holds(text(Purpose, EndsWith, "DANKE")));
    assert!(!holds(text(Purpose, Contains, "Lidl")));
}

#[test]
fn capitals_count_when_asked() {
    let condition = |value: &str, operator| Condition::Text {
        field: TextField::OtherPartyName,
        operator,
        value: value.into(),
        case_sensitive: true,
    };
    assert!(holds(condition("REWE", TextOperator::Contains)));
    assert!(!holds(condition("rewe", TextOperator::Contains)));
    assert!(holds(condition("^REWE", TextOperator::Matches)));
    assert!(!holds(condition("^rewe", TextOperator::Matches)));
}

#[test]
fn patterns_match_anywhere_unless_anchored() {
    use TextOperator::Matches;
    let purpose = TextField::RemittanceInformation;
    assert!(holds(text(purpose, Matches, r"danke \d{4}")));
    assert!(holds(text(purpose, Matches, r"^rewe .* \d+$")));
    assert!(!holds(text(purpose, Matches, r"^danke")));
}

#[test]
fn ibans_are_compared_without_spaces() {
    let iban = TextField::OtherPartyIban;
    assert!(holds(text(
        iban,
        TextOperator::Equals,
        "DE89 3704 0044 0532 0130 00"
    )));
    let details = ImportedDetails {
        other_party_iban: Some("de89 3704 0044 0532 0130 00".into()),
        ..delivered()
    };
    let rules = [leaf(rule(
        vec![text(iban, TextOperator::Equals, IBAN)],
        None,
    ))];
    assert!(run(&rules, &facts(AccountId::generate(), Some(&details))).is_some());
}

#[test]
fn a_missing_field_fails_its_condition() {
    // Entered by hand: nothing delivered, and no payee.
    let rules = [leaf(rule(
        vec![text(
            TextField::RemittanceInformation,
            TextOperator::Contains,
            "REWE",
        )],
        None,
    ))];
    assert_eq!(run(&rules, &facts(AccountId::generate(), None)), None);
    let payee = [leaf(rule(
        vec![text(TextField::Payee, TextOperator::Contains, "bakery")],
        None,
    ))];
    let mut entered = facts(AccountId::generate(), None);
    assert_eq!(run(&payee, &entered), None);
    entered.payee = Some("Corner Bakery");
    assert!(run(&payee, &entered).is_some());
}

#[test]
fn amounts_are_signed_and_exact() {
    assert!(holds(Condition::Amount(AmountTest::Equals(eur(-5320)))));
    assert!(!holds(Condition::Amount(AmountTest::Equals(eur(5320)))));
    assert!(!holds(Condition::Amount(AmountTest::Equals(
        Money::from_minor_units(-5320, USD)
    ))));
    let between = |min, max| {
        Condition::Amount(AmountTest::Between {
            min: eur(min),
            max: eur(max),
        })
    };
    assert!(holds(between(-10_000, -1000)));
    assert!(holds(between(-5320, -5320)));
    assert!(!holds(between(1000, 10_000)));
    assert!(!holds(between(-5319, 0)));
    assert!(holds(Condition::Amount(AmountTest::MoneyOut)));
    assert!(!holds(Condition::Amount(AmountTest::MoneyIn)));
}

#[test]
fn zero_is_neither_money_in_nor_out() {
    let rules = |test| [leaf(rule(vec![Condition::Amount(test)], None))];
    let mut zero = facts(AccountId::generate(), None);
    zero.amount = eur(0);
    assert_eq!(run(&rules(AmountTest::MoneyIn), &zero), None);
    assert_eq!(run(&rules(AmountTest::MoneyOut), &zero), None);
}

#[test]
fn dates_currencies_and_accounts() {
    let dates = |first, last| Condition::BookingDate {
        first: october(first),
        last: october(last),
    };
    assert!(holds(dates(1, 31)));
    assert!(holds(dates(5, 5)));
    assert!(!holds(dates(6, 31)));
    assert!(holds(Condition::Currency(EUR)));
    assert!(!holds(Condition::Currency(USD)));

    let checking = AccountId::generate();
    let rules = [leaf(rule(vec![Condition::Account(checking)], None))];
    assert!(run(&rules, &facts(checking, None)).is_some());
    assert_eq!(run(&rules, &facts(AccountId::generate(), None)), None);
}

#[test]
fn all_needs_every_condition_and_any_one() {
    let details = delivered();
    let facts = facts(AccountId::generate(), Some(&details));
    let mut groceries = rule(
        vec![
            text(TextField::OtherPartyName, TextOperator::Contains, "REWE"),
            Condition::Amount(AmountTest::MoneyIn),
        ],
        None,
    );
    assert_eq!(run(&[leaf(groceries.clone())], &facts), None);
    groceries.match_mode = MatchMode::Any;
    assert!(run(&[leaf(groceries)], &facts).is_some());
}

#[test]
fn the_first_matching_top_level_rule_wins() {
    let details = delivered();
    let facts = facts(AccountId::generate(), Some(&details));
    let (groceries, household) = (AccountId::generate(), AccountId::generate());
    let lidl = rule(
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "Lidl",
        )],
        Some(BookTo::Category(AccountId::generate())),
    );
    let rewe = rule(
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "REWE",
        )],
        Some(BookTo::Category(groceries)),
    );
    let card = rule(
        vec![Condition::Amount(AmountTest::MoneyOut)],
        Some(BookTo::Category(household)),
    );
    let rules = [leaf(lidl), leaf(rewe.clone()), leaf(card)];
    let outcome = run(&rules, &facts).expect("a Rule matches");
    assert_eq!(outcome.rules, vec![rewe.id]);
    assert_eq!(outcome.rule(), rewe.id);
    assert_eq!(outcome.actions.book_to, Some(BookTo::Category(groceries)));
}

#[test]
fn a_matching_sub_rule_refines_its_parent() {
    let details = delivered();
    let facts = facts(AccountId::generate(), Some(&details));
    let (food, groceries, drinks) = (
        AccountId::generate(),
        AccountId::generate(),
        AccountId::generate(),
    );
    let mut parent = rule(
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "REWE",
        )],
        Some(BookTo::Category(food)),
    );
    parent.actions.payee = Some("Supermarket".into());
    let not_this = rule(
        vec![text(
            TextField::RemittanceInformation,
            TextOperator::Contains,
            "PFAND",
        )],
        Some(BookTo::Category(drinks)),
    );
    let this = rule(
        vec![Condition::Amount(AmountTest::MoneyOut)],
        Some(BookTo::Category(groceries)),
    );
    let tree = [RuleNode {
        rule: parent.clone(),
        sub_rules: vec![leaf(not_this), leaf(this.clone())],
    }];
    let outcome = run(&tree, &facts).expect("the parent matches");
    assert_eq!(outcome.rules, vec![parent.id, this.id]);
    // The category comes from the sub-Rule, the payee from the parent.
    assert_eq!(
        outcome.actions,
        Actions {
            book_to: Some(BookTo::Category(groceries)),
            payee: Some("Supermarket".into()),
        }
    );
}

#[test]
fn without_a_matching_sub_rule_the_parent_decides() {
    let details = delivered();
    let facts = facts(AccountId::generate(), Some(&details));
    let food = AccountId::generate();
    let parent = rule(
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "REWE",
        )],
        Some(BookTo::Category(food)),
    );
    let sub = rule(
        vec![Condition::Amount(AmountTest::MoneyIn)],
        Some(BookTo::Category(AccountId::generate())),
    );
    let tree = [RuleNode {
        rule: parent.clone(),
        sub_rules: vec![leaf(sub)],
    }];
    let outcome = run(&tree, &facts).expect("the parent matches");
    assert_eq!(outcome.rules, vec![parent.id]);
    assert_eq!(outcome.actions.book_to, Some(BookTo::Category(food)));
}

#[test]
fn a_sub_rule_is_only_checked_when_its_parent_matched() {
    let details = delivered();
    let facts = facts(AccountId::generate(), Some(&details));
    let parent = rule(
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "Lidl",
        )],
        None,
    );
    let sub = rule(
        vec![Condition::Amount(AmountTest::MoneyOut)],
        Some(BookTo::Category(AccountId::generate())),
    );
    let tree = [RuleNode {
        rule: parent,
        sub_rules: vec![leaf(sub)],
    }];
    assert_eq!(run(&tree, &facts), None);
}

#[test]
fn sub_rules_nest_to_any_depth() {
    let details = delivered();
    let facts = facts(AccountId::generate(), Some(&details));
    let out = || vec![Condition::Amount(AmountTest::MoneyOut)];
    let deepest = AccountId::generate();
    let mut node = leaf(rule(out(), Some(BookTo::Category(deepest))));
    let mut ids = vec![node.rule.id];
    for _ in 0..5 {
        let parent = rule(out(), Some(BookTo::Category(AccountId::generate())));
        ids.insert(0, parent.id);
        node = RuleNode {
            rule: parent,
            sub_rules: vec![node],
        };
    }
    let outcome = run(&[node], &facts).expect("all match");
    assert_eq!(outcome.rules, ids);
    assert_eq!(outcome.actions.book_to, Some(BookTo::Category(deepest)));
}

#[test]
fn a_matching_rule_without_actions_still_decides() {
    // "Leave my landlord's payments alone": later Rules aren't checked.
    let details = delivered();
    let facts = facts(AccountId::generate(), Some(&details));
    let leave_alone = rule(
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "REWE",
        )],
        None,
    );
    let later = rule(
        vec![Condition::Amount(AmountTest::MoneyOut)],
        Some(BookTo::Category(AccountId::generate())),
    );
    let outcome = run(&[leaf(leave_alone.clone()), leaf(later)], &facts).expect("a match");
    assert_eq!(outcome.rule(), leave_alone.id);
    assert!(outcome.actions.is_empty());
}

#[test]
fn rules_that_cannot_work_are_refused() {
    let check = |conditions, payee: Option<&str>| {
        let mut checked = rule(conditions, None);
        checked.actions.payee = payee.map(str::to_owned);
        let id = checked.id;
        (checked.check(), id)
    };
    let (result, id) = check(vec![], None);
    assert_eq!(result, Err(RuleError::NoConditions(id)));

    let name = TextField::OtherPartyName;
    let (result, id) = check(
        vec![
            Condition::Amount(AmountTest::MoneyOut),
            text(name, TextOperator::Contains, "  "),
        ],
        None,
    );
    assert_eq!(
        result,
        Err(RuleError::EmptyText {
            rule: id,
            condition: 1
        })
    );
    let (result, id) = check(vec![text(name, TextOperator::Matches, "REWE (")], None);
    assert_eq!(
        result,
        Err(RuleError::BadPattern {
            rule: id,
            condition: 0
        })
    );
    // Backreferences need a backtracking engine, which `regex` leaves out
    // on purpose (D-015).
    let (result, id) = check(vec![text(name, TextOperator::Matches, r"(a)\1")], None);
    assert_eq!(
        result,
        Err(RuleError::BadPattern {
            rule: id,
            condition: 0
        })
    );
    let (result, id) = check(
        vec![Condition::Amount(AmountTest::Between {
            min: eur(-1000),
            max: eur(-5000),
        })],
        None,
    );
    assert_eq!(
        result,
        Err(RuleError::AmountRangeReversed {
            rule: id,
            condition: 0
        })
    );
    let (result, id) = check(
        vec![Condition::Amount(AmountTest::Between {
            min: eur(-5000),
            max: Money::from_minor_units(-1000, USD),
        })],
        None,
    );
    assert_eq!(
        result,
        Err(RuleError::AmountRangeCurrencies {
            rule: id,
            condition: 0
        })
    );
    let (result, id) = check(
        vec![Condition::BookingDate {
            first: october(6),
            last: october(5),
        }],
        None,
    );
    assert_eq!(
        result,
        Err(RuleError::DateRangeReversed {
            rule: id,
            condition: 0
        })
    );
    let (result, id) = check(vec![Condition::Amount(AmountTest::MoneyOut)], Some(" "));
    assert_eq!(result, Err(RuleError::EmptyPayee(id)));
}

#[test]
fn a_rule_set_checks_its_sub_rules_too() {
    let broken = rule(vec![], None);
    let tree = [RuleNode {
        rule: rule(vec![Condition::Amount(AmountTest::MoneyOut)], None),
        sub_rules: vec![leaf(broken.clone())],
    }];
    assert_eq!(
        RuleSet::new(&tree).map(|_| ()),
        Err(RuleError::NoConditions(broken.id))
    );
}

#[test]
fn a_pasted_pattern_cannot_hang_the_app() {
    // Catastrophic for a backtracking engine; linear time for `regex`.
    let details = ImportedDetails {
        remittance_information: Some(format!("{}!", "a".repeat(10_000))),
        ..delivered()
    };
    let rules = [leaf(rule(
        vec![text(
            TextField::RemittanceInformation,
            TextOperator::Matches,
            "^(a+)+$",
        )],
        None,
    ))];
    assert_eq!(
        run(&rules, &facts(AccountId::generate(), Some(&details))),
        None
    );
}
