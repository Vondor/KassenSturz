// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The transactions tile as a QML object (US4.7, D-716), on the book the
//! app has open (`session.rs`). The work is in `transactions_tile.rs`; this
//! file turns it into the plain values QML reads. Figures are text (D-065).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};

use crate::charts::ChartsError;
use crate::number_format::app_locale;
use crate::scope::{FilterView, Scope};
use crate::session;
use crate::transactions_tile::{self, TileRow, TileView};
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
        /// The transactions tile of the open book. Ask with `show`; `view`
        /// then holds an object with `rows`, `total` (how many fit, those
        /// not shown included), `severalAccounts` and `filter`, or an
        /// `error`.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BookTransactions = super::BookTransactionsRust;
    }

    extern "RustQt" {
        /// Shows the transactions of `period` (a stored period word),
        /// at most `rows` of them (a `params.rows` word), with the scope set
        /// last.
        #[qinvokable]
        fn show(self: Pin<&mut BookTransactions>, period: &QString, rows: &QString);

        /// The dashboard's filters for this tile, as the layout gives them
        /// (`Scope::to_text`); applies from the next `show`.
        #[qinvokable]
        #[cxx_name = "setScope"]
        fn set_scope(self: Pin<&mut BookTransactions>, scope: &QString);

        /// Asks again with the last request, after the book changed.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookTransactions>);
    }
}

/// The Rust side of [`qobject::BookTransactions`].
#[derive(Default)]
pub struct BookTransactionsRust {
    view: QVariant,
    request: Option<(String, String)>,
    scope: Scope,
}

impl qobject::BookTransactions {
    fn show(mut self: Pin<&mut Self>, period: &QString, rows: &QString) {
        self.as_mut().rust_mut().request = Some((period.to_string(), rows.to_string()));
        self.refresh();
    }

    fn set_scope(mut self: Pin<&mut Self>, scope: &QString) {
        self.as_mut().rust_mut().scope = Scope::from_text(&scope.to_string());
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some((period, rows)) = self.rust().request.clone() else {
            return;
        };
        let scope = self.rust().scope.clone();
        let locale = app_locale();
        let view = match session::with_book(|book| {
            transactions_tile::tile(book, &locale, &period, &rows, &scope)
        }) {
            Some(result) => view_object(&result),
            None => QVariant::default(),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }
}

fn count(value: usize) -> QVariant {
    QVariant::from(&i32::try_from(value).unwrap_or(i32::MAX))
}

fn row_object(row: &TileRow) -> QVariant {
    map([
        ("id", text(&row.id)),
        ("accountId", text(&row.account_id)),
        ("account", text(&row.account)),
        ("date", text(&row.date)),
        ("otherParty", text(&row.other_party)),
        ("kind", text(row.kind)),
        ("other", text(&row.other)),
        ("amount", text(&row.amount.text)),
        ("exact", text(&row.amount.exact)),
        ("incoming", QVariant::from(&row.incoming)),
    ])
}

fn view_object(result: &Result<(TileView, FilterView), ChartsError>) -> QVariant {
    match result {
        Ok((view, filter)) => map([
            ("filter", filter.object()),
            ("rows", list(view.rows.iter().map(row_object))),
            ("total", count(view.total)),
            ("severalAccounts", QVariant::from(&view.several_accounts)),
        ]),
        Err(error) => {
            let report = error.report();
            if report.is_none() {
                eprintln!("kassensturz: the transactions tile was asked for with a wrong name");
            }
            map([(
                "error",
                map([(
                    "code",
                    text(report.map_or("charts.bad-request", |r| r.code)),
                )]),
            )])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::tests::TestLocale;
    use crate::transactions_tile::tests::fixture;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, object_of, text_of};

    #[test]
    fn the_view_reads_back_the_way_qml_reads_it() {
        let f = fixture();
        let view = view_object(&transactions_tile::tile(
            &f.book,
            &TestLocale,
            "this-month",
            "10",
            &Scope::default(),
        ));
        assert_eq!(keys(&view), ["filter", "rows", "severalAccounts", "total"]);
        assert_eq!(number_of(&view, "total"), 2);
        assert!(flag_of(&view, "severalAccounts"));
        let rows = list_of(&view, "rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            keys(&rows[1]),
            [
                "account",
                "accountId",
                "amount",
                "date",
                "exact",
                "id",
                "incoming",
                "kind",
                "other",
                "otherParty"
            ]
        );
        assert_eq!(text_of(&rows[1], "kind"), "simple");
        assert_eq!(text_of(&rows[1], "other"), "Groceries");
        assert_eq!(text_of(&rows[1], "account"), "Checking");
        assert_eq!(text_of(&rows[1], "accountId"), f.checking.to_string());
        assert!(!flag_of(&rows[1], "incoming"));
        assert!(!text_of(&rows[1], "amount").is_empty());
        assert!(!text_of(&rows[1], "date").is_empty());
        assert_eq!(text_of(&rows[0], "kind"), "transfer");
        let filter = object_of(&view, "filter");
        assert!(keys(&filter).contains(&"notes".to_owned()));
    }

    #[test]
    fn a_wrong_period_is_an_error_object() {
        let f = fixture();
        let view = view_object(&transactions_tile::tile(
            &f.book,
            &TestLocale,
            "someday",
            "10",
            &Scope::default(),
        ));
        assert_eq!(keys(&view), ["error"]);
        assert_eq!(
            text_of(&object_of(&view, "error"), "code"),
            "charts.bad-request"
        );
    }
}
