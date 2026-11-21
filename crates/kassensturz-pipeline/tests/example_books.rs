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
//! hold the 24 months before it and some cash and card spending in it, and
//! "last month" is the one just before it. The app moves a book's dates by
//! whole months when it makes the book, so the reference month becomes the
//! running one (D-584); the figures of last month stay the same. To write the sheets (and Flower Utopia's complete export) again after
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
    Choices, EntryOutcome, ImportReport, RuleScope, example_dashboards, export_book, fill_book,
    fill_book_from, fill_book_from_with_dashboards, import_statements, resolve_names,
};
use kassensturz_report::{
    Filter, Level, Period, Source, income_expense_by_month, spending_by_category,
};
use kassensturz_store::Book;

/// One example book: its slug, title, the KMyMoney file it is read from
/// (relative to `tests/books/`), the account the statements belong to, the
/// food category (top level), the book's own currency, and whether its
/// answer sheet is in English (the English book, D-843) or German.
struct Example {
    slug: &'static str,
    title: &'static str,
    file: &'static str,
    account: &'static str,
    food: &'static str,
    currency: &'static str,
    english: bool,
}

const EXAMPLES: [Example; 5] = [
    Example {
        slug: "suburban-family",
        title: "Suburban Family (Familie Sommerfeld)",
        file: "examples/suburban-family-v15.sqlite",
        account: "Girokonto Jonas",
        food: "Essen",
        currency: "EUR",
        english: false,
    },
    Example {
        slug: "suburban-family-en",
        title: "Suburban Family (English, the Summerfield Family)",
        file: "examples/suburban-family-en-v15.sqlite",
        account: "Jonas's checking account",
        food: "Food",
        currency: "EUR",
        english: true,
    },
    Example {
        slug: "flower-utopia",
        title: "Flower Utopia (Blumenwiese Gartengemeinschaft)",
        file: "examples/flower-utopia-v15.sqlite",
        account: "Vereinskonto",
        food: "Essen und Trinken",
        currency: "EUR",
        english: false,
    },
    Example {
        slug: "udssr-agent",
        title: "UdSSR Government Agent (Hauptverwaltung für Windstille)",
        file: "examples/udssr-agent-v15.sqlite",
        account: "Gehaltskonto",
        food: "Verpflegung",
        currency: "RUB",
        english: false,
    },
    Example {
        slug: "space-colony",
        title: "Space Colony",
        file: "space-colony-5y-v15.sqlite",
        account: "Everyday Checking",
        food: "Food",
        currency: "EUR",
        english: false,
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
    written(money, '.', ',')
}

/// An amount as an English reader writes it: `1,234.56 EUR`.
fn english(money: Money) -> String {
    written(money, ',', '.')
}

fn written(money: Money, group: char, decimal: char) -> String {
    let minor = money.minor_units();
    let sign = if minor < 0 { "-" } else { "" };
    let minor = minor.unsigned_abs();
    let whole = (minor / 100).to_string();
    let mut grouped = String::new();
    for (place, digit) in whole.chars().rev().enumerate() {
        if place > 0 && place % 3 == 0 {
            grouped.push(group);
        }
        grouped.push(digit);
    }
    let grouped: String = grouped.chars().rev().collect();
    format!(
        "{sign}{grouped}{decimal}{:02} {}",
        minor % 100,
        money.currency()
    )
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

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
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
    let months = if example.english { MONTHS_EN } else { MONTHS };
    let last_name = format!(
        "{} {}",
        months[usize::from(last.month().unsigned_abs()) - 1],
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
    if example.english {
        return english_sheet(example, &card4, net, &last_name, &camt, &csv);
    }
    let mut sheet = String::new();
    let _ = write!(
        sheet,
        "# Antwortbogen: {title}\n\n\
Dieses Blatt schreibt der Test `example_books` (`crates/kassensturz-pipeline/tests/example_books.rs`) \
aus dem echten Import- und Berichtscode, nicht von Hand. Das Buch und die \
Kontoauszüge macht `tools/example-books/generate.py`; nichts darin gehört zu \
einer echten Person.\n\n\
Referenzmonat: {year}-{month:02} (der letzte Monat ist {last_name}). Das Buch \
reicht etwa zwei Jahre zurück. Legen Sie es in einem späteren Monat an, \
verschiebt KassenSturz alle Daten um ganze Monate, sodass der Referenzmonat der \
laufende Monat ist; die Zahlen unten gelten dann für den Monat vor dem \
laufenden. Buchungen nach dem heutigen Tag lässt KassenSturz weg. Die \
Kontoauszüge passen nur im Referenzmonat selbst zum Buch.\n\n\
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

/// The answer sheet of the English book (D-843): the same figures and
/// checks as the German sheets, in English.
fn english_sheet(
    example: &Example,
    card4: &Overview,
    net: Money,
    last_name: &str,
    camt: &Imports,
    csv: &Imports,
) -> String {
    let (year, month) = reference();
    let mut sheet = String::new();
    let _ = write!(
        sheet,
        "# Answer sheet: {title}\n\n\
This sheet is written by the test `example_books` (`crates/kassensturz-pipeline/tests/example_books.rs`) \
from the real import and report code, not by hand. The book and the \
statements are made by `tools/example-books/generate.py`; nothing in them \
belongs to a real person.\n\n\
Reference month: {year}-{month:02} (last month is {last_name}). The book goes \
back about two years. If you make it in a later month, KassenSturz moves every \
date by whole months so that the reference month is the running month; the \
figures below are then those of the month before the running one. KassenSturz \
leaves out bookings after today. The statements match the book only in the \
reference month itself.\n\n\
## Card 4: Overview\n\n\
- Spent on \u{201c}{food}\u{201d} in {last_name}: **{food_total}**\n\
- Money in during {last_name}: {income}; money out: {expenses}; \
difference: {net}\n\
- Did more come in than go out? **{more}**\n\n\
## Card 5: Read a bank statement (account \u{201c}{account}\u{201d})\n\n\
Both files are in `crates/kassensturz-import/tests/books/examples/statements/` \
and hold the same {lines} lines; exactly one of them is already in the book.\n\n\
| File | Lines | new | already in the book (duplicate) | looks the same (you decide) | closing balance from the bank | balance after reading |\n\
|---|---|---|---|---|---|---|\n\
| `{slug}.camt053.xml` | {lines} | {c_new} | {c_dup} | {c_sim} | {closing} | {after} |\n\
| `{slug}.csv` | {lines} | {s_new} | {s_dup} | {s_sim} | (a CSV file has none) | – |\n\n\
A CAMT file carries the bank's number for every line, so KassenSturz safely \
finds the line that is already there as a duplicate. A CSV file has no such \
number; the line that is already there has the same amount and almost the same \
date as a booking in the book, so it is only shown as \u{201c}looks the \
same\u{201d} and you decide whether to read it anyway. The other {new_lines} \
lines are new. Reading the CAMT file first and the CSV file after it, \
KassenSturz finds every line already in the book.\n\n\
The CSV settings (delimiter \u{201c},\u{201d}, decimal point, year-month-day, \
columns Booking date, Value date, Payee, Payee IBAN, Reference, Amount, \
Currency) are in `{slug}.csv.settings`.\n",
        title = example.title,
        food = example.food,
        food_total = english(card4.food),
        income = english(card4.income),
        expenses = english(card4.expenses),
        net = english(net),
        more = if net.minor_units() > 0 { "Yes" } else { "No" },
        account = example.account,
        slug = example.slug,
        lines = camt.lines,
        c_new = camt.new,
        c_dup = camt.duplicates,
        c_sim = camt.similar,
        closing = english(camt.bank_closing.expect("a closing balance")),
        after = english(camt.balance_after),
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
fn english_suburban_family_answer_sheet() {
    check_example("suburban-family-en");
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

// ------------------------------------------- the examples' complete exports

/// A monthly budget of an example book: its category, its amount in minor
/// units, and its first and last month counted from the reference month
/// (negative: before it); no last month means it runs on.
struct BudgetPlan {
    category: &'static str,
    minor: i64,
    from: i32,
    until: Option<i32>,
}

const fn monthly(category: &'static str, minor: i64, from: i32, until: Option<i32>) -> BudgetPlan {
    BudgetPlan {
        category,
        minor,
        from,
        until,
    }
}

/// Flower Utopia: five budgets from the book's first month on.
const FLOWER_BUDGETS: [BudgetPlan; 5] = [
    monthly("Essen und Trinken", 8_000, -24, None),
    monthly("Saatgut und Pflanzen", 6_000, -24, None),
    monthly("Wasser", 4_000, -24, None),
    monthly("Erde und Dünger", 5_000, -24, None),
    monthly("Geräte und Werkzeug", 3_500, -24, None),
];

/// Suburban Family (D-946): the household's groceries, close to used up by
/// the third week of the month, the kitchen's 36 rates (the 14th is due in
/// the reference month, see `tools/example-books/generate.py`) and the
/// e-bike's 24 rates from the month after the reference month.
const SUBURBAN_BUDGETS: [BudgetPlan; 3] = [
    monthly("Lebensmittel", 110_000, -24, None),
    monthly("Ratenkauf Küche", 18_900, -13, Some(22)),
    monthly("Ratenkauf E-Bike", 8_500, 1, Some(24)),
];

/// The same in the English book's words.
const SUBURBAN_BUDGETS_EN: [BudgetPlan; 3] = [
    monthly("Groceries", 110_000, -24, None),
    monthly("Kitchen on instalments", 18_900, -13, Some(22)),
    monthly("E-bike on instalments", 8_500, 1, Some(24)),
];

/// The examples that come as a complete export, which holds what a
/// KMyMoney file cannot (budgets and saved dashboards): slug, file, budgets.
const EXPORTED: [(&str, &str, &[BudgetPlan]); 3] = [
    (
        "flower-utopia",
        "examples/flower-utopia.json",
        &FLOWER_BUDGETS,
    ),
    (
        "suburban-family",
        "examples/suburban-family.json",
        &SUBURBAN_BUDGETS,
    ),
    (
        "suburban-family-en",
        "examples/suburban-family-en.json",
        &SUBURBAN_BUDGETS_EN,
    ),
];

/// The first day of the month `months` after the reference month.
fn month_start(months: i32) -> Date {
    let (year, month) = reference();
    let number = i32::from(year) * 12 + i32::from(month) - 1 + months;
    Date::new(
        i16::try_from(number.div_euclid(12)).expect("a year"),
        i8::try_from(number.rem_euclid(12) + 1).expect("a month"),
        1,
    )
    .expect("a day")
}

/// The example `slug` with its budgets and its dashboards (#829, D-788),
/// which the plain KMyMoney books get when the app fills them.
fn with_budgets(slug: &str, budgets: &[BudgetPlan]) -> Book {
    let example = EXAMPLES
        .iter()
        .find(|example| example.slug == slug)
        .expect("an example");
    let mut book = the_book(example);
    let categories: HashMap<String, _> = book
        .categories()
        .expect("categories")
        .into_iter()
        .map(|entry| (entry.category.name.clone(), entry.category.id))
        .collect();
    for plan in budgets {
        let category = categories[plan.category];
        let currency = book.account(category).expect("the category").currency;
        let last = plan.until.map(|until| {
            let next = month_start(until + 1);
            next.yesterday().expect("a day before")
        });
        book.add_budget(
            &[category],
            &[],
            BudgetEvery::Month,
            month_start(plan.from),
            last,
            Money::from_minor_units(plan.minor, currency),
            false,
        )
        .unwrap_or_else(|error| panic!("{slug} {}: {error:?}", plan.category));
    }
    let accounts = book.all_accounts().expect("accounts");
    for text in example_dashboards(example.slug) {
        let document = resolve_names(&text, &accounts).expect("every name is in the book");
        book.add_view(&document).expect("a dashboard");
    }
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

/// Flower Utopia and the two Suburban Family books as complete exports:
/// the book plus its budgets and its saved dashboards, which a KMyMoney file
/// cannot hold. These are the files the chooser opens for them.
#[test]
fn the_exports_are_the_committed_files() {
    for (slug, file, budgets) in EXPORTED {
        let book = with_budgets(slug, budgets);
        check_file(&books().join(file), &exported(&book));
    }
}

/// The saved dashboards read, the Overview first and marked as the one the
/// book opens at, *Sonstiges* (*More*) after it, and the budgets that run
/// last month have figures for it.
#[test]
fn the_exported_budgets_have_figures_for_last_month() {
    use kassensturz_report::{BudgetGrouping, Document, budget_utilization};

    for (slug, file, budgets) in EXPORTED {
        let text = fs::read_to_string(books().join(file)).expect("the export");
        let data = read_book(text.as_bytes()).expect("a complete export");
        let mut book = Book::create_in_memory().expect("a book in memory");
        fill_book(&mut book, &data).expect("filled");
        assert_eq!(
            book.budgets().expect("budgets").len(),
            budgets.len(),
            "{slug}"
        );
        let views = book.views(None).expect("views");
        assert_eq!(views.len(), 2, "{slug}");
        let document = Document::read(&views[0].document).expect("a valid dashboard");
        let english = slug.ends_with("-en");
        assert_eq!(
            document.name,
            if english { "Overview" } else { "Übersicht" }
        );
        assert!(document.default);
        let others = Document::read(&views[1].document).expect("valid");
        assert!(!others.default);
        assert_eq!(others.name, if english { "More" } else { "Sonstiges" });

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
            None,
        )
        .expect("figures");
        let running = budgets
            .iter()
            .filter(|plan| plan.from <= -1 && plan.until.is_none_or(|until| until >= -1))
            .count();
        let spent = rows
            .iter()
            .filter(|row| {
                row.periods
                    .first()
                    .is_some_and(|figures| figures.spent.minor_units() > 0)
            })
            .count();
        assert!(
            spent >= running.min(3),
            "{slug}: {spent} budgets with spending last month"
        );
    }
}

/// #829, D-788, D-946: each KMyMoney example, filled with its dashboards as
/// the app fills it, has two readable dashboards in its own language, the
/// Overview first and marked, then *Sonstiges* (*More*); every account and
/// category they name is the book's; the warning finds its account, the
/// food goal counts the food category's spending and *Left of the salary*
/// has a figure.
#[test]
fn every_example_comes_with_two_dashboards_that_name_its_own_accounts() {
    use kassensturz_core::ViewKind;
    use kassensturz_report::{
        AlarmState, Document, Formula, FormulaValue, Item, Setting, evaluate_alarms, templates,
    };

    let (year, month) = reference();
    let today = Date::constant(year, month, 1);
    for example in &EXAMPLES {
        let bytes = fs::read(books().join(example.file)).expect("the KMyMoney book");
        let Imported::Book(imported) =
            import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
        else {
            panic!("a book");
        };
        let mut book = Book::create_in_memory().expect("a book in memory");
        let dashboards = example_dashboards(example.slug);
        assert_eq!(dashboards.len(), 2, "{}", example.slug);
        fill_book_from_with_dashboards(
            &mut book,
            &imported,
            &HashMap::new(),
            RuleScope::default(),
            today,
            &dashboards,
        )
        .expect("filled");
        let views = book.views(Some(ViewKind::Dashboard)).expect("views");
        assert_eq!(views.len(), 2, "{}: none left out", example.slug);
        let ids: Vec<String> = book
            .all_accounts()
            .expect("accounts")
            .into_iter()
            .map(|account| account.id.to_string())
            .collect();
        let overview_name = if example.slug == "space-colony" || example.english {
            "Overview"
        } else {
            "Übersicht"
        };
        let currency = code(example.currency);
        let period = Period::last_month(today).expect("last month");
        let source = Source::read_with_budgets(&book, period, today).expect("read");
        let (mut alarms, mut goals, mut rests) = (0, 0, 0);
        for (index, view) in views.iter().enumerate() {
            let document = Document::read(&view.document)
                .unwrap_or_else(|error| panic!("{}: {error:?}", example.slug));
            assert_eq!(document.default, index == 0, "{}", example.slug);
            if index == 0 {
                assert_eq!(document.name, overview_name);
                assert_eq!(document.template.as_deref(), Some(templates::OVERVIEW));
            }
            for entry in &document.entries {
                let Item::Component(component) = &entry.item else {
                    panic!("{}: {} is of a known type", example.slug, entry.id);
                };
                for (_, setting) in component.settings() {
                    if let Setting::Ids(named) = setting {
                        for id in named {
                            assert!(ids.contains(id), "{}: {}", example.slug, entry.id);
                        }
                    }
                }
                if let Some(Setting::Alarms(rules)) = component.get("alarms") {
                    for result in evaluate_alarms(&book, rules, today).expect("checked") {
                        alarms += 1;
                        assert!(
                            matches!(result.state, AlarmState::Fine | AlarmState::Triggered),
                            "{}: {:?}",
                            example.slug,
                            result.state
                        );
                    }
                }
                if let Some(Setting::Text(text)) = component.get("formula")
                    && !text.is_empty()
                {
                    let result = Formula::parse(text)
                        .expect("a formula")
                        .evaluate(&source, period, &Filter::default(), currency)
                        .expect("a figure");
                    let FormulaValue::Amount(amount) = result.value else {
                        panic!("{}: an amount", example.slug);
                    };
                    assert_ne!(amount.minor_units(), 0, "{}", example.slug);
                    // The Overview's *Left of the salary* counts more than
                    // the food goal; the goal counts the food category.
                    if entry.id == "rest-of-salary" {
                        rests += 1;
                        continue;
                    }
                    goals += 1;
                    assert!(
                        result
                            .counted
                            .iter()
                            .any(|counted| counted.path == [example.food]),
                        "{}: {:?}",
                        example.slug,
                        result.counted
                    );
                }
            }
        }
        assert_eq!((alarms, goals, rests), (1, 1, 1), "{}", example.slug);
    }
}
