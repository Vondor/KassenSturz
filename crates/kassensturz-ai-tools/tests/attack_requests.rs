// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario B4 of the outside security review: a flood of change
//! requests through the request folder, and request files written by hand,
//! without the MCP server. All data is made up (AGENTS.md §3.1 rule 2).
//!
//! The window side (that a review never takes keyboard focus while the user
//! types, and that requests are ignored while AI is off) lives in the Qt
//! bridge and is checked in a VM round; here are the folder and the check
//! of every call against the app's own permission grid.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use kassensturz_ai_tools::prepare_named;
use kassensturz_ai_tools::requests::{MAX_AGE, RequestFolder};
use kassensturz_core::ai_switch::{Field, Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Book, Computer};
use serde_json::json;

const TODAY: Date = Date::constant(2026, 10, 25);

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("kassensturz-attack-b4-{}", AccountId::generate()));
        std::fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").expect("valid code")
}

fn account(name: &str, kind: AccountKind) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency: eur(),
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    }
}

/// A book file with one account, one category and one transaction.
fn book(folder: &TempFolder) -> (PathBuf, Book, TransactionId) {
    let path = folder.join("Household.ksz");
    let computer = Computer {
        name: "Laptop".into(),
        backup_folder: folder.join("Backups"),
    };
    let mut book = Book::create(&path, &computer).expect("book created");
    book.use_currency(eur()).expect("currency used");
    let checking = account("Checking", AccountKind::Asset);
    let food = account("Food", AccountKind::Expense);
    book.add_account(&checking).expect("added");
    book.add_account(&food).expect("added");
    let transaction = Transaction::new(
        TransactionId::generate(),
        Date::constant(2026, 10, 3),
        Some("Corner Shop".into()),
        vec![
            Split::new(checking.id, Money::from_minor_units(-500, eur())),
            Split::uncategorized(Money::from_minor_units(500, eur())),
        ],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    (path, book, transaction.id())
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn b4_the_request_folder_and_its_files_are_the_users_only() {
    let folder = TempFolder::new();
    let (path, _book, _) = book(&folder);
    let place = folder.join("mcp-requests");
    std::fs::create_dir(&place).expect("made");
    // Someone left it open to others: opening it closes it.
    std::fs::set_permissions(&place, std::fs::Permissions::from_mode(0o777)).expect("opened");
    let requests = RequestFolder::open(place.clone()).expect("folder");
    assert_eq!(mode(&place), 0o700);
    requests
        .send(&path, &json!({"new_categories": []}))
        .expect("sent");
    for entry in std::fs::read_dir(&place).expect("listed") {
        let entry = entry.expect("entry");
        assert_eq!(mode(&entry.path()), 0o600, "{:?}", entry.file_name());
    }
}

#[test]
fn b4_a_flood_of_30_requests_queues_in_order_and_old_ones_expire() {
    let folder = TempFolder::new();
    let (path, _book, _) = book(&folder);
    let place = folder.join("mcp-requests");
    let server = RequestFolder::open(place.clone()).expect("folder");
    let mut sent = Vec::new();
    for n in 0..30 {
        let id = server
            .send(
                &path,
                &json!({"new_categories": [{"key": format!("k{n}"), "name": format!("Flood {n}")}]}),
            )
            .expect("sent");
        sent.push(id);
    }
    let mut app = RequestFolder::open(place.clone()).expect("folder");
    let taken = app.take_new_at(&path, SystemTime::now());
    assert_eq!(taken.len(), 30, "every request waits in the queue");
    // Each is taken once: a second look finds none of them again.
    assert!(app.take_new_at(&path, SystemTime::now()).is_empty());

    // A request nobody took for longer than an hour is removed unread.
    server
        .send(&path, &json!({"new_categories": []}))
        .expect("sent");
    let later = SystemTime::now() + MAX_AGE + Duration::from_secs(60);
    assert!(
        app.take_new_at(&path, later).is_empty(),
        "an old request was shown"
    );
}

#[test]
fn b4_a_hand_written_request_for_another_function_or_book_is_never_shown() {
    let folder = TempFolder::new();
    let (path, _book, _) = book(&folder);
    let place = folder.join("mcp-requests");
    let mut app = RequestFolder::open(place.clone()).expect("folder");
    let write = |name: &str, body: serde_json::Value| {
        std::fs::write(place.join(name), body.to_string()).expect("written");
    };
    // A function that changes settings, which no request may call.
    write(
        "0123456789abcdef0123456789abcdef.request.json",
        json!({"version": 1, "book": path, "function": "set_history_retention", "arguments": {}}),
    );
    // A request for a book this app hasn't open.
    write(
        "1123456789abcdef0123456789abcdef.request.json",
        json!({"version": 1, "book": folder.join("Other.ksz"), "arguments": {"new_categories": []}}),
    );
    assert!(app.take_new_at(&path, SystemTime::now()).is_empty());
}

#[test]
fn b4_a_hand_written_request_is_checked_against_the_apps_own_grid() {
    let folder = TempFolder::new();
    let (path, book, transaction) = book(&folder);
    let place = folder.join("mcp-requests");
    let mut app = RequestFolder::open(place.clone()).expect("folder");
    // Written by hand: no MCP server checked it against anything.
    std::fs::write(
        place.join("2123456789abcdef0123456789abcdef.request.json"),
        json!({"version": 1, "book": path, "function": "propose_changes", "arguments": {
            "edit_transactions": [{"transaction_id": transaction.to_string(), "note": "Paid back"}],
        }})
        .to_string(),
    )
    .expect("written");
    let taken = app.take_new_at(&path, SystemTime::now());
    assert_eq!(taken.len(), 1);
    let request = &taken[0];
    let everything = Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false));
    // The call itself is sound: with every cell on it is prepared.
    assert!(
        prepare_named(
            &book,
            request.function,
            &request.arguments,
            everything,
            TODAY
        )
        .is_ok()
    );
    // The app's grid: everything on but changing transactions.
    let grid = everything.with(Permission::SortTransactions, false);
    let result = prepare_named(&book, request.function, &request.arguments, grid, TODAY);
    assert!(result.is_err(), "the app's grid forbids this edit");
    // With AI's changes all off, nothing at all is prepared.
    let none = Permissions::defaults();
    assert!(prepare_named(&book, request.function, &request.arguments, none, TODAY).is_err());
}
