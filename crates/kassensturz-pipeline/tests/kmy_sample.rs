// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The KMyMoney sample file all the way through: read by the importer,
//! filled into a book, exported, read back and filled again (D-132). The
//! sample is randomized data the maintainer approved (AGENTS.md §9 rule 8).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_core::{PartStatus, PayeeRecognition};
use kassensturz_export::{BookExport, ExportData, export, read_book};
use kassensturz_import::{Imported, Kmy, Limits, Skipped, SkippedData, import};
use kassensturz_pipeline::{export_book, fill_book, fill_book_from};
use kassensturz_store::Book;

fn sample() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy/kassensturz-sample.kmy");
    fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_kmymoney_sample_keeps_its_details_through_a_book_and_its_export() {
    let Imported::Book(imported) =
        import(&Kmy, &sample(), &Limits::default()).expect("the sample reads")
    else {
        panic!("a .kmy file is a book");
    };
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    assert_eq!(report.transactions, 538);
    assert_eq!(
        report.skipped,
        vec![
            Skipped {
                what: SkippedData::Rates,
                count: 20,
            },
            Skipped {
                what: SkippedData::OnlineBanking,
                count: 2,
            },
        ]
    );

    // The bank, and the accounts held there with their numbers, grouped as
    // the file grouped them.
    let banks = book.institutions().expect("banks");
    assert_eq!(banks.len(), 1);
    assert_eq!(banks[0].name, "Lumsi Bank");
    assert_eq!(banks[0].bank_code.as_deref(), Some("37040044"));
    let accounts = book.all_accounts().expect("accounts");
    let named = |name: &str| {
        accounts
            .iter()
            .find(|account| account.name == name)
            .unwrap_or_else(|| panic!("{name} is in the book"))
    };
    let checking = named("Konto Wodübex");
    assert_eq!(checking.number.as_deref(), Some("0532013000"));
    assert_eq!(checking.institution, Some(banks[0].id));
    assert_eq!(checking.parent, Some(named("Besitz Wofenn").id));
    assert_eq!(
        accounts
            .iter()
            .filter(|account| account.institution.is_some())
            .count(),
        2
    );

    // Every payee with how KMyMoney recognized it; the file has no tags.
    let payees = book.payees().expect("payees");
    assert_eq!(payees.len(), 131);
    assert!(payees.iter().all(|payee| {
        payee.recognition == PayeeRecognition::ByName && payee.recognition_ignores_case
    }));
    assert_eq!(book.tags().expect("tags"), Vec::new());

    // The memos, entry dates and reconciled parts.
    let transactions = book.transactions().expect("transactions");
    assert_eq!(transactions.len(), 538);
    assert!(
        transactions
            .iter()
            .all(|transaction| transaction.entered_on().is_some())
    );
    assert_eq!(
        transactions
            .iter()
            .filter(|transaction| transaction.note().is_some())
            .count(),
        526
    );
    assert_eq!(
        transactions
            .iter()
            .flat_map(|transaction| transaction.splits())
            .filter(|part| part.status == Some(PartStatus::Reconciled))
            .count(),
        73
    );
    let purchase = transactions
        .iter()
        .find(|transaction| transaction.payee() == Some("Marröndt Sidrox"))
        .expect("a purchase");
    assert_eq!(purchase.note(), Some("Rosra waln 958366"));
    // The account's part keeps its memo as the bank's text, as before
    // (D-083), not as a note of its own.
    let in_checking = purchase
        .splits()
        .iter()
        .find(|part| part.account == Some(checking.id))
        .expect("the part in the account");
    assert_eq!(in_checking.note, None);
    assert_eq!(
        in_checking
            .imported
            .as_ref()
            .and_then(|details| details.remittance_information.as_deref()),
        Some("Rosra waln 958366")
    );

    // All of it comes back from the complete export, into a book that
    // writes the same file again.
    let data = match export_book(&book).expect("gathered") {
        ExportData::Book(data) => data,
        _ => panic!("a book was gathered"),
    };
    assert_eq!(data.institutions, banks);
    assert_eq!(data.payees, payees);
    let file = export(&BookExport, &ExportData::Book(data.clone())).expect("written");
    let read = read_book(&file).expect("read back");
    assert_eq!(read, data);
    let mut copy = Book::create_in_memory().expect("book created");
    fill_book(&mut copy, &read).expect("filled");
    let again = export(&BookExport, &export_book(&copy).expect("gathered")).expect("written");
    assert_eq!(again, file);
}
