// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of bank links (Q-086, D-148) on books in memory. The IBANs are the
//! examples from SWIFT's IBAN Registry (D-055); all data is made up
//! (AGENTS.md §3.1 rule 2).

use kassensturz_core::{AccountKind, BankLink};

use crate::tests::{EUR, Sample, account, date, sample};
use crate::{Action, StoreError};

const GERMAN_EXAMPLE: &str = "DE89370400440532013000";
const AUSTRIAN_EXAMPLE: &str = "AT611904300234573201";

fn by_iban(account: kassensturz_core::AccountId, iban: &str) -> BankLink {
    BankLink {
        account,
        iban: Some(iban.into()),
        bank_code: None,
        number: None,
        last_fetched_on: None,
    }
}

fn by_number(account: kassensturz_core::AccountId, bank_code: &str, number: &str) -> BankLink {
    BankLink {
        account,
        iban: None,
        bank_code: Some(bank_code.into()),
        number: Some(number.into()),
        last_fetched_on: None,
    }
}

#[test]
fn a_link_is_kept_changed_removed_and_each_step_can_be_undone() {
    let Sample {
        mut book, checking, ..
    } = sample();
    assert_eq!(book.bank_link(checking).expect("read"), None);
    assert_eq!(book.bank_links().expect("read"), Vec::new());

    let link = by_iban(checking, GERMAN_EXAMPLE);
    book.link_bank_account(&link).expect("linked");
    assert_eq!(book.bank_link(checking).expect("read"), Some(link.clone()));
    assert_eq!(book.bank_links().expect("read"), vec![link.clone()]);
    let history = book.history().expect("history");
    assert_eq!(history[0].action, Action::LinkBankAccount);

    // Linking again changes the link; the fetch day is what the caller says.
    let card = BankLink {
        last_fetched_on: Some(date(2026, 10, 20)),
        ..by_number(checking, "37040044", "4111 1111 1111 1111")
    };
    book.link_bank_account(&card).expect("changed");
    let kept = book.bank_link(checking).expect("read").expect("linked");
    assert_eq!(kept.iban, None);
    assert_eq!(kept.bank_code.as_deref(), Some("37040044"));
    assert_eq!(kept.number.as_deref(), Some("4111 1111 1111 1111"));
    assert_eq!(kept.last_fetched_on, Some(date(2026, 10, 20)));

    book.unlink_bank_account(checking).expect("unlinked");
    assert_eq!(book.bank_link(checking).expect("read"), None);
    let history = book.history().expect("history");
    assert_eq!(history[0].action, Action::UnlinkBankAccount);
    // The currency and three accounts, then the link, its change, the unlink.
    assert_eq!(history.len(), 4 + 3);

    // Unlinking an account without a link changes nothing.
    book.unlink_bank_account(checking).expect("nothing to do");
    assert_eq!(book.history().expect("history").len(), 7);

    book.undo().expect("undone");
    assert_eq!(book.bank_link(checking).expect("read"), Some(kept));
    book.undo().expect("undone");
    assert_eq!(book.bank_link(checking).expect("read"), Some(link));
    book.undo().expect("undone");
    assert_eq!(book.bank_link(checking).expect("read"), None);
    book.redo().expect("redone");
    assert_eq!(
        book.bank_link(checking)
            .expect("read")
            .map(|link| link.iban),
        Some(Some(GERMAN_EXAMPLE.into()))
    );
    // The account itself is as it was.
    assert_eq!(
        book.account(checking).expect("account").name,
        "Checking account"
    );
}

#[test]
fn a_link_names_a_bank_account_of_an_account_the_user_sees() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    // A category has no bank account.
    assert!(matches!(
        book.link_bank_account(&by_iban(groceries, GERMAN_EXAMPLE)),
        Err(StoreError::NotAnAccount(id)) if id == groceries
    ));
    let gone = kassensturz_core::AccountId::generate();
    assert!(matches!(
        book.link_bank_account(&by_iban(gone, GERMAN_EXAMPLE)),
        Err(StoreError::UnknownAccount(id)) if id == gone
    ));
    assert!(matches!(
        book.bank_link(gone),
        Err(StoreError::UnknownAccount(id)) if id == gone
    ));
    // Without an IBAN, the bank code and the number are both needed, and
    // none of them may be empty.
    for incomplete in [
        BankLink {
            account: checking,
            iban: None,
            bank_code: None,
            number: None,
            last_fetched_on: None,
        },
        by_number(checking, "37040044", " "),
        by_number(checking, "", "4111111111111111"),
        by_iban(checking, ""),
        by_iban(checking, "DE89-3704-0044-0532-0130-00"),
    ] {
        assert!(
            matches!(
                book.link_bank_account(&incomplete),
                Err(StoreError::IncompleteBankLink(id)) if id == checking
            ),
            "{incomplete:?}"
        );
    }
    assert_eq!(book.bank_link(checking).expect("read"), None);
    // Nothing was recorded: the currency and the three accounts only.
    assert_eq!(book.history().expect("history").len(), 4);
}

#[test]
fn the_iban_is_kept_without_spaces_and_in_capitals() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let typed = BankLink {
        bank_code: Some(" 37040044 ".into()),
        ..by_iban(checking, "de89 3704 0044 0532 0130 00")
    };
    book.link_bank_account(&typed).expect("linked");
    let kept = book.bank_link(checking).expect("read").expect("linked");
    assert_eq!(kept.iban.as_deref(), Some(GERMAN_EXAMPLE));
    assert_eq!(kept.bank_code.as_deref(), Some("37040044"));
    assert_eq!(kept.number, None);
}

#[test]
fn no_two_accounts_are_linked_to_the_same_bank_account() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let savings = account(&mut book, "Savings", AccountKind::Asset, EUR);
    let card = account(&mut book, "Credit card", AccountKind::Liability, EUR);
    book.link_bank_account(&by_iban(checking, GERMAN_EXAMPLE))
        .expect("linked");
    book.link_bank_account(&by_number(card, "37040044", "4111111111111111"))
        .expect("linked");
    // The same IBAN, however it is typed, and the same number at the same
    // bank are taken; the same number at another bank isn't.
    assert!(matches!(
        book.link_bank_account(&by_iban(savings, "de89 3704 0044 0532 0130 00")),
        Err(StoreError::BankAccountAlreadyLinked { existing }) if existing == checking
    ));
    assert!(matches!(
        book.link_bank_account(&by_number(savings, "37040044", "4111111111111111")),
        Err(StoreError::BankAccountAlreadyLinked { existing }) if existing == card
    ));
    book.link_bank_account(&by_number(savings, "50010517", "4111111111111111"))
        .expect("another bank");
    // An account can be linked to the same bank account again, e.g. to
    // add its bank code.
    book.link_bank_account(&BankLink {
        bank_code: Some("37040044".into()),
        ..by_iban(checking, GERMAN_EXAMPLE)
    })
    .expect("linked again");
    book.link_bank_account(&by_iban(savings, AUSTRIAN_EXAMPLE))
        .expect("relinked");
    let links = book.bank_links().expect("read");
    assert_eq!(links.len(), 3);
    let mut ids: Vec<_> = links.iter().map(|link| link.account).collect();
    ids.sort();
    assert_eq!(
        links.iter().map(|link| link.account).collect::<Vec<_>>(),
        ids
    );
}

#[test]
fn a_fetch_notes_its_day_on_the_link() {
    let Sample {
        mut book, checking, ..
    } = sample();
    assert!(matches!(
        book.note_bank_fetch(checking, date(2026, 10, 23)),
        Err(StoreError::NotLinked(id)) if id == checking
    ));
    book.link_bank_account(&by_iban(checking, GERMAN_EXAMPLE))
        .expect("linked");
    book.note_bank_fetch(checking, date(2026, 10, 23))
        .expect("noted");
    let link = book.bank_link(checking).expect("read").expect("linked");
    assert_eq!(link.last_fetched_on, Some(date(2026, 10, 23)));
    let history = book.history().expect("history");
    assert_eq!(history[0].action, Action::FetchStatement);
    // Within an import, the day is part of that one change.
    let before = history.len();
    book.as_one_action(Action::ImportStatement, |book| {
        book.note_bank_fetch(checking, date(2026, 10, 24))
    })
    .expect("imported");
    let history = book.history().expect("history");
    assert_eq!(history.len(), before + 1);
    assert_eq!(history[0].action, Action::ImportStatement);
    book.undo().expect("undone");
    assert_eq!(
        book.bank_link(checking)
            .expect("read")
            .expect("linked")
            .last_fetched_on,
        Some(date(2026, 10, 23))
    );
    book.undo().expect("undone");
    assert_eq!(
        book.bank_link(checking)
            .expect("read")
            .expect("linked")
            .last_fetched_on,
        None
    );
}
