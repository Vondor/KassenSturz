// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The properties panel's component as a QML object (M6a.6, D-278). The
//! rules are the property schema's, in `kassensturz-report`; this file
//! keeps the one component being edited and turns it into plain values.
//!
//! `view` holds `type`, `dataset`, `datasets` (the ids the type can show),
//! `fields` (see `properties.rs`), `json` (the component as a document
//! writes it) and, after a refused change, `error`. A refused change leaves
//! the component as it was.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QVariant};
use kassensturz_core::{ErrorParam, Report};
use kassensturz_report::{Component, ComponentType, ReportError, Setting};

use crate::properties::{self, Field};
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
        /// The component being edited. `view` is empty until `open` or
        /// `load` has been called.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, view, READ, NOTIFY)]
        type ComponentProperties = super::ComponentPropertiesRust;
    }

    extern "RustQt" {
        /// Starts a new component of this type (`chart.line`, ...) on this
        /// dataset, with every property at its default.
        #[qinvokable]
        fn open(self: Pin<&mut ComponentProperties>, kind: &QString, dataset: &QString);

        /// Starts from a component of a document, as JSON text. A component
        /// the schema doesn't allow gives an `error` and an empty view.
        #[qinvokable]
        fn load(self: Pin<&mut ComponentProperties>, json: &QString);

        /// Makes the component another type (`chart.pie`, ...), keeping what
        /// still fits.
        #[qinvokable]
        #[cxx_name = "setType"]
        fn set_type(self: Pin<&mut ComponentProperties>, kind: &QString);

        /// Shows another dataset; settings that no longer fit go back to
        /// their defaults.
        #[qinvokable]
        #[cxx_name = "setDataset"]
        fn set_dataset(self: Pin<&mut ComponentProperties>, dataset: &QString);

        /// Sets a text property (a title).
        #[qinvokable]
        #[cxx_name = "setText"]
        fn set_text(self: Pin<&mut ComponentProperties>, key: &QString, value: &QString);

        /// Sets a yes-or-no property.
        #[qinvokable]
        #[cxx_name = "setFlag"]
        fn set_flag(self: Pin<&mut ComponentProperties>, key: &QString, value: bool);

        /// Sets a property that is one word of a list.
        #[qinvokable]
        #[cxx_name = "setWord"]
        fn set_word(self: Pin<&mut ComponentProperties>, key: &QString, value: &QString);
    }
}

/// The Rust side of [`qobject::ComponentProperties`].
#[derive(Default)]
pub struct ComponentPropertiesRust {
    view: QVariant,
    component: Option<Component>,
}

fn error_object(error: &ReportError) -> QVariant {
    let report = error.report();
    let shown = |name: &str| match report.param(name) {
        Some(ErrorParam::Keyword(word)) => (*word).to_owned(),
        Some(ErrorParam::Text(value)) => value.clone(),
        _ => String::new(),
    };
    map([
        ("code", text(report.code)),
        ("reason", text(&shown("reason"))),
        ("path", text(&shown("path"))),
    ])
}

fn field_object(field: &Field) -> QVariant {
    map([
        ("key", text(field.key)),
        ("place", text(field.place)),
        ("kind", text(field.kind)),
        ("text", text(&field.text)),
        ("flag", QVariant::from(&field.flag)),
        ("word", text(&field.word)),
        ("words", list(field.words.iter().map(|word| text(word)))),
    ])
}

/// What QML gets for a component, with the error of the last refused change.
fn view_object(component: Option<&Component>, error: Option<&ReportError>) -> QVariant {
    let mut entries = Vec::new();
    if let Some(component) = component {
        entries.push(("type", text(component.kind().id())));
        entries.push(("dataset", text(component.dataset().id)));
        entries.push((
            "datasets",
            list(
                properties::dataset_ids(component.kind())
                    .into_iter()
                    .map(text),
            ),
        ));
        entries.push((
            "fields",
            list(properties::fields(component).iter().map(field_object)),
        ));
        entries.push(("json", text(&properties::json(component))));
        let preview = properties::preview(component);
        entries.push((
            "preview",
            map([
                ("shows", text(preview.shows)),
                ("chart", text(preview.chart)),
                ("grouping", text(preview.grouping)),
                ("chosen", text(&preview.chosen)),
                ("showHidden", QVariant::from(&preview.show_hidden)),
                ("compare", text(&preview.compare)),
                ("measure", text(&preview.measure)),
            ]),
        ));
    }
    if let Some(error) = error {
        entries.push(("error", error_object(error)));
    }
    map(entries)
}

impl qobject::ComponentProperties {
    fn show(mut self: Pin<&mut Self>, error: Option<&ReportError>) {
        let view = view_object(self.rust().component.as_ref(), error);
        self.as_mut().rust_mut().view = view;
        self.as_mut().view_changed();
    }

    fn start(mut self: Pin<&mut Self>, made: Result<Component, ReportError>) {
        match made {
            Ok(component) => {
                self.as_mut().rust_mut().component = Some(component);
                self.show(None);
            }
            Err(error) => {
                self.as_mut().rust_mut().component = None;
                self.show(Some(&error));
            }
        }
    }

    fn open(self: Pin<&mut Self>, kind: &QString, dataset: &QString) {
        let made = ComponentType::from_id(&kind.to_string())
            .ok_or(ReportError::DocumentInvalid {
                reason: "unknown-type",
                path: "type".to_owned(),
            })
            .and_then(|kind| Component::new(kind, &dataset.to_string()));
        self.start(made);
    }

    fn load(self: Pin<&mut Self>, json: &QString) {
        self.start(Component::from_json(&json.to_string()));
    }

    /// Applies a change to a copy and keeps it only if the schema allows it.
    fn change(
        mut self: Pin<&mut Self>,
        change: impl FnOnce(&mut Component) -> Result<(), ReportError>,
    ) {
        let Some(mut component) = self.rust().component.clone() else {
            return;
        };
        match change(&mut component) {
            Ok(()) => {
                self.as_mut().rust_mut().component = Some(component);
                self.show(None);
            }
            Err(error) => self.show(Some(&error)),
        }
    }

    fn set_type(self: Pin<&mut Self>, kind: &QString) {
        let kind = kind.to_string();
        self.change(|component| {
            let kind = ComponentType::from_id(&kind).ok_or(ReportError::DocumentInvalid {
                reason: "unknown-type",
                path: "type".to_owned(),
            })?;
            component.change_kind(kind)
        });
    }

    fn set_dataset(self: Pin<&mut Self>, dataset: &QString) {
        let dataset = dataset.to_string();
        self.change(|component| component.use_dataset(&dataset));
    }

    fn set_text(self: Pin<&mut Self>, key: &QString, value: &QString) {
        let (key, value) = (key.to_string(), value.to_string());
        self.change(|component| component.set(&key, Setting::Text(value)));
    }

    fn set_flag(self: Pin<&mut Self>, key: &QString, value: bool) {
        let key = key.to_string();
        self.change(|component| component.set(&key, Setting::Flag(value)));
    }

    fn set_word(self: Pin<&mut Self>, key: &QString, value: &QString) {
        let (key, value) = (key.to_string(), value.to_string());
        self.change(|component| component.set(&key, Setting::Word(value)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::read_back::{flag_of, keys, list_of, object_of, text_of};

    fn component(kind: ComponentType, dataset: &str) -> Component {
        Component::new(kind, dataset).expect("a component the schema allows")
    }

    #[test]
    fn a_chart_reads_back_the_way_qml_reads_it() {
        let chart = component(ComponentType::ChartPie, "spending_by_category");
        let view = view_object(Some(&chart), None);
        assert_eq!(
            keys(&view),
            ["dataset", "datasets", "fields", "json", "preview", "type"]
        );
        assert_eq!(text_of(&view, "type"), "chart.pie");
        assert_eq!(text_of(&view, "dataset"), "spending_by_category");
        let datasets: Vec<String> = list_of(&view, "datasets")
            .iter()
            .map(|id| id.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(datasets, ["spending_by_category"]);
        let fields = list_of(&view, "fields");
        let order: Vec<String> = fields.iter().map(|f| text_of(f, "key")).collect();
        assert_eq!(order, ["grouping", "title", "legend", "show_hidden"]);
        for field in &fields {
            assert_eq!(
                keys(field),
                ["flag", "key", "kind", "place", "text", "word", "words"]
            );
        }
        assert_eq!(text_of(&fields[0], "kind"), "choice");
        assert_eq!(text_of(&fields[0], "place"), "params");
        assert_eq!(text_of(&fields[0], "word"), "category");
        let words: Vec<String> = list_of(&fields[0], "words")
            .iter()
            .map(|w| w.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(words, ["category", "top_level_category"]);
        assert_eq!(text_of(&fields[1], "kind"), "text");
        assert_eq!(text_of(&fields[2], "kind"), "flag");
        assert!(flag_of(&fields[2], "flag"));
        let preview = object_of(&view, "preview");
        assert_eq!(
            keys(&preview),
            [
                "chart",
                "chosen",
                "compare",
                "grouping",
                "measure",
                "showHidden",
                "shows"
            ]
        );
        assert_eq!(text_of(&preview, "shows"), "chart");
        assert_eq!(text_of(&preview, "chart"), "pie");
        assert_eq!(text_of(&preview, "grouping"), "each");
    }

    #[test]
    fn the_preview_carries_groups_series_and_the_hidden_flag_and_the_panel_does_not_offer_them() {
        let mut pie = component(ComponentType::ChartPie, "spending_by_category");
        pie.set("grouping", Setting::Word("groups".into())).unwrap();
        pie.set("groups", Setting::Ids(vec!["a".into(), "b".into()]))
            .unwrap();
        pie.set("show_hidden", Setting::Flag(true)).unwrap();
        let preview = properties::preview(&pie);
        assert_eq!(preview.grouping, "groups");
        assert_eq!(preview.chosen, "a,b");
        assert!(preview.show_hidden);
        let view = view_object(Some(&pie), None);
        let order: Vec<String> = list_of(&view, "fields")
            .iter()
            .map(|f| text_of(f, "key"))
            .collect();
        // Lists of IDs have no field; the grouping offers no word for groups
        // until a group can be picked.
        assert_eq!(order, ["grouping", "title", "legend", "show_hidden"]);
        let grouping = &list_of(&view, "fields")[0];
        assert_eq!(list_of(grouping, "words").len(), 2);
        let preview = object_of(&view, "preview");
        assert_eq!(text_of(&preview, "chosen"), "a,b");
        assert!(flag_of(&preview, "showHidden"));
        // Monthly figures leave nothing out, so no field for hidden ones.
        let monthly = component(ComponentType::ChartLine, "income_expense_by_month");
        let view = view_object(Some(&monthly), None);
        assert!(
            !list_of(&view, "fields")
                .iter()
                .any(|f| text_of(f, "key") == "show_hidden")
        );
    }

    #[test]
    fn the_preview_names_what_charts_and_cards_are_asked_for() {
        let ask =
            |kind: ComponentType, dataset: &str| properties::preview(&component(kind, dataset));
        let top = {
            let mut pie = component(ComponentType::ChartPie, "spending_by_category");
            pie.set("grouping", Setting::Word("top_level_category".into()))
                .unwrap();
            properties::preview(&pie)
        };
        assert_eq!((top.chart, top.grouping), ("pie", "top-level"));
        let card = ask(ComponentType::KpiCard, "income_expense_by_month");
        assert_eq!(
            (
                card.shows,
                card.chart,
                card.measure.as_str(),
                card.compare.as_str()
            ),
            ("card", "", "income", "previous_period")
        );
        let bar = ask(ComponentType::ProgressBar, "income_expense_by_month");
        assert_eq!(
            (bar.shows, bar.measure.as_str()),
            ("progress", "savings-rate")
        );
        let table = ask(ComponentType::TableRows, "spending_by_category");
        assert_eq!((table.shows, table.chart), ("table", "bar"));
        // Every name is one `BookCharts` and `BookCards` take.
        for kind in ComponentType::ALL {
            for dataset in kind.datasets() {
                let preview = ask(kind, dataset.id);
                assert!(
                    ["", "line", "bar", "stacked-bar", "area", "pie", "donut"]
                        .contains(&preview.chart)
                );
                assert!(["top-level", "each"].contains(&preview.grouping));
            }
        }
    }

    #[test]
    fn what_the_preview_asks_for_is_accepted_by_charts_and_cards() {
        use crate::register::tests::TestLocale;
        use crate::{cards, charts};

        let book = kassensturz_store::Book::create_in_memory().unwrap();
        let locale = TestLocale;
        for kind in ComponentType::ALL {
            for dataset in kind.datasets() {
                let component = component(kind, dataset.id);
                let preview = properties::preview(&component);
                if preview.chart.is_empty() {
                    let request = cards::Request {
                        period: "this-year".into(),
                        compare: preview.compare.clone(),
                    };
                    cards::cards(&book, &locale, &request).expect("cards accept it");
                } else {
                    let request = charts::Request {
                        dataset: dataset.id.into(),
                        kind: preview.chart.into(),
                        period: "this-year".into(),
                        grouping: preview.grouping.into(),
                        groups: Vec::new(),
                        series: Vec::new(),
                        show_hidden: preview.show_hidden,
                    };
                    charts::charts(&book, &locale, &request)
                        .unwrap_or_else(|e| panic!("{} on {}: {e:?}", kind.id(), dataset.id));
                }
            }
        }
    }

    #[test]
    fn a_choice_with_nothing_to_choose_has_no_field() {
        // Monthly figures have one grouping, so no "group by" field; a key
        // figure card chooses a figure and a comparison, then has its title.
        let card = component(ComponentType::KpiCard, "income_expense_by_month");
        let view = view_object(Some(&card), None);
        let order: Vec<String> = list_of(&view, "fields")
            .iter()
            .map(|f| text_of(f, "key"))
            .collect();
        assert_eq!(order, ["measure", "compare", "title"]);
        let line = component(ComponentType::ChartLine, "income_expense_by_month");
        let order: Vec<String> = list_of(&view_object(Some(&line), None), "fields")
            .iter()
            .map(|f| text_of(f, "key"))
            .collect();
        assert_eq!(order, ["title", "legend"]);
    }

    #[test]
    fn json_is_the_document_part_and_reads_back_into_the_same_component() {
        let mut chart = component(ComponentType::ChartDonut, "spending_by_category");
        chart
            .set("title", Setting::Text("Where it went".into()))
            .unwrap();
        let view = view_object(Some(&chart), None);
        let json = text_of(&view, "json");
        let again = Component::from_json(&json).unwrap();
        assert_eq!(again, chart);
    }

    #[test]
    fn a_refused_change_is_an_error_object_and_the_component_stays() {
        let mut chart = component(ComponentType::ChartBar, "spending_by_category");
        let before = chart.clone();
        let error = chart
            .set("grouping", Setting::Word("month".into()))
            .unwrap_err();
        assert_eq!(chart, before);
        let view = view_object(Some(&chart), Some(&error));
        let shown = object_of(&view, "error");
        assert_eq!(keys(&shown), ["code", "path", "reason"]);
        assert_eq!(text_of(&shown, "code"), "report.document-invalid");
        assert_eq!(text_of(&shown, "reason"), "bad-choice");
        assert_eq!(text_of(&shown, "path"), "params.grouping");
    }

    #[test]
    fn no_component_is_an_empty_view_with_an_error_if_it_could_not_be_made() {
        assert!(keys(&view_object(None, None)).is_empty());
        let error = Component::new(ComponentType::ChartPie, "income_expense_by_month").unwrap_err();
        let view = view_object(None, Some(&error));
        assert_eq!(keys(&view), ["error"]);
        assert_eq!(
            text_of(&object_of(&view, "error"), "reason"),
            "dataset-not-suited"
        );
    }
}
