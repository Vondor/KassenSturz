// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario B9 (asked by the maintainer after the outside review):
//! SQL written into a payee, a memo, a category name or a Rule condition,
//! through every import format and through the AI tools. Every value must
//! reach the book as plain text and leave its tables whole. The files are
//! the importers' synthetic golden files with one name replaced (AGENTS.md
//! §3.1 rule 2). The MCP server calls the same functions as the chat
//! (`call`, `propose`), so these cover its tools too.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::path::PathBuf;

use kassensturz_ai_tools::{Confirmed, call, propose};
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode, Date};
use kassensturz_import::{
    Camt053, Csv, CsvSettings, Imported, Importer, Kmy, KmySqlite, Limits, Mt940, Ofx, currency,
    import,
};
use kassensturz_pipeline::{Choices, fill_book_from, import_statements};
use kassensturz_store::Book;
use serde_json::{Value, json};

/// SQL that would drop or empty tables if any of it reached SQLite as SQL.
const SQL: &str = "'); DROP TABLE splits;--";

const TODAY: Date = Date::constant(2026, 10, 25);

fn golden(path: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden")
        .join(path);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// `bytes` with every `from` replaced by `to`.
fn replaced(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let text = String::from_utf8(bytes.to_vec()).expect("a text file");
    assert!(text.contains(from), "{from:?} is in the golden file");
    text.replace(from, to).into_bytes()
}

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").expect("valid code")
}

/// A book with one checking account to import statements into.
fn book() -> (Book, AccountId) {
    let mut book = Book::create_in_memory().expect("a book");
    book.use_currency(eur()).expect("currency used");
    let checking = Account {
        id: AccountId::generate(),
        name: "Checking".into(),
        kind: AccountKind::Asset,
        currency: eur(),
        account_type: None,
        closed: false,
        parent: None,
        number: None,
        notes: None,
        institution: None,
    };
    book.add_account(&checking).expect("added");
    (book, checking.id)
}

/// Everything a reader of the book sees, to check that it is whole.
fn whole(book: &Book) -> (usize, usize, usize) {
    (
        book.all_accounts().expect("accounts").len(),
        book.transactions().expect("transactions").len(),
        book.transactions()
            .expect("transactions")
            .iter()
            .map(|t| t.splits().len())
            .sum(),
    )
}

fn holds_sql(book: &Book) -> bool {
    format!("{:?}", book.transactions().expect("transactions")).contains(&format!("{SQL:?}"))
        || book
            .all_accounts()
            .expect("accounts")
            .iter()
            .any(|account| account.name.contains(SQL))
}

fn import_statement_file(importer: &dyn Importer, bytes: &[u8]) {
    let Imported::Statements(statements) =
        import(importer, bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("statements");
    };
    let (mut book, checking) = book();
    import_statements(&mut book, checking, &statements, &Choices::default(), None)
        .expect("imported");
    let (_, transactions, parts) = whole(&book);
    assert!(transactions > 0 && parts >= 2 * transactions);
    assert!(holds_sql(&book), "the text was kept as text");
}

#[test]
fn b9_sql_in_a_camt_name_is_text() {
    let file = replaced(
        &golden("camt053/batch-entry.xml"),
        "Example Landlord",
        "&apos;); DROP TABLE splits;--",
    );
    import_statement_file(&Camt053, &file);
}

#[test]
fn b9_sql_in_an_mt940_name_is_text() {
    let file = replaced(&golden("mt940/german-bank.sta"), "Muster Energie AG", SQL);
    import_statement_file(&Mt940, &file);
}

#[test]
fn b9_sql_in_an_ofx_name_is_text() {
    let file = replaced(
        &golden("ofx/bank-xml.ofx"),
        "Muster Energie AG",
        "&apos;); DROP TABLE splits;--",
    );
    import_statement_file(&Ofx, &file);
}

#[test]
fn b9_sql_in_a_csv_name_is_text() {
    let settings = CsvSettings::from_text(
        "delimiter = \";\"\ndecimal-mark = comma\ndate-order = day-month-year\n\
         booking-date = \"Datum\"\namount = \"Betrag\"\nother-party-name = \"Name\"\n",
    )
    .expect("valid settings");
    let euro = currency(eur()).expect("a currency");
    let csv = Csv::new(settings, euro).expect("an importer");
    let file = format!("Datum;Name;Betrag\n25.10.2026;\"{SQL}\";-1,00\n");
    import_statement_file(&csv, file.as_bytes());
}

fn fill(imported: Imported) -> Book {
    let Imported::Book(imported) = imported else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    book
}

#[test]
fn b9_sql_in_a_kmy_payee_and_category_is_text() {
    let file = replaced(
        &golden("kmy/features.kmy"),
        "Corner Grocery",
        "&apos;); DROP TABLE splits;--",
    );
    let file = replaced(
        &file,
        "name=\"Holiday\"",
        "name=\"&apos;); DROP TABLE splits;--\"",
    );
    let book = fill(import(&Kmy, &file, &Limits::default()).expect("the file reads"));
    assert!(whole(&book).1 > 0);
    assert!(holds_sql(&book));
}

#[test]
fn b9_sql_in_a_kmymoney_sqlite_payee_is_text() {
    // Same length as the name it replaces, so the SQLite file stays valid.
    let name = "Colony Council Tax Office";
    let sql = format!("{SQL:<width$}", width = name.len());
    let bytes = golden("kmy-sqlite/space-colony-v15.sqlite");
    let mut hostile = bytes.clone();
    let mut count = 0;
    let mut at = 0;
    while let Some(found) = hostile[at..]
        .windows(name.len())
        .position(|window| window == name.as_bytes())
    {
        let start = at + found;
        hostile[start..start + name.len()].copy_from_slice(sql.as_bytes());
        at = start + name.len();
        count += 1;
    }
    assert!(count > 0, "the payee is in the file");
    let book = fill(import(&KmySqlite, &hostile, &Limits::default()).expect("the file reads"));
    let original = fill(import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads"));
    assert_eq!(
        whole(&book),
        whole(&original),
        "every account and part arrived"
    );
    assert!(format!("{:?}", book.transactions().unwrap()).contains(sql.trim_end()));
}

/// Everything allowed, nothing locked.
fn open() -> Permissions {
    Field::ALL
        .iter()
        .fold(Permissions::all(), |p, field| p.with_lock(*field, false))
}

fn run(book: &Book, name: &str, arguments: Value) -> Value {
    call(book, name, &arguments, TODAY)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
        .value
}

#[test]
fn b9_sql_through_the_ai_tools_is_text() {
    let file = replaced(&golden("mt940/german-bank.sta"), "Muster Energie AG", SQL);
    let Imported::Statements(statements) = import(&Mt940, &file, &Limits::default()).unwrap()
    else {
        panic!("statements");
    };
    let (mut book, checking) = book();
    import_statements(&mut book, checking, &statements, &Choices::default(), None).unwrap();
    let before = whole(&book);

    // A search for the SQL finds the transaction and nothing breaks.
    let found = run(&book, "search_transactions", json!({"text": SQL}));
    assert!(found.to_string().contains("DROP TABLE"), "{found}");
    for text in ["%", "_", "' OR '1'='1", "\\", "\"; --"] {
        run(&book, "search_transactions", json!({ "text": text }));
    }

    // A category, a Rule condition and a note that hold SQL.
    let transaction = book.transactions().unwrap()[0].id();
    let proposal = propose(
        &book,
        &json!({
            "new_categories": [{"key": "sql", "name": SQL}],
            "new_rules": [{
                "name": SQL,
                "conditions": [{"field": "payee", "test": "contains", "text": SQL}],
                "new_category": "sql",
            }],
            "edit_transactions": [{"transaction_id": transaction.to_string(), "note": SQL}],
        }),
        open(),
    )
    .expect("prepared");
    proposal
        .changes
        .apply(&mut book, Confirmed::by_the_user())
        .expect("applied");
    let after = whole(&book);
    assert_eq!(after.1, before.1, "no transaction lost");
    assert_eq!(after.2, before.2, "no part lost");
    assert!(
        book.categories()
            .unwrap()
            .iter()
            .any(|entry| entry.category.name == SQL)
    );
    assert_eq!(book.transaction(transaction).unwrap().note(), Some(SQL));
    let rules = run(&book, "list_rules", json!({}));
    assert!(rules.to_string().contains("DROP TABLE"), "{rules}");
    book.undo().expect("undone").expect("a change");
    assert_eq!(whole(&book), before);
}
