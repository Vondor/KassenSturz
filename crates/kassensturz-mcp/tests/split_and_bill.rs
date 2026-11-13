// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! #717 as an automated test instead of a check by hand: through the real
//! program, the user's AI tool finds a payment by the reference of its
//! bill, splits it into parts that add up to the bank amount, links the
//! bill from the bill folder, and the app applies the set after the check
//! card; one Undo brings back the state before. The app's side is the real
//! request folder, the real check (`prepare_named_with_bills`) and the real
//! apply with `Confirmed`. Synthetic data only: the shop, the invoice
//! numbers and the files are invented.

#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use common::{CHECKING, EUR, Folder, Session};
use kassensturz_ai_tools::requests::{FOLDER_NAME, Inbox, Reply, RequestFolder};
use kassensturz_ai_tools::{Confirmed, prepare_named_with_bills};
use kassensturz_core::ai_switch::permissions_in;
use kassensturz_core::{
    AccountId, AccountKind, Date, ImportedDetails, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Action, Book};
use serde_json::json;

const OFFICE: AccountId = AccountId::from_bytes([7; 16]);
const DEBIT: TransactionId = TransactionId::from_bytes([9; 16]);

/// A book with a debit of 54.10 whose bank text names two invoices, and a
/// second debit of the same amount without a reference.
fn book_with_debit(folder: &Folder) -> PathBuf {
    let path = folder.book();
    let mut book = Book::open(&path, &folder.computer()).expect("open");
    book.add_account(&common::account(
        OFFICE,
        "Office supplies",
        AccountKind::Expense,
    ))
    .expect("account");
    let mut paid = Split::new(CHECKING, Money::from_minor_units(-5_410, EUR));
    paid.imported = Some(ImportedDetails {
        remittance_information: Some("Rechnung INV-5501 und INV-5502 Kundennr 9".into()),
        ..ImportedDetails::default()
    });
    book.add_transaction(
        &Transaction::new(
            DEBIT,
            Date::constant(2026, 10, 20),
            Some("Papierladen Nord".into()),
            vec![
                paid,
                Split::uncategorized(Money::from_minor_units(5_410, EUR)),
            ],
        )
        .expect("balanced"),
    )
    .expect("added");
    book.add_transaction(&common::transaction(
        5,
        Date::constant(2026, 10, 21),
        "Versand Sued",
        &[(CHECKING, -5_410), (OFFICE, 5_410)],
    ))
    .expect("added");
    path
}

fn bill_folder(folder: &Folder) -> PathBuf {
    let bills = folder.path().join("Bills");
    std::fs::create_dir_all(bills.join("2026")).expect("folder");
    for file in ["2026/INV-5501.pdf", "2026/INV-5502.pdf"] {
        std::fs::write(bills.join(file), b"%PDF-1.4 synthetic").expect("file");
    }
    bills
}

fn request_folder(folder: &Folder) -> PathBuf {
    folder
        .data()
        .join("io.github.vondor.KassenSturz")
        .join(FOLDER_NAME)
}

/// Plays the running app: shows the check card, then applies the set to
/// the book as the user's Confirm does.
fn app(
    folder: &Path,
    book: &Path,
    computer: kassensturz_store::Computer,
    bills: PathBuf,
    settings: &Path,
    done: Arc<AtomicBool>,
) {
    let (folder, book) = (folder.to_path_buf(), book.to_path_buf());
    let settings = std::fs::read_to_string(settings).expect("settings");
    std::thread::spawn(move || {
        let mut inbox = RequestFolder::open(folder).expect("the app's folder");
        while !done.load(Ordering::SeqCst) {
            for request in inbox.take_new(&book) {
                // The user's own settings, as the app reads them.
                let allowed = permissions_in(&settings);
                let mut writable = Book::open(&book, &computer).expect("the app's book");
                let proposal = prepare_named_with_bills(
                    &writable,
                    request.function,
                    &request.arguments,
                    allowed,
                    Date::constant(2026, 10, 31),
                    Some(&bills),
                )
                .expect("proposed");
                inbox
                    .reply(
                        &request.id,
                        &Reply::Shown {
                            changes: proposal.changes.changes().len(),
                        },
                    )
                    .expect("answered");
                std::thread::sleep(Duration::from_millis(200));
                proposal
                    .changes
                    .apply(&mut writable, Confirmed::by_the_user())
                    .expect("applied");
                inbox.reply(&request.id, &Reply::Applied).expect("answered");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

#[test]
fn the_ai_finds_splits_and_links_a_bill_and_one_undo_takes_it_back() {
    let folder = Folder::new("split-and-bill");
    let bills = bill_folder(&folder);
    folder.allow(&[
        "AllowSplitTransactions=true",
        "AllowAttachBills=true",
        "LockAmount=false",
        "LockCategory=false",
        &format!("BillFolder={}", bills.display()),
    ]);
    let book_path = book_with_debit(&folder);
    let before = Book::open_to_read(&book_path)
        .expect("read")
        .transaction(DEBIT)
        .expect("debit");
    let done = Arc::new(AtomicBool::new(false));
    app(
        &request_folder(&folder),
        &book_path,
        folder.computer(),
        bills,
        &folder.path().join("kassensturzrc"),
        Arc::clone(&done),
    );
    let mut session = Session::start(folder.path(), &book_path);
    session.initialize("2025-06-18");

    // 1. Finds the payment by the bill's reference, not by amount.
    let (answer, is_error) = session.call("search_transactions", json!({"reference": "inv 5501"}));
    assert!(!is_error, "{answer}");
    assert_eq!(answer["matched_by"], "reference", "{answer}");
    let rows = answer["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "{answer}");
    assert_eq!(rows[0]["transaction_id"], DEBIT.to_string(), "{answer}");
    // A reference that matches nothing is "not found", never a guess by amount.
    let (answer, _) = session.call("search_transactions", json!({"reference": "INV-9999"}));
    assert_eq!(answer["found"], json!(false), "{answer}");

    // 2. The parts must add up to the bank amount: a split that is off by a
    // cent is refused with the difference, and nothing reaches the app.
    let parts = |first: &str| {
        json!([
            {"category_id": OFFICE.to_string(), "amount": first, "name": "Invoice INV-5501"},
            {"amount": "-24.10", "name": "Invoice INV-5502"}
        ])
    };
    let bills_call = json!([
        {"transaction_id": DEBIT.to_string(), "file": "2026/INV-5501.pdf"},
        {"transaction_id": DEBIT.to_string(), "file": "2026/INV-5502.pdf"}
    ]);
    let call = |first: &str| {
        json!({
            "split_transactions": [{"transaction_id": DEBIT.to_string(), "parts": parts(first)}],
            "attach_bills": bills_call.clone(),
        })
    };
    let mut off = call("-30.01");
    off["check_only"] = json!(true);
    let (answer, is_error) = session.call("propose_changes", off);
    assert!(is_error, "{answer}");
    assert_eq!(answer["error"], "ai-tools.parts-dont-add-up", "{answer}");
    assert_eq!(answer["difference"], "0.01", "{answer}");

    // 3. A file outside the bill folder is refused.
    let mut outside = call("-30.00");
    outside["attach_bills"][0]["file"] = json!("../Household.ksz");
    let (answer, is_error) = session.call("propose_changes", outside);
    assert!(is_error, "{answer}");
    assert_eq!(
        Book::open_to_read(&book_path)
            .expect("read")
            .transaction(DEBIT)
            .expect("debit"),
        before,
        "a refused call changes nothing"
    );

    // 4. The right call is shown on the check card and applied on confirm.
    let (answer, is_error) = session.call("propose_changes", call("-30.00"));
    done.store(true, Ordering::SeqCst);
    assert!(!is_error, "{answer}");
    assert_eq!(answer["status"], "applied", "{answer}");
    let mut book = Book::open(&book_path, &folder.computer()).expect("open");
    let after = book.transaction(DEBIT).expect("debit");
    assert_eq!(after.splits().len(), 3);
    assert_eq!(after.attachments().len(), 2);
    let own = after
        .splits()
        .iter()
        .find(|split| split.account == Some(CHECKING))
        .expect("the bank's part");
    assert_eq!(own.amount, Money::from_minor_units(-5_410, EUR));
    let sum: i64 = after.splits().iter().map(|s| s.amount.minor_units()).sum();
    assert_eq!(sum, 0, "the entry balances");
    assert!(
        after
            .splits()
            .iter()
            .any(|s| s.account == Some(OFFICE) && s.amount.minor_units() == 3_000)
    );
    assert!(bills_still_there(&folder));
    let history = book.history().expect("history");
    assert_eq!(history[0].action, Action::ApplyAiChanges, "one entry");

    // 5. One Undo brings back the state before.
    book.undo().expect("undo").expect("a change");
    assert_eq!(book.transaction(DEBIT).expect("debit"), before);
    assert!(session.finish().status.success());
}

fn bills_still_there(folder: &Folder) -> bool {
    ["2026/INV-5501.pdf", "2026/INV-5502.pdf"]
        .iter()
        .all(|file| folder.path().join("Bills").join(file).is_file())
}
