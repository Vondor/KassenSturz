// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The one field for anything secret that the user types: a PIN, a TAN, a
// password (D-162). What is typed shows as dots, until the user presses the
// eye button; every time the field is shown again (`hide()`) it is hidden
// again. The keyboard is told not to guess words or remember the text, and
// not to capitalize. It is Kirigami's `PasswordField` made over, because
// that one's eye button is English and a list of actions given to it adds
// to its own instead of replacing it. A rule the tests check: no screen
// types a secret into a plain `TextField`.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.ActionTextField {
    id: field

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Whether the text shows in clear; the eye button turns it on and off.
    property bool showPassword: false

    // Hides the text again, for a dialog that opens with an empty field.
    function hide() {
        showPassword = false;
    }

    // Dots, and Qt then also refuses to copy or cut the text.
    echoMode: field.showPassword ? TextInput.Normal : TextInput.Password
    placeholderText: ""
    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
    rightActions: Kirigami.Action {
        text: field.showPassword
            ? field.i18nContext.i18nc("@action:button hide what was typed in a secret field", "Hide what you typed")
            : field.i18nContext.i18nc("@action:button show what was typed in a secret field", "Show what you typed")
        icon.name: field.showPassword ? "password-show-off" : "password-show-on"
        onTriggered: field.showPassword = !field.showPassword
    }

    // No undo: it would bring back what was typed (QTBUG-103934).
    Keys.onPressed: event => {
        if (event.matches(StandardKey.Undo)) {
            event.accepted = true;
        }
    }
}
