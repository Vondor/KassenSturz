// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of Transfer suggestions and joining, on books in memory. All data
//! is made up (AGENTS.md §3.1 rule 2); the IBANs are the examples in SWIFT's
//! IBAN Registry (D-055).

use kassensturz_core::{
    AccountKind, Actions, BookTo, Condition, ImportedDetails, MatchMode, Money, Rule, RuleId,
    Split, TextField, TextOperator, Transaction, TransferError, TransferSide,
};

use super::*;
use crate::tests::{EUR, Sample, USD, account, date, eur, sample, transaction};

/// One bank's side of a move between accounts, as an import (M2) would add
/// it: the account's part with what the bank delivered, and a part without
/// a category yet (D-083). `day` is in October 2026; the 5th is a Monday.
fn side(book: &mut Book, account: AccountId, minor_units: i64, day: i8) -> Transaction {
    named_side(book, account, minor_units, day, None)
}

/// [`side`] with a payee, e.g. the other party's name an import fills in.
fn named_side(
    book: &mut Book,
    account: AccountId,
    minor_units: i64,
    day: i8,
    payee: Option<&str>,
) -> Transaction {
    let delivered = ImportedDetails {
        bank_id: Some(format!("2026-10-{day:02}-{minor_units}")),
        booking_date: Some(date(2026, 10, day)),
        value_date: Some(date(2026, 10, day)),
        other_party_name: Some("Own transfer".into()),
        other_party_iban: Some("DE89370400440532013000".into()),
        remittance_information: Some("Savings".into()),
    };
    let booked = Split {
        imported: Some(delivered),
        ..Split::new(account, Money::from_minor_units(minor_units, EUR))
    };
    let other = Split::uncategorized(Money::from_minor_units(-minor_units, EUR));
    let transaction = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, day),
        payee.map(str::to_owned),
        vec![booked, other],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction
}

struct Accounts {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    groceries: AccountId,
}

fn accounts() -> Accounts {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let savings = account(&mut book, "Savings account", AccountKind::Asset, EUR);
    Accounts {
        book,
        checking,
        savings,
        groceries,
    }
}

fn pairs(book: &Book) -> Vec<(TransactionId, TransactionId)> {
    book.transfer_suggestions()
        .expect("suggestions")
        .into_iter()
        .map(|found| (found.outgoing.transaction, found.incoming.transaction))
        .collect()
}

#[test]
fn both_sides_of_a_move_between_accounts_are_suggested() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    // Out on Friday, in on Monday: one business day.
    let out = side(&mut book, checking, -20_000, 9);
    let into = side(&mut book, savings, 20_000, 12);
    let suggestions = book.transfer_suggestions().expect("suggestions");
    assert_eq!(suggestions.len(), 1);
    let found = suggestions[0];
    assert_eq!(found.outgoing.transaction, out.id());
    assert_eq!(found.outgoing.account, checking);
    assert_eq!(found.outgoing.amount, eur(-20_000));
    assert_eq!(found.incoming.transaction, into.id());
    assert_eq!(found.incoming.account, savings);
    assert_eq!(found.business_days, 1);
    // Suggesting changes nothing.
    assert_eq!(book.transaction(out.id()).expect("read"), out);
    assert_eq!(book.transaction(into.id()).expect("read"), into);
}

#[test]
fn sides_the_user_sorted_are_not_suggested() {
    let Accounts {
        mut book,
        checking,
        savings,
        groceries,
    } = accounts();
    let out = side(&mut book, checking, -20_000, 5);
    let into = side(&mut book, savings, 20_000, 6);
    // The user said what the other part of the deposit was for, so it is
    // no Transfer anymore: manual choices win (D-031).
    let parts = vec![
        into.splits()[0].clone(),
        Split {
            account: Some(groceries),
            ..into.splits()[1].clone()
        },
    ];
    let categorized = Transaction::new(into.id(), date(2026, 10, 6), None, parts).expect("ok");
    book.update_transaction(&categorized).expect("changed");
    assert_eq!(pairs(&book), Vec::new());

    // Neither are categories, nor transactions that are Transfers already.
    let salary = transaction(
        date(2026, 10, 6),
        "Salary",
        &[(checking, eur(20_000)), (groceries, eur(-20_000))],
    );
    book.add_transaction(&salary).expect("added");
    let moved = transaction(
        date(2026, 10, 6),
        "To savings",
        &[(checking, eur(-20_000)), (savings, eur(20_000))],
    );
    book.add_transaction(&moved).expect("added");
    assert_eq!(pairs(&book), Vec::new());

    // A new deposit fits again.
    let again = side(&mut book, savings, 20_000, 7);
    assert_eq!(pairs(&book), vec![(out.id(), again.id())]);
}

#[test]
fn sides_in_other_currencies_or_too_far_apart_are_not_suggested() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    // Monday and the Friday after: four business days.
    side(&mut book, checking, -20_000, 5);
    side(&mut book, savings, 20_000, 9);
    assert_eq!(pairs(&book), Vec::new());

    book.use_currency(USD).expect("currency used");
    let dollars = account(&mut book, "Dollar account", AccountKind::Asset, USD);
    let deposit = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        None,
        vec![
            Split::new(dollars, Money::from_minor_units(20_000, USD)),
            Split::uncategorized(Money::from_minor_units(-20_000, USD)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&deposit).expect("added");
    assert_eq!(pairs(&book), Vec::new());
}

#[test]
fn a_credit_card_payment_is_a_transfer_too() {
    let Accounts {
        mut book, checking, ..
    } = accounts();
    let card = account(&mut book, "Credit card", AccountKind::Liability, EUR);
    // The payment lowers what the user owes: money flows into the card.
    let into = side(&mut book, card, 30_000, 5);
    let out = side(&mut book, checking, -30_000, 6);
    assert_eq!(pairs(&book), vec![(out.id(), into.id())]);
    book.join_transfer(into.id(), out.id()).expect("joined");
    assert_eq!(book.balance(card).expect("balance"), eur(30_000));
    assert_eq!(book.balance(checking).expect("balance"), eur(-30_000));
}

#[test]
fn joining_makes_one_transfer_that_keeps_what_both_banks_delivered() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    let out = side(&mut book, checking, -20_000, 9);
    let into = side(&mut book, savings, 20_000, 12);

    let joined = book.join_transfer(into.id(), out.id()).expect("joined");
    assert_eq!(joined, out.id());
    let transfer = book.transaction(joined).expect("read");
    assert_eq!(transfer.booking_date(), date(2026, 10, 9));
    // The savings part keeps the day the savings account showed (D-108).
    let savings_part = Split {
        own_date: Some(date(2026, 10, 12)),
        ..into.splits()[0].clone()
    };
    assert_eq!(transfer.splits(), [out.splits()[0].clone(), savings_part]);
    assert!(matches!(
        book.transaction(into.id()),
        Err(StoreError::UnknownTransaction(id)) if id == into.id()
    ));
    // Both registers show the one transaction, each on the day it showed
    // before, and the balances stay.
    let register = |account| {
        book.register(account)
            .expect("register")
            .into_iter()
            .map(|entry| (entry.transaction, entry.booking_date, entry.amount))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        register(checking),
        vec![(joined, date(2026, 10, 9), eur(-20_000))]
    );
    assert_eq!(
        register(savings),
        vec![(joined, date(2026, 10, 12), eur(20_000))]
    );
    assert_eq!(pairs(&book), Vec::new());
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::JoinTransfer
    );

    // Undo brings both back as they were, and redo joins them again.
    book.undo().expect("undone");
    assert_eq!(book.transaction(out.id()).expect("back"), out);
    assert_eq!(book.transaction(into.id()).expect("back"), into);
    assert_eq!(pairs(&book), vec![(out.id(), into.id())]);
    book.redo().expect("redone");
    assert_eq!(book.transaction(joined).expect("read"), transfer);
    assert!(book.transaction(into.id()).is_err());
}

#[test]
fn each_register_keeps_the_date_and_payee_its_bank_delivered() {
    // Issue #44: the checking account paid the credit card on Tuesday the
    // 20th; the card's bank booked it on Wednesday the 21st, from the
    // account holder's name. The card was used for a purchase on the 20th
    // too.
    let Accounts {
        mut book, checking, ..
    } = accounts();
    let card = account(&mut book, "Credit card", AccountKind::Liability, EUR);
    let out = named_side(
        &mut book,
        checking,
        -9_840,
        20,
        Some("Example Card Services"),
    );
    let into = named_side(&mut book, card, 9_840, 21, Some("Alex Example"));
    let purchase = named_side(&mut book, card, -1_250, 20, Some("Corner Grocery"));
    let lines = |book: &Book, account| {
        book.register(account)
            .expect("register")
            .into_iter()
            .map(|entry| {
                (
                    entry.transaction,
                    entry.booking_date,
                    entry.payee,
                    entry.amount,
                )
            })
            .collect::<Vec<_>>()
    };
    let checking_before = lines(&book, checking);
    let card_before = lines(&book, card);

    let joined = book.join_transfer(out.id(), into.id()).expect("joined");
    let transfer = book.transaction(joined).expect("read");
    assert_eq!(
        (transfer.booking_date(), transfer.payee()),
        (date(2026, 10, 20), Some("Example Card Services"))
    );
    // Neither register changes, except that the card's payment is now the
    // Transfer: still on the 21st, after the purchase, from Alex Example.
    let renamed = |lines: Vec<_>| {
        lines
            .into_iter()
            .map(|(id, day, payee, amount)| {
                let id = if id == into.id() { joined } else { id };
                (id, day, payee, amount)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(&book, checking), checking_before);
    assert_eq!(lines(&book, card), renamed(card_before.clone()));
    assert_eq!(
        lines(&book, card),
        vec![
            (
                purchase.id(),
                date(2026, 10, 20),
                Some("Corner Grocery".into()),
                eur(-1_250)
            ),
            (
                joined,
                date(2026, 10, 21),
                Some("Alex Example".into()),
                eur(9_840)
            ),
        ]
    );

    // Undo takes the join back as a whole, the parts' own date and payee
    // included, and redo brings them back.
    book.undo().expect("undone");
    assert_eq!(lines(&book, card), card_before);
    assert_eq!(book.transaction(into.id()).expect("back"), into);
    book.redo().expect("redone");
    assert_eq!(book.transaction(joined).expect("read"), transfer);
}

#[test]
fn joining_keeps_the_incoming_payee_if_the_outgoing_side_has_none() {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    let out = side(&mut book, checking, -20_000, 9);
    let into = side(&mut book, savings, 20_000, 9);
    let named = Transaction::new(
        into.id(),
        into.booking_date(),
        Some("From checking".into()),
        into.splits().to_vec(),
    )
    .expect("balanced");
    book.update_transaction(&named).expect("changed");
    let joined = book.join_transfer(out.id(), into.id()).expect("joined");
    assert_eq!(
        book.transaction(joined).expect("read").payee(),
        Some("From checking")
    );
}

#[test]
fn what_is_no_transfer_is_not_joined() {
    let Accounts {
        mut book,
        checking,
        savings,
        groceries,
    } = accounts();
    let out = side(&mut book, checking, -20_000, 5);
    let less = side(&mut book, savings, 19_999, 5);
    let same_account = side(&mut book, checking, 20_000, 5);
    // Broken into more Parts (D-017); with only one category it would be a
    // side (D-691).
    let shopping = transaction(
        date(2026, 10, 5),
        "Corner Grocery",
        &[
            (savings, eur(20_000)),
            (groceries, eur(-15_000)),
            (groceries, eur(-5_000)),
        ],
    );
    book.add_transaction(&shopping).expect("added");
    let unknown = TransactionId::generate();

    let refused = |book: &mut Book, other: TransactionId| {
        let before = book.history().expect("history").len();
        let result = book.join_transfer(out.id(), other);
        assert_eq!(book.history().expect("history").len(), before);
        result
    };
    assert!(matches!(
        refused(&mut book, less.id()),
        Err(StoreError::Transfer(TransferError::AmountsDiffer { .. }))
    ));
    assert!(matches!(
        refused(&mut book, same_account.id()),
        Err(StoreError::Transfer(TransferError::SameAccount(id))) if id == checking
    ));
    assert!(matches!(
        refused(&mut book, shopping.id()),
        Err(StoreError::Transfer(TransferError::NotASide(id))) if id == shopping.id()
    ));
    assert!(matches!(
        refused(&mut book, out.id()),
        Err(StoreError::Transfer(TransferError::SameTransaction(_)))
    ));
    assert!(matches!(
        refused(&mut book, unknown),
        Err(StoreError::UnknownTransaction(id)) if id == unknown
    ));
    // Nothing changed.
    for kept in [&out, &less, &same_account, &shopping] {
        assert_eq!(&book.transaction(kept.id()).expect("read"), kept);
    }
}

#[test]
fn a_category_is_no_side_of_a_transfer() {
    // A transaction between a category and no category yet: there is no
    // account the money moved out of.
    let Accounts {
        mut book,
        groceries,
        ..
    } = accounts();
    let odd = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        None,
        vec![
            Split::new(groceries, eur(-20_000)),
            Split::uncategorized(eur(20_000)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&odd).expect("added");
    let accounts = book.account_ids().expect("accounts");
    assert_eq!(
        TransferSide::of(&odd, |id| accounts.contains(&id)),
        Err(TransferError::NotASide(odd.id()))
    );
    assert_eq!(pairs(&book), Vec::new());
}

/// A Rule that puts every side [`side`] adds ("Own transfer") in
/// `category`, added to the book.
fn own_transfer_rule(book: &mut Book, category: AccountId) -> Rule {
    let rule = Rule {
        id: RuleId::generate(),
        name: "Own transfer".into(),
        match_mode: MatchMode::All,
        conditions: vec![Condition::Text {
            field: TextField::OtherPartyName,
            operator: TextOperator::Contains,
            value: "Own transfer".into(),
            case_sensitive: false,
        }],
        actions: Actions {
            book_to: Some(BookTo::Category(category)),
            payee: None,
        },
    };
    book.add_rule(&rule, None).expect("Rule added");
    rule
}

/// A side whose other part a Rule put in a category: the checking
/// account's side of a standing order, sorted before the savings account's
/// statement came in (#594, D-691).
struct RuleSorted {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    plan: AccountId,
    rule: Rule,
    out: Transaction,
}

fn rule_sorted() -> RuleSorted {
    let Accounts {
        mut book,
        checking,
        savings,
        ..
    } = accounts();
    let plan = account(&mut book, "Savings plan", AccountKind::Expense, EUR);
    let rule = own_transfer_rule(&mut book, plan);
    let out = side(&mut book, checking, -20_000, 5);
    book.run_rules().expect("Rules ran");
    let out = book.transaction(out.id()).expect("read");
    RuleSorted {
        book,
        checking,
        savings,
        plan,
        rule,
        out,
    }
}

#[test]
fn a_side_a_rule_sorted_is_suggested_and_one_sorted_by_hand_is_not() {
    let RuleSorted {
        mut book,
        savings,
        plan,
        rule,
        out,
        ..
    } = rule_sorted();
    let side_of = |book: &Book, id: TransactionId| {
        let accounts = book.account_ids().expect("accounts");
        TransferSide::of(&book.transaction(id).expect("read"), |a| {
            accounts.contains(&a)
        })
        .expect("a side")
    };
    let sorted = side_of(&book, out.id());
    assert_eq!(sorted.category, Some(plan));
    assert_eq!(
        book.side_sorting(&sorted).expect("sorting"),
        SideSorting::ByRule(Some(rule.id))
    );

    // The savings account's side comes in later, without a category.
    let into = side(&mut book, savings, 20_000, 6);
    assert_eq!(pairs(&book), vec![(out.id(), into.id())]);
    // The Rules leave it for the user to decide, and say so.
    let run = book.run_rules().expect("Rules ran");
    assert!(run.changed.is_empty());
    assert!(run.skipped.iter().any(|skip| skip.transaction == into.id()
        && matches!(skip.reason, StoreError::TransferSuggested(id) if id == into.id())));
    assert_eq!(book.transaction(into.id()).expect("read"), into);

    // Once the user changes the sorted side by hand, it is theirs (D-031)
    // and no longer suggested.
    book.update_transaction(&out).expect("changed");
    assert_eq!(
        book.side_sorting(&side_of(&book, out.id()))
            .expect("sorting"),
        SideSorting::ByHand
    );
    assert_eq!(pairs(&book), Vec::new());
}

#[test]
fn open_sides_keep_their_suggestion_when_a_rule_sorted_side_is_closer() {
    let RuleSorted {
        mut book,
        checking,
        savings,
        out: ruled,
        ..
    } = rule_sorted();
    // The Rule-sorted side takes 200 out of checking on Monday the 5th. An
    // open side puts 200 into savings the same day, and an open one takes
    // 200 out of checking on Wednesday the 7th: the open pair wins, though
    // the Rule-sorted side is closer (D-691).
    let into = side(&mut book, savings, 20_000, 5);
    let out = side(&mut book, checking, -20_000, 7);
    assert_eq!(pairs(&book), vec![(out.id(), into.id())]);
    // A second open side into savings on Tuesday is closer to the open
    // side out, so it takes that one, and the Rule-sorted side gets the
    // side left over.
    let second = side(&mut book, savings, 20_000, 6);
    assert_eq!(
        pairs(&book),
        vec![(ruled.id(), into.id()), (out.id(), second.id())]
    );
}

#[test]
fn joining_a_rule_sorted_side_keeps_both_accounts_parts_and_undo_brings_the_category_back() {
    let RuleSorted {
        mut book,
        checking,
        savings,
        plan,
        rule,
        out,
    } = rule_sorted();
    let into = side(&mut book, savings, 20_000, 6);
    let mark = book
        .rule_mark(out.id())
        .expect("mark")
        .expect("a Rule booked it");
    assert_eq!(mark.rule, Some(rule.id));
    let account_part = |transaction: &Transaction, account: AccountId| {
        transaction
            .splits()
            .iter()
            .find(|split| split.account == Some(account))
            .expect("the account's part")
            .clone()
    };

    let joined = book.join_transfer(into.id(), out.id()).expect("joined");
    assert_eq!(joined, out.id());
    let transfer = book.transaction(joined).expect("read");
    // Both accounts' parts are as their banks delivered them; the category
    // and the Rule's mark went with their part.
    assert_eq!(
        transfer.splits(),
        [
            account_part(&out, checking),
            Split {
                own_date: Some(date(2026, 10, 6)),
                own_payee: None,
                ..account_part(&into, savings)
            }
        ]
    );
    assert!(
        transfer
            .splits()
            .iter()
            .all(|split| split.account != Some(plan))
    );
    assert_eq!(book.rule_mark(joined).expect("mark"), None);
    assert!(matches!(
        book.transaction(into.id()),
        Err(StoreError::UnknownTransaction(_))
    ));
    assert_eq!(
        book.history().expect("history")[0].action,
        Action::JoinTransfer
    );
    assert_eq!(pairs(&book), Vec::new());

    // One undo brings both sides back, the category and the Rule's mark
    // too, so the Rules may sort that side again as before.
    book.undo().expect("undone").expect("something to undo");
    assert_eq!(book.transaction(out.id()).expect("read"), out);
    assert_eq!(book.transaction(into.id()).expect("read"), into);
    assert_eq!(book.rule_mark(out.id()).expect("mark"), Some(mark));
    assert_eq!(pairs(&book), vec![(out.id(), into.id())]);
}

#[test]
fn the_user_may_pick_any_side_with_the_opposite_amount() {
    let Accounts {
        mut book,
        checking,
        savings,
        groceries,
    } = accounts();
    let card = account(&mut book, "Credit card", AccountKind::Liability, EUR);
    let out = side(&mut book, checking, -20_000, 7);
    // A side into savings two weeks later, which the user sorted by hand.
    let late = side(&mut book, savings, 20_000, 21);
    let sorted_late = Transaction::new(
        late.id(),
        late.booking_date(),
        None,
        vec![
            late.splits()[0].clone(),
            Split {
                account: Some(groceries),
                ..late.splits()[1].clone()
            },
        ],
    )
    .expect("balanced");
    book.update_transaction(&sorted_late).expect("changed");
    // One into the card the next day, without a category.
    let near = side(&mut book, card, 20_000, 8);
    // Not partners: the same account, another amount, the same direction.
    side(&mut book, checking, 20_000, 7);
    side(&mut book, savings, 19_999, 7);
    side(&mut book, savings, -20_000, 7);

    let partners = book.transfer_partners(out.id()).expect("partners");
    let listed: Vec<(TransactionId, SideSorting, u32, bool)> = partners
        .iter()
        .map(|partner| {
            (
                partner.side.transaction,
                partner.sorting,
                partner.business_days,
                partner.likely,
            )
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            (near.id(), SideSorting::Open, 1, true),
            (late.id(), SideSorting::ByHand, 10, false),
        ]
    );
    // The hand-sorted side was never suggested, but the user may join it.
    assert!(pairs(&book).iter().all(|(_, b)| *b != late.id()));
    book.join_transfer(out.id(), late.id()).expect("joined");

    // A transaction that can't be a side has no partners.
    let shopping = transaction(
        date(2026, 10, 9),
        "Corner Grocery",
        &[
            (checking, eur(-3_000)),
            (groceries, eur(2_000)),
            (groceries, eur(1_000)),
        ],
    );
    book.add_transaction(&shopping).expect("added");
    assert!(matches!(
        book.transfer_partners(shopping.id()),
        Err(StoreError::Transfer(TransferError::NotASide(id))) if id == shopping.id()
    ));
}
