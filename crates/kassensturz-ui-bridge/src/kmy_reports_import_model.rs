// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! *Add reports from a KMyMoney file…* as a QML object (M6d.6b, D-450), on
//! the book the app has open (`session.rs`). The work is in
//! `kmy_reports_import.rs`; this file turns it into plain values for
//! `KmyReportsImportPage.qml`.
//!
//! `preview` holds what the page shows before anything is written:
//! `fileName`, `reports` (one object per report of the file, in its order:
//! the fields of a line of `KmyReportsSummary.qml`, plus `state`,
//! `canChoose` and `chosen`), `noLongerInFile` (names of views whose report
//! the file no longer has), and how many would be `added` and `replaced`
//! with the current choices. Every name is the file's or the book's text,
//! shown as plain text.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QUrl, QVariant};

use crate::book_from_file::{FromFileError, error_text_in};
use crate::book_from_file_model::report_object;
use crate::book_session::local_path;
use crate::errors::ErrorText;
use crate::kmy_reports_import::{self, Opened, PreviewLine};
use crate::number_format::app_locale;
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        /// A KMyMoney file's reports planned for the open book, before
        /// anything is written.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, preview, READ, NOTIFY)]
        type KmyReportsImport = super::KmyReportsImportRust;
    }

    extern "RustQt" {
        /// Reads the file and plans its reports for the open book. Returns
        /// an empty object and fills `preview`, or the error; nothing in the
        /// book changes.
        #[qinvokable]
        fn open(self: Pin<&mut KmyReportsImport>, file: &QUrl) -> QVariant;

        /// Ticks or unticks the report at `index`, if it has a choice.
        #[qinvokable]
        fn choose(self: Pin<&mut KmyReportsImport>, index: i32, chosen: bool);

        /// Writes what is ticked, as one entry of the change history.
        /// Returns `added` and `replaced`, or the error.
        #[qinvokable]
        fn add(self: Pin<&mut KmyReportsImport>) -> QVariant;

        /// Forgets the file read last.
        #[qinvokable]
        fn cancel(self: Pin<&mut KmyReportsImport>);
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut KmyReportsImport>);
    }
}

/// The Rust side of [`qobject::KmyReportsImport`].
#[derive(Default)]
pub struct KmyReportsImportRust {
    preview: QVariant,
    opened: Option<Opened>,
}

fn count(number: usize) -> QVariant {
    QVariant::from(&i32::try_from(number).unwrap_or(i32::MAX))
}

/// One line of the preview; see the module's documentation.
pub(crate) fn line_object(line: &PreviewLine) -> QVariant {
    report_object(
        &line.text,
        [
            ("state", text(line.state)),
            ("canChoose", QVariant::from(&line.can_choose)),
            ("chosen", QVariant::from(&line.chosen)),
        ],
    )
}

/// The page's object; see the module's documentation.
pub(crate) fn preview_object(file_name: &str, lines: &[PreviewLine], gone: &[String]) -> QVariant {
    let ticked = |state: &str| {
        lines
            .iter()
            .filter(|line| line.chosen && line.can_choose && line.state == state)
            .count()
    };
    map([
        ("fileName", text(file_name)),
        ("reports", list(lines.iter().map(line_object))),
        ("noLongerInFile", list(gone.iter().map(|name| text(name)))),
        ("added", count(ticked("new") + ticked("deleted-earlier"))),
        ("replaced", count(ticked("kept-yours"))),
    ])
}

/// The error as QML gets it.
fn error_text(error: &FromFileError) -> ErrorText {
    let locale = app_locale();
    session::with_book(|book| error_text_in(Some(book), error, &locale))
        .unwrap_or_else(|| ErrorText::plain("input.no-such-entry"))
}

impl qobject::KmyReportsImport {
    fn show(mut self: Pin<&mut Self>) {
        let preview = self
            .rust()
            .opened
            .as_ref()
            .map_or_else(QVariant::default, |opened| {
                preview_object(
                    &opened.file_name,
                    &opened.lines(&app_locale()),
                    &opened.plan.no_longer_in_file,
                )
            });
        self.as_mut().rust_mut().preview = preview;
        self.as_mut().preview_changed();
    }

    fn open(mut self: Pin<&mut Self>, file: &QUrl) -> QVariant {
        self.as_mut().cancel();
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let locale = app_locale();
        match session::with_book(|book| kmy_reports_import::open(book, &path, &locale)) {
            Some(Ok(opened)) => {
                self.as_mut().rust_mut().opened = Some(opened);
                self.show();
                map([])
            }
            Some(Err(error)) => error_object(&error_text(&error)),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn choose(mut self: Pin<&mut Self>, index: i32, chosen: bool) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if let Some(opened) = self.as_mut().rust_mut().opened.as_mut() {
            opened.choose(index, chosen);
        }
        self.show();
    }

    fn add(mut self: Pin<&mut Self>) -> QVariant {
        let Some(opened) = self.rust().opened.clone() else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        match session::with_book_mut(|book| opened.add(book)) {
            Some(Ok(added)) => {
                self.as_mut().cancel();
                self.as_mut().changed();
                map([
                    ("added", count(added.added)),
                    ("replaced", count(added.replaced)),
                ])
            }
            Some(Err(error)) => error_object(&error_text(&error)),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().opened = None;
        self.show();
    }
}

#[cfg(test)]
mod tests {
    use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString};

    use super::*;
    use crate::book_from_file::ReportText;

    fn get(value: &QVariant, key: &str) -> QVariant {
        value
            .value::<QMap<QMapPair_QString_QVariant>>()
            .expect("an object")
            .get(&QString::from(key))
            .unwrap_or_else(|| panic!("no `{key}`"))
    }

    fn string(value: &QVariant, key: &str) -> String {
        get(value, key)
            .value::<QString>()
            .unwrap_or_else(|| panic!("`{key}` is not text"))
            .to_string()
    }

    fn items(value: &QVariant, key: &str) -> Vec<QVariant> {
        get(value, key)
            .value::<QList<QVariant>>()
            .unwrap_or_else(|| panic!("`{key}` is not a list"))
            .iter()
            .cloned()
            .collect()
    }

    /// The preview read back the way the page reads it.
    #[test]
    fn the_preview_reads_back_as_the_page_reads_it() {
        let line = |name: &str, state, can_choose, chosen| PreviewLine {
            text: ReportText {
                name: name.into(),
                outcome: "changed",
                reasons: vec!["range-fixed", "account-not-found"],
                filters: Vec::new(),
                period: "last30days",
                first: "2026-10-02".into(),
                last: "2026-11-01".into(),
                imported: "2026-11-01".into(),
            },
            state,
            can_choose,
            chosen,
        };
        let lines = [
            line("Fuel", "new", true, true),
            line("Rockets", "kept-yours", true, false),
            line("Report 3", "already-here", false, false),
        ];
        let preview = preview_object("colony.kmy", &lines, &["Old view".to_owned()]);
        assert_eq!(string(&preview, "fileName"), "colony.kmy");
        assert_eq!(get(&preview, "added").value::<i32>(), Some(1));
        assert_eq!(get(&preview, "replaced").value::<i32>(), Some(0));
        let gone: Vec<String> = items(&preview, "noLongerInFile")
            .iter()
            .map(|name| name.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(gone, ["Old view"]);
        let [first, second, _] = items(&preview, "reports").try_into().expect("three");
        assert_eq!(string(&first, "name"), "Fuel");
        assert_eq!(string(&first, "state"), "new");
        assert_eq!(get(&first, "canChoose").value::<bool>(), Some(true));
        assert_eq!(get(&first, "chosen").value::<bool>(), Some(true));
        assert_eq!(string(&first, "outcome"), "changed");
        assert_eq!(string(&first, "period"), "last30days");
        assert_eq!(string(&first, "first"), "2026-10-02");
        assert_eq!(string(&first, "last"), "2026-11-01");
        assert_eq!(string(&first, "imported"), "2026-11-01");
        let reasons: Vec<String> = items(&first, "reasons")
            .iter()
            .map(|word| word.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(reasons, ["range-fixed", "account-not-found"]);
        assert!(items(&first, "filters").is_empty());
        assert_eq!(string(&second, "state"), "kept-yours");
        assert_eq!(get(&second, "chosen").value::<bool>(), Some(false));
    }
}
