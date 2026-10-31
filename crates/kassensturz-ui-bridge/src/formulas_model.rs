// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The result of a formula, and what completes it, as a QML object (M6b.7,
//! D-322), on the book the app has open (`session.rs`). The work is in
//! `formulas.rs`; this file turns it into the plain values QML reads.
//! Figures are text; only `permille` is a number, and it only places the
//! mark of a progress bar.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};

use crate::charts::ChartsError;
use crate::charts_model::error_object as charts_error_object;
use crate::formulas::{self, Completion, Line, Mistake, Outcome, Request, Shown};
use crate::number_format::app_locale;
use crate::scope::Scope;
use crate::session;
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// A formula worked out on the open book. Ask with `show`; `view`
        /// then holds `empty`, the figure with what it counted, or an
        /// `error` with the place marked.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type BookFormula = super::BookFormulaRust;
    }

    extern "RustQt" {
        /// Works `formula` out for `period` (`this-month`, `last-month`,
        /// `last-12-months`, `this-year`, `last-year` or a month as
        /// `YYYY-MM`). `typed` says it is written in the user's number
        /// format, as in the field; else it is in the form the book stores.
        #[qinvokable]
        fn show(self: Pin<&mut BookFormula>, period: &QString, formula: &QString, typed: bool);

        /// Works a progress bar's `formula` out and measures it against its
        /// `target`, a formula of what a full bar stands for (empty for
        /// none), both in the form the book stores. The view is as for
        /// `show`; its result has `permille` and `target` too.
        #[qinvokable]
        #[cxx_name = "showBar"]
        fn show_bar(
            self: Pin<&mut BookFormula>,
            period: &QString,
            formula: &QString,
            target: &QString,
        );

        /// Sets the filters the next requests are narrowed by (M6c.7,
        /// D-350): the `scope` text the dashboard's layout gives a component
        /// (empty for none). Call it before `show` or `showBar`.
        #[qinvokable]
        #[cxx_name = "setScope"]
        fn set_scope(self: Pin<&mut BookFormula>, scope: &QString);

        /// Reads the open book again for the last request, e.g. after the
        /// book or the number format changed.
        #[qinvokable]
        fn refresh(self: Pin<&mut BookFormula>);

        /// The names of the book that fit the word at `cursor` (a position
        /// in `formula` as QML counts it): an object with `start`, `end`
        /// (what to replace) and `names`, each with `insert`, `label` and
        /// `group`. Nothing fits where no name is being typed.
        #[qinvokable]
        fn complete(self: Pin<&mut BookFormula>, formula: &QString, cursor: i32) -> QVariant;

        /// The functions a formula can call, each with its `name` and
        /// `arguments`, and how the user's number format writes the
        /// `separator` between them and the `decimal` mark.
        #[qinvokable]
        fn functions(self: Pin<&mut BookFormula>) -> QVariant;
    }
}

/// The Rust side of [`qobject::BookFormula`].
#[derive(Default)]
pub struct BookFormulaRust {
    view: QVariant,
    request: Option<Request>,
    scope: Scope,
}

impl qobject::BookFormula {
    fn show(mut self: Pin<&mut Self>, period: &QString, formula: &QString, typed: bool) {
        let scope = self.rust().scope.clone();
        self.as_mut().rust_mut().request = Some(Request {
            period: period.to_string(),
            text: formula.to_string(),
            typed,
            target: None,
            scope,
        });
        self.refresh();
    }

    fn show_bar(mut self: Pin<&mut Self>, period: &QString, formula: &QString, target: &QString) {
        let scope = self.rust().scope.clone();
        self.as_mut().rust_mut().request = Some(Request {
            period: period.to_string(),
            text: formula.to_string(),
            typed: false,
            target: Some(target.to_string()),
            scope,
        });
        self.refresh();
    }

    fn set_scope(mut self: Pin<&mut Self>, scope: &QString) {
        self.as_mut().rust_mut().scope = Scope::from_text(&scope.to_string());
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let Some(request) = self.rust().request.clone() else {
            return;
        };
        let locale = app_locale();
        let view = match session::with_book(|book| formulas::show(book, &locale, &request)) {
            Some(result) => view_object(&result),
            None => QVariant::default(),
        };
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    fn complete(self: Pin<&mut Self>, formula: &QString, cursor: i32) -> QVariant {
        let cursor = usize::try_from(cursor).unwrap_or(0);
        let completion = session::with_book(|book| {
            formulas::complete(book, &app_locale(), &formula.to_string(), cursor)
        })
        .unwrap_or_default();
        completion_object(&completion)
    }

    fn functions(self: Pin<&mut Self>) -> QVariant {
        functions_object(&formulas::number_form(&app_locale()))
    }
}

fn line_object(line: &Line) -> QVariant {
    map([
        ("name", text(&line.name)),
        ("inside", list(line.inside.iter().map(|name| text(name)))),
        (
            "insideMore",
            QVariant::from(&i32::try_from(line.inside_more).unwrap_or(i32::MAX)),
        ),
        ("ownPeriod", QVariant::from(&line.own_period)),
        ("month", text(&line.month)),
        ("firstDay", text(&line.first_day)),
        ("lastDay", text(&line.last_day)),
    ])
}

fn shown_object(shown: &Shown) -> QVariant {
    let mut entries = vec![
        ("kind", text(shown.kind)),
        ("value", text(&shown.value)),
        ("exactValue", text(&shown.exact_value)),
        ("negative", QVariant::from(&shown.negative)),
        ("currency", text(&shown.currency)),
        ("counted", list(shown.counted.iter().map(line_object))),
    ];
    // Only a bar has a fill, and only when the figure can fill it.
    if let Some(permille) = shown.permille {
        entries.push(("permille", QVariant::from(&permille)));
        entries.push(("target", text(&shown.target)));
    }
    map(entries)
}

fn mistake_object(mistake: &Mistake) -> QVariant {
    map([
        ("error", error_object(&mistake.error)),
        ("before", text(&mistake.before)),
        ("after", text(&mistake.after)),
    ])
}

pub(crate) fn view_object(result: &Result<Outcome, ChartsError>) -> QVariant {
    match result {
        Ok(Outcome::Empty) => map([("empty", QVariant::from(&true))]),
        Ok(Outcome::Shown(shown)) => map([
            ("empty", QVariant::from(&false)),
            ("result", shown_object(shown)),
            ("filter", shown.filter.object()),
        ]),
        Ok(Outcome::Nothing(filter)) => map([
            ("empty", QVariant::from(&false)),
            ("nothing", QVariant::from(&true)),
            ("filter", filter.object()),
        ]),
        Ok(Outcome::Mistake(mistake)) => map([
            ("empty", QVariant::from(&false)),
            ("mistake", mistake_object(mistake)),
        ]),
        Err(error) => charts_error_object(error),
    }
}

fn completion_object(completion: &Completion) -> QVariant {
    let start = i32::try_from(completion.start).unwrap_or(0);
    let end = i32::try_from(completion.end).unwrap_or(0);
    map([
        ("start", QVariant::from(&start)),
        ("end", QVariant::from(&end)),
        (
            "names",
            list(completion.names.iter().map(|name| {
                map([
                    ("insert", text(&name.insert)),
                    ("label", text(&name.label)),
                    ("group", QVariant::from(&name.group)),
                ])
            })),
        ),
    ])
}

fn functions_object(form: &formulas::Writing) -> QVariant {
    map([
        ("separator", text(form.separator)),
        ("decimal", text(form.decimal)),
        (
            "functions",
            list(formulas::functions().map(|(name, arguments)| {
                map([
                    ("name", text(name)),
                    (
                        "arguments",
                        list(arguments.iter().map(|argument| text(argument))),
                    ),
                ])
            })),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formulas::tests::{PointLocale, book};
    use crate::register::tests::TestLocale;
    use crate::variant::read_back::{flag_of, keys, list_of, number_of, object_of, text_of};

    fn request(text: &str, target: Option<&str>) -> Request {
        Request {
            period: "last-month".into(),
            text: text.into(),
            typed: false,
            target: target.map(str::to_owned),
            scope: Scope::default(),
        }
    }

    fn viewed(request: &Request) -> QVariant {
        view_object(&formulas::show(&book(), &TestLocale, request))
    }

    #[test]
    fn a_result_reads_back_the_way_qml_reads_it() {
        let view = viewed(&request("=Pets", None));
        assert_eq!(keys(&view), ["empty", "result"]);
        assert!(!flag_of(&view, "empty"));
        let result = object_of(&view, "result");
        assert_eq!(
            keys(&result),
            [
                "counted",
                "currency",
                "exactValue",
                "kind",
                "negative",
                "value"
            ]
        );
        assert_eq!(text_of(&result, "kind"), "amount");
        assert_eq!(text_of(&result, "value"), "80,00 €");
        assert_eq!(text_of(&result, "exactValue"), "80,00 €");
        assert_eq!(text_of(&result, "currency"), "EUR");
        assert!(!flag_of(&result, "negative"));
        let counted = list_of(&result, "counted");
        assert_eq!(counted.len(), 1);
        assert_eq!(
            keys(&counted[0]),
            [
                "firstDay",
                "inside",
                "insideMore",
                "lastDay",
                "month",
                "name",
                "ownPeriod"
            ]
        );
        assert_eq!(text_of(&counted[0], "name"), "Pets");
        let inside: Vec<String> = list_of(&counted[0], "inside")
            .iter()
            .map(|name| name.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(inside, ["Vet"]);
        assert_eq!(number_of(&counted[0], "insideMore"), 0);
        assert!(flag_of(&counted[0], "ownPeriod"));
    }

    #[test]
    fn a_bar_reads_back_with_its_fill_and_target() {
        let view = viewed(&request("=Pets", Some("=Salary / 10")));
        let result = object_of(&view, "result");
        assert_eq!(
            keys(&result),
            [
                "counted",
                "currency",
                "exactValue",
                "kind",
                "negative",
                "permille",
                "target",
                "value"
            ]
        );
        assert_eq!(number_of(&result, "permille"), 400);
        assert_eq!(text_of(&result, "target"), "200,00 €");
        // A percent without a target fills the bar and has no target text.
        let view = viewed(&request("=PERCENT_OF(Pets; Salary)", Some("")));
        let result = object_of(&view, "result");
        assert_eq!(number_of(&result, "permille"), 40);
        assert_eq!(text_of(&result, "target"), "");
        // An amount alone has no fill: the keys are those of a plain result.
        let view = viewed(&request("=Pets", Some("")));
        assert!(!keys(&object_of(&view, "result")).contains(&"permille".to_owned()));
    }

    #[test]
    fn a_mistake_reads_back_with_its_place() {
        let view = viewed(&request("=Pets - Nothing", None));
        assert_eq!(keys(&view), ["empty", "mistake"]);
        let mistake = object_of(&view, "mistake");
        assert_eq!(keys(&mistake), ["after", "before", "error"]);
        assert_eq!(text_of(&mistake, "before"), "=Pets - ");
        assert_eq!(text_of(&mistake, "after"), "Nothing");
        let error = object_of(&mistake, "error");
        assert_eq!(
            keys(&error),
            ["code", "decimal", "name", "position", "separator"]
        );
        assert_eq!(text_of(&error, "code"), "report.formula-unknown-name");
        assert_eq!(text_of(&error, "name"), "Nothing");
        assert_eq!(text_of(&error, "position"), "9");
        assert_eq!(text_of(&error, "separator"), ";");
        // The target's mistake is marked as the target's.
        let view = viewed(&request("=Pets", Some("=Nothing")));
        let error = object_of(&object_of(&view, "mistake"), "error");
        assert_eq!(text_of(&error, "target"), "yes");
    }

    #[test]
    fn an_empty_formula_and_a_bad_request_read_back() {
        assert_eq!(keys(&viewed(&request("", None))), ["empty"]);
        let mut bad = request("=Pets", None);
        bad.period = "yesterday".into();
        assert_eq!(keys(&viewed(&bad)), ["error"]);
    }

    #[test]
    fn the_completion_and_the_function_list_read_back() {
        let completion = completion_object(&formulas::complete(&book(), &TestLocale, "=Pe", 3));
        assert_eq!(keys(&completion), ["end", "names", "start"]);
        assert_eq!(number_of(&completion, "start"), 1);
        assert_eq!(number_of(&completion, "end"), 3);
        let names = list_of(&completion, "names");
        // Pets, and Vet inside it, whose path holds the word.
        assert_eq!(names.len(), 2);
        assert_eq!(keys(&names[0]), ["group", "insert", "label"]);
        assert_eq!(text_of(&names[0], "insert"), "Pets");
        assert!(!flag_of(&names[0], "group"));
        // Nothing to complete still has the three keys.
        let none = completion_object(&formulas::complete(&book(), &TestLocale, "=", 1));
        assert_eq!(keys(&none), ["end", "names", "start"]);
        assert!(list_of(&none, "names").is_empty());

        let functions = functions_object(&formulas::number_form(&PointLocale));
        assert_eq!(keys(&functions), ["decimal", "functions", "separator"]);
        assert_eq!(text_of(&functions, "separator"), ",");
        assert_eq!(text_of(&functions, "decimal"), ".");
        let listed = list_of(&functions, "functions");
        assert_eq!(listed.len(), 18);
        assert_eq!(keys(&listed[0]), ["arguments", "name"]);
        assert_eq!(text_of(&listed[0], "name"), "SUM");
        assert_eq!(list_of(&listed[0], "arguments").len(), 2);
    }
}
