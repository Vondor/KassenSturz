// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain values for QML: text, objects and arrays as `QVariant`s. The lists
//! the screens show hold a few dozen rows, so they go to QML as arrays of
//! objects, which QML views take as models.

use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};

use crate::errors::ErrorText;

/// Text.
pub fn text(value: &str) -> QVariant {
    QVariant::from(&QString::from(value))
}

/// A QML object with these properties.
pub fn map<'a>(entries: impl IntoIterator<Item = (&'a str, QVariant)>) -> QVariant {
    let mut map = QMap::<QMapPair_QString_QVariant>::default();
    for (key, value) in entries {
        map.insert(QString::from(key), value);
    }
    QVariant::from(&map)
}

/// A QML array of these values.
pub fn list(values: impl IntoIterator<Item = QVariant>) -> QVariant {
    let mut list = QList::<QVariant>::default();
    for value in values {
        list.append(value);
    }
    QVariant::from(&list)
}

/// An error as QML gets it: an object with its `code` and its parameters
/// by name.
pub fn error_object(error: &ErrorText) -> QVariant {
    map(std::iter::once(("code", text(error.code))).chain(
        error
            .params
            .iter()
            .map(|(name, value)| (*name, text(value))),
    ))
}

/// Reads an object back the way QML does, by property name and type, for
/// the tests of the objects the bridge builds (AGENTS.md §9.4). A property
/// that is missing, or of another type than QML reads it as, panics with its
/// name: such a field would be invisible to a test of the Rust struct.
#[cfg(test)]
pub(crate) mod read_back {
    use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};

    /// The object's properties.
    pub(crate) fn object(value: &QVariant) -> QMap<QMapPair_QString_QVariant> {
        value
            .value::<QMap<QMapPair_QString_QVariant>>()
            .expect("an object")
    }

    /// The object's names of properties, in Qt's order (by name).
    pub(crate) fn keys(value: &QVariant) -> Vec<String> {
        object(value)
            .iter()
            .map(|(key, _)| key.to_string())
            .collect()
    }

    fn property(value: &QVariant, key: &str) -> QVariant {
        object(value)
            .get(&QString::from(key))
            .unwrap_or_else(|| panic!("no property `{key}`"))
    }

    /// A property that is an object itself.
    pub(crate) fn object_of(value: &QVariant, key: &str) -> QVariant {
        property(value, key)
    }

    /// A text property.
    pub(crate) fn text_of(value: &QVariant, key: &str) -> String {
        property(value, key)
            .value::<QString>()
            .unwrap_or_else(|| panic!("`{key}` is not text"))
            .to_string()
    }

    /// A true-or-false property.
    pub(crate) fn flag_of(value: &QVariant, key: &str) -> bool {
        property(value, key)
            .value::<bool>()
            .unwrap_or_else(|| panic!("`{key}` is not a flag"))
    }

    /// A whole number property.
    pub(crate) fn number_of(value: &QVariant, key: &str) -> i32 {
        property(value, key)
            .value::<i32>()
            .unwrap_or_else(|| panic!("`{key}` is not a number"))
    }

    /// An array property.
    pub(crate) fn list_of(value: &QVariant, key: &str) -> Vec<QVariant> {
        elements(&property(value, key)).unwrap_or_else(|| panic!("`{key}` is not an array"))
    }

    /// The elements of an array, `None` if `value` is not one.
    pub(crate) fn elements(value: &QVariant) -> Option<Vec<QVariant>> {
        value
            .value::<QList<QVariant>>()
            .map(|list| list.iter().cloned().collect())
    }

    /// Whether the property is there but empty, which QML reads as `null`.
    pub(crate) fn is_null(value: &QVariant, key: &str) -> bool {
        property(value, key).is_null()
    }
}
