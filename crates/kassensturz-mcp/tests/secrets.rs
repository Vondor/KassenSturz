// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Secrets are unreachable through the running server (threats T-23, T-25,
//! M5.12): a synthetic book holds test secrets wherever a book can hold
//! them, a bank link with example IBANs, and a tool-using program calls
//! every tool the server offers, with and without the write tools. Every
//! answer is scanned, and the server's list of tools is held to the fixed
//! list of read functions.

#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use common::{CHECKING, EUR, Folder, GROCERIES, Session, account, transaction};
use kassensturz_ai_tools::FUNCTIONS;
use kassensturz_core::{AccountKind, BankLink, Date, Institution, InstitutionId};
use kassensturz_store::Book;
use serde_json::json;

/// Example IBANs from SWIFT's IBAN Registry (D-055) and test secrets.
const NEVER: &[&str] = &[
    "SECRET",
    "DE89",
    "GB82",
    "0532013000",
    "37040044",
    "97531",
    "sk-test-key",
];

/// A book with the secrets a book can hold, in a file of the folder.
fn book_with_secrets(folder: &Folder) -> std::path::PathBuf {
    let path = folder.path().join("Secrets.ksz");
    let mut book = Book::create(&path, &folder.computer()).expect("created");
    book.use_currency(EUR).expect("currency used");
    let institution = Institution {
        id: InstitutionId::generate(),
        name: "Orbit Test Bank".to_owned(),
        bank_code: Some("bank-code-SECRET".to_owned()),
        contact: Some("login-name-SECRET".to_owned()),
        address: None,
        phone: None,
    };
    book.add_institution(&institution).expect("institution");
    let mut checking = account(CHECKING, "Checking", AccountKind::Asset);
    checking.number = Some("number-SECRET".to_owned());
    checking.notes = Some("notes-SECRET, PIN 97531, key sk-test-key".to_owned());
    checking.institution = Some(institution.id);
    book.add_account(&checking).expect("account");
    book.add_account(&account(GROCERIES, "Groceries", AccountKind::Expense))
        .expect("account");
    book.add_transaction(&transaction(
        1,
        Date::constant(2026, 10, 3),
        "Corner Shop",
        &[(GROCERIES, 4_250), (CHECKING, -4_250)],
    ))
    .expect("added");
    book.link_bank_account(&BankLink {
        account: CHECKING,
        iban: Some("DE89370400440532013000".to_owned()),
        bank_code: Some("37040044".to_owned()),
        number: Some("0532013000".to_owned()),
        last_fetched_on: None,
    })
    .expect("linked");
    path
}

fn tool_names(session: &mut Session) -> Vec<String> {
    session.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name").to_owned())
        .collect()
}

#[test]
fn no_tool_answer_holds_a_secret() {
    let folder = Folder::new("no-secrets");
    folder.switch_ai(true);
    let path = book_with_secrets(&folder);
    let mut session = Session::start(folder.path(), &path);
    session.initialize("2025-06-18");

    // Read-only by default: exactly the fixed list of read functions.
    let mut names = tool_names(&mut session);
    names.sort();
    let mut expected: Vec<_> = FUNCTIONS.iter().map(|f| f.name.to_owned()).collect();
    expected.sort();
    assert_eq!(names, expected);

    let checking = CHECKING.to_string();
    let groceries = GROCERIES.to_string();
    for (name, arguments) in [
        ("list_accounts", json!({"include_closed": true})),
        ("account_balance", json!({"account_id": checking})),
        (
            "category_totals",
            json!({"from": "2026-01-01", "to": "2026-12-31"}),
        ),
        ("search_transactions", json!({})),
        ("search_transactions", json!({"category_id": groceries})),
        ("list_categories", json!({})),
        ("list_rules", json!({})),
    ] {
        let (answer, is_error) = session.call(name, arguments);
        assert!(!is_error, "{name}: {answer}");
        let text = answer.to_string();
        for never in NEVER {
            assert!(!text.contains(never), "{name} holds {never}: {text}");
        }
    }
    // A tool outside the list is refused, whatever it asks for.
    for name in ["run_sql", "read_file", "list_bank_links", "send_money"] {
        let answer = session.request(
            "tools/call",
            json!({"name": name, "arguments": {"query": "SELECT * FROM bank_links"}}),
        );
        assert!(
            answer.get("error").is_some() || answer["result"]["isError"] == json!(true),
            "{name}: {answer}"
        );
        for never in NEVER {
            assert!(!answer.to_string().contains(never), "{name}: {answer}");
        }
    }
    session.finish();
}

#[test]
fn the_write_tools_offer_no_way_to_apply_or_pay() {
    let folder = Folder::new("write-tools-list");
    folder.switch_ai(true);
    folder.switch_write_tools(true);
    let path = folder.book();
    let mut session = Session::start(folder.path(), &path);
    session.initialize("2025-06-18");
    let mut names = tool_names(&mut session);
    names.sort();
    let mut expected: Vec<_> = FUNCTIONS.iter().map(|f| f.name.to_owned()).collect();
    expected.extend([
        "propose_changes".to_owned(),
        "check_proposed_changes".to_owned(),
    ]);
    expected.sort();
    assert_eq!(names, expected, "write tools only propose and check");
}
