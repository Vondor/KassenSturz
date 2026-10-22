// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The golden-file harness (AGENTS.md §9): every file in an importer's
//! folder under `tests/golden/` is imported, and what comes out, written as
//! plain text, must be exactly what its `.expected` file next to it says.
//!
//! Golden files are synthetic: made-up names and amounts, and only the
//! example IBANs from SWIFT's IBAN Registry (AGENTS.md §3.1 rule 2, D-055).
//! To add a case, put the file into the folder and run
//!
//! ```text
//! KASSENSTURZ_BLESS=1 cargo test -p kassensturz-import --test import
//! ```
//!
//! which writes the `.expected` files instead of comparing them. An
//! importer that needs settings for each file, such as CSV's, reads them
//! from the file's `.settings` file next to it. Read every line of a new
//! or changed `.expected` file before committing it: it is the test.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use kassensturz_core::{
    AccountKind, AccountType, ErrorParam, ErrorReport, Money, PartStatus, PayeeRecognition, Report,
};
use kassensturz_import::{
    Balance, BookPart, BookTransaction, Entry, EntryStatus, ImportError, Imported, ImportedBook,
    Importer, Limits, RECOGNITION_BYTES, Recognition, Statement, import,
};

/// The extension of the files holding what an import must give.
const EXPECTED: &str = "expected";

/// The extension of the files holding the settings an importer reads a
/// file with, such as a CSV file's columns.
const SETTINGS: &str = "settings";

/// Imports every file in `tests/golden/<id>/`, where `<id>` is the
/// importer's ID, and compares the result with its `.expected` file, or
/// writes that file when `KASSENSTURZ_BLESS` is set.
pub fn check_folder(importer: &dyn Importer) {
    check_folder_with(importer.info().id, &|_| Ok(Box::new(Shared(importer))));
}

/// Like [`check_folder`] for an importer that needs settings for each file:
/// `importer_for` makes the importer for a file from the file's path,
/// usually from its `.settings` file next to it. If it fails, the error is
/// what the file gives.
pub fn check_folder_with(id: &str, importer_for: &ImporterFor<'_>) {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(id);
    let bless = std::env::var_os("KASSENSTURZ_BLESS").is_some();
    let mut inputs: Vec<PathBuf> = fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .map(|entry| entry.expect("folder entry").path())
        .collect();
    inputs.sort();
    let (companions, inputs): (Vec<PathBuf>, Vec<PathBuf>) = inputs.into_iter().partition(|path| {
        path.extension()
            .is_some_and(|ext| ext == EXPECTED || ext == SETTINGS)
    });
    assert!(
        !inputs.is_empty(),
        "{} has no golden files",
        folder.display()
    );
    for path in &companions {
        assert!(
            inputs.contains(&path.with_extension("")),
            "{} belongs to no file; remove it",
            path.display()
        );
    }
    let mut failures = Vec::new();
    for input in &inputs {
        let actual = render_file(importer_for, input);
        let expected_path = companion(input, EXPECTED);
        if bless {
            fs::write(&expected_path, &actual).expect("expected file written");
            continue;
        }
        match fs::read_to_string(&expected_path) {
            Ok(expected) if expected == actual => {}
            Ok(expected) => failures.push(format!(
                "{} gives\n{actual}\nbut {} says\n{expected}",
                input.display(),
                expected_path.display()
            )),
            Err(_) => failures.push(format!(
                "{} has no {} file yet",
                input.display(),
                expected_path.display()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{}\n\nIf the new output is right, run the tests with KASSENSTURZ_BLESS=1 \
         and review the changed .expected files.",
        failures.join("\n\n")
    );
}

/// Makes the importer for a golden file from its path.
pub type ImporterFor<'a> = dyn Fn(&Path) -> Result<Box<dyn Importer + 'a>, ImportError> + 'a;

/// An importer borrowed, for [`check_folder`].
struct Shared<'a>(&'a dyn Importer);

impl Importer for Shared<'_> {
    fn info(&self) -> kassensturz_import::ImporterInfo {
        self.0.info()
    }

    fn recognize(&self, start: &[u8]) -> Recognition {
        self.0.recognize(start)
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        self.0.read(input, limits)
    }
}

/// The file next to `input` with `extension` added to its name, such as
/// its `.expected` or `.settings` file.
pub fn companion(input: &Path, extension: &str) -> PathBuf {
    let mut name = input.file_name().expect("a file name").to_owned();
    name.push(".");
    name.push(extension);
    input.with_file_name(name)
}

/// What importing the file at `path` gives, as text: whether the importer
/// recognizes it, then what it read or the error.
fn render_file(importer_for: &ImporterFor<'_>, path: &Path) -> String {
    let input = fs::read(path).expect("golden file read");
    let importer = match importer_for(path) {
        Ok(importer) => importer,
        Err(error) => {
            let mut out = String::new();
            render_error(&mut out, &error.report(), "");
            return out;
        }
    };
    let importer = importer.as_ref();
    let start = &input[..input.len().min(RECOGNITION_BYTES)];
    let recognized = match importer.recognize(start) {
        Recognition::Yes => "yes",
        Recognition::Maybe => "maybe",
        Recognition::No => "no",
    };
    let mut out = format!("recognized: {recognized}\n");
    match import(importer, &input, &Limits::default()) {
        Ok(imported) => render_imported(&mut out, &imported),
        Err(error) => render_error(&mut out, &error.report(), ""),
    }
    out
}

/// `imported` as text, one field per line, every field written even when
/// it is empty (`-`), so a field an importer stops filling shows up.
pub fn render_imported(out: &mut String, imported: &Imported) {
    match imported {
        Imported::Statements(statements) => {
            for (number, statement) in (1..).zip(statements) {
                render_statement(out, number, statement);
            }
        }
        Imported::Book(book) => render_book(out, book),
        _ => panic!("the harness can't write {imported:?} yet"),
    }
}

/// A whole book: one line per account, one per transaction and one per
/// part below it, one per rate, one per currency no table lists and one per
/// kind of data left out, so a book of a few hundred transactions stays
/// readable.
fn render_book(out: &mut String, book: &ImportedBook) {
    let _ = writeln!(out, "book");
    for institution in &book.institutions {
        let _ = writeln!(
            out,
            "  institution {:?}: {:?}, bank code {}, contact {}, address {}, phone {}",
            institution.key,
            institution.name,
            text(&institution.bank_code),
            text(&institution.contact),
            text(&institution.address),
            text(&institution.phone),
        );
    }
    for account in &book.accounts {
        let kind = match account.kind {
            AccountKind::Asset => "asset",
            AccountKind::Liability => "liability",
            AccountKind::Income => "income",
            AccountKind::Expense => "expense",
            AccountKind::Equity => "equity",
        };
        let account_type = match account.account_type {
            None => "-",
            Some(AccountType::Checking) => "checking",
            Some(AccountType::Savings) => "savings",
            Some(AccountType::Cash) => "cash",
            Some(AccountType::CreditCard) => "credit card",
        };
        let _ = writeln!(
            out,
            "  account {:?}: {:?}, {kind}, type {account_type}, {} with {} decimal places, \
             inside {}{}",
            account.key,
            account.name,
            account.currency.code(),
            account.currency.minor_digits(),
            text(&account.parent),
            if account.closed { ", closed" } else { "" },
        );
        if account.number.is_some() || account.notes.is_some() || account.institution.is_some() {
            let _ = writeln!(
                out,
                "    number {}, notes {}, at {}",
                text(&account.number),
                text(&account.notes),
                text(&account.institution),
            );
        }
    }
    for payee in &book.payees {
        let recognition = match &payee.recognition {
            PayeeRecognition::Never => "never".to_owned(),
            PayeeRecognition::ByName => "by name".to_owned(),
            PayeeRecognition::ByKeyWords(words) => format!("by {words:?}"),
        };
        let _ = writeln!(
            out,
            "  payee {:?}: {:?}, address {}, email {}, phone {}, reference {}, recognized \
             {recognition}{}",
            payee.key,
            payee.name,
            text(&payee.address),
            text(&payee.email),
            text(&payee.phone),
            text(&payee.reference),
            if payee.recognition_ignores_case {
                " ignoring case"
            } else {
                ""
            },
        );
    }
    for tag in &book.tags {
        let _ = writeln!(out, "  tag {:?}: {:?}", tag.key, tag.name);
    }
    for (number, transaction) in (1..).zip(&book.transactions) {
        render_book_transaction(out, number, transaction);
    }
    for rate in &book.rates {
        let _ = writeln!(
            out,
            "  rate on {}: 1 {} = {} {}",
            rate.date, rate.from, rate.rate, rate.to
        );
    }
    for unlisted in &book.unlisted {
        let suggestion = unlisted.suggestion.map_or_else(
            || "-".to_owned(),
            |suggestion| {
                format!(
                    "{} at {} from {}",
                    suggestion.successor, suggestion.rate, suggestion.date
                )
            },
        );
        let _ = writeln!(
            out,
            "  unlisted {} with {} decimal places, suggested {suggestion}",
            unlisted.currency.code(),
            unlisted.currency.minor_digits(),
        );
    }
    for skipped in &book.skipped {
        let _ = writeln!(
            out,
            "  skipped {}: {}",
            skipped.what.keyword(),
            skipped.count
        );
    }
}

fn render_book_transaction(out: &mut String, number: usize, transaction: &BookTransaction) {
    let digits = transaction.currency.minor_digits();
    let _ = writeln!(
        out,
        "  transaction {number} on {}, payee {}{}{}",
        transaction.date,
        text(&transaction.payee),
        transaction
            .note
            .as_ref()
            .map_or_else(String::new, |note| format!(", note {note:?}")),
        transaction
            .entered_on
            .map_or_else(String::new, |entered| format!(", entered {entered}")),
    );
    for part in &transaction.parts {
        render_book_part(out, part, digits);
    }
}

fn render_book_part(out: &mut String, part: &BookPart, digits: u8) {
    let account_amount = part.account_amount.map_or_else(
        || "-".to_owned(),
        |money| {
            amount(
                money,
                money.currency().iso_minor_digits().unwrap_or_default(),
            )
        },
    );
    let _ = write!(
        out,
        "    part {}: {}, in the account {account_amount}",
        text(&part.account),
        amount(part.amount, digits),
    );
    if let Some(details) = &part.details {
        let _ = write!(
            out,
            ", bank id {}, other party {} {}, text {}",
            text(&details.bank_id),
            text(&details.other_party_name),
            text(&details.other_party_iban),
            text(&details.remittance_information),
        );
    }
    if let Some(note) = &part.note {
        let _ = write!(out, ", note {note:?}");
    }
    if let Some(number) = &part.number {
        let _ = write!(out, ", number {number:?}");
    }
    match part.status {
        Some(PartStatus::Cleared) => {
            let _ = write!(out, ", cleared");
        }
        Some(PartStatus::Reconciled) => {
            let _ = write!(out, ", reconciled");
        }
        None => {}
    }
    if !part.tags.is_empty() {
        let _ = write!(out, ", tags {:?}", part.tags);
    }
    let _ = writeln!(out);
}

fn render_statement(out: &mut String, number: usize, statement: &Statement) {
    let digits = statement.currency.minor_digits();
    let balance = |balance: Option<Balance>| match balance {
        Some(balance) => format!("{} on {}", amount(balance.amount, digits), balance.date),
        None => "-".to_owned(),
    };
    let _ = writeln!(out, "statement {number}");
    let _ = writeln!(out, "  account iban: {}", text(&statement.account.iban));
    let _ = writeln!(out, "  account number: {}", text(&statement.account.number));
    let _ = writeln!(
        out,
        "  currency: {} with {digits} decimal places",
        statement.currency.code()
    );
    let _ = writeln!(
        out,
        "  opening balance: {}",
        balance(statement.opening_balance)
    );
    let _ = writeln!(
        out,
        "  closing balance: {}",
        balance(statement.closing_balance)
    );
    for (number, entry) in (1..).zip(&statement.entries) {
        render_entry(out, number, entry, digits);
    }
}

fn render_entry(out: &mut String, number: usize, entry: &Entry, digits: u8) {
    let status = match entry.status {
        EntryStatus::Booked => "booked",
        EntryStatus::Pending => "pending",
    };
    let value_date = entry
        .value_date
        .map_or_else(|| "-".to_owned(), |date| date.to_string());
    let _ = writeln!(out, "  entry {number}");
    let _ = writeln!(out, "    status: {status}");
    let _ = writeln!(out, "    bank id: {}", text(&entry.bank_id));
    let _ = writeln!(out, "    booking date: {}", entry.booking_date);
    let _ = writeln!(out, "    value date: {value_date}");
    let _ = writeln!(out, "    amount: {}", amount(entry.amount, digits));
    let _ = writeln!(
        out,
        "    other party name: {}",
        text(&entry.other_party_name)
    );
    let _ = writeln!(
        out,
        "    other party iban: {}",
        text(&entry.other_party_iban)
    );
    let _ = writeln!(
        out,
        "    remittance information: {}",
        text(&entry.remittance_information)
    );
}

/// An error report: its code, then each parameter, then its cause.
fn render_error(out: &mut String, report: &ErrorReport, indent: &str) {
    let _ = writeln!(out, "{indent}error: {}", report.code);
    for (name, value) in &report.params {
        let value = match value {
            ErrorParam::Text(found) => format!("{found:?}"),
            ErrorParam::Number(number) => number.to_string(),
            ErrorParam::Currency(currency) => currency.to_string(),
            ErrorParam::Date(date) => date.to_string(),
            ErrorParam::Keyword(keyword) => (*keyword).to_owned(),
            ErrorParam::Money(money) => amount(
                *money,
                money.currency().iso_minor_digits().unwrap_or_default(),
            ),
            other => format!("{other:?}"),
        };
        let _ = writeln!(out, "{indent}  {name}: {value}");
    }
    if let Some(cause) = &report.cause {
        render_error(out, cause, &format!("{indent}  "));
    }
}

/// Text from the file in quotes, with anything that isn't printable
/// escaped, so control characters and look-alike spaces show; `-` if
/// there is none.
fn text(value: &Option<String>) -> String {
    value
        .as_ref()
        .map_or_else(|| "-".to_owned(), |value| format!("{value:?}"))
}

/// `money` written with `digits` decimal places and its currency, e.g.
/// `-25.00 EUR`.
fn amount(money: Money, digits: u8) -> String {
    let minor_units = money.minor_units();
    let magnitude = u128::from(minor_units.unsigned_abs());
    let sign = if minor_units < 0 { "-" } else { "" };
    let currency = money.currency();
    if digits == 0 {
        return format!("{sign}{magnitude} {currency}");
    }
    let scale = 10_u128.pow(u32::from(digits));
    format!(
        "{sign}{}.{:0width$} {currency}",
        magnitude / scale,
        magnitude % scale,
        width = usize::from(digits)
    )
}

/// Makes sure the harness itself writes what it should, so a golden test
/// can't pass by writing nothing.
#[test]
fn the_harness_writes_every_field() {
    let eur = kassensturz_core::CurrencyCode::new("EUR").expect("a valid code");
    let date = kassensturz_core::Date::new(2026, 10, 5).expect("a valid date");
    let mut entry = Entry::booked(date, Money::from_minor_units(-2_505, eur));
    entry.remittance_information = Some("Line\nbreak".to_owned());
    let statement = Statement {
        account: kassensturz_import::StatementAccount::default(),
        currency: kassensturz_core::Currency::new(eur, 2).expect("valid decimal places"),
        opening_balance: None,
        closing_balance: Some(Balance {
            amount: Money::from_minor_units(5, eur),
            date,
        }),
        entries: vec![entry],
    };
    let mut out = String::new();
    render_imported(&mut out, &Imported::Statements(vec![statement]));
    assert_eq!(
        out,
        "statement 1\n\
         \x20 account iban: -\n\
         \x20 account number: -\n\
         \x20 currency: EUR with 2 decimal places\n\
         \x20 opening balance: -\n\
         \x20 closing balance: 0.05 EUR on 2026-10-05\n\
         \x20 entry 1\n\
         \x20   status: booked\n\
         \x20   bank id: -\n\
         \x20   booking date: 2026-10-05\n\
         \x20   value date: -\n\
         \x20   amount: -25.05 EUR\n\
         \x20   other party name: -\n\
         \x20   other party iban: -\n\
         \x20   remittance information: \"Line\\nbreak\"\n"
    );
}
