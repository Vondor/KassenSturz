// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adding a dashboard from a file as a QML object (M6d.4, D-441, D-442),
//! on the book the app has open (`session.rs`). The work is in
//! `dashboard_file.rs`; this file turns it into plain values for
//! `DashboardFilePage.qml`.
//!
//! `summary` holds what the page shows before anything is added: `name`
//! (free in this book), `fileName`, `widgets` (how many), `found` (an object
//! with lists of text `accounts`, `categories` and `groups`), `missing`
//! (objects with `name`, `kind`, `several`, `filterBar` and `widgets`),
//! `showsMore` (objects with `widget`, empty for the filter bar, `list` and
//! `named`), `formulaNames` (objects with `widget` and `name`) and `unknown`
//! (widgets). A widget is an object with `title` and `kind`. Every name is
//! the file's or the book's text, shown as plain text (T-08, T-21).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl, QVariant};
use kassensturz_report::{Reference, Resolved, Widget};

use crate::book_session::local_path;
use crate::dashboard_file::{self, DashboardFileError, Opened};
use crate::errors::{ErrorText, report_text};
use crate::number_format::app_locale;
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        /// A dashboard file read into the open book, before it is added.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, summary, READ, NOTIFY)]
        type DashboardFileImport = super::DashboardFileImportRust;
    }

    extern "RustQt" {
        /// Reads the file and looks its names up in the open book. Returns
        /// an empty object and fills `summary`, or the error; nothing in the
        /// book changes.
        #[qinvokable]
        fn open(self: Pin<&mut DashboardFileImport>, file: &QUrl) -> QVariant;

        /// Adds the dashboard read last under `name`, as one entry of the
        /// change history. Returns its `id`, or the error.
        #[qinvokable]
        fn add(self: Pin<&mut DashboardFileImport>, name: &QString) -> QVariant;

        /// Forgets the file read last.
        #[qinvokable]
        fn cancel(self: Pin<&mut DashboardFileImport>);
    }

    extern "RustQt" {
        /// The book changed through this object: the other screens read it
        /// again.
        #[qsignal]
        fn changed(self: Pin<&mut DashboardFileImport>);
    }
}

/// The Rust side of [`qobject::DashboardFileImport`].
#[derive(Default)]
pub struct DashboardFileImportRust {
    summary: QVariant,
    opened: Option<Opened>,
}

/// What a reference names, as one line of text: a category's path with
/// `›` between the names.
fn reference_text(reference: &Reference) -> String {
    match reference {
        Reference::Category(path) => path.join(" › "),
        Reference::Group(name) | Reference::Account(name) => name.clone(),
    }
}

fn reference_kind(reference: &Reference) -> &'static str {
    match reference {
        Reference::Category(_) => "category",
        Reference::Group(_) => "group",
        Reference::Account(_) => "account",
    }
}

fn widget_object(widget: &Widget) -> QVariant {
    map([("title", text(&widget.title)), ("kind", text(&widget.kind))])
}

fn count(number: usize) -> QVariant {
    QVariant::from(&i32::try_from(number).unwrap_or(i32::MAX))
}

/// The summary page's object; see the module's documentation.
pub(crate) fn summary_object(resolved: &Resolved, name: &str, file_name: &str) -> QVariant {
    let found = |kind: &str| {
        list(
            resolved
                .found
                .iter()
                .filter(|reference| reference_kind(reference) == kind)
                .map(|reference| text(&reference_text(reference))),
        )
    };
    map([
        ("name", text(name)),
        ("fileName", text(file_name)),
        ("widgets", count(resolved.document.entries.len())),
        (
            "found",
            map([
                ("accounts", found("account")),
                ("categories", found("category")),
                ("groups", found("group")),
            ]),
        ),
        (
            "missing",
            list(resolved.missing.iter().map(|missing| {
                map([
                    ("name", text(&reference_text(&missing.reference))),
                    ("kind", text(reference_kind(&missing.reference))),
                    ("several", QVariant::from(&missing.several)),
                    ("filterBar", QVariant::from(&missing.filter_bar)),
                    ("widgets", list(missing.widgets.iter().map(widget_object))),
                ])
            })),
        ),
        (
            "showsMore",
            list(resolved.shows_more.iter().map(|more| {
                map([
                    (
                        "widget",
                        more.widget
                            .as_ref()
                            .map_or_else(QVariant::default, widget_object),
                    ),
                    ("list", text(more.list)),
                    ("named", count(more.named)),
                ])
            })),
        ),
        (
            "formulaNames",
            list(resolved.formula_names.iter().map(|note| {
                map([
                    ("widget", widget_object(&note.widget)),
                    ("name", text(&note.name)),
                ])
            })),
        ),
        ("unknown", list(resolved.unknown.iter().map(widget_object))),
    ])
}

/// The error as QML gets it.
fn error_text(error: &DashboardFileError) -> ErrorText {
    let locale = app_locale();
    session::with_book(|book| report_text(book, &error.report(), &locale))
        .unwrap_or_else(|| ErrorText::plain("input.no-such-entry"))
}

impl qobject::DashboardFileImport {
    fn set_summary(mut self: Pin<&mut Self>, summary: QVariant) {
        self.as_mut().rust_mut().summary = summary;
        self.as_mut().summary_changed();
    }

    fn open(mut self: Pin<&mut Self>, file: &QUrl) -> QVariant {
        self.as_mut().cancel();
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let opened = match dashboard_file::read_bytes(&path) {
            Ok(bytes) => session::with_book(|book| dashboard_file::open(book, &bytes)),
            Err(error) => Some(Err(error)),
        };
        match opened {
            Some(Ok(opened)) => {
                let summary = summary_object(&opened.resolved, &opened.name, &file_name);
                self.as_mut().rust_mut().opened = Some(opened);
                self.set_summary(summary);
                map([])
            }
            Some(Err(error)) => error_object(&error_text(&error)),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn add(mut self: Pin<&mut Self>, name: &QString) -> QVariant {
        let Some(opened) = self.rust().opened.clone() else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let name = name.to_string();
        let added = session::with_book_mut(|book| {
            dashboard_file::add(book, &opened.resolved.document, &name)
        });
        match added {
            Some(Ok(id)) => {
                self.as_mut().cancel();
                self.as_mut().changed();
                map([("id", text(&id))])
            }
            Some(Err(error)) => error_object(&error_text(&error)),
            None => error_object(&ErrorText::plain("input.no-such-entry")),
        }
    }

    fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().opened = None;
        self.set_summary(QVariant::default());
    }
}

#[cfg(test)]
mod tests {
    use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant};
    use kassensturz_report::{FormulaName, IncomingDashboardFile, MissingName, ShowsMore, Widget};

    use super::*;

    fn object(value: &QVariant) -> QMap<QMapPair_QString_QVariant> {
        value
            .value::<QMap<QMapPair_QString_QVariant>>()
            .expect("an object")
    }

    fn get(value: &QVariant, key: &str) -> QVariant {
        object(value)
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
        let list = get(value, key)
            .value::<QList<QVariant>>()
            .unwrap_or_else(|| panic!("`{key}` is not a list"));
        list.iter().cloned().collect()
    }

    fn texts(value: &QVariant, key: &str) -> Vec<String> {
        items(value, key)
            .iter()
            .map(|item| item.value::<QString>().expect("text").to_string())
            .collect()
    }

    /// A summary with one of everything, read back the way the page reads
    /// it.
    #[test]
    fn the_summary_reads_back_as_the_page_reads_it() {
        let document = IncomingDashboardFile::read(
            &serde_json::json!({
                "format": "kassensturz-dashboard-file",
                "version": 1,
                "dashboard": {
                    // The current version: an older one would get the
                    // period header and the filter bar (D-476).
                    "format": "kassensturz-dashboard", "version": kassensturz_report::VERSION,
                    "kind": "dashboard", "name": "Spending", "components": [],
                },
            })
            .to_string(),
        )
        .expect("read")
        .resolve(&kassensturz_report::BookNames::default())
        .expect("resolved")
        .document;
        let widget = Widget {
            id: "c1".into(),
            kind: "chart.donut".into(),
            title: "Pets".into(),
        };
        let resolved = Resolved {
            document,
            found: vec![
                Reference::Category(vec!["Everyday".into(), "Pets".into()]),
                Reference::Group("Fixed costs".into()),
                Reference::Account("Colony Checking".into()),
            ],
            missing: vec![MissingName {
                reference: Reference::Account("Old savings".into()),
                several: true,
                filter_bar: true,
                widgets: vec![widget.clone()],
            }],
            shows_more: vec![
                ShowsMore {
                    widget: None,
                    list: "accounts",
                    named: 2,
                },
                ShowsMore {
                    widget: Some(widget.clone()),
                    list: "series",
                    named: 1,
                },
            ],
            formula_names: vec![FormulaName {
                widget: widget.clone(),
                name: "Rockets".into(),
            }],
            unknown: vec![widget],
        };
        let summary = summary_object(&resolved, "Spending (2)", "spending.dashboard.json");
        assert_eq!(string(&summary, "name"), "Spending (2)");
        assert_eq!(string(&summary, "fileName"), "spending.dashboard.json");
        assert_eq!(get(&summary, "widgets").value::<i32>(), Some(0));
        let found = get(&summary, "found");
        assert_eq!(texts(&found, "categories"), ["Everyday › Pets"]);
        assert_eq!(texts(&found, "groups"), ["Fixed costs"]);
        assert_eq!(texts(&found, "accounts"), ["Colony Checking"]);
        let [missing] = items(&summary, "missing").try_into().expect("one");
        assert_eq!(string(&missing, "name"), "Old savings");
        assert_eq!(string(&missing, "kind"), "account");
        assert_eq!(get(&missing, "several").value::<bool>(), Some(true));
        assert_eq!(get(&missing, "filterBar").value::<bool>(), Some(true));
        let [used_by] = items(&missing, "widgets").try_into().expect("one");
        assert_eq!(string(&used_by, "title"), "Pets");
        assert_eq!(string(&used_by, "kind"), "chart.donut");
        let [bar, chart] = items(&summary, "showsMore").try_into().expect("two");
        assert_eq!(string(&bar, "list"), "accounts");
        assert_eq!(get(&bar, "named").value::<i32>(), Some(2));
        // The filter bar has no widget: QML reads an empty value.
        assert!(
            get(&bar, "widget")
                .value::<QMap<QMapPair_QString_QVariant>>()
                .is_none()
        );
        assert_eq!(string(&get(&chart, "widget"), "title"), "Pets");
        let [formula] = items(&summary, "formulaNames").try_into().expect("one");
        assert_eq!(string(&formula, "name"), "Rockets");
        assert_eq!(string(&get(&formula, "widget"), "kind"), "chart.donut");
        let [unknown] = items(&summary, "unknown").try_into().expect("one");
        assert_eq!(string(&unknown, "title"), "Pets");
    }
}
