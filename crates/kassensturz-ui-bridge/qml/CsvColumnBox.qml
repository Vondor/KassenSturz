// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Picks one column of a CSV file on the column screen (M3.10, D-095): by
// its name from the header row, or by its number. `value` is the column as
// the settings keep it (a name or a number as text); a saved column the
// file doesn't have stays choosable, so settings from an older file can
// still be read and corrected.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.ki18n

Controls.ComboBox {
    id: box

    // The file's columns: [{ text, value }].
    property var columns: []
    // Whether the file may lack this column.
    property bool optional: true
    // The column chosen, as text; empty for none.
    property string value: ""

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    textRole: "text"
    valueRole: "value"
    model: {
        let all = optional ? [{ text: i18nContext.i18nc("@item:inlistbox the file has no such column", "Not in the file"), value: "" }] : [];
        all = all.concat(columns);
        if (value !== "" && all.findIndex(column => column.value === value) < 0) {
            all.push({ text: i18nContext.i18nc("@item:inlistbox %1 is the name or number of a column that the file doesn't have", "%1 (not in this file)", value), value: value });
        }
        return all;
    }

    onActivated: value = currentValue
    onModelChanged: sync()
    onValueChanged: sync()
    Component.onCompleted: sync()

    function sync() {
        const index = indexOfValue(value);
        currentIndex = index >= 0 ? index : (optional ? 0 : -1);
    }
}
