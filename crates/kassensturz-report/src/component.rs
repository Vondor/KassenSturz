// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One component of a dashboard or view, checked against the property
//! schema (M6a.6, D-278, D-281).
//!
//! A [`Component`] can only hold values the schema allows: it is made from
//! defaults, changed through [`Component::set`] and read from a document
//! with [`Component::from_value`], and each of these refuses anything else.
//! Everything in a document is data: text stays text and is never run or
//! read as markup.

use serde_json::{Map, Value};

use crate::ReportError;
use crate::catalog::DatasetInfo;
use crate::schema::{ColumnSetting, ComponentType, Kind, Place, Property, Setting, dataset_for};

/// A component with a dataset and one value for every property of its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    kind: ComponentType,
    dataset: &'static DatasetInfo,
    /// One value for each of `kind.properties()`, in the same order.
    settings: Vec<Setting>,
}

/// One column as a document writes it: `{"name": ..., "members": [...]}`.
fn read_column(entry: &Value) -> Option<ColumnSetting> {
    let entry = entry.as_object()?;
    // The formula is written only when there is one (D-341).
    let formula = match entry.get("formula") {
        None => String::new(),
        Some(formula) => formula.as_str()?.to_owned(),
    };
    if entry.len() != 2 + usize::from(entry.contains_key("formula")) {
        return None;
    }
    Some(ColumnSetting {
        formula,
        name: entry.get("name")?.as_str()?.to_owned(),
        members: entry
            .get("members")?
            .as_array()?
            .iter()
            .map(|id| id.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?,
    })
}

fn invalid(reason: &'static str, path: &str) -> ReportError {
    ReportError::DocumentInvalid {
        reason,
        path: path.to_owned(),
    }
}

impl Component {
    /// A new component with every property at its default.
    pub fn new(kind: ComponentType, dataset_id: &str) -> Result<Self, ReportError> {
        let dataset = dataset_for(kind, dataset_id)?;
        let settings = kind
            .properties()
            .iter()
            .map(|property| property.default_setting(dataset))
            .collect();
        Ok(Self {
            kind,
            dataset,
            settings,
        })
    }

    /// The type of component.
    pub const fn kind(&self) -> ComponentType {
        self.kind
    }

    /// The dataset it shows.
    pub const fn dataset(&self) -> &'static DatasetInfo {
        self.dataset
    }

    /// Each property with its value, in the order the panel lists them.
    pub fn settings(&self) -> impl Iterator<Item = (&'static Property, &Setting)> {
        self.kind.properties().iter().zip(&self.settings)
    }

    /// The value of the property with this key.
    pub fn get(&self, key: &str) -> Option<&Setting> {
        self.settings()
            .find(|(property, _)| property.key == key)
            .map(|(_, setting)| setting)
    }

    /// Changes one property; nothing changes if the value isn't allowed.
    pub fn set(&mut self, key: &str, setting: Setting) -> Result<(), ReportError> {
        let index = self
            .kind
            .properties()
            .iter()
            .position(|property| property.key == key)
            .ok_or_else(|| invalid("unknown-property", key))?;
        self.kind.properties()[index].check(self.dataset, &setting)?;
        if self.kind == ComponentType::CustomChart
            && matches!(key, "series" | "categories")
            && matches!(&setting, Setting::Ids(ids) if !ids.is_empty())
            && !self.takes_choice(key)
        {
            // The choice belongs to a dataset the description doesn't name.
            return Err(invalid("bad-choice", &format!("params.{key}")));
        }
        let changed = self.settings[index] != setting;
        self.settings[index] = setting;
        if self.kind == ComponentType::CustomChart && key == "spec" {
            // A new description starts drawing only what it fully says: the
            // choice to leave parts out was made for the old one (D-508).
            if changed && let Some(at) = self.position("partial") {
                self.settings[at] = Setting::Flag(false);
            }
            self.follow_spec();
        }
        Ok(())
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.kind
            .properties()
            .iter()
            .position(|property| property.key == key)
    }

    /// Whether a custom chart's `series` or `categories` means something for
    /// the dataset it shows: the series of `category_over_time` and the
    /// categories of `budget_utilization` (Q-353).
    fn takes_choice(&self, key: &str) -> bool {
        match key {
            "series" => self.dataset.id == crate::CATEGORY_OVER_TIME,
            "categories" => self.dataset.id == crate::BUDGET_UTILIZATION,
            _ => true,
        }
    }

    /// Makes a custom chart show the dataset its description names, and drops
    /// the choices the new dataset doesn't take, in the same edit (D-504).
    /// A description that names no dataset (it was refused or has only
    /// `data.url`) leaves the dataset as it was.
    fn follow_spec(&mut self) {
        if let Some(Setting::Text(text)) = self.get("spec")
            && let Ok(read) = crate::spec::read(text)
            && let Some(chart) = read.chart()
            && let Some(dataset) = crate::dataset(chart.dataset().id())
        {
            self.dataset = dataset;
        }
        self.drop_unused_choices();
    }

    /// Empties the `series` and `categories` of a custom chart whose dataset
    /// doesn't take them, so nothing unused is stored.
    fn drop_unused_choices(&mut self) {
        if self.kind != ComponentType::CustomChart {
            return;
        }
        for key in ["series", "categories"] {
            if self.takes_choice(key) {
                continue;
            }
            if let Some(index) = self.position(key) {
                self.settings[index] = Setting::Ids(Vec::new());
            }
        }
    }

    /// Shows another dataset. The properties that depend on the dataset go
    /// back to their defaults when their value isn't allowed for the new
    /// one (a grouping the new dataset doesn't have); the others stay.
    pub fn use_dataset(&mut self, dataset_id: &str) -> Result<(), ReportError> {
        let dataset = dataset_for(self.kind, dataset_id)?;
        // A custom chart shows the dataset its description names (D-504):
        // another one comes only by changing the description.
        if self.kind == ComponentType::CustomChart && dataset.id != self.dataset.id {
            return Err(invalid("dataset-not-suited", "dataset"));
        }
        for (property, setting) in self.kind.properties().iter().zip(&mut self.settings) {
            if property.check(dataset, setting).is_err() {
                *setting = property.default_setting(dataset);
            }
        }
        self.dataset = dataset;
        Ok(())
    }

    /// Makes this another type of component. It keeps its dataset if the new
    /// type can show it and otherwise takes the first dataset the new type
    /// can show. A setting the new type has too, with a value it allows
    /// there, stays (the title, for one); the rest take their defaults.
    pub fn change_kind(&mut self, kind: ComponentType) -> Result<(), ReportError> {
        let dataset = if kind.datasets().any(|d| d.id == self.dataset.id) {
            self.dataset
        } else {
            kind.datasets()
                .next()
                .ok_or_else(|| invalid("dataset-not-suited", "dataset"))?
        };
        let settings = kind
            .properties()
            .iter()
            .map(|property| {
                self.settings()
                    .find(|(old, _)| old.key == property.key && old.place == property.place)
                    .map(|(_, setting)| setting)
                    .filter(|setting| property.check(dataset, setting).is_ok())
                    .cloned()
                    .unwrap_or_else(|| property.default_setting(dataset))
            })
            .collect();
        self.kind = kind;
        self.dataset = dataset;
        self.settings = settings;
        self.drop_unused_choices();
        Ok(())
    }

    /// Reads a component from JSON text, as [`Component::from_value`]; text
    /// that isn't JSON is refused as a value of the wrong type.
    pub fn from_json(json: &str) -> Result<Self, ReportError> {
        let value = serde_json::from_str(json).map_err(|_| invalid("wrong-type", "component"))?;
        Self::from_value(&value)
    }

    /// Reads a component of a document: an object with `type`, `dataset`,
    /// and optionally `params` and `props`, whose keys must all be in the
    /// schema. A missing property takes its default. `id` and `frame` belong
    /// to the dashboard and are left to the code that reads it.
    pub fn from_value(value: &Value) -> Result<Self, ReportError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("wrong-type", "component"))?;
        if let Some(key) = object.keys().find(|key| {
            !matches!(
                key.as_str(),
                "id" | "type" | "dataset" | "params" | "props" | "frame"
            )
        }) {
            return Err(invalid("unknown-property", key));
        }
        let text = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("wrong-type", key))
        };
        let kind =
            ComponentType::from_id(text("type")?).ok_or_else(|| invalid("unknown-type", "type"))?;
        // A heading shows no data: its document names no dataset (D-312).
        let dataset = if kind.shows_no_data() {
            if object.contains_key("dataset") {
                return Err(invalid("unknown-property", "dataset"));
            }
            ""
        } else {
            text("dataset")?
        };
        let mut component = Self::new(kind, dataset)?;
        for place in [Place::Params, Place::Props] {
            let Some(group) = object.get(place.key()) else {
                continue;
            };
            let group = group
                .as_object()
                .ok_or_else(|| invalid("wrong-type", place.key()))?;
            for (key, value) in group {
                let path = format!("{}.{key}", place.key());
                let index = kind
                    .properties()
                    .iter()
                    .position(|property| property.place == place && property.key == key)
                    .ok_or_else(|| invalid("unknown-property", &path))?;
                let property = &kind.properties()[index];
                let setting = match (property.kind, value) {
                    (Kind::Text | Kind::Formula | Kind::Spec, Value::String(text)) => {
                        Setting::Text(text.clone())
                    }
                    (Kind::Flag { .. }, Value::Bool(flag)) => Setting::Flag(*flag),
                    (Kind::Ids | Kind::Accounts | Kind::Budgets, Value::Array(list)) => {
                        Setting::Ids(
                            list.iter()
                                .map(|id| id.as_str().map(str::to_owned))
                                .collect::<Option<Vec<_>>>()
                                .ok_or_else(|| invalid("wrong-type", &path))?,
                        )
                    }
                    (Kind::Columns, Value::Array(list)) => Setting::Columns(
                        list.iter()
                            .map(|entry| {
                                read_column(entry).ok_or_else(|| invalid("wrong-type", &path))
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    (Kind::Choice(_) | Kind::Grouping, Value::String(word)) => {
                        Setting::Word(word.clone())
                    }
                    _ => return Err(invalid("wrong-type", &path)),
                };
                property.check(component.dataset, &setting)?;
                component.settings[index] = setting;
            }
        }
        if kind == ComponentType::CustomChart {
            component.check_custom()?;
        }
        Ok(component)
    }

    /// What a document must also keep for a custom chart: its `dataset` is
    /// the one the description names (`spec-dataset`), and `series` and
    /// `categories` are there only for the dataset that takes them (D-504).
    fn check_custom(&self) -> Result<(), ReportError> {
        if let Some(Setting::Text(text)) = self.get("spec")
            && let Ok(read) = crate::spec::read(text)
            && let Some(chart) = read.chart()
            && chart.dataset().id() != self.dataset.id
        {
            return Err(invalid("spec-dataset", "dataset"));
        }
        for key in ["series", "categories"] {
            if !self.takes_choice(key)
                && matches!(self.get(key), Some(Setting::Ids(ids)) if !ids.is_empty())
            {
                return Err(invalid("unknown-property", &format!("params.{key}")));
            }
        }
        Ok(())
    }

    /// The component as a document writes it: `type`, `dataset`, `params`
    /// and `props` with every property. A custom chart writes the `series` and
    /// `categories` of its dataset only (D-504); the series' own filters are
    /// written only when they have a list (D-646).
    pub fn to_value(&self) -> Value {
        let mut params = Map::new();
        let mut props = Map::new();
        for (property, setting) in self.settings() {
            let value = match setting {
                Setting::Text(text) | Setting::Word(text) => Value::String(text.clone()),
                Setting::Flag(flag) => Value::Bool(*flag),
                Setting::Ids(ids) => Value::Array(ids.iter().cloned().map(Value::String).collect()),
                Setting::Columns(columns) => Value::Array(
                    columns
                        .iter()
                        .map(|column| {
                            let mut entry = Map::new();
                            entry.insert("name".into(), column.name.clone().into());
                            entry.insert(
                                "members".into(),
                                Value::Array(
                                    column.members.iter().cloned().map(Value::String).collect(),
                                ),
                            );
                            if !column.formula.is_empty() {
                                entry.insert("formula".into(), column.formula.clone().into());
                            }
                            Value::Object(entry)
                        })
                        .collect(),
                ),
            };
            if self.kind == ComponentType::CustomChart
                && property.place == Place::Params
                && !self.takes_choice(property.key)
            {
                continue;
            }
            // A series' own filter is written only when it has a list
            // (D-646), so documents without one read as they always did.
            if crate::is_series_filter(property.key)
                && matches!(setting, Setting::Ids(ids) if ids.is_empty())
            {
                continue;
            }
            // The total is the default and is not written (D-648).
            if property.key == crate::schema::AVERAGE_KEY
                && matches!(setting, Setting::Word(word) if word == "total")
            {
                continue;
            }
            match property.place {
                Place::Params => params.insert(property.key.to_owned(), value),
                Place::Props => props.insert(property.key.to_owned(), value),
            };
        }
        let mut object = Map::new();
        object.insert("type".into(), self.kind.id().into());
        if !self.kind.shows_no_data() {
            object.insert("dataset".into(), self.dataset.id.into());
            object.insert("params".into(), Value::Object(params));
        }
        object.insert("props".into(), Value::Object(props));
        Value::Object(object)
    }
}
