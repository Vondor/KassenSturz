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
use kassensturz_charts::{ChartModel, HiddenNote, PALETTE_SIZE, Prepared, Series};
use kassensturz_core::ErrorParam;

use crate::charts::{self, ChartSet, ChartsError, Request};
use crate::number_format::app_locale;
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
        /// anyway (`props.show_hidden`).
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
        // Each dataset reads only the list it uses.
        let ids: Vec<String> = chosen
            .to_string()
            .split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();
        self.as_mut().rust_mut().request = Some(Request {
            dataset: dataset.to_string(),
            kind: kind.to_string(),
            period: period.to_string(),
            grouping: grouping.to_string(),
            groups: ids.clone(),
            series: ids,
            show_hidden,
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
        assert_eq!(keys(&view), ["charts", "empty", "missing"]);
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
}
