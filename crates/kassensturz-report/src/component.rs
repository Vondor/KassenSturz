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
use crate::schema::{ComponentType, Kind, Place, Property, Setting, dataset_for};

/// A component with a dataset and one value for every property of its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    kind: ComponentType,
    dataset: &'static DatasetInfo,
    /// One value for each of `kind.properties()`, in the same order.
    settings: Vec<Setting>,
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
        self.settings[index] = setting;
        Ok(())
    }

    /// Shows another dataset. The properties that depend on the dataset go
    /// back to their defaults when their value isn't allowed for the new
    /// one (a grouping the new dataset doesn't have); the others stay.
    pub fn use_dataset(&mut self, dataset_id: &str) -> Result<(), ReportError> {
        let dataset = dataset_for(self.kind, dataset_id)?;
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
        let mut component = Self::new(kind, text("dataset")?)?;
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
                    (Kind::Text, Value::String(text)) => Setting::Text(text.clone()),
                    (Kind::Flag { .. }, Value::Bool(flag)) => Setting::Flag(*flag),
                    (Kind::Choice(_) | Kind::Grouping, Value::String(word)) => {
                        Setting::Word(word.clone())
                    }
                    _ => return Err(invalid("wrong-type", &path)),
                };
                property.check(component.dataset, &setting)?;
                component.settings[index] = setting;
            }
        }
        Ok(component)
    }

    /// The component as a document writes it: `type`, `dataset`, `params`
    /// and `props` with every property.
    pub fn to_value(&self) -> Value {
        let mut params = Map::new();
        let mut props = Map::new();
        for (property, setting) in self.settings() {
            let value = match setting {
                Setting::Text(text) | Setting::Word(text) => Value::String(text.clone()),
                Setting::Flag(flag) => Value::Bool(*flag),
            };
            match property.place {
                Place::Params => params.insert(property.key.to_owned(), value),
                Place::Props => props.insert(property.key.to_owned(), value),
            };
        }
        let mut object = Map::new();
        object.insert("type".into(), self.kind.id().into());
        object.insert("dataset".into(), self.dataset.id.into());
        object.insert("params".into(), Value::Object(params));
        object.insert("props".into(), Value::Object(props));
        Value::Object(object)
    }
}
