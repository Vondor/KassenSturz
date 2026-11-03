// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The four example books and their bank files (M7a.8, D-561).
//!
//! `tools/example-books/generate.py` writes three made-up books and, for each
//! of the four example books (Space Colony too), a CSV and a CAMT.053
//! statement with one line already in the book. This test reads every book
//! through the real importer into a book, imports both statements with the
//! real pipeline and writes the answer sheet from what the real report and
//! import code say, never by hand. The sheet is what the tester kit
//! (M7a.3) quotes for cards 4 and 5, and what a person who tries an example
//! book compares their screen with.
//!
//! The reference month is `reference-month.txt` next to the books: the books
//! hold the three months before it, and "last month" is the one just before
//! it. To write the sheets (and Flower Utopia's complete export) again after
//! a change to the generator:
//!
//! ```sh
//! KASSENSTURZ_BLESS=1 cargo test -p kassensturz-pipeline --test example_books
//! ```
//!
//! Without `KASSENSTURZ_BLESS` the test fails if a committed file differs.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use kassensturz_core::{AccountId, BudgetEvery, CurrencyCode, Date, Money};
use kassensturz_export::{BookExport, export, read_book};
use kassensturz_import::{
    Camt053, Csv, CsvSettings, Imported, KmySqlite, Limits, currency, import,
};
use kassensturz_pipeline::{
    Choices, EntryOutcome, ImportReport, export_book, fill_book, fill_book_from, import_statements,
};
use kassensturz_report::{
    Filter, Level, Period, Source, income_expense_by_month, spending_by_category,
};
use kassensturz_store::Book;

/// One example book: its slug, German title, the KMyMoney file it is read
/// from (relative to `tests/books/`), the account the statements belong to,
/// the food category (top level) and the book's own currency.
struct Example {
    slug: &'static str,
    title: &'static str,
    file: &'static str,
    account: &'static str,
    food: &'static str,
    currency: &'static str,
}

const EXAMPLES: [Example; 4] = [
    Example {
        slug: "suburban-family",
        title: "Suburban Family (Familie Sommerfeld)",
        file: "examples/suburban-family-v15.sqlite",
        account: "Girokonto",
        food: "Essen",
        currency: "EUR",
    },
    Example {
        slug: "flower-utopia",
        title: "Flower Utopia (Blumenwiese Gartengemeinschaft)",
        file: "examples/flower-utopia-v15.sqlite",
        account: "Vereinskonto",
        food: "Essen und Trinken",
        currency: "EUR",
    },
    Example {
        slug: "udssr-agent",
        title: "UdSSR Government Agent (Hauptverwaltung für Windstille)",
        file: "examples/udssr-agent-v15.sqlite",
        account: "Gehaltskonto",
        food: "Verpflegung",
        currency: "RUB",
    },
    Example {
        slug: "space-colony",
        title: "Space Colony",
        file: "space-colony-5y-v15.sqlite",
        account: "Everyday Checking",
        food: "Food",
        currency: "EUR",
    },
];

fn books() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests/books")
}

fn bless() -> bool {
    std::env::var_os("KASSENSTURZ_BLESS").is_some()
}

/// The month the books were made for, as `(year, month)`.
fn reference() -> (i16, i8) {
    let text = fs::read_to_string(books().join("examples/reference-month.txt"))
        .expect("the reference month");
    let (year, month) = text.trim().split_once('-').expect("YYYY-MM");
    (
        year.parse().expect("a year"),
        month.parse().expect("a month"),
    )
}

fn the_book(example: &Example) -> Book {
    let bytes = fs::read(books().join(example.file)).expect("the KMyMoney book");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    book
}

fn account_of(book: &Book, name: &str) -> AccountId {
    book.accounts(false)
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("no account {name}"))
        .id
}

fn code(text: &str) -> CurrencyCode {
    CurrencyCode::new(text).expect("a valid code")
}

/// An amount as a German reader writes it: `1.234,56 EUR`.
fn german(money: Money) -> String {
    let minor = money.minor_units();
    let sign = if minor < 0 { "-" } else { "" };
    let minor = minor.unsigned_abs();
    let whole = (minor / 100).to_string();
    let mut grouped = String::new();
    for (place, digit) in whole.chars().rev().enumerate() {
        if place > 0 && place % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    let grouped: String = grouped.chars().rev().collect();
    format!("{sign}{grouped},{:02} {}", minor % 100, money.currency())
}

const MONTHS: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];

/// Card 4: last month's food total in the book's own currency, and what
/// came in less what went out in it.
struct Overview {
    food: Money,
    income: Money,
    expenses: Money,
}

fn overview(book: &Book, example: &Example, today: Date) -> Overview {
    let period = Period::last_month(today).expect("last month");
    let source = Source::read(book, period).expect("source");
    let own = code(example.currency);
    let spending = spending_by_category(&source, period, &Filter::default(), Level::TopLevel)
        .expect("spending");
    let food = spending
        .iter()
        .find(|result| result.currency == own)
        .and_then(|result| {
            result
                .rows
                .iter()
                .find(|row| row.path == [example.food.to_string()])
        })
        .map_or(Money::zero(own), |row| row.total);
    let months = income_expense_by_month(&source, period, &Filter::default()).expect("months");
    let month = months
        .iter()
        .find(|result| result.currency == own)
        .expect("money in the book's currency");
    Overview {
        food,
        income: month.income,
        expenses: month.expenses,
    }
}

/// What importing one statement file does: lines, new, already in the book,
/// to decide, the bank's closing balance, and the account's balance after.
struct Imports {
    lines: usize,
    new: usize,
    duplicates: usize,
    similar: usize,
    bank_closing: Option<Money>,
    balance_after: Money,
    matches: Option<bool>,
}

fn run_import(example: &Example, file: &str, camt: bool) -> Imports {
    let mut book = the_book(example);
    let account = account_of(&book, example.account);
    let bytes = fs::read(books().join("examples/statements").join(file)).expect("the statement");
    let imported = if camt {
        import(&Camt053, &bytes, &Limits::default())
    } else {
        let settings = fs::read_to_string(
            books()
                .join("examples/statements")
                .join(format!("{}.csv.settings", example.slug)),
        )
        .expect("the CSV settings");
        let csv = Csv::new(
            CsvSettings::from_text(&settings).expect("settings"),
            currency(code(example.currency)).expect("a current currency"),
        )
        .expect("the importer");
        import(&csv, &bytes, &Limits::default())
    }
    .expect("the statement reads");
    let Imported::Statements(statements) = imported else {
        panic!("a statement");
    };
    assert_eq!(statements.len(), 1);
    let lines = statements[0].entries.len();
    let report: ImportReport =
        import_statements(&mut book, account, &statements, &Choices::default(), None)
            .expect("imported");
    let summary = report.summary();
    // Every duplicate is found by its bank reference, every similar line by
    // amount and date; nothing else is left out.
    for result in &report.entries {
        assert!(
            !matches!(result.outcome, EntryOutcome::Pending),
            "{result:?}"
        );
    }
    Imports {
        lines,
        new: summary.imported,
        duplicates: summary.duplicates,
        similar: summary.to_decide,
        bank_closing: statements[0].closing_balance.map(|balance| balance.amount),
        balance_after: book.balance(account).expect("balance"),
        matches: report.balances.first().map(|check| check.matches()),
    }
}

fn answer_sheet(example: &Example) -> String {
    let (year, month) = reference();
    let today = Date::constant(year, month, 1);
    let book = the_book(example);
    let card4 = overview(&book, example, today);
    let last = Period::last_month(today).expect("last month").first();
    let last_name = format!(
        "{} {}",
        MONTHS[usize::from(last.month().unsigned_abs()) - 1],
        last.year()
    );

    let camt = run_import(example, &format!("{}.camt053.xml", example.slug), true);
    let csv = run_import(example, &format!("{}.csv", example.slug), false);

    // The design of the statements: one line already in the book, found by
    // its bank reference (CAMT.053) or, without one, by amount and date
    // (CSV); every other line is new. A closing balance that does not match
    // would be a mistake of the generator.
    assert_eq!(camt.lines, csv.lines, "{}", example.slug);
    assert_eq!(
        (camt.duplicates, camt.similar, camt.new),
        (1, 0, camt.lines - 1),
        "{} CAMT.053",
        example.slug
    );
    assert_eq!(
        (csv.duplicates, csv.similar, csv.new),
        (0, 1, csv.lines - 1),
        "{} CSV: a CSV line is found only as 'looks the same'",
        example.slug
    );
    assert_eq!(camt.matches, Some(true), "{} closing balance", example.slug);
    assert_eq!(
        camt.bank_closing,
        Some(camt.balance_after),
        "{}",
        example.slug
    );

    let net = card4
        .income
        .checked_sub(card4.expenses)
        .expect("a difference");
    let mut sheet = String::new();
    let _ = write!(
        sheet,
        "# Antwortbogen: {title}\n\n\
Dieses Blatt schreibt der Test `example_books` (`crates/kassensturz-pipeline/tests/example_books.rs`) \
aus dem echten Import- und Berichtscode, nicht von Hand. Das Buch und die \
Kontoauszüge macht `tools/example-books/generate.py`; nichts darin gehört zu \
einer echten Person.\n\n\
Referenzmonat: {year}-{month:02} (der letzte Monat ist {last_name}). Haben Sie das \
Buch zu einem späteren Zeitpunkt aufgemacht, zeigt die App den dann letzten \
Monat; die Zahlen unten gelten für {last_name}.\n\n\
## Karte 4: Übersicht\n\n\
- Ausgaben für „{food}“ im {last_name}: **{food_total}**\n\
- Eingenommen im {last_name}: {income}; ausgegeben: {expenses}; \
Unterschied: {net}\n\
- Ist mehr hereingekommen als hinausgegangen? **{more}**\n\n\
## Karte 5: Kontoauszug einlesen (Konto „{account}“)\n\n\
Beide Dateien liegen in `crates/kassensturz-import/tests/books/examples/statements/` \
und enthalten dieselben {lines} Zeilen; genau eine davon steht schon im Buch.\n\n\
| Datei | Zeilen | neu | schon im Buch (doppelt) | sieht gleich aus (Sie entscheiden) | Schlusssaldo laut Bank | Kontostand nach dem Einlesen |\n\
|---|---|---|---|---|---|---|\n\
| `{slug}.camt053.xml` | {lines} | {c_new} | {c_dup} | {c_sim} | {closing} | {after} |\n\
| `{slug}.csv` | {lines} | {s_new} | {s_dup} | {s_sim} | (eine CSV-Datei hat keinen) | – |\n\n\
Eine CAMT-Datei trägt für jede Zeile die Nummer der Bank; deshalb erkennt \
KassenSturz die vorhandene Zeile sicher als doppelt. Eine CSV-Datei hat keine \
solche Nummer; die vorhandene Zeile hat denselben Betrag und fast dasselbe Datum \
wie eine Buchung im Buch und wird deshalb nur als „sieht gleich aus“ gezeigt, \
und Sie entscheiden, ob sie trotzdem eingelesen wird. Die übrigen {new_lines} \
Zeilen sind neu. Liest man zuerst die CAMT-Datei und danach die CSV-Datei, \
sieht KassenSturz alle Zeilen schon im Buch.\n\n\
Die CSV-Einstellungen (Trennzeichen „;“, Dezimalkomma, Tag.Monat.Jahr, Spalten \
Buchungstag, Valuta, Name Zahlungsbeteiligter, IBAN Zahlungsbeteiligter, \
Verwendungszweck, Betrag, Währung) stehen in `{slug}.csv.settings`.\n",
        title = example.title,
        food = example.food,
        food_total = german(card4.food),
        income = german(card4.income),
        expenses = german(card4.expenses),
        net = german(net),
        more = if net.minor_units() > 0 { "Ja" } else { "Nein" },
        account = example.account,
        slug = example.slug,
        lines = camt.lines,
        c_new = camt.new,
        c_dup = camt.duplicates,
        c_sim = camt.similar,
        closing = german(camt.bank_closing.expect("a closing balance")),
        after = german(camt.balance_after),
        s_new = csv.new,
        s_dup = csv.duplicates,
        s_sim = csv.similar,
        new_lines = camt.lines - 1,
    );
    sheet
}

/// Compares `text` with the committed file, or writes it when blessing.
fn check_file(path: &PathBuf, text: &str) {
    if bless() {
        fs::create_dir_all(path.parent().expect("a folder")).expect("folder");
        fs::write(path, text).expect("written");
        return;
    }
    let committed =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert_eq!(
        committed,
        text,
        "{} differs; if the new output is right, run the test with KASSENSTURZ_BLESS=1",
        path.display()
    );
}

fn check_example(slug: &str) {
    let example = EXAMPLES
        .iter()
        .find(|example| example.slug == slug)
        .expect("an example");
    let path = books().join("examples/answers").join(format!("{slug}.md"));
    check_file(&path, &answer_sheet(example));
}

#[test]
fn suburban_family_answer_sheet() {
    check_example("suburban-family");
}

#[test]
fn flower_utopia_answer_sheet() {
    check_example("flower-utopia");
}

#[test]
fn udssr_agent_answer_sheet() {
    check_example("udssr-agent");
}

#[test]
fn space_colony_answer_sheet() {
    check_example("space-colony");
}

/// Every new book has a food category with spending in each of its months,
/// last month included, and at least one Transfer (a part pair joined
/// between two own accounts).
#[test]
fn every_example_has_food_in_last_month_and_a_transfer() {
    let (year, month) = reference();
    let today = Date::constant(year, month, 1);
    for example in &EXAMPLES {
        let book = the_book(example);
        let card4 = overview(&book, example, today);
        assert!(
            card4.food.minor_units() > 0,
            "{} has no food spending last month",
            example.slug
        );
        // A Transfer: every part of the transaction is booked to one of the
        // user's own accounts, not to a category.
        let own: Vec<AccountId> = book
            .accounts(false)
            .expect("accounts")
            .into_iter()
            .map(|account| account.id)
            .collect();
        let transfers = book
            .transactions()
            .expect("transactions")
            .iter()
            .filter(|transaction| {
                transaction.splits().len() == 2
                    && transaction
                        .splits()
                        .iter()
                        .all(|part| part.account.is_some_and(|id| own.contains(&id)))
            })
            .count();
        assert!(transfers > 0, "{} has no Transfer", example.slug);
    }
}

// ------------------------------------------------------ Flower Utopia export

const BUDGETS: [(&str, i64); 5] = [
    ("Essen und Trinken", 8_000),
    ("Saatgut und Pflanzen", 6_000),
    ("Wasser", 4_000),
    ("Erde und Dünger", 5_000),
    ("Geräte und Werkzeug", 3_500),
];

/// The Overview Flower Utopia is saved with: a first look for a garden
/// club's treasurer. Like the Space Colony Overview, it starts on last month.
const OVERVIEW: &str = r#"{
  "format": "kassensturz-dashboard", "version": 1, "kind": "dashboard",
  "name": "Übersicht",
  "filters": {"period": "last-month"},
  "components": [
    {"id": "welcome", "type": "text.heading",
     "props": {"text": "Willkommen in der Blumenwiese! So steht es um die Gemeinschaftskasse."},
     "frame": {"col": 0, "row": 0, "cols": 12, "rows": 1}},
    {"id": "worth", "type": "card.net_worth",
     "frame": {"col": 0, "row": 1, "cols": 12, "rows": 2}},
    {"id": "income", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "income", "compare": "previous_period"},
     "frame": {"col": 0, "row": 3, "cols": 3, "rows": 2}},
    {"id": "expenses", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "expenses", "compare": "previous_period"},
     "frame": {"col": 3, "row": 3, "cols": 3, "rows": 2}},
    {"id": "left", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "net", "compare": "previous_period"},
     "frame": {"col": 6, "row": 3, "cols": 3, "rows": 2}},
    {"id": "kept", "type": "kpi.card", "dataset": "income_expense_by_month",
     "params": {"measure": "savings-rate", "compare": "previous_period"},
     "frame": {"col": 9, "row": 3, "cols": 3, "rows": 2}},
    {"id": "budgets", "type": "budget.progress", "dataset": "budget_utilization",
     "props": {"title": "Ihre Budgets"},
     "frame": {"col": 0, "row": 5, "cols": 12, "rows": 4}},
    {"id": "spending", "type": "chart.donut", "dataset": "spending_by_category",
     "params": {"grouping": "top_level_category"},
     "props": {"title": "Wofür das Geld ausgegeben wurde"},
     "frame": {"col": 0, "row": 9, "cols": 6, "rows": 5}},
    {"id": "biggest", "type": "table.rows", "dataset": "spending_by_category",
     "params": {"grouping": "top_level_category"},
     "props": {"title": "Die größten Ausgaben"},
     "frame": {"col": 6, "row": 9, "cols": 6, "rows": 5}},
    {"id": "trend", "type": "chart.line", "dataset": "income_expense_by_month",
     "props": {"period": "last-12-months", "title": "Einnahmen und Ausgaben, Monat für Monat"},
     "frame": {"col": 0, "row": 14, "cols": 12, "rows": 4}}
  ]
}"#;

fn flower_utopia_with_budgets() -> Book {
    let example = &EXAMPLES[1];
    let mut book = the_book(example);
    let categories: HashMap<String, _> = book
        .categories()
        .expect("categories")
        .into_iter()
        .map(|entry| (entry.category.name.clone(), entry.category.id))
        .collect();
    let (year, month) = reference();
    // The first month of the book: three months before the reference month.
    let first = if month > 3 {
        Date::constant(year, month - 3, 1)
    } else {
        Date::constant(year - 1, month + 9, 1)
    };
    for (name, minor) in BUDGETS {
        let category = categories[name];
        let currency = book.account(category).expect("the category").currency;
        book.add_budget(
            category,
            BudgetEvery::Month,
            first,
            None,
            Money::from_minor_units(minor, currency),
            false,
        )
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
    book.add_view(OVERVIEW).expect("the Overview");
    book
}

/// The book's complete export, with every ID numbered in the order it first
/// appears: the same book always gives the same file.
fn exported(book: &Book) -> String {
    let data = export_book(book).expect("gathered");
    let bytes = export(&BookExport, &data).expect("written");
    let text = String::from_utf8(bytes).expect("UTF-8");
    let mut numbers: HashMap<&str, usize> = HashMap::new();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        if rest.len() >= 36
            && is_id(&rest.as_bytes()[..36])
            && !rest[36..].starts_with(|c: char| c.is_ascii_hexdigit())
        {
            let next = numbers.len() + 1;
            let number = *numbers.entry(&rest[..36]).or_insert(next);
            out.push_str(&format!("019a0000-0000-7000-8000-{number:012x}"));
            at += 36;
        } else {
            let character = rest.chars().next().expect("text left");
            out.push(character);
            at += character.len_utf8();
        }
    }
    out
}

fn is_id(bytes: &[u8]) -> bool {
    bytes.iter().enumerate().all(|(place, &byte)| match place {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    })
}

/// Flower Utopia as a complete export: the book plus its budgets and its
/// saved Overview, which a KMyMoney file cannot hold. This is the file the
/// chooser opens for it.
#[test]
fn flower_utopia_export_is_the_committed_file() {
    let book = flower_utopia_with_budgets();
    check_file(
        &books().join("examples/flower-utopia.json"),
        &exported(&book),
    );
}

/// The saved Overview reads, starts on last month, and the budgets have
/// figures for last month.
#[test]
fn flower_utopia_budgets_have_figures_for_last_month() {
    use kassensturz_report::{BudgetGrouping, Document, budget_utilization};

    let text = fs::read_to_string(books().join("examples/flower-utopia.json")).expect("the export");
    let data = read_book(text.as_bytes()).expect("a complete export");
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book(&mut book, &data).expect("filled");
    let views = book.views(None).expect("views");
    assert_eq!(views.len(), 1);
    let document = Document::read(&views[0].document).expect("a valid dashboard");
    assert_eq!(document.name, "Übersicht");
    assert_eq!(document.filters().period.as_deref(), Some("last-month"));

    let (year, month) = reference();
    let today = Date::constant(year, month, 1);
    let period = Period::last_month(today).expect("last month");
    let source = Source::read_with_budgets(&book, period, today).expect("read");
    let rows = budget_utilization(
        &source,
        period,
        today,
        BudgetGrouping::Period,
        &Filter::default(),
    )
    .expect("figures");
    assert_eq!(rows.len(), BUDGETS.len());
    let spent = rows
        .iter()
        .filter(|row| row.periods[0].spent.minor_units() > 0)
        .count();
    assert!(spent >= 3, "{spent} budgets with spending last month");
}
