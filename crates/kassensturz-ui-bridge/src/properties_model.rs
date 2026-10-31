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
use kassensturz_report::{Component, ComponentType, NumberForm, ReportError, Setting};

use crate::formulas;
use crate::number_format::app_locale;
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

        /// Sets a list of group or category IDs, separated by commas
        /// (empty for none).
        #[qinvokable]
        #[cxx_name = "setIds"]
        fn set_ids(self: Pin<&mut ComponentProperties>, key: &QString, value: &QString);

        /// Sets the columns of a chart or table: a JSON list of
        /// `{"name": ..., "members": [...], "formula": "..."}` (the formula in the
        /// user's number format, empty for none), empty list for none (D-339).
        /// A text that is no such list changes nothing.
        #[qinvokable]
        #[cxx_name = "setColumns"]
        fn set_columns(self: Pin<&mut ComponentProperties>, json: &QString);

        /// Sets a formula property (`formula`, or a progress bar's `target`),
        /// written in the user's number format as in the field. The book
        /// keeps it in the form it stores (D-295). A text that cannot be read
        /// as a formula at all is not kept; the field shows why.
        #[qinvokable]
        #[cxx_name = "setFormula"]
        fn set_formula(self: Pin<&mut ComponentProperties>, key: &QString, value: &QString);

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
        ("ids", list(field.ids.iter().map(|id| text(id)))),
    ])
}

/// What QML gets for a component, with the error of the last refused change.
pub(crate) fn view_object(
    component: Option<&Component>,
    error: Option<&ReportError>,
    form: NumberForm,
) -> QVariant {
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
            list(properties::fields(component, form).iter().map(field_object)),
        ));
        entries.push(("json", text(&properties::json(component))));
        // The columns, as JSON text, for a component that has them (D-339).
        // Their formulas are written in the user's number format (D-341).
        let show = |stored: &str| formulas::users_form(form, stored);
        if let Some(columns) = properties::columns_json(component, &show) {
            entries.push(("columns", text(&columns)));
        }
        if let Some((word, words)) = properties::period(component) {
            entries.push((
                "period",
                map([
                    ("word", text(&word)),
                    ("words", list(words.into_iter().map(text))),
                ]),
            ));
        }
        let preview = properties::preview(component);
        entries.push((
            "preview",
            map([
                ("shows", text(preview.shows)),
                ("chart", text(preview.chart)),
                ("grouping", text(preview.grouping)),
                ("chosen", text(&preview.chosen)),
                ("only", text(&preview.only)),
                ("showHidden", QVariant::from(&preview.show_hidden)),
                ("columns", text(&preview.columns)),
                ("compare", text(&preview.compare)),
                ("measure", text(&preview.measure)),
                ("usesFormula", QVariant::from(&preview.uses_formula)),
                ("formula", text(&preview.formula)),
                ("target", text(&preview.target)),
                ("scope", text(&preview.scope)),
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
        let form = formulas::form(&app_locale());
        let view = view_object(self.rust().component.as_ref(), error, form);
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

    fn set_ids(self: Pin<&mut Self>, key: &QString, value: &QString) {
        let key = key.to_string();
        let ids = value
            .to_string()
            .split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();
        self.change(|component| component.set(&key, Setting::Ids(ids)));
    }

    fn set_columns(self: Pin<&mut Self>, json: &QString) {
        // A formula comes in the user's number format and goes to the book
        // in the form it stores; empty means no formula.
        let keep = |typed: &str| {
            if typed.trim().is_empty() {
                Some(String::new())
            } else {
                formulas::canonical(&app_locale(), typed.trim()).ok()
            }
        };
        let Some(columns) = properties::read_columns(&json.to_string(), &keep) else {
            return;
        };
        self.change(|component| component.set("columns", Setting::Columns(columns)));
    }

    fn set_formula(self: Pin<&mut Self>, key: &QString, value: &QString) {
        let key = key.to_string();
        let typed = value.to_string();
        let stored = if typed.trim().is_empty() {
            Ok(String::new())
        } else {
            formulas::canonical(&app_locale(), typed.trim())
        };
        // A text that is no formula at all changes nothing: the field has
        // shown the mistake while it was typed.
        if let Ok(stored) = stored {
            self.change(|component| component.set(&key, Setting::Text(stored)));
        }
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
        let view = view_object(Some(&chart), None, NumberForm::DecimalComma);
        assert_eq!(
            keys(&view),
            [
                "columns", "dataset", "datasets", "fields", "json", "period", "preview", "type"
            ]
        );
        // The grid editor's panel offers the period; the workbench's fields
        // do not (it has its own).
        let period = object_of(&view, "period");
        assert_eq!(keys(&period), ["word", "words"]);
        assert_eq!(text_of(&period, "word"), "page");
        assert_eq!(list_of(&period, "words").len(), 6);
        assert_eq!(text_of(&view, "type"), "chart.pie");
        assert_eq!(text_of(&view, "dataset"), "spending_by_category");
        let datasets: Vec<String> = list_of(&view, "datasets")
            .iter()
            .map(|id| id.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(datasets, ["spending_by_category"]);
        let fields = list_of(&view, "fields");
        let order: Vec<String> = fields.iter().map(|f| text_of(f, "key")).collect();
        assert_eq!(
            order,
            ["grouping", "categories", "accounts", "title", "legend", "show_hidden"]
        );
        for field in &fields {
            assert_eq!(
                keys(field),
                [
                    "flag", "ids", "key", "kind", "place", "text", "word", "words"
                ]
            );
        }
        assert_eq!(text_of(&fields[0], "kind"), "choice");
        assert_eq!(text_of(&fields[0], "place"), "params");
        assert_eq!(text_of(&fields[0], "word"), "category");
        let words: Vec<String> = list_of(&fields[0], "words")
            .iter()
            .map(|w| w.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(words, ["category", "top_level_category", "groups"]);
        assert_eq!(text_of(&fields[1], "kind"), "ids");
        assert_eq!(text_of(&fields[1], "key"), "categories");
        // The accounts it counts on its own (M6c.7).
        assert_eq!(text_of(&fields[2], "kind"), "ids");
        assert_eq!(text_of(&fields[2], "key"), "accounts");
        assert_eq!(text_of(&fields[3], "kind"), "text");
        assert_eq!(text_of(&fields[4], "kind"), "flag");
        assert!(flag_of(&fields[4], "flag"));
        let preview = object_of(&view, "preview");
        assert_eq!(
            keys(&preview),
            [
                "chart",
                "chosen",
                "columns",
                "compare",
                "formula",
                "grouping",
                "measure",
                "only",
                "scope",
                "showHidden",
                "shows",
                "target",
                "usesFormula"
            ]
        );
        assert_eq!(text_of(&preview, "shows"), "chart");
        assert_eq!(text_of(&preview, "chart"), "pie");
        assert_eq!(text_of(&preview, "grouping"), "each");
    }

    #[test]
    fn the_preview_carries_groups_series_and_the_hidden_flag_and_the_panel_offers_the_lists_where_they_are_used()
     {
        let mut pie = component(ComponentType::ChartPie, "spending_by_category");
        pie.set("grouping", Setting::Word("groups".into())).unwrap();
        pie.set("groups", Setting::Ids(vec!["a".into(), "b".into()]))
            .unwrap();
        pie.set("show_hidden", Setting::Flag(true)).unwrap();
        let preview = properties::preview(&pie);
        assert_eq!(preview.grouping, "groups");
        assert_eq!(preview.chosen, "a,b");
        assert!(preview.show_hidden);
        let view = view_object(Some(&pie), None, NumberForm::DecimalComma);
        let order: Vec<String> = list_of(&view, "fields")
            .iter()
            .map(|f| text_of(f, "key"))
            .collect();
        // The list of groups is a field while the chart groups by them.
        assert_eq!(
            order,
            [
                "grouping",
                "groups",
                "categories",
                "accounts",
                "title",
                "legend",
                "show_hidden"
            ]
        );
        let fields = list_of(&view, "fields");
        assert_eq!(text_of(&fields[1], "kind"), "ids");
        let chosen: Vec<String> = list_of(&fields[1], "ids")
            .iter()
            .map(|id| id.value::<QString>().expect("text").to_string())
            .collect();
        assert_eq!(chosen, ["a", "b"]);
        assert_eq!(list_of(&fields[0], "words").len(), 3);
        let preview = object_of(&view, "preview");
        assert_eq!(text_of(&preview, "chosen"), "a,b");
        assert_eq!(text_of(&preview, "only"), "");
        assert!(flag_of(&preview, "showHidden"));
        // The categories a chart is limited to are asked for as they are
        // written, and only a spending chart offers them.
        pie.set("categories", Setting::Ids(vec!["c".into(), "d".into()]))
            .unwrap();
        assert_eq!(properties::preview(&pie).only, "c,d");
        let view = view_object(Some(&pie), None, NumberForm::DecimalComma);
        assert_eq!(text_of(&object_of(&view, "preview"), "only"), "c,d");
        // Monthly figures leave nothing out, so no field for hidden ones;
        // they can be limited to categories and accounts as a filter does.
        let monthly = component(ComponentType::ChartLine, "income_expense_by_month");
        let view = view_object(Some(&monthly), None, NumberForm::DecimalComma);
        let keys_of = list_of(&view, "fields")
            .iter()
            .map(|f| text_of(f, "key"))
            .collect::<Vec<_>>();
        assert!(!keys_of.contains(&"show_hidden".to_owned()));
        assert!(keys_of.contains(&"categories".to_owned()));
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
                assert!(["top-level", "each", "account-kind"].contains(&preview.grouping));
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
                        dataset: dataset.id.into(),
                        period: "this-year".into(),
                        compare: preview.compare.clone(),
                        scope: crate::scope::Scope::from_text(&preview.scope),
                    };
                    cards::cards(&book, &locale, &request).expect("cards accept it");
                } else {
                    let request = charts::Request {
                        dataset: dataset.id.into(),
                        kind: preview.chart.into(),
                        period: "this-year".into(),
                        grouping: preview.grouping.into(),
                        groups: Vec::new(),
                        categories: Vec::new(),
                        series: Vec::new(),
                        show_hidden: preview.show_hidden,
                        columns: Vec::new(),
                        scope: crate::scope::Scope::from_text(&preview.scope),
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
        let view = view_object(Some(&card), None, NumberForm::DecimalComma);
        let order: Vec<String> = list_of(&view, "fields")
            .iter()
            .map(|f| text_of(f, "key"))
            .collect();
        assert_eq!(order, ["measure", "accounts", "compare", "title"]);
        let line = component(ComponentType::ChartLine, "income_expense_by_month");
        let order: Vec<String> = list_of(
            &view_object(Some(&line), None, NumberForm::DecimalComma),
            "fields",
        )
        .iter()
        .map(|f| text_of(f, "key"))
        .collect();
        // Monthly figures take accounts and categories as filters (M6c.7).
        assert_eq!(order, ["categories", "accounts", "title", "legend"]);
    }

    #[test]
    fn json_is_the_document_part_and_reads_back_into_the_same_component() {
        let mut chart = component(ComponentType::ChartDonut, "spending_by_category");
        chart
            .set("title", Setting::Text("Where it went".into()))
            .unwrap();
        let view = view_object(Some(&chart), None, NumberForm::DecimalComma);
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
        let view = view_object(Some(&chart), Some(&error), NumberForm::DecimalComma);
        let shown = object_of(&view, "error");
        assert_eq!(keys(&shown), ["code", "path", "reason"]);
        assert_eq!(text_of(&shown, "code"), "report.document-invalid");
        assert_eq!(text_of(&shown, "reason"), "bad-choice");
        assert_eq!(text_of(&shown, "path"), "params.grouping");
    }

    #[test]
    fn no_component_is_an_empty_view_with_an_error_if_it_could_not_be_made() {
        assert!(keys(&view_object(None, None, NumberForm::DecimalComma)).is_empty());
        let error = Component::new(ComponentType::ChartPie, "income_expense_by_month").unwrap_err();
        let view = view_object(None, Some(&error), NumberForm::DecimalComma);
        assert_eq!(keys(&view), ["error"]);
        assert_eq!(
            text_of(&object_of(&view, "error"), "reason"),
            "dataset-not-suited"
        );
    }

    #[test]
    fn the_series_of_a_chart_over_time_are_a_field_with_their_ids() {
        let mut line = component(
            ComponentType::ChartLine,
            kassensturz_report::CATEGORY_OVER_TIME,
        );
        line.set("series", Setting::Ids(vec!["x".into(), "y".into()]))
            .unwrap();
        let fields = properties::fields(&line, NumberForm::DecimalComma);
        let series = fields
            .iter()
            .find(|field| field.key == "series")
            .expect("a field for the series");
        assert_eq!(series.kind, "ids");
        assert_eq!(series.ids, ["x", "y"]);
        // The groups of a spending chart are not offered here.
        assert!(!fields.iter().any(|field| field.key == "groups"));
    }

    fn keys_of_fields(component: &Component, form: NumberForm) -> Vec<String> {
        properties::fields(component, form)
            .iter()
            .map(|field| field.key.to_owned())
            .collect()
    }

    #[test]
    fn a_key_figure_offers_its_formula_when_its_measure_is_a_formula() {
        let mut card = component(ComponentType::KpiCard, "income_expense_by_month");
        // Not chosen yet: the formula is not offered, the comparison is.
        assert_eq!(
            keys_of_fields(&card, NumberForm::DecimalComma),
            ["measure", "accounts", "compare", "title"]
        );
        let words: Vec<String> = list_of(
            &view_object(Some(&card), None, NumberForm::DecimalComma),
            "fields",
        )
        .iter()
        .find(|field| text_of(field, "key") == "measure")
        .map(|field| {
            list_of(field, "words")
                .iter()
                .map(|word| word.value::<QString>().expect("text").to_string())
                .collect()
        })
        .expect("the measure field");
        assert_eq!(
            words,
            ["income", "expenses", "net", "savings-rate", "formula"]
        );
        card.set("measure", Setting::Word("formula".into()))
            .unwrap();
        assert_eq!(
            keys_of_fields(&card, NumberForm::DecimalComma),
            ["measure", "accounts", "formula", "title"]
        );
        // The preview asks for the formula, not for a card of the cards.
        let preview = properties::preview(&card);
        assert!(preview.uses_formula);
        assert_eq!(preview.formula, "");
        // The formula is shown in the user's number format and previewed in
        // the form the book stores.
        card.set("formula", Setting::Text("=ROUND(Pets * 1.5; 2)".into()))
            .unwrap();
        for (form, shown) in [
            (NumberForm::DecimalComma, "=ROUND(Pets * 1,5; 2)"),
            (NumberForm::DecimalPoint, "=ROUND(Pets * 1.5, 2)"),
        ] {
            let view = view_object(Some(&card), None, form);
            let fields = list_of(&view, "fields");
            let field = fields
                .iter()
                .find(|field| text_of(field, "key") == "formula")
                .expect("the formula field");
            assert_eq!(text_of(field, "kind"), "formula");
            assert_eq!(text_of(field, "place"), "props");
            assert_eq!(text_of(field, "text"), shown);
            let preview = object_of(&view, "preview");
            assert_eq!(text_of(&preview, "formula"), "=ROUND(Pets * 1.5; 2)");
            assert!(flag_of(&preview, "usesFormula"));
        }
        // Another measure keeps the formula in the document but does not use
        // it.
        card.set("measure", Setting::Word("net".into())).unwrap();
        assert!(!properties::preview(&card).uses_formula);
        assert_eq!(properties::preview(&card).formula, "=ROUND(Pets * 1.5; 2)");
        assert!(!keys_of_fields(&card, NumberForm::DecimalComma).contains(&"formula".to_owned()));
    }

    #[test]
    fn a_progress_bar_offers_a_target_once_it_has_a_formula() {
        let mut bar = component(ComponentType::ProgressBar, "income_expense_by_month");
        // Without a formula it shows the share you kept.
        assert_eq!(
            keys_of_fields(&bar, NumberForm::DecimalComma),
            ["accounts", "compare", "formula", "title"]
        );
        let preview = properties::preview(&bar);
        assert!(!preview.uses_formula);
        assert_eq!(preview.measure, "savings-rate");
        bar.set("formula", Setting::Text("=Pets".into())).unwrap();
        bar.set("target", Setting::Text("=Salary / 10".into()))
            .unwrap();
        assert_eq!(
            keys_of_fields(&bar, NumberForm::DecimalComma),
            ["accounts", "formula", "target", "title"]
        );
        let preview = properties::preview(&bar);
        assert!(preview.uses_formula);
        assert_eq!(
            (preview.formula.as_str(), preview.target.as_str()),
            ("=Pets", "=Salary / 10")
        );
        let view = view_object(Some(&bar), None, NumberForm::DecimalComma);
        let fields = list_of(&view, "fields");
        let target = fields
            .iter()
            .find(|field| text_of(field, "key") == "target")
            .expect("the target field");
        assert_eq!(text_of(target, "kind"), "formula");
        assert_eq!(text_of(target, "text"), "=Salary / 10");
        // The formula is gone again when the text is cleared.
        bar.set("formula", Setting::Text(String::new())).unwrap();
        assert!(!properties::preview(&bar).uses_formula);
    }

    #[test]
    fn a_formula_from_a_saved_document_is_a_field_of_the_panel() {
        // The way the grid editor gets it: the component read from a
        // document's JSON, then shown in the panel and in the preview.
        let json = r#"{"type": "progress.bar", "dataset": "income_expense_by_month",
            "params": {"compare": "previous_period"},
            "props": {"title": "Pets", "period": "page",
                      "formula": "=Pets@\"last month\"", "target": "=300"}}"#;
        let bar = Component::from_json(json).expect("a component the schema allows");
        let view = view_object(Some(&bar), None, NumberForm::DecimalPoint);
        let preview = object_of(&view, "preview");
        assert_eq!(text_of(&preview, "shows"), "progress");
        assert_eq!(text_of(&preview, "formula"), "=Pets@\"last month\"");
        assert_eq!(text_of(&preview, "target"), "=300");
        // And what the panel shows round-trips through the JSON it saves.
        let again = Component::from_json(&text_of(&view, "json")).expect("saved and read");
        assert_eq!(again, bar);
    }

    #[test]
    fn a_column_formula_goes_through_the_columns_text_and_back() {
        let mut pie = component(ComponentType::ChartPie, "spending_by_category");
        pie.set(
            "columns",
            Setting::Columns(vec![kassensturz_report::ColumnSetting {
                name: "Left over".into(),
                members: Vec::new(),
                formula: "Pay - Food".into(),
            }]),
        )
        .unwrap();
        let json = properties::columns_json(&pie, &|stored| format!("<{stored}>")).unwrap();
        assert!(json.contains("<Pay - Food>"), "{json}");
        let unwrap = |typed: &str| {
            typed
                .strip_prefix('<')
                .and_then(|rest| rest.strip_suffix('>'))
                .map(str::to_owned)
        };
        let back = properties::read_columns(&json, &unwrap).unwrap();
        assert_eq!(back[0].formula, "Pay - Food");
        // A text that is no formula refuses the whole list.
        assert!(properties::read_columns(&json, &|_| None).is_none());
        // A column of the older kind has no formula.
        let plain = properties::read_columns(r#"[{"name":"A","members":["x"]}]"#, &|typed| {
            Some(typed.to_owned())
        })
        .unwrap();
        assert_eq!(plain[0].formula, "");
        assert_eq!(plain[0].members, ["x"]);
    }

    #[test]
    fn every_dataset_and_word_the_catalog_offers_has_plain_words_in_the_panel() {
        let panel = include_str!("../qml/PropertiesPanel.qml");
        for dataset in kassensturz_report::CATALOG {
            let case = format!("case \"{}\":", dataset.id);
            assert!(
                panel.matches(&case).count() >= 2,
                "{} needs a name and a help text in the panel",
                dataset.id
            );
            let figures = if dataset.suits.contains(&"kpi") {
                dataset.measures
            } else {
                &[]
            };
            for word in dataset.groupings.iter().chain(figures) {
                assert!(
                    panel.contains(&format!("case \"{word}\":")),
                    "{word} of {} needs words in the panel",
                    dataset.id
                );
            }
        }
    }
}
