// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of institutions, payees, tags, and the notes, numbers and status
//! that came with them (D-132), on books in memory. All data is made up
//! (AGENTS.md §3.1 rule 2).

use kassensturz_core::{
    Account, AccountId, AccountKind, PartStatus, Split, Transaction, TransactionId,
};

use super::*;
use crate::tests::{EUR, Sample, account, date, eur, sample};
use crate::{Action, StoreError};

fn bank() -> Institution {
    Institution {
        id: InstitutionId::generate(),
        name: "Example Bank".into(),
        bank_code: Some("EXAMDEFFXXX".into()),
        contact: None,
        address: Some("Example Street 1\n12345 Example City".into()),
        phone: None,
    }
}

fn payee(name: &str, recognition: PayeeRecognition) -> Payee {
    Payee {
        id: PayeeId::generate(),
        name: name.into(),
        address: None,
        email: Some("shop@example.com".into()),
        phone: None,
        reference: Some("Customer 42".into()),
        recognition,
        recognition_ignores_case: true,
    }
}

fn tag(name: &str) -> Tag {
    Tag {
        id: TagId::generate(),
        name: name.into(),
    }
}

#[test]
fn institutions_payees_and_tags_are_kept_and_listed_by_name() {
    let Sample { mut book, .. } = sample();
    let bank = bank();
    book.add_institution(&bank).expect("institution added");
    let grocer = payee("Corner Grocery", PayeeRecognition::ByName);
    let club = payee(
        "Sports Club",
        PayeeRecognition::ByKeyWords(vec!["SPORTS CLUB".into(), "membership".into()]),
    );
    let nobody = payee("Anybody", PayeeRecognition::Never);
    for payee in [&grocer, &club, &nobody] {
        book.add_payee(payee).expect("payee added");
    }
    let holiday = tag("Holiday");
    let birthday = tag("birthday");
    book.add_tag(&holiday).expect("tag added");
    book.add_tag(&birthday).expect("tag added");

    assert_eq!(book.institution(bank.id).expect("read"), bank);
    assert_eq!(book.institutions().expect("read"), vec![bank.clone()]);
    assert_eq!(
        book.payees().expect("read"),
        vec![nobody.clone(), grocer.clone(), club.clone()]
    );
    assert_eq!(
        book.tags().expect("read"),
        vec![birthday.clone(), holiday.clone()]
    );
    let actions: Vec<Action> = book
        .history()
        .expect("history")
        .into_iter()
        .map(|change| change.action)
        .collect();
    // Newest first.
    assert_eq!(
        &actions[..6],
        &[
            Action::AddTag,
            Action::AddTag,
            Action::AddPayee,
            Action::AddPayee,
            Action::AddPayee,
            Action::AddInstitution,
        ]
    );

    // Undo takes the tags and the last payee out again, redo puts the
    // payee back (D-066).
    book.undo().expect("undone");
    book.undo().expect("undone");
    book.undo().expect("undone");
    assert_eq!(book.tags().expect("read"), Vec::new());
    assert_eq!(book.payees().expect("read"), vec![grocer, club]);
    book.redo().expect("redone");
    assert_eq!(book.payees().expect("read").len(), 3);
}

#[test]
fn names_must_be_given_and_not_taken() {
    let Sample { mut book, .. } = sample();
    assert!(matches!(
        book.add_institution(&Institution {
            name: " ".into(),
            ..bank()
        }),
        Err(StoreError::EmptyName)
    ));
    book.add_payee(&payee("Corner Grocery", PayeeRecognition::Never))
        .expect("payee added");
    assert!(matches!(
        book.add_payee(&payee(" corner grocery ", PayeeRecognition::Never)),
        Err(StoreError::DuplicatePayeeName { existing }) if existing == "Corner Grocery"
    ));
    assert!(matches!(
        book.add_payee(&payee("Club", PayeeRecognition::ByKeyWords(vec![]))),
        Err(StoreError::EmptyName)
    ));
    book.add_tag(&tag("Holiday")).expect("tag added");
    assert!(matches!(
        book.add_tag(&tag("HOLIDAY")),
        Err(StoreError::DuplicateTagName { existing }) if existing == "Holiday"
    ));
    assert!(matches!(book.add_tag(&tag("")), Err(StoreError::EmptyName)));
}

#[test]
fn an_account_keeps_its_number_notes_institution_and_group() {
    let Sample { mut book, .. } = sample();
    let bank = bank();
    book.add_institution(&bank).expect("institution added");
    let group = account(&mut book, "Example Bank accounts", AccountKind::Asset, EUR);
    let savings = Account {
        id: AccountId::generate(),
        name: "Savings".into(),
        kind: AccountKind::Asset,
        currency: EUR,
        account_type: None,
        closed: false,
        parent: Some(group),
        number: Some("DE02120300000000202051".into()),
        notes: Some("For the holiday".into()),
        institution: Some(bank.id),
    };
    book.add_account(&savings).expect("account added");
    assert_eq!(book.account(savings.id).expect("read"), savings);
    // It is listed like any account; the group doesn't hide it.
    assert!(
        book.accounts(false)
            .expect("accounts")
            .iter()
            .any(|account| account.id == savings.id)
    );

    // An unknown institution, a group that is a category, or a group of
    // another kind is refused; so is a number on a category.
    let snacks = account(&mut book, "Snacks", AccountKind::Expense, EUR);
    let mut refused = |account: Account| book.add_account(&account).expect_err("refused");
    assert!(matches!(
        refused(Account {
            id: AccountId::generate(),
            name: "Other".into(),
            institution: Some(InstitutionId::generate()),
            ..savings.clone()
        }),
        StoreError::UnknownInstitution(_)
    ));
    assert!(matches!(
        refused(Account {
            id: AccountId::generate(),
            name: "Other".into(),
            parent: Some(snacks),
            ..savings.clone()
        }),
        StoreError::ParentNotAnAccount(parent) if parent == snacks
    ));
    assert!(matches!(
        refused(Account {
            id: AccountId::generate(),
            name: "Card".into(),
            kind: AccountKind::Liability,
            ..savings.clone()
        }),
        StoreError::ParentKindMismatch { .. }
    ));
    assert!(matches!(
        refused(Account {
            id: AccountId::generate(),
            name: "Numbered".into(),
            kind: AccountKind::Expense,
            parent: None,
            institution: None,
            ..savings.clone()
        }),
        StoreError::NotAnAccount(_)
    ));
}

#[test]
fn a_transaction_keeps_its_notes_numbers_status_and_tags() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let holiday = tag("Holiday");
    let food = tag("Food");
    book.add_tag(&holiday).expect("tag added");
    book.add_tag(&food).expect("tag added");
    let shopping = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        Some("Corner Grocery".into()),
        vec![
            Split {
                number: Some("1042".into()),
                status: Some(PartStatus::Reconciled),
                ..Split::new(checking, eur(-5320))
            },
            Split {
                note: Some("Fruit and bread".into()),
                tags: vec![food.id, holiday.id],
                ..Split::new(groceries, eur(5320))
            },
        ],
    )
    .expect("balanced")
    .with_note(Some("Weekly shop".into()))
    .with_entry_date(Some(date(2026, 10, 6)))
    .with_attachments(vec![
        kassensturz_core::Attachment {
            link: "receipts/2026-10-05-grocery.pdf".into(),
            title: Some("Receipt".into()),
        },
        kassensturz_core::Attachment {
            link: "https://example.com/order/4711".into(),
            title: None,
        },
        // Dropped: no link.
        kassensturz_core::Attachment {
            link: "  ".into(),
            title: Some("Nothing".into()),
        },
    ]);
    assert_eq!(shopping.attachments().len(), 2);
    book.add_transaction(&shopping).expect("transaction added");
    assert_eq!(book.transaction(shopping.id()).expect("read"), shopping);
    assert_eq!(book.transactions().expect("read"), vec![shopping.clone()]);

    // A change keeps what the user doesn't touch, and can drop a tag or
    // an attachment.
    let mut parts = shopping.splits().to_vec();
    parts[1].tags = vec![holiday.id];
    let changed = Transaction::new(shopping.id(), date(2026, 10, 5), None, parts)
        .expect("balanced")
        .with_note(Some("Weekly shop".into()))
        .with_attachments(shopping.attachments()[1..].to_vec());
    book.update_transaction(&changed).expect("changed");
    assert_eq!(book.transaction(shopping.id()).expect("read"), changed);
    book.undo().expect("undone");
    assert_eq!(book.transaction(shopping.id()).expect("read"), shopping);

    // An unknown tag, or a tag twice, is refused.
    let stray = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 7),
        None,
        vec![
            Split::new(checking, eur(-100)),
            Split {
                tags: vec![TagId::generate()],
                ..Split::new(groceries, eur(100))
            },
        ],
    )
    .expect("balanced");
    assert!(matches!(
        book.add_transaction(&stray),
        Err(StoreError::UnknownTag(_))
    ));
}
