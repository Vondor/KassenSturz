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
