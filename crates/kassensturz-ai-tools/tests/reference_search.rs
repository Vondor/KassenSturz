// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `search_transactions` finds the payment of a bill by its reference
//! (US5.12, D-738, `docs/AI_SPLITS.md` §2): letters and digits only,
//! ignoring case, in the bank text, the note or a part's name; at least
//! four of them; and a reference that matches nothing is "not found",
//! never guessed by amount (#717 item 5).
//!
//! The seam (AGENTS.md §9.4): the bank text of real CAMT.053 and MT940
//! output, as `import_statements` books it, is found by its reference.
//! Synthetic data only.

#![cfg(feature = "ai")]

use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{ToolError, call};
use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, ImportedDetails, Money,
    Report as _, Split, Transaction, TransactionId,
};
use kassensturz_import::{Camt053, Imported, Importer, Limits, Mt940, import};
use kassensturz_pipeline::{Choices, import_statements};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 11, 12);
const DAY: Date = Date::constant(2026, 11, 3);

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").unwrap()
}

fn money(minor: i64) -> Money {
    Money::from_minor_units(minor, eur())
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

struct Fixture {
    book: Book,
    /// 19.99 paid with the bank text "Rechnung RE-2026/0042".
    by_bank_text: TransactionId,
    /// 19.99 too, with the note "Order 7781-AB" and no bank text.
    by_note: TransactionId,
    /// 54.10 in two parts named "Invoice INV-5501" and "Invoice INV-5502".
    by_parts: TransactionId,
}

fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur()).unwrap();
    let checking = account("Everyday Checking", AccountKind::Asset);
    let office = account("Office supplies", AccountKind::Expense);
    book.add_account(&checking).unwrap();
    book.add_account(&office).unwrap();

    let by_bank_text = TransactionId::generate();
    let mut paid = Split::new(checking.id, money(-1_999));
    paid.imported = Some(ImportedDetails {
        remittance_information: Some("Rechnung RE-2026/0042 Kundennr 9".into()),
        ..ImportedDetails::default()
    });
    book.add_transaction(
        &Transaction::new(
            by_bank_text,
            DAY,
            Some("Papierladen Nord".into()),
            vec![paid, Split::uncategorized(money(1_999))],
        )
        .unwrap(),
    )
    .unwrap();

    let by_note = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            by_note,
            DAY,
            Some("Versand Sued".into()),
            vec![
                Split::new(checking.id, money(-1_999)),
                Split::uncategorized(money(1_999)),
            ],
        )
        .unwrap()
        .with_note(Some("Order 7781-AB".into())),
    )
    .unwrap();

    let by_parts = TransactionId::generate();
    let mut first = Split::new(office.id, money(3_000));
    first.note = Some("Invoice INV-5501".into());
    let mut second = Split::uncategorized(money(2_410));
    second.note = Some("Invoice INV-5502".into());
    book.add_transaction(
        &Transaction::new(
            by_parts,
            DAY,
            Some("Buerobedarf West".into()),
            vec![Split::new(checking.id, money(-5_410)), first, second],
        )
        .unwrap(),
    )
    .unwrap();

    Fixture {
        book,
        by_bank_text,
        by_note,
        by_parts,
    }
}

fn search(book: &Book, arguments: &Value) -> Result<Value, ToolError> {
    call(book, "search_transactions", arguments, TODAY).map(|answer| answer.value)
}

/// The transaction IDs of the rows of `answer`.
fn ids(answer: &Value) -> Vec<String> {
    answer["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["transaction_id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_reference_is_found_in_the_bank_text_by_its_letters_and_digits() {
    let f = fixture();
    for reference in ["RE-2026/0042", "re 2026 0042", "re20260042", "Re.2026.0042"] {
        let answer = search(&f.book, &json!({"reference": reference})).unwrap();
        assert_eq!(ids(&answer), [f.by_bank_text.to_string()], "{reference}");
        assert_eq!(answer["matched_by"], "reference");
        assert!(answer.get("found").is_none());
    }
}

#[test]
fn a_reference_is_found_in_the_note_and_in_a_parts_name() {
    let f = fixture();
    let answer = search(&f.book, &json!({"reference": "7781 ab"})).unwrap();
    assert_eq!(ids(&answer), [f.by_note.to_string()]);
    // Several bills, one debit: each reference finds the same transaction.
    for reference in ["INV-5501", "inv5502"] {
        let answer = search(&f.book, &json!({"reference": reference})).unwrap();
        assert_eq!(ids(&answer), [f.by_parts.to_string()], "{reference}");
        assert_eq!(answer["matched_by"], "reference");
    }
}

#[test]
fn a_reference_that_matches_nothing_is_not_found_and_never_guessed_by_amount() {
    let f = fixture();
    // Two payments of 19.99 are in the book; neither carries the reference.
    let answer = search(&f.book, &json!({"reference": "RE-2026-9999"})).unwrap();
    assert_eq!(answer["found"], false);
    assert_eq!(answer["matches"], 0);
    assert!(ids(&answer).is_empty());
    assert!(answer.get("matched_by").is_none());
    let note = answer["what_to_do"].as_str().unwrap();
    assert!(note.contains("not found"), "{note}");
    assert!(note.contains("Do not match it by amount"), "{note}");
    // Together with the amount of the bill, still nothing.
    let answer = search(
        &f.book,
        &json!({"reference": "RE-2026-9999", "min_amount": "19.99", "max_amount": "19.99"}),
    )
    .unwrap();
    assert_eq!(answer["found"], false);
    assert!(ids(&answer).is_empty());
}

#[test]
fn a_reference_needs_four_letters_or_digits() {
    let f = fixture();
    for reference in ["R-42", "  42 ", "--/--"] {
        let error = search(&f.book, &json!({"reference": reference})).unwrap_err();
        let report = error.report();
        assert_eq!(report.code, "ai-tools.reference-too-short", "{reference}");
        assert_eq!(
            report.params,
            vec![("min_chars", kassensturz_core::ErrorParam::count(4_usize))]
        );
    }
    // Four is enough: "0042" is in the bank text.
    let answer = search(&f.book, &json!({"reference": "00-42"})).unwrap();
    assert_eq!(ids(&answer), [f.by_bank_text.to_string()]);
    // A text too long for the search is a bad argument.
    let long = "A".repeat(101);
    let error = search(&f.book, &json!({"reference": long})).unwrap_err();
    assert_eq!(error.report().code, "ai-tools.bad-argument");
}

#[test]
fn without_a_reference_the_answer_says_nothing_about_it() {
    let f = fixture();
    for arguments in [
        json!({}),
        json!({"reference": ""}),
        json!({"reference": "  "}),
    ] {
        let answer = search(&f.book, &arguments).unwrap();
        assert_eq!(answer["matches"], 3, "{arguments}");
        assert!(answer.get("matched_by").is_none());
        assert!(answer.get("found").is_none());
        assert!(answer.get("what_to_do").is_none());
    }
    let answer = search(&f.book, &json!({})).unwrap();
    assert!(answer.get("matched_by").is_none());
    assert!(answer.get("found").is_none());
    assert!(answer.get("what_to_do").is_none());
}

// --- The seam: the bank text of real importer output -------------------

fn import_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests")
}

/// Imports `path` with `importer` into a fresh account as the app does,
/// then finds every delivered entry whose bank text has a word of four or
/// more letters and digits by that word written differently.
fn finds_delivered_bank_texts(importer: &dyn Importer, path: &PathBuf, what: &str) {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur()).unwrap();
    let checking = account("Everyday Checking", AccountKind::Asset);
    book.add_account(&checking).unwrap();
    let bytes = fs::read(path).unwrap_or_else(|_| panic!("{}", path.display()));
    let Imported::Statements(statements) =
        import(importer, &bytes, &Limits::default()).expect("the statement reads")
    else {
        panic!("statements");
    };
    import_statements(
        &mut book,
        checking.id,
        &statements,
        &Choices::default(),
        None,
    )
    .expect("imported");
    let mut checked = 0;
    for transaction in book.transactions().unwrap() {
        let Some(text) = transaction
            .splits()
            .iter()
            .find_map(|split| split.imported.as_ref()?.remittance_information.clone())
        else {
            continue;
        };
        let Some(word) = text
            .split_whitespace()
            .find(|word| word.chars().filter(|c| c.is_alphanumeric()).count() >= 4)
        else {
            continue;
        };
        // The same letters and digits, in other case, with separators.
        let reference: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_uppercase)
            .flat_map(|c| [c, '-'])
            .collect();
        let answer = search(&book, &json!({"reference": reference, "limit": 50}))
            .unwrap_or_else(|error| panic!("{what}: {reference}: {error:?}"));
        assert_eq!(answer["matched_by"], "reference", "{what}: {reference}");
        assert!(
            ids(&answer).contains(&transaction.id().to_string()),
            "{what}: {reference} finds its entry"
        );
        checked += 1;
    }
    assert!(checked > 0, "{what}: an entry with a bank text");
}

#[test]
fn a_camt053_bank_text_is_found_by_its_reference() {
    let path = import_dir().join("books/examples/statements/space-colony.camt053.xml");
    finds_delivered_bank_texts(&Camt053, &path, "CAMT.053");
}

#[test]
fn an_mt940_bank_text_is_found_by_its_reference() {
    let path = import_dir().join("golden/mt940/german-bank.sta");
    finds_delivered_bank_texts(&Mt940, &path, "MT940");
}
