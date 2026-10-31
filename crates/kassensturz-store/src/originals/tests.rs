// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that what a bank or file delivered stays as delivered (D-375), on
//! books in memory. All data is made up (AGENTS.md §3.1 rule 2); the IBAN is
//! the example for Germany in SWIFT's IBAN Registry (D-055).

use kassensturz_core::{AccountId, ImportedDetails, Split, Transaction, TransactionId};
use rusqlite::params;

use super::*;
use crate::tests::{Sample, date, eur, sample};
use crate::{Action, Book};

fn delivered() -> ImportedDetails {
    ImportedDetails {
        bank_id: Some("2026100500017".into()),
        booking_date: Some(date(2026, 10, 5)),
        value_date: Some(date(2026, 10, 6)),
        other_party_name: Some("CORNER GROCERY 0815".into()),
        other_party_iban: Some("DE89370400440532013000".into()),
        remittance_information: Some("Card payment 2026-10-04".into()),
    }
}

/// A card payment as an import adds it: the checking account's part with
/// what the bank delivered, and a part without a category yet.
fn imported_purchase(book: &mut Book, checking: AccountId) -> Transaction {
    let bank_part = Split {
        imported: Some(delivered()),
        ..Split::new(checking, eur(-5320))
    };
    let purchase = Transaction::new(
        TransactionId::generate(),
        date(2026, 10, 5),
        Some("Corner Grocery".into()),
        vec![bank_part, Split::uncategorized(eur(5320))],
    )
    .expect("balanced");
    book.add_transaction(&purchase).expect("added");
    purchase
}

fn count(book: &Book, table: &str) -> i64 {
    book.conn
        .query_row(&format!("SELECT count(*) FROM main.{table}"), [], |row| {
            row.get(0)
        })
        .expect("rows counted")
}

/// Whether `sql`, run by a program other than KassenSturz, was refused by
/// one of layout 23's triggers.
fn refused(book: &Book, sql: &str, id: [u8; 16]) -> bool {
    let error = book
        .as_another_program()
        .execute(sql, [id])
        .expect_err("refused");
    matches!(StoreError::from(error), StoreError::DeliveredKept)
}

#[test]
fn another_program_can_neither_change_nor_remove_an_original() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let bank_part = purchase.splits()[0].id.to_bytes();
    let transaction = purchase.id().to_bytes();
    for sql in [
        "UPDATE split_imports SET other_party_name = 'SOMEONE ELSE' WHERE split_id = ?1",
        "UPDATE split_imports SET bank_id = bank_id WHERE split_id = ?1",
        "DELETE FROM split_imports WHERE split_id = ?1",
        "DELETE FROM splits WHERE id = ?1",
        "UPDATE splits SET amount = -5300 WHERE id = ?1",
        "UPDATE splits SET account_id = NULL WHERE id = ?1",
        "UPDATE splits SET foreign_amount = -5320, foreign_currency = 'USD' WHERE id = ?1",
    ] {
        assert!(refused(&book, sql, bank_part), "{sql}");
    }
    for sql in [
        "DELETE FROM transactions WHERE id = ?1",
        "UPDATE transactions SET currency = 'USD' WHERE id = ?1",
    ] {
        assert!(refused(&book, sql, transaction), "{sql}");
    }
    assert_eq!(book.transaction(purchase.id()).expect("kept"), purchase);
}

#[test]
fn what_the_user_changes_sits_next_to_the_original() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let other = purchase.splits()[1].id.to_bytes();
    let bank_part = purchase.splits()[0].id.to_bytes();
    // Another program may still change what is the user's: the payee, a
    // note, the category of the other part, the part's own payee.
    let program = book.as_another_program();
    program
        .execute(
            "UPDATE transactions SET payee = 'Groceries', note = 'weekly shop' WHERE id = ?1",
            [purchase.id().to_bytes()],
        )
        .expect("payee changed");
    program
        .execute(
            "UPDATE splits SET own_payee = 'Corner shop', note = 'milk' WHERE id = ?1",
            [bank_part],
        )
        .expect("own payee changed");
    program
        .execute(
            "UPDATE splits SET account_id = ?2 WHERE id = ?1",
            params![other, groceries.to_bytes()],
        )
        .expect("category set");
    drop(program);
    let changed = book.transaction(purchase.id()).expect("read");
    assert_eq!(changed.payee(), Some("Groceries"));
    assert_eq!(changed.splits()[0].imported, Some(delivered()));
    assert_eq!(changed.splits()[0].account_amount(), eur(-5320));
}

#[test]
fn the_user_removing_a_transaction_takes_its_original_and_undo_brings_it_back() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    book.remove_transaction(purchase.id()).expect("removed");
    assert_eq!(count(&book, "split_imports"), 0);
    assert_eq!(count(&book, "originals_unlocked"), 0);
    book.undo().expect("undone").expect("a change");
    assert_eq!(book.transaction(purchase.id()).expect("back"), purchase);
    assert_eq!(count(&book, "originals_unlocked"), 0);
    book.redo().expect("redone").expect("a change");
    assert_eq!(count(&book, "split_imports"), 0);
    book.undo().expect("undone").expect("a change");
    assert_eq!(book.transaction(purchase.id()).expect("back"), purchase);
}

#[test]
fn editing_a_transaction_keeps_its_original() {
    let Sample {
        mut book,
        checking,
        groceries,
        ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let mut parts = purchase.splits().to_vec();
    parts[1].account = Some(groceries);
    parts[0].own_payee = Some("Corner shop".into());
    let edited = Transaction::new(
        purchase.id(),
        date(2026, 10, 7),
        Some("Groceries".into()),
        parts,
    )
    .expect("balanced");
    book.update_transaction(&edited).expect("edited");
    assert_eq!(book.transaction(purchase.id()).expect("read"), edited);
    assert_eq!(count(&book, "originals_unlocked"), 0);
    // Undo and redo write the parts back through the guard too.
    book.undo().expect("undone").expect("a change");
    assert_eq!(book.transaction(purchase.id()).expect("read"), purchase);
    book.redo().expect("redone").expect("a change");
    assert_eq!(book.transaction(purchase.id()).expect("read"), edited);
}

#[test]
fn a_write_that_loses_an_original_is_taken_back() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let id = purchase.id();
    // A slip in an edit: the parts are deleted but not written again.
    let lost = book.as_one_action(Action::EditTransaction, |book| {
        keeping_originals(&book.conn, &[id], || {
            book.conn.execute(
                "DELETE FROM splits WHERE transaction_id = ?1",
                [id.to_bytes()],
            )?;
            Ok(())
        })
    });
    assert!(matches!(
        lost,
        Err(StoreError::ImportedSplitChanged(split)) if split == purchase.splits()[0].id
    ));
    // Written again with other delivered details: refused as well.
    let mut parts = purchase.splits().to_vec();
    parts[0].imported = Some(ImportedDetails {
        other_party_name: Some("SOMEONE ELSE".into()),
        ..delivered()
    });
    let changed = Transaction::new(id, purchase.booking_date(), None, parts).expect("balanced");
    let rewritten = book.as_one_action(Action::EditTransaction, |book| {
        keeping_originals(&book.conn, &[id], || {
            book.conn.execute(
                "DELETE FROM splits WHERE transaction_id = ?1",
                [id.to_bytes()],
            )?;
            crate::transactions::insert_splits(&book.conn, &changed)
        })
    });
    assert!(matches!(
        rewritten,
        Err(StoreError::ImportedSplitChanged(_))
    ));
    assert_eq!(book.transaction(id).expect("kept"), purchase);
    assert_eq!(count(&book, "originals_unlocked"), 0);
}

#[test]
fn a_slip_inside_kassensturz_is_refused_with_its_own_error() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let bank_part = purchase.splits()[0].id.to_bytes();
    let changed = book.as_one_action(Action::EditTransaction, |book| {
        book.conn.execute(
            "UPDATE split_imports SET remittance_information = 'changed' WHERE split_id = ?1",
            [bank_part],
        )?;
        Ok::<_, StoreError>(())
    });
    assert!(matches!(changed, Err(StoreError::DeliveredKept)));
    let removed = book.as_one_action(Action::EditTransaction, |book| {
        book.conn
            .execute("DELETE FROM splits WHERE id = ?1", [bank_part])?;
        Ok::<_, StoreError>(())
    });
    assert!(matches!(removed, Err(StoreError::DeliveredKept)));
    assert_eq!(book.transaction(purchase.id()).expect("kept"), purchase);
}

#[test]
fn the_guard_never_stays_open_after_a_failure() {
    let Sample {
        mut book, checking, ..
    } = sample();
    let purchase = imported_purchase(&mut book, checking);
    let failed = unlocked(&book.conn, || Err::<(), _>(StoreError::AmountOverflow));
    assert!(matches!(failed, Err(StoreError::AmountOverflow)));
    assert_eq!(count(&book, "originals_unlocked"), 0);
    assert!(refused(
        &book,
        "DELETE FROM split_imports WHERE split_id = ?1",
        purchase.splits()[0].id.to_bytes()
    ));
}
