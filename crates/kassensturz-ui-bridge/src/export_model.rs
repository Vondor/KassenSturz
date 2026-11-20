// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind the export screens (M3.12): a copy of the
//! transactions as CSV, or of the whole book; and a dashboard file (M6d.3). The work is in `export.rs`;
//! this file only moves text between it and QML (D-065).

use core::pin::Pin;

use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QUrl, QVariant};

use kassensturz_export::Exporter;

use crate::book_session::{app_version, local_path};
use crate::errors::ErrorText;
use crate::export::{self, CsvForm, Saved};
use crate::format::Locale;
use crate::number_format::app_locale;
use crate::register_model::{field, flag};
use crate::session;
use crate::variant::{error_object, list, map, text};
use crate::{ai_switch_model, plugin_formats};

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

        /// The plugin exporters the user added (#791, D-852): a list of
        /// objects with `file` (which names the plugin), `name` (plain text)
        /// and `extension`.
        #[qinvokable]
        #[cxx_name = "pluginExporters"]
        fn plugin_exporters(self: &DataExport) -> QVariant;

        /// Writes the transactions of the accounts `ids` names to `file`
        /// with the plugin exporter `plugin` names (its `file`), for the
        /// days of the `choices`. Answers as `saveCsv` does.
        #[qinvokable]
        #[cxx_name = "saveWithPlugin"]
        fn save_with_plugin(
            self: &DataExport,
            file: &QUrl,
            plugin: &QString,
            ids: &QVariant,
            choices: &QVariant,
        ) -> QVariant;

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
        let locale = app_locale();
        csv_choices_object(&CsvForm::usual(&locale), &locale.short_date(locale.today()))
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
        let (ids, form) = ids_and_form(ids, choices);
        self.saved(|book| export::save_csv(book, &ids, &form, &app_locale(), &path))
    }

    fn plugin_exporters(&self) -> QVariant {
        plugin_exporters_object(&plugin_formats::exporters(ai_switch_model::ai_on()))
    }

    fn save_with_plugin(
        &self,
        file: &QUrl,
        plugin: &QString,
        ids: &QVariant,
        choices: &QVariant,
    ) -> QVariant {
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let Some(choice) = plugin_formats::exporter(&plugin.to_string(), ai_switch_model::ai_on())
        else {
            return error_object(&ErrorText::plain("input.plugin-gone"));
        };
        let (ids, form) = ids_and_form(ids, choices);
        self.saved(|book| {
            export::save_with_plugin(book, &ids, &form, &app_locale(), &path, &choice.exporter)
        })
    }
}

/// The accounts' IDs and the form of what QML hands over.
fn ids_and_form(ids: &QVariant, choices: &QVariant) -> (Vec<String>, CsvForm) {
    {
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
            from: field(&choices, "from"),
            until: field(&choices, "until"),
        };
        (ids, form)
    }
}

/// `pluginExporters()` as QML reads it.
fn plugin_exporters_object(choices: &[plugin_formats::ExporterChoice]) -> QVariant {
    list(choices.iter().map(|choice| {
        map([
            ("file", text(&choice.file)),
            ("name", text(&choice.name)),
            ("extension", text(choice.exporter.info().extension)),
        ])
    }))
}

impl qobject::DataExport {
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

/// What QML keeps of the choices for the CSV file. `date_example` is a day
/// written the way the user's region writes it, for the hint under the first
/// and last day (#901).
fn csv_choices_object(usual: &CsvForm, date_example: &str) -> QVariant {
    map([
        ("separator", text(&usual.separator)),
        ("decimalMark", text(&usual.decimal_mark)),
        ("header", QVariant::from(&usual.header)),
        ("byteOrderMark", QVariant::from(&usual.byte_order_mark)),
        ("from", text(&usual.from)),
        ("until", text(&usual.until)),
        ("dateExample", text(date_example)),
    ])
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

    /// The choices read back the way QML reads them (AGENTS.md §9.4): the
    /// first and last day start empty, which means all dates (#901).
    #[test]
    fn the_csv_choices_object_has_the_fields_qml_reads() {
        let usual = CsvForm::usual(&crate::register::tests::TestLocale);
        let object = csv_choices_object(&usual, "2026-10-19");
        assert_eq!(
            keys(&object),
            [
                "byteOrderMark",
                "dateExample",
                "decimalMark",
                "from",
                "header",
                "separator",
                "until"
            ]
        );
        assert_eq!(text_of(&object, "from"), "");
        assert_eq!(text_of(&object, "until"), "");
        assert_eq!(text_of(&object, "dateExample"), "2026-10-19");
        assert_eq!(text_of(&object, "separator"), "semicolon");
    }

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

    /// #791: the plugin exporters read back as QML reads them.
    #[test]
    fn the_plugin_exporters_read_back_as_qml_reads_them() {
        let folder = crate::test_folder::TestFolder::new();
        std::fs::write(
            folder.join("echo.wasm"),
            crate::test_plugins::echo_exporter(),
        )
        .expect("plugin written");
        let object = plugin_exporters_object(&plugin_formats::exporters_in(folder.path(), false));
        let items = crate::variant::read_back::elements(&object).expect("a list");
        assert_eq!(items.len(), 1);
        assert_eq!(keys(&items[0]), ["extension", "file", "name"]);
        assert_eq!(text_of(&items[0], "file"), "echo.wasm");
        assert_eq!(text_of(&items[0], "name"), "Example echo export");
        assert_eq!(text_of(&items[0], "extension"), "json");
    }
}
