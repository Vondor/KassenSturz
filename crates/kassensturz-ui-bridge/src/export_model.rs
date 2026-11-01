// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind the export screens (M3.12): a copy of the
//! transactions as CSV, or of the whole book; and a dashboard file (M6d.3). The work is in `export.rs`;
//! this file only moves text between it and QML (D-065).

use core::pin::Pin;

use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QUrl, QVariant};

use crate::book_session::{app_version, local_path};
use crate::errors::ErrorText;
use crate::export::{self, CsvForm, Saved};
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::register_model::{field, flag};
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// Saving a copy of the open book's data in a new file.
        #[qobject]
        #[qml_element]
        type DataExport = super::DataExportRust;
    }

    extern "RustQt" {
        /// The accounts to choose from, closed ones too: a list of objects
        /// with `id`, `name`, `currency` and `closed`.
        #[qinvokable]
        fn accounts(self: &DataExport) -> QVariant;

        /// The usual choices for the CSV file in the user's region: an
        /// object with `separator`, `decimalMark`, `header` and
        /// `byteOrderMark`.
        #[qinvokable]
        #[cxx_name = "csvChoices"]
        fn csv_choices(self: &DataExport) -> QVariant;

        /// A name for a new file: the book's name and today's date, with
        /// `extension` (`csv` or `json`).
        #[qinvokable]
        #[cxx_name = "suggestedName"]
        fn suggested_name(self: &DataExport, extension: &QString) -> QString;

        /// Writes the transactions of the accounts `ids` names (a list of
        /// IDs) to `file` as CSV with the `choices`. An error object, or an
        /// object with `fileName`, `transactions` and `accounts`.
        #[qinvokable]
        #[cxx_name = "saveCsv"]
        fn save_csv(self: &DataExport, file: &QUrl, ids: &QVariant, choices: &QVariant)
        -> QVariant;

        /// Writes the whole book to `file`. An error object, or an object
        /// with `fileName`, `transactions` and `accounts`.
        #[qinvokable]
        #[cxx_name = "saveBook"]
        fn save_book(self: &DataExport, file: &QUrl) -> QVariant;

        /// The file name suggested for the dashboard `id` names (the book's
        /// ID as text; empty for the built-in Overview): its name with
        /// `.dashboard.json`. Empty if there is no such dashboard.
        #[qinvokable]
        #[cxx_name = "dashboardFileName"]
        fn dashboard_file_name(self: &DataExport, id: &QString) -> QString;

        /// Writes the dashboard `id` names (empty: the built-in Overview)
        /// to `file` as a dashboard file, replacing a file there (the
        /// chooser asked already, D-443). An error object, or an object with
        /// `fileName`, `references` and `leftOut` (how many accounts,
        /// categories and groups the book no longer has, D-458).
        #[qinvokable]
        #[cxx_name = "saveDashboard"]
        fn save_dashboard(self: &DataExport, file: &QUrl, id: &QString) -> QVariant;
    }

    impl cxx_qt::Initialize for DataExport {}
}

/// The Rust side of [`qobject::DataExport`].
#[derive(Default)]
pub struct DataExportRust;

impl cxx_qt::Initialize for qobject::DataExport {
    fn initialize(self: Pin<&mut Self>) {}
}

impl qobject::DataExport {
    fn accounts(&self) -> QVariant {
        let found = session::with_book(export::account_choices);
        match found {
            Some(Ok(choices)) => list(choices.iter().map(|choice| {
                map([
                    ("id", text(&choice.id)),
                    ("name", text(&choice.name)),
                    ("currency", text(&choice.currency)),
                    ("closed", QVariant::from(&choice.closed)),
                ])
            })),
            _ => list([]),
        }
    }

    fn csv_choices(&self) -> QVariant {
        let usual = CsvForm::usual(&app_locale());
        map([
            ("separator", text(&usual.separator)),
            ("decimalMark", text(&usual.decimal_mark)),
            ("header", QVariant::from(&usual.header)),
            ("byteOrderMark", QVariant::from(&usual.byte_order_mark)),
        ])
    }

    fn suggested_name(&self, extension: &QString) -> QString {
        let stem = session::path()
            .and_then(|path| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        let today = app_locale().today();
        QString::from(format!("{stem} {today}.{extension}").trim_start())
    }

    fn save_csv(&self, file: &QUrl, ids: &QVariant, choices: &QVariant) -> QVariant {
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let ids: Vec<String> = ids
            .value::<QList<QVariant>>()
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.value::<QString>())
                    .map(|id| id.to_string())
                    .collect()
            })
            .unwrap_or_default();
        let choices = choices
            .value::<QMap<QMapPair_QString_QVariant>>()
            .unwrap_or_default();
        let form = CsvForm {
            separator: field(&choices, "separator"),
            decimal_mark: field(&choices, "decimalMark"),
            header: flag(&choices, "header"),
            byte_order_mark: flag(&choices, "byteOrderMark"),
        };
        self.saved(|book| export::save_csv(book, &ids, &form, &path))
    }

    fn save_book(&self, file: &QUrl) -> QVariant {
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        self.saved(|book| export::save_book(book, &path))
    }

    fn dashboard_file_name(&self, id: &QString) -> QString {
        let id = id.to_string();
        session::with_book(|book| export::dashboard_file_suggestion(book, &id).ok())
            .flatten()
            .map(|name| QString::from(&name))
            .unwrap_or_default()
    }

    fn save_dashboard(&self, file: &QUrl, id: &QString) -> QVariant {
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let id = id.to_string();
        let application = format!("KassenSturz {}", app_version());
        let done = session::with_book(|book| {
            export::save_dashboard(book, &id, &application, &path)
                .map_err(|error| export::error_text(book, &error, &app_locale()))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match done {
            Ok(saved) => saved_dashboard_object(&saved),
            Err(error) => error_object(&error),
        }
    }

    /// Runs `save` on the open book and turns what it did into what QML gets.
    fn saved(
        &self,
        save: impl FnOnce(&kassensturz_store::Book) -> Result<Saved, export::SaveError>,
    ) -> QVariant {
        let done = session::with_book(|book| {
            save(book).map_err(|error| export::error_text(book, &error, &app_locale()))
        })
        .unwrap_or_else(|| Err(ErrorText::plain("input.no-such-entry")));
        match done {
            Ok(saved) => map([
                ("fileName", text(&saved.file_name)),
                (
                    "transactions",
                    QVariant::from(&i32::try_from(saved.transactions).unwrap_or(i32::MAX)),
                ),
                (
                    "accounts",
                    QVariant::from(&i32::try_from(saved.accounts).unwrap_or(i32::MAX)),
                ),
            ]),
            Err(error) => error_object(&error),
        }
    }
}

/// What QML gets after a dashboard file was saved.
fn saved_dashboard_object(saved: &export::SavedDashboard) -> QVariant {
    let count = |number: usize| QVariant::from(&i32::try_from(number).unwrap_or(i32::MAX));
    map([
        ("fileName", text(&saved.file_name)),
        ("references", count(saved.references)),
        ("leftOut", count(saved.left_out)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::read_back::{keys, number_of, text_of};

    /// The object reads back the way QML reads it (AGENTS.md §9.4).
    #[test]
    fn the_saved_dashboard_object_has_the_fields_qml_reads() {
        let object = saved_dashboard_object(&export::SavedDashboard {
            file_name: "Overview.dashboard.json".into(),
            references: 3,
            left_out: 1,
        });
        assert_eq!(keys(&object), ["fileName", "leftOut", "references"]);
        assert_eq!(text_of(&object, "fileName"), "Overview.dashboard.json");
        assert_eq!(number_of(&object, "references"), 3);
        assert_eq!(number_of(&object, "leftOut"), 1);
    }
}
