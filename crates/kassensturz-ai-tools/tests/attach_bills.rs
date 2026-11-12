// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's own AI tool links a bill from the bill folder to a
//! transaction (US5.13, D-739, `docs/AI_SPLITS.md` §3, threat T-62):
//! behind its own cell, only with a bill folder, only a PDF, PNG or JPEG
//! file inside that folder once `..` and links are resolved, never one the
//! transaction has, and with a split in the same set as one Undo. The
//! link is part of the complete export. Synthetic files only.

#![cfg(feature = "ai")]

use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{
    BILL_FOLDER, Confirmed, PROPOSE_CHANGES, ProposedChange, ToolError, bill_folder_answer,
    prepare_named, prepare_named_with_bills, propose_spec,
};
use kassensturz_core::ai_switch::{Field, Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, Attachment, CurrencyCode, Date, Money,
    Report as _, Split, Transaction, TransactionId,
};
use kassensturz_export::{BookExport, ExportData, export};
use kassensturz_pipeline::export_book;
use kassensturz_store::{Action, Book};
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 11, 12);
const DAY: Date = Date::constant(2026, 11, 3);

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").unwrap()
}

fn money(minor: i64) -> Money {
    Money::from_minor_units(minor, eur())
}

/// Every cell on and no field locked.
fn unlocked() -> Permissions {
    Field::ALL
        .into_iter()
        .fold(Permissions::all(), |permissions, field| {
            permissions.with_lock(field, false)
        })
}

fn account(name: &str, kind: AccountKind) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: eur(),
        account_type: kind.is_account().then_some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

/// A folder of its own under the system's temporary folder, removed at
/// the end: the bill folder, and a folder beside it that is outside.
struct Folders {
    root: PathBuf,
}

impl Folders {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("kassensturz-bills-{}", TransactionId::generate()));
        fs::create_dir_all(root.join("Bills/2026")).unwrap();
        fs::create_dir_all(root.join("Elsewhere")).unwrap();
        Self { root }
    }

    fn bills(&self) -> PathBuf {
        self.root.join("Bills")
    }

    fn file(&self, path: &str) -> PathBuf {
        let path = self.root.join(path);
        fs::write(&path, b"synthetic bill").unwrap();
        path
    }
}

impl Drop for Folders {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Fixture {
    book: Book,
    checking: AccountId,
    office: AccountId,
    /// Checking pays 54.10 to a shop, not sorted yet.
    debit: TransactionId,
    folders: Folders,
}

fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur()).unwrap();
    let checking = account("Everyday Checking", AccountKind::Asset);
    let office = account("Office supplies", AccountKind::Expense);
    book.add_account(&checking).unwrap();
    book.add_account(&office).unwrap();
    let debit = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            debit,
            DAY,
            Some("Buerobedarf West".into()),
            vec![
                Split::new(checking.id, money(-5_410)),
                Split::uncategorized(money(5_410)),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let folders = Folders::new();
    folders.file("Bills/2026/INV-5501.pdf");
    folders.file("Bills/2026/INV-5502.PNG");
    folders.file("Bills/notes.txt");
    folders.file("Elsewhere/secret.pdf");
    Fixture {
        book,
        checking: checking.id,
        office: office.id,
        debit,
        folders,
    }
}

fn attach(transaction: TransactionId, file: &str) -> Value {
    json!({"attach_bills": [{"transaction_id": transaction.to_string(), "file": file}]})
}

fn prepare(
    f: &Fixture,
    call: &Value,
    allowed: Permissions,
) -> Result<kassensturz_ai_tools::Proposal, ToolError> {
    prepare_named_with_bills(
        &f.book,
        PROPOSE_CHANGES.name,
        call,
        allowed,
        TODAY,
        Some(&f.folders.bills()),
    )
}

/// The code of a refusal, without the item it came from.
fn code(result: Result<impl Sized, ToolError>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(error) => error.kind().report().code,
    }
}

fn links(book: &Book, transaction: TransactionId) -> Vec<Attachment> {
    book.transaction(transaction)
        .unwrap()
        .attachments()
        .to_vec()
}

#[test]
fn attaching_is_off_until_its_own_cell_is_on_and_needs_a_bill_folder() {
    let f = fixture();
    assert!(!Permissions::defaults().allows(Permission::AttachBills));
    let without = unlocked().with(Permission::AttachBills, false);
    let spec = propose_spec(without).expect("other changes are offered");
    assert!(!spec.parameters.contains("attach_bills"));
    let call = attach(f.debit, "2026/INV-5501.pdf");
    assert_eq!(code(prepare(&f, &call, without)), "ai-tools.not-allowed");
    assert!(
        propose_spec(unlocked())
            .expect("offered")
            .parameters
            .contains("attach_bills")
    );
    // Without a folder, as the chat's way in has none: nothing is linked.
    assert_eq!(
        code(prepare_named(
            &f.book,
            PROPOSE_CHANGES.name,
            &call,
            unlocked(),
            TODAY
        )),
        "ai-tools.no-bill-folder"
    );
    // A folder that is gone is none.
    let gone = f.folders.root.join("Gone");
    assert_eq!(
        code(prepare_named_with_bills(
            &f.book,
            PROPOSE_CHANGES.name,
            &call,
            unlocked(),
            TODAY,
            Some(&gone)
        )),
        "ai-tools.no-bill-folder"
    );
}

#[test]
fn a_bill_is_reviewed_linked_as_one_undo_and_the_file_stays_untouched() {
    let mut f = fixture();
    let call = json!({"attach_bills": [
        {"transaction_id": f.debit.to_string(), "file": "2026/INV-5501.pdf", "title": "Invoice INV-5501"},
        {"transaction_id": f.debit.to_string(), "file": f.folders.bills().join("2026/./INV-5502.PNG").to_str().unwrap()}
    ]});
    let proposal = prepare(&f, &call, unlocked()).expect("prepared");
    let changes = proposal.changes.changes();
    assert_eq!(changes.len(), 2);
    let ProposedChange::AttachBill(first) = &changes[0] else {
        panic!("a bill");
    };
    assert_eq!(first.file_name, "INV-5501.pdf");
    assert_eq!(first.title.as_deref(), Some("Invoice INV-5501"));
    assert_eq!(first.account, "Everyday Checking");
    assert_eq!(first.payee.as_deref(), Some("Buerobedarf West"));
    assert_eq!(first.date, DAY);
    let real = f
        .folders
        .bills()
        .join("2026/INV-5501.pdf")
        .canonicalize()
        .unwrap();
    assert_eq!(first.link, real.to_str().unwrap());
    let before = f.book.transaction(f.debit).unwrap();
    let history = f.book.history().unwrap().len();
    proposal
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let linked = links(&f.book, f.debit);
    assert_eq!(linked.len(), 2);
    assert_eq!(linked[0].title.as_deref(), Some("Invoice INV-5501"));
    assert_eq!(linked[1].title, None);
    assert!(linked[1].link.ends_with("INV-5502.PNG"));
    // Only the links changed.
    let after = f.book.transaction(f.debit).unwrap();
    assert_eq!(after.splits(), before.splits());
    assert_eq!(fs::read(&real).unwrap(), b"synthetic bill");
    let entries = f.book.history().unwrap();
    assert_eq!(entries.len(), history + 1);
    assert_eq!(entries[0].action, Action::ApplyAiChanges);
    f.book.undo().unwrap().expect("a change");
    assert_eq!(f.book.transaction(f.debit).unwrap(), before);
}

#[test]
fn a_bill_outside_the_folder_is_refused_before_the_disk_is_asked() {
    let f = fixture();
    let outside = f.folders.root.join("Elsewhere/secret.pdf");
    let missing_outside = f.folders.root.join("Elsewhere/nothing.pdf");
    for file in [
        "../Elsewhere/secret.pdf",
        "2026/../../Elsewhere/secret.pdf",
        outside.to_str().unwrap(),
        // Whether a file exists outside is never told.
        missing_outside.to_str().unwrap(),
        "/etc/hostname.pdf",
        "../../../../../../../../../../etc/passwd",
        // The folder itself is no bill.
        ".",
    ] {
        assert_eq!(
            code(prepare(&f, &attach(f.debit, file), unlocked())),
            "ai-tools.bill-outside-folder",
            "{file}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_folder_that_leads_out_or_to_another_type_is_refused() {
    let f = fixture();
    let bills = f.folders.bills();
    std::os::unix::fs::symlink(
        f.folders.root.join("Elsewhere/secret.pdf"),
        bills.join("looks-fine.pdf"),
    )
    .unwrap();
    std::os::unix::fs::symlink(f.folders.root.join("Elsewhere"), bills.join("Linked")).unwrap();
    std::os::unix::fs::symlink(bills.join("notes.txt"), bills.join("renamed.pdf")).unwrap();
    std::os::unix::fs::symlink(bills.join("2026/INV-5501.pdf"), bills.join("short.pdf")).unwrap();
    for (file, wanted) in [
        ("looks-fine.pdf", "ai-tools.bill-outside-folder"),
        ("Linked/secret.pdf", "ai-tools.bill-outside-folder"),
        ("renamed.pdf", "ai-tools.bill-type"),
        ("short.pdf", "ok"),
    ] {
        assert_eq!(
            code(prepare(&f, &attach(f.debit, file), unlocked())),
            wanted,
            "{file}"
        );
    }
    // A link to a bill inside keeps the bill's own path.
    let proposal = prepare(&f, &attach(f.debit, "short.pdf"), unlocked()).unwrap();
    let ProposedChange::AttachBill(bill) = &proposal.changes.changes()[0] else {
        panic!("a bill");
    };
    assert!(bill.link.ends_with("2026/INV-5501.pdf"), "{}", bill.link);
}

#[test]
fn wrong_types_missing_files_and_doubles_are_refused() {
    let mut f = fixture();
    fs::create_dir_all(f.folders.bills().join("folder.pdf")).unwrap();
    for (file, wanted) in [
        ("notes.txt", "ai-tools.bill-type"),
        ("2026/INV-5501", "ai-tools.bill-type"),
        ("2026/INV-5501.pdf.exe", "ai-tools.bill-type"),
        ("2026/INV-9999.pdf", "ai-tools.bill-not-found"),
        ("folder.pdf", "ai-tools.bill-not-found"),
        ("", "ai-tools.bad-argument"),
    ] {
        assert_eq!(
            code(prepare(&f, &attach(f.debit, file), unlocked())),
            wanted,
            "{file:?}"
        );
    }
    // The same bill twice in one set, written two ways.
    let twice = json!({"attach_bills": [
        {"transaction_id": f.debit.to_string(), "file": "2026/INV-5501.pdf"},
        {"transaction_id": f.debit.to_string(), "file": "2026/../2026/INV-5501.pdf"}
    ]});
    assert_eq!(code(prepare(&f, &twice, unlocked())), "ai-tools.duplicate");
    // A bill the transaction has already.
    prepare(&f, &attach(f.debit, "2026/INV-5501.pdf"), unlocked())
        .unwrap()
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .unwrap();
    assert_eq!(
        code(prepare(
            &f,
            &attach(f.debit, "2026/INV-5501.pdf"),
            unlocked()
        )),
        "ai-tools.duplicate"
    );
    // A transaction that isn't there, and more than 100 bills.
    assert_eq!(
        code(prepare(
            &f,
            &attach(TransactionId::generate(), "2026/INV-5502.PNG"),
            unlocked()
        )),
        "ai-tools.unknown-transaction"
    );
    let many: Vec<Value> = (0..101)
        .map(|_| json!({"transaction_id": f.debit.to_string(), "file": "2026/INV-5502.PNG"}))
        .collect();
    assert_eq!(
        code(prepare(&f, &json!({"attach_bills": many}), unlocked())),
        "ai-tools.too-many-changes"
    );
}

#[test]
fn a_split_and_its_bills_are_one_set_and_one_undo() {
    let mut f = fixture();
    let call = json!({
        "split_transactions": [{"transaction_id": f.debit.to_string(), "parts": [
            {"category_id": f.office.to_string(), "amount": "-30.00", "name": "Invoice INV-5501"},
            {"amount": "-24.10", "name": "Invoice INV-5502"}
        ]}],
        "attach_bills": [
            {"transaction_id": f.debit.to_string(), "file": "2026/INV-5501.pdf"},
            {"transaction_id": f.debit.to_string(), "file": "2026/INV-5502.PNG"}
        ]
    });
    let before = f.book.transaction(f.debit).unwrap();
    let proposal = prepare(&f, &call, unlocked()).expect("prepared");
    assert_eq!(proposal.changes.changes().len(), 3);
    proposal
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect("applied");
    let after = f.book.transaction(f.debit).unwrap();
    assert_eq!(after.splits().len(), 3);
    assert_eq!(after.attachments().len(), 2);
    let own = after
        .splits()
        .iter()
        .find(|split| split.account == Some(f.checking))
        .unwrap();
    assert_eq!(own.amount, money(-5_410));
    f.book.undo().unwrap().expect("a change");
    assert_eq!(f.book.transaction(f.debit).unwrap(), before);
}

#[test]
fn a_bill_never_goes_on_a_transaction_the_set_removes() {
    let mut f = fixture();
    let mut cash = account("Wallet", AccountKind::Asset);
    cash.account_type = Some(AccountType::Cash);
    f.book.add_account(&cash).unwrap();
    let typed = TransactionId::generate();
    f.book
        .add_transaction(
            &Transaction::new(
                typed,
                DAY,
                Some("Kiosk".into()),
                vec![
                    Split::new(cash.id, money(-350)),
                    Split::uncategorized(money(350)),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let call = json!({
        "delete_transactions": [{"transaction_id": typed.to_string()}],
        "attach_bills": [{"transaction_id": typed.to_string(), "file": "2026/INV-5501.pdf"}]
    });
    let result = prepare(&f, &call, unlocked());
    assert_eq!(code(result), "ai-tools.not-changeable");
}

#[test]
fn the_bill_folder_names_the_folder_and_the_types() {
    let f = fixture();
    let answer = bill_folder_answer(&f.folders.bills(), &json!({})).unwrap();
    assert_eq!(answer.value["folder"], f.folders.bills().to_str().unwrap());
    assert_eq!(
        answer.value["file_types"],
        json!(["pdf", "png", "jpg", "jpeg"])
    );
    assert!(answer.value["note"].is_string(), "the data notice");
    assert_eq!(
        code(bill_folder_answer(&f.folders.bills(), &json!({"x": 1}))),
        "ai-tools.bad-argument"
    );
    assert!(BILL_FOLDER.description.contains("attach_bills"));
}

#[test]
fn the_complete_export_carries_the_link() {
    let mut f = fixture();
    prepare(&f, &attach(f.debit, "2026/INV-5501.pdf"), unlocked())
        .unwrap()
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .unwrap();
    let link = links(&f.book, f.debit)[0].link.clone();
    let data = export_book(&f.book).unwrap();
    let ExportData::Book(book) = &data else {
        panic!("a book");
    };
    let exported = book
        .transactions
        .iter()
        .find(|transaction| transaction.id() == f.debit)
        .unwrap();
    assert_eq!(exported.attachments()[0].link, link);
    let file = String::from_utf8(export(&BookExport, &data).unwrap()).unwrap();
    let escaped = serde_json::to_string(&link).unwrap();
    assert!(
        file.contains(escaped.trim_matches('"')),
        "the file holds the link"
    );
}
