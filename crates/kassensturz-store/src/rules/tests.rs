// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of Rules in a book and of running them, on books in memory. All
//! data is made up (AGENTS.md §3.1 rule 2); the IBAN is the example for
//! Germany in SWIFT's IBAN Registry (D-055).

use kassensturz_core::{
    AccountKind, DatedRate, DatedRateId, ImportedDetails, RateSource, RuleError,
};
use rusqlite::params;

use super::*;
use crate::tests::{EUR, Sample, USD, account, date, eur, sample, transaction};

/// A purchase as an import (M2) adds it: the account's part with what the
/// bank delivered, and a part without a category yet (D-083). `day` is in
/// October 2026; the 5th is a Monday.
fn imported(
    book: &mut Book,
    account: AccountId,
    minor_units: i64,
    day: i8,
    other_party: &str,
) -> Transaction {
    let delivered = ImportedDetails {
        bank_id: Some(format!("2026-10-{day:02}-{minor_units}-{other_party}")),
        booking_date: Some(date(2026, 10, day)),
        value_date: Some(date(2026, 10, day)),
        other_party_name: Some(other_party.into()),
        other_party_iban: Some("DE89370400440532013000".into()),
        remittance_information: Some(format!("{other_party} card payment")),
    };
    let booked = Split {
        imported: Some(delivered),
        ..Split::new(account, eur(minor_units))
    };
    let transaction = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, day),
        None,
        vec![booked, Split::uncategorized(eur(-minor_units))],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction
}

fn name_contains(value: &str) -> Condition {
    Condition::Text {
        field: TextField::OtherPartyName,
        operator: TextOperator::Contains,
        value: value.into(),
        case_sensitive: false,
    }
}

fn rule(name: &str, conditions: Vec<Condition>, book_to: Option<BookTo>) -> Rule {
    Rule {
        id: RuleId::generate(),
        name: name.into(),
        match_mode: MatchMode::All,
        conditions,
        actions: Actions {
            book_to,
            payee: None,
        },
    }
}

/// Adds a Rule that books to `category` what comes from `other_party`.
fn add(
    book: &mut Book,
    name: &str,
    other_party: &str,
    book_to: Option<BookTo>,
    parent: Option<RuleId>,
) -> Rule {
    let rule = rule(name, vec![name_contains(other_party)], book_to);
    book.add_rule(&rule, parent).expect("Rule added");
    rule
}

/// The category, or account, the part without the account is booked to.
fn other_side(book: &Book, transaction: &Transaction, account: AccountId) -> Option<AccountId> {
    let stored = book.transaction(transaction.id()).expect("read");
    stored
        .splits()
        .iter()
        .find(|split| split.account != Some(account))
        .expect("two parts")
        .account
}

fn names(nodes: &[RuleNode]) -> Vec<String> {
    let mut found = Vec::new();
    for node in nodes {
        found.push(node.rule.name.clone());
        for sub in names(&node.sub_rules) {
            found.push(format!("{}/{sub}", node.rule.name));
        }
    }
    found
}

fn changed(run: &RuleRun) -> Vec<(TransactionId, RuleId)> {
    run.changed
        .iter()
        .map(|change| (change.transaction, change.rule))
        .collect()
}

fn history_length(book: &Book) -> usize {
    book.history().expect("history").len()
}

#[test]
fn rules_are_kept_as_a_tree_in_the_users_order() {
    let Sample {
        mut book,
        groceries,
        ..
    } = sample();
    let before = history_length(&book);
    let food = Some(BookTo::Category(groceries));
    let rewe = add(&mut book, "REWE", "REWE", food, None);
    let lidl = add(&mut book, "Lidl", "Lidl", food, None);
    let drinks = add(&mut book, "Drinks", "Getränke", None, Some(rewe.id));
    let mut detailed = rule(
        "Details",
        vec![
            Condition::Amount(AmountTest::Between {
                min: eur(-10_000),
                max: eur(-1000),
            }),
            Condition::Amount(AmountTest::Equals(eur(-5320))),
            Condition::Amount(AmountTest::MoneyOut),
            Condition::BookingDate {
                first: date(2026, 1, 1),
                last: date(2026, 12, 31),
            },
            Condition::Currency(EUR),
            Condition::Text {
                field: TextField::RemittanceInformation,
                operator: TextOperator::Matches,
                value: r"^\d+ PFAND$".into(),
                case_sensitive: true,
            },
        ],
        None,
    );
    detailed.match_mode = MatchMode::Any;
    detailed.actions.payee = Some("Supermarket".into());
    book.add_rule(&detailed, Some(drinks.id)).expect("added");

    let tree = book.rules().expect("Rules");
    assert_eq!(
        names(&tree),
        ["REWE", "REWE/Drinks", "REWE/Drinks/Details", "Lidl"]
    );
    assert_eq!(tree[0].rule, rewe);
    assert_eq!(tree[1].rule, lidl);
    assert_eq!(tree[0].sub_rules[0].sub_rules[0].rule, detailed);
    assert_eq!(book.rule(detailed.id).expect("read"), detailed);
    assert_eq!(
        history_length(&book),
        before + 4,
        "each Rule is one user action, conditions included"
    );
}

#[test]
fn a_rule_is_checked_before_it_is_added() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let first = add(&mut book, "Groceries", "REWE", None, None);
    let refused = |book: &mut Book, rule: Rule, parent| book.add_rule(&rule, parent).unwrap_err();

    let again = rule(" groceries ", vec![name_contains("Lidl")], None);
    assert!(matches!(
        refused(&mut book, again.clone(), None),
        StoreError::DuplicateRuleName { existing } if existing == first.id
    ));
    // The same name is fine in another place.
    book.add_rule(&again, Some(first.id)).expect("a sub-Rule");
    assert!(matches!(
        refused(
            &mut book,
            rule(" ", vec![name_contains("Lidl")], None),
            None
        ),
        StoreError::EmptyRuleName
    ));
    assert!(matches!(
        refused(&mut book, first.clone(), None),
        StoreError::RuleIdInUse(id) if id == first.id
    ));
    let nowhere = RuleId::generate();
    assert!(matches!(
        refused(&mut book, rule("Lidl", vec![name_contains("Lidl")], None), Some(nowhere)),
        StoreError::UnknownRule(id) if id == nowhere
    ));
    let empty = rule("Lidl", vec![], None);
    assert!(matches!(
        refused(&mut book, empty.clone(), None),
        StoreError::Rule(RuleError::NoConditions(id)) if id == empty.id
    ));
    let into_account = rule(
        "Lidl",
        vec![name_contains("Lidl")],
        Some(BookTo::Category(checking)),
    );
    assert!(matches!(
        refused(&mut book, into_account, None),
        StoreError::RuleNeedsCategory(id) if id == checking
    ));
    let into_category = rule(
        "Lidl",
        vec![name_contains("Lidl")],
        Some(BookTo::Transfer(groceries)),
    );
    assert!(matches!(
        refused(&mut book, into_category, None),
        StoreError::NotAnAccount(id) if id == groceries
    ));
    let on_category = rule("Lidl", vec![Condition::Account(groceries)], None);
    assert!(matches!(
        refused(&mut book, on_category, None),
        StoreError::NotAnAccount(id) if id == groceries
    ));
    let in_dollars = rule("Lidl", vec![Condition::Currency(USD)], None);
    assert!(matches!(
        refused(&mut book, in_dollars, None),
        StoreError::UnknownCurrency(currency) if currency == USD
    ));
    let dollar_amount = rule(
        "Lidl",
        vec![Condition::Amount(AmountTest::Equals(
            Money::from_minor_units(-999, USD),
        ))],
        None,
    );
    assert!(matches!(
        refused(&mut book, dollar_amount, None),
        StoreError::UnknownCurrency(currency) if currency == USD
    ));
    let unknown = AccountId::generate();
    let to_nowhere = rule(
        "Lidl",
        vec![name_contains("Lidl")],
        Some(BookTo::Category(unknown)),
    );
    assert!(matches!(
        refused(&mut book, to_nowhere, None),
        StoreError::UnknownAccount(id) if id == unknown
    ));
    assert_eq!(
        names(&book.rules().expect("Rules")),
        ["Groceries", "Groceries/ groceries "]
    );
}

#[test]
fn a_rule_can_be_changed_in_place() {
    let Sample {
        mut book,
        groceries,
        salary,
        ..
    } = sample();
    let first = add(&mut book, "Shops", "REWE", None, None);
    let sub = add(&mut book, "Food", "Markt", None, Some(first.id));
    add(
        &mut book,
        "Pay",
        "Employer",
        Some(BookTo::Category(salary)),
        None,
    );
    let before = history_length(&book);

    let mut changed = first.clone();
    changed.name = "Supermarkets".into();
    changed.match_mode = MatchMode::Any;
    changed.conditions.push(name_contains("Lidl"));
    changed.actions = Actions {
        book_to: Some(BookTo::Category(groceries)),
        payee: Some("Supermarket".into()),
    };
    book.update_rule(&changed).expect("changed");
    let tree = book.rules().expect("Rules");
    assert_eq!(tree[0].rule, changed);
    assert_eq!(tree[0].sub_rules[0].rule, sub, "its sub-Rules stay");
    assert_eq!(names(&tree)[2], "Pay", "its place stays");
    assert_eq!(history_length(&book), before + 1);

    // Its first condition stayed, so the history copies only what changed.
    let copies: i64 = book
        .conn
        .query_row(
            "SELECT count(*) FROM main.changed_rule_conditions WHERE change_id = (
                 SELECT id FROM main.changes ORDER BY sequence DESC LIMIT 1)",
            [],
            |row| row.get(0),
        )
        .expect("counted");
    assert_eq!(copies, 1);
    // Changing nothing records nothing.
    book.update_rule(&changed).expect("unchanged");
    assert_eq!(history_length(&book), before + 1);

    let missing = rule("Missing", vec![name_contains("x")], None);
    assert!(matches!(
        book.update_rule(&missing),
        Err(StoreError::UnknownRule(id)) if id == missing.id
    ));
    let mut clash = sub.clone();
    clash.name = "food".into();
    let other = add(&mut book, "Other", "Other", None, Some(first.id));
    clash.id = other.id;
    assert!(matches!(
        book.update_rule(&clash),
        Err(StoreError::DuplicateRuleName { existing }) if existing == sub.id
    ));
}

#[test]
fn rules_move_within_their_list_and_into_other_rules() {
    let Sample { mut book, .. } = sample();
    let a = add(&mut book, "A", "a", None, None);
    let b = add(&mut book, "B", "b", None, None);
    let c = add(&mut book, "C", "c", None, None);
    let a1 = add(&mut book, "A1", "a1", None, Some(a.id));

    book.move_rule(c.id, None, 0).expect("moved first");
    assert_eq!(
        names(&book.rules().expect("Rules")),
        ["C", "A", "A/A1", "B"]
    );
    book.move_rule(c.id, None, 99).expect("moved last");
    assert_eq!(
        names(&book.rules().expect("Rules")),
        ["A", "A/A1", "B", "C"]
    );
    book.move_rule(b.id, Some(a.id), 0).expect("moved inside");
    assert_eq!(
        names(&book.rules().expect("Rules")),
        ["A", "A/B", "A/A1", "C"]
    );
    book.move_rule(a.id, None, 1)
        .expect("moved with its sub-Rules");
    assert_eq!(
        names(&book.rules().expect("Rules")),
        ["C", "A", "A/B", "A/A1"]
    );

    for inside in [a.id, a1.id] {
        assert!(matches!(
            book.move_rule(a.id, Some(inside), 0),
            Err(StoreError::RuleInsideItself(id)) if id == a.id
        ));
    }
    let a_again = add(&mut book, "A", "x", None, Some(b.id));
    assert!(matches!(
        book.move_rule(a_again.id, None, 0),
        Err(StoreError::DuplicateRuleName { existing }) if existing == a.id
    ));
    // Undo puts it back where it was.
    book.move_rule(c.id, Some(a.id), 1).expect("moved");
    book.undo().expect("undone");
    assert_eq!(
        names(&book.rules().expect("Rules")),
        ["C", "A", "A/B", "A/B/A", "A/A1"]
    );
}

#[test]
fn removing_a_rule_removes_its_sub_rules() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let shops = add(
        &mut book,
        "Shops",
        "REWE",
        Some(BookTo::Category(groceries)),
        None,
    );
    let sub = add(&mut book, "Sub", "REWE", None, Some(shops.id));
    add(&mut book, "Later", "Lidl", None, None);
    let purchase = imported(&mut book, checking, -5320, 5, "REWE Markt");
    book.run_rules().expect("run");
    assert_eq!(
        book.rule_mark(purchase.id()).expect("mark").map(|m| m.rule),
        Some(Some(sub.id))
    );

    book.remove_rule(shops.id).expect("removed");
    assert_eq!(names(&book.rules().expect("Rules")), ["Later"]);
    assert!(matches!(book.rule(sub.id), Err(StoreError::UnknownRule(_))));
    let conditions: i64 = book
        .conn
        .query_row("SELECT count(*) FROM rule_conditions", [], |row| row.get(0))
        .expect("counted");
    assert_eq!(conditions, 1);
    let position: i64 = book
        .conn
        .query_row("SELECT position FROM rules", [], |row| row.get(0))
        .expect("read");
    assert_eq!(position, 0);
    // What they did stays, but no longer says which Rule it was.
    assert_eq!(other_side(&book, &purchase, checking), Some(groceries));
    assert_eq!(
        book.rule_mark(purchase.id()).expect("mark").map(|m| m.rule),
        Some(None)
    );
    // Undo brings them back, and the mark with them.
    book.undo().expect("undone");
    assert_eq!(
        names(&book.rules().expect("Rules")),
        ["Shops", "Shops/Sub", "Later"]
    );
    assert_eq!(
        book.rule_mark(purchase.id()).expect("mark").map(|m| m.rule),
        Some(Some(sub.id))
    );
    assert!(matches!(
        book.remove_rule(RuleId::generate()),
        Err(StoreError::UnknownRule(_))
    ));
}

#[test]
fn running_the_rules_books_what_has_no_category_yet() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let shops = add(
        &mut book,
        "Shops",
        "REWE",
        Some(BookTo::Category(groceries)),
        None,
    );
    let mut pay = rule(
        "Pay",
        vec![
            name_contains("Example Employer"),
            Condition::Amount(AmountTest::MoneyIn),
        ],
        Some(BookTo::Category(salary)),
    );
    pay.actions.payee = Some("Salary".into());
    book.add_rule(&pay, None).expect("added");
    let purchase = imported(&mut book, checking, -5320, 5, "REWE Markt");
    let wages = imported(&mut book, checking, 240_000, 1, "Example Employer");
    let other = imported(&mut book, checking, -1250, 6, "Bakery");
    let before = history_length(&book);

    let preview = book.preview_rules().expect("preview");
    assert_eq!(
        changed(&preview),
        [(wages.id(), pay.id), (purchase.id(), shops.id)],
        "oldest first"
    );
    assert_eq!(
        other_side(&book, &purchase, checking),
        None,
        "nothing changed"
    );
    assert_eq!(history_length(&book), before);

    let run = book.run_rules().expect("run");
    assert_eq!(changed(&run), changed(&preview));
    assert!(run.skipped.is_empty());
    assert_eq!(other_side(&book, &purchase, checking), Some(groceries));
    assert_eq!(other_side(&book, &wages, checking), Some(salary));
    assert_eq!(other_side(&book, &other, checking), None);
    let paid = book.transaction(wages.id()).expect("read");
    assert_eq!(paid.payee(), Some("Salary"));
    // What the bank delivered stays as it was (D-067).
    assert_eq!(paid.splits()[0], wages.splits()[0]);
    assert_eq!(
        book.balance(checking).expect("balance"),
        eur(240_000 - 5320 - 1250)
    );
    assert_eq!(book.balance(groceries).expect("balance"), eur(5320));

    // One user action, which undo takes back as a whole.
    assert_eq!(history_length(&book), before + 1);
    assert_eq!(
        book.change_to_undo().expect("read").map(|c| c.action),
        Some(Action::RunRules)
    );
    book.undo().expect("undone");
    assert_eq!(book.transaction(purchase.id()).expect("read"), purchase);
    assert_eq!(book.transaction(wages.id()).expect("read"), wages);
    assert_eq!(book.rule_mark(purchase.id()).expect("mark"), None);
    book.redo().expect("redone");
    assert_eq!(other_side(&book, &purchase, checking), Some(groceries));

    // Running them again changes nothing, and records nothing.
    let again = book.run_rules().expect("run");
    assert!(again.changed.is_empty());
    assert_eq!(history_length(&book), before + 1);
}

#[test]
fn manual_choices_win() {
    let Sample {
        mut book,
        checking,
        groceries,
        salary,
    } = sample();
    let household = account(&mut book, "Household", AccountKind::Expense, EUR);
    add(
        &mut book,
        "Shops",
        "REWE",
        Some(BookTo::Category(groceries)),
        None,
    );
    // Categorized by hand when it was entered.
    let by_hand = transaction(
        date(2026, 10, 5),
        "REWE Markt",
        &[(checking, eur(-5320)), (household, eur(5320))],
    );
    book.add_transaction(&by_hand).expect("added");
    // Broken into Parts by the user (D-017), with one Part still open.
    let parts = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 6),
        None,
        vec![
            Split {
                imported: imported(&mut book, checking, -100, 1, "REWE Markt").splits()[0]
                    .imported
                    .clone(),
                ..Split::new(checking, eur(-3000))
            },
            Split::new(household, eur(1000)),
            Split::uncategorized(eur(2000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&parts).expect("added");
    // Categorized by a Rule, then changed by the user.
    let changed_later = imported(&mut book, checking, -999, 7, "REWE Markt");
    book.run_rules().expect("run");
    let mut edited = book.transaction(changed_later.id()).expect("read");
    let splits = edited
        .splits()
        .iter()
        .map(|split| Split {
            account: split
                .account
                .map(|id| if id == groceries { household } else { id }),
            ..split.clone()
        })
        .collect();
    edited = Transaction::new(edited.id(), edited.booking_date(), None, splits).expect("ok");
    book.update_transaction(&edited).expect("edited");
    assert_eq!(book.rule_mark(changed_later.id()).expect("mark"), None);

    // Even a Rule that matches all of them leaves them alone.
    let everything = rule(
        "Everything",
        vec![Condition::Account(checking)],
        Some(BookTo::Category(salary)),
    );
    book.add_rule(&everything, None).expect("added");
    book.move_rule(everything.id, None, 0).expect("first");
    let run = book.run_rules().expect("run");
    let touched: Vec<TransactionId> = run.changed.iter().map(|c| c.transaction).collect();
    assert!(!touched.contains(&by_hand.id()));
    assert!(!touched.contains(&parts.id()));
    assert!(!touched.contains(&changed_later.id()));
    assert_eq!(book.transaction(by_hand.id()).expect("read"), by_hand);
    assert_eq!(book.transaction(parts.id()).expect("read"), parts);
    assert_eq!(book.transaction(changed_later.id()).expect("read"), edited);
}

#[test]
fn running_again_follows_changed_rules() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let household = account(&mut book, "Household", AccountKind::Expense, EUR);
    let mut shops = add(
        &mut book,
        "Shops",
        "REWE",
        Some(BookTo::Category(groceries)),
        None,
    );
    let purchase = imported(&mut book, checking, -5320, 5, "REWE Markt");
    book.run_rules().expect("run");
    assert_eq!(other_side(&book, &purchase, checking), Some(groceries));

    // The user decides REWE is household shopping after all.
    shops.actions.book_to = Some(BookTo::Category(household));
    book.update_rule(&shops).expect("changed");
    let run = book.run_rules().expect("run again");
    assert_eq!(changed(&run), [(purchase.id(), shops.id)]);
    assert_eq!(other_side(&book, &purchase, checking), Some(household));

    // Without a matching Rule, what an earlier one did stays.
    book.remove_rule(shops.id).expect("removed");
    assert!(book.run_rules().expect("run").changed.is_empty());
    assert_eq!(other_side(&book, &purchase, checking), Some(household));
}

#[test]
fn a_rule_can_make_a_transfer_into_an_own_account() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let cash = account(&mut book, "Cash", AccountKind::Asset, EUR);
    let atm = Rule {
        actions: Actions {
            book_to: Some(BookTo::Transfer(cash)),
            payee: Some("Cash withdrawal".into()),
        },
        ..rule("ATM", vec![name_contains("ATM")], None)
    };
    book.add_rule(&atm, None).expect("added");
    let withdrawal = imported(&mut book, checking, -10_000, 5, "ATM Main Street");
    let run = book.run_rules().expect("run");
    assert_eq!(changed(&run), [(withdrawal.id(), atm.id)]);
    assert_eq!(other_side(&book, &withdrawal, checking), Some(cash));
    assert_eq!(book.balance(cash).expect("balance"), eur(10_000));
    assert_eq!(
        book.transaction(withdrawal.id()).expect("read").payee(),
        Some("Cash withdrawal")
    );

    // A Transfer into the account the money comes from goes nowhere.
    let into_checking = Rule {
        actions: Actions {
            book_to: Some(BookTo::Transfer(checking)),
            payee: None,
        },
        ..rule("Back", vec![name_contains("Refund")], None)
    };
    book.add_rule(&into_checking, None).expect("added");
    let refund = imported(&mut book, checking, -500, 6, "Refund desk");
    let run = book.run_rules().expect("run");
    assert!(run.changed.is_empty());
    assert_eq!(run.skipped.len(), 1);
    assert_eq!(run.skipped[0].transaction, refund.id());
    assert!(matches!(
        run.skipped[0].reason,
        StoreError::Transfer(TransferError::SameAccount(id)) if id == checking
    ));
}

#[test]
fn a_rule_that_only_sets_the_payee_leaves_the_category_open() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let mut tidy = rule("Tidy", vec![name_contains("REWE")], None);
    tidy.actions.payee = Some("Supermarket".into());
    book.add_rule(&tidy, None).expect("added");
    let purchase = imported(&mut book, checking, -5320, 5, "REWE Markt");
    book.run_rules().expect("run");
    let read = book.transaction(purchase.id()).expect("read");
    assert_eq!(read.payee(), Some("Supermarket"));
    assert_eq!(other_side(&book, &purchase, checking), None);
    // It is still the Rules' to change: a later Rule can add the category.
    let mark = book
        .rule_mark(purchase.id())
        .expect("mark")
        .expect("marked");
    assert_eq!(mark.split, purchase.splits()[1].id);
    let shops = add(
        &mut book,
        "Shops",
        "REWE",
        Some(BookTo::Category(groceries)),
        None,
    );
    book.move_rule(shops.id, None, 0).expect("first");
    book.run_rules().expect("run");
    assert_eq!(other_side(&book, &purchase, checking), Some(groceries));
}

#[test]
fn sides_of_a_suggested_transfer_wait_for_the_user() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let savings = account(&mut book, "Savings account", AccountKind::Asset, EUR);
    let rule = add(
        &mut book,
        "Bank",
        "Own",
        Some(BookTo::Category(groceries)),
        None,
    );
    let out = imported(&mut book, checking, -20_000, 5, "Own transfer");
    let into = imported(&mut book, savings, 20_000, 6, "Own transfer");
    let run = book.run_rules().expect("run");
    assert!(run.changed.is_empty());
    let skipped: Vec<(TransactionId, RuleId)> = run
        .skipped
        .iter()
        .map(|skip| (skip.transaction, skip.rule))
        .collect();
    assert_eq!(skipped, [(out.id(), rule.id), (into.id(), rule.id)]);
    assert!(matches!(
        run.skipped[0].reason,
        StoreError::TransferSuggested(id) if id == out.id()
    ));
    assert_eq!(other_side(&book, &out, checking), None);
}

#[test]
fn a_category_in_another_currency_gets_the_amount_converted() {
    let Sample {
        mut book, checking, ..
    } = sample();
    book.use_currency(USD).expect("used");
    let travel = account(&mut book, "Travel", AccountKind::Expense, USD);
    let hotel = add(
        &mut book,
        "Hotel",
        "Hotel",
        Some(BookTo::Category(travel)),
        None,
    );
    let bill = imported(&mut book, checking, -9250, 5, "Hotel New York");
    let run = book.run_rules().expect("run");
    assert!(run.changed.is_empty());
    assert!(matches!(
        run.skipped[0].reason,
        StoreError::NoRate { from, to, .. } if from == EUR && to == USD
    ));
    assert_eq!(run.skipped[0].rule, hotel.id);

    let rate = DatedRate {
        id: DatedRateId::generate(),
        date: date(2026, 10, 1),
        from: EUR,
        to: USD,
        rate: "1.0856".parse().expect("valid rate"),
        source: RateSource::Manual,
    };
    book.add_rate(&rate).expect("rate added");
    let run = book.run_rules().expect("run");
    assert_eq!(changed(&run), [(bill.id(), hotel.id)]);
    let booked = book.transaction(bill.id()).expect("read");
    let travel_part = &booked.splits()[1];
    assert_eq!(travel_part.amount, eur(9250));
    assert_eq!(
        travel_part.foreign,
        Some(ForeignAmount {
            amount: Money::from_minor_units(10_042, USD),
            rate: Some(rate),
        })
    );
    assert_eq!(
        book.balance(travel).expect("balance"),
        Money::from_minor_units(10_042, USD)
    );

    // A newer rate doesn't change what is booked already.
    let newer = DatedRate {
        id: DatedRateId::generate(),
        date: date(2026, 10, 2),
        rate: "1.2".parse().expect("valid rate"),
        ..rate
    };
    book.add_rate(&newer).expect("rate added");
    assert!(book.run_rules().expect("run").changed.is_empty());
    assert_eq!(book.transaction(bill.id()).expect("read"), booked);
}

#[test]
fn closed_accounts_stay_as_they_are() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let cash = account(&mut book, "Cash", AccountKind::Asset, EUR);
    book.close_account(cash).expect("closed");
    let atm = add(&mut book, "ATM", "ATM", Some(BookTo::Transfer(cash)), None);
    let withdrawal = imported(&mut book, checking, -10_000, 5, "ATM Main Street");
    let run = book.run_rules().expect("run");
    assert!(matches!(
        run.skipped[0].reason,
        StoreError::AccountClosed(id) if id == cash
    ));
    assert_eq!(other_side(&book, &withdrawal, checking), None);

    // Money a Rule moved into an account that is closed since stays there.
    book.reopen_account(cash).expect("opened");
    book.run_rules().expect("run");
    assert_eq!(other_side(&book, &withdrawal, checking), Some(cash));
    // Moved back by hand, so the account can be closed again.
    let back = transaction(
        date(2026, 10, 7),
        "Back to checking",
        &[(cash, eur(-10_000)), (checking, eur(10_000))],
    );
    book.add_transaction(&back).expect("added");
    book.close_account(cash).expect("closed");
    let mut groceries_instead = atm.clone();
    groceries_instead.actions.book_to = Some(BookTo::Category(groceries));
    book.update_rule(&groceries_instead).expect("changed");
    let run = book.run_rules().expect("run");
    assert!(run.changed.is_empty());
    assert!(matches!(
        run.skipped[0].reason,
        StoreError::AccountClosed(id) if id == cash
    ));
    assert_eq!(other_side(&book, &withdrawal, checking), Some(cash));
}

#[test]
fn the_rules_check_the_payee_the_user_or_a_rule_set() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let bakery = Condition::Text {
        field: TextField::Payee,
        operator: TextOperator::StartsWith,
        value: "corner".into(),
        case_sensitive: false,
    };
    let food = rule("Bakery", vec![bakery], Some(BookTo::Category(groceries)));
    book.add_rule(&food, None).expect("added");
    // Entered by hand, without a category yet (D-083).
    let entered = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        Some("Corner Bakery".into()),
        vec![
            Split::new(checking, eur(-450)),
            Split::uncategorized(eur(450)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&entered).expect("added");
    assert_eq!(
        changed(&book.run_rules().expect("run")),
        [(entered.id(), food.id)]
    );
}

#[test]
fn a_broken_pattern_in_a_book_stops_the_run() {
    let Sample { mut book, .. } = sample();
    let rule = add(&mut book, "Shops", "REWE", None, None);
    book.as_another_program()
        .execute(
            "UPDATE rule_conditions SET operator = 'matches', text_value = 'REWE ('",
            [],
        )
        .expect("changed");
    assert!(matches!(
        book.run_rules(),
        Err(StoreError::Rule(RuleError::BadPattern { rule: id, condition: 0 })) if id == rule.id
    ));
}

#[test]
fn damaged_rules_are_reported() {
    let Sample { mut book, .. } = sample();
    let parent = add(&mut book, "Parent", "a", None, None);
    let child = add(&mut book, "Child", "b", None, Some(parent.id));
    let damage = |sql: &str| {
        book.as_another_program().execute(sql, []).expect("changed");
    };
    // An amount condition with a text.
    damage("UPDATE rule_conditions SET field = 'amount' WHERE position = 0");
    assert!(matches!(
        book.rules(),
        Err(StoreError::Corrupt(Corruption::BadRule(_)))
    ));
    damage("UPDATE rule_conditions SET field = 'other_party_name'");
    book.rules().expect("readable again");
    // A text condition without its text.
    book.as_another_program()
        .execute(
            "UPDATE rule_conditions SET text_value = NULL WHERE rule_id = ?1",
            [parent.id.to_bytes()],
        )
        .expect("changed");
    assert!(matches!(
        book.rule(parent.id),
        Err(StoreError::Corrupt(Corruption::BadRule(id))) if id == parent.id
    ));
    book.as_another_program()
        .execute(
            "UPDATE rule_conditions SET text_value = 'a' WHERE rule_id = ?1",
            [parent.id.to_bytes()],
        )
        .expect("changed");

    // Two Rules inside each other, which no top-level Rule leads to.
    book.as_another_program()
        .execute(
            "UPDATE rules SET parent_id = ?1 WHERE id = ?2",
            params![child.id.to_bytes(), parent.id.to_bytes()],
        )
        .expect("changed");
    let lowest = parent.id.min(child.id);
    assert!(matches!(
        book.rules(),
        Err(StoreError::Corrupt(Corruption::BadRuleTree(id))) if id == lowest
    ));
}
