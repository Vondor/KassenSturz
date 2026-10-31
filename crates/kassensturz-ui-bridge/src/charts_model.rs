// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The charts as a QML object (M6a.3, D-276), on the book the app has open
//! (`session.rs`). The work is in `charts.rs`; this file turns a chart model
//! into the plain values QML reads.
//!
//! QML gets labels, names and amounts as text. Each point also has a
//! `value`, a drawing value that only places the mark (AGENTS.md §5), and a
//! `minorUnits` text with the exact amount.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_charts::{ChartModel, HiddenNote, PALETTE_SIZE, Prepared, Series, Slices};
use kassensturz_core::ErrorParam;

use crate::charts::{self, ChartSet, ChartsError, Request};
use crate::number_format::app_locale;
use crate::properties;
use crate::session;
use crate::variant::{list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// One chart of the open book's data per currency. Ask for one with
        /// `show`; `view` then holds an object with `charts` (a list of
        /// charts, empty when there was nothing to show) or an `error`.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BookCharts = super::BookChartsRust;
    }

    extern "RustQt" {
        /// Shows `dataset` (`spending_by_category` or
        /// `income_expense_by_month`) as a chart of `kind` (`line`, `bar`,
        /// `stacked-bar`, `area`, `pie` or `donut`) for `period`
        /// (`this-month`, `last-month`, `last-12-months`, `this-year`,
        /// `last-year` or a month as `YYYY-MM`). `grouping` is `top-level` or `each` and matters for
        /// spending only.
        #[qinvokable]
        fn show(
            self: Pin<&mut BookCharts>,
            dataset: &QString,
            kind: &QString,
            period: &QString,
            grouping: &QString,
        );

        /// As `show`, for a component of a dashboard (M6b.5): `chosen` is the
        /// IDs of My groups (spending) or of the categories and groups of a
        /// chart over time, separated by commas (`params.groups`,
        /// `params.series`), and `show_hidden` shows the hidden categories
        /// anyway (`props.show_hidden`). After the list in `chosen` and a `;`
        /// come the categories and groups spending is limited to, separated
        /// by commas (`params.categories`, D-331); none for all.
        #[qinvokable]
        #[cxx_name = "showWith"]
        fn show_with(
            self: Pin<&mut BookCharts>,
            dataset: &QString,
            kind: &QString,
            period: &QString,
            grouping: &QString,
            chosen: &QString,
            show_hidden: bool,
        );

        /// Reads the open book again for the last request, e.g. after a
        /// screen changed the book or the number format changed.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookCharts>);
    }
}

/// The Rust side of [`qobject::BookCharts`].
#[derive(Default)]
pub struct BookChartsRust {
    view: QVariant,
    request: Option<Request>,
}

impl qobject::BookCharts {
    fn show(
        mut self: Pin<&mut Self>,
        dataset: &QString,
        kind: &QString,
        period: &QString,
        grouping: &QString,
    ) {
        self.as_mut().rust_mut().request = Some(Request {
            dataset: dataset.to_string(),
            kind: kind.to_string(),
            period: period.to_string(),
            grouping: grouping.to_string(),
            ..Request::default()
        });
        self.refresh();
    }

    fn show_with(
        mut self: Pin<&mut Self>,
        dataset: &QString,
        kind: &QString,
        period: &QString,
        grouping: &QString,
        chosen: &QString,
        show_hidden: bool,
    ) {
        // Each dataset reads only the list it uses. The text holds the list of
        // groups or series, then `;` and the list of categories the chart is
        // limited to (D-331).
        // After a second `;` come the columns of the chart as a JSON list of
        // `{"name", "members"}` (D-339); it may hold `;` itself.
        let chosen = chosen.to_string();
        let mut parts = chosen.splitn(3, ';');
        let (chosen, only, columns) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        let columns: Vec<(String, Vec<String>, String)> =
            properties::read_columns(columns, &|formula| Some(formula.to_owned()))
                .unwrap_or_default()
                .into_iter()
                .map(|column| (column.name, column.members, column.formula))
                .collect();
        let id_list = |text: &str| -> Vec<String> {
            text.split(',')
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let ids = id_list(chosen);
        self.as_mut().rust_mut().request = Some(Request {
            dataset: dataset.to_string(),
            kind: kind.to_string(),
            period: period.to_string(),
            grouping: grouping.to_string(),
            groups: ids.clone(),
            categories: id_list(only),
            series: ids,
            show_hidden,
            columns,
        });
        self.refresh();
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some(request) = self.rust().request.clone() else {
            return;
        };
        let locale = app_locale();
        let view = match session::with_book(|book| charts::charts(book, &locale, &request)) {
            Some(result) => view_object(&result),
            None => QVariant::default(),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }
}

/// The marker (a second mark besides the color) of palette index `color`:
/// the first four series get one, the others none, as -1 (D-283).
fn marker(color: u8) -> i32 {
    if color < 4 { i32::from(color) } else { -1 }
}

fn count(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

/// The key QML tells series apart by: the series' fixed name, or for a series
/// the user chose its place, since those share the key `series`.
fn series_key(series: &Series, index: usize) -> String {
    match series.name {
        Some(_) => format!("series-{index}"),
        None => series.key.to_owned(),
    }
}

/// The hidden categories a chart leaves out as QML reads them: `first`, the
/// number of `more` and the `total`, or an empty object when nothing is
/// hidden (the screen writes the sentence, D-034).
fn hidden_object(note: Option<&HiddenNote>) -> QVariant {
    match note {
        Some(note) => map([
            ("first", text(&note.first)),
            ("more", count(note.more)),
            ("total", text(&note.total)),
        ]),
        None => map([]),
    }
}

/// What a round chart says about its slices, as QML reads it: the place of
/// "Other" (-1 for none), what it holds, and for every slice its share as
/// text and whether it carries its name or can't be drawn. Empty lists for
/// the other kinds of chart.
fn slices_object(slices: &Slices) -> QVariant {
    let other = slices
        .other
        .map_or(-1, |index| i32::try_from(index).unwrap_or(-1));
    let flags = |flags: &[bool]| list(flags.iter().map(QVariant::from));
    map([
        ("other", QVariant::from(&other)),
        (
            "holds",
            list(
                slices
                    .holds
                    .iter()
                    .map(|(name, label)| map([("name", text(name)), ("label", text(label))])),
            ),
        ),
        (
            "shares",
            list(slices.shares.iter().map(|share| text(share))),
        ),
        ("named", flags(&slices.named)),
        ("undrawn", flags(&slices.undrawn)),
    ])
}

/// An amount's drawing value, which only places a mark.
fn drawing(value: f64) -> QVariant {
    QVariant::from(&value)
}

fn chart_object(model: &ChartModel, prepared: &Prepared) -> QVariant {
    let round = model.kind.is_round();
    let series =
        model
            .series
            .iter()
            .zip(&prepared.series)
            .enumerate()
            .map(|(index, (series, drawn))| {
                let points = series.points.iter().zip(&drawn.values).enumerate().map(
                    |(index, (point, &value))| {
                        // A slice takes the palette color of its place, a bar or line
                        // the color of its series.
                        let color = if round {
                            ChartModel::color_of(index)
                        } else {
                            series.color
                        };
                        map([
                            ("label", text(&point.label)),
                            ("minorUnits", text(&point.minor_units.to_string())),
                            ("value", drawing(value)),
                            ("color", QVariant::from(&i32::from(color))),
                            ("marker", QVariant::from(&marker(color))),
                        ])
                    },
                );
                map([
                    ("key", text(&series_key(series, index))),
                    ("name", text(series.name.as_deref().unwrap_or(""))),
                    ("color", QVariant::from(&i32::from(series.color))),
                    ("marker", QVariant::from(&marker(series.color))),
                    ("points", list(points)),
                ])
            });
    let ticks = model
        .ticks
        .iter()
        .zip(&prepared.ticks)
        .map(|(tick, &value)| {
            map([
                ("label", text(&tick.label)),
                ("minorUnits", text(&tick.minor_units.to_string())),
                ("value", drawing(value)),
            ])
        });
    let table = map([
        ("currency", text(&model.table.currency)),
        (
            "columns",
            list(model.table.columns.iter().map(|name| text(name))),
        ),
        (
            "rows",
            list(
                model
                    .table
                    .rows
                    .iter()
                    .map(|row| list(row.iter().map(|cell| text(cell)))),
            ),
        ),
    ]);
    let unsorted = model
        .unsorted
        .map_or(-1, |index| i32::try_from(index).unwrap_or(-1));
    map([
        ("kind", text(model.kind.key())),
        ("dataset", text(model.dataset)),
        ("currency", text(&model.currency)),
        ("round", QVariant::from(&round)),
        ("total", text(&model.total)),
        ("hidden", hidden_object(model.hidden.as_ref())),
        ("slices", slices_object(&model.slices)),
        ("paletteSize", QVariant::from(&i32::from(PALETTE_SIZE))),
        ("count", count(model.categories.len())),
        ("unsortedIndex", QVariant::from(&unsorted)),
        (
            "categories",
            list(model.categories.iter().map(|name| text(name))),
        ),
        ("series", list(series)),
        ("ticks", list(ticks)),
        ("table", table),
    ])
}

/// The error of a request as QML gets it: its code, and for an unsupported
/// kind the kind. A request with a name that isn't one of the fixed ones is a
/// mistake in the screen and logged, not shown.
pub(crate) fn error_object(error: &ChartsError) -> QVariant {
    let Some(report) = error.report() else {
        if let ChartsError::UnknownName(what) = error {
            eprintln!("kassensturz: a chart was asked for with an unknown {what}");
        }
        return map([("error", map([("code", text("charts.bad-request"))]))]);
    };
    let params = report.params.iter().map(|(name, value)| {
        let shown = match value {
            ErrorParam::Keyword(word) => (*word).to_owned(),
            _ => String::new(),
        };
        (*name, text(&shown))
    });
    let mut entries = vec![("code", text(report.code))];
    entries.extend(params);
    let cause = report.cause.as_ref().map(|cause| text(cause.code));
    if let Some(cause) = cause {
        entries.push(("cause", cause));
    }
    map([("error", map(entries))])
}

fn view_object(result: &Result<ChartSet, ChartsError>) -> QVariant {
    match result {
        Ok(set) => map([
            ("empty", QVariant::from(&set.charts.is_empty())),
            ("missing", count(set.missing)),
            (
                "shared",
                list(set.shared.iter().map(|shared| {
                    map([
                        ("category", text(&shared.path.join(" › "))),
                        (
                            "columns",
                            list(shared.columns.iter().map(|name| text(name))),
                        ),
                    ])
                })),
            ),
            (
                "totals",
                list(set.totals.iter().map(|(column, amounts)| {
                    map([
                        ("column", text(column)),
                        ("amounts", list(amounts.iter().map(|amount| text(amount)))),
                    ])
                })),
            ),
            (
                "charts",
                list(
                    set.charts
                        .iter()
                        .map(|(model, prepared)| chart_object(model, prepared)),
                ),
            ),
        ]),
        Err(error) => error_object(error),
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{
        Account, AccountId, AccountKind, AccountType, CurrencyCode, Money, Split, Transaction,
        TransactionId,
    };
    use kassensturz_store::Book;

    use super::*;
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{
        elements, flag_of, keys, list_of, number_of, object_of, text_of,
    };

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: EUR,
            account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        id
    }

    /// Salary, rent, groceries and money not sorted yet, in the months
    /// before `TestLocale`'s today.
    fn book() -> Book {
        let mut book = Book::create_in_memory().expect("book created");
        book.use_currency(EUR).expect("currency used");
        let checking = account(&mut book, "Checking account", AccountKind::Asset);
        let rent = account(&mut book, "Rent", AccountKind::Expense);
        let groceries = account(&mut book, "Groceries", AccountKind::Expense);
        let salary = account(&mut book, "Salary", AccountKind::Income);
        let add = |book: &mut Book, month: i8, parts: &[(Option<AccountId>, i64)]| {
            let splits = parts
                .iter()
                .map(|&(account, amount)| match account {
                    Some(account) => Split::new(account, Money::from_minor_units(amount, EUR)),
                    None => Split::uncategorized(Money::from_minor_units(amount, EUR)),
                })
                .collect();
            let day = kassensturz_core::Date::new(2026, month, 3).expect("a date");
            let transaction =
                Transaction::new(TransactionId::generate(), day, None, splits).expect("balanced");
            book.add_transaction(&transaction).expect("added");
        };
        for month in [8, 9] {
            add(
                &mut book,
                month,
                &[(Some(checking), 250_000), (Some(salary), -250_000)],
            );
            add(
                &mut book,
                month,
                &[(Some(checking), -80_000), (Some(rent), 80_000)],
            );
        }
        add(
            &mut book,
            9,
            &[(Some(checking), -5_320), (Some(groceries), 5_320)],
        );
        add(&mut book, 9, &[(Some(checking), -1_234), (None, 1_234)]);
        book
    }

    fn request(dataset: &str, kind: &str) -> Request {
        Request {
            dataset: dataset.into(),
            kind: kind.into(),
            period: "last-12-months".into(),
            grouping: "each".into(),
            ..Request::default()
        }
    }

    fn view_of(request: &Request) -> QVariant {
        let result = charts::charts(&book(), &TestLocale, request);
        view_object(&result)
    }

    #[test]
    fn a_spending_chart_reads_back_the_way_qml_reads_it() {
        let view = view_of(&request("spending_by_category", "pie"));
        assert_eq!(
            keys(&view),
            ["charts", "empty", "missing", "shared", "totals"]
        );
        assert_eq!(number_of(&view, "missing"), 0);
        assert!(!flag_of(&view, "empty"));
        let shown = list_of(&view, "charts");
        assert_eq!(shown.len(), 1);
        let chart = &shown[0];
        assert_eq!(
            keys(chart),
            [
                "categories",
                "count",
                "currency",
                "dataset",
                "hidden",
                "kind",
                "paletteSize",
                "round",
                "series",
                "slices",
                "table",
                "ticks",
                "total",
                "unsortedIndex"
            ]
        );
        assert_eq!(text_of(chart, "kind"), "pie");
        assert_eq!(text_of(chart, "dataset"), "spending_by_category");
        assert_eq!(text_of(chart, "currency"), "EUR");
        assert!(flag_of(chart, "round"));
        assert_eq!(number_of(chart, "paletteSize"), 8);
        assert_eq!(number_of(chart, "count"), 3);
        assert_eq!(text_of(chart, "total"), "1.665,54 €".replace('.', ""));
        let names: Vec<String> = list_of(chart, "categories")
            .iter()
            .map(|name| name.value::<QString>().expect("text").to_string())
            .collect();
        // Money not sorted yet has no text here: the screen writes it.
        assert_eq!(names, ["Rent", "Groceries", ""]);
        assert_eq!(number_of(chart, "unsortedIndex"), 2);
        assert!(list_of(chart, "ticks").is_empty());

        let series = list_of(chart, "series");
        assert_eq!(series.len(), 1);
        assert_eq!(
            keys(&series[0]),
            ["color", "key", "marker", "name", "points"]
        );
        assert_eq!(text_of(&series[0], "name"), "");
        let slices = object_of(chart, "slices");
        assert_eq!(
            keys(&slices),
            ["holds", "named", "other", "shares", "undrawn"]
        );
        assert_eq!(number_of(&slices, "other"), -1);
        assert!(list_of(&slices, "holds").is_empty());
        let shares: Vec<String> = list_of(&slices, "shares")
            .iter()
            .map(|share| share.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(shares.len(), 3);
        assert!(shares[0].ends_with('%'), "{shares:?}");
        let named: Vec<bool> = list_of(&slices, "named")
            .iter()
            .map(|flag| flag.value::<bool>().expect("flag"))
            .collect();
        // Rent is nearly all of it; the 12 euros not sorted yet are under 5 %.
        assert_eq!(named, [true, false, false]);
        assert!(
            list_of(&slices, "undrawn")
                .iter()
                .all(|f| f.value::<bool>() == Some(false))
        );
        // Nothing is hidden in this book: an empty object.
        assert!(keys(&object_of(chart, "hidden")).is_empty());
        assert_eq!(text_of(&series[0], "key"), "spending");
        let points = list_of(&series[0], "points");
        assert_eq!(
            keys(&points[0]),
            ["color", "label", "marker", "minorUnits", "value"]
        );
        // Amounts are text; the drawing value is a number.
        assert_eq!(text_of(&points[0], "label"), "1600,00 €");
        assert_eq!(text_of(&points[0], "minorUnits"), "160000");
        assert_eq!(
            points[0]
                .value::<f64>()
                .or_else(|| object_of(&points[0], "value").value::<f64>()),
            Some(160_000.0)
        );
        // A slice takes the color of its place; the first four have a marker.
        let colors: Vec<i32> = points.iter().map(|p| number_of(p, "color")).collect();
        assert_eq!(colors, [0, 1, 2]);
        let markers: Vec<i32> = points.iter().map(|p| number_of(p, "marker")).collect();
        assert_eq!(markers, [0, 1, 2]);

        let table = object_of(chart, "table");
        assert_eq!(keys(&table), ["columns", "currency", "rows"]);
        assert_eq!(text_of(&table, "currency"), "EUR");
        let rows = list_of(&table, "rows");
        assert_eq!(rows.len(), 3);
        let cells = elements(&rows[0]).expect("a row");
        assert_eq!(cells.len(), 2);
        assert_eq!(
            cells[1].value::<QString>().expect("text").to_string(),
            "1600,00 €"
        );
    }

    #[test]
    fn a_monthly_chart_has_an_axis_and_series_that_read_back() {
        let view = view_of(&request("income_expense_by_month", "bar"));
        let chart = &list_of(&view, "charts")[0];
        assert!(!flag_of(chart, "round"));
        assert_eq!(number_of(chart, "unsortedIndex"), -1);
        // Only a round chart has anything to say about slices.
        let slices = object_of(chart, "slices");
        assert!(list_of(&slices, "shares").is_empty());
        assert_eq!(number_of(&slices, "other"), -1);
        let series = list_of(chart, "series");
        let keys_of: Vec<String> = series.iter().map(|s| text_of(s, "key")).collect();
        assert_eq!(keys_of, ["income", "expenses"]);
        assert_eq!(number_of(&series[1], "color"), 1);
        let ticks = list_of(chart, "ticks");
        assert_eq!(keys(&ticks[0]), ["label", "minorUnits", "value"]);
        assert_eq!(text_of(&ticks[0], "minorUnits"), "0");
        assert_eq!(text_of(&ticks[1], "minorUnits"), "50000");
        assert_eq!(text_of(&ticks[1], "label"), "500,00 €");
        // Twelve months, also those in which nothing happened.
        assert_eq!(number_of(chart, "count"), 12);
        assert_eq!(list_of(&series[0], "points").len(), 12);
    }

    #[test]
    fn an_empty_book_gives_no_charts() {
        let empty = Book::create_in_memory().expect("book created");
        let result = charts::charts(&empty, &TestLocale, &request("spending_by_category", "bar"));
        let view = view_object(&result);
        assert!(flag_of(&view, "empty"));
        assert!(list_of(&view, "charts").is_empty());
    }

    #[test]
    fn a_kind_the_dataset_cannot_show_is_an_error_object() {
        let view = view_of(&request("spending_by_category", "line"));
        assert_eq!(keys(&view), ["error"]);
        let error = object_of(&view, "error");
        assert_eq!(text_of(&error, "code"), "charts.kind-not-supported");
        assert_eq!(text_of(&error, "kind"), "line");
    }

    #[test]
    fn a_name_that_is_not_fixed_is_a_mistake_in_the_screen() {
        for bad in [
            Request {
                dataset: "balance".into(),
                ..request("x", "bar")
            },
            Request {
                kind: "radar".into(),
                ..request("spending_by_category", "bar")
            },
            Request {
                period: "forever".into(),
                ..request("spending_by_category", "bar")
            },
            Request {
                grouping: "some".into(),
                ..request("spending_by_category", "bar")
            },
        ] {
            let view = view_of(&bad);
            assert_eq!(
                text_of(&object_of(&view, "error"), "code"),
                "charts.bad-request",
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_exact_amount_and_the_shown_text_are_kept_apart() {
        // The text is for the user's locale; the exact amount is plain
        // digits that no locale changes.
        let view = view_of(&request("spending_by_category", "bar"));
        let chart = &list_of(&view, "charts")[0];
        let points = list_of(&list_of(chart, "series")[0], "points");
        let exact: Vec<String> = points.iter().map(|p| text_of(p, "minorUnits")).collect();
        assert_eq!(exact, ["160000", "5320", "1234"]);
    }

    fn category_id(book: &Book, name: &str) -> kassensturz_core::AccountId {
        book.categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.category.name == name)
            .expect("category")
            .category
            .id
    }

    fn view_with(book: &Book, request: &Request) -> QVariant {
        view_object(&charts::charts(book, &TestLocale, request))
    }

    #[test]
    fn the_hidden_note_reads_back_and_show_hidden_takes_it_away() {
        let mut book = book();
        let rent = category_id(&book, "Rent");
        book.set_hidden_categories(&[rent]).expect("hidden");
        let mut asked = request("spending_by_category", "pie");
        let view = view_with(&book, &asked);
        let chart = &list_of(&view, "charts")[0];
        let hidden = object_of(chart, "hidden");
        assert_eq!(keys(&hidden), ["first", "more", "total"]);
        assert_eq!(text_of(&hidden, "first"), "Rent");
        assert_eq!(number_of(&hidden, "more"), 0);
        assert_eq!(text_of(&hidden, "total"), "1600,00 €");
        // The chart and its table hold what is shown.
        assert_eq!(text_of(chart, "total"), "65,54 €");
        assert_eq!(number_of(chart, "count"), 2);
        asked.show_hidden = true;
        let view = view_with(&book, &asked);
        let chart = &list_of(&view, "charts")[0];
        assert!(keys(&object_of(chart, "hidden")).is_empty());
        assert_eq!(number_of(chart, "count"), 3);
    }

    #[test]
    fn columns_are_the_rows_and_the_series_and_a_shared_category_is_named() {
        let book = book();
        let groceries = category_id(&book, "Groceries");
        let rent = category_id(&book, "Rent");
        let mut asked = request("spending_by_category", "pie");
        asked.columns = vec![
            ("Home".into(), vec![rent.to_string()], String::new()),
            (
                "Both".into(),
                vec![
                    groceries.to_string(),
                    rent.to_string(),
                    "no-such-category".into(),
                ],
                String::new(),
            ),
        ];
        let view = view_with(&book, &asked);
        assert_eq!(number_of(&view, "missing"), 1);
        let chart = &list_of(&view, "charts")[0];
        let names: Vec<String> = list_of(chart, "categories")
            .iter()
            .map(|name| name.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(names, ["Both", "Home"]);
        let shared = list_of(&view, "shared");
        assert_eq!(shared.len(), 1);
        assert_eq!(text_of(&shared[0], "category"), "Rent");
        let columns: Vec<String> = list_of(&shared[0], "columns")
            .iter()
            .map(|name| name.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(columns, ["Home", "Both"]);
        // Each column's figure for its card on the board, in the order of the
        // columns, one text per currency.
        let totals = list_of(&view, "totals");
        assert_eq!(totals.len(), 2);
        assert_eq!(text_of(&totals[0], "column"), "Home");
        assert_eq!(text_of(&totals[1], "column"), "Both");
        for total in &totals {
            let amounts = list_of(total, "amounts");
            assert_eq!(amounts.len(), 1);
            let amount = amounts[0].value::<QString>().expect("text").to_string();
            assert!(!amount.is_empty());
        }
        // The same columns month by month, as lines.
        let mut lines = asked.clone();
        lines.kind = "line".into();
        let view = view_with(&book, &lines);
        assert!(!flag_of(&view, "empty"));
        assert_eq!(list_of(&view, "shared").len(), 1);
    }

    #[test]
    fn groups_and_series_of_a_document_reach_the_chart_and_unknown_ids_are_counted() {
        let mut book = book();
        let food = category_id(&book, "Groceries");
        let rent = category_id(&book, "Rent");
        let household = book
            .add_category_group(
                "Household",
                kassensturz_core::GroupKind::Expense,
                &[kassensturz_core::GroupMember::Category(food)],
            )
            .expect("group");
        let mut asked = request("spending_by_category", "pie");
        asked.grouping = "groups".into();
        asked.groups = vec![
            household.to_string(),
            "no-such-group".into(),
            rent.to_string(),
        ];
        let view = view_with(&book, &asked);
        // A category's ID is not a group, and the other is unknown.
        assert_eq!(number_of(&view, "missing"), 2);
        let chart = &list_of(&view, "charts")[0];
        let names: Vec<String> = list_of(chart, "categories")
            .iter()
            .map(|name| name.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(names, ["Rent", "Household", ""]);

        // The same groups are ignored by a grouping that doesn't show them.
        asked.grouping = "each".into();
        let view = view_with(&book, &asked);
        assert_eq!(number_of(&view, "missing"), 0);
        assert_eq!(number_of(&list_of(&view, "charts")[0], "count"), 3);

        // Series of a chart over time are named by the book.
        let mut over_time = request("category_over_time", "line");
        over_time.series = vec![rent.to_string(), household.to_string()];
        let view = view_with(&book, &over_time);
        let chart = &list_of(&view, "charts")[0];
        let series = list_of(chart, "series");
        let keys_of: Vec<String> = series.iter().map(|s| text_of(s, "key")).collect();
        assert_eq!(keys_of, ["series-0", "series-1"]);
        let named: Vec<String> = series.iter().map(|s| text_of(s, "name")).collect();
        assert_eq!(named, ["Rent", "Household"]);
        assert_eq!(number_of(chart, "count"), 12);
    }

    #[test]
    fn a_typed_range_reaches_the_chart_with_the_books_first_day_as_an_open_start() {
        let book = book();
        let total = |period: &str| {
            let mut asked = request("spending_by_category", "bar");
            asked.show_hidden = true;
            asked.period = period.into();
            let view = view_with(&book, &asked);
            list_of(&view, "charts")
                .first()
                .map(|chart| text_of(chart, "total"))
        };
        let everything = total("last-12-months");
        // The book has spending in August and September 2026 only.
        assert_eq!(total("..2026-09-30"), everything);
        assert_eq!(total("2026-08-01..2026-09-30"), everything);
        assert_eq!(total("2026-08-01.."), everything);
        let september = total("2026-09-01..2026-09-30");
        assert!(september.is_some() && september != everything);
        assert_eq!(total("2026-09-01.."), september);
        assert_eq!(total("..2026-07-31"), None);
        // A range that is no period is refused, not shown as something else.
        let mut asked = request("spending_by_category", "bar");
        asked.period = "2026-09-30..2026-09-01".into();
        let shown = charts::charts(&book, &TestLocale, &asked);
        assert!(shown.is_err());
    }

    #[test]
    fn the_chosen_categories_limit_a_chart_and_a_table_and_unknown_ones_are_counted() {
        let book = book();
        let rent = category_id(&book, "Rent");
        let mut asked = request("spending_by_category", "pie");
        asked.categories = vec![rent.to_string(), "no-such-category".into()];
        let view = view_with(&book, &asked);
        assert_eq!(number_of(&view, "missing"), 1);
        let chart = &list_of(&view, "charts")[0];
        assert_eq!(number_of(chart, "count"), 1);
        assert_eq!(text_of(chart, "total"), "1600,00 €");
        // The table shows what the chart shows.
        let rows = list_of(&object_of(chart, "table"), "rows");
        assert_eq!(rows.len(), 1);
        asked.categories = Vec::new();
        let view = view_with(&book, &asked);
        assert_eq!(number_of(&view, "missing"), 0);
        assert_eq!(number_of(&list_of(&view, "charts")[0], "count"), 3);
    }

    #[test]
    fn two_groups_that_share_a_category_are_refused_for_a_pie_and_allowed_as_bars() {
        let mut book = book();
        let food = category_id(&book, "Groceries");
        let kind = kassensturz_core::GroupKind::Expense;
        let member = kassensturz_core::GroupMember::Category(food);
        let first = book
            .add_category_group("A", kind, &[member])
            .expect("group");
        let second = book
            .add_category_group("B", kind, &[member])
            .expect("group");
        let mut asked = request("spending_by_category", "pie");
        asked.grouping = "groups".into();
        asked.groups = vec![first.to_string(), second.to_string()];
        let view = view_with(&book, &asked);
        assert_eq!(
            text_of(&object_of(&view, "error"), "code"),
            "report.groups-overlap"
        );
        asked.kind = "bar".into();
        let view = view_with(&book, &asked);
        assert!(keys(&view).contains(&"charts".to_owned()));
    }

    #[test]
    fn net_worth_and_cash_flow_read_back_the_way_qml_reads_them() {
        // Net worth over twelve months: nothing until July, then what the
        // checking account holds (1700,00 in August, 3334,46 from September
        // on).
        let view = view_of(&request("net_worth_over_time", "line"));
        let chart = &list_of(&view, "charts")[0];
        assert_eq!(text_of(chart, "dataset"), "net_worth_over_time");
        assert_eq!(number_of(chart, "count"), 12);
        assert!(!flag_of(chart, "round"));
        let series = list_of(chart, "series");
        assert_eq!(series.len(), 1);
        assert_eq!(text_of(&series[0], "key"), "net_worth");
        let worth: Vec<String> = list_of(&series[0], "points")
            .iter()
            .map(|point| text_of(point, "minorUnits"))
            .collect();
        assert_eq!(worth[7..], ["0", "0", "170000", "333446", "333446"][..]);
        assert_eq!(text_of(chart, "total"), "3334,46 €");
        let table = object_of(chart, "table");
        let columns: Vec<String> = list_of(&table, "columns")
            .iter()
            .map(|name| name.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(columns, ["month", "assets", "liabilities", "net_worth"]);
        // The table of the same dataset, as a table asks for it.
        let as_table = view_of(&request("net_worth_over_time", "bar"));
        assert_eq!(list_of(&list_of(&as_table, "charts")[0], "series").len(), 1);

        // Cash flow: money in and out as two series of bars.
        let view = view_of(&request("cash_flow_by_month", "bar"));
        let chart = &list_of(&view, "charts")[0];
        let keys_of: Vec<String> = list_of(chart, "series")
            .iter()
            .map(|series| text_of(series, "key"))
            .collect();
        assert_eq!(keys_of, ["money_in", "money_out"]);
        assert_eq!(text_of(chart, "total"), "3334,46 €");
        let table = object_of(chart, "table");
        let rows = list_of(&table, "rows");
        assert_eq!(rows.len(), 12);
        // September, the second to last row: start 1700,00, in 2500,00, out
        // 865,54, starting balances 0, end 3334,46.
        let cells: Vec<String> = elements(&rows[10])
            .expect("a row")
            .iter()
            .map(|cell| cell.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(
            cells[1..],
            ["1700,00 €", "2500,00 €", "865,54 €", "0,00 €", "3334,46 €"][..]
        );
        // A kind the dataset cannot show is an error the screen can read.
        let view = view_of(&request("cash_flow_by_month", "pie"));
        assert_eq!(
            text_of(&object_of(&view, "error"), "code"),
            "charts.kind-not-supported"
        );
    }

    #[test]
    fn the_balances_are_a_table_by_account_or_by_kind() {
        let mut asked = request("account_balances", "bar");
        asked.period = "today".into();
        let view = view_of(&asked);
        let chart = &list_of(&view, "charts")[0];
        let rows = list_of(&object_of(chart, "table"), "rows");
        let cells = |row: &QVariant| -> Vec<String> {
            elements(row)
                .expect("a row")
                .iter()
                .map(|cell| cell.value::<QString>().expect("text").to_string())
                .collect()
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(cells(&rows[0]), ["Checking account", "have", "3334,46 €"]);
        assert_eq!(text_of(chart, "total"), "3334,46 €");
        asked.grouping = "account-kind".into();
        let view = view_of(&asked);
        let chart = &list_of(&view, "charts")[0];
        let rows = list_of(&object_of(chart, "table"), "rows");
        let kinds: Vec<String> = rows.iter().map(|row| cells(row)[0].clone()).collect();
        assert_eq!(kinds, ["have", "owe", "worth"]);
        // A grouping that is not fixed is a mistake in the screen.
        asked.grouping = "someday".into();
        let view = view_of(&asked);
        assert_eq!(
            text_of(&object_of(&view, "error"), "code"),
            "charts.bad-request"
        );
    }

    #[test]
    fn the_datasets_of_the_balances_ignore_what_only_spending_uses() {
        // Columns decide what a chart shows (D-339), so they win over any
        // dataset; without them the balances are read.
        let mut asked = request("net_worth_over_time", "line");
        asked.show_hidden = true;
        asked.groups = vec!["no-such-group".into()];
        let view = view_of(&asked);
        assert_eq!(number_of(&view, "missing"), 0);
        assert!(!flag_of(&view, "empty"));
    }

    #[test]
    fn every_column_of_the_tables_of_the_accounts_has_a_name_on_screen() {
        // A column the screen has no words for would show its fixed name.
        let table = include_str!("../qml/ChartTable.qml");
        for (dataset, kind, grouping) in [
            ("account_balances", "bar", "each"),
            ("account_balances", "bar", "account-kind"),
            ("net_worth_over_time", "line", "each"),
            ("cash_flow_by_month", "bar", "each"),
        ] {
            let mut asked = request(dataset, kind);
            asked.grouping = grouping.into();
            let view = view_of(&asked);
            let chart = &list_of(&view, "charts")[0];
            for column in list_of(&object_of(chart, "table"), "columns") {
                let name = column.value::<QString>().expect("text").to_string();
                assert!(
                    table.contains(&format!("\"{name}\":")),
                    "{name} of {dataset} needs a title in ChartTable.qml"
                );
            }
        }
    }
}
