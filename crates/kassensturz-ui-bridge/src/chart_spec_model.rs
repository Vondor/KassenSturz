// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The check of a custom chart's description as a QML object (M6e.3, D-505).
//! The properties panel asks it about the text in its field after a short
//! pause in typing, and about a file the user chose. The work is in
//! `chart_spec.rs`; this file turns it into plain values.
//!
//! `view` holds `ok` (the description can be kept), `dataset` (the one it
//! names, or empty), `canDraw`, `count` and `unsupported` (objects with
//! `path` and `reason`, the parts that are not supported here), `text` and
//! `opened` (a number that goes up with every file read, so a file with the
//! same text as the last still changes the field) and, when something is
//! wrong, `error` (an object with `code` and the parameters of the code by
//! name). Every path is the user's own text and is shown as plain text.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl, QVariant};
use kassensturz_core::{ErrorParam, Report};
use kassensturz_report::ReportError;

use crate::book_session::local_path;
use crate::chart_spec::{self, Check};
use crate::variant::{list, map, text};

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
        /// What the check of a chart description found. Empty until `check`
        /// or `openFile` has been called.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type ChartDescription = super::ChartDescriptionRust;
    }

    extern "RustQt" {
        /// Checks `text`, a pasted or typed description, the way it is
        /// checked when it is saved. Nothing is kept.
        #[qinvokable]
        fn check(self: Pin<&mut ChartDescription>, text: &QString);

        /// Reads the file (a local file, at most 32 KiB, UTF-8) and checks
        /// its text. `view.text` holds what was read; a file that can't be
        /// used gives an `error` and no text.
        #[qinvokable]
        #[cxx_name = "openFile"]
        fn open_file(self: Pin<&mut ChartDescription>, file: &QUrl);
    }
}

/// The Rust side of [`qobject::ChartDescription`].
#[derive(Default)]
pub struct ChartDescriptionRust {
    view: QVariant,
    opened: i32,
}

/// An error as QML gets it: its code and every parameter as text.
fn error_map(error: &ReportError) -> QVariant {
    let report = error.report();
    let params = report.params.iter().map(|(name, value)| {
        let shown = match value {
            ErrorParam::Keyword(word) => (*word).to_owned(),
            ErrorParam::Text(value) => value.clone(),
            ErrorParam::Number(number) => number.to_string(),
            _ => String::new(),
        };
        (*name, text(&shown))
    });
    let mut entries = vec![("code", text(report.code))];
    entries.extend(params);
    map(entries)
}

/// The object QML reads for a check; `read` is the text of an opened file,
/// `opened` the number of files read so far.
pub(crate) fn view_object(check: &Check, read: &str, opened: i32) -> QVariant {
    let count = i32::try_from(check.unsupported.len()).unwrap_or(i32::MAX);
    let mut entries = vec![
        ("ok", QVariant::from(&check.refused.is_none())),
        ("dataset", text(&check.dataset)),
        ("canDraw", QVariant::from(&check.can_draw)),
        ("count", QVariant::from(&count)),
        (
            "unsupported",
            list(
                check
                    .unsupported
                    .iter()
                    .map(|(path, reason)| map([("path", text(path)), ("reason", text(reason))])),
            ),
        ),
        ("text", text(read)),
        ("opened", QVariant::from(&opened)),
    ];
    if let Some(error) = &check.refused {
        entries.push(("error", error_map(error)));
    }
    map(entries)
}

impl qobject::ChartDescription {
    fn show(mut self: Pin<&mut Self>, view: QVariant) {
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    fn check(self: Pin<&mut Self>, text: &QString) {
        let opened = self.rust().opened;
        let view = view_object(&chart_spec::check(&text.to_string()), "", opened);
        self.show(view);
    }

    fn open_file(mut self: Pin<&mut Self>, file: &QUrl) {
        let result = match local_path(file) {
            Some(path) => chart_spec::read_file(&path),
            None => Err(ReportError::SpecInvalid {
                reason: "unreadable",
                path: String::new(),
            }),
        };
        let view = match result {
            Ok(read) => {
                let opened = self.rust().opened.saturating_add(1);
                self.as_mut().rust_mut().opened = opened;
                view_object(&chart_spec::check(&read), &read, opened)
            }
            // The field keeps what it had: only the reason is shown.
            Err(error) => {
                let failed = Check {
                    refused: Some(error),
                    dataset: String::new(),
                    can_draw: false,
                    unsupported: Vec::new(),
                };
                view_object(&failed, "", self.rust().opened)
            }
        };
        self.show(view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, object_of, text_of};

    const MONTHLY: &str = r#"{"data":{"name":"income_expense_by_month"},"mark":"bar",
      "encoding":{"x":{"field":"month","type":"temporal","timeUnit":"yearmonth"},
      "y":{"field":"net","type":"quantitative"}}}"#;

    /// A fully supported description, a description with a part that is not
    /// supported and a refused one, read back the way QML reads them.
    #[test]
    fn the_three_kinds_of_result_read_back_the_way_qml_reads_them() {
        let ok = view_object(&chart_spec::check(MONTHLY), "", 0);
        assert_eq!(
            keys(&ok),
            [
                "canDraw",
                "count",
                "dataset",
                "ok",
                "opened",
                "text",
                "unsupported"
            ]
        );
        assert!(flag_of(&ok, "ok"));
        assert!(flag_of(&ok, "canDraw"));
        assert_eq!(text_of(&ok, "dataset"), "income_expense_by_month");
        assert_eq!(number_of(&ok, "count"), 0);
        assert!(list_of(&ok, "unsupported").is_empty());

        let partly = MONTHLY.replacen(
            r#""mark":"bar","#,
            r#""mark":"bar","transform":[{"calculate":"datum.net * 2","as":"x"}],"#,
            1,
        );
        let partly = view_object(&chart_spec::check(&partly), "", 0);
        assert!(flag_of(&partly, "ok"), "kept, but not drawn until chosen");
        assert!(flag_of(&partly, "canDraw"));
        assert_eq!(number_of(&partly, "count"), 1);
        let parts = list_of(&partly, "unsupported");
        assert_eq!(keys(&parts[0]), ["path", "reason"]);
        assert_eq!(text_of(&parts[0], "path"), "transform[0].calculate");
        assert_eq!(text_of(&parts[0], "reason"), "expression");

        let refused = view_object(&chart_spec::check("{\"mark\":"), "", 0);
        assert!(!flag_of(&refused, "ok"));
        assert!(!flag_of(&refused, "canDraw"));
        let error = object_of(&refused, "error");
        assert_eq!(keys(&error), ["code", "path", "reason"]);
        assert_eq!(text_of(&error, "code"), "report.spec-invalid");
        assert_eq!(text_of(&error, "reason"), "not-json");
    }

    #[test]
    fn a_description_with_only_a_url_has_a_list_and_nothing_to_draw() {
        let only_url = r#"{"data":{"url":"https://example.invalid/x.json"},"mark":"bar"}"#;
        let view = view_object(&chart_spec::check(only_url), "", 0);
        assert!(flag_of(&view, "ok"));
        assert!(!flag_of(&view, "canDraw"));
        assert_eq!(text_of(&view, "dataset"), "");
        let parts = list_of(&view, "unsupported");
        assert_eq!(text_of(&parts[0], "reason"), "data-source");
        assert_eq!(text_of(&parts[0], "path"), "data.url");
    }

    #[test]
    fn the_text_of_a_file_and_the_count_of_files_reach_the_field() {
        let view = view_object(&chart_spec::check(MONTHLY), MONTHLY, 3);
        assert_eq!(text_of(&view, "text"), MONTHLY);
        assert_eq!(number_of(&view, "opened"), 3);
        assert!(list_of(&view, "unsupported").is_empty());
    }

    #[test]
    fn too_big_and_unreadable_files_give_the_codes_of_the_design() {
        let dir = std::env::temp_dir().join(format!("ks-spec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).expect("a file");
            path
        };

        // A file as long as the limit is read; one byte more is refused.
        let limit = kassensturz_report::spec::MAX_SPEC_BYTES;
        let padded = format!("{MONTHLY}{}", " ".repeat(limit - MONTHLY.len()));
        assert_eq!(padded.len(), limit);
        let read = chart_spec::read_file(&write("exact.vl.json", padded.as_bytes())).expect("read");
        assert_eq!(read, padded);
        let over = format!("{padded} ");
        let error = chart_spec::read_file(&write("over.vl.json", over.as_bytes())).unwrap_err();
        let view = view_object(
            &Check {
                refused: Some(error),
                dataset: String::new(),
                can_draw: false,
                unsupported: Vec::new(),
            },
            "",
            0,
        );
        assert_eq!(
            text_of(&object_of(&view, "error"), "code"),
            "report.spec-too-big"
        );
        assert_eq!(text_of(&object_of(&view, "error"), "limit"), "bytes");

        // A huge file costs a read of the limit and one byte, not its size.
        let huge = dir.join("huge.json");
        let file = std::fs::File::create(&huge).expect("a file");
        file.set_len(5 * 1024 * 1024 * 1024).expect("a sparse file");
        assert!(matches!(
            chart_spec::read_file(&huge),
            Err(ReportError::SpecTooBig { limit: "bytes" })
        ));

        // Bytes that are not UTF-8.
        let error =
            chart_spec::read_file(&write("latin.json", b"{\"title\":\"caf\xe9\"}")).unwrap_err();
        assert!(matches!(
            error,
            ReportError::SpecInvalid {
                reason: "not-utf8",
                ..
            }
        ));

        // A leading byte-order mark is taken off, and nothing else changes.
        let mut with_mark = vec![0xef, 0xbb, 0xbf];
        with_mark.extend_from_slice(MONTHLY.as_bytes());
        let read = chart_spec::read_file(&write("bom.json", &with_mark)).expect("read");
        assert_eq!(read, MONTHLY);

        // A folder and a file that isn't there are unreadable.
        for path in [dir.clone(), dir.join("nothing.json")] {
            assert!(matches!(
                chart_spec::read_file(&path),
                Err(ReportError::SpecInvalid {
                    reason: "unreadable",
                    ..
                })
            ));
        }
        std::fs::remove_dir_all(&dir).expect("cleaned up");
    }
}
