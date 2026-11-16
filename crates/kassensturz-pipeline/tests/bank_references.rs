// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bank references through the real MT940 importer and the import
//! pipeline (B-332). Synthetic data only; IBANs from SWIFT's IBAN
//! Registry.

use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode};
use kassensturz_import::{Imported, Limits, Mt940, Statement, import};
use kassensturz_pipeline::{Choices, Duplicate, EntryOutcome, import_statements};
use kassensturz_store::Book;

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR"),
};

fn book_with_checking() -> (Book, AccountId) {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(EUR).unwrap();
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: "Checking".into(),
        kind: AccountKind::Asset,
        currency: EUR,
        account_type: None,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .unwrap();
    (book, id)
}

fn statements(importer: &dyn kassensturz_import::Importer, text: &str) -> Vec<Statement> {
    match import(importer, text.as_bytes(), &Limits::default()).unwrap() {
        Imported::Statements(statements) => statements,
        _ => panic!("statements expected"),
    }
}

/// The rent of October and of November, each the first booking of its day
/// with the bank's reference `0001` (a per-day counter). Two different
/// payments a month apart.
const OCTOBER: &str = ":20:STMT-OCT
:25:DE89370400440532013000
:60F:C261001EUR2000,00
:61:2610011001D800,00NDDTNONREF//0001
:86:Rent October
:62F:C261031EUR1200,00
-
";
const NOVEMBER: &str = ":20:STMT-NOV
:25:DE89370400440532013000
:60F:C261101EUR1200,00
:61:2611021102D800,00NDDTNONREF//0001
:86:Rent November
:62F:C261130EUR400,00
-
";

#[test]
fn a_reused_bank_reference_a_month_later_is_not_a_duplicate() {
    let (mut book, checking) = book_with_checking();
    import_statements(
        &mut book,
        checking,
        &statements(&Mt940, OCTOBER),
        &Choices::default(),
        None,
    )
    .unwrap();
    let report = import_statements(
        &mut book,
        checking,
        &statements(&Mt940, NOVEMBER),
        &Choices::default(),
        None,
    )
    .unwrap();
    let outcome = &report.entries[0].outcome;
    assert!(
        !matches!(outcome, EntryOutcome::Duplicate(Duplicate::InBook(_))),
        "a payment a month later was skipped without a word: {outcome:?}"
    );
    assert_eq!(report.summary().imported, 1, "{outcome:?}");
}
